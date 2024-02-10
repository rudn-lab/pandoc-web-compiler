use api::{PricingInfo, UserInfoResult};
use gloo::storage::Storage;
use gloo::utils::window;
use js_sys::ArrayBuffer;
use js_sys::Promise;
use js_sys::Uint8Array;
use shadow_clone::shadow_clone;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::File;
use web_sys::FileList;
use web_sys::HtmlElement;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew::suspense::use_future;
use yew_autoprops::autoprops;
use yew_bootstrap::component::Column;
use yew_bootstrap::component::Row;
use yew_bootstrap::component::Spinner;
use yew_bootstrap::icons::BI;
use yew_hooks::use_async;
use yew_hooks::{use_drop_with_options, use_list, UseDropOptions};
use yew_router::hooks::use_navigator;

use crate::Route;

#[function_component(Upload)]
pub fn upload() -> Html {
    let fallback = html! {
        <h1>{"Загружаем текущие расценки..."}<Spinner/></h1>
    };
    html!(
        <Suspense {fallback}>
            <UploadInner />
        </Suspense>
    )
}
#[function_component(UploadInner)]
fn upload_inner() -> HtmlResult {
    let navigator = use_navigator().unwrap();
    let dropped_files: yew_hooks::prelude::UseListHandle<(File, String)> = use_list(vec![]);

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

            let pricing = reqwest::get("https://pandoc.danya02.ru/api/pricing")
                .await?
                .error_for_status()?
                .json::<PricingInfo>()
                .await?;

            let my_info = reqwest::get(format!("https://pandoc.danya02.ru/api/user-info/{token}"))
                .await?
                .error_for_status()?
                .json::<UserInfoResult>()
                .await?;

            Ok::<_, anyhow::Error>((pricing, my_info))
        })?
    };

    let do_upload = {
        shadow_clone!(dropped_files, navigator);
        use_async(async move {
            let profile_key = gloo::storage::LocalStorage::get("token");
            let profile_key: Option<String> = match profile_key {
                Ok(key) => key,
                Err(_) => None,
            };
            let token = if let Some(key) = profile_key {
                key
            } else {
                navigator.push(&Route::Profile);
                return Err("Нет токена");
            };

            let client = reqwest::Client::new();

            let mut form = reqwest::multipart::Form::new();

            for (file, name) in dropped_files.current().iter() {
                let promise: Promise = file.array_buffer();
                let array_buf = wasm_bindgen_futures::JsFuture::from(promise).await;
                let array_buf = if let Ok(a) = array_buf {
                    a
                } else {
                    let _ = web_sys::window().unwrap().alert_with_message(
                        "Ошибка при чтении файла, проверьте существование всех файлов",
                    );
                    return Err("Ошибка при чтении файлов");
                };
                let array_buf: ArrayBuffer = array_buf.dyn_into().unwrap();
                let int8arr = Uint8Array::new(&array_buf);
                let data = int8arr.to_vec();
                form = form.part(name.to_string(), reqwest::multipart::Part::bytes(data));
            }

            let resp = client
                .post(format!("https://pandoc.danya02.ru/api/orders/{token}/new"))
                .multipart(form)
                .send()
                .await
                .map_err(|_| "Ошибка при отправке файлов")?;

            Ok::<String, &'static str>(resp.text().await.map_err(|_| "Что-то не так с ответом")?)
        })
    };

    // let push = {
    //     shadow_clone!(dropped_files);
    //     Callback::from(move |what: File| {
    //         // Ensure that there are no files with the same name already here
    //         dropped_files.retain(|v: &(File, String)| v.1 != what.name());

    //         let name = what.name();
    //         web_sys::console::log_1(&what);
    //         dropped_files.push((what, name));
    //     })
    // };

    let push_with_path = {
        shadow_clone!(dropped_files);
        Callback::from(move |items: Vec<(File, String)>| {
            let mut common_prefix;
            if let Some(first) = items.iter().next() {
                common_prefix = first.1.as_str();
            } else {
                return;
            }
            for (_what, path) in items.iter() {
                let mut new_common_prefix_len = 0;
                for (a, b) in common_prefix.bytes().zip(path.bytes()) {
                    if a == b {
                        new_common_prefix_len += 1;
                    } else {
                        break;
                    }
                }
                while !common_prefix.is_char_boundary(new_common_prefix_len) {
                    new_common_prefix_len -= 1;
                }
                common_prefix = &common_prefix[0..new_common_prefix_len];
            }
            let common_prefix_len = common_prefix.len();
            let _ = common_prefix; // destroy reference to within items

            for (what, path) in items {
                let path = &path[common_prefix_len..];
                // Ensure that there are no files with the same name already here
                dropped_files.retain(|v: &(File, String)| v.1 != path);

                dropped_files.push((what, path.to_string()));
            }
        })
    };

    let result_html = match *resp {
        Ok((ref pricing, ref me)) => {
            let me = if let UserInfoResult::Ok(what) = me {
                what
            } else {
                window().location().reload().unwrap();
                loop {}
            };
            let make_delete_cb = {
                |name: String| {
                    let f = dropped_files.clone();
                    Callback::from(move |ev: MouseEvent| {
                        ev.prevent_default();
                        shadow_clone!(name);
                        f.retain(|v| &v.1 != &name);
                    })
                }
            };
            let mut total_cost = 0.0;
            let dropped_items = dropped_files
                .current()
                .iter()
                .enumerate()
                .map(|(idx, f)| {
                    let delete = make_delete_cb(f.1.clone());
                    let list = dropped_files.clone();
                    let oninput = Callback::from(move |ev: InputEvent| {
                        let target: HtmlInputElement = ev.target().unwrap().dyn_into().unwrap();
                        let new_name = target.value();

                        // This trickery takes the file out of the managed list, changes it, then puts it back in its original position.
                        let last_idx = list.current().len() - 1;
                        list.swap(idx, last_idx);
                        let mut file = list.pop().unwrap();
                        file.1 = new_name;
                        list.push(file);
                        list.swap(idx, last_idx);
                    });
                    let size = f.0.size();
                    let size_str = size_format::SizeFormatterBinary::new(size as u64);
                    let cost = (size / 1024.0 / 1024.0) * pricing.upload_mb_factor + pricing.upload_file_factor;
                    total_cost += cost;
                    let cost_str = format!("{cost:.3}");
                    html!(
                    <div class="input-group mb-1" >
                        <input type="text" class="form-control" value={f.1.clone()} {oninput}/>
                        <span class="input-group-text">{size_str}{"B = "}<code>{cost_str}{"𐆘"}</code></span>
                        <button onclick={delete} class="btn btn-outline-danger">{BI::X}</button>
                    </div>
                    )
                })
                .collect::<Html>();

            let delete_all = {
                shadow_clone!(dropped_files);
                Callback::from(move |ev: MouseEvent| {
                    ev.prevent_default();
                    dropped_files.clear();
                })
            };

            let cost_str = format!("{total_cost:.3}");

            let upload_block = {
                let mut failure_reasons = vec![];

                if total_cost > me.balance {
                    failure_reasons.push(html!(<>{"У вас недостаточно баланса, чтобы загрузить все эти файлы. "}<a href="https://t.me/danya02">{"Свяжитесь с администратором"}</a>{" для покупки промо-кодов, удалите или замените большие файлы или подождите восполнения баланса."}</>));
                }
                let mut makefile_exists = false;
                for (_file, name) in dropped_files.current().iter() {
                    if name == "Makefile" {
                        makefile_exists = true;
                        break;
                    }
                }

                if !makefile_exists {
                    failure_reasons.push(html!(<>{"В загруженных файлах должен присутствовать "}<code>{"Makefile"}</code>{". Он будет выполнен для обработки вашего заказа."}</>));
                }

                if failure_reasons.is_empty() {
                    let perform_upload = {
                        shadow_clone!(do_upload);
                        Callback::from(move |ev: MouseEvent| {
                            ev.prevent_default();
                            do_upload.run();
                        })
                    };
                    if let Some(ref resp) = do_upload.data {
                        if let Ok(id) = resp.parse::<i64>() {
                            navigator.push(&Route::Order { order_id: id });
                        } else {
                            log::error!("Server replied with non-integer: {resp:?}");
                        }
                    }
                    html!(
                        <div class="d-grid gap-2">
                            <button class="btn btn-primary" onclick={perform_upload} disabled={do_upload.loading}>
                                if do_upload.loading {
                                    <Spinner small={true} />
                                }
                                {"Отправить заказ"}
                            </button>
                            if let Some(error) = do_upload.error {
                                <p class="text-danger">{error}</p>
                            } else {}
                        </div>
                    )
                } else {
                    html!(
                        <>
                            <p>{"Вы не можете отправить ваш заказ на обработку, потому что: "}</p>
                            <ul>
                                {
                                    for failure_reasons.into_iter().map(|v| html!(<li>{v}</li>))
                                }
                            </ul>
                            <div class="d-grid gap-2">
                                <button disabled={true} class="btn btn-primary">
                                    {"Отправить заказ"}
                                </button>
                            </div>
                        </>
                    )
                }
            };

            html!(
                <>
                <p>{"Текущий баланс: "}<code>{me.balance}{"𐆘"}</code></p>

                <p>{"Текушие расценки:"}</p>
                <ul>
                    <li><code>{pricing.wall_time_factor}{"𐆘"}</code>{" за секунду реального времени выполнения"}</li>
                    <li><code>{pricing.user_time_factor}{"𐆘"}</code>{" за секунду процессора для основного кода"}</li>
                    <li><code>{pricing.sys_time_factor}{"𐆘"}</code>{" за секунду процессора для кода ядра"}</li>
                    <li><code>{pricing.upload_mb_factor}{"𐆘"}</code>{" за 1МБ загруженных файлов"}</li>
                    <li><code>{pricing.upload_file_factor}{"𐆘"}</code>{" за один загруженный файл"}</li>
                </ul>

                <p>{"Загрузите папку с работой сюда:"}
                    <UploadBox on_upload={push_with_path}/>
                </p>

                <hr />
                <Row>
                        <Column>
                        <p>{"Загруженные файлы:"}</p>
                        <div class="input-group mb-1" >
                            <button onclick={delete_all.clone()} class="btn btn-outline-danger">{"Удалить все файлы"}</button>
                        </div>
                        if dropped_files.current().len() == 0 {
                            <div class="input-group mb-1" >
                                <span class="input-group-text">{"Пока не загружено файлов..."}</span>
                            </div>

                        } else {
                            {dropped_items}
                        }
                        <div class="input-group mb-1" >
                            <button onclick={delete_all} class="btn btn-outline-danger">{"Удалить все файлы"}</button>
                        </div>
                        <p class="fs-3">
                            {"Общая стоимость файлов: "}
                            <code>
                            {cost_str}
                            {"𐆘"}
                            </code>
                        </p>
                        </Column>

                        <Column>
                            {upload_block}
                        </Column>
                    </Row>

                </>
            )
        }
        Err(ref failure) => {
            html!(<div class="alert alert-danger">{"Ошибка при загрузке информации. Перезагрузите страницу. Причина: "}{failure}</div>)
        }
    };

    Ok(result_html)
}

#[autoprops]
#[function_component(DropArea)]
#[allow(unreachable_code, unused_variables)]
fn drop(on_drop: Callback<web_sys::File>) -> Html {
    todo!("This component doesn't handle directories properly, fix this before using!");
    let node = use_node_ref();
    let state = use_drop_with_options(
        node.clone(),
        UseDropOptions {
            onfiles: Some(Box::new(move |files, _data_transfer| {
                log::info!("Dropped files: {files:?}");
                for file in files {
                    let file: web_sys::File = file;

                    on_drop.emit(file);
                }
            })),
            ..Default::default()
        },
    );
    html! {
        <div class="card">
            <div ref={node} class="card-body" style={
                if *state.over {
                    "background-color: var(--bs-success-bg-subtle);"
                } else {
                    "background-color: var(--bs-success-border-subtle);"
                }
            }>
                <p class="text-center">
                    { "Перенесите файлы для загрузки сюда" }
                </p>
                <p class="text-center text-secondary fs-1 my-4">
                    { yew_bootstrap::icons::BI::PLUS_SQUARE_DOTTED }
                </p>
            </div>
        </div>
    }
}

#[wasm_bindgen]
extern "C" {
    fn prepare_input_element(el: &HtmlElement, on_change: &Closure<dyn FnMut(Event)>);
}

#[autoprops]
#[function_component(UploadBox)]
fn uploadbox(on_upload: Callback<Vec<(web_sys::File, String)>>) -> Html {
    let node = use_node_ref();

    use_effect_with((node.clone(), on_upload.clone()), {
        shadow_clone!(node);

        move |(_node, on_upload)| {
            let mut files_listener = None;
            shadow_clone!(on_upload);

            if let Some(element) = node.cast::<HtmlElement>() {
                let onfiles = Callback::from(move |ev: Event| {
                    log::debug!("Called Callback on files!");
                    let source: HtmlInputElement = ev.target().unwrap().dyn_into().unwrap();
                    log::debug!("Received element");
                    if let Some(files) = source.files() {
                        log::debug!("The source has files");
                        let files: FileList = files;
                        let count = files.length();
                        let mut items = Vec::with_capacity(count as usize);
                        for i in 0..count {
                            items.push(files.get(i).expect("File at this index should exist because it's inside the bounds of the list."));
                        }
                        log::debug!("Collected items");
                        let propname = JsValue::from_str("webkitRelativePath");
                        let target_collection: Vec<_> = items
                            .into_iter()
                            .map(|item| {
                                let path = js_sys::Reflect::get(&item, &propname)
                                    .unwrap_or_else(|_| JsValue::from_str(""))
                                    .as_string()
                                    .unwrap_or_else(String::new);
                                (item, path)
                            })
                            .collect();
                        if target_collection.is_empty() {
                            let _ = web_sys::window().unwrap().alert_with_message(
                                "В этой папке нет файлов, поэтому ничего не было добавлено.",
                            );
                        };

                        on_upload.emit(target_collection)
                    }
                });

                web_sys::console::log_1(&element);

                let listener = Closure::new(move |ev| {
                    log::debug!("Called Rust closure on files!");
                    onfiles.emit(ev);
                });
                prepare_input_element(&element, &listener);

                files_listener = Some(listener);
                log::info!("Created listener on object");
            }

            move || drop(files_listener)
        }
    });

    html!(
        <>
            <div ref={node}></div>
        </>
    )
}
