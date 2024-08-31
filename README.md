# Leptos WebExtension Template

This repository is a template for getting started with Trunk and Leptos, outputting the directory format required for WebExtensions.

It includes an action popup, an options page, and a background script. All of these are contained within the same wasm binary, and
each script/page calls a different wasm_bindgen function.

This is achieved by the included `wextrunk` script, which is a post-build hook for Trunk that allows outputting multiple pages instead of one.

## Features

- Supports Firefox and Chrome WebExtensions
- Uses Trunk for building
- Compatible with Trunk file hashing
- Leptos-based (and uses nightly Rust)
- TailwindCSS for styling (though it's not required)
- Hot Reloading in popup and options pages
- Nix flake with nightly Rust
- Debugging with the [Chrome DWARF extension](https://goo.gle/wasm-debugging-extension)

## Prerequisites

- Nightly Rust (stable should also work, but this template uses nightly by default)
- wasm32-unknown-unknown target (install with `rustup target add wasm32-unknown-unknown`)
- Trunk (install with `cargo install trunk`)
- Wasm-pack (install with `cargo install wasm-pack`)

If using Nix and direnv, these should all be handled automatically.

## Running in development mode

In order to develop extensions with hot reloading enabled:

```sh
# Building a Chrome extension
trunk serve

# Building a Firefox extension
WEXTRUNK_TARGET=firefox trunk serve
```

This will output a debug build of the extension to the `dist` directory, as well as run a Trunk dev server at `localhost:8080` that will reload the extension page when changes are made.

Additionally, you should be able to use the VSCode debugger to debug your webextension.

To change the details of the Trunk dev server, you'll need to use environment variables:

```sh
TRUNK_SERVE_ADDRESS=10.0.0.1 TRUNK_SERVE_PORT=8081 TRUNK_SERVE_WS_BASE=/other trunk serve
```

Using the CLI flags (i.e. `--address`, `--port`, `--ws-base`) will not work, as Trunk does not pass these flags
to `wextrunk`.

## Building for production

In order to build a production version of the extension:

```sh
# Building a Chrome extension
trunk build --release

# Building a Firefox extension
WEXTRUNK_TARGET=firefox trunk build --release
```

This will output a production build of the extension to the `dist` directory.

## Configuration

Like with a regular Trunk install, configuration is done by adding tags to `index.html`.

The `wextrunk` script will pick up on tags containing `data-wextrunk`, which can be used to add HTML pages and background scripts to the extension.

These are also used to select the correct manifest file.

In order to restrict tags to only specific pages, you can use the `data-wextrunk-include` attribute. Note that since `wextrunk` is a post-build hook, it will only filter post-build tags. Luckily, Trunk forwards `data-wextrunk-include` on most tags, so the inout should match the output.

## Debugging

This template includes a `launch.json` file for debugging in VSCode. This file is set up to use the Chrome DWARF extension, which allows for debugging Rust code in the browser.

To use this, you should be able to go to the debug tab and click "Debug (Chrome)". Once you run this, you should see the
original Rust code in the browser dev console.

The "Debug (Firefox)" configuration is YMMV.

## Loading the extension into a browser

This template will not automatically load an extension into a temporary browser, which means you will have to manually load the extension into Chrome.

### Chrome

1. Open Chrome and navigate to `chrome://extensions`
2. Make sure developer mode is enabled in the top right corner
3. Click on "Load unpacked" in the top left corner
4. Select the `dist` directory

### Firefox

1. Open Firefox and navigate to `about:debugging`
2. Click on "This Firefox" in the left sidebar
3. Click on "Load Temporary Add-on..."
4. Select the `manifest.json` file in the `dist` directory

If you're running `trunk serve`, the extension should automatically reload when changes are made.

## How `wextrunk` works

`wextrunk` is a post-build hook for Trunk that allows outputting multiple pages instead of one. The
post-build hook is defined in `Trunk.toml`.

It's set up using [the `xtask` method](https://github.com/matklad/cargo-xtask), which means no extra
applications need to be installed. The `cargo wextrunk` alias is defined in .cargo/config.toml.

When Trunk finishes building, it will create an `index.html` file in the `dist` directory. This file
is then read by `wextrunk`, which will parse the file and look for tags containing `data-wextrunk`,
processing them accordingly.

Finally, `wextrunk` will process the `data-wextrunk-include` attributes for each HTML page, filtering
out the ones that don't match the current page. By default, Trunk outputs scripts inline in the HTML,
which is forbidden in WebExtensions. `wextrunk` will move these scripts to a "shim" file, which is then
referred to in the HTML file.

JavaScript scripts are also post-processed by `wextrunk`, by inserting the calls to the correct `wasm_bindgen`
function for any given defined page or script. In the case of background scripts, since Trunk outputs scripts
with top-level async calls, `wextrunk` will wrap the script in an async IIFE.

Finally, `wextrunk` will copy the `manifest.json` file for the selected target to the `dist` directory. No
special processing happens here; it just copies from whatever's specified in the manifest tag's `href` attribute.

## How `wextsplit` works

`wextsplit` is set up similarly to `wextrunk`, such that it's using another `xtask` package. This package is
used to enable Rust debugging in Chrome.

By default, if wasm-bindgen is told to output debug symbols, it will output one wasm file containing both the
symbols and the code. This would normally work for a regular web application because the Chrome DWARF extension
can easily access it via HTTP. However, since we're building a WebExtension, the extension tries to access the wasm
file via the `chrome-extension://` protocol, which won't work.

To get around this, we can split the wasm file into two: one containing the symbols and one containing the code.
Then, we can add an entry in the wasm file to tell the DWARF extension that it can grab the symbols over HTTP.

More specifically, `wextsplit` calls symbolicator's `wasm-split` command to do exactly this. It then piggybacks
on the `trunk serve` command to serve the symbols over HTTP.

In the end, this means debugging should Just Work™. (Of course there are a billion ways this could fail, but so far
I haven't had too many issues with it.)

## License

This template is released to the public domain.

Where that is not possible, it is licenced under:

- MIT License (LICENSE-MIT or http://opensource.org/licenses/MIT)
- Apache License, Version 2.0 (LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0)
- CC0 1.0 Universal (LICENSE-CC0 or https://creativecommons.org/publicdomain/zero/1.0/)

at your option.

If you plan to use this template in your own project and don't want to release it to the public domain or under
these licenses, don't forget to remove these files and this blurb.
