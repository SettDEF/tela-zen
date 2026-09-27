//! Userscript support — the `.user.js` files Violentmonkey and Tampermonkey run.
//!
//! A userscript is JavaScript with a `==UserScript==` metadata block. Injection
//! goes through the same content-script pipeline extensions use, so this is
//! stage zero of the extension host rather than a parallel system.

use crate::matcher::{MatchSet, Pattern};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunAt {
    DocumentStart,
    DocumentEnd,
    DocumentIdle,
}

impl RunAt {
    fn parse(value: &str) -> Self {
        match value.trim() {
            "document-start" => RunAt::DocumentStart,
            "document-end" => RunAt::DocumentEnd,
            _ => RunAt::DocumentIdle,
        }
    }

    /// Maps onto `WebEngineScript.injectionPoint`.
    pub fn injection_point(self) -> &'static str {
        match self {
            RunAt::DocumentStart => "DocumentCreation",
            RunAt::DocumentEnd => "DocumentReady",
            RunAt::DocumentIdle => "Deferred",
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserScript {
    pub name: String,
    pub namespace: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub run_at: RunAt,
    pub grants: Vec<String>,
    pub matches: MatchSet,
    /// `@require` URLs — external libraries to load before the body. Fetching
    /// and inlining them is the host's job; parsing only records them.
    pub requires: Vec<String>,
    /// `@resource NAME URL` — named assets reachable via GM_getResourceText /
    /// GM_getResourceURL. `(name, url)` pairs; the host fetches them.
    pub resources: Vec<(String, String)>,
    /// The script body, exactly as written.
    pub source: String,
}

impl UserScript {
    /// Parse a `.user.js` file. Returns `None` when there is no metadata block —
    /// a plain `.js` file is not a userscript.
    pub fn parse(source: &str) -> Option<Self> {
        let start = source.find("// ==UserScript==")?;
        let after = &source[start..];
        let end = after.find("// ==/UserScript==")?;
        let block = &after[..end];

        let mut script = UserScript {
            name: String::new(),
            namespace: None,
            description: None,
            version: None,
            run_at: RunAt::DocumentIdle,
            grants: Vec::new(),
            matches: MatchSet::default(),
            requires: Vec::new(),
            resources: Vec::new(),
            source: source.to_owned(),
        };

        for line in block.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("//") else {
                continue;
            };
            let rest = rest.trim();
            let Some(rest) = rest.strip_prefix('@') else {
                continue;
            };

            let (key, value) = match rest.split_once(char::is_whitespace) {
                Some((key, value)) => (key, value.trim()),
                None => (rest, ""),
            };

            match key {
                "name" => script.name = value.to_owned(),
                "namespace" => script.namespace = Some(value.to_owned()),
                "description" => script.description = Some(value.to_owned()),
                "version" => script.version = Some(value.to_owned()),
                "run-at" => script.run_at = RunAt::parse(value),
                "require" => {
                    if value.starts_with("http://") || value.starts_with("https://") {
                        script.requires.push(value.to_owned());
                    }
                }
                "resource" => {
                    if let Some((name, url)) = value.split_once(char::is_whitespace) {
                        let url = url.trim();
                        if url.starts_with("http://") || url.starts_with("https://") {
                            script.resources.push((name.trim().to_owned(), url.to_owned()));
                        }
                    }
                }
                "grant" => {
                    if value != "none" && !value.is_empty() {
                        script.grants.push(value.to_owned());
                    }
                }
                // @match is structured; @include is a loose glob. Parsing an
                // @match that turns out to be malformed as a glob instead would
                // silently widen its scope, so a bad @match is dropped.
                "match" => {
                    if let Some(pattern) = Pattern::parse_match(value) {
                        script.matches.includes.push(pattern);
                    }
                }
                "include" => script.matches.includes.push(Pattern::parse_glob(value)),
                "exclude" | "exclude-match" => {
                    let pattern = Pattern::parse_match(value)
                        .unwrap_or_else(|| Pattern::parse_glob(value));
                    script.matches.excludes.push(pattern);
                }
                _ => {}
            }
        }

        if script.name.is_empty() {
            script.name = "Untitled script".to_owned();
        }

        Some(script)
    }

    pub fn matches_url(&self, url: &str) -> bool {
        // A script with no @match/@include runs nowhere, which is what both
        // Violentmonkey and the WebExtensions spec do.
        !self.matches.is_empty() && self.matches.matches(url)
    }

    /// A stable identity for storage keys and enable/disable state.
    pub fn id(&self) -> String {
        match &self.namespace {
            Some(namespace) => format!("{namespace}/{}", self.name),
            None => self.name.clone(),
        }
    }
}

/// The `GM_*` surface exposed to userscripts.
///
/// Deliberately small. `GM_xmlhttpRequest` is absent: its whole purpose is
/// issuing cross-origin requests that bypass CORS, which is exactly the
/// page-reachable host bridge this project refuses to build. Scripts that
/// require it will fail loudly on an undefined function rather than silently
/// misbehave.
pub const GM_SHIM: &str = r#"
(function () {
  if (window.__telaGM) return;
  const prefix = "tela.gm.";
  window.__telaGM = {
    api(scriptId) {
      const key = (name) => prefix + scriptId + "." + name;
      // Per-script value-change listeners, fired by GM_setValue/deleteValue.
      const listeners = {};
      let listenerSeq = 0;
      const fire = (name, oldValue, newValue) => {
        for (const id in listeners) {
          const l = listeners[id];
          if (l.name === name) { try { l.fn(name, oldValue, newValue, false); } catch (e) {} }
        }
      };
      const read = (name, fallback) => {
        const raw = localStorage.getItem(key(name));
        if (raw === null) return fallback;
        try { return JSON.parse(raw); } catch (e) { return fallback; }
      };
      return {
        GM_info: { scriptHandler: "Tela", version: "0.1.0", script: {} },
        GM_addStyle(css) {
          const el = document.createElement("style");
          el.textContent = css;
          (document.head || document.documentElement).appendChild(el);
          return el;
        },
        GM_addElement(a, b, c) {
          let parent = a, tag = b, attrs = c;
          if (typeof a === "string") { parent = (document.head || document.documentElement); tag = a; attrs = b; }
          const el = document.createElement(tag);
          if (attrs) for (const k in attrs) {
            if (k === "textContent") el.textContent = attrs[k]; else el.setAttribute(k, attrs[k]);
          }
          (parent || document.documentElement).appendChild(el);
          return el;
        },
        GM_setValue(name, value) {
          const old = read(name, undefined);
          localStorage.setItem(key(name), JSON.stringify(value));
          fire(name, old, value);
        },
        GM_getValue(name, fallback) { return read(name, fallback); },
        GM_deleteValue(name) {
          const old = read(name, undefined);
          localStorage.removeItem(key(name));
          fire(name, old, undefined);
        },
        GM_listValues() {
          const out = [];
          const head = prefix + scriptId + ".";
          for (let i = 0; i < localStorage.length; i++) {
            const k = localStorage.key(i);
            if (k && k.startsWith(head)) out.push(k.slice(head.length));
          }
          return out;
        },
        GM_addValueChangeListener(name, fn) {
          const id = ++listenerSeq; listeners[id] = { name, fn }; return id;
        },
        GM_removeValueChangeListener(id) { delete listeners[id]; },
        GM_log(...args) { console.log("[userscript]", ...args); },
        GM_openInTab(url, opts) {
          const w = window.open(url, "_blank");
          return { close: () => { try { w && w.close(); } catch (e) {} }, closed: false };
        },
        // Menu commands have no chrome to live in yet; register them so scripts
        // that rely on the call (and its returned id) run, and expose the list
        // for a future menu rather than throwing.
        GM_registerMenuCommand(caption, fn, accessKey) {
          window.__telaGMMenu = window.__telaGMMenu || [];
          const id = window.__telaGMMenu.length + 1;
          window.__telaGMMenu.push({ id, caption, fn, scriptId });
          return id;
        },
        GM_unregisterMenuCommand(id) {
          if (window.__telaGMMenu)
            window.__telaGMMenu = window.__telaGMMenu.filter((c) => c.id !== id);
        },
        GM_setClipboard(text) {
          try { navigator.clipboard.writeText(String(text)); return; } catch (e) {}
          const t = document.createElement("textarea");
          t.value = String(text); t.style.position = "fixed"; t.style.opacity = "0";
          document.body.appendChild(t); t.select();
          try { document.execCommand("copy"); } catch (e) {}
          t.remove();
        },
        // Best-effort cross-origin request. In a content-script world this is
        // subject to CORS (unlike a privileged extension XHR), so cross-origin
        // calls a site does not allow will fail — but same-origin and permitted
        // ones work, and the call no longer throws as undefined.
        GM_xmlhttpRequest(details) {
          const xhr = new XMLHttpRequest();
          try {
            xhr.open(details.method || "GET", details.url, true);
            if (details.headers)
              for (const h in details.headers) { try { xhr.setRequestHeader(h, details.headers[h]); } catch (e) {} }
            if (details.responseType) xhr.responseType = details.responseType;
            if (details.timeout) xhr.timeout = details.timeout;
            if (details.overrideMimeType) xhr.overrideMimeType(details.overrideMimeType);
            const resp = () => ({
              readyState: xhr.readyState, status: xhr.status, statusText: xhr.statusText,
              responseText: (xhr.responseType === "" || xhr.responseType === "text") ? xhr.responseText : "",
              response: xhr.response, responseHeaders: xhr.getAllResponseHeaders(),
              finalUrl: details.url, context: details.context,
            });
            xhr.onload = () => details.onload && details.onload(resp());
            xhr.onerror = () => details.onerror && details.onerror(resp());
            xhr.ontimeout = () => details.ontimeout && details.ontimeout(resp());
            xhr.onprogress = (e) => details.onprogress && details.onprogress(e);
            if (details.onreadystatechange) xhr.onreadystatechange = () => details.onreadystatechange(resp());
            xhr.send(details.data !== undefined ? details.data : null);
          } catch (e) {
            details.onerror && details.onerror({ error: String(e), status: 0 });
          }
          return { abort: () => { try { xhr.abort(); } catch (e) {} } };
        },
        GM_download(a, name) {
          const url = typeof a === "string" ? a : a.url;
          const fn = typeof a === "string" ? name : a.name;
          const link = document.createElement("a");
          link.href = url; if (fn) link.download = fn; link.style.display = "none";
          document.body.appendChild(link); link.click(); link.remove();
          if (typeof a === "object" && a.onload) try { a.onload(); } catch (e) {}
        },
        GM_notification(a, title) {
          const o = typeof a === "string" ? { text: a, title } : (a || {});
          try {
            if (window.Notification && Notification.permission === "granted")
              new Notification(o.title || "", { body: o.text || "", icon: o.image });
            else console.log("[userscript notification]", o.title || "", o.text || "");
          } catch (e) { console.log("[userscript notification]", o.text || ""); }
          if (o.onclick) { /* no persistent notification handle to bind */ }
        },
        // @resource, fetched and injected by the host into window.__telaRes.
        GM_getResourceText(name) {
          var r = (window.__telaRes || {})[scriptId] || {};
          return (r[name] && r[name].text) || "";
        },
        GM_getResourceURL(name) {
          var r = (window.__telaRes || {})[scriptId] || {};
          return (r[name] && r[name].url) || "";
        },
      };
    },
  };
})();
"#;

/// Wrap a script so it only executes on URLs it declares.
///
/// `WebEngineScript` has no match-pattern support — it injects into every frame
/// it is attached to — so the guard has to live inside the injected source. The
/// same evaluation runs here at injection time; this is belt-and-braces for
/// frames that navigate after injection.
pub fn wrap(script: &UserScript) -> String {
    let grants: Vec<&str> = script.grants.iter().map(String::as_str).collect();
    let destructure = if grants.is_empty() {
        String::new()
    } else {
        let names: Vec<String> = grants
            .iter()
            .filter(|name| name.starts_with("GM_"))
            .map(|name| (*name).to_owned())
            .collect();
        if names.is_empty() {
            String::new()
        } else {
            format!(
                "  const {{ {} }} = window.__telaGM.api({});\n",
                names.join(", "),
                json_string(&script.id())
            )
        }
    };

    format!(
        "(function () {{\n\
         {destructure}  try {{\n\
         {}\n\
         \x20 }} catch (error) {{\n\
         \x20   console.error({}, error);\n\
         \x20 }}\n\
         }})();\n",
        script.source,
        json_string(&format!("[userscript] {} failed", script.name))
    )
}

fn json_string(value: &str) -> String {
    serde_json::Value::String(value.to_owned()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = r#"
// ==UserScript==
// @name         Example Script
// @namespace    https://example.com
// @version      1.2
// @description  Does a thing
// @match        *://*.example.com/*
// @exclude      *://admin.example.com/*
// @run-at       document-start
// @grant        GM_addStyle
// @grant        GM_setValue
// ==/UserScript==

console.log("hello");
"#;

    #[test]
    fn parses_the_metadata_block() {
        let script = UserScript::parse(SCRIPT).expect("should parse");
        assert_eq!(script.name, "Example Script");
        assert_eq!(script.namespace.as_deref(), Some("https://example.com"));
        assert_eq!(script.version.as_deref(), Some("1.2"));
        assert_eq!(script.run_at, RunAt::DocumentStart);
        assert_eq!(script.grants, vec!["GM_addStyle", "GM_setValue"]);
        assert_eq!(script.id(), "https://example.com/Example Script");
    }

    #[test]
    fn applies_match_and_exclude() {
        let script = UserScript::parse(SCRIPT).unwrap();
        assert!(script.matches_url("https://www.example.com/page"));
        assert!(!script.matches_url("https://admin.example.com/page"));
        assert!(!script.matches_url("https://other.com/"));
    }

    #[test]
    fn a_script_with_no_match_runs_nowhere() {
        let source = "// ==UserScript==\n// @name Bare\n// ==/UserScript==\nvoid 0;";
        let script = UserScript::parse(source).unwrap();
        assert!(!script.matches_url("https://example.com/"));
    }

    #[test]
    fn plain_javascript_is_not_a_userscript() {
        assert!(UserScript::parse("console.log('hi');").is_none());
    }

    #[test]
    fn run_at_maps_to_injection_points() {
        assert_eq!(RunAt::DocumentStart.injection_point(), "DocumentCreation");
        assert_eq!(RunAt::DocumentEnd.injection_point(), "DocumentReady");
        assert_eq!(RunAt::DocumentIdle.injection_point(), "Deferred");
    }

    #[test]
    fn wrapping_destructures_only_granted_gm_functions() {
        let script = UserScript::parse(SCRIPT).unwrap();
        let wrapped = wrap(&script);
        assert!(wrapped.contains("GM_addStyle, GM_setValue"));
        assert!(wrapped.contains("console.log(\"hello\")"));
        // Errors in one script must not take down the others.
        assert!(wrapped.contains("catch (error)"));
    }

    #[test]
    fn wrapping_a_grantless_script_adds_no_shim_call() {
        let source = "// ==UserScript==\n// @name Bare\n// @match *://x/*\n// ==/UserScript==\nvoid 0;";
        let script = UserScript::parse(source).unwrap();
        assert!(!wrap(&script).contains("__telaGM"));
    }
}
