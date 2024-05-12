use api::RedeemPromocodeResponse;
use chrono::Local;
use gloo::storage::Storage;
use shadow_clone::shadow_clone;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_bootstrap::{
    component::{
        form::{FormControl, FormControlType, FormControlValidation},
        Button, Spinner,
    },
    util::Color,
};
use yew_hooks::use_async;
use yew_router::hooks::use_navigator;

use crate::Route;

#[function_component(RedeemPromocodeWidget)]
pub fn redeem_promocode_widget() -> Html {
    let navigator = use_navigator().unwrap();
    let profile_key: Result<Option<String>, gloo::storage::errors::StorageError> =
        gloo::storage::LocalStorage::get("token");
    let profile_key: Option<String> = match profile_key {
        Ok(key) => key,
        Err(_) => None,
    };
    let token = if let Some(t) = profile_key {
        t
    } else {
        navigator.push(&Route::Profile);
        String::new()
    };

    let code_state = use_state(|| String::new());

    let oninput = {
        shadow_clone!(code_state);
        move |ev: InputEvent| {
            let target: HtmlInputElement = ev.target().unwrap().dyn_into().unwrap();
            code_state.set(target.value());
        }
    };

    let redeem_result: yew_hooks::prelude::UseAsyncHandle<RedeemPromocodeResponse, String> =
        use_async({
            shadow_clone!(code_state);
            async move {
                Ok({
                    let code = (*code_state).clone();
                    let client = reqwest::Client::default();
                    client
                        .post(format!(
                            "https://pandoc.danya02.ru/api/user-info/{token}/redeem/{code}"
                        ))
                        .send()
                        .await
                        .map_err(|v| v.to_string())?
                        .json::<RedeemPromocodeResponse>()
                        .await
                        .map_err(|v| v.to_string())?
                })
            }
        });

    let start = {
        shadow_clone!(redeem_result);
        move |_ev| {
            redeem_result.run();
        }
    };

    let validation = match &redeem_result.data {
        Some(v) => match v {
            RedeemPromocodeResponse::Ok {
                promocode_value,
                user_balance_after,
            } => FormControlValidation::Valid(Some(format!("Вы успешно пополнили баланс на {promocode_value:.3}𐆘! Теперь у вас {user_balance_after:.3}𐆘, обновите страницу чтобы увидеть результат.").into())),
            RedeemPromocodeResponse::AlreadyRedeemed {
                when_unix_time,
                by_me,
            } => FormControlValidation::Invalid({
                let when = chrono::DateTime::from_timestamp(*when_unix_time as i64, 0)
                    .expect("failed to parse incoming unix time as date")
                    .with_timezone(&Local)
                    .to_string();
                match by_me {
                true => format!("Вы уже активировали этот промокод в {when}."),
                false => format!("Этот промокод уже был активирован кем-то еще в {when}. Свяжитесь с администратором для информации.")
            }.into()
            }),
            RedeemPromocodeResponse::NotFound => FormControlValidation::Invalid(
                "Мы не смогли найти такой промокод. Свяжитесь с администратором для информации."
                    .into(),
            ),
        },
        None => match &redeem_result.error {
            Some(why) => FormControlValidation::Invalid(
                format!("Ошибка при активации промокода: {why}").into(),
            ),
            None => FormControlValidation::None,
        },
    };

    html! {
        <>
        <h3>{"Активировать промокод"}</h3>
            <FormControl id="promocode" ctype={FormControlType::Text} class="mb-3" label="Промокод" {oninput} value={(*code_state).clone()} disabled={&redeem_result.loading} validation={validation.clone()}/>

            <Button style={Color::Primary} disabled={&redeem_result.loading} onclick={start}>
                if redeem_result.loading {
                    <Spinner small={true}  />
                    {"Активируем..."}
                }
                else {
                    {"Активировать"}
                }
            </Button>

        </>
    }
}
