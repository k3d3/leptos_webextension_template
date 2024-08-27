use leptos::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn popup_page() {
    mount_to_body(|| {
        view! {
            <p class="bg-blue-200 h-[200px] w-[200px] flex items-center justify-center">
                "Hello, popup page!"
            </p>
        }
    })
}
