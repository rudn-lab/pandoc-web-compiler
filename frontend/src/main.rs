mod debug_pow;
mod order;
mod profile;
mod promocodes;
mod proof_of_work_agent;
mod upload;

use gloo::storage::Storage;
use yew::prelude::*;
use yew_bootstrap::component::*;
use yew_bootstrap::icons::*;
use yew_router::prelude::Link;
use yew_router::prelude::*;

use crate::order::Order;
use crate::profile::Profile;
use crate::profile::ProfileNav;
use crate::upload::Upload;

const MONEY: &str = "𐆘";

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Home,
    #[at("/profile")]
    Profile,

    #[at("/upload")]
    Upload,

    #[at("/order/:order_id")]
    Order { order_id: i64 },

    #[at("/debug/pow")]
    DebugPow,

    #[not_found]
    #[at("/404")]
    NotFound,
}

#[function_component(Home)]
fn home() -> Html {
    let navigator: Navigator = use_navigator().unwrap();

    let profile_key = gloo::storage::LocalStorage::get("token");
    let profile_key: Option<String> = match profile_key {
        Ok(key) => key,
        Err(_) => None,
    };
    if profile_key.is_none() {
        navigator.push(&Route::Profile);
    }

    html! {
        <>
            {"Hello world"}
        </>
    }
}

#[function_component(App)]
fn app() -> Html {
    fn switch(route: Route) -> Html {
        match route {
            Route::Home => html!(<Home/>),
            Route::Profile => html!(<Profile />),
            Route::Upload => html!(<Upload />),
            Route::Order { order_id: id } => html!(<Order {id} />),
            Route::DebugPow => html!(<debug_pow::DebugPow />),
            Route::NotFound => html!("404"),
        }
    }

    html! {
        <BrowserRouter>
            {BIFiles::cdn()}
            <nav class="navbar bg-body-tertiary">
                <div class="container-fluid">
                    <Link<Route> classes="navbar-brand" to={Route::Home}>{"Yamadharma Pandoc"}</Link<Route>>

                    <Link<Route> classes="nav-link" to={Route::Upload}>{"Загрузка на обработку"}</Link<Route>>

                    <ProfileNav />
                </div>
            </nav>
            <Container>
                <Switch<Route> render={switch} />
            </Container>
        </BrowserRouter>
    }
}

lazy_static::lazy_static! {
    static ref BASE_URL: String = web_sys::window().unwrap().origin().to_string();
}

#[macro_use]
mod url_macro {
    macro_rules! url {
        ($($x:expr),*) => {
            {
                use crate::BASE_URL;
                let url_fragment = format!($($x),*);
                let base_url = &*BASE_URL;
                format!("{}{url_fragment}", base_url)
            }
        };
    }
    pub(crate) use url;
}

fn main() { 
    wasm_logger::init(wasm_logger::Config::default());

    yew::Renderer::<App>::new().render();
}
