use std::{
    collections::HashMap,
    io::{stderr, stdout},
    os::fd::{AsRawFd, IntoRawFd},
    path::Path,
    time::Duration,
};

use nix::{
    sys::{
        time::TimeSpec,
        wait::{WaitPidFlag, WaitStatus},
    },
    unistd::Pid,
};
use sqlx::SqlitePool;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::manager::ManagerRequest;

/// This represents the status of a running job.
#[derive(Debug)]
pub enum WorkStatus {
    /// The job is copying files into the work dir.
    CopyingFiles,
}

#[tracing::instrument(level = "info")]
pub async fn run_order_work(
    order_id: i64,
    db: SqlitePool,
    sender: mpsc::Sender<ManagerRequest>,
) -> anyhow::Result<()> {
    let (status_send, status_recv) = watch::channel(WorkStatus::CopyingFiles);
    let cancel = CancellationToken::new();

    // We'll send a value here if we finish successfully, and drop it on the way out of the function otherwise.
    let (exit_send, exit_recv) = oneshot::channel();

    // The first thing to do is to announce ourselves.
    sender
        .send(ManagerRequest::BeginWork {
            order_id,
            status: status_recv,
            cancel: cancel.clone(),
            exit: exit_recv,
        })
        .await?;

    tracing::warn!("Entering danger section");
    fork_and_make(order_id).await?;
    tracing::warn!("Exiting danger section");

    tokio::time::sleep(Duration::from_secs(10)).await;

    sender
        .send(ManagerRequest::FinishWork { order_id })
        .await
        .unwrap();
    exit_send.send(()).unwrap();
    Ok(())
}

async fn fork_and_make(order_id: i64) -> anyhow::Result<()> {
    match unsafe { nix::unistd::fork() } {
        Ok(nix::unistd::ForkResult::Child) => {
            // We're the child process: chdir to the order's directory
            std::env::set_current_dir(&Path::new(&format!("/compile/{order_id}")))
                .expect("Child failed to chdir into the order directory");

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
            println!(
                "Continuing execution in parent process, new child has pid: {}",
                child
            );

            // Loop, periodically waiting for the child.
            // Collect the process times each cycle.
            let mut child_is_alive = true;
            let mut cpu_times: HashMap<Pid, TimeSpec> = HashMap::new();
            while child_is_alive {
                let res = nix::sys::wait::waitid(
                    nix::sys::wait::Id::Pid(child),
                    WaitPidFlag::WNOHANG | WaitPidFlag::WEXITED,
                )
                .expect("Failed to wait for child");
                if let WaitStatus::Exited(pid, status) = res {
                    println!("Child exited with status: {status}");
                    child_is_alive = false;
                }

                collect_cpu_time(child, &mut cpu_times)
                    .await
                    .expect("Failed to collect cpu time for process");

                println!("{cpu_times:#?}");
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(())
        }

        Err(_) => Err(anyhow::anyhow!("Fork failed")),
    }
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
