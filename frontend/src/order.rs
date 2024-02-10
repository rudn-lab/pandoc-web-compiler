use api::OrderInfoResult;
use gloo::storage::Storage;
use shadow_clone::shadow_clone;
use yew::{prelude::*, suspense::use_future};
use yew_autoprops::autoprops;
use yew_bootstrap::component::Spinner;
use yew_hooks::use_websocket;
use yew_router::hooks::use_navigator;

use crate::Route;

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
            OrderInfoResult::Completed(_) => todo!(),
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
        yew_hooks::UseWebSocketReadyState::Open => {
            // TODO: render actual data
            html!(<div class="alert alert-warning attention">
            {(*ws.message).clone()}
            </div>)
        }
        yew_hooks::UseWebSocketReadyState::Closing => {
            html!(<h1>{"Заказ почти готов..."}<Spinner/></h1>)
        }
        yew_hooks::UseWebSocketReadyState::Closed => {
            html!(<h1>{"Заказ готов..."}<Spinner/></h1>)
        }
    }
}
