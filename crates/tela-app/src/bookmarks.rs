//! Bookmarks, stored as plain JSON at `~/.config/tela/bookmarks.json`.
//!
//! Deliberately a flat list rather than a folder tree. Folders are the part of
//! bookmarking people abandon; search over a flat list is what actually gets
//! used, and the command palette already provides it. Tags can be layered on
//! later without migrating anyone's data.

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
        #[qproperty(i32, count)]
        /// The whole list as JSON, newest first.
        #[qproperty(QString, list_json)]
        type Bookmarks = super::BookmarksRust;

        /// Load from disk. Safe to call repeatedly.
        #[qinvokable]
        fn reload(self: Pin<&mut Bookmarks>) -> bool;

        /// Add, or update the title of an existing entry. Returns true when the
        /// list changed.
        #[qinvokable]
        fn add(self: Pin<&mut Bookmarks>, url: &QString, title: &QString) -> bool;

        #[qinvokable]
        fn remove(self: Pin<&mut Bookmarks>, url: &QString) -> bool;

        /// Add if absent, remove if present. Returns the state afterwards.
        #[qinvokable]
        fn toggle(self: Pin<&mut Bookmarks>, url: &QString, title: &QString) -> bool;

        #[qinvokable]
        fn contains(self: Pin<&mut Bookmarks>, url: &QString) -> bool;

        /// Entries whose title or URL contain `query`, as JSON. Empty query
        /// returns the most recent few.
        #[qinvokable]
        fn search(self: Pin<&mut Bookmarks>, query: &QString, limit: i32) -> QString;

        /// Write the collection as Netscape bookmark HTML — what Firefox and
        /// Chrome both read. `target` may be a path or a `file://` URL.
        #[qinvokable]
        fn export_html(self: Pin<&mut Bookmarks>, target: &QString) -> bool;

        /// Merge a Netscape bookmark file in. Returns how many were added, or
        /// -1 if the file could not be read.
        #[qinvokable]
        fn import_html(self: Pin<&mut Bookmarks>, source: &QString) -> i32;

        /// The same, from text already in hand — a subscription fetched over
        /// the network rather than a file on disk.
        #[qinvokable]
        fn import_text(self: Pin<&mut Bookmarks>, html: &QString) -> i32;
    }
}

#[derive(Default)]
pub struct BookmarksRust {
    count: i32,
    list_json: QString,
    entries: Vec<Entry>,
}

#[derive(Clone)]
struct Entry {
    url: String,
    title: String,
    /// Milliseconds since the epoch, for ordering only.
    added: u64,
}

fn store_path() -> Option<PathBuf> {
    crate::config::config_dir().map(|dir| dir.join("bookmarks.json"))
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Trailing slashes and fragments produce duplicates that look identical in the
/// UI, so entries are keyed on a normalised form.
fn normalise(url: &str) -> String {
    let trimmed = url.trim();
    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    without_fragment.trim_end_matches('/').to_owned()
}

/// A QML file dialog hands back `file:///path`; everything else here uses
/// plain paths.
fn local_path(value: &str) -> PathBuf {
    let trimmed = value.trim();
    let path = trimmed.strip_prefix("file://").unwrap_or(trimmed);

    // Percent-decoding, because a dialog escapes spaces and anything non-ASCII.
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

fn unescape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|end| *end <= 12) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix('#')
                .and_then(|number| match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => number.parse().ok(),
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Byte index of `needle` in `haystack` at or after `from`, ignoring ASCII
/// case. The needle must be ASCII, which keeps the result on a char boundary.
fn find_ci(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    let (hay, need) = (haystack.as_bytes(), needle.as_bytes());
    if need.is_empty() || hay.len() < need.len() {
        return None;
    }
    (from..=hay.len() - need.len()).find(|&i| hay[i..i + need.len()].eq_ignore_ascii_case(need))
}

/// The value of an attribute inside one tag, quoted or bare.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut at = find_ci(tag, name, 0)?;
    loop {
        let after = &tag[at + name.len()..];
        let rest = after.trim_start();
        if let Some(rest) = rest.strip_prefix('=') {
            let rest = rest.trim_start();
            let value = match rest.chars().next() {
                Some(quote @ ('"' | '\'')) => {
                    let body = &rest[1..];
                    &body[..body.find(quote)?]
                }
                _ => {
                    let end = rest.find([' ', '\t', '\n', '>']).unwrap_or(rest.len());
                    &rest[..end]
                }
            };
            return Some(unescape_html(value));
        }
        at = find_ci(tag, name, at + 1)?;
    }
}

/// Anchors in a Netscape bookmark file, flattened — folders are structure this
/// browser deliberately does not keep.
fn parse_netscape(html: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(start) = find_ci(html, "<a", at) {
        // `<a` must begin a tag, not merely appear inside a word.
        let after = html[start + 2..].chars().next();
        if !matches!(after, Some(' ' | '\t' | '\n' | '\r')) {
            at = start + 2;
            continue;
        }
        let Some(tag_end) = html[start..].find('>').map(|end| start + end) else {
            break;
        };
        let tag = &html[start..tag_end];
        at = tag_end + 1;

        let Some(href) = attribute(tag, "href") else {
            continue;
        };
        // Bookmarklets are code, not places. Importing one from a file someone
        // sent you is how a bookmark bar becomes an execution vector.
        let lower = href.to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            continue;
        }

        let text_end = find_ci(html, "</a", at).unwrap_or(html.len());
        let title = unescape_html(html[at..text_end].trim());
        let added = attribute(tag, "add_date")
            .and_then(|seconds| seconds.parse::<u64>().ok())
            .map(|seconds| seconds.saturating_mul(1000))
            .unwrap_or_else(now_millis);

        out.push(Entry { url: normalise(&href), title, added });
    }
    out
}

impl BookmarksRust {
    /// The collection as Netscape bookmark HTML.
    fn to_netscape(&self) -> String {
        let mut out = String::from(
            "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n\
             <META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n\
             <TITLE>Bookmarks</TITLE>\n\
             <H1>Bookmarks</H1>\n\
             <DL><p>\n",
        );
        for entry in &self.entries {
            out.push_str(&format!(
                "    <DT><A HREF=\"{}\" ADD_DATE=\"{}\">{}</A>\n",
                escape_html(&entry.url),
                entry.added / 1000,
                escape_html(&entry.title),
            ));
        }
        out.push_str("</DL><p>\n");
        out
    }

    fn persist(&self) {
        let Some(path) = store_path() else {
            return;
        };
        let list: Vec<serde_json::Value> = self
            .entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "url": entry.url,
                    "title": entry.title,
                    "added": entry.added,
                })
            })
            .collect();

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Temp file plus rename, so a crash cannot truncate the collection.
        let temp = path.with_extension("json.tmp");
        let encoded = serde_json::to_string_pretty(&list).unwrap_or_else(|_| "[]".to_owned());
        if std::fs::write(&temp, encoded).is_ok() && std::fs::rename(&temp, &path).is_err() {
            let _ = std::fs::remove_file(&temp);
        }
    }

    fn encode(&self) -> String {
        let list: Vec<serde_json::Value> = self
            .entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "url": entry.url,
                    "title": entry.title,
                    "added": entry.added,
                })
            })
            .collect();
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_owned())
    }
}

impl ffi::Bookmarks {
    fn reload(mut self: Pin<&mut Self>) -> bool {
        let entries = store_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| value.as_array().cloned())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        Some(Entry {
                            url: item["url"].as_str()?.to_owned(),
                            title: item["title"].as_str().unwrap_or("").to_owned(),
                            added: item["added"].as_u64().unwrap_or(0),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        {
            let mut rust = self.as_mut().rust_mut();
            rust.entries = entries;
            rust.entries.sort_by(|a, b| b.added.cmp(&a.added));
        }
        self.as_mut().publish();
        true
    }

    fn add(mut self: Pin<&mut Self>, url: &QString, title: &QString) -> bool {
        let key = normalise(&url.to_string());
        if key.is_empty() {
            return false;
        }
        let title = title.to_string();

        {
            let mut rust = self.as_mut().rust_mut();
            if let Some(existing) = rust.entries.iter_mut().find(|e| normalise(&e.url) == key) {
                // Re-bookmarking an open page is how titles get corrected.
                if !title.is_empty() && existing.title != title {
                    existing.title = title;
                } else {
                    return false;
                }
            } else {
                rust.entries.insert(
                    0,
                    Entry { url: key, title, added: now_millis() },
                );
            }
            rust.persist();
        }
        self.as_mut().publish();
        true
    }

    fn remove(mut self: Pin<&mut Self>, url: &QString) -> bool {
        let key = normalise(&url.to_string());
        let removed = {
            let mut rust = self.as_mut().rust_mut();
            let before = rust.entries.len();
            rust.entries.retain(|e| normalise(&e.url) != key);
            let changed = rust.entries.len() != before;
            if changed {
                rust.persist();
            }
            changed
        };
        if removed {
            self.as_mut().publish();
        }
        removed
    }

    fn toggle(mut self: Pin<&mut Self>, url: &QString, title: &QString) -> bool {
        if self.as_mut().contains(url) {
            self.remove(url);
            false
        } else {
            self.add(url, title);
            true
        }
    }

    fn contains(self: Pin<&mut Self>, url: &QString) -> bool {
        let key = normalise(&url.to_string());
        !key.is_empty() && self.rust().entries.iter().any(|e| normalise(&e.url) == key)
    }

    fn export_html(self: Pin<&mut Self>, target: &QString) -> bool {
        let path = local_path(&target.to_string());
        if path.as_os_str().is_empty() {
            return false;
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, self.rust().to_netscape()).is_ok()
    }

    fn import_html(mut self: Pin<&mut Self>, source: &QString) -> i32 {
        let path = local_path(&source.to_string());
        let Ok(text) = std::fs::read_to_string(&path) else {
            return -1;
        };
        self.as_mut().import_text(&QString::from(&text))
    }

    fn import_text(mut self: Pin<&mut Self>, html: &QString) -> i32 {
        let text = html.to_string();
        if text.trim().is_empty() {
            return -1;
        }

        let added = {
            let mut rust = self.as_mut().rust_mut();
            let mut added = 0;
            for entry in parse_netscape(&text) {
                if rust.entries.iter().any(|held| normalise(&held.url) == entry.url) {
                    continue;
                }
                rust.entries.push(entry);
                added += 1;
            }
            if added > 0 {
                rust.entries.sort_by(|a, b| b.added.cmp(&a.added));
                rust.persist();
            }
            added
        };

        if added > 0 {
            self.as_mut().publish();
        }
        added
    }

    fn search(self: Pin<&mut Self>, query: &QString, limit: i32) -> QString {
        let needle = query.to_string().to_lowercase();
        let limit = limit.max(0) as usize;

        let matched: Vec<serde_json::Value> = self
            .rust()
            .entries
            .iter()
            .filter(|entry| {
                needle.is_empty()
                    || entry.title.to_lowercase().contains(&needle)
                    || entry.url.to_lowercase().contains(&needle)
            })
            .take(if limit == 0 { usize::MAX } else { limit })
            .map(|entry| {
                serde_json::json!({ "url": entry.url, "title": entry.title })
            })
            .collect();

        QString::from(&serde_json::to_string(&matched).unwrap_or_else(|_| "[]".to_owned()))
    }
}

impl ffi::Bookmarks {
    /// Push the current list out to QML.
    fn publish(mut self: Pin<&mut Self>) {
        let encoded = self.rust().encode();
        let total = self.rust().entries.len() as i32;
        self.as_mut().set_count(total);
        self.as_mut().set_list_json(QString::from(&encoded));
    }
}

#[cfg(test)]
mod tests {
    use super::{local_path, normalise, parse_netscape, unescape_html};

    #[test]
    fn normalises_away_trailing_slash_and_fragment() {
        assert_eq!(normalise("https://a.test/x/"), "https://a.test/x");
        assert_eq!(normalise("https://a.test/x#top"), "https://a.test/x");
        assert_eq!(normalise("  https://a.test/x  "), "https://a.test/x");
        // Different paths must stay distinct.
        assert_ne!(normalise("https://a.test/x"), normalise("https://a.test/y"));
    }

    const SAMPLE: &str = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<DL><p>
    <DT><H3>A folder</H3>
    <DL><p>
        <DT><A HREF="https://a.test/x" ADD_DATE="1700000000">Tom &amp; Jerry</A>
        <DT><A HREF="https://b.test/y">No date</A>
    </DL><p>
    <DT><A HREF="javascript:alert(1)">A bookmarklet</A>
    <DT><A HREF="https://c.test/z?a=1&amp;b=2">Query &lt;here&gt;</A>
</DL><p>"#;

    #[test]
    fn reads_anchors_out_of_folders() {
        let found = parse_netscape(SAMPLE);
        // Folders are flattened away; the anchors inside them are kept.
        let urls: Vec<&str> = found.iter().map(|e| e.url.as_str()).collect();
        assert_eq!(urls, ["https://a.test/x", "https://b.test/y", "https://c.test/z?a=1&b=2"]);
    }

    #[test]
    fn refuses_bookmarklets() {
        // Importing a file someone sent you must not import code.
        assert!(parse_netscape(SAMPLE).iter().all(|e| !e.url.starts_with("javascript:")));
    }

    #[test]
    fn decodes_entities_in_titles_and_urls() {
        let found = parse_netscape(SAMPLE);
        assert_eq!(found[0].title, "Tom & Jerry");
        assert_eq!(found[2].title, "Query <here>");
        assert_eq!(found[2].url, "https://c.test/z?a=1&b=2");
    }

    #[test]
    fn keeps_the_original_date_when_there_is_one() {
        let found = parse_netscape(SAMPLE);
        assert_eq!(found[0].added, 1_700_000_000_000);
        // Missing ADD_DATE falls back to now rather than to the epoch, so it
        // does not sort to the bottom forever.
        assert!(found[1].added > 1_700_000_000_000);
    }

    #[test]
    fn survives_a_round_trip() {
        let mut store = super::BookmarksRust::default();
        store.entries = parse_netscape(SAMPLE);
        let again = parse_netscape(&store.to_netscape());

        assert_eq!(again.len(), store.entries.len());
        assert_eq!(again[0].url, store.entries[0].url);
        assert_eq!(again[0].title, store.entries[0].title);
        assert_eq!(again[0].added, store.entries[0].added);
    }

    #[test]
    fn takes_a_path_or_a_file_url() {
        assert_eq!(local_path("/tmp/marks.html").to_string_lossy(), "/tmp/marks.html");
        assert_eq!(local_path("file:///tmp/marks.html").to_string_lossy(), "/tmp/marks.html");
        // A dialog escapes spaces.
        assert_eq!(local_path("file:///tmp/my%20marks.html").to_string_lossy(),
                   "/tmp/my marks.html");
    }

    #[test]
    fn leaves_a_bare_ampersand_alone() {
        assert_eq!(unescape_html("a & b"), "a & b");
        assert_eq!(unescape_html("&amp;&lt;&#39;&#x41;"), "&<'A");
    }

    #[test]
    fn imports_a_real_exported_file() {
        // The shape a browser actually writes: a folder, entities in titles,
        // ADD_DATE in seconds, and a bookmarklet that must not come through.
        let html = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fmhy-bookmarks.html"),
        )
        .expect("fixture present");

        let found = parse_netscape(&html);
        assert_eq!(found.len(), 8, "eight places, and not the bookmarklet");
        assert!(found.iter().all(|e| e.url.starts_with("https://")));
        assert_eq!(found[0].url, "https://fmhy.net");
        assert_eq!(found[0].added, 1_735_689_600_000);
        // Entities decoded, trailing slash normalised away.
        assert_eq!(found[2].title, "Video & Streaming");
        assert_eq!(found[3].title, "Reading <books>");
    }
}
