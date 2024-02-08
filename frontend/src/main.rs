mod profile;

use gloo::storage::Storage;
use yew::prelude::*;
use yew_bootstrap::component::*;
use yew_bootstrap::icons::*;
use yew_router::prelude::Link;
use yew_router::prelude::*;

use crate::profile::Profile;
use crate::profile::ProfileNav;

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Home,
    #[at("/profile")]
    Profile,

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
            Route::NotFound => html!("404"),
        }
    }

    html! {
        <BrowserRouter>
            {BIFiles::cdn()}
            <nav class="navbar bg-body-tertiary">
                <div class="container-fluid">
                    <Link<Route> classes="navbar-brand" to={Route::Home}>{"Yamadharma Pandoc"}</Link<Route>>

                    <ProfileNav />
                </div>
            </nav>
            <Container>
                <Switch<Route> render={switch} />
            </Container>
        </BrowserRouter>
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());

    yew::Renderer::<App>::new().render();
}
