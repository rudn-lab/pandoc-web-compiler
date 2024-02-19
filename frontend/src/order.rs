use api::{LiveStatus, OrderInfoResult};
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
            OrderInfoResult::Completed { info, is_on_disk } => {
                Ok(html!(<>{"Completed: "}{format!("{info:?} {is_on_disk}")}</>))
            }
        },
        Err(ref failure) => Ok(
            html!(<div class="alert alert-danger">{"Ошибка при загрузке профиля: "}{failure.to_string()}</div>),
        ),
    }
}

#[autoprops]
#[function_component(OrderInnerLive)]
pub fn order_inner_live(id: i64) -> Html {
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
