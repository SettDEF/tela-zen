//! Bring a signed-in Google session over from Firefox.
//!
//! Tela's engine cannot sign in to Google — Google blocks every Qt WebEngine
//! browser alike, verified against qutebrowser. The escape hatch is the
//! handoff: the user signs in in their real Firefox, then this copies that
//! session across so they are signed in inside Tela too.
//!
//! Deliberately narrow and deliberately user-driven. It runs only when the user
//! clicks "Import from Firefox", reads only Google and YouTube cookies, and
//! hands them straight to the cookie store. Nothing here prints or returns a
//! cookie value — a session cookie is a bearer token for the account, and the
//! one safe thing to do with it is move it and never look.
//!
//! Firefox keeps cookies as plain SQLite, so unlike Chrome there is nothing to
//! decrypt and no keyring to unlock.

use std::path::PathBuf;
use std::pin::Pin;

use cxx_qt_lib::QString;

/// A cookie ready for the store. Values pass through, never out: this never
/// leaves the crate and is never formatted.
struct FirefoxCookie {
    host: String,
    name: String,
    value: String,
    path: String,
    secure: bool,
    http_only: bool,
    /// Firefox expiry: seconds since the Unix epoch. 0 for a session cookie.
    expiry: i64,
    /// Firefox SameSite: 0 None, 1 Lax, 2 Strict. Kept so a cross-site auth
    /// cookie (Google's) stays SameSite=None and reaches embedded iframes.
    same_site: i32,
}

/// Every place a Firefox install keeps its profiles here. The plain
/// `~/.mozilla`, the XDG `~/.config/mozilla` some builds use (this machine's
/// does), and the sandboxed Flatpak and Snap trees.
fn firefox_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join(".mozilla/firefox"));
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        roots.push(config.join("mozilla/firefox"));
        roots.push(home.join(".var/app/org.mozilla.firefox/.mozilla/firefox"));
        roots.push(home.join("snap/firefox/common/.mozilla/firefox"));
    }
    roots
}

/// The default Firefox profile's cookie database, across every install layout.
///
/// `profiles.ini` names the default; failing that, the conventional
/// `*.default*` directory is used. `cookies.sqlite` is plain — no decryption.
fn firefox_cookie_db() -> Option<PathBuf> {
    for root in firefox_roots() {
        // Prefer the profile marked Default in profiles.ini.
        if let Ok(ini) = std::fs::read_to_string(root.join("profiles.ini")) {
            if let Some(path) = default_profile_from_ini(&ini) {
                let db = root.join(&path).join("cookies.sqlite");
                if db.is_file() {
                    return Some(db);
                }
            }
        }

        // Otherwise the first directory that looks like a default profile.
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.contains(".default") {
                let db = entry.path().join("cookies.sqlite");
                if db.is_file() {
                    return Some(db);
                }
            }
        }
    }
    None
}

/// The `Path=` of the `[Install…]` default, or the last `[Profile]` with
/// `Default=1`. Returns the profile directory, relative to the firefox root.
fn default_profile_from_ini(ini: &str) -> Option<String> {
    // An [Install*] section names the running default directly and wins.
    let mut in_install = false;
    for line in ini.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_install = line.starts_with("[Install");
        } else if in_install {
            if let Some(path) = line.strip_prefix("Default=") {
                return Some(path.trim().to_owned());
            }
        }
    }

    // Otherwise a [Profile*] section flagged Default=1.
    let mut path: Option<String> = None;
    let mut is_default = false;
    for line in ini.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            if is_default {
                if let Some(p) = path.take() {
                    return Some(p);
                }
            }
            path = None;
            is_default = false;
        } else if let Some(p) = line.strip_prefix("Path=") {
            path = Some(p.trim().to_owned());
        } else if line == "Default=1" {
            is_default = true;
        }
    }
    if is_default { path } else { None }
}

/// True for the Google and YouTube domains — the narrow scope of the handoff.
/// Matched on label boundaries so `google.com` does not pull in `notgoogle.com`.
fn is_google(host: &str) -> bool {
    let host = host.trim_start_matches('.').to_ascii_lowercase();
    const DOMAINS: [&str; 4] = ["google.com", "youtube.com", "google.co", "ytimg.com"];
    DOMAINS.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Read Google/YouTube cookies from Firefox's store.
fn read_cookies(db: &std::path::Path) -> Result<Vec<FirefoxCookie>, String> {
    // immutable=1: read even while Firefox holds the file open, without a lock
    // or a WAL write.
    let uri = format!("file:{}?immutable=1&mode=ro", db.display());
    let conn = rusqlite::Connection::open_with_flags(
        &uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| format!("cannot open Firefox cookies: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT host, name, value, path, isSecure, isHttpOnly, expiry, sameSite \
             FROM moz_cookies",
        )
        .map_err(|e| format!("unexpected cookie schema: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(FirefoxCookie {
                host: row.get::<_, String>(0)?,
                name: row.get::<_, String>(1)?,
                value: row.get::<_, String>(2)?,
                path: row.get::<_, String>(3)?,
                secure: row.get::<_, i64>(4)? != 0,
                http_only: row.get::<_, i64>(5)? != 0,
                expiry: row.get::<_, i64>(6)?,
                same_site: row.get::<_, i64>(7).unwrap_or(1) as i32,
            })
        })
        .map_err(|e| format!("cannot read cookies: {e}"))?;

    Ok(rows
        .flatten()
        .filter(|cookie| is_google(&cookie.host))
        .collect())
}

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[namespace = "tela"]
    unsafe extern "C++" {
        include!("tela_cookies.h");

        fn cookies_set(
            host: &QString,
            name: &QString,
            value: &QString,
            path: &QString,
            secure: bool,
            http_only: bool,
            expires_unix: i64,
            same_site: i32,
        );
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type FirefoxImport = super::FirefoxImportRust;

        /// Whether a Firefox profile with cookies exists to import from, so the
        /// button only appears when it can do something.
        #[qinvokable]
        fn firefox_available(self: Pin<&mut FirefoxImport>) -> bool;

        /// Copy the Google/YouTube session from Firefox into Tela. Returns a
        /// short status line for the UI — counts only, never a cookie value.
        #[qinvokable]
        fn import(self: Pin<&mut FirefoxImport>) -> QString;
    }
}

#[derive(Default)]
pub struct FirefoxImportRust;

impl ffi::FirefoxImport {
    fn firefox_available(self: Pin<&mut Self>) -> bool {
        firefox_cookie_db().is_some()
    }

    fn import(self: Pin<&mut Self>) -> QString {
        let Some(db) = firefox_cookie_db() else {
            return QString::from("No Firefox profile found to import from.");
        };

        let cookies = match read_cookies(&db) {
            Ok(cookies) => cookies,
            Err(err) => {
                return QString::from(&format!("Could not read Firefox's cookies. ({err})"));
            }
        };

        if cookies.is_empty() {
            return QString::from(
                "No Google session found in Firefox — sign in there first, then \
                 import.",
            );
        }

        let mut written = 0;
        for cookie in &cookies {
            ffi::cookies_set(
                &QString::from(&cookie.host),
                &QString::from(&cookie.name),
                &QString::from(&cookie.value),
                &QString::from(&cookie.path),
                cookie.secure,
                cookie.http_only,
                cookie.expiry,
                cookie.same_site,
            );
            written += 1;
        }

        QString::from(&format!(
            "Signed in — imported {written} cookie{} from Firefox. Reload the \
             page.",
            if written == 1 { "" } else { "s" }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_scope_matches_on_label_boundaries() {
        assert!(is_google("accounts.google.com"));
        assert!(is_google(".youtube.com"));
        assert!(is_google("www.google.com"));
        assert!(!is_google("notgoogle.com"));
        assert!(!is_google("google.com.evil.test"));
    }

    #[test]
    fn install_default_wins_over_profile_flag() {
        let ini = "\
[Install ABC]\n\
Default=abcd.default-release\n\
\n\
[Profile0]\n\
Path=other.default\n\
Default=1\n";
        assert_eq!(
            default_profile_from_ini(ini).as_deref(),
            Some("abcd.default-release")
        );
    }

    #[test]
    fn falls_back_to_the_default_flagged_profile() {
        let ini = "\
[Profile0]\n\
Path=xyz.default-release\n\
Default=1\n\
\n\
[Profile1]\n\
Path=other\n";
        assert_eq!(
            default_profile_from_ini(ini).as_deref(),
            Some("xyz.default-release")
        );
    }

    #[test]
    fn no_default_yields_nothing() {
        let ini = "[Profile0]\nPath=a\n\n[Profile1]\nPath=b\n";
        assert_eq!(default_profile_from_ini(ini), None);
    }
}
