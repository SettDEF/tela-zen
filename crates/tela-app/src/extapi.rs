//! The host side of the `browser.*` API.
//!
//! Extension JavaScript never reaches Rust directly. It calls a shim, the shim
//! marshals over QWebChannel, and lands here. Every method is explicit — there
//! is no generic "invoke" entry point, because that would be exactly the
//! page-reachable command bridge this project refuses to build.
//!
//! State lives per extension under `~/.config/tela/extensions/<id>/storage.json`,
//! so one extension can never read another's data: the id is supplied by the
//! host when it creates the channel, not by the calling script.

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

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
        /// Set by QML to the tab list, as JSON, so `tabs.query` has something
        /// to answer from without this object reaching into the UI.
        #[qproperty(QString, tabs_json)]
        type ExtensionApi = super::ExtensionApiRust;

        /// `browser.storage.local.get` — `keys` is null for "everything",
        /// otherwise a JSON array of key names.
        #[qinvokable]
        fn storage_get(self: Pin<&mut ExtensionApi>, ext: &QString, keys: &QString) -> QString;

        /// `browser.storage.local.set` — `items` is a JSON object.
        #[qinvokable]
        fn storage_set(self: Pin<&mut ExtensionApi>, ext: &QString, items: &QString) -> bool;

        /// `browser.storage.local.remove` — `keys` is a JSON array.
        #[qinvokable]
        fn storage_remove(self: Pin<&mut ExtensionApi>, ext: &QString, keys: &QString) -> bool;

        /// `browser.storage.local.clear`.
        #[qinvokable]
        fn storage_clear(self: Pin<&mut ExtensionApi>, ext: &QString) -> bool;

        /// `browser.tabs.query`. Only the fields a content script is allowed to
        /// see; no favicon data, no session state.
        #[qinvokable]
        fn tabs_query(self: Pin<&mut ExtensionApi>, query: &QString) -> QString;

        /// `browser.runtime.sendMessage` from a content script to its
        /// background context, and `browser.tabs.sendMessage` in reverse.
        /// Delivery is a signal so QML can route it to the right view.
        #[qinvokable]
        fn post_message(self: Pin<&mut ExtensionApi>, ext: &QString, target: &QString, payload: &QString);

        /// Emitted for QML to deliver. `target` is "background" or "tabs".
        #[qsignal]
        fn message_posted(self: Pin<&mut ExtensionApi>, ext: QString, target: QString, payload: QString);

        /// A reply travelling back to whoever called sendMessage.
        #[qinvokable]
        fn post_response(self: Pin<&mut ExtensionApi>, ext: &QString, request: &QString, payload: &QString);

        #[qsignal]
        fn response_posted(self: Pin<&mut ExtensionApi>, ext: QString, request: QString, payload: QString);

        /// Regenerate the desktop palette from a seed colour.
        ///
        /// Gated three ways: the feature must be enabled in config, the caller
        /// must be the allowlisted extension, and the seed must parse as
        /// #RRGGBB. Returns a human-readable outcome rather than a bare bool so
        /// the UI can say *why* nothing happened.
        #[qinvokable]
        fn apply_system_theme(self: Pin<&mut ExtensionApi>, ext: &QString, seed: &QString) -> QString;

        /// `browser.tabs.executeScript`. `injection` is the WebExtensions
        /// object: `{ file }` or `{ code }`, plus `allFrames`.
        ///
        /// A `file` is resolved against the calling extension's own bundle and
        /// never anywhere else — the id comes from the host, so a script cannot
        /// name another extension's files. Returns false when there was nothing
        /// to run.
        #[qinvokable]
        fn execute_script(
            self: Pin<&mut ExtensionApi>,
            ext: &QString,
            tab_id: i32,
            injection: &QString,
        ) -> bool;

        /// Source bound for one tab's isolated world. QML owns the views, so it
        /// does the running.
        #[qsignal]
        fn script_requested(
            self: Pin<&mut ExtensionApi>,
            ext: QString,
            tab_id: i32,
            source: QString,
        );

        /// `browser.tabs.insertCSS`. This is how element hiding actually
        /// arrives: uBlock computes a page's cosmetic filters, then pushes them
        /// as a stylesheet through here. While this did nothing, uBlock could
        /// run perfectly and still hide not one element.
        #[qinvokable]
        fn insert_css(
            self: Pin<&mut ExtensionApi>,
            ext: &QString,
            tab_id: i32,
            injection: &QString,
        ) -> bool;

        /// `browser.tabs.removeCSS`. Without it, hiding applied for one page
        /// would outlive the filters that justified it.
        #[qinvokable]
        fn remove_css(
            self: Pin<&mut ExtensionApi>,
            ext: &QString,
            tab_id: i32,
            injection: &QString,
        ) -> bool;

        /// A stylesheet to add to, or drop from, one tab.
        #[qsignal]
        fn css_requested(
            self: Pin<&mut ExtensionApi>,
            ext: QString,
            tab_id: i32,
            css: QString,
            remove: bool,
        );

        /// `browser.tabs.create`. A relative URL is resolved against the
        /// calling extension, which is how one opens its own dashboard —
        /// uBlock's popup buttons are all this call.
        #[qinvokable]
        fn create_tab(self: Pin<&mut ExtensionApi>, ext: &QString, details: &QString) -> bool;

        #[qsignal]
        fn tab_open_requested(self: Pin<&mut ExtensionApi>, ext: QString, url: QString);

        /// `browser.tabs.update` — navigate a tab, select it, or both.
        /// `props` is the WebExtensions object: `{ url, active }`.
        #[qinvokable]
        fn update_tab(
            self: Pin<&mut ExtensionApi>,
            ext: &QString,
            tab_id: i32,
            props: &QString,
        ) -> bool;

        #[qsignal]
        fn tab_update_requested(
            self: Pin<&mut ExtensionApi>,
            tab_id: i32,
            url: QString,
            select: bool,
        );

        /// `browser.tabs.remove`. Takes one id; the shim unrolls an array.
        #[qinvokable]
        fn remove_tab(self: Pin<&mut ExtensionApi>, ext: &QString, tab_id: i32) -> bool;

        #[qsignal]
        fn tab_close_requested(self: Pin<&mut ExtensionApi>, tab_id: i32);

        /// `browser.tabs.reload`.
        #[qinvokable]
        fn reload_tab(self: Pin<&mut ExtensionApi>, ext: &QString, tab_id: i32) -> bool;

        #[qsignal]
        fn tab_reload_requested(self: Pin<&mut ExtensionApi>, tab_id: i32);

        /// A cross-origin request on an extension's behalf.
        ///
        /// Extension pages are an origin like any other here, so the web's CORS
        /// rules apply to them — and they should not. A real browser exempts
        /// extension requests under their host permissions, which is why uBlock
        /// can fetch filter lists at all. `spec` is `{url, method, headers,
        /// body}`; the reply comes back through `post_response`.
        #[qinvokable]
        fn fetch_url(
            self: Pin<&mut ExtensionApi>,
            ext: &QString,
            request_id: &QString,
            spec: &QString,
        ) -> bool;

        #[qsignal]
        fn fetch_requested(
            self: Pin<&mut ExtensionApi>,
            ext: QString,
            request_id: QString,
            spec: QString,
        );

        /// Whether anything on this `TELA_DEBUG` channel prints. Asked before
        /// building a log line, so a disabled channel costs approximately
        /// nothing.
        #[qinvokable]
        fn debug_enabled(self: Pin<&mut ExtensionApi>, channel: &QString) -> bool;

        /// Emit one line on a channel. `level` is err/fixme/warn/trace;
        /// anything else is treated as trace. `subject` is what the line is
        /// about — an extension id, a tab — so a log can be narrowed to one
        /// actor; pass "" when there isn't one.
        #[qinvokable]
        fn debug_log(
            self: Pin<&mut ExtensionApi>,
            level: &QString,
            channel: &QString,
            subject: &QString,
            func: &QString,
            message: &QString,
        );
    }
}

#[derive(Default)]
pub struct ExtensionApiRust {
    tabs_json: QString,
    /// Loaded storage per extension, written through on change.
    storage: HashMap<String, serde_json::Map<String, serde_json::Value>>,
}

fn storage_path(ext: &str) -> Option<PathBuf> {
    // The id is host-supplied, but defend anyway: a traversal here would let
    // one extension read another's file, or anything else on disk.
    if ext.is_empty() || ext.contains('/') || ext.contains('.') {
        return None;
    }
    crate::config::config_dir().map(|dir| dir.join("extensions").join(ext).join("storage.json"))
}

/// Drop state that exists only because this shim once aliased
/// `storage.managed` to `storage.local`.
///
/// uBlock reads managed storage at startup and caches the result back into
/// local under `cachedManagedStorage`. While the two were the same area, that
/// cached the entire store inside itself once per run, squaring its size — a
/// profile here reached 143 MB, which every later write rewrote in full. The
/// key holds administrator policy, of which there is none, so dropping it loses
/// nothing; the next run writes it back correctly and empty.
///
/// Returns true when something was dropped and the store needs rewriting.
fn heal(map: &mut serde_json::Map<String, serde_json::Value>) -> bool {
    map.remove("cachedManagedStorage").is_some()
}

impl ExtensionApiRust {
    fn load(&mut self, ext: &str) -> &mut serde_json::Map<String, serde_json::Value> {
        if !self.storage.contains_key(ext) {
            let mut loaded = storage_path(ext)
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default();
            let healed = heal(&mut loaded);
            self.storage.insert(ext.to_owned(), loaded);
            // Rewrite immediately rather than waiting for the extension's next
            // write, so the cost of reading the bloated file is paid once ever.
            if healed {
                self.persist(ext);
            }
        }
        self.storage.get_mut(ext).expect("just inserted")
    }

    fn persist(&self, ext: &str) {
        let Some(path) = storage_path(ext) else {
            return;
        };
        let Some(map) = self.storage.get(ext) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Temp file plus rename: a crash mid-write must not truncate the store.
        let temp = path.with_extension("json.tmp");
        let encoded = serde_json::to_string_pretty(map).unwrap_or_else(|_| "{}".to_owned());
        if std::fs::write(&temp, encoded).is_ok() && std::fs::rename(&temp, &path).is_err() {
            let _ = std::fs::remove_file(&temp);
        }
    }
}

impl ffi::ExtensionApi {
    fn execute_script(
        mut self: Pin<&mut Self>,
        ext: &QString,
        tab_id: i32,
        injection: &QString,
    ) -> bool {
        let ext = ext.to_string();
        let Ok(request) = serde_json::from_str::<serde_json::Value>(&injection.to_string()) else {
            return false;
        };

        let source = if let Some(code) = request["code"].as_str() {
            Some(code.to_owned())
        } else if let Some(file) = request["file"].as_str() {
            // Resolved against this extension's own bundle, and only that one:
            // the id is supplied by the host when the channel is created, not
            // by the calling script.
            crate::extensions::bundle_asset(&ext, file)
        } else {
            None
        };

        let Some(source) = source else {
            crate::fixme!(
                "script",
                "executeScript",
                "{ext}: nothing to run for {}",
                injection.to_string()
            );
            return false;
        };

        crate::trace!(
            "script",
            "executeScript",
            "{ext}: {} bytes into tab {tab_id}",
            source.len()
        );
        self.as_mut()
            .script_requested(QString::from(&ext), tab_id, QString::from(&source));
        true
    }

    fn insert_css(self: Pin<&mut Self>, ext: &QString, tab_id: i32, injection: &QString) -> bool {
        self.apply_css(ext, tab_id, injection, false)
    }

    fn remove_css(self: Pin<&mut Self>, ext: &QString, tab_id: i32, injection: &QString) -> bool {
        self.apply_css(ext, tab_id, injection, true)
    }

    /// Shared by insert and remove: both take the same injection object and
    /// differ only in what QML then does with the stylesheet.
    fn apply_css(
        mut self: Pin<&mut Self>,
        ext: &QString,
        tab_id: i32,
        injection: &QString,
        remove: bool,
    ) -> bool {
        let ext = ext.to_string();
        let Ok(request) = serde_json::from_str::<serde_json::Value>(&injection.to_string()) else {
            return false;
        };

        let css = if let Some(code) = request["code"].as_str() {
            Some(code.to_owned())
        } else if let Some(file) = request["file"].as_str() {
            crate::extensions::bundle_asset(&ext, file)
        } else {
            None
        };

        let Some(css) = css else {
            crate::fixme!("script", "insertCSS", "{ext}: no stylesheet in the request");
            return false;
        };

        crate::trace!(
            "script",
            if remove { "removeCSS" } else { "insertCSS" },
            "{ext}: {} bytes, tab {tab_id}",
            css.len()
        );
        self.as_mut()
            .css_requested(QString::from(&ext), tab_id, QString::from(&css), remove);
        true
    }

    fn create_tab(mut self: Pin<&mut Self>, ext: &QString, details: &QString) -> bool {
        let ext = ext.to_string();
        let Ok(request) = serde_json::from_str::<serde_json::Value>(&details.to_string()) else {
            return false;
        };
        let Some(url) = request["url"].as_str() else {
            crate::fixme!("script", "createTab", "{ext}: no url in the request");
            return false;
        };

        // An extension naming one of its own pages writes it relative —
        // `dashboard.html`, `logger-ui.html`. Only a URL with a scheme is
        // already absolute.
        let resolved = if url.contains("://") {
            url.to_owned()
        } else {
            format!("tela-extension://{ext}/{}", url.trim_start_matches('/'))
        };

        crate::trace!("script", "createTab", "{ext}: {resolved}");
        self.as_mut()
            .tab_open_requested(QString::from(&ext), QString::from(&resolved));
        true
    }

    fn update_tab(
        mut self: Pin<&mut Self>,
        ext: &QString,
        tab_id: i32,
        props: &QString,
    ) -> bool {
        let ext = ext.to_string();
        let Ok(request) = serde_json::from_str::<serde_json::Value>(&props.to_string()) else {
            return false;
        };

        // A relative url means one of the extension's own pages, same as
        // `tabs.create`.
        let url = request["url"].as_str().map(|url| {
            if url.contains("://") {
                url.to_owned()
            } else {
                format!("tela-extension://{ext}/{}", url.trim_start_matches('/'))
            }
        });
        // Absent means "leave selection alone", which is not the same as false.
        let select = request["active"].as_bool().unwrap_or(false);

        if url.is_none() && !select {
            return false;
        }
        crate::trace!(
            "script",
            "updateTab",
            "{ext}: tab {tab_id} url={:?} select={select}",
            url
        );
        self.as_mut().tab_update_requested(
            tab_id,
            QString::from(&url.unwrap_or_default()),
            select,
        );
        true
    }

    fn remove_tab(mut self: Pin<&mut Self>, ext: &QString, tab_id: i32) -> bool {
        crate::trace!("script", "removeTab", "{}: tab {tab_id}", ext.to_string());
        self.as_mut().tab_close_requested(tab_id);
        true
    }

    fn reload_tab(mut self: Pin<&mut Self>, ext: &QString, tab_id: i32) -> bool {
        crate::trace!("script", "reloadTab", "{}: tab {tab_id}", ext.to_string());
        self.as_mut().tab_reload_requested(tab_id);
        true
    }

    fn fetch_url(
        mut self: Pin<&mut Self>,
        ext: &QString,
        request_id: &QString,
        spec: &QString,
    ) -> bool {
        let ext = ext.to_string();
        let Ok(request) = serde_json::from_str::<serde_json::Value>(&spec.to_string()) else {
            return false;
        };
        let Some(url) = request["url"].as_str() else {
            return false;
        };

        // Only the web. An extension reaching file:// or another extension
        // through the host would be a way around both of those boundaries.
        let lower = url.to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            crate::fixme!("fetch", "fetchUrl", "{ext}: refused {url}");
            return false;
        }

        // Earned the way a real browser makes it earned: the manifest must ask
        // for that host. Otherwise this call is an open cross-origin proxy for
        // anything that can reach the shim.
        if !crate::extensions::may_fetch(&ext, url) {
            crate::fixme!("fetch", "fetchUrl", "{ext}: no host permission for {url}");
            return false;
        }

        crate::trace!("fetch", "fetchUrl", "{ext}: {url}");
        self.as_mut().fetch_requested(
            QString::from(&ext),
            QString::from(&request_id.to_string()),
            QString::from(&spec.to_string()),
        );
        true
    }

    fn debug_enabled(self: Pin<&mut Self>, channel: &QString) -> bool {
        crate::debug::channel_enabled(&channel.to_string())
    }

    fn debug_log(
        self: Pin<&mut Self>,
        level: &QString,
        channel: &QString,
        subject: &QString,
        func: &QString,
        message: &QString,
    ) {
        use crate::debug::Class;
        let class = match level.to_string().as_str() {
            "err" => Class::Err,
            "fixme" => Class::Fixme,
            "warn" => Class::Warn,
            _ => Class::Trace,
        };
        crate::debug::log_about(
            class,
            &channel.to_string(),
            &subject.to_string(),
            &func.to_string(),
            format_args!("{}", message.to_string()),
        );
    }

    fn storage_get(mut self: Pin<&mut Self>, ext: &QString, keys: &QString) -> QString {
        let ext = ext.to_string();
        let requested: Option<Vec<String>> =
            serde_json::from_str::<serde_json::Value>(&keys.to_string())
                .ok()
                .and_then(|value| {
                    value.as_array().map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str().map(str::to_owned))
                            .collect()
                    })
                });

        // Bind the pin first: `rust_mut()` yields a temporary that would be
        // dropped while the borrow of the store is still live.
        let mut rust = self.as_mut().rust_mut();
        let store = rust.load(&ext);
        let result = match requested {
            // A null/absent key list means "give me everything".
            None => serde_json::Value::Object(store.clone()),
            Some(names) => {
                let mut out = serde_json::Map::new();
                for name in names {
                    if let Some(value) = store.get(&name) {
                        out.insert(name, value.clone());
                    }
                }
                serde_json::Value::Object(out)
            }
        };

        QString::from(&result.to_string())
    }

    fn storage_set(mut self: Pin<&mut Self>, ext: &QString, items: &QString) -> bool {
        let ext = ext.to_string();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&items.to_string()) else {
            return false;
        };
        let Some(object) = value.as_object() else {
            return false;
        };

        {
            let mut rust = self.as_mut().rust_mut();
            let store = rust.load(&ext);
            for (key, value) in object {
                store.insert(key.clone(), value.clone());
            }
        }
        self.rust().persist(&ext);
        true
    }

    fn storage_remove(mut self: Pin<&mut Self>, ext: &QString, keys: &QString) -> bool {
        let ext = ext.to_string();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&keys.to_string()) else {
            return false;
        };
        let names: Vec<String> = match &value {
            serde_json::Value::String(one) => vec![one.clone()],
            serde_json::Value::Array(items) => items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect(),
            _ => return false,
        };

        {
            let mut rust = self.as_mut().rust_mut();
            let store = rust.load(&ext);
            for name in names {
                store.remove(&name);
            }
        }
        self.rust().persist(&ext);
        true
    }

    fn storage_clear(mut self: Pin<&mut Self>, ext: &QString) -> bool {
        let ext = ext.to_string();
        {
            let mut rust = self.as_mut().rust_mut();
            rust.load(&ext).clear();
        }
        self.rust().persist(&ext);
        true
    }

    fn tabs_query(self: Pin<&mut Self>, query: &QString) -> QString {
        let tabs = serde_json::from_str::<serde_json::Value>(&self.rust().tabs_json.to_string())
            .unwrap_or_else(|_| serde_json::json!([]));
        let filter = serde_json::from_str::<serde_json::Value>(&query.to_string())
            .unwrap_or_else(|_| serde_json::json!({}));

        let Some(list) = tabs.as_array() else {
            return QString::from("[]");
        };

        let want_active = filter["active"].as_bool();
        let matched: Vec<&serde_json::Value> = list
            .iter()
            .filter(|tab| match want_active {
                Some(active) => tab["active"].as_bool().unwrap_or(false) == active,
                None => true,
            })
            .collect();

        QString::from(&serde_json::to_string(&matched).unwrap_or_else(|_| "[]".to_owned()))
    }

    fn post_message(self: Pin<&mut Self>, ext: &QString, target: &QString, payload: &QString) {
        self.message_posted(ext.clone(), target.clone(), payload.clone());
    }

    fn apply_system_theme(self: Pin<&mut Self>, ext: &QString, seed: &QString) -> QString {
        use crate::system_theme::{Outcome, Seed, apply};

        let settings = crate::config::system_theme_settings();
        let outcome = if !settings.enabled {
            Outcome::Disabled
        } else if ext.to_string() != settings.extension {
            Outcome::NotAllowed
        } else {
            match Seed::parse(&seed.to_string()) {
                Some(seed) => apply(&seed, &settings.script),
                None => Outcome::BadSeed,
            }
        };

        QString::from(&serde_json::json!({
            "ok": outcome == Outcome::Applied,
            "message": outcome.message(),
        }).to_string())
    }

    fn post_response(self: Pin<&mut Self>, ext: &QString, request: &QString, payload: &QString) {
        self.response_posted(ext.clone(), request.clone(), payload.clone());
    }
}

/// The `browser.*` shim injected into extension contexts.
///
/// It implements nothing itself — every call marshals to the host object above.
/// Deliberately promise-based, matching Firefox's WebExtensions rather than
/// Chrome's callbacks, with callback support kept for MV2 extensions that
/// still use it.
pub const BROWSER_SHIM: &str = r#"
// Installs a factory rather than a global `browser`. Several extensions can
// have content scripts on one page, and each must see its own identity —
// otherwise one extension's storage calls would be attributed to another.
// Each injected script closes over its own api object built from this.
(function () {
  if (window.__telaMakeApi) return;

  let host = null;
  const pending = [];
  const listenersByExt = new Map();
  // `runtime.onConnect` listeners, per extension — the background half of a
  // port. Kept beside the message listeners because a context can hold both.
  const connectListenersByExt = new Map();
  // Every port this context owns an end of, keyed by the id both ends share.
  const portsById = new Map();
  let portSeq = 0;
  // Listeners for events the *host* raises — tab and navigation lifecycle —
  // as `extension id -> event name -> functions`. An extension that is never
  // told a tab exists cannot answer questions about it: uBlock refuses to
  // serve a page's cosmetic filters until it has seen that page navigate.
  const eventListenersByExt = new Map();

  // Every call names its host method so the queue fallback can dispatch it
  // without a live object to call through.
  function withHost(method, args, fn) {
    if (host) return Promise.resolve(fn(host));
    // No channel in this world: queue for the host to drain.
    if (!hasChannel) return viaQueue(method, args);
    return new Promise((resolve) => pending.push(() => resolve(fn(host))));
  }

  const hasChannel = typeof QWebChannel !== "undefined" && typeof qt !== "undefined";

  // ── Fallback transport ───────────────────────────────────────────
  // Qt injects qt.webChannelTransport only into the view it owns the channel
  // for; a page view's isolated world never receives it. QML *can* call into
  // this world via runJavaScript, so calls are queued here and the host drains
  // them. Strictly worse than a real channel — it costs a poll — but it is
  // built only on a mechanism proven to work in this world.
  const outbox = [];
  let sequence = 0;
  const waiting = new Map();

  window.__telaDrain = function () {
    if (outbox.length === 0) return "[]";
    const batch = JSON.stringify(outbox);
    outbox.length = 0;
    return batch;
  };

  window.__telaResolve = function (id, value) {
    const resolve = waiting.get(id);
    if (!resolve) return;
    waiting.delete(id);
    resolve(value);
  };

  // Requests awaiting a reply, keyed by the id carried in the envelope.
  const awaitingReply = new Map();
  let requestSeq = 0;

  // The tab this context sits in, once known. Resolved lazily; -1 until then,
  // and always -1 for a background page, which is not in a tab.
  let knownTabId = -1;

  function send(extensionId, target, message) {
    const id = extensionId + ":" + (++requestSeq);
    // Who is sending. Extensions inspect this to decide whether a message came
    // from their own pages or from a content script, and reading `sender.url`
    // off an object that has neither url nor origin throws — which kills the
    // listener before it can reply, so the caller sees a dead background page.
    const from = {
      url: location.href,
      origin: location.origin,
      frameId: 0,
      tabId: knownTabId,
    };
    const envelope = JSON.stringify({ __telaReq: id, body: message, from });

    const reply = new Promise((resolve) => {
      awaitingReply.set(id, resolve);
      // A listener that never answers must not leave the caller hanging
      // forever; undefined is what Firefox delivers in that case too.
      setTimeout(() => {
        if (awaitingReply.has(id)) {
          awaitingReply.delete(id);
          // Extensions read this as "the background is gone", so say which
          // message went unanswered rather than leaving a silent undefined.
          console.error("tela:timeout no reply for " + extensionId + " -> " + target
                        + " " + JSON.stringify(message).slice(0, 160));
          resolve(undefined);
        }
      }, 5000);
    });

    withHost("postMessage", [extensionId, target, envelope],
             (h) => h.postMessage(extensionId, target, envelope));
    return reply;
  }

  // ── Ports ────────────────────────────────────────────────────────
  // `runtime.connect` is not a nicety. uBlock Origin runs *all* of its
  // content-script traffic over a port — including the request for a page's
  // cosmetic filters — so a port that cannot carry a reply means no element
  // hiding at all, however good the filter lists are. This was previously a
  // stub whose onMessage discarded its listeners.
  //
  // Built on the same envelope transport as sendMessage, with one difference:
  // no `__telaReq`, because a port is a standing channel rather than a
  // question. Routing fans out to every context, so each end ignores port ids
  // it does not own.
  function postEnvelope(ext, target, body) {
    const encoded = JSON.stringify({ body });
    withHost("postMessage", [ext, target, encoded],
             (h) => h.postMessage(ext, target, encoded));
  }

  function listenerSet(fns) {
    return {
      addListener: (fn) => fns.push(fn),
      removeListener: (fn) => {
        const at = fns.indexOf(fn);
        if (at >= 0) fns.splice(at, 1);
      },
      hasListener: (fn) => fns.indexOf(fn) >= 0,
    };
  }

  // `peer` is where this end's messages are addressed: a content script talks
  // to "background", the background talks back out to "tabs".
  function makePort(ext, portId, name, peer, sender, open) {
    const messageFns = [];
    const disconnectFns = [];
    const entry = { open: open === true, queue: [] };

    const port = {
      name,
      sender,
      postMessage(message) {
        // The connecting end hands back a port synchronously and extensions
        // post to it immediately, so anything said before the far end exists
        // is held rather than dropped.
        if (!entry.open) {
          entry.queue.push(message);
          return;
        }
        postEnvelope(ext, peer, { __telaPort: "msg", portId, body: message });
      },
      disconnect() {
        if (!portsById.has(portId)) return;
        portsById.delete(portId);
        postEnvelope(ext, peer, { __telaPort: "close", portId });
      },
      onMessage: listenerSet(messageFns),
      onDisconnect: listenerSet(disconnectFns),
    };

    entry.port = port;
    entry.deliver = (message) => {
      for (const fn of messageFns) {
        try { fn(message, port); } catch (e) { console.error(e); }
      }
    };
    entry.closed = () => {
      for (const fn of disconnectFns) {
        try { fn(port); } catch (e) { console.error(e); }
      }
    };
    entry.flush = () => {
      entry.open = true;
      const held = entry.queue.splice(0, entry.queue.length);
      for (const message of held)
        postEnvelope(ext, peer, { __telaPort: "msg", portId, body: message });
    };

    portsById.set(portId, entry);
    return port;
  }

  // Which tab this context is in. Only the queue transport can answer it: the
  // host object is shared by every view, so a context holding a real channel
  // cannot be told apart by it — and such a context is the background page or
  // a popup, which is not a tab at all.
  let tabIdPromise = null;
  function currentTabId() {
    if (!tabIdPromise) {
      tabIdPromise = (hasChannel
        ? Promise.resolve(-1)
        : viaQueue("tabId", []).then(
            (value) => (typeof value === "number" ? value : -1),
            () => -1))
        // Cached so a sender can be built synchronously.
        .then((value) => (knownTabId = value));
    }
    return tabIdPromise;
  }

  function openPort(ext, portId, name) {
    // uBlock reads `sender.url` unconditionally and `sender.tab.id` to find the
    // page a request belongs to; an incomplete sender either throws in its
    // connect handler or silently returns the wrong page's filters.
    currentTabId().then((tabId) => {
      const url = location.href;
      const sender = {
        id: ext,
        url,
        origin: location.origin,
        // Content scripts do not run in subframes yet, so this is honest
        // rather than assumed.
        frameId: 0,
        tab: tabId >= 0 ? { id: tabId, url, active: true, index: tabId, windowId: 0 } : undefined,
      };
      // Routed to the host's `page` debug channel — see TELA_DEBUG.
      console.debug("tela:port open " + portId + " name=" + name
                    + " tab=" + tabId + " url=" + url);
      postEnvelope(ext, "background", { __telaPort: "open", portId, name, sender });
      const entry = portsById.get(portId);
      if (entry) entry.flush();
    });
  }

  // Connections that arrived before anything was listening for them.
  const pendingOpens = new Map();

  function acceptPort(ext, envelope) {
    const port = makePort(ext, envelope.portId, envelope.name || "", "tabs",
                          envelope.sender, true);
    for (const fn of (connectListenersByExt.get(ext) || []).slice()) {
      try { fn(port); } catch (e) { console.error(e); }
    }
  }

  // Called when an extension registers its first onConnect listener.
  function drainPendingOpens(ext) {
    const held = pendingOpens.get(ext);
    if (!held || held.length === 0) return;
    pendingOpens.delete(ext);
    for (const envelope of held) {
      if (!portsById.has(envelope.portId))
        acceptPort(ext, envelope);
    }
  }

  function handlePort(ext, envelope) {
    const portId = envelope.portId;
    if (!portId) return;

    if (envelope.__telaPort === "open") {
      // Delivery reaches every context that might care, so accept once only.
      if (portsById.has(portId)) return;
      const listeners = connectListenersByExt.get(ext);
      if (!listeners || listeners.length === 0) {
        // A page can finish loading before its extension's background page has
        // started and registered a handler. Dropping the connection there
        // leaves the content script waiting for an answer that will never
        // come — for uBlock, that is a page with no element hiding, on every
        // load that wins the race.
        const held = pendingOpens.get(ext)
          || (pendingOpens.set(ext, []), pendingOpens.get(ext));
        held.push(envelope);
        // Bounded: this covers a startup gap, not a backlog.
        if (held.length > 32) held.shift();
        return;
      }
      acceptPort(ext, envelope);
      return;
    }

    const entry = portsById.get(portId);
    // Not ours: the fan-out reached a context that never opened this port.
    if (!entry) return;
    if (envelope.__telaPort === "msg") {
      entry.deliver(envelope.body);
    } else if (envelope.__telaPort === "close") {
      portsById.delete(portId);
      entry.closed();
    }
  }

  // ── Network on an extension's behalf ─────────────────────────────
  // An extension page is an origin, so the web's CORS rules apply to it and
  // every cross-origin fetch fails — filter lists, config, script updates. A
  // real browser exempts these under host permissions. The host performs them
  // instead, where no page origin is involved.
  function hostFetch(ext, spec) {
    const id = ext + ":fetch:" + (++requestSeq);
    const reply = new Promise((resolve) => {
      awaitingReply.set(id, resolve);
      setTimeout(() => {
        if (awaitingReply.has(id)) {
          awaitingReply.delete(id);
          resolve({ error: "timeout" });
        }
      }, 30000);
    });
    const encoded = JSON.stringify(spec);
    withHost("fetchUrl", [ext, id, encoded], (h) => h.fetchUrl(ext, id, encoded));
    return reply;
  }

  function headerObject(init) {
    const out = {};
    if (!init) return out;
    if (typeof init.forEach === "function" && !Array.isArray(init)) {
      init.forEach((value, name) => { out[name] = String(value); });
    } else if (Array.isArray(init)) {
      for (const pair of init)
        if (pair && pair.length === 2) out[pair[0]] = String(pair[1]);
    } else {
      for (const name of Object.keys(init)) out[name] = String(init[name]);
    }
    return out;
  }

  // Only cross-origin http(s) is routed; an extension reading its own files
  // stays on the native path.
  function needsHost(url) {
    const target = String(url || "");
    if (!/^https?:/i.test(target)) return false;
    try {
      return new URL(target, location.href).origin !== location.origin;
    } catch (error) {
      return false;
    }
  }

  function installHostNetwork(ext) {
    const nativeFetch = typeof fetch === "function" ? fetch.bind(window) : null;
    const NativeXHR = window.XMLHttpRequest;

    window.fetch = function (input, init) {
      const url = typeof input === "string" ? input : (input && input.url) || "";
      if (!needsHost(url))
        return nativeFetch ? nativeFetch(input, init)
                           : Promise.reject(new TypeError("fetch unavailable"));
      const options = init || {};
      return hostFetch(ext, {
        url: String(url),
        method: options.method || "GET",
        headers: headerObject(options.headers),
        body: typeof options.body === "string" ? options.body : null,
      }).then((result) => {
        if (!result || result.error) throw new TypeError("Failed to fetch");
        return new Response(result.body, {
          status: result.status,
          statusText: result.statusText || "",
        });
      });
    };

    // Enough of XMLHttpRequest for the way extensions use it: uBlock and
    // Violentmonkey both fetch their lists with it. Same-origin work is handed
    // straight to the real one.
    function TelaXHR() {
      this.readyState = 0;
      this.status = 0;
      this.statusText = "";
      this.responseText = "";
      this.response = "";
      this.responseType = "";
      this.timeout = 0;
      this.onreadystatechange = null;
      this.onload = null;
      this.onerror = null;
      this.ontimeout = null;
      this.onloadend = null;
      this._headers = {};
      this._native = null;
      this._responseHeaders = "";
    }

    TelaXHR.prototype._advance = function (state) {
      this.readyState = state;
      if (typeof this.onreadystatechange === "function") {
        try { this.onreadystatechange(); } catch (error) { console.error(error); }
      }
    };

    TelaXHR.prototype._finish = function (result) {
      if (!result || result.error) {
        this.status = 0;
        this._advance(4);
        if (typeof this.onerror === "function") this.onerror(new Error("network"));
      } else {
        this.status = result.status;
        this.statusText = result.statusText || "";
        this.responseText = result.body || "";
        this.response = this.responseText;
        this._responseHeaders = result.headers || "";
        this._advance(4);
        if (typeof this.onload === "function") this.onload();
      }
      if (typeof this.onloadend === "function") this.onloadend();
    };

    TelaXHR.prototype.open = function (method, url, async) {
      this._method = method || "GET";
      this._url = String(url || "");
      if (!needsHost(this._url)) {
        this._native = new NativeXHR();
        return this._native.open(method, url, async !== false);
      }
      this._advance(1);
    };

    TelaXHR.prototype.setRequestHeader = function (name, value) {
      if (this._native) return this._native.setRequestHeader(name, value);
      this._headers[name] = String(value);
    };

    TelaXHR.prototype.getAllResponseHeaders = function () {
      return this._native ? this._native.getAllResponseHeaders() : this._responseHeaders;
    };

    TelaXHR.prototype.getResponseHeader = function (name) {
      if (this._native) return this._native.getResponseHeader(name);
      const wanted = String(name).toLowerCase();
      for (const line of this._responseHeaders.split("\r\n")) {
        const at = line.indexOf(":");
        if (at > 0 && line.slice(0, at).trim().toLowerCase() === wanted)
          return line.slice(at + 1).trim();
      }
      return null;
    };

    TelaXHR.prototype.abort = function () {
      if (this._native) this._native.abort();
    };

    TelaXHR.prototype.addEventListener = function (name, fn) {
      if (this._native) return this._native.addEventListener(name, fn);
      if (name === "load") this.onload = fn;
      else if (name === "error") this.onerror = fn;
      else if (name === "loadend") this.onloadend = fn;
      else if (name === "readystatechange") this.onreadystatechange = fn;
    };
    TelaXHR.prototype.removeEventListener = function () {};

    TelaXHR.prototype.send = function (body) {
      if (this._native) {
        const native = this._native;
        const self = this;
        native.onreadystatechange = function () {
          self.readyState = native.readyState;
          self.status = native.status;
          self.statusText = native.statusText;
          try {
            self.responseText = native.responseText;
            self.response = native.response;
          } catch (error) { /* responseType makes these throw */ }
          if (typeof self.onreadystatechange === "function") self.onreadystatechange();
          if (native.readyState === 4 && typeof self.onload === "function") self.onload();
        };
        return native.send(body);
      }
      this._advance(2);
      hostFetch(ext, {
        url: this._url,
        method: this._method,
        headers: this._headers,
        body: typeof body === "string" ? body : null,
      }).then((result) => this._finish(result));
    };

    TelaXHR.DONE = 4;
    TelaXHR.LOADING = 3;
    TelaXHR.HEADERS_RECEIVED = 2;
    TelaXHR.OPENED = 1;
    TelaXHR.UNSENT = 0;
    window.XMLHttpRequest = TelaXHR;
  }

  // Requests already handled here. Delivery can reach a context more than
  // once — routing fans out to every context that might be waiting — and a
  // listener must not run twice for one request, least of all one that mutates
  // state before replying.
  const handledRequests = new Set();

  // Called by the host when a message arrives for this context.
  window.__telaDeliver = function (ext, payload) {
    const listeners = listenersByExt.get(ext);
    let envelope;
    try { envelope = JSON.parse(payload); } catch (e) { envelope = { body: payload }; }
    if (!envelope || typeof envelope !== "object") envelope = { body: envelope };

    const requestId = envelope.__telaReq;
    if (requestId) {
      if (handledRequests.has(requestId)) return;
      handledRequests.add(requestId);
      // Bounded: this only needs to cover in-flight requests.
      if (handledRequests.size > 500) handledRequests.clear();
    }
    const body = "body" in envelope ? envelope.body : envelope;

    // Port traffic is not a runtime.onMessage delivery and must never be shown
    // to those listeners: uBlock registers both, and would answer its own port
    // frames as if they were messages.
    if (body && typeof body === "object" && body.__telaPort) {
      handlePort(ext, body);
      return;
    }

    // Likewise for host-raised events: they carry their own listener list.
    if (body && typeof body === "object" && body.__telaEvent) {
      const perExt = eventListenersByExt.get(ext);
      const fns = perExt && perExt.get(body.__telaEvent);
      if (fns) {
        // Copied first: a listener that removes itself while the list is being
        // walked would otherwise make the walk skip its neighbour.
        for (const fn of fns.slice()) {
          try { fn.apply(null, body.args || []); } catch (e) { console.error(e); }
        }
      }
      return;
    }

    // What WebExtensions calls a MessageSender. `origin` and `url` are the
    // fields extensions actually branch on to tell their own pages apart from
    // a content script.
    const from = envelope.from || {};
    const sender = { id: ext, url: from.url || "", origin: from.origin || "",
                     frameId: from.frameId || 0 };
    if (typeof from.tabId === "number" && from.tabId >= 0)
      sender.tab = { id: from.tabId, url: from.url || "" };

    if (!listeners || listeners.length === 0) {
      if (requestId) respond(ext, requestId, undefined);
      return;
    }

    let answered = false;
    // A listener signals "I will answer later" by returning true or a promise.
    // Without tracking that, the not-answered fallback below races the async
    // reply and wins, turning every promise-returning listener into undefined.
    let asyncPending = false;
    const answer = (value) => {
      if (answered || !requestId) return;
      answered = true;
      respond(ext, requestId, value);
    };

    for (const listener of listeners) {
      try {
        // A listener answers by returning a value, returning a promise, or
        // calling sendResponse — all three are legal in WebExtensions.
        const result = listener(body, sender, answer);
        if (result && typeof result.then === "function") {
          asyncPending = true;
          result.then(answer, () => answer(undefined));
        } else if (result === true) {
          // Chrome's convention: keep the channel open for sendResponse.
          asyncPending = true;
        } else if (result !== undefined) {
          answer(result);
        }
      } catch (e) {
        console.error(e);
      }
    }

    // Nobody answered and nobody claimed they would: reply now rather than
    // leaving the caller waiting on the 5s timeout.
    if (requestId && !answered && !asyncPending) {
      answer(undefined);
    }
  };

  function respond(ext, requestId, value) {
    const encoded = JSON.stringify(value === undefined ? null : value);
    withHost("postResponse", [ext, requestId, encoded],
             (h) => h.postResponse(ext, requestId, encoded));
  }

  // Called by the host when a reply comes back.
  window.__telaResponse = function (ext, requestId, payload) {
    const resolve = awaitingReply.get(requestId);
    if (!resolve) return;
    awaitingReply.delete(requestId);
    let value;
    try { value = JSON.parse(payload); } catch (e) { value = payload; }
    resolve(value === null ? undefined : value);
  };

  function viaQueue(method, args) {
    return new Promise((resolve) => {
      const id = ++sequence;
      waiting.set(id, resolve);
      outbox.push({ id, method, args });
    });
  }

  // qwebchannel.js is injected ahead of this shim.
  if (typeof QWebChannel !== "undefined" && typeof qt !== "undefined") {
    new QWebChannel(qt.webChannelTransport, (channel) => {
      host = channel.objects.telaApi;
      // Deliberately *not* connecting host.messagePosted here. The host fans
      // messages out to every context through runJavaScript, and a direct
      // signal connection would deliver the same message a second time —
      // running every listener twice and racing two replies for one request.
      while (pending.length) pending.shift()();
    });
  }

  // Chrome's MV2 style passes a callback; Firefox returns a promise. Support
  // both so unmodified extensions from either ecosystem work.
  function dual(promise, callback) {
    if (typeof callback === "function") {
      promise.then((value) => callback(value), () => callback(undefined));
      return undefined;
    }
    return promise;
  }

  // Shapes reused by the inert namespaces below.
  function event() {
    return { addListener: () => {}, removeListener: () => {}, hasListener: () => false };
  }
  function setting() {
    return {
      get: (_d, cb) => {
        const v = { value: undefined, levelOfControl: "not_controllable" };
        return cb ? cb(v) : Promise.resolve(v);
      },
      set: (_d, cb) => (cb ? cb(false) : Promise.resolve(false)),
      clear: (_d, cb) => (cb ? cb(false) : Promise.resolve(false)),
      onChange: event(),
    };
  }
  function makeAction() {
    return {
      // These return promises rather than undefined: extensions chain .catch()
      // onto them, and undefined.catch is a hard failure at startup.
      setPopup: () => Promise.resolve(),
      getPopup: (_d, cb) => (cb ? cb("") : Promise.resolve("")),
      setTitle: () => Promise.resolve(),
      getTitle: (_d, cb) => (cb ? cb("") : Promise.resolve("")),
      setIcon: (_d, cb) => (cb ? cb() : Promise.resolve()),
      setBadgeText: () => Promise.resolve(),
      getBadgeText: (_d, cb) => (cb ? cb("") : Promise.resolve("")),
      setBadgeBackgroundColor: () => Promise.resolve(),
      setBadgeTextColor: () => Promise.resolve(),
      enable: () => Promise.resolve(),
      disable: () => Promise.resolve(),
      openPopup: () => Promise.resolve(),
      onClicked: event(),
    };
  }

  const makeApi = function (extensionId, grants) {
  // Which manifest permission each namespace requires. Namespaces absent from
  // this table need no permission in a real browser either — runtime, i18n and
  // extension are always available, and `permissions` must work in order to
  // request anything else.
  //
  // An ungranted namespace is deleted rather than stubbed: extensions feature
  // detect with `if (browser.cookies)`, so a stub that throws on use would be
  // worse than plain absence.
  //
  // Only namespaces that are genuinely permission-gated appear here. Three
  // that look like they belong are deliberately absent:
  //   tabs/windows — always available; the "tabs" permission gates the
  //                  sensitive properties (url, title, favIconUrl), not the
  //                  namespace. Gating the whole thing broke SponsorBlock's
  //                  tabs.onUpdated listener, which needs no permission.
  //   commands     — granted by a top-level `commands` manifest key rather
  //                  than a permission, so no entry in `permissions` implies
  //                  its absence. Gating it broke Midnight Lizard.
  const REQUIRES = {
    storage: "storage",
    cookies: "cookies",
    webRequest: "webRequest",
    webNavigation: "webNavigation",
    declarativeNetRequest: "declarativeNetRequest",
    downloads: "downloads",
    scripting: "scripting",
    idle: "idle",
    privacy: "privacy",
    dns: "dns",
    theme: "theme",
  };
  const granted = Array.isArray(grants) ? grants : null;

  const messageListeners = listenersByExt.get(extensionId)
    || (listenersByExt.set(extensionId, []), listenersByExt.get(extensionId));

  const connectListeners = connectListenersByExt.get(extensionId)
    || (connectListenersByExt.set(extensionId, []), connectListenersByExt.get(extensionId));

  const extEvents = eventListenersByExt.get(extensionId)
    || (eventListenersByExt.set(extensionId, new Map()), eventListenersByExt.get(extensionId));

  // An event the host raises, as opposed to `event()`, which is an inert stub
  // for the ones nothing here can deliver yet.
  function hostEvent(name) {
    const fns = extEvents.get(name) || (extEvents.set(name, []), extEvents.get(name));
    return listenerSet(fns);
  }

  const local = {
    get(keys, callback) {
      const encoded = keys == null ? "null"
        : JSON.stringify(Array.isArray(keys) ? keys
          : typeof keys === "string" ? [keys] : Object.keys(keys));
      const promise = withHost("storageGet", [extensionId, encoded],
                               (h) => h.storageGet(extensionId, encoded))
        .then((json) => {
          const value = JSON.parse(json || "{}");
          // An object argument supplies defaults for missing keys.
          if (keys && !Array.isArray(keys) && typeof keys === "object") {
            for (const key of Object.keys(keys)) {
              if (!(key in value)) value[key] = keys[key];
            }
          }
          return value;
        });
      return dual(promise, callback);
    },
    set(items, callback) {
      const encoded = JSON.stringify(items);
      return dual(withHost("storageSet", [extensionId, encoded],
                           (h) => h.storageSet(extensionId, encoded)), callback);
    },
    remove(keys, callback) {
      const encoded = JSON.stringify(keys);
      return dual(withHost("storageRemove", [extensionId, encoded],
                           (h) => h.storageRemove(extensionId, encoded)), callback);
    },
    clear(callback) {
      return dual(withHost("storageClear", [extensionId],
                           (h) => h.storageClear(extensionId)), callback);
    },
    // Some extensions listen on the area rather than on `storage`.
    onChanged: { addListener: () => {}, removeListener: () => {}, hasListener: () => false },
    QUOTA_BYTES: 5242880,
  };

  // Administrator policy, of which there is none here — so it reads empty and
  // refuses writes, exactly as a browser with no managed manifest does.
  //
  // This must never alias `local`. uBlock Origin reads managed storage during
  // startup and writes back whatever it finds under `cachedManagedStorage`;
  // aliased, that copies the entire store into itself on every run, squaring
  // its size each time. One profile reached 143 MB of JSON — re-parsed on first
  // access and rewritten whole on every `set`, which presents as "the browser
  // is randomly slow" rather than as a storage bug.
  const readOnly = () => Promise.reject(new Error("storage.managed is read-only"));

  const managed = {
    get(keys, callback) {
      // An object argument still supplies its defaults: no policy set means
      // every key falls back rather than coming back missing.
      const value = keys && !Array.isArray(keys) && typeof keys === "object"
        ? Object.assign({}, keys) : {};
      return dual(Promise.resolve(value), callback);
    },
    set: (_items, callback) => dual(readOnly(), callback),
    remove: (_keys, callback) => dual(readOnly(), callback),
    clear: (callback) => dual(readOnly(), callback),
    onChanged: event(),
    QUOTA_BYTES: 0,
  };

  // Session storage is meant to die with the browser. Backing it with the same
  // file as `local` persists it forever and grows the store with data nobody
  // intended to keep. Held per context rather than shared with the background
  // page, which is the one way this is thinner than the real thing.
  const sessionData = new Map();

  const session = {
    get(keys, callback) {
      const value = {};
      if (keys == null) {
        for (const [key, held] of sessionData) value[key] = held;
      } else if (typeof keys === "string") {
        if (sessionData.has(keys)) value[keys] = sessionData.get(keys);
      } else if (Array.isArray(keys)) {
        for (const key of keys)
          if (sessionData.has(key)) value[key] = sessionData.get(key);
      } else {
        for (const key of Object.keys(keys))
          value[key] = sessionData.has(key) ? sessionData.get(key) : keys[key];
      }
      return dual(Promise.resolve(value), callback);
    },
    set(items, callback) {
      for (const key of Object.keys(items || {})) sessionData.set(key, items[key]);
      return dual(Promise.resolve(), callback);
    },
    remove(keys, callback) {
      for (const key of Array.isArray(keys) ? keys : [keys]) sessionData.delete(key);
      return dual(Promise.resolve(), callback);
    },
    clear(callback) {
      sessionData.clear();
      return dual(Promise.resolve(), callback);
    },
    onChanged: event(),
    QUOTA_BYTES: 10485760,
  };

  // insertCSS and removeCSS differ only in the host method they reach. Both
  // accept `(tabId, details)` and, for the current tab, `(details)`.
  function cssCall(method, tabId, details, callback) {
    const inCurrentTab = tabId !== null && typeof tabId === "object";
    const target = inCurrentTab ? -1 : (typeof tabId === "number" ? tabId : -1);
    const injection = inCurrentTab ? tabId : (details || {});
    const done = inCurrentTab ? details : callback;

    const encoded = JSON.stringify(injection);
    const promise = withHost(method, [extensionId, target, encoded],
                             (h) => h[method](extensionId, target, encoded))
      .then(() => undefined, () => undefined);
    return dual(promise, typeof done === "function" ? done : undefined);
  }

  const api = {
    __tela: true,
    runtime: {
      id: extensionId,
      getURL: (path) => `tela-extension://${extensionId}/${String(path).replace(/^\//, "")}`,
      getManifest: () => window.__telaManifest || {},
      sendMessage(message, callback) {
        // Two-argument form: sendMessage(extensionId, message).
        if (typeof message === "string" && arguments.length > 1
            && typeof callback !== "function") {
          message = arguments[1];
        }
        return dual(send(extensionId, "background", message), callback);
      },
      onMessage: {
        addListener: (fn) => messageListeners.push(fn),
        removeListener: (fn) => {
          const at = messageListeners.indexOf(fn);
          if (at >= 0) messageListeners.splice(at, 1);
        },
        hasListener: (fn) => messageListeners.indexOf(fn) >= 0,
      },
      // Nothing installs or updates at runtime yet; a no-op listener is
      // friendlier than an undefined property crashing the extension.
      onInstalled: event(),
      onStartup: event(),
      onSuspend: event(),
      onSuspendCanceled: event(),
      onUpdateAvailable: event(),
      onConnectExternal: event(),
      onMessageExternal: event(),
      onConnect: listenerSet(connectListeners),
      // Port-style messaging. Extensions open a port during startup and expect
      // an object back immediately, so the port is returned before the far end
      // has acknowledged it and anything said meanwhile is buffered.
      connect(_extensionIdOrInfo, _info) {
        const info = _extensionIdOrInfo && typeof _extensionIdOrInfo === "object"
          ? _extensionIdOrInfo : (_info || {});
        const name = info.name || "";
        const portId = extensionId + ":" + (++portSeq) + ":"
          + Math.random().toString(36).slice(2);
        const port = makePort(extensionId, portId, name, "background", undefined, false);
        openPort(extensionId, portId, name);
        return port;
      },
      PlatformOs: { MAC: "mac", WIN: "win", ANDROID: "android", CROS: "cros",
                    LINUX: "linux", OPENBSD: "openbsd" },
      PlatformArch: { ARM: "arm", X86_32: "x86-32", X86_64: "x86-64" },
      OnInstalledReason: { INSTALL: "install", UPDATE: "update",
                          BROWSER_UPDATE: "browser_update" },
      getPlatformInfo(callback) {
        const info = { os: "linux", arch: "x86-64" };
        return typeof callback === "function" ? callback(info) : Promise.resolve(info);
      },
      getBrowserInfo(callback) {
        const info = { name: "Tela", vendor: "Tela", version: "0.1.0", buildID: "0" };
        return typeof callback === "function" ? callback(info) : Promise.resolve(info);
      },
      setUninstallURL: (_u, cb) => (cb ? cb() : Promise.resolve()),
      openOptionsPage: (cb) => (cb ? cb() : Promise.resolve()),
      reload: () => {},
      lastError: null,
      onPerformanceWarning: event(),
    },
    storage: {
      local,
      // No sync backend exists. Firefox without an account behaves the same
      // way: writes land locally and simply never leave the machine.
      sync: local,
      onChanged: event(),
      managed,
      session,
    },
    tabs: {
      TAB_ID_NONE: -1,
      TAB_INDEX_NONE: -1,
      query(info, callback) {
        const encoded = JSON.stringify(info || {});
        const promise = withHost("tabsQuery", [encoded], (h) => h.tabsQuery(encoded))
          .then((json) => JSON.parse(json || "[]"));
        return dual(promise, callback);
      },
      sendMessage(tabId, message, callback) {
        return dual(send(extensionId, "tabs", message), callback);
      },
      onUpdated: hostEvent("tabs.onUpdated"),
      onRemoved: hostEvent("tabs.onRemoved"),
      onActivated: hostEvent("tabs.onActivated"),
      onCreated: hostEvent("tabs.onCreated"),
      onMoved: event(), onAttached: event(),
      onDetached: event(), onReplaced: event(), onHighlighted: event(),
      onZoomChange: event(),
      // Answered from the host's tab list rather than with null. An extension
      // that cannot look a tab up by id cannot associate a request, a port or
      // a content script with the page it came from.
      get(tabId, callback) {
        const promise = withHost("tabsQuery", ["{}"], (h) => h.tabsQuery("{}"))
          .then((json) => {
            const list = JSON.parse(json || "[]");
            return list.find((tab) => tab.id === tabId) || null;
          })
          .catch(() => null);
        return dual(promise, callback);
      },
      getCurrent(callback) {
        const encoded = JSON.stringify({ active: true });
        const promise = withHost("tabsQuery", [encoded], (h) => h.tabsQuery(encoded))
          .then((json) => {
            const list = JSON.parse(json || "[]");
            return list.length > 0 ? list[0] : null;
          })
          .catch(() => null);
        return dual(promise, callback);
      },
      // uBlock's popup buttons — dashboard, logger, element picker — are all
      // this call. Returning null made every one of them do nothing.
      create(props, callback) {
        const details = props || {};
        const encoded = JSON.stringify(details);
        const promise = withHost("createTab", [extensionId, encoded],
                                 (h) => h.createTab(extensionId, encoded))
          // The tab object a caller expects back. The id is not yet knowable
          // here; extensions overwhelmingly ignore it for a tab they opened.
          .then(() => ({ id: -1, url: details.url || "", active: details.active !== false }),
                () => null);
        return dual(promise, callback);
      },
      // Real, all three. An extension that cannot focus, navigate, close or
      // reload a tab fails silently at whatever it was doing — the "no-op that
      // looks like success" shape that hid every bug in this shim.
      //
      // Both argument forms are legal: `(tabId, props)` and, for the current
      // tab, `(props)`.
      update(tabId, props, callback) {
        const forCurrent = tabId !== null && typeof tabId === "object";
        const target = forCurrent ? -1 : (typeof tabId === "number" ? tabId : -1);
        const details = forCurrent ? tabId : (props || {});
        const done = forCurrent ? props : callback;

        const encoded = JSON.stringify(details);
        const promise = withHost("updateTab", [extensionId, target, encoded],
                                 (h) => h.updateTab(extensionId, target, encoded))
          .then(() => ({ id: target, url: details.url || "" }), () => null);
        return dual(promise, typeof done === "function" ? done : undefined);
      },
      remove(tabIds, callback) {
        // The spec takes an id or an array of them.
        const list = Array.isArray(tabIds) ? tabIds : [tabIds];
        const promise = Promise.all(list.map((id) =>
          withHost("removeTab", [extensionId, id],
                   (h) => h.removeTab(extensionId, id))))
          .then(() => undefined, () => undefined);
        return dual(promise, callback);
      },
      reload(tabId, props, callback) {
        const forCurrent = tabId === undefined || typeof tabId === "object";
        const target = forCurrent ? -1 : (typeof tabId === "number" ? tabId : -1);
        const done = typeof props === "function" ? props : callback;
        const promise = withHost("reloadTab", [extensionId, target],
                                 (h) => h.reloadTab(extensionId, target))
          .then(() => undefined, () => undefined);
        return dual(promise, typeof done === "function" ? done : undefined);
      },
      // Present rather than absent, deliberately. Tela defers restored tabs on
      // its own, so there is nothing here to discard — but auto-tab-discard
      // wraps this in a Proxy at load time, and `new Proxy(undefined, …)`
      // throws before any of its code runs. A function that resolves is the
      // difference between the extension loading and not.
      discard: (_id, cb) => (cb ? cb(null) : Promise.resolve(null)),
      // Element hiding arrives here and nowhere else. uBlock computes a page's
      // cosmetic filters and pushes them as a stylesheet through this call, so
      // a stub that resolves silently means an extension can run flawlessly
      // and still hide nothing at all.
      insertCSS(tabId, details, callback) {
        return cssCall("insertCss", tabId, details, callback);
      },
      removeCSS(tabId, details, callback) {
        return cssCall("removeCss", tabId, details, callback);
      },
      // Real, because the alternative is not "degraded" — it is nothing.
      // Violentmonkey injects every userscript through this call, and uBlock
      // loads its procedural-filter code the same way. A stub returning `[]`
      // makes both look like they ran.
      //
      // Both argument shapes are legal: `(tabId, details)` and, for the
      // current tab, `(details)`.
      executeScript(tabId, details, callback) {
        const inCurrentTab = tabId !== null && typeof tabId === "object";
        const target = inCurrentTab ? -1 : (typeof tabId === "number" ? tabId : -1);
        const injection = inCurrentTab ? tabId : (details || {});
        const done = inCurrentTab ? details : callback;

        const encoded = JSON.stringify(injection);
        const promise = withHost("executeScript", [extensionId, target, encoded],
                                 (h) => h.executeScript(extensionId, target, encoded))
          // The result array is what a caller destructures; the host runs the
          // source in another world and has nothing to hand back from it.
          .then(() => [], () => []);
        return dual(promise, typeof done === "function" ? done : undefined);
      },
      connect: () => ({ postMessage: () => {}, disconnect: () => {},
                        onMessage: event(), onDisconnect: event() }),
    },
    // ── alarms ───────────────────────────────────────────────────────
    // Genuinely implemented, not stubbed: alarms are just timers, and doing
    // them in the extension's own context is both simpler and more accurate
    // than round-tripping through the host.
    alarms: (function () {
      const timers = new Map();
      const listeners = [];

      function fire(name) {
        const alarm = { name, scheduledTime: Date.now() };
        for (const listener of listeners) {
          try { listener(alarm); } catch (e) { console.error(e); }
        }
      }

      return {
        create(name, info) {
          if (typeof name === "object") { info = name; name = ""; }
          info = info || {};
          alarmsClear(name);

          const delay = info.when ? Math.max(0, info.when - Date.now())
                      : (info.delayInMinutes || 0) * 60000;
          const period = (info.periodInMinutes || 0) * 60000;

          const start = setTimeout(() => {
            fire(name);
            if (period > 0) {
              const repeat = setInterval(() => fire(name), period);
              timers.set(name, { handle: repeat, repeating: true });
            } else {
              timers.delete(name);
            }
          }, delay);
          timers.set(name, { handle: start, repeating: false });
        },
        clear(name, callback) {
          const cleared = alarmsClear(name || "");
          return typeof callback === "function" ? callback(cleared) : Promise.resolve(cleared);
        },
        clearAll(callback) {
          for (const name of [...timers.keys()]) alarmsClear(name);
          return typeof callback === "function" ? callback(true) : Promise.resolve(true);
        },
        get(name, callback) {
          const found = timers.has(name || "") ? { name: name || "" } : undefined;
          return typeof callback === "function" ? callback(found) : Promise.resolve(found);
        },
        getAll(callback) {
          const all = [...timers.keys()].map((name) => ({ name }));
          return typeof callback === "function" ? callback(all) : Promise.resolve(all);
        },
        onAlarm: {
          addListener: (fn) => listeners.push(fn),
          removeListener: (fn) => {
            const at = listeners.indexOf(fn);
            if (at >= 0) listeners.splice(at, 1);
          },
        },
      };

      function alarmsClear(name) {
        const existing = timers.get(name);
        if (!existing) return false;
        existing.repeating ? clearInterval(existing.handle) : clearTimeout(existing.handle);
        timers.delete(name);
        return true;
      }
    })(),

    // ── idle ─────────────────────────────────────────────────────────
    // No idle detection is wired up, so the honest answer is "active".
    // Extensions use this to back off work; always-active is the safe reply.
    idle: {
      queryState(_seconds, callback) {
        return typeof callback === "function"
          ? callback("active") : Promise.resolve("active");
      },
      setDetectionInterval: () => Promise.resolve(),
      onStateChanged: { addListener: () => {}, removeListener: () => {} },
    },

    // ── notifications ────────────────────────────────────────────────
    // Routed to the page's own Notification API where permitted. Not a host
    // notification yet; it does not silently vanish either.
    notifications: (function () {
      let counter = 0;
      return {
        create(id, options, callback) {
          if (typeof id === "object") { options = id; id = "n" + (++counter); }
          options = options || {};
          try {
            if (typeof Notification !== "undefined" && Notification.permission === "granted") {
              new Notification(options.title || "", { body: options.message || "" });
            } else {
              console.log("[notification]", options.title || "", options.message || "");
            }
          } catch (e) { /* Never let a notification break the caller. */ }
          return typeof callback === "function" ? callback(id) : Promise.resolve(id);
        },
        clear(id, callback) {
          return typeof callback === "function" ? callback(true) : Promise.resolve(true);
        },
        getAll(callback) {
          return typeof callback === "function" ? callback({}) : Promise.resolve({});
        },
        onClicked: { addListener: () => {}, removeListener: () => {} },
        onClosed: { addListener: () => {}, removeListener: () => {} },
      };
    })(),

    // ── contextMenus / menus ─────────────────────────────────────────
    // Items are recorded so create/remove behave consistently and ids stay
    // stable. They are not yet shown in the browser's own context menu —
    // that wiring is the remaining work, and nothing here crashes without it.
    contextMenus: (function () {
      const items = new Map();
      let counter = 0;
      return {
        ContextType: {
          ALL: "all", PAGE: "page", FRAME: "frame", SELECTION: "selection",
          LINK: "link", EDITABLE: "editable", IMAGE: "image", VIDEO: "video",
          AUDIO: "audio", LAUNCHER: "launcher", BOOKMARK: "bookmark",
          BROWSER_ACTION: "browser_action", PAGE_ACTION: "page_action",
          ACTION: "action", TAB: "tab", TOOLS_MENU: "tools_menu",
        },
        ItemType: {
          NORMAL: "normal", CHECKBOX: "checkbox", RADIO: "radio",
          SEPARATOR: "separator",
        },
        ACTION_MENU_TOP_LEVEL_LIMIT: 6,
        create(properties, callback) {
          const id = properties.id || "menu" + (++counter);
          items.set(id, properties);
          if (typeof callback === "function") callback();
          return id;
        },
        update(id, properties, callback) {
          if (items.has(id)) items.set(id, Object.assign({}, items.get(id), properties));
          return typeof callback === "function" ? callback() : Promise.resolve();
        },
        remove(id, callback) {
          items.delete(id);
          return typeof callback === "function" ? callback() : Promise.resolve();
        },
        removeAll(callback) {
          items.clear();
          return typeof callback === "function" ? callback() : Promise.resolve();
        },
        onClicked: { addListener: () => {}, removeListener: () => {} },
      };
    })(),

    // ── scripting ────────────────────────────────────────────────────
    // Only the same-context form. Cross-tab injection needs the host and is
    // deliberately rejected rather than silently doing nothing.
    scripting: {
      executeScript(injection, callback) {
        const run = () => {
          try {
            if (typeof injection.func === "function") {
              return injection.func.apply(null, injection.args || []);
            }
          } catch (e) { console.error("[scripting]", e); }
          return undefined;
        };
        const result = [{ result: run() }];
        return typeof callback === "function" ? callback(result) : Promise.resolve(result);
      },
      insertCSS(injection, callback) {
        try {
          const style = document.createElement("style");
          style.textContent = injection.css || "";
          (document.head || document.documentElement).appendChild(style);
        } catch (e) { /* no document in a background context */ }
        return typeof callback === "function" ? callback() : Promise.resolve();
      },
    },

    // ── Namespaces present but inert ─────────────────────────────────
    // Extensions do not crash on missing *behaviour* — they crash on missing
    // *namespaces*, at load, before any of their working code runs. Providing
    // the shape lets an extension boot and use the parts that do work, instead
    // of dying on a property access during startup.
    //
    // These are honestly inert: listeners are never called, setters record
    // nothing. That is visible in behaviour rather than as a crash.
    browserAction: makeAction(),
    action: makeAction(),
    pageAction: makeAction(),

    webNavigation: {
      onBeforeNavigate: hostEvent("webNavigation.onBeforeNavigate"),
      onCommitted: hostEvent("webNavigation.onCommitted"),
      onCompleted: hostEvent("webNavigation.onCompleted"),
      onDOMContentLoaded: hostEvent("webNavigation.onDOMContentLoaded"),
      onErrorOccurred: event(),
      onCreatedNavigationTarget: event(), onHistoryStateUpdated: event(),
      onReferenceFragmentUpdated: event(),
      getFrame: (_d, cb) => (cb ? cb(null) : Promise.resolve(null)),
      getAllFrames: (_d, cb) => (cb ? cb([]) : Promise.resolve([])),
    },

    // Blocking request interception is not exposed to extensions: Tela does
    // its filtering natively. The namespace exists so extensions load.
    webRequest: {
      ResourceType: {
        MAIN_FRAME: "main_frame", SUB_FRAME: "sub_frame", STYLESHEET: "stylesheet",
        SCRIPT: "script", IMAGE: "image", FONT: "font", OBJECT: "object",
        XMLHTTPREQUEST: "xmlhttprequest", PING: "ping", CSP_REPORT: "csp_report",
        MEDIA: "media", WEBSOCKET: "websocket", OTHER: "other",
      },
      // These option enums are read, not called. An extension asks whether a
      // member exists to decide what to request — `hasOwn(OnXOptions,
      // 'EXTRA_HEADERS')` — and a missing enum object throws there rather than
      // answering false. Privacy Badger died on exactly this: without
      // OnResponseStartedOptions its Badger never constructed, and every one of
      // its eleven errors was the resulting "badger is not defined".
      OnBeforeRequestOptions: { BLOCKING: "blocking", REQUEST_BODY: "requestBody" },
      OnBeforeSendHeadersOptions: { BLOCKING: "blocking", REQUEST_HEADERS: "requestHeaders" },
      OnHeadersReceivedOptions: { BLOCKING: "blocking", RESPONSE_HEADERS: "responseHeaders" },
      OnResponseStartedOptions: { RESPONSE_HEADERS: "responseHeaders" },
      OnCompletedOptions: { RESPONSE_HEADERS: "responseHeaders" },
      OnErrorOccurredOptions: {},
      OnAuthRequiredOptions: {
        BLOCKING: "blocking", RESPONSE_HEADERS: "responseHeaders",
        ASYNC_BLOCKING: "asyncBlocking",
      },
      MAX_HANDLER_BEHAVIOR_CHANGED_CALLS_PER_10_MINUTES: 20,
      onBeforeRequest: event(), onBeforeSendHeaders: event(), onSendHeaders: event(),
      onHeadersReceived: event(), onResponseStarted: event(), onBeforeRedirect: event(),
      onCompleted: event(), onErrorOccurred: event(), onAuthRequired: event(),
      handlerBehaviorChanged: (cb) => (cb ? cb() : Promise.resolve()),
    },

    declarativeNetRequest: {
      ResourceType: { MAIN_FRAME: "main_frame", SUB_FRAME: "sub_frame", SCRIPT: "script",
                      IMAGE: "image", STYLESHEET: "stylesheet", XMLHTTPREQUEST: "xmlhttprequest",
                      OTHER: "other" },
      RuleActionType: { BLOCK: "block", ALLOW: "allow", REDIRECT: "redirect",
                        UPGRADE_SCHEME: "upgradeScheme", MODIFY_HEADERS: "modifyHeaders" },
      updateDynamicRules: (_o, cb) => (cb ? cb() : Promise.resolve()),
      getDynamicRules: (cb) => (cb ? cb([]) : Promise.resolve([])),
      onRuleMatchedDebug: event(),
    },

    windows: {
      WINDOW_ID_CURRENT: -2, WINDOW_ID_NONE: -1,
      getCurrent: (_i, cb) => {
        const w = { id: 1, focused: true, type: "normal" };
        return cb ? cb(w) : Promise.resolve(w);
      },
      getAll: (_i, cb) => {
        const all = [{ id: 1, focused: true, type: "normal" }];
        return cb ? cb(all) : Promise.resolve(all);
      },
      create: (_i, cb) => (cb ? cb(null) : Promise.resolve(null)),
      update: (_i, _u, cb) => (cb ? cb(null) : Promise.resolve(null)),
      onCreated: event(), onRemoved: event(), onFocusChanged: event(),
    },

    permissions: {
      contains: (_p, cb) => (cb ? cb(true) : Promise.resolve(true)),
      request: (_p, cb) => (cb ? cb(true) : Promise.resolve(true)),
      getAll: (cb) => {
        const none = { permissions: [], origins: [] };
        return cb ? cb(none) : Promise.resolve(none);
      },
      remove: (_p, cb) => (cb ? cb(true) : Promise.resolve(true)),
      onAdded: event(), onRemoved: event(),
    },

    commands: {
      getAll: (cb) => (cb ? cb([]) : Promise.resolve([])),
      onCommand: event(),
    },

    cookies: {
      get: (_d, cb) => (cb ? cb(null) : Promise.resolve(null)),
      getAll: (_d, cb) => (cb ? cb([]) : Promise.resolve([])),
      set: (_d, cb) => (cb ? cb(null) : Promise.resolve(null)),
      remove: (_d, cb) => (cb ? cb(null) : Promise.resolve(null)),
      onChanged: event(),
    },

    extension: {
      getURL: (path) => `tela-extension://${extensionId}/${String(path).replace(/^\//, "")}`,
      getViews: () => [],
      getBackgroundPage: () => null,
      // Extensions branch on these at startup; a missing function is fatal
      // where a false answer merely disables a feature.
      isAllowedIncognitoAccess: (cb) => (cb ? cb(false) : Promise.resolve(false)),
      isAllowedFileSchemeAccess: (cb) => (cb ? cb(false) : Promise.resolve(false)),
      ViewType: { TAB: "tab", POPUP: "popup", SIDEBAR: "sidebar" },
      inIncognitoContext: false,
    },

    privacy: {
      network: { networkPredictionEnabled: setting(), webRTCIPHandlingPolicy: setting() },
      websites: { thirdPartyCookiesAllowed: setting(), hyperlinkAuditingEnabled: setting(),
                  referrersEnabled: setting(), trackingProtectionMode: setting() },
    },

    dns: {
      resolve: (_h, _f, cb) => (cb ? cb(null) : Promise.resolve(null)),
    },

    theme: {
      getCurrent: (cb) => (cb ? cb({}) : Promise.resolve({})),
      update: () => Promise.resolve(),
      reset: () => Promise.resolve(),
      onUpdated: event(),
    },

    downloads: {
      download: (_o, cb) => (cb ? cb(-1) : Promise.resolve(-1)),
      search: (_q, cb) => (cb ? cb([]) : Promise.resolve([])),
      onCreated: event(), onChanged: event(),
    },

    // ── Tela-specific ────────────────────────────────────────────────
    // Not part of WebExtensions. Namespaced separately so it is obvious an
    // extension using it is targeting this browser, and so nothing here can be
    // mistaken for a standard API that other browsers implement.
    tela: {
      applySystemTheme(seed) {
        return withHost("applySystemTheme", [extensionId, String(seed)],
                        (h) => h.applySystemTheme(extensionId, String(seed)))
          .then((json) => {
            try { return JSON.parse(json); }
            catch (e) { return { ok: false, message: "no response" }; }
          });
      },
    },

    i18n: {
      // Real message resolution happens host-side at install; this covers
      // runtime lookups with a readable fallback.
      getMessage: (key) => (window.__telaMessages || {})[key]?.message || "",
      getUILanguage: () => (navigator.language || "en-US"),
      getAcceptLanguages: (callback) => {
        const langs = navigator.languages && navigator.languages.length
          ? Array.from(navigator.languages) : ["en-US"];
        return typeof callback === "function" ? callback(langs) : Promise.resolve(langs);
      },
      detectLanguage: (_text, callback) => {
        const result = { isReliable: false, languages: [] };
        return typeof callback === "function" ? callback(result) : Promise.resolve(result);
      },
    },
  };

    // Firefox exposes the same namespace under both names.
    api.menus = api.contextMenus;
    delete api.__gated;

    // Remove whatever this extension did not ask for. `grants === null` means
    // the caller supplied no list at all — an older injection path — and is
    // treated as "ungated" rather than "nothing granted", so a missing list
    // cannot silently disable every extension at once.
    if (granted) {
      for (const namespace of Object.keys(REQUIRES)) {
        if (granted.indexOf(REQUIRES[namespace]) < 0) {
          delete api[namespace];
        }
      }
      if (!api.contextMenus) delete api.menus;
    }
    return api;
  };

  // An uncaught error in an extension page kills whatever was building its UI,
  // and the page just stays blank. The console message alone names the document
  // and a line, which is rarely where the fault is — the stack is. Reported to
  // the console, so it lands on the host's `page` / `ext` debug channels rather
  // than being printed unconditionally.
  window.addEventListener("error", function (event) {
    const stack = event.error && event.error.stack ? "\n" + event.error.stack : "";
    console.error("tela:uncaught " + (event.message || "")
                  + " @ " + (event.filename || "?") + ":" + (event.lineno || 0) + stack);
  });

  window.addEventListener("unhandledrejection", function (event) {
    const reason = event.reason;
    const stack = reason && reason.stack ? "\n" + reason.stack : "";
    console.error("tela:unhandled " + (reason && reason.message ? reason.message : String(reason))
                  + stack);
  });

  // A page may only mint its *own* browser.*. An extension page runs this shim
  // in the main world, so any script it loads could otherwise mint another
  // extension's API — with `null` grants, skipping permission gating — and read
  // its storage. Conditional because a content-script world has no single
  // identity: several extensions share it, and none set __telaExtensionId.
  window.__telaMakeApi = function (extensionId, grants) {
    const own = window.__telaExtensionId;
    if (own && extensionId !== own) {
      console.error("tela:refused browser.* for " + extensionId
                    + " requested on " + own + "'s page");
      return undefined;
    }
    return makeApi(extensionId, grants);
  };

  // Resolve this context's tab id up front: a message sent before it is known
  // would carry tabId -1, and extensions route on that.
  if (!hasChannel) currentTabId();

  // The background page has exactly one identity, so it gets the globals an
  // extension expects. Content scripts each bind their own instead.
  if (window.__telaExtensionId) {
    // Extension contexts only: a content script shares its world with other
    // extensions, and a web page must never get this.
    installHostNetwork(window.__telaExtensionId);
    const api = window.__telaMakeApi(window.__telaExtensionId, window.__telaGrants);
    window.browser = api;
    if (!window.chrome || !window.chrome.runtime) window.chrome = api;
  }
})();
"#;

#[cfg(test)]
mod tests {
    use super::heal;
    use serde_json::json;

    #[test]
    fn drops_the_self_referential_managed_cache() {
        // Shaped like the real thing: the cache holds a copy of the store it
        // lives in, which is what made the file grow without bound.
        let mut store = json!({
            "selectedFilterLists": ["ublock-filters"],
            "cachedManagedStorage": { "selectedFilterLists": ["ublock-filters"] },
        })
        .as_object()
        .cloned()
        .expect("object");

        assert!(heal(&mut store));
        assert!(!store.contains_key("cachedManagedStorage"));
        // Everything the user actually chose survives.
        assert_eq!(store["selectedFilterLists"], json!(["ublock-filters"]));
    }

    #[test]
    fn leaves_a_healthy_store_alone() {
        let mut store = json!({ "selectedFilterLists": ["ublock-filters"] })
            .as_object()
            .cloned()
            .expect("object");

        // No rewrite when there is nothing to fix: a clean store must not be
        // republished to disk on every load.
        assert!(!heal(&mut store));
        assert_eq!(store.len(), 1);
    }
}
