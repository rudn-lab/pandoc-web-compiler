use api::{LiveStatus, OrderFileList, OrderInfoFull, OrderInfoResult};
use gloo::{storage::Storage, utils::document};
use shadow_clone::shadow_clone;
use yew::{prelude::*, suspense::use_future};
use yew_autoprops::autoprops;
use yew_bootstrap::component::Spinner;
use yew_hooks::use_websocket;
use yew_router::hooks::use_navigator;

use crate::{Route, MONEY};

#[autoprops]
#[function_component(Order)]
pub fn order(id: i64) -> Html {
    let fallback = html! {
        <h1>{"Загружаем состояние заказа..."}<Spinner/></h1>
    };
    html!(
        <Suspense {fallback}>
            <OrderInner {id} />
        </Suspense>
    )
}

#[autoprops]
#[function_component(OrderInner)]
pub fn order_inner(id: i64) -> HtmlResult {
    let navigator = use_navigator().unwrap();

    let resp = {
        shadow_clone!(navigator);
        use_future(|| async move {
            let profile_key = gloo::storage::LocalStorage::get("token");
            let profile_key: Option<String> = match profile_key {
                Ok(key) => key,
                Err(_) => None,
            };
            let token = if let Some(key) = profile_key {
                key
            } else {
                navigator.push(&Route::Profile);
                String::new()
            };

            let order_info =
                reqwest::get(format!("https://pandoc.danya02.ru/api/orders/{token}/{id}"))
                    .await?
                    .error_for_status()?
                    .json::<OrderInfoResult>()
                    .await?;

            Ok::<_, anyhow::Error>(order_info)
        })?
    };

    match *resp {
        Ok(ref res) => match res {
            OrderInfoResult::NotAccessible => Ok(
                html!(<div class="alert alert-warning">{"Такой заказ не существует или недоступен."}</div>),
            ),
            OrderInfoResult::Running => Ok(html!(
                    <OrderInnerLive {id} />
            )),
            OrderInfoResult::Completed(info) => {
                Ok(html!(<DisplayCompletedOrder {id} info={info.clone()}/>))
            }
        },
        Err(ref failure) => Ok(
            html!(<div class="alert alert-danger">{"Ошибка при загрузке профиля: "}{failure.to_string()}</div>),
        ),
    }
}

#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    fn format_unix_time(time: f64) -> String;
}

#[autoprops]
#[function_component(DisplayCompletedOrder)]
fn display_completed_order(id: i64, info: &OrderInfoFull) -> Html {
    let files = if !info.is_on_disk {
        html!(<div class="alert alert-warning">{"Файлы этого заказа были удалены, потому что он был выполнен слишком давно."}</div>)
    } else {
        let fallback = html!(<p>{"Загружаем список файлов..."} <Spinner small={true} /></p>);
        html!(
            <Suspense {fallback}>
                <h3>{"Файлы в рабочей директории"}</h3>
                <OrderFiles {id} />
            </Suspense>
        )
    };

    let cost_breakdown = match info.record.termination {
        api::JobTerminationStatus::AbnormalTermination(ref why) => {
            html!(<><p>{"Неожиданный результат: "}{why}</p></>)
        }
        api::JobTerminationStatus::VeryAbnormalTermination(ref why) => {
            html!(<><p>{"Совсем неожиданный результат: "}{why}</p></>)
        }
        api::JobTerminationStatus::ProcessExit {
            exit_code,
            ref cause,
            ref metrics,
            ref costs,
        } => {
            let priced = costs;
            let cause = match cause {
                api::TerminationCause::NaturalTermination => "процесс завершился самостоятельно",
                api::TerminationCause::UserKill => "остановка пользователем",
                api::TerminationCause::BalanceKill => "остановка по недостатку баланса",
            };
            html!(
                <>
                <p>{"Процесс завершился с кодом: "}{exit_code}</p>
                <p>{"Причина завершения: "}{cause}</p>
                <p>{"Секунд процессора: "}<code>{format!("{:.5}", metrics.cpu_seconds)}</code>{"="}<code>{format!("{:.5}", priced.cpu_time)}{MONEY}</code></p>
                <p>{"Секунд реального времени: "}<code>{format!("{:.5}", metrics.wall_seconds)}</code>{"="}<code>{format!("{:.5}", priced.wall_time)}{MONEY}</code></p>
                <p>{"Процессов запущенно: "}<code>{format!("{:.5}", metrics.processes_forked)}</code>{"="}<code>{format!("{:.5}", priced.processes)}{MONEY}</code></p>
                <p>{"МБ загружено: "}<code>{format!("{:.5}", metrics.uploaded_mb)}</code>{"="}<code>{format!("{:.5}", priced.upload_mb)}{MONEY}</code></p>
                <p>{"Файлов загружено: "}<code>{format!("{:.5}", metrics.uploaded_files)}</code>{"="}<code>{format!("{:.5}", priced.upload_files)}{MONEY}</code></p>
                </>
            )
        }
    };

    html!(
        <>
            <h1>{"Заказ "}{id}</h1>
            <p>{"Создан: "}{format_unix_time(info.created_at_unix_time as f64)}</p>
                <details>
                    <summary>{"Стоимость: "}<code>{format!("{:.3}{MONEY}", info.record.order_cost)}</code></summary>

                    {cost_breakdown}

                </details>
            <p>{"Результат: "}{format!("{:?}", info.record.termination)}</p>
            <hr/>
            {files}
        </>
    )
}

#[autoprops]
#[function_component(OrderFiles)]
fn order_files(id: i64) -> HtmlResult {
    let navigator = use_navigator().unwrap();
    let resp = {
        shadow_clone!(navigator);
        use_future(|| async move {
            let profile_key = gloo::storage::LocalStorage::get("token");
            let profile_key: Option<String> = match profile_key {
                Ok(key) => key,
                Err(_) => None,
            };
            let token = if let Some(key) = profile_key {
                key
            } else {
                navigator.push(&Route::Profile);
                String::new()
            };

            let order_info = reqwest::get(format!(
                "https://pandoc.danya02.ru/api/orders/{token}/{id}/files"
            ))
            .await?
            .error_for_status()?
            .json::<OrderFileList>()
            .await?;

            Ok::<_, anyhow::Error>(order_info)
        })?
    };

    Ok(match &*resp {
        Ok(files) => {
            let rows = files
                .0
                .iter()
                .map(|v| {
                    html!(
                        <tr>
                            <td>{&v.path}</td>
                            <td>{size_format::SizeFormatterBinary::new(v.size_bytes)}{"Б"}</td>
                        </tr>
                    )
                })
                .collect::<Html>();
            html!(
                <table class="table">
                    <thead>
                        <tr>
                            <th>{"Путь к файлу"}</th>
                            <th>{"Размер"}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {rows}
                    </tbody>
                </table>
            )
        }
        Err(ref why) => {
            html!(<div class="alert alert-danger">{"Не получилось загрузить список файлов: "}{why}</div>)
        }
    })
}

#[autoprops]
#[function_component(OrderInnerLive)]
fn order_inner_live(id: i64) -> Html {
    let navigator = use_navigator().unwrap();
    let last_data = use_state_eq(|| None);
    let did_open = use_state_eq(|| false);

    let token = gloo::storage::LocalStorage::get("token");
    let token: Option<String> = match token {
        Ok(token) => token,
        Err(_) => None,
    };

    let token = if let Some(token) = token {
        token
    } else {
        navigator.push(&Route::Profile);
        String::new()
    };

    let ws = use_websocket(format!(
        "wss://pandoc.danya02.ru/api/orders/{token}/{id}/ws"
    ));

    match *ws.ready_state {
        yew_hooks::UseWebSocketReadyState::Connecting => {
            html!(<h1>{"Подключаемся к заказу..."}<Spinner/></h1>)
        }
        yew_hooks::UseWebSocketReadyState::Closing => {
            html!(<h1>{"Заказ почти готов..."}<Spinner/></h1>)
        }
        yew_hooks::UseWebSocketReadyState::Closed => {
            if *did_open {
                document().location().unwrap().reload().unwrap();
            }
            html!(<h1>{"Заказ почти готов..."}<Spinner/></h1>)
        }
        yew_hooks::UseWebSocketReadyState::Open => {
            did_open.set(true);
            if let Some(ref msg) = &*ws.message {
                // Try parsing the data
                match serde_json::from_str::<LiveStatus>(msg) {
                    Err(why) => log::error!("Server sent wrong LiveStatus: {why}"),
                    Ok(v) => {
                        last_data.set(Some(v));
                    }
                }
            }
            let display = match *last_data {
                Some(ref data) => html!(<OrderLiveStatus status={data.clone()} />),
                None => html!(<h1>{"Ждем информации..."}<Spinner/></h1>),
            };

            let do_stop = {
                shadow_clone!(ws);
                Callback::from(move |ev: MouseEvent| {
                    ev.prevent_default();
                    ws.send(String::from("STOP"));
                })
            };

            html!(<>
                {display}
                <hr/>
                <button class="btn btn-outline-danger" onclick={do_stop}>{"Остановить выполнение"}</button>
            </>)
        }
    }
}

#[autoprops]
#[function_component(OrderLiveStatus)]
fn order_live_status(status: &LiveStatus) -> Html {
    match status.status {
        api::JobStatus::Preparing => html!(<h1>{"Заказ скоро запустится..."}<Spinner/></h1>),
        api::JobStatus::Executing(metrics) => {
            let priced = metrics.calculate_costs(&status.pricing);
            html!(<>
                <p>{"Секунд процессора: "}<code>{format!("{:.5}", metrics.cpu_seconds)}</code>{"="}<code>{format!("{:.5}", priced.cpu_time)}{MONEY}</code></p>
                <p>{"Секунд реального времени: "}<code>{format!("{:.5}", metrics.wall_seconds)}</code>{"="}<code>{format!("{:.5}", priced.wall_time)}{MONEY}</code></p>
                <p>{"Процессов запущенно: "}<code>{format!("{:.5}", metrics.processes_forked)}</code>{"="}<code>{format!("{:.5}", priced.processes)}{MONEY}</code></p>
                <p>{"МБ загружено: "}<code>{format!("{:.5}", metrics.uploaded_mb)}</code>{"="}<code>{format!("{:.5}", priced.upload_mb)}{MONEY}</code></p>
                <p>{"Файлов загружено: "}<code>{format!("{:.5}", metrics.uploaded_files)}</code>{"="}<code>{format!("{:.5}", priced.upload_files)}{MONEY}</code></p>
                <p class="fs-5">{"Всего: "}<code>{format!("{:.5}", priced.grand_total())}{MONEY}</code></p>
                </>)
        }
        api::JobStatus::Terminated(_) => html!(<h1>{"Заказ скоро завершится..."}<Spinner/></h1>),
    }
}
