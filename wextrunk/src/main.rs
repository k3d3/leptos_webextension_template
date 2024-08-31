//! This is a post-build hook script for Trunk that will take the index.html file output
//! by Trunk, and post-process it such that it can be used in a WebExtension.
//!
//! The main functions of this script are to:
//! - Split the index.html file into multiple endpoints, so it can be use in various
//!   WebExtension contexts (e.g. popup, background, content script, options page).
//! - Move the inline script into a separate "shim" file, as WebExtensions don't allow inline
//!   scripts.
//! - Remove preloads, as they're incompatible with WebExtensions.
//! - Remove integrity attributes, as they're incompatible with WebExtensions.
//! - For background scripts, wrap Trunk's output in an async IIFE, as top-level await is not
//!   allowed in service workers, as used in background scripts.
//! - For automatic reloading, substitutes the dev server variables in the auto-reload script,
//!   so they don't need to be run through the `trunk serve` web server.
//!
//! There's also functionality to remove reload functionality from scripts on a per-page and
//! per-script basis.

use core::panic;
use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    time::Instant,
};

use lol_html::{element, html_content::ContentType, text, HtmlRewriter, Settings};

/// HTML page to output. Will more or less clone the output index.html file,
/// but with a changed name, and the inline script moved elsewhere.
#[derive(Debug)]
struct HtmlPage {
    name: String,
    html: String,
    no_reload: bool,
    wasm_fn: String,
}

/// Script to output. Will basically just be what's normally in the inline script.
/// This means background scripts can be reloaded.
#[derive(Debug)]
struct Script {
    js: String,
    no_reload: bool,
    background_script: bool,
    wasm_fn: String,
}

/// Manifest file to output. Will be copied from the source directory to the
/// staging directory.
#[derive(Debug)]
struct Manifest {
    href: String,
}

/// Results of processing the index.html file. This should contain everything
/// needed to output the HTML and script files needed within the WebExtension.
#[derive(Debug)]
struct CollectOutput {
    html_pages: Vec<HtmlPage>,
    scripts: Vec<Script>,
    manifest: Manifest,
    html_template: String,
    script_contents: String,
}

/// Create an HTML template from Trunk-generated index.html,
/// collecting wextrunk-specific values along the way.
fn process_index_html(html_path: &Path, target: Option<&str>) -> CollectOutput {
    let mut html_pages = Vec::new();
    let mut scripts = Vec::new();
    let mut script_contents = String::new();

    let mut selected_manifest: Option<Manifest> = None;

    let mut html_template_bytes = Vec::new();
    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                // Handle the `data-wextrunk` tags, which are used to define output
                // HTML pages, scripts, and manifests.
                element!("link[data-wextrunk]", |el| {
                    match el.get_attribute("rel").as_deref() {
                        Some("htmlpage") => {
                            html_pages.push(HtmlPage {
                                name: el
                                    .get_attribute("name")
                                    .expect("htmlpage link must have a name")
                                    .to_string(),
                                html: el
                                    .get_attribute("html")
                                    .expect("htmlpage link must have an html field")
                                    .to_string(),
                                no_reload: el.has_attribute("no-reload"),
                                wasm_fn: el
                                    .get_attribute("wasm-fn")
                                    .expect("htmlpage link must have a wasm-fn field")
                                    .to_string(),
                            });
                        }
                        Some("script") => {
                            scripts.push(Script {
                                js: el
                                    .get_attribute("js")
                                    .expect("script link must have a js field")
                                    .to_string(),
                                no_reload: el.has_attribute("no-reload"),
                                background_script: el.has_attribute("background-script"),
                                wasm_fn: el
                                    .get_attribute("wasm-fn")
                                    .expect("script link must have a wasm-fn field")
                                    .to_string(),
                            });
                        }
                        Some("manifest") => {
                            if let Some(requested_target) = target {
                                let manifest_target = el
                                    .get_attribute("target")
                                    .expect("manifest link must have an href");
                                if manifest_target == requested_target {
                                    if selected_manifest.is_some() {
                                        panic!("Multiple manifests were selected, but only one is allowed.");
                                    }
                                    selected_manifest = Some(Manifest {
                                        href: el
                                            .get_attribute("href")
                                            .expect("manifest link must have an href")
                                            .to_string(),
                                    });
                                }
                            } else if el.has_attribute("default") {
                                if selected_manifest.is_some() {
                                    panic!("Multiple default manifests were selected, but only one is allowed.");
                                }
                                selected_manifest = Some(Manifest {
                                    href: el
                                        .get_attribute("href")
                                        .expect("manifest link must have an href")
                                        .to_string(),
                                });
                            }
                        }
                        _ => {}
                    }
                    el.remove();
                    Ok(())
                }),
                // Handler for generated inline string. We want
                // to grab the contents, and then delete it.
                text!("script:not([src])", |el| {
                    script_contents.push_str(el.as_str());
                    el.remove();
                    if el.last_in_text_node() {
                        el.replace("", ContentType::Text);
                    }
                    Ok(())
                }),
                // Sometimes, Trunk outputs a separate empty script tag.
                // We don't want anything to do with this, so just remove it.
                element!("script:not([src])", |el| {
                    if el.attributes().is_empty() {
                        el.remove();
                    }
                    Ok(())
                }),
            ],
            ..Settings::default()
        },
        |c: &[u8]| html_template_bytes.extend_from_slice(c),
    );

    // Feed the index.html file into the lol_html rewriter.
    // In doing so, lol_html will write the html template out
    // to html_template_bytes.
    let mut html_file = File::open(html_path).unwrap();
    let mut rewriter_buf = [0; 16384];
    loop {
        let bytes_read = html_file.read(&mut rewriter_buf).unwrap();
        if bytes_read == 0 {
            break;
        }
        rewriter.write(&rewriter_buf[..bytes_read]).unwrap();
    }
    rewriter.end().unwrap();

    let Some(manifest) = selected_manifest else {
        panic!("No manifest was selected, but one is required. You must specify a manifest as default, or specify a target with the WEXTRUNK_TARGET environment variable.");
    };

    let html_template = std::str::from_utf8(&html_template_bytes)
        .unwrap()
        .to_string();

    CollectOutput {
        html_pages,
        scripts,
        manifest,
        html_template,
        script_contents,
    }
}

/// Template used for the auto-reload script.
/// This just splits the auto-reload script into hardcoded parts,
/// where variables are interspersed between them.
#[derive(Debug)]
struct AutoReloadTemplate {
    /// Everything before the TRUNK_ADDRESS varable.
    before_address: String,
    /// Everything between TRUNK_ADDRESS and TRUNK_WS_BASE.
    after_address: String,
    /// Everything after TRUNK_WS_BASE.
    after_base: String,
}

impl AutoReloadTemplate {
    fn new(auto_reload_contents: &str) -> Self {
        const TRUNK_ADDRESS: &str = "{{__TRUNK_ADDRESS__}}";
        let address_start = auto_reload_contents
            .find(TRUNK_ADDRESS)
            .expect("Should find address in auto-reload script output");
        let address_end = address_start + TRUNK_ADDRESS.len();

        const TRUNK_WS_BASE: &str = "{{__TRUNK_WS_BASE__}}";
        let base_start = auto_reload_contents[address_end..]
            .find(TRUNK_WS_BASE)
            .expect("Should find base in auto-reload script output")
            + address_end;
        let base_end = base_start + TRUNK_WS_BASE.len();

        AutoReloadTemplate {
            before_address: auto_reload_contents[..address_start].to_string(),
            after_address: auto_reload_contents[address_end..base_start].to_string(),
            after_base: auto_reload_contents[base_end..].to_string(),
        }
    }

    /// Render to a writer, to reduce String clones.
    fn render(&self, address: &str, base: &str, writer: &mut impl Write) {
        writer.write_all(self.before_address.as_bytes()).unwrap();
        writer.write_all(address.as_bytes()).unwrap();
        writer.write_all(self.after_address.as_bytes()).unwrap();
        writer.write_all(base.as_bytes()).unwrap();
        writer.write_all(self.after_base.as_bytes()).unwrap();
    }
}

/// Template for output script files.
/// There's a decent amount of post-processing happening here,
/// however it's similar to the AutoReloadTemplate in that each
/// parsed section is handled differently depending on how
/// the ScriptTemplate is called.
#[derive(Debug)]
struct ScriptTemplate {
    /// Import init line.
    import_line: String,
    /// Everything before `dispatchEvent`. This is where we want to put wasm_fn's call.
    init: String,
    /// DispatchEvent itself. We want to keep this separate from auto-reload code.
    dispatch_event: String,
    /// Auto-reload code, if it exists. Otherwise, just an empty string.
    auto_reload: Option<AutoReloadTemplate>,
}

impl ScriptTemplate {
    fn find_dispatch_event(script_contents: &str, start_offset: usize) -> Option<(usize, usize)> {
        let dispatch_event_start =
            script_contents[start_offset..].find("\ndispatchEvent")? + start_offset + 1;
        let dispatch_event_end =
            script_contents[dispatch_event_start..].find(";\n")? + dispatch_event_start + 1;

        Some((dispatch_event_start, dispatch_event_end))
    }

    fn new(script_contents: &str) -> Self {
        let import_start = script_contents
            .find("import")
            .expect("Should find import line in Trunk script output");
        let import_end = script_contents[import_start..]
            .find(";\n")
            .expect("Should find end of import line in Trunk script output")
            + import_start
            + 1;
        let (dispatch_event_start, dispatch_event_end) =
            match Self::find_dispatch_event(&script_contents, import_end) {
                Some((start, end)) => (start, end),
                None => {
                    let init_end = script_contents[import_end..]
                        .find(".wasm');\n")
                        .expect("Should find end of init line in Trunk script output")
                        + import_end
                        + 1;

                    (init_end, init_end)
                }
            };

        let import_line = format!("{}\n", &script_contents[import_start..import_end]);
        let dispatch_event = script_contents[dispatch_event_start..dispatch_event_end].to_string();
        let pre_init = fix_init_line(script_contents[import_end..dispatch_event_start].trim());
        let auto_reload_contents = script_contents[dispatch_event_end..].to_string();
        let auto_reload = if auto_reload_contents.contains("function") {
            Some(AutoReloadTemplate::new(&auto_reload_contents))
        } else {
            None
        };

        ScriptTemplate {
            import_line,
            init: pre_init,
            dispatch_event,
            auto_reload,
        }
    }

    /// Render to a writer, to reduce String clones.
    ///
    /// Adds a wrapper depending on if we're writing to a background script or not.
    fn render(&self, wasm_fn: &str, no_reload: bool, bg_wrapper: bool, writer: &mut impl Write) {
        let address = env::var("TRUNK_SERVE_ADDRESS").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = env::var("TRUNK_SERVE_PORT").unwrap_or_else(|_| "8080".to_string());
        let ws_base = env::var("TRUNK_SERVE_WS_BASE").unwrap_or_else(|_| "/".to_string());
        let address = format!("{address}:{port}");

        if bg_wrapper {
            self.render_with_wrapper(wasm_fn, no_reload, &address, &ws_base, writer);
        } else {
            self.render_without_wrapper(wasm_fn, no_reload, &address, &ws_base, writer);
        }
    }

    /// Render without a wrapper, for scripts that don't need to be background scripts.
    fn render_without_wrapper(
        &self,
        wasm_fn: &str,
        no_reload: bool,
        address: &str,
        ws_base: &str,
        writer: &mut impl Write,
    ) {
        writer.write_all(self.import_line.as_bytes()).unwrap();
        writer.write_all(self.init.as_bytes()).unwrap();
        let wasm_fn = format!("await wasm.{wasm_fn}();\n");
        writer.write_all(wasm_fn.as_bytes()).unwrap();
        writer.write_all(self.dispatch_event.as_bytes()).unwrap();
        if !no_reload {
            if let Some(auto_reload) = &self.auto_reload {
                auto_reload.render(address, ws_base, writer);
            }
        }
    }

    /// Render to a writer with a wrapper that allows using this as a
    /// background service worker in Chrome.
    fn render_with_wrapper(
        &self,
        wasm_fn: &str,
        no_reload: bool,
        address: &str,
        ws_base: &str,
        writer: &mut impl Write,
    ) {
        writer.write_all(self.import_line.as_bytes()).unwrap();
        writer.write_all("(async () => {\n\n".as_bytes()).unwrap();
        writer.write_all(self.init.as_bytes()).unwrap();
        let wasm_fn = format!("await wasm.{wasm_fn}();\n");
        writer.write_all(wasm_fn.as_bytes()).unwrap();
        writer.write_all(self.dispatch_event.as_bytes()).unwrap();
        if !no_reload {
            if let Some(auto_reload) = &self.auto_reload {
                auto_reload.render(address, ws_base, writer);
            }
        }
        writer.write_all("\n\n})();\n".as_bytes()).unwrap();
    }
}

/// The init() call takes a string, when it should take an object with a key of `module_or_path`.
/// This stops wasm-bindgen from complaining via console.warn.
fn fix_init_line(input: &str) -> String {
    input
        .replace("init(", "init({module_or_path: ")
        .replace(");", "});\n")
}

/// Write a script file (either a shim or background script) to the staging directory.
fn write_script(script: Script, staging_dir: &str, script_template: &ScriptTemplate) {
    let js_path = Path::new(staging_dir).join(script.js);

    let mut js_file = File::create(js_path).unwrap();

    script_template.render(
        &script.wasm_fn,
        script.no_reload,
        script.background_script,
        &mut js_file,
    );
}

/// Write an HTML file to the staging directory.
fn write_html_page(
    page: HtmlPage,
    staging_dir: &str,
    script_template: &ScriptTemplate,
    html_template: &str,
) {
    let js_path = format!("{}_shim.js", page.html.replace(".", "_"));
    write_script(
        Script {
            js: js_path.clone(),
            no_reload: page.no_reload,
            background_script: false,
            wasm_fn: page.wasm_fn.clone(),
        },
        staging_dir,
        script_template,
    );

    let html_path = Path::new(staging_dir).join(&page.html);
    let mut html_file = File::create(html_path).unwrap();

    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                // Filter out preloads, since they're incompatible with webextensions.
                element!("link[rel=modulepreload], link[rel=preload]", |el| {
                    el.remove();
                    Ok(())
                }),
                // Filter out integrity attributes
                element!("[integrity]", |el| {
                    el.remove_attribute("integrity");
                    Ok(())
                }),
                // The script tag with a nonce is "our" tag. This is a pretty dumb
                // way to do this that can possibly break in numerous ways, but it's
                // good enough for the quick hack that this entire script is.
                element!("script[nonce]", |el| {
                    el.remove_attribute("nonce");
                    el.set_attribute("src", &format!("/{}", &js_path)).unwrap();
                    Ok(())
                }),
                // If data-wextrunk-include is set to page.name, keep the element.
                // Also make sure to not remove the tag if multiple `data-wextrunk-include`
                // attributes are set.
                element!("[data-wextrunk-include]", |el| {
                    let mut keep = false;
                    for element in el.attributes() {
                        if element.name() == "data-wextrunk-include" && element.value() == page.name
                        {
                            keep = true;
                        }
                    }
                    if !keep {
                        el.remove();
                    }
                    while el.has_attribute("data-wextrunk-include") {
                        el.remove_attribute("data-wextrunk-include");
                    }
                    Ok(())
                }),
            ],
            ..Settings::default()
        },
        |c: &[u8]| {
            html_file.write_all(c).unwrap();
        },
    );

    rewriter.write(html_template.as_bytes()).unwrap();
    rewriter.end().unwrap();
}

/// Write out the manifest file. If any post-processing occurred, it would happen here.
/// Perhaps it would be nice to have a manifest input that works for both Firefox and Chrome,
/// but for now, just copy from source to staging.
fn write_manifest(manifest: Manifest, source_dir: &str, staging_dir: &str) {
    let source_manifest_path = Path::new(source_dir).join(&manifest.href);
    let staging_manifest_path = Path::new(staging_dir).join("manifest.json");

    std::fs::copy(source_manifest_path, staging_manifest_path).unwrap();
}

fn main() {
    let start_time = Instant::now();
    let source_dir = env::var("TRUNK_SOURCE_DIR").unwrap();
    let staging_dir = env::var("TRUNK_STAGING_DIR").unwrap();
    let target = env::var("WEXTRUNK_TARGET").ok();
    let index_path = Path::new(&staging_dir).join("index.html");
    let CollectOutput {
        html_pages,
        scripts,
        manifest,
        html_template,
        script_contents,
    } = process_index_html(&index_path, target.as_deref());

    write_manifest(manifest, &source_dir, &staging_dir);

    let script_template = ScriptTemplate::new(&script_contents);

    for script in scripts {
        write_script(script, &staging_dir, &script_template);
    }

    for page in html_pages {
        write_html_page(page, &staging_dir, &script_template, &html_template);
    }

    fs::remove_file(index_path).unwrap();

    let duration = start_time.elapsed();
    println!("Wextrunk finished in {:?}", duration);
}
