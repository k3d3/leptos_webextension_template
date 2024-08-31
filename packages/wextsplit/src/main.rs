//! This is a post-build hook script for Trunk that will use wasm-split to take
//! the output wasm file, split it into a debug file, and make sure
//! the wasm file refers to the debug file via the Trunk server,
//! rather than through the `chrome-extension://` protocol.
//!
//! This allows Chrome's [DWARF debugging extension](https://goo.gle/wasm-debugging-extension)
//! to actually be able to debug the wasm file. By default, the extension cannot
//! access debug files via `chrome-extension://` or `file://` URLs.
//!
//! We should only run this script in debug mode, since we don't want debug information
//! in releases (as far as I assume).

use std::env;
use std::time::Instant;

fn main() {
    let start_time = Instant::now();
    // let trunk_profile = std::env::var("TRUNK_PROFILE").unwrap();
    // if trunk_profile != "debug" {
    //     return;
    // }

    // list all environment variables starting with TRUNK_
    for (key, value) in std::env::vars() {
        if key.starts_with("TRUNK_") {
            println!("{}: {}", key, value);
        }
    }

    let trunk_staging_dir = std::env::var("TRUNK_STAGING_DIR").unwrap();
    let trunk_staging_dir = std::path::Path::new(&trunk_staging_dir);
    // get the first file that ends with _bg.wasm
    let wasm_file = trunk_staging_dir
        .read_dir()
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().unwrap() == "wasm")
        .unwrap();

    // Move the _bg.wasm file to _bg.wasm.orig
    let wasm_file_orig = wasm_file.with_extension("wasm.orig");
    let wasm_file_debug = wasm_file.with_extension("wasm.debug");
    std::fs::rename(&wasm_file, &wasm_file_orig).unwrap();

    // Figure out the `trunk serve` server address, if set in the environment
    let address = env::var("TRUNK_SERVE_ADDRESS").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("TRUNK_SERVE_PORT").unwrap_or_else(|_| "8080".to_string());
    let address = format!("{address}:{port}");

    // Run wasm-split <.wasm.orig> -o <.wasm> --strip --debug-out=<.debug> --external-dwarf-url=<.debug>
    std::process::Command::new("wasm-split")
        .arg(&wasm_file_orig)
        .arg("-o")
        .arg(&wasm_file)
        .arg("--strip")
        .arg("--debug-out")
        .arg(&wasm_file_debug)
        .arg("--external-dwarf-url")
        .arg(&format!(
            "http://{}/{}",
            address,
            wasm_file_debug.file_name().unwrap().to_str().unwrap()
        ))
        .output()
        .unwrap();

    let duration = start_time.elapsed();
    println!("Wextsplit finished in {:?}", duration);
}
