//! Session persistence — the tabs you had open survive a restart.
//!
//! Written atomically on every change, because the case that matters is the one
//! where the browser did *not* shut down cleanly. A save-on-exit design loses
//! exactly the sessions people care about recovering.
//!
//! The previous session is also kept, so a restore that goes wrong is
//! recoverable rather than a silent loss.

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
        /// Tabs from the last run, as JSON. Empty array when there is nothing
        /// to restore.
        #[qproperty(QString, restored_json)]
        #[qproperty(i32, restored_count)]
        /// Index that was active last time, clamped to the restored range.
        #[qproperty(i32, restored_index)]
        type Session = super::SessionRust;

        /// Read the stored session. Safe to call before anything is saved.
        #[qinvokable]
        fn load(self: Pin<&mut Session>) -> bool;

        /// Persist the current tabs. `tabs` is a JSON array of
        /// `{title, url, pinned}`; `active` is the selected index.
        #[qinvokable]
        fn save(self: Pin<&mut Session>, tabs: &QString, active: i32) -> bool;

        /// Forget the stored session without touching the backup.
        #[qinvokable]
        fn clear(self: Pin<&mut Session>) -> bool;
    }
}

#[derive(Default)]
pub struct SessionRust {
    restored_json: QString,
    restored_count: i32,
    restored_index: i32,
}

fn session_path() -> Option<PathBuf> {
    crate::config::config_dir().map(|dir| dir.join("session.json"))
}

fn backup_path() -> Option<PathBuf> {
    crate::config::config_dir().map(|dir| dir.join("session.previous.json"))
}

/// Tabs worth restoring. A blank or start-page tab carries no information, and
/// restoring a window full of them is worse than restoring nothing.
fn is_restorable(url: &str) -> bool {
    let url = url.trim();
    !(url.is_empty()
        || url == "about:blank"
        || url == "tela://start"
        || url.starts_with("tela-extension://"))
}

impl ffi::Session {
    fn load(mut self: Pin<&mut Self>) -> bool {
        let Some(path) = session_path() else {
            return false;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            self.as_mut().set_restored_json(QString::from("[]"));
            return false;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            return false;
        };

        let tabs: Vec<serde_json::Value> = json["tabs"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter(|tab| tab["url"].as_str().map(is_restorable).unwrap_or(false))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        let active = json["active"].as_i64().unwrap_or(0) as i32;
        let count = tabs.len() as i32;
        let encoded = serde_json::to_string(&tabs).unwrap_or_else(|_| "[]".to_owned());

        self.as_mut().set_restored_json(QString::from(&encoded));
        self.as_mut().set_restored_count(count);
        // A stored index can outlive the tab it pointed at.
        self.as_mut()
            .set_restored_index(active.clamp(0, (count - 1).max(0)));
        count > 0
    }

    fn save(self: Pin<&mut Self>, tabs: &QString, active: i32) -> bool {
        let Some(path) = session_path() else {
            return false;
        };

        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&tabs.to_string()) else {
            return false;
        };
        let Some(list) = parsed.as_array() else {
            return false;
        };

        let keep: Vec<&serde_json::Value> = list
            .iter()
            .filter(|tab| tab["url"].as_str().map(is_restorable).unwrap_or(false))
            .collect();

        // Nothing worth keeping: remove the file rather than leaving a stale
        // session that would restore over a deliberate fresh start.
        if keep.is_empty() {
            let _ = std::fs::remove_file(&path);
            return true;
        }

        let payload = serde_json::json!({
            "version": 1,
            "active": active.max(0).min((keep.len() as i32 - 1).max(0)),
            "tabs": keep,
        });

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Keep the last good session before overwriting, so a bad write or a
        // restore that goes wrong is recoverable by hand.
        if let Some(backup) = backup_path() {
            let _ = std::fs::copy(&path, backup);
        }

        let temp = path.with_extension("json.tmp");
        let encoded = serde_json::to_string_pretty(&payload).unwrap_or_default();
        if std::fs::write(&temp, encoded).is_err() {
            return false;
        }
        if std::fs::rename(&temp, &path).is_err() {
            let _ = std::fs::remove_file(&temp);
            return false;
        }
        true
    }

    fn clear(mut self: Pin<&mut Self>) -> bool {
        if let Some(path) = session_path() {
            let _ = std::fs::remove_file(path);
        }
        self.as_mut().set_restored_json(QString::from("[]"));
        self.as_mut().set_restored_count(0);
        self.as_mut().set_restored_index(0);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::is_restorable;

    #[test]
    fn keeps_real_pages() {
        assert!(is_restorable("https://example.com/a"));
        assert!(is_restorable("http://localhost:8080"));
        assert!(is_restorable("file:///home/x/notes.html"));
    }

    #[test]
    fn drops_tabs_that_carry_nothing() {
        // Restoring a window of blank tabs is worse than restoring none.
        assert!(!is_restorable(""));
        assert!(!is_restorable("   "));
        assert!(!is_restorable("about:blank"));
        assert!(!is_restorable("tela://start"));
        // An extension page is tied to a bundle that may not be installed next
        // time; restoring it would fail confusingly.
        assert!(!is_restorable("tela-extension://ublock-origin/popup.html"));
    }
}
