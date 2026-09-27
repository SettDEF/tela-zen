//! Browsing history with full-text search.
//!
//! "That page about X I read on Tuesday" should be one query, so the store is
//! SQLite with an FTS5 index over title and URL. One row per URL — revisiting a
//! page bumps its count and timestamp rather than piling up duplicates — which
//! keeps the list scannable and the search ranked by something meaningful.
//!
//! Per profile, beside the session and bookmarks. Credential and internal URLs
//! are never recorded; history is a convenience, not a surveillance log.

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use rusqlite::Connection;

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
        type History = super::HistoryRust;

        /// Open the database and create the schema if needed. Call once before
        /// anything else; returns false if the store could not be opened.
        #[qinvokable]
        fn open(self: Pin<&mut History>) -> bool;

        /// Record a visit to `url` with `title`. Bumps the existing row if the
        /// URL is already known. Ignores internal, blank and credential URLs.
        #[qinvokable]
        fn record_visit(self: Pin<&mut History>, url: &QString, title: &QString);

        /// Entries matching `query` (full-text over title and URL), newest
        /// first, as a JSON array of `{url, title, lastVisit, visitCount}`. An
        /// empty query returns the most recent entries.
        #[qinvokable]
        fn search(self: Pin<&mut History>, query: &QString, limit: i32) -> QString;

        /// Forget one URL. Returns true if a row was removed.
        #[qinvokable]
        fn remove(self: Pin<&mut History>, url: &QString) -> bool;

        /// Wipe all history.
        #[qinvokable]
        fn clear(self: Pin<&mut History>) -> bool;
    }
}

#[derive(Default)]
pub struct HistoryRust {
    conn: Option<Connection>,
}

/// URLs worth keeping. Start pages, blanks and extension pages carry nothing;
/// credential origins are deliberately never logged.
fn is_recordable(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty()
        || url == "about:blank"
        || url.starts_with("tela://")
        || url.starts_with("tela-extension://")
        || url.starts_with("about:")
    {
        return false;
    }
    // Only real web pages.
    url.starts_with("http://") || url.starts_with("https://")
}

/// Turn free-typed text into a safe FTS5 MATCH expression: each token becomes a
/// quoted prefix term, so `git hub` matches `github…` without the user needing
/// to know FTS5 syntax, and stray quotes or operators cannot break the query.
fn fts_query(raw: &str) -> String {
    let mut terms = Vec::new();
    for token in raw.split_whitespace() {
        let cleaned: String = token
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
            .collect();
        if !cleaned.is_empty() {
            terms.push(format!("\"{cleaned}\"*"));
        }
    }
    terms.join(" ")
}

impl HistoryRust {
    fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                 id          INTEGER PRIMARY KEY,
                 url         TEXT UNIQUE NOT NULL,
                 title       TEXT NOT NULL DEFAULT '',
                 visit_count INTEGER NOT NULL DEFAULT 1,
                 last_visit  INTEGER NOT NULL
             );

             CREATE VIRTUAL TABLE IF NOT EXISTS history_fts USING fts5(
                 url, title, content='history', content_rowid='id'
             );

             CREATE TRIGGER IF NOT EXISTS history_ai AFTER INSERT ON history BEGIN
                 INSERT INTO history_fts(rowid, url, title)
                 VALUES (new.id, new.url, new.title);
             END;
             CREATE TRIGGER IF NOT EXISTS history_ad AFTER DELETE ON history BEGIN
                 INSERT INTO history_fts(history_fts, rowid, url, title)
                 VALUES ('delete', old.id, old.url, old.title);
             END;
             CREATE TRIGGER IF NOT EXISTS history_au AFTER UPDATE ON history BEGIN
                 INSERT INTO history_fts(history_fts, rowid, url, title)
                 VALUES ('delete', old.id, old.url, old.title);
                 INSERT INTO history_fts(rowid, url, title)
                 VALUES (new.id, new.url, new.title);
             END;",
        )
    }
}

impl ffi::History {
    fn open(mut self: Pin<&mut Self>) -> bool {
        let Some(dir) = crate::config::config_dir() else {
            return false;
        };
        if std::fs::create_dir_all(&dir).is_err() {
            return false;
        }
        let path = dir.join("history.db");
        match Connection::open(&path).and_then(|conn| {
            HistoryRust::ensure_schema(&conn)?;
            Ok(conn)
        }) {
            Ok(conn) => {
                self.as_mut().rust_mut().conn = Some(conn);
                true
            }
            Err(_) => false,
        }
    }

    fn record_visit(self: Pin<&mut Self>, url: &QString, title: &QString) {
        let url = url.to_string();
        if !is_recordable(&url) {
            return;
        }
        let title = title.to_string();
        let Some(conn) = self.rust().conn.as_ref() else {
            return;
        };
        // now() as a plain unix-seconds integer; passed from QML would be
        // cleaner, but the DB clock is fine for "recently".
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // Upsert: first visit inserts, later ones bump count and refresh the
        // title (it may have changed) and timestamp.
        let _ = conn.execute(
            "INSERT INTO history (url, title, visit_count, last_visit)
             VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(url) DO UPDATE SET
                 visit_count = visit_count + 1,
                 last_visit  = ?3,
                 title       = CASE WHEN ?2 <> '' THEN ?2 ELSE title END",
            rusqlite::params![url, title, now],
        );
    }

    fn search(self: Pin<&mut Self>, query: &QString, limit: i32) -> QString {
        let Some(conn) = self.rust().conn.as_ref() else {
            return QString::from("[]");
        };
        let limit = limit.clamp(1, 500) as i64;
        let raw = query.to_string();
        let fts = fts_query(&raw);

        let rows: rusqlite::Result<Vec<(String, String, i64, i64)>> = if fts.is_empty() {
            // No query: most recently visited.
            let mut stmt = match conn.prepare(
                "SELECT url, title, last_visit, visit_count FROM history
                 ORDER BY last_visit DESC LIMIT ?1",
            ) {
                Ok(stmt) => stmt,
                Err(_) => return QString::from("[]"),
            };
            stmt.query_map([limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .and_then(|m| m.collect())
        } else {
            // Full-text: rank by FTS relevance, then recency.
            let mut stmt = match conn.prepare(
                "SELECT h.url, h.title, h.last_visit, h.visit_count
                 FROM history_fts f
                 JOIN history h ON h.id = f.rowid
                 WHERE history_fts MATCH ?1
                 ORDER BY bm25(history_fts), h.last_visit DESC
                 LIMIT ?2",
            ) {
                Ok(stmt) => stmt,
                Err(_) => return QString::from("[]"),
            };
            stmt.query_map(rusqlite::params![fts, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .and_then(|m| m.collect())
        };

        let entries = rows.unwrap_or_default();
        let json: Vec<serde_json::Value> = entries
            .into_iter()
            .map(|(url, title, last_visit, visit_count)| {
                serde_json::json!({
                    "url": url,
                    "title": title,
                    "lastVisit": last_visit,
                    "visitCount": visit_count,
                })
            })
            .collect();
        QString::from(&serde_json::to_string(&json).unwrap_or_else(|_| "[]".to_owned()))
    }

    fn remove(self: Pin<&mut Self>, url: &QString) -> bool {
        let Some(conn) = self.rust().conn.as_ref() else {
            return false;
        };
        conn.execute("DELETE FROM history WHERE url = ?1", [url.to_string()])
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    fn clear(self: Pin<&mut Self>) -> bool {
        let Some(conn) = self.rust().conn.as_ref() else {
            return false;
        };
        conn.execute_batch("DELETE FROM history;").is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_real_pages_recorded() {
        assert!(is_recordable("https://example.com/a"));
        assert!(is_recordable("http://localhost:8080"));
        assert!(!is_recordable(""));
        assert!(!is_recordable("about:blank"));
        assert!(!is_recordable("tela://start"));
        assert!(!is_recordable("tela-extension://ublock/popup.html"));
        assert!(!is_recordable("file:///etc/hosts"));
    }

    #[test]
    fn fts_query_makes_prefix_terms_and_drops_operators() {
        assert_eq!(fts_query("git hub"), "\"git\"* \"hub\"*");
        // FTS5 operators and quotes cannot leak through.
        assert_eq!(fts_query("foo\" OR bar"), "\"foo\"* \"OR\"* \"bar\"*");
        assert_eq!(fts_query("   "), "");
    }

    #[test]
    fn record_search_remove_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        HistoryRust::ensure_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO history (url, title, visit_count, last_visit)
             VALUES ('https://github.com', 'GitHub Home', 1, 100)",
            [],
        )
        .unwrap();
        // FTS finds it by a prefix of the title.
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM history_fts WHERE history_fts MATCH '\"git\"*'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        // Delete keeps the FTS mirror in sync via the trigger.
        conn.execute("DELETE FROM history WHERE url='https://github.com'", [])
            .unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM history_fts WHERE history_fts MATCH '\"git\"*'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }
}
