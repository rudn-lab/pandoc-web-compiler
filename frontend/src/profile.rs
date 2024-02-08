use api::UserInfo;
use api::UserInfoResult;
use gloo::storage::Storage;
use shadow_clone::shadow_clone;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew::suspense::use_future;
use yew_bootstrap::component::form::*;
use yew_bootstrap::component::*;
use yew_bootstrap::util::*;
use yew_hooks::use_async;
use yew_router::hooks::use_navigator;
use yew_router::prelude::Link;

use crate::Route;

#[function_component(Profile)]
pub fn profile() -> Html {
    let profile_token = gloo::storage::LocalStorage::get("token");
    let profile_token: Option<String> = match profile_token {
        Ok(key) => key,
        Err(_) => None,
    };

    if let Some(token) = profile_token {
        let fallback = html! {
            <h1>{"Загружаем информацию профиля..."}<Spinner /></h1>
        };
        html!(
            <Suspense {fallback}>
                <ProfileInner token={token} />
            </Suspense>

        )
    } else {
        html!(<Register />)
    }
}

#[function_component(ProfileInner)]
fn profile_inner(props: &ProfileNavInnerProps) -> HtmlResult {
    let navigator = use_navigator().unwrap();
    let ProfileNavInnerProps { token } = props;
    let token = token.clone();

    let resp = use_future(|| async move {
        reqwest::get(format!("https://pandoc.danya02.ru/api/user-info/{token}"))
            .await?
            .json::<UserInfoResult>()
            .await
    })?;

    let result_html = match *resp {
        Ok(ref res) => match res {
            UserInfoResult::Ok(UserInfo { name, balance }) => html! {
                <>
                    <h1>{name}</h1>
                    <h2>{"Ваш текущий баланс: "}<code>{balance}</code></h2>

                </>
            },
            UserInfoResult::NoSuchToken => {
                navigator.push(&Route::Profile);
                gloo::storage::LocalStorage::delete("token");
                gloo::utils::document()
                    .location()
                    .unwrap()
                    .reload()
                    .unwrap();
                html!({ "Пользователь не существует" })
            }
        },
        Err(ref failure) => html!(<>{"Ошибка при загрузке профиля: "}{failure.to_string()}</>),
    };

    Ok(result_html)
}

#[function_component(ProfileNav)]
pub fn profile_nav() -> Html {
    let profile_key: Result<Option<String>, gloo::storage::errors::StorageError> =
        gloo::storage::LocalStorage::get("token");
    let profile_key: Option<String> = match profile_key {
        Ok(key) => key,
        Err(_) => None,
    };

    if let Some(key) = profile_key {
        let fallback = html! {
            <Link<Route> classes="nav-link" to={Route::Profile}>{"Загружаем пользователя..."}</Link<Route>>
        };
        html!(
            <div class="nav-item">
                <Suspense {fallback}>
                    <ProfileNavInner token={key} />
                </Suspense>
            </div>
        )
    } else {
        html!(
            <div class="nav-item">
                <Link<Route> classes="nav-link" to={Route::Profile}>{"Зарегестрируйся или войди сначала"}</Link<Route>>
            </div>
        )
    }
}

#[derive(Properties, PartialEq, Clone)]
struct ProfileNavInnerProps {
    pub token: AttrValue,
}

#[function_component(ProfileNavInner)]
fn profile_nav_inner(props: &ProfileNavInnerProps) -> HtmlResult {
    let navigator = use_navigator().unwrap();
    let ProfileNavInnerProps { token } = props;
    let token = token.clone();

    let resp = use_future(|| async move {
        reqwest::get(format!("https://pandoc.danya02.ru/api/user-info/{token}"))
            .await?
            .json::<UserInfoResult>()
            .await
    })?;

    let result_html = match *resp {
        Ok(ref res) => match res {
            UserInfoResult::Ok(UserInfo { name, .. }) => {
                format!("Привет, {name}")
            }
            UserInfoResult::NoSuchToken => {
                navigator.push(&Route::Profile);
                gloo::storage::LocalStorage::delete("token");

                gloo::utils::document()
                    .location()
                    .unwrap()
                    .reload()
                    .unwrap();

                "Пользователь не существует".to_string()
            }
        },
        Err(ref failure) => failure.to_string(),
    };

    Ok(html!(<Link<Route> classes="nav-link" to={Route::Profile}>{result_html}</Link<Route>>))
}

#[function_component(Register)]
fn register() -> Html {
    html!(
        <div>
            <div class="alert alert-warning attention">
                {"Войдите в ваш аккаунт, чтобы использовать конвертатор"}
            </div>
            <Row>
                <Column>
                    <ExistingRegister />
                </Column>
            </Row>
        </div>
    )
}

#[function_component(ExistingRegister)]
fn existing_register() -> Html {
    let navigator = use_navigator().unwrap();
    let token_state = use_state(|| String::new());
    let oninput_token = {
        shadow_clone!(token_state);
        move |ev: InputEvent| {
            let target: HtmlInputElement = ev.target().unwrap().dyn_into().unwrap();
            token_state.set(target.value());
        }
    };
    let token_result: yew_hooks::prelude::UseAsyncHandle<UserInfoResult, String> = use_async({
        shadow_clone!(token_state);
        async move {
            let token = (*token_state).clone();
            Ok(
                reqwest::get(format!("https://pandoc.danya02.ru/api/user-info/{token}"))
                    .await
                    .map_err(|v| v.to_string())?
                    .json::<UserInfoResult>()
                    .await
                    .map_err(|v| v.to_string())?,
            )
        }
    });

    let validation = match &token_result.data {
        Some(data) => match data {
            UserInfoResult::Ok(_) => FormControlValidation::Valid(None),
            UserInfoResult::NoSuchToken => {
                FormControlValidation::Invalid("Этот токен не найден".into())
            }
        },
        None => match &token_result.error {
            Some(why) => {
                FormControlValidation::Invalid(format!("Ошибка при проверке токена: {why}").into())
            }
            None => FormControlValidation::None,
        },
    };

    let start = {
        shadow_clone!(token_result);
        move |_ev| {
            token_result.run();
        }
    };

    if let Some(data) = &token_result.data {
        if let UserInfoResult::Ok(UserInfo { .. }) = data {
            let token = &*token_state;
            gloo::storage::LocalStorage::set("token", token.to_string()).unwrap();
            navigator.push(&Route::Home);
            gloo::utils::document()
                .location()
                .unwrap()
                .reload()
                .unwrap();
        }
    }

    html!(
        <>
            <h1>{"Войти в аккаунт"}</h1>

            <FormControl id="token" ctype={FormControlType::Text} class="mb-3" label="Токен аккаунта" oninput={oninput_token} value={(*token_state).clone()} {validation}/>


            <Button style={Color::Primary} disabled={&token_result.loading} onclick={start}>
                if token_result.loading {
                    <Spinner small={true}  />
                }
                {"Войти с токеном"}
            </Button>
            <hr />
            <p>{"Если у вас нет токена, "}<a href="https://t.me/danya02">{"обратитесь к администратору для регистрации"}</a>{"."}</p>
        </>
    )
}
