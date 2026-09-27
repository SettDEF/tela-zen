//! `manifest.json` parsing, normalising MV2 and MV3 into one model.
//!
//! Firefox XPIs are usually MV2, Chrome CRXs increasingly MV3, and the two
//! spell the same concepts differently (`browser_action` vs `action`,
//! `background.scripts` vs `background.service_worker`, host permissions inside
//! `permissions` vs split into `host_permissions`). Everything downstream reads
//! this normalised form so the differences stop here.

use serde_json::Value;

use crate::matcher::{MatchSet, Pattern};
use crate::userscript::RunAt;

#[derive(Debug, Clone)]
pub struct ContentScript {
    pub matches: MatchSet,
    pub js: Vec<String>,
    pub css: Vec<String>,
    pub run_at: RunAt,
    pub all_frames: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Background {
    /// MV2 event/background page scripts.
    pub scripts: Vec<String>,
    /// MV3 service worker entry point.
    pub service_worker: Option<String>,
    /// MV2 explicit background page.
    pub page: Option<String>,
    pub persistent: bool,
}

impl Background {
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty() && self.service_worker.is_none() && self.page.is_none()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Action {
    pub default_popup: Option<String>,
    pub default_title: Option<String>,
}

/// Something the extension asks for that this host cannot honour.
#[derive(Debug, Clone, PartialEq)]
pub struct CompatIssue {
    pub feature: String,
    pub severity: Severity,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The extension will not work in any meaningful way.
    Blocking,
    /// The extension loads, but this capability is missing or reduced.
    Degraded,
}

#[derive(Debug, Clone)]
pub struct Manifest {
    pub manifest_version: u8,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub default_locale: Option<String>,
    pub content_scripts: Vec<ContentScript>,
    pub background: Background,
    pub action: Option<Action>,
    pub options_page: Option<String>,
    pub permissions: Vec<String>,
    pub host_permissions: Vec<String>,
    pub web_accessible_resources: Vec<String>,
}

fn string_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Host permissions are written as match patterns; anything that parses as one
/// is a host permission rather than an API permission.
fn looks_like_host_pattern(value: &str) -> bool {
    value == "<all_urls>" || value.contains("://")
}

impl Manifest {
    pub fn parse(source: &str) -> Option<Self> {
        let json: Value = serde_json::from_str(source).ok()?;

        let manifest_version = json["manifest_version"].as_u64().unwrap_or(2) as u8;
        let name = json["name"].as_str()?.to_owned();
        let version = json["version"].as_str().unwrap_or("0").to_owned();

        // MV2 mixes host patterns into `permissions`; MV3 splits them out.
        let raw_permissions = string_list(&json["permissions"]);
        let mut permissions = Vec::new();
        let mut host_permissions = string_list(&json["host_permissions"]);
        for permission in raw_permissions {
            if looks_like_host_pattern(&permission) {
                host_permissions.push(permission);
            } else {
                permissions.push(permission);
            }
        }

        let mut content_scripts = Vec::new();
        if let Some(entries) = json["content_scripts"].as_array() {
            for entry in entries {
                let mut matches = MatchSet::default();
                for pattern in string_list(&entry["matches"]) {
                    if let Some(parsed) = Pattern::parse_match(&pattern) {
                        matches.includes.push(parsed);
                    }
                }
                for pattern in string_list(&entry["exclude_matches"]) {
                    if let Some(parsed) = Pattern::parse_match(&pattern) {
                        matches.excludes.push(parsed);
                    }
                }
                // `include_globs`/`exclude_globs` are loose globs, not patterns.
                for glob in string_list(&entry["include_globs"]) {
                    matches.includes.push(Pattern::parse_glob(&glob));
                }
                for glob in string_list(&entry["exclude_globs"]) {
                    matches.excludes.push(Pattern::parse_glob(&glob));
                }

                content_scripts.push(ContentScript {
                    matches,
                    js: string_list(&entry["js"]),
                    css: string_list(&entry["css"]),
                    run_at: match entry["run_at"].as_str() {
                        Some("document_start") => RunAt::DocumentStart,
                        Some("document_end") => RunAt::DocumentEnd,
                        _ => RunAt::DocumentIdle,
                    },
                    all_frames: entry["all_frames"].as_bool().unwrap_or(false),
                });
            }
        }

        let background_value = &json["background"];
        let background = Background {
            scripts: string_list(&background_value["scripts"]),
            service_worker: background_value["service_worker"]
                .as_str()
                .map(str::to_owned),
            page: background_value["page"].as_str().map(str::to_owned),
            persistent: background_value["persistent"].as_bool().unwrap_or(false),
        };

        // MV3 renamed browser_action/page_action to action.
        let action_value = if json["action"].is_object() {
            &json["action"]
        } else if json["browser_action"].is_object() {
            &json["browser_action"]
        } else {
            &json["page_action"]
        };
        let action = if action_value.is_object() {
            Some(Action {
                default_popup: action_value["default_popup"].as_str().map(str::to_owned),
                default_title: action_value["default_title"].as_str().map(str::to_owned),
            })
        } else {
            None
        };

        let options_page = json["options_ui"]["page"]
            .as_str()
            .or_else(|| json["options_page"].as_str())
            .map(str::to_owned);

        // MV3 wraps web accessible resources in objects with match lists.
        let mut web_accessible_resources = string_list(&json["web_accessible_resources"]);
        if let Some(entries) = json["web_accessible_resources"].as_array() {
            for entry in entries {
                if entry.is_object() {
                    web_accessible_resources.extend(string_list(&entry["resources"]));
                }
            }
        }

        Some(Manifest {
            manifest_version,
            name,
            version,
            description: json["description"].as_str().map(str::to_owned),
            default_locale: json["default_locale"].as_str().map(str::to_owned),
            content_scripts,
            background,
            action,
            options_page,
            permissions,
            host_permissions,
            web_accessible_resources,
        })
    }

    /// Resolve `__MSG_name__` placeholders against the bundle's `_locales`.
    ///
    /// Real extensions localise their own name and description, so a manifest
    /// read straight from disk often has placeholders where the display strings
    /// should be. Falls back to English, then to any available locale, because
    /// showing `__MSG_extName__` in a list is worse than showing the wrong
    /// language.
    pub fn resolve_messages(&mut self, bundle_dir: &std::path::Path) {
        let locales = bundle_dir.join("_locales");
        if !locales.is_dir() {
            return;
        }

        let mut candidates: Vec<String> = Vec::new();
        if let Some(default) = &self.default_locale {
            candidates.push(default.clone());
        }
        candidates.push("en_US".to_owned());
        candidates.push("en".to_owned());
        if let Ok(entries) = std::fs::read_dir(&locales) {
            for entry in entries.filter_map(Result::ok) {
                candidates.push(entry.file_name().to_string_lossy().into_owned());
            }
        }

        let mut messages = None;
        for candidate in candidates {
            let path = locales.join(&candidate).join("messages.json");
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<Value>(&text) {
                    messages = Some(json);
                    break;
                }
            }
        }

        let Some(messages) = messages else {
            return;
        };

        let lookup = |value: &str| -> Option<String> {
            let key = value.strip_prefix("__MSG_")?.strip_suffix("__")?;
            messages[key]["message"].as_str().map(str::to_owned)
        };

        if let Some(resolved) = lookup(&self.name) {
            self.name = resolved;
        }
        if let Some(description) = &self.description {
            if let Some(resolved) = lookup(description) {
                self.description = Some(resolved);
            }
        }
    }

    /// What this host cannot deliver for this extension.
    ///
    /// Surfaced in the UI at install time: an extension that silently half-works
    /// is worse than one that says up front which parts are missing.
    pub fn compatibility(&self) -> Vec<CompatIssue> {
        let mut issues = Vec::new();
        let has = |name: &str| self.permissions.iter().any(|p| p == name);

        if has("webRequestBlocking") || has("webRequest") {
            issues.push(CompatIssue {
                feature: "webRequest".to_owned(),
                severity: Severity::Degraded,
                detail: "Requests can be cancelled and redirected, but response \
                         bodies cannot be rewritten: filterResponseData is a \
                         Gecko-only API with no Chromium equivalent."
                    .to_owned(),
            });
        }

        if has("nativeMessaging") {
            issues.push(CompatIssue {
                feature: "nativeMessaging".to_owned(),
                severity: Severity::Blocking,
                detail: "Launching native host processes is deliberately not \
                         implemented — it is arbitrary code execution reachable \
                         from extension code."
                    .to_owned(),
            });
        }

        if has("devtools") || self.permissions.iter().any(|p| p.starts_with("devtools")) {
            issues.push(CompatIssue {
                feature: "devtools".to_owned(),
                severity: Severity::Blocking,
                detail: "Devtools panel APIs are not hosted.".to_owned(),
            });
        }

        if self.background.service_worker.is_some() {
            issues.push(CompatIssue {
                feature: "background.service_worker".to_owned(),
                severity: Severity::Degraded,
                detail: "Runs as a persistent hidden page rather than a real \
                         service worker; lifecycle events differ."
                    .to_owned(),
            });
        }

        if has("proxy") {
            issues.push(CompatIssue {
                feature: "proxy".to_owned(),
                severity: Severity::Blocking,
                detail: "Per-extension proxy configuration is not exposed.".to_owned(),
            });
        }

        issues
    }

    /// True when nothing blocking stands in the way of loading it.
    pub fn is_loadable(&self) -> bool {
        !self
            .compatibility()
            .iter()
            .any(|issue| issue.severity == Severity::Blocking)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MV2: &str = r#"{
      "manifest_version": 2,
      "name": "Test MV2",
      "version": "1.0",
      "permissions": ["storage", "tabs", "webRequest", "webRequestBlocking", "*://*.example.com/*"],
      "background": { "scripts": ["bg.js"], "persistent": true },
      "browser_action": { "default_popup": "popup.html", "default_title": "Test" },
      "options_page": "options.html",
      "content_scripts": [{
        "matches": ["*://*.example.com/*"],
        "exclude_matches": ["*://admin.example.com/*"],
        "js": ["content.js"],
        "css": ["content.css"],
        "run_at": "document_start",
        "all_frames": true
      }]
    }"#;

    const MV3: &str = r#"{
      "manifest_version": 3,
      "name": "Test MV3",
      "version": "2.0",
      "permissions": ["storage", "declarativeNetRequest"],
      "host_permissions": ["<all_urls>"],
      "background": { "service_worker": "sw.js" },
      "action": { "default_popup": "popup.html" },
      "options_ui": { "page": "options.html" },
      "web_accessible_resources": [{ "resources": ["inject.js"], "matches": ["<all_urls>"] }]
    }"#;

    #[test]
    fn parses_mv2() {
        let manifest = Manifest::parse(MV2).expect("should parse");
        assert_eq!(manifest.manifest_version, 2);
        assert_eq!(manifest.name, "Test MV2");
        assert_eq!(manifest.background.scripts, vec!["bg.js"]);
        assert!(manifest.background.persistent);
        assert_eq!(
            manifest.action.as_ref().unwrap().default_popup.as_deref(),
            Some("popup.html")
        );
        assert_eq!(manifest.options_page.as_deref(), Some("options.html"));
    }

    #[test]
    fn splits_host_patterns_out_of_mv2_permissions() {
        let manifest = Manifest::parse(MV2).unwrap();
        assert!(manifest.permissions.contains(&"storage".to_owned()));
        assert!(!manifest.permissions.iter().any(|p| p.contains("://")));
        assert_eq!(manifest.host_permissions, vec!["*://*.example.com/*"]);
    }

    #[test]
    fn parses_content_scripts_with_matching() {
        let manifest = Manifest::parse(MV2).unwrap();
        let script = &manifest.content_scripts[0];
        assert_eq!(script.js, vec!["content.js"]);
        assert_eq!(script.run_at, RunAt::DocumentStart);
        assert!(script.all_frames);
        assert!(script.matches.matches("https://www.example.com/x"));
        assert!(!script.matches.matches("https://admin.example.com/x"));
    }

    #[test]
    fn normalises_mv3_action_and_worker() {
        let manifest = Manifest::parse(MV3).unwrap();
        assert_eq!(manifest.manifest_version, 3);
        assert_eq!(manifest.background.service_worker.as_deref(), Some("sw.js"));
        assert_eq!(
            manifest.action.as_ref().unwrap().default_popup.as_deref(),
            Some("popup.html")
        );
        assert_eq!(manifest.options_page.as_deref(), Some("options.html"));
        assert_eq!(manifest.web_accessible_resources, vec!["inject.js"]);
        assert_eq!(manifest.host_permissions, vec!["<all_urls>"]);
    }

    #[test]
    fn reports_webrequest_as_degraded_not_blocking() {
        let manifest = Manifest::parse(MV2).unwrap();
        let issues = manifest.compatibility();
        let issue = issues.iter().find(|i| i.feature == "webRequest").unwrap();
        assert_eq!(issue.severity, Severity::Degraded);
        assert!(manifest.is_loadable());
    }

    #[test]
    fn native_messaging_blocks_loading() {
        let source = r#"{"manifest_version":2,"name":"N","version":"1",
                         "permissions":["nativeMessaging"]}"#;
        let manifest = Manifest::parse(source).unwrap();
        assert!(!manifest.is_loadable());
        assert_eq!(manifest.compatibility()[0].severity, Severity::Blocking);
    }

    #[test]
    fn rejects_a_manifest_with_no_name() {
        assert!(Manifest::parse(r#"{"manifest_version":3,"version":"1"}"#).is_none());
        assert!(Manifest::parse("not json").is_none());
    }
}
