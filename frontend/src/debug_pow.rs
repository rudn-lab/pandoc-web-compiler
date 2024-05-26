use api::verification::ProofOfWorkChallenge;
use shadow_clone::shadow_clone;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_agent::reactor::ReactorProvider;
use yew_hooks::use_list;

use crate::proof_of_work_agent::{PowReactorCommand, PowReactorInput, Sha256PowReactor};

#[function_component(DebugPow)]
pub fn debug_pow_outer() -> Html {
    html!(
        <ReactorProvider<Sha256PowReactor> path="/worker.js">
            <DebugPowInner />
        </ReactorProvider<Sha256PowReactor>>
    )
}

#[function_component(DebugPowInner)]
fn debug_proof_of_work() -> Html {
    let challenge_state = use_state(String::new);
    let logs = use_list(vec![]);

    let sub = yew_agent::reactor::use_reactor_bridge::<Sha256PowReactor, _>({
        shadow_clone!(logs);
        move |msg| {
            logs.push(format!("> {msg:?}"));
        }
    });

    let oninput = {
        let challenge_state = challenge_state.clone();
        Callback::from(move |event: InputEvent| {
            shadow_clone!(challenge_state);
            let input: HtmlInputElement = event.target_unchecked_into();
            challenge_state.set(input.value());
        })
    };

    let start_calc = Callback::from({
        shadow_clone!(challenge_state, logs, sub);
        move |ev: MouseEvent| {
            ev.prevent_default();
            let state = (*challenge_state).clone();
            logs.push(format!("Starting calculation using data: {state}"));
            let state = state.split(".").next().unwrap();
            let challenge: ProofOfWorkChallenge = serde_json::from_str(&state).unwrap();

            sub.send(PowReactorCommand::Input(PowReactorInput {
                difficulty: challenge.difficulty,
                nonce: challenge.nonce,
            }));
        }
    });

    let stop_calc = Callback::from({
        shadow_clone!(logs, sub);
        move |ev: MouseEvent| {
            ev.prevent_default();
            logs.push("Stopping calculation".to_string());
            sub.send(PowReactorCommand::Stop);
        }
    });

    html! {
        <>
            <div class="form-group">
                <input cls="form-control" type="text" value={(*challenge_state).clone()} {oninput} />

                <button class="btn btn-primary" onclick={start_calc}>{"Start"}</button>
                <button class="btn btn-danger" onclick={stop_calc}>{"Stop"}</button>

                <ul class="list-group">
                    { for logs.current().iter().map(|msg| html! { <li class="list-group-item">{msg}</li> }) }
                </ul>
            </div>
        </>
    }
}
