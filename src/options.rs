use leptos::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn options_page() {
    mount_to_body(|| {
        view! {
            <p class="bg-green-200 h-screen flex items-center justify-center">
                "Hello, options page!"
            </p>
        }
    })
}
