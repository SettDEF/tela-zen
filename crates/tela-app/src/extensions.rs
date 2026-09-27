//! The extension host's runtime side.
//!
//! Performance shape matters here, because this is the piece most able to make
//! the browser feel slow:
//!
//! - Scripts are parsed and wrapped **once**, at load. Navigation only does
//!   match evaluation against precompiled patterns.
//! - Match evaluation happens in Rust, not JavaScript, so a page that matches
//!   nothing costs one function call and injects nothing at all — no shim, no
//!   guard, no empty script objects handed to WebEngine.
//! - `scripts_for_url` is called once per navigation, not per frame.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use tela_extension::{Manifest, Pattern, Severity, UserScript, userscript};

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(i32, count)]
        #[qproperty(i32, enabled_count)]
        /// Installed scripts as JSON, for the settings list.
        #[qproperty(QString, list_json)]
        #[qproperty(QString, scripts_dir)]
        #[qproperty(i32, bundle_count)]
        /// Installed extension bundles as JSON, with compatibility reports.
        #[qproperty(QString, bundles_json)]
        type Extensions = super::ExtensionsRust;

        /// Rescan the userscripts directory.
        #[qinvokable]
        fn reload(self: Pin<&mut Extensions>) -> bool;

        /// Import every `.user.js` from a directory. Returns how many parsed.
        #[qinvokable]
        fn import_from(self: Pin<&mut Extensions>, directory: &QString) -> i32;

        /// Import from the user's Downloads directory.
        #[qinvokable]
        fn import_from_downloads(self: Pin<&mut Extensions>) -> i32;

        /// Scripts that apply to `url`, as JSON, ready to hand to
        /// `WebEngineView.userScripts.collection`. Empty array when nothing
        /// matches — the common case, and it must stay cheap.
        #[qinvokable]
        fn scripts_for_url(self: Pin<&mut Extensions>, url: &QString) -> QString;

        #[qinvokable]
        fn set_enabled(self: Pin<&mut Extensions>, id: &QString, enabled: bool) -> bool;

        /// Unpack an .xpi or .crx into the extensions directory. Returns the
        /// bundle id, or an empty string on failure.
        #[qinvokable]
        fn install_bundle(self: Pin<&mut Extensions>, archive: &QString) -> QString;

        /// Scripts an extension page (popup, options) needs before its own run:
        /// the WebChannel client, the browser.* factory, and its identity.
        #[qinvokable]
        fn page_scripts(self: Pin<&mut Extensions>, ext: &QString) -> QString;
    }
}

/// A script plus everything precomputed at load time.
struct Loaded {
    script: UserScript,
    /// Wrapped source, built once rather than per navigation.
    wrapped: String,
    file: PathBuf,
    enabled: bool,
}

#[derive(Default)]
pub struct ExtensionsRust {
    count: i32,
    enabled_count: i32,
    list_json: QString,
    scripts_dir: QString,

    bundle_count: i32,
    bundles_json: QString,

    loaded: Vec<Loaded>,
    bundles: Vec<Bundle>,
    disabled: HashMap<String, bool>,
}

/// An installed extension bundle, with its content scripts pre-read.
struct Bundle {
    id: String,
    manifest: Manifest,
    /// Raw manifest.json, so extension pages can be handed getManifest() data
    /// without the parsed model having to carry every field.
    manifest_source: Option<String>,
    /// Absolute file:// URL of the best available icon, or "".
    icon: String,
    /// One entry per manifest content_scripts block: (matches, wrapped source,
    /// injection point). Sources are concatenated and read once at load.
    scripts: Vec<BundleScript>,
    enabled: bool,
}

struct BundleScript {
    matches: tela_extension::MatchSet,
    source: String,
    injection_point: &'static str,
    all_frames: bool,
}

fn bundles_dir() -> Option<PathBuf> {
    crate::config::config_dir().map(|dir| dir.join("extensions"))
}

fn scripts_dir() -> Option<PathBuf> {
    crate::config::config_dir().map(|dir| dir.join("userscripts"))
}

fn state_file() -> Option<PathBuf> {
    crate::config::config_dir().map(|dir| dir.join("userscripts/state.json"))
}

/// Where fetched `@require` libraries are cached, keyed by URL.
fn deps_dir() -> Option<PathBuf> {
    scripts_dir().map(|dir| dir.join(".deps"))
}

fn require_cache_path(url: &str) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    deps_dir().map(|dir| dir.join(format!("{:016x}.js", hasher.finish())))
}

/// Fetch text over HTTP with a short timeout. `None` on any failure, so a dead
/// CDN or being offline degrades to "no dependency" rather than blocking.
fn fetch_require(url: &str) -> Option<String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(6))
        .build();
    let body = agent.get(url).call().ok()?.into_string().ok()?;
    // Guard against a huge or clearly-wrong response; a require is a JS library.
    if body.len() > 8 * 1024 * 1024 {
        return None;
    }
    Some(body)
}

/// Minimal base64 (standard alphabet) so a fetched @resource can be handed to
/// the page as a data: URL without pulling in a crate for it.
fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(A[(n >> 18 & 63) as usize] as char);
        out.push(A[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { A[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { A[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn mime_for(url: &str) -> &'static str {
    let u = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    if u.ends_with(".json") { "application/json" }
    else if u.ends_with(".css") { "text/css" }
    else if u.ends_with(".js") { "text/javascript" }
    else if u.ends_with(".svg") { "image/svg+xml" }
    else if u.ends_with(".png") { "image/png" }
    else if u.ends_with(".jpg") || u.ends_with(".jpeg") { "image/jpeg" }
    else if u.ends_with(".webp") { "image/webp" }
    else if u.ends_with(".woff2") { "font/woff2" }
    else { "application/octet-stream" }
}

/// Fetch a `@resource` as raw bytes (needed because some are binary), cached.
fn fetch_resource_bytes(url: &str, cache: &std::path::Path) -> Option<Vec<u8>> {
    if let Ok(bytes) = std::fs::read(cache) {
        return Some(bytes);
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(6))
        .build();
    let resp = agent.get(url).call().ok()?;
    let mut buf = Vec::new();
    use std::io::Read;
    resp.into_reader().take(8 * 1024 * 1024).read_to_end(&mut buf).ok()?;
    let _ = std::fs::write(cache, &buf);
    Some(buf)
}

fn json_str(s: &str) -> String {
    serde_json::Value::String(s.to_owned()).to_string()
}

/// Build the JS that publishes a script's `@resource`s into `window.__telaRes`,
/// so its GM_getResourceText / GM_getResourceURL resolve. Each entry carries the
/// UTF-8 text (empty for binary) and a data: URL.
fn resolve_resources(script_id: &str, resources: &[(String, String)]) -> String {
    let Some(dir) = deps_dir() else {
        return String::new();
    };
    let _ = std::fs::create_dir_all(&dir);

    let mut entries = Vec::new();
    for (name, url) in resources {
        let Some(path) = require_cache_path(url) else {
            continue;
        };
        let Some(bytes) = fetch_resource_bytes(url, &path) else {
            continue;
        };
        let text = String::from_utf8(bytes.clone()).unwrap_or_default();
        let data_url = format!("data:{};base64,{}", mime_for(url), base64(&bytes));
        entries.push(format!(
            "{}:{{text:{},url:{}}}",
            json_str(name),
            json_str(&text),
            json_str(&data_url)
        ));
    }
    if entries.is_empty() {
        return String::new();
    }
    let id = json_str(script_id);
    format!(
        "window.__telaRes=(window.__telaRes||{{}});window.__telaRes[{id}]=Object.assign(window.__telaRes[{id}]||{{}},{{{}}});",
        entries.join(",")
    )
}

/// Concatenate a script's `@require` dependencies, fetching any not yet cached.
/// Cached hits are free; a first run pays one HTTP request per new URL, then
/// never again.
fn resolve_requires(requires: &[String]) -> String {
    let Some(dir) = deps_dir() else {
        return String::new();
    };
    let _ = std::fs::create_dir_all(&dir);

    let mut out = String::new();
    for url in requires {
        let Some(path) = require_cache_path(url) else {
            continue;
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(cached) => cached,
            Err(_) => match fetch_require(url) {
                Some(body) => {
                    let _ = std::fs::write(&path, &body);
                    body
                }
                // Leave it uncached and try again on a later load.
                None => continue,
            },
        };
        out.push_str(&content);
        out.push_str("\n;\n");
    }
    out
}

fn downloads_dir() -> Option<PathBuf> {
    // XDG_DOWNLOAD_DIR when set, otherwise the conventional location.
    if let Some(dir) = std::env::var_os("XDG_DOWNLOAD_DIR") {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Downloads"))
}

/// Resolve a path a manifest gave us against the bundle it came from.
///
/// Manifests routinely write these rooted — uBlock Origin declares its content
/// scripts as `/js/vapi.js` — and `Path::join` with an absolute path *discards
/// the base*, yielding `/js/vapi.js`. That file does not exist, the read fails,
/// the whole content-script block is skipped, and the extension runs with no
/// content scripts at all. uBlock loaded, showed its dashboard, answered
/// `browser.*` calls, and filtered exactly nothing, because the half of it that
/// does the filtering was never injected.
///
/// Traversal is refused rather than clamped: nothing outside the bundle is
/// ever a legitimate target, and a bundle is untrusted input.
fn asset_path(dir: &Path, relative: &str) -> Option<PathBuf> {
    let mut out = dir.to_path_buf();
    for part in Path::new(relative.trim_start_matches('/')).components() {
        match part {
            std::path::Component::Normal(name) => out.push(name),
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

/// The contents of a bundle-relative file, or None if it is missing or escapes.
fn asset_source(dir: &Path, relative: &str) -> Option<String> {
    asset_path(dir, relative).and_then(|path| std::fs::read_to_string(path).ok())
}

/// Whether `ext` declared a host permission covering `url`.
///
/// The host performs cross-origin requests for extensions, which is the one
/// privilege the web platform does not hand out. It has to be earned the same
/// way a real browser makes it earned: by the manifest asking for that host.
pub fn may_fetch(ext: &str, url: &str) -> bool {
    static HOSTS: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Vec<String>>>> =
        std::sync::OnceLock::new();
    let cache = HOSTS.get_or_init(|| std::sync::Mutex::new(HashMap::new()));

    let patterns = {
        let Ok(mut held) = cache.lock() else {
            return false;
        };
        if !held.contains_key(ext) {
            let declared = bundles_dir()
                .map(|dir| dir.join(ext))
                .and_then(|dir| std::fs::read_to_string(dir.join("manifest.json")).ok())
                .and_then(|source| Manifest::parse(&source))
                .map(|manifest| manifest.host_permissions)
                .unwrap_or_default();
            held.insert(ext.to_owned(), declared);
        }
        held.get(ext).cloned().unwrap_or_default()
    };

    patterns.iter().any(|pattern| {
        pattern == "<all_urls>"
            || Pattern::parse_match(pattern).is_some_and(|parsed| parsed.matches(url))
    })
}

/// The same, named by extension id rather than by directory — for callers that
/// hold an id, such as `tabs.executeScript`.
pub fn bundle_asset(ext: &str, relative: &str) -> Option<String> {
    // The id decides which directory is searched, so it must not be able to
    // name a different one.
    if ext.is_empty() || ext.contains('/') || ext.contains('.') {
        return None;
    }
    asset_source(&bundles_dir()?.join(ext), relative)
}

fn read_disabled() -> HashMap<String, bool> {
    let Some(path) = state_file() else {
        return HashMap::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return HashMap::new();
    };

    json["disabled"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str())
                .map(|id| (id.to_owned(), true))
                .collect()
        })
        .unwrap_or_default()
}

fn write_disabled(disabled: &HashMap<String, bool>) {
    let Some(path) = state_file() else {
        return;
    };
    let ids: Vec<&str> = disabled
        .iter()
        .filter(|(_, off)| **off)
        .map(|(id, _)| id.as_str())
        .collect();
    let json = serde_json::json!({ "disabled": ids });
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, serde_json::to_string_pretty(&json).unwrap_or_default());
}

impl ffi::Extensions {
    fn reload(mut self: Pin<&mut Self>) -> bool {
        let Some(dir) = scripts_dir() else {
            return false;
        };
        let _ = std::fs::create_dir_all(&dir);

        let disabled = read_disabled();
        let mut loaded = Vec::new();

        let entries = std::fs::read_dir(&dir).ok();
        if let Some(entries) = entries {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                let is_userscript = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.to_ascii_lowercase().ends_with(".user.js"))
                    .unwrap_or(false);
                if !is_userscript {
                    continue;
                }

                let Ok(source) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Some(mut script) = UserScript::parse(&source) else {
                    continue;
                };

                // Publish @resource assets (GM_getResourceText/URL), then inline
                // @require libraries ahead of the body, so a script that depends
                // on either (SearchJumper, Ophel Atlas, …) is not left calling an
                // undefined global or reading an empty resource. Fetched once and
                // cached on disk.
                let script_id = script.id();
                if !script.resources.is_empty() {
                    let res = resolve_resources(&script_id, &script.resources);
                    if !res.is_empty() {
                        script.source = format!("{res}\n{}", script.source);
                    }
                }
                if !script.requires.is_empty() {
                    let deps = resolve_requires(&script.requires);
                    if !deps.is_empty() {
                        script.source = format!("{deps}\n;\n{}", script.source);
                    }
                }

                let id = script.id();
                loaded.push(Loaded {
                    wrapped: userscript::wrap(&script),
                    enabled: !disabled.get(&id).copied().unwrap_or(false),
                    script,
                    file: path,
                });
            }
        }

        loaded.sort_by(|a, b| a.script.name.to_lowercase().cmp(&b.script.name.to_lowercase()));

        let bundles = load_bundles(&disabled);
        let bundle_list: Vec<serde_json::Value> = bundles
            .iter()
            .map(|bundle| {
                let issues: Vec<serde_json::Value> = bundle
                    .manifest
                    .compatibility()
                    .iter()
                    .map(|issue| {
                        serde_json::json!({
                            "feature": issue.feature,
                            "blocking": issue.severity == Severity::Blocking,
                            "detail": issue.detail,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "id": bundle.id,
                    "name": bundle.manifest.name,
                    "version": bundle.manifest.version,
                    "description": bundle.manifest.description,
                    "manifestVersion": bundle.manifest.manifest_version,
                    "enabled": bundle.enabled,
                    "contentScripts": bundle.scripts.len(),
                    "hasBackground": !bundle.manifest.background.is_empty(),
                    "loadable": bundle.manifest.is_loadable(),
                    "issues": issues,
                    // A file:// URL, because QML's Image loads through Qt's
                    // network stack and never sees the tela-extension scheme
                    // handler, which only exists inside WebEngine.
                    "icon": bundle.icon.clone(),
                    // The extension's own action popup, if it declares one.
                    "popup": bundle.manifest.action.as_ref()
                        .and_then(|a| a.default_popup.clone())
                        .unwrap_or_default(),
                    "optionsPage": bundle.manifest.options_page.clone().unwrap_or_default(),
                    "hostAccess": host_access_summary(&bundle.manifest),
                    // The APIs actually granted, so the Options list states
                    // what is enforced rather than implying limits that are not.
                    "apiAccess": api_access_summary(&bundle.manifest),
                })
            })
            .collect();

        let bundles_encoded =
            serde_json::to_string(&bundle_list).unwrap_or_else(|_| "[]".to_owned());

        let list: Vec<serde_json::Value> = loaded
            .iter()
            .map(|item| {
                serde_json::json!({
                    "id": item.script.id(),
                    "name": item.script.name,
                    "version": item.script.version,
                    "description": item.script.description,
                    "enabled": item.enabled,
                    "grants": item.script.grants,
                    "file": item.file.to_string_lossy(),
                })
            })
            .collect();

        let enabled_count = loaded.iter().filter(|item| item.enabled).count() as i32;
        let encoded = serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_owned());
        let bundle_total = bundles.len() as i32;
        let script_total = loaded.len() as i32;

        // Commit state *before* emitting any property change. A listener that
        // reacts to bundlesJson and immediately calls scripts_for_url would
        // otherwise be answered from the previous, empty bundle list.
        {
            let mut rust = self.as_mut().rust_mut();
            rust.loaded = loaded;
            rust.bundles = bundles;
            rust.disabled = disabled;
        }

        self.as_mut().set_count(script_total);
        self.as_mut().set_enabled_count(enabled_count);
        self.as_mut().set_list_json(QString::from(&encoded));
        self.as_mut()
            .set_scripts_dir(QString::from(&dir.to_string_lossy().into_owned()));
        self.as_mut().set_bundle_count(bundle_total);
        self.as_mut().set_bundles_json(QString::from(&bundles_encoded));
        true
    }

    fn page_scripts(self: Pin<&mut Self>, ext: &QString) -> QString {
        let ext = ext.to_string();
        let rust = self.rust();
        let Some(bundle) = rust.bundles.iter().find(|b| b.id == ext) else {
            return QString::from("[]");
        };

        let transport = std::fs::read_to_string(QWEBCHANNEL_JS).unwrap_or_default();
        let manifest = bundle
            .manifest_source
            .clone()
            .unwrap_or_else(|| "{}".to_owned());

        // Identity first, then the transport, then the factory — the same
        // order the generated background page uses.
        let messages = bundle
            .manifest
            .default_locale
            .as_ref()
            .and_then(|locale| {
                let dir = bundles_dir()?;
                std::fs::read_to_string(
                    dir.join(&ext).join("_locales").join(locale).join("messages.json"),
                )
                .ok()
            })
            .unwrap_or_else(|| "{}".to_owned());

        let grants = self
            .rust()
            .bundles
            .iter()
            .find(|bundle| bundle.id == ext)
            .map(|bundle| serde_json::to_string(&bundle.manifest.permissions).unwrap_or_default())
            .unwrap_or_else(|| "[]".to_owned());

        let preamble = format!(
            "window.__telaExtensionId={};\nwindow.__telaManifest={};\n\
             window.__telaMessages={};\nwindow.__telaGrants={};\n",
            serde_json::Value::String(ext.clone()),
            manifest,
            messages,
            grants
        );

        let payload = serde_json::json!([{
            "name": "tela:page-shim",
            "source": format!("{}\n{}\n{}", preamble, transport, crate::extapi::BROWSER_SHIM),
        }]);

        QString::from(&serde_json::to_string(&payload).unwrap_or_else(|_| "[]".to_owned()))
    }

    fn install_bundle(self: Pin<&mut Self>, archive: &QString) -> QString {
        let Some(root) = bundles_dir() else {
            return QString::from("");
        };
        let path = PathBuf::from(archive.to_string());

        match tela_extension::install_bundle(&path, &root) {
            Ok(bundle) => {
                let id = tela_extension::bundle_id(&bundle.manifest);
                write_runtime_files(&bundle.root, &id, &bundle.manifest);
                self.reload();
                QString::from(&id)
            }
            Err(_) => QString::from(""),
        }
    }

    fn import_from(self: Pin<&mut Self>, directory: &QString) -> i32 {
        let Some(target) = scripts_dir() else {
            return 0;
        };
        let source = PathBuf::from(directory.to_string());
        let imported = tela_extension::import_userscripts(&source, &target);
        let count = imported.len() as i32;
        if count > 0 {
            self.reload();
        }
        count
    }

    fn import_from_downloads(self: Pin<&mut Self>) -> i32 {
        let Some(downloads) = downloads_dir() else {
            return 0;
        };
        self.import_from(&QString::from(&downloads.to_string_lossy().into_owned()))
    }

    fn scripts_for_url(self: Pin<&mut Self>, url: &QString) -> QString {
        let url = url.to_string();

        // Cheap rejection first: extension and blank pages never get scripts.
        if !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("file://")
        {
            return QString::from("[]");
        }

        let rust = self.rust();
        let mut out: Vec<serde_json::Value> = Vec::new();
        let mut needs_shim = false;
        let mut needs_channel = false;

        for item in &rust.loaded {
            if !item.enabled || !item.script.matches_url(&url) {
                continue;
            }
            if !item.script.grants.is_empty() {
                needs_shim = true;
            }
            out.push(serde_json::json!({
                "name": item.script.name,
                "source": item.wrapped,
                "injectionPoint": item.script.run_at.injection_point(),
            }));
        }

        for bundle in &rust.bundles {
            if !bundle.enabled {
                continue;
            }
            for script in &bundle.scripts {
                if !script.matches.matches(&url) {
                    continue;
                }
                needs_channel = true;
                out.push(serde_json::json!({
                    "name": format!("{}:{}", bundle.id, out.len()),
                    "source": script.source,
                    "injectionPoint": script.injection_point,
                    "allFrames": script.all_frames,
                }));
            }
        }

        if out.is_empty() {
            return QString::from("[]");
        }

        // Extension content scripts need the WebChannel client and the
        // browser.* factory in place before they run. Injected only when a
        // bundle actually matched, so ordinary pages pay nothing.
        if needs_channel {
            let transport = std::fs::read_to_string(QWEBCHANNEL_JS).unwrap_or_default();
            out.insert(
                0,
                serde_json::json!({
                    "name": "tela:webchannel",
                    "source": format!("{}\n{}", transport, crate::extapi::BROWSER_SHIM),
                    "injectionPoint": "DocumentCreation",
                }),
            );
        }

        // The GM shim only rides along when something actually granted an API.
        if needs_shim {
            out.insert(
                0,
                serde_json::json!({
                    "name": "tela:gm-shim",
                    "source": userscript::GM_SHIM,
                    "injectionPoint": "DocumentCreation",
                }),
            );
        }

        QString::from(&serde_json::to_string(&out).unwrap_or_else(|_| "[]".to_owned()))
    }

    fn set_enabled(mut self: Pin<&mut Self>, id: &QString, enabled: bool) -> bool {
        let id = id.to_string();
        {
            let mut rust = self.as_mut().rust_mut();
            if enabled {
                rust.disabled.remove(&id);
            } else {
                rust.disabled.insert(id, true);
            }
            write_disabled(&rust.disabled);
        }
        self.reload()
    }
}

/// Read installed bundles from disk, pre-reading and concatenating each
/// content-script block so navigation only has to match patterns.
fn load_bundles(disabled: &HashMap<String, bool>) -> Vec<Bundle> {
    let Some(root) = bundles_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };

    let mut bundles = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(dir.join("manifest.json")) else {
            continue;
        };
        let Some(mut manifest) = Manifest::parse(&source) else {
            continue;
        };
        // Localised names live in _locales; without this the UI shows raw
        // __MSG_extName__ placeholders.
        manifest.resolve_messages(&dir);

        let id = entry.file_name().to_string_lossy().into_owned();

        let mut scripts = Vec::new();
        for block in &manifest.content_scripts {
            // CSS first, injected as a style element, then the JS bodies. One
            // concatenated source per block keeps injection to a single script
            // object rather than one per file.
            let mut parts = Vec::new();
            for css in &block.css {
                match asset_source(&dir, css) {
                    Some(text) => parts.push(format!(
                        "(function(){{const s=document.createElement('style');\
                         s.textContent={};\
                         (document.head||document.documentElement).appendChild(s);}})();",
                        serde_json::Value::String(text)
                    )),
                    None => crate::trace!("script", "content_css", "{id}: missing {css}"),
                }
            }
            for js in &block.js {
                match asset_source(&dir, js) {
                    Some(text) => parts.push(text),
                    None => crate::trace!("script", "content_js", "{id}: missing {js}"),
                }
            }
            if parts.is_empty() {
                // Silence here is what hid the bug below for so long: a block
                // whose files all failed to load looks exactly like a block
                // that matched nothing.
                crate::fixme!(
                    "script",
                    "content_scripts",
                    "{id}: block loaded no source, injecting nothing"
                );
                continue;
            }

            scripts.push(BundleScript {
                matches: block.matches.clone(),
                // Each script closes over its own `browser`/`chrome`, so two
                // extensions sharing a page cannot be mistaken for each other.
                // Fail closed. If the API factory is not present the script
                // must not run at all: an extension that runs *without* its
                // API throws partway through, and page-restyling extensions
                // leave the page visually broken when they do.
                source: format!(
                    "(function(){{\n\
                     if(!window.__telaMakeApi){{console.warn({} + ' skipped: API unavailable');return;}}\n\
                     const browser=window.__telaMakeApi({});\n\
                     const chrome=browser;\n\
                     window.browser=browser;window.chrome=browser;\n\
                     try{{\n{}\n}}catch(e){{console.error({},e);}}}})();",
                    serde_json::Value::String(id.clone()),
                    serde_json::Value::String(id.clone()),
                    parts.join("\n"),
                    serde_json::Value::String(format!("[{id}] content script failed"))
                ),
                injection_point: block.run_at.injection_point(),
                all_frames: block.all_frames,
            });

            // Applied after the shim wrapper so the guard is outermost — the
            // wait has to happen before any of the extension's code runs.
            if let Some(last) = scripts.last_mut() {
                if last.injection_point == "DocumentCreation" {
                    last.source = wrap_document_start(&last.source);
                }
            }
        }

        // Regenerate on every load, not only at install. These files embed the
        // shim and the injected prelude, so an install made before a change to
        // either would otherwise keep running the old code indefinitely — the
        // extension looks installed and current while behaving like an old
        // build, which is a genuinely hard failure to spot.
        write_runtime_files(&dir, &id, &manifest);

        let enabled = !disabled.get(&id).copied().unwrap_or(false) && manifest.is_loadable();
        let icon = find_icon(&dir, &source);
        bundles.push(Bundle {
            id,
            manifest,
            manifest_source: Some(source),
            icon,
            scripts,
            enabled,
        });
    }

    bundles.sort_by(|a, b| a.manifest.name.to_lowercase().cmp(&b.manifest.name.to_lowercase()));
    bundles
}

/// Qt ships the client half of the WebChannel transport; extensions need it
/// served from their own origin, so it is copied in at install time.
const QWEBCHANNEL_JS: &str = "/usr/share/qt6/webchannel/qwebchannel.js";

/// Write the generated background page and its supporting scripts.
///
/// MV2 declares `background.scripts` with no page; MV3 declares a service
/// worker. Neither is something WebEngine can load directly, so a host page is
/// generated that pulls in the transport, the `browser.*` shim, and then the
/// extension's own scripts in declared order.
/// Globals every page of an extension needs before its own scripts run.
///
/// Written for all bundles, not just those with a background script: an
/// extension's dashboard and options pages are ordinary documents, and the
/// scheme handler injects this into each of them. uBlock declares
/// `background: { page }` rather than scripts, so gating this on scripts left
/// exactly the page people want to open without a working browser.*.
fn write_prelude(root: &std::path::Path, id: &str) {
    let manifest_json = std::fs::read_to_string(root.join("manifest.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    let grants: Vec<&str> = manifest_json["permissions"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .filter(|value| *value != "<all_urls>" && !value.contains("://"))
                .collect()
        })
        .unwrap_or_default();

    // Same source the background prelude uses: the default locale's strings.
    let messages = std::fs::read_to_string(root.join("manifest.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|m| m["default_locale"].as_str().map(str::to_owned))
        .and_then(|locale| {
            std::fs::read_to_string(root.join("_locales").join(locale).join("messages.json")).ok()
        })
        .unwrap_or_else(|| "{}".to_owned());

    let prelude = format!(
        "window.__telaExtensionId={};\nwindow.__telaManifest={};\n\
         window.__telaMessages={};\nwindow.__telaGrants={};\n",
        serde_json::Value::String(id.to_owned()),
        manifest_json,
        messages,
        serde_json::to_string(&grants).unwrap_or_else(|_| "[]".to_owned()),
    );
    let _ = std::fs::write(root.join("_tela/prelude.js"), prelude);
}

fn write_runtime_files(root: &std::path::Path, id: &str, manifest: &Manifest) {
    let support = root.join("_tela");
    if std::fs::create_dir_all(&support).is_err() {
        return;
    }

    // The transport client. Copied rather than linked so the page loads it from
    // its own origin and the scheme handler's rules apply uniformly.
    if let Ok(source) = std::fs::read_to_string(QWEBCHANNEL_JS) {
        let _ = std::fs::write(support.join("qwebchannel.js"), source);
    }
    let _ = std::fs::write(support.join("shim.js"), crate::extapi::BROWSER_SHIM);
    write_prelude(root, id);

    // Locale strings, so runtime i18n lookups resolve without host round-trips.
    let messages = manifest
        .default_locale
        .as_ref()
        .and_then(|locale| {
            std::fs::read_to_string(root.join("_locales").join(locale).join("messages.json")).ok()
        })
        .unwrap_or_else(|| "{}".to_owned());

    let mut scripts = manifest.background.scripts.clone();
    if let Some(worker) = &manifest.background.service_worker {
        // An MV3 worker runs as an ordinary script here. Lifecycle differs and
        // the compatibility report says so.
        scripts.push(worker.clone());
    }
    if scripts.is_empty() {
        return;
    }

    // MV2 permits background.type == "module"; those scripts use `import` and
    // are a syntax error if loaded as classic scripts.
    let module_type = std::fs::read_to_string(root.join("manifest.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .map(|json| json["background"]["type"] == "module")
        .unwrap_or(false);
    let script_attr = if module_type { " type=\"module\"" } else { "" };

    let tags: String = scripts
        .iter()
        .map(|src| {
            format!(
                "<script{}  src=\"{}\"></script>",
                script_attr,
                src.replace('"', "&quot;")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Extensions read real fields off getManifest() — action popups, icons,
    // version strings. A summary object makes them fail on a missing property,
    // so the whole file goes in verbatim.
    let manifest_json = std::fs::read_to_string(root.join("manifest.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    // Same split the Manifest parser performs: MV2 mixes host match patterns
    // into `permissions`, and those are not API grants.
    let grants: Vec<&str> = manifest_json["permissions"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .filter(|value| *value != "<all_urls>" && !value.contains("://"))
                .collect()
        })
        .unwrap_or_default();

    let page = format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\">\n\
         <script>window.__telaExtensionId={};\n\
         window.__telaManifest={};\n\
         window.__telaMessages={};\n\
         window.__telaGrants={};</script>\n\
         <script src=\"_tela/qwebchannel.js\"></script>\n\
         <script src=\"_tela/shim.js\"></script>\n\
         </head><body>\n{}\n</body></html>\n",
        serde_json::Value::String(id.to_owned()),
        manifest_json,
        messages,
        serde_json::to_string(&grants).unwrap_or_else(|_| "[]".to_owned()),
        tags
    );

    let _ = std::fs::write(root.join("_tela_background.html"), page);
}

/// Pick the largest declared icon that actually exists on disk.
///
/// `Manifest` does not model icons — nothing in the engine needs them — so this
/// reads the raw JSON rather than widening the parsed type for a UI detail.
fn find_icon(dir: &std::path::Path, manifest_source: &str) -> String {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(manifest_source) else {
        return String::new();
    };

    let mut candidates: Vec<(u64, String)> = Vec::new();
    for key in ["icons", "browser_action", "action"] {
        let node = if key == "icons" {
            &json["icons"]
        } else {
            &json[key]["default_icon"]
        };

        match node {
            serde_json::Value::Object(map) => {
                for (size, path) in map {
                    if let Some(path) = path.as_str() {
                        candidates.push((size.parse().unwrap_or(0), path.to_owned()));
                    }
                }
            }
            // default_icon may be a bare string.
            serde_json::Value::String(path) => candidates.push((0, path.clone())),
            _ => {}
        }
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, relative) in candidates {
        let path = dir.join(relative.trim_start_matches('/'));
        if path.is_file() {
            return format!("file://{}", path.to_string_lossy());
        }
    }
    String::new()
}

/// Wrap a `document_start` script so it matches WebExtensions semantics.
///
/// Chromium's DocumentCreation fires *earlier* than `document_start` does in a
/// real browser: there, the parser has already produced `documentElement` and
/// `head`, so a script may touch them immediately. Injecting before that exists
/// makes `document.head.appendChild(...)` throw on null — which is exactly how
/// theming extensions fail, since inserting a stylesheet is the first thing
/// they do.
///
/// Waiting for `head` rather than deferring to DocumentReady preserves the
/// point of `document_start`: the script still runs before the page's own
/// scripts, just not before the DOM it is written against exists.
fn wrap_document_start(source: &str) -> String {
    format!(
        r#"(function(){{
var __run = function(){{
{source}
}};
if (document.head) {{ __run(); }}
else {{
  var __observer = new MutationObserver(function(_, obs){{
    if (document.head) {{ obs.disconnect(); __run(); }}
  }});
  __observer.observe(document, {{ childList: true, subtree: true }});
}}
}})();"#
    )
}

/// One line describing what the extension can reach, in the terms a person
/// cares about rather than raw match patterns.
/// The permission-gated APIs this extension may use, in the order declared.
///
/// Only namespaces the shim actually gates are listed — naming a permission the
/// browser does not enforce would restore exactly the misleading impression
/// this is meant to remove.
fn api_access_summary(manifest: &Manifest) -> String {
    const GATED: [&str; 10] = [
        "storage", "cookies", "webRequest", "webNavigation", "declarativeNetRequest",
        "downloads", "scripting", "idle", "privacy", "dns",
    ];
    let granted: Vec<&str> = manifest
        .permissions
        .iter()
        .map(String::as_str)
        .filter(|value| GATED.contains(value))
        .collect();

    if granted.is_empty() {
        return "No privileged APIs".to_owned();
    }
    granted.join(", ")
}

fn host_access_summary(manifest: &Manifest) -> String {
    let hosts = &manifest.host_permissions;
    if hosts.iter().any(|h| h == "<all_urls>" || h == "*://*/*") {
        return "Can read and change data on all sites".to_owned();
    }
    if hosts.is_empty() {
        if manifest.content_scripts.is_empty() {
            return "No site access".to_owned();
        }
        return "Runs on some sites".to_owned();
    }
    if hosts.len() == 1 {
        return format!("Can read and change data on {}", hosts[0]);
    }
    format!("Can read and change data on {} sites", hosts.len())
}
