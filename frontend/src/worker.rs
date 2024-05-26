use yew_agent::Registrable;

pub mod proof_of_work_agent;

fn main() {
    proof_of_work_agent::Sha256PowReactor::registrar().register();
}
