use std::{
    collections::HashMap,
    io::{stderr, stdout},
    os::fd::{AsRawFd, IntoRawFd},
    path::Path,
    time::{Duration, SystemTime},
};

use api::{
    JobTerminationStatus, OrderExecutionMetrics, OrderExecutionMetricsCosts, TerminationCause,
};
use nix::{
    sys::{
        time::TimeSpec,
        wait::{WaitPidFlag, WaitStatus},
    },
    unistd::Pid,
};
use sqlx::SqlitePool;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::{manager::ManagerRequest, pricing::get_current_pricing};

/// This represents the status of a running job.
#[derive(Debug)]
pub enum JobStatus {
    /// The job is preparing to start executing the makefile.
    Preparing,

    /// The job is currently executing the makefile, with the following metrics.
    Executing(OrderExecutionMetrics),

    /// The job is now terminated
    Terminated(JobTerminationStatus),
}

/// This allows communicating with a job that's currently running.
#[derive(Debug)]
pub struct RunningJobHandle {
    /// Receive the current job status
    pub status: watch::Receiver<JobStatus>,

    /// Cancel this to command the job to terminate as soon as possible.
    pub stop: CancellationToken,

    /// This channel will never be written to.
    /// If this channel is closed, then the job has terminated.
    pub job_termination: broadcast::Receiver<()>,
}

impl Clone for RunningJobHandle {
    fn clone(&self) -> Self {
        Self {
            status: self.status.clone(),
            stop: self.stop.clone(),
            job_termination: self.job_termination.resubscribe(),
        }
    }
}

/// This returns Ok if the job terminates after writing down its status in the database;
/// Err or panic otherwise.
#[tracing::instrument(level = "info")]
pub async fn run_order_work(
    order_id: i64,
    db: SqlitePool,
    sender: mpsc::Sender<ManagerRequest>,
    (uploaded_files, uploaded_mb): (usize, f64),
) -> anyhow::Result<()> {
    let (mut status_send, status_recv) = watch::channel(JobStatus::Preparing);
    let mut cancel = CancellationToken::new();

    // We'll drop this on the way out of the function (including on panics)
    let (_term_send, term_recv) = broadcast::channel(1);

    // We'll write to here if we manage to write everything to the database.

    let handle = RunningJobHandle {
        status: status_recv,
        stop: cancel.clone(),
        job_termination: term_recv,
    };

    // The first thing to do is to announce ourselves.
    sender
        .send(ManagerRequest::BeginWork { order_id, handle })
        .await?;

    let data = match sqlx::query!("SELECT accounts.* FROM accounts INNER JOIN orders ON orders.user_id=accounts.id WHERE orders.id=?", order_id)
        .fetch_optional(&db)
        .await?
    {
        Some(v) => v,
        None => {
            let term = JobTerminationStatus::AbnormalTermination(format!("When job was preparing, could not find account associated with order {order_id}"));
            let status_json = serde_json::to_string(&term).unwrap();
            status_send.send_replace(JobStatus::Terminated(term));
            sqlx::query!("UPDATE orders SET is_running=0, status_json=? WHERE id=?", status_json, order_id).execute(&db).await?;
    return Ok(());
        },
    };
    let original_balance = data.balance;

    let pre_metrics = OrderExecutionMetrics {
        uploaded_mb,
        uploaded_files,
        ..Default::default()
    };

    tracing::warn!("Entering danger section");
    let termination = fork_and_make(
        order_id,
        pre_metrics,
        &mut status_send,
        &mut cancel,
        original_balance,
    )
    .await?;
    tracing::warn!("Exiting danger section");

    status_send.send_replace(JobStatus::Terminated(termination));

    tokio::time::sleep(Duration::from_secs(10)).await;

    sender
        .send(ManagerRequest::FinishWork { order_id })
        .await
        .unwrap();

    Ok(())
}

async fn fork_and_make(
    order_id: i64,
    mut metrics: OrderExecutionMetrics,
    status: &mut watch::Sender<JobStatus>,
    cancel: &mut CancellationToken,
    user_balance_at_start: f64,
) -> anyhow::Result<JobTerminationStatus> {
    Ok(match unsafe { nix::unistd::fork() } {
        Ok(nix::unistd::ForkResult::Child) => {
            // We're the child process: chdir to the order's directory
            std::env::set_current_dir(&Path::new(&format!("/compile/{order_id}")))
                .expect("Child failed to chdir into the order directory");

            // Become the process group leader.
            nix::unistd::setsid().expect("Child failed to become leader of process group");

            // Set up a stdout and stderr redirection.
            let stdout_file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open("make-stdout.txt")
                .expect("Failed to open stdout file");
            let stderr_file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open("make-stderr.txt")
                .expect("Failed to open stdout file");
            let real_stdout = stdout().as_raw_fd();
            let real_stderr = stderr().as_raw_fd();
            nix::unistd::dup2(stdout_file.into_raw_fd(), real_stdout)
                .expect("Failed to reassign stdout in child");
            nix::unistd::dup2(stderr_file.into_raw_fd(), real_stderr)
                .expect("Failed to reassign stderr in child");

            println!(
                "Started make process at {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
            );
            eprintln!(
                "Started make process at {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
            );
            println!("-----");
            eprintln!("-----");

            // Finally, exec the target program.
            // This only returns in the case that we couldn't spawn it.
            let why = exec::Command::new("make").exec();
            println!("!! Failed to spawn the make program: {why}");
            eprintln!("!! Failed to spawn the make program: {why}");
            unsafe {
                libc::_exit(255);
            }
        }

        Ok(nix::unistd::ForkResult::Parent { child, .. }) => {
            // println!(
            //     "Continuing execution in parent process, new child has pid: {}",
            //     child
            // );
            let spawned_at = std::time::SystemTime::now();
            let pricing = get_current_pricing();

            // Loop, periodically waiting for the child.
            // Collect the process times each cycle.
            let mut child_exit_status = None;
            let mut child_should_die = false;
            let mut cpu_times: HashMap<Pid, TimeSpec> = HashMap::new();
            let mut termination_cause = TerminationCause::NaturalTermination;
            let mut overdraft_started_at = None;
            while child_exit_status.is_none() {
                if let Err(why) = collect_cpu_time(child, &mut cpu_times).await {
                    tracing::error!("Error while process time accounting: {why}");
                }

                let mut total_cpu_time = TimeSpec::new(0, 0);
                let mut total_forked_processes = 0;
                for value in cpu_times.iter() {
                    total_cpu_time = total_cpu_time + *value.1;
                    total_forked_processes += 1;
                }

                metrics.processes_forked = total_forked_processes;
                metrics.cpu_seconds = Duration::from(total_cpu_time).as_secs_f64();
                metrics.wall_seconds = spawned_at.elapsed().unwrap().as_secs_f64();

                status.send_replace(JobStatus::Executing(metrics));

                let total_cost = metrics.calculate_costs(&pricing).grand_total();
                if user_balance_at_start - total_cost < 0.0 {
                    if overdraft_started_at.is_none() {
                        overdraft_started_at = Some(SystemTime::now());
                    }
                }

                if let Some(start_time) = overdraft_started_at {
                    let elapsed = start_time.elapsed().unwrap().as_secs_f64();
                    let remaining = pricing.overdraft_seconds_allowed - elapsed;
                    metrics.time_until_overdraft_stop = Some(remaining);
                    if remaining < 0.0 {
                        child_should_die = true;
                        termination_cause = TerminationCause::BalanceKill;
                    }
                }

                if cancel.is_cancelled() {
                    child_should_die = true;
                    termination_cause = TerminationCause::UserKill;
                }

                if child_should_die {
                    if let Err(why) =
                        nix::sys::signal::killpg(child, Some(nix::sys::signal::Signal::SIGKILL))
                    {
                        tracing::error!("Error killing child: {why}");
                    }
                }

                tokio::time::sleep(Duration::from_millis(50)).await;

                let res = nix::sys::wait::waitid(
                    nix::sys::wait::Id::Pid(child),
                    WaitPidFlag::WNOHANG | WaitPidFlag::WEXITED,
                );

                if let Ok(res) = res {
                    if let WaitStatus::Exited(_pid, status) = res {
                        println!("Child exited with status: {status}");
                        child_exit_status = Some(status);
                    }
                } else {
                    return Err(anyhow::anyhow!("Failed to wait for completion of child"));
                }
            }

            JobTerminationStatus::ProcessExit {
                exit_code: child_exit_status.unwrap(),
                cause: termination_cause,
                metrics,
                costs: metrics.calculate_costs(&pricing),
            }
        }

        Err(_) => Err(anyhow::anyhow!("Fork failed"))?,
    })
}

async fn collect_cpu_time(
    which: Pid,
    cpu_times: &mut HashMap<Pid, TimeSpec>,
) -> anyhow::Result<()> {
    // First, detect all the children of the process.
    #[async_recursion::async_recursion]
    async fn collect_children(of: Pid, to: &mut Vec<Pid>) -> anyhow::Result<()> {
        to.push(of);
        let tasks = tokio::fs::read_dir(format!("/proc/{of}/task")).await;
        let mut tasks = if let Ok(t) = tasks {
            t
        } else {
            return Ok(()); // If there was an error reading the task list, then the process probably died already, so we ignore it.
        };
        while let Some(item) = tasks.next_entry().await? {
            let task = item.file_name();
            let task = task.to_string_lossy();
            if let Ok(children) =
                tokio::fs::read_to_string(format!("/proc/{of}/task/{task}/children")).await
            {
                for child in children.split_ascii_whitespace() {
                    collect_children(
                        Pid::from_raw(child.parse().expect("Process ID is not an integer?")),
                        to,
                    )
                    .await?;
                }
            } else {
                // If failed reading children, then the process probably died already, so we ignore it.
                return Ok(());
            }
        }
        Ok(())
    }

    let mut pids = vec![];
    collect_children(which, &mut pids).await?;

    // Then, for each one, get its CPU clock, then query its value.
    for pid in pids {
        if let Ok(clock_id) = nix::time::clock_getcpuclockid(pid) {
            if let Ok(time) = nix::time::clock_gettime(clock_id) {
                cpu_times.insert(pid, time);
            } else {
                // The process probably died already.
                continue;
            }
        } else {
            // The process probably died already.
            continue;
        }
    }

    Ok(())
}
