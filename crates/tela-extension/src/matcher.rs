//! URL matching, shared by extension content scripts and userscripts.
//!
//! Two pattern dialects have to be supported, because the two ecosystems use
//! different ones:
//!
//! - **Match patterns** (`*://*.example.com/*`) — WebExtensions `content_scripts.matches`
//!   and a userscript's `@match`. Structured: scheme, host, path, each with
//!   specific wildcard rules.
//! - **Globs** (`*://example.com/foo*bar`) — a userscript's `@include`/`@exclude`.
//!   Unstructured: `*` matches anything, anywhere.
//!
//! Conflating them is the usual source of scripts that silently never run, so
//! they are parsed separately here.

/// A compiled match pattern.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// `<all_urls>` — every http/https/ftp/file URL.
    AllUrls,
    Match {
        /// `None` means the `*` scheme, which covers http and https only.
        scheme: Option<String>,
        host: HostPattern,
        /// Path glob, always starting with `/`.
        path: String,
    },
    /// An `@include`/`@exclude` glob, matched against the whole URL.
    Glob(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum HostPattern {
    /// `*` — any host.
    Any,
    /// `*.example.com` — that domain or any subdomain of it.
    Subdomains(String),
    /// `example.com` — exactly this host.
    Exact(String),
}

impl Pattern {
    /// Parse a WebExtensions-style match pattern.
    pub fn parse_match(pattern: &str) -> Option<Self> {
        if pattern == "<all_urls>" {
            return Some(Pattern::AllUrls);
        }

        let (scheme, rest) = pattern.split_once("://")?;
        let scheme = match scheme {
            "*" => None,
            "http" | "https" | "file" | "ftp" => Some(scheme.to_owned()),
            _ => return None,
        };

        // `file://` URLs have no host, so the remainder is entirely path.
        if scheme.as_deref() == Some("file") {
            return Some(Pattern::Match {
                scheme,
                host: HostPattern::Any,
                path: if rest.is_empty() { "/*".to_owned() } else { rest.to_owned() },
            });
        }

        let (host, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, "/*"),
        };
        if host.is_empty() {
            return None;
        }

        let host = if host == "*" {
            HostPattern::Any
        } else if let Some(domain) = host.strip_prefix("*.") {
            if domain.is_empty() {
                return None;
            }
            HostPattern::Subdomains(domain.to_ascii_lowercase())
        } else if host.contains('*') {
            // A `*` anywhere else is not a legal match pattern.
            return None;
        } else {
            HostPattern::Exact(host.to_ascii_lowercase())
        };

        Some(Pattern::Match { scheme, host, path: path.to_owned() })
    }

    /// Parse a Greasemonkey-style glob (`@include` / `@exclude`).
    pub fn parse_glob(pattern: &str) -> Self {
        Pattern::Glob(pattern.to_owned())
    }

    pub fn matches(&self, url: &str) -> bool {
        match self {
            Pattern::AllUrls => {
                let scheme = url.split("://").next().unwrap_or("");
                matches!(scheme, "http" | "https" | "ftp" | "file")
            }
            Pattern::Glob(glob) => glob_matches(glob, url),
            Pattern::Match { scheme, host, path } => {
                let Some((url_scheme, rest)) = url.split_once("://") else {
                    return false;
                };

                match scheme {
                    Some(expected) => {
                        if url_scheme != expected {
                            return false;
                        }
                    }
                    // The `*` scheme deliberately excludes file and ftp.
                    None => {
                        if url_scheme != "http" && url_scheme != "https" {
                            return false;
                        }
                    }
                }

                let (url_host, url_path) = match rest.find('/') {
                    Some(index) => (&rest[..index], &rest[index..]),
                    None => (rest, "/"),
                };

                // Ports and credentials are not part of host matching.
                let url_host = url_host.rsplit('@').next().unwrap_or(url_host);
                let url_host = url_host.split(':').next().unwrap_or(url_host);
                let url_host = url_host.to_ascii_lowercase();

                let host_ok = match host {
                    HostPattern::Any => true,
                    HostPattern::Exact(expected) => url_host == *expected,
                    HostPattern::Subdomains(domain) => {
                        url_host == *domain || url_host.ends_with(&format!(".{domain}"))
                    }
                };
                if !host_ok {
                    return false;
                }

                // The query string counts as part of the path for matching.
                glob_matches(path, url_path)
            }
        }
    }
}

/// `*` matches any run of characters, including none. No other metacharacters.
fn glob_matches(pattern: &str, value: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == value;
    }

    let mut cursor = 0usize;

    // The segment before the first `*` must sit at the start.
    if let Some(first) = parts.first() {
        if !value.starts_with(first) {
            return false;
        }
        cursor = first.len();
    }

    // The segment after the last `*` must sit at the end, and must not overlap
    // what the leading segment already consumed.
    if let Some(last) = parts.last() {
        if !value.ends_with(last) || value.len() < cursor + last.len() {
            return false;
        }
    }

    let middle = &parts[1..parts.len() - 1];
    let end = value.len() - parts[parts.len() - 1].len();

    for segment in middle {
        if segment.is_empty() {
            continue;
        }
        match value.get(cursor..end).and_then(|slice| slice.find(segment)) {
            Some(index) => cursor += index + segment.len(),
            None => return false,
        }
    }

    true
}

/// A set of include/exclude patterns, evaluated the way both ecosystems do:
/// excludes win over includes.
#[derive(Debug, Clone, Default)]
pub struct MatchSet {
    pub includes: Vec<Pattern>,
    pub excludes: Vec<Pattern>,
}

impl MatchSet {
    pub fn matches(&self, url: &str) -> bool {
        if self.excludes.iter().any(|pattern| pattern.matches(url)) {
            return false;
        }
        self.includes.iter().any(|pattern| pattern.matches(url))
    }

    pub fn is_empty(&self) -> bool {
        self.includes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pattern: &str) -> Pattern {
        Pattern::parse_match(pattern).expect("pattern should parse")
    }

    #[test]
    fn scheme_wildcard_covers_http_and_https_only() {
        let pattern = m("*://example.com/*");
        assert!(pattern.matches("http://example.com/"));
        assert!(pattern.matches("https://example.com/a/b"));
        assert!(!pattern.matches("file://example.com/"));
        assert!(!pattern.matches("ftp://example.com/"));
    }

    #[test]
    fn subdomain_wildcard_includes_the_bare_domain() {
        let pattern = m("*://*.example.com/*");
        assert!(pattern.matches("https://example.com/"));
        assert!(pattern.matches("https://www.example.com/"));
        assert!(pattern.matches("https://deep.nested.example.com/"));
        // Must not match a domain that merely ends with the same text.
        assert!(!pattern.matches("https://notexample.com/"));
        assert!(!pattern.matches("https://evil-example.com/"));
    }

    #[test]
    fn exact_host_rejects_subdomains() {
        let pattern = m("https://example.com/*");
        assert!(pattern.matches("https://example.com/x"));
        assert!(!pattern.matches("https://www.example.com/x"));
    }

    #[test]
    fn port_and_credentials_are_ignored_for_host_matching() {
        let pattern = m("https://example.com/*");
        assert!(pattern.matches("https://example.com:8443/x"));
        assert!(pattern.matches("https://user@example.com/x"));
    }

    #[test]
    fn path_globs_apply() {
        let pattern = m("https://example.com/api/*");
        assert!(pattern.matches("https://example.com/api/v1"));
        assert!(!pattern.matches("https://example.com/other"));
    }

    #[test]
    fn all_urls_covers_the_usual_schemes() {
        let pattern = Pattern::parse_match("<all_urls>").unwrap();
        assert!(pattern.matches("http://a/"));
        assert!(pattern.matches("file:///home/x"));
        assert!(!pattern.matches("tela-extension://abc/popup.html"));
    }

    #[test]
    fn rejects_malformed_patterns() {
        assert!(Pattern::parse_match("example.com").is_none());
        assert!(Pattern::parse_match("javascript://*/*").is_none());
        // A `*` in the middle of a host is not legal.
        assert!(Pattern::parse_match("*://ex*ple.com/*").is_none());
    }

    #[test]
    fn globs_match_anywhere() {
        let pattern = Pattern::parse_glob("*://example.com/*/edit");
        assert!(pattern.matches("https://example.com/doc/edit"));
        assert!(!pattern.matches("https://example.com/doc/view"));
    }

    #[test]
    fn glob_segments_do_not_overlap() {
        // A single "ab" must not satisfy both the leading and trailing segment.
        assert!(!glob_matches("ab*ab", "ab"));
        assert!(glob_matches("ab*ab", "abab"));
        assert!(glob_matches("ab*ab", "abXab"));
    }

    #[test]
    fn excludes_beat_includes() {
        let set = MatchSet {
            includes: vec![m("*://*.example.com/*")],
            excludes: vec![m("*://admin.example.com/*")],
        };
        assert!(set.matches("https://www.example.com/"));
        assert!(!set.matches("https://admin.example.com/"));
    }
}
