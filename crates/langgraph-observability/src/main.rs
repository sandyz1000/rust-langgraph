#[cfg(feature = "frontend")]
use dioxus::prelude::*;
#[cfg(feature = "frontend")]
use langgraph_observability::frontend::App;

#[cfg(feature = "frontend")]
fn main() {
    dioxus_web::launch::launch_cfg(App, dioxus_web::Config::new());
}

#[cfg(not(feature = "frontend"))]
fn main() {
    println!("Frontend feature is not enabled. Use --features frontend to enable WASM frontend.");
}
