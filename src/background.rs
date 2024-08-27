use wasm_bindgen::prelude::*;

use gloo_console::log;

#[wasm_bindgen]
pub async fn background_script() {
    log!("Hello, background script!");
}
