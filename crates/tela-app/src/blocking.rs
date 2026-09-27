//! Network-level content blocking.
//!
//! The engine lives in `tela-block`; this is the part that puts it on the
//! request path. Blocking here rather than in an extension means it works with
//! no extension host involved, and it cannot be defeated by a page.
//!
//! `should_block` is called for **every** request, on Chromium's IO thread. The
//! engine is built once at startup, packed to bytes, and rebuilt per thread on
//! first use — so the hot path takes no lock and touches no shared state.

use std::cell::RefCell;
use std::ffi::{CStr, c_char};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{OnceLock, RwLock};

use tela_block::Blocker;

/// The built engine in portable form. The engine type itself is not `Sync`, so
/// the bytes are what crosses the thread boundary.
static PACKED: OnceLock<Vec<u8>> = OnceLock::new();

/// Set `TELA_LOG_BLOCKS=1` to print every cancelled request. Off by default:
/// this is on the hot path, and an ad-heavy page cancels hundreds of requests.
static LOG_BLOCKS: OnceLock<bool> = OnceLock::new();

fn log_blocks() -> bool {
    *LOG_BLOCKS.get_or_init(|| std::env::var("TELA_LOG_BLOCKS").is_ok_and(|v| v == "1"))
}

/// The master switch, mirroring `blocking.enabled`. An atomic rather than a
/// lock because it is read on every request and written about once a session.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// True when [`ALLOWLIST`] is non-empty, so the common case never takes a lock.
static HAS_ALLOWLIST: AtomicBool = AtomicBool::new(false);

/// Hostnames the user has taken out of filtering, from `blocking.allowlist`.
static ALLOWLIST: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Apply `blocking.enabled` and `blocking.allowlist`. Called on load and on
/// every config reload, so both take effect without a restart.
pub fn configure(enabled: bool, allowlist: Vec<String>) {
    let hosts: Vec<String> = allowlist
        .into_iter()
        .map(|host| host.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|host| !host.is_empty())
        .collect();
    HAS_ALLOWLIST.store(!hosts.is_empty(), Ordering::Relaxed);
    if let Ok(mut slot) = ALLOWLIST.write() {
        *slot = hosts;
    }
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// The host part of a URL, lowercased by the caller's comparison rather than
/// here — this runs per request and must not allocate.
///
/// Deliberately not a URL parser: it needs the authority and nothing else, and
/// a wrong answer only ever means "not allowlisted", which is the safe way to
/// be wrong.
fn host_of(url: &str) -> &str {
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    // Credentials come before the host, a port after it. An IPv6 literal is
    // bracketed and full of colons, so the port is only what follows the "]".
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    match host.strip_prefix('[') {
        Some(rest) => rest.split_once(']').map_or(host, |(inner, _)| inner),
        None => host.split_once(':').map_or(host, |(h, _)| h),
    }
}

/// True when `host` is an allowlisted site or a subdomain of one.
///
/// Matched on label boundaries: `cineby.at` covers `www.cineby.at` but not
/// `notcineby.at`, which a plain suffix test would wrongly exempt.
fn is_allowlisted(host: &str) -> bool {
    if !HAS_ALLOWLIST.load(Ordering::Relaxed) {
        return false;
    }
    let Ok(hosts) = ALLOWLIST.read() else {
        return false;
    };
    hosts.iter().any(|allowed| {
        host.len() >= allowed.len()
            && host.to_ascii_lowercase().ends_with(allowed.as_str())
            && (host.len() == allowed.len()
                || host.as_bytes()[host.len() - allowed.len() - 1] == b'.')
    })
}

thread_local! {
    /// Per-thread engine, rebuilt from `PACKED` on first use. In practice this
    /// is built exactly once, on Chromium's IO thread.
    static LOCAL: RefCell<Option<Option<Blocker>>> = const { RefCell::new(None) };
}

/// Where the user's own lists live. Anything ending in `.txt` is loaded.
fn filters_dir() -> Option<PathBuf> {
    crate::config::config_dir().map(|dir| dir.join("filters"))
}

/// Lists shipped inside an installed uBlock Origin, used to seed a first run.
///
/// Reading another extension's assets is a one-time convenience, not a
/// dependency: once copied, the lists are the browser's own and uBlock can be
/// removed. Without this, a fresh install blocks nothing until the user finds
/// and downloads a filter list by hand.
fn seed_sources() -> Vec<PathBuf> {
    let Some(dir) = crate::config::config_dir() else {
        return Vec::new();
    };
    let assets = dir.join("extensions/ublock-origin/assets");
    [
        "thirdparties/easylist/easylist.txt",
        "thirdparties/easylist/easyprivacy.txt",
        "ublock/filters.min.txt",
        "ublock/privacy.min.txt",
        "ublock/badware.min.txt",
        "ublock/unbreak.min.txt",
    ]
    .iter()
    .map(|name| assets.join(name))
    .filter(|path| path.is_file())
    .collect()
}

/// Copy seed lists in if the filters directory is empty. Returns how many.
fn seed_if_empty(dir: &PathBuf) -> usize {
    let already_present = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "txt"))
        })
        .unwrap_or(false);
    if already_present {
        return 0;
    }

    let mut copied = 0;
    for source in seed_sources() {
        let Some(name) = source.file_name() else {
            continue;
        };
        if std::fs::copy(&source, dir.join(name)).is_ok() {
            copied += 1;
        }
    }
    copied
}

/// Newest mtime across the filter lists, used to tell a stale cache from a
/// current one. Second granularity is enough: a list edited within the same
/// second as the cache was written would be missed, so the cache is treated as
/// stale on ties.
fn newest_list_mtime(dir: &PathBuf) -> Option<std::time::SystemTime> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "txt"))
        .filter_map(|entry| entry.metadata().ok()?.modified().ok())
        .max()
}

/// Build the engine. Call once, before any page loads.
///
/// Returns the number of lists loaded so startup can report it rather than
/// failing silently — a blocker with no rules looks identical to a working one
/// until you notice the ads.
pub fn init() -> usize {
    // Nothing packed means nothing blocked, which is the correct fallback.
    let Some(dir) = filters_dir() else {
        return 0;
    };
    let _ = std::fs::create_dir_all(&dir);
    seed_if_empty(&dir);

    // Parsing several megabytes of filter text costs over a second, and the
    // lists change rarely. Reuse the packed engine whenever it is newer than
    // every list that produced it.
    let cache = dir.join(".engine.bin");
    let newest = newest_list_mtime(&dir);
    let cache_fresh = match (std::fs::metadata(&cache).and_then(|m| m.modified()), newest) {
        (Ok(cached), Some(newest)) => cached > newest,
        _ => false,
    };

    if cache_fresh {
        if let Ok(bytes) = std::fs::read(&cache) {
            // Only trust the cache if it actually rebuilds — a truncated or
            // stale-format file must not silently disable blocking.
            if Blocker::from_serialized(&bytes).is_some() {
                let count = std::fs::read_dir(&dir)
                    .map(|entries| {
                        entries
                            .flatten()
                            .filter(|e| e.path().extension().is_some_and(|x| x == "txt"))
                            .count()
                    })
                    .unwrap_or(0);
                let _ = PACKED.set(bytes);
                return count;
            }
        }
    }

    let mut lists = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "txt") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    lists.push(text);
                }
            }
        }
    }

    let count = lists.len();
    if !lists.is_empty() {
        let packed = Blocker::from_lists(lists).serialize();
        let _ = std::fs::write(&cache, &packed);
        let _ = PACKED.set(packed);
    }
    count
}

fn as_str<'a>(value: *const c_char) -> &'a str {
    if value.is_null() {
        return "";
    }
    // Safety: Chromium hands us NUL-terminated UTF-8 for the lifetime of the
    // call, and nothing here holds the reference past returning.
    unsafe { CStr::from_ptr(value) }.to_str().unwrap_or("")
}

/// Called from the C++ interceptor for every request.
///
/// Deliberately infallible: any parse failure or missing engine allows the
/// request. A blocker that fails closed would break the web on a bad rule.
///
/// # Safety
/// All four pointers must be NUL-terminated C strings valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tela_should_block(
    url: *const c_char,
    source_url: *const c_char,
    request_type: *const c_char,
    method: *const c_char,
) -> bool {
    if !ENABLED.load(Ordering::Relaxed) {
        return false;
    }
    let Some(packed) = PACKED.get() else {
        return false;
    };
    // Allowlisting is about the page you are on, not the server being asked:
    // exempting a site has to cover the third-party requests it makes, which
    // is where every ad on it comes from. `source_url` is empty for the
    // top-level document, so fall back to the request itself.
    let page = as_str(source_url);
    let page = if page.is_empty() { as_str(url) } else { page };
    if is_allowlisted(host_of(page)) {
        return false;
    }
    LOCAL.with(|cell| {
        let mut slot = cell.borrow_mut();
        // Outer Option: "have we tried yet". Inner: "did it work". A failed
        // rebuild is remembered so it is not retried on every request.
        let engine = slot.get_or_insert_with(|| Blocker::from_serialized(packed));
        match engine {
            Some(engine) => {
                let target = as_str(url);
                let blocked = engine.should_block(
                    target,
                    as_str(source_url),
                    as_str(request_type),
                    as_str(method),
                );
                if blocked && log_blocks() {
                    eprintln!("tela-block: {} {}", as_str(request_type), target);
                }
                blocked
            }
            None => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{host_of, is_allowlisted};
    use tela_block::Blocker;

    #[test]
    fn reads_the_host_out_of_a_url() {
        assert_eq!(host_of("https://www.cineby.at/watch?x=1"), "www.cineby.at");
        assert_eq!(host_of("https://user:pw@site.test:8443/a"), "site.test");
        assert_eq!(host_of("http://[::1]:8080/a"), "::1");
        assert_eq!(host_of("site.test/a"), "site.test");
        assert_eq!(host_of(""), "");
    }

    #[test]
    fn an_allowlisted_site_covers_its_subdomains_only() {
        super::configure(true, vec!["cineby.at".to_owned()]);
        assert!(is_allowlisted("cineby.at"));
        assert!(is_allowlisted("www.cineby.at"));
        assert!(is_allowlisted("WWW.CINEBY.AT"));
        // A suffix test without the label boundary would exempt these.
        assert!(!is_allowlisted("notcineby.at"));
        assert!(!is_allowlisted("cineby.at.evil.test"));
        super::configure(true, Vec::new());
        assert!(!is_allowlisted("cineby.at"));
    }

    #[test]
    fn blocks_what_a_list_says_to_block() {
        let blocker = Blocker::from_lists(["||ads.example.com^", "/banner.gif"]);
        assert!(blocker.should_block(
            "https://ads.example.com/x.js",
            "https://site.test/",
            "script",
            "GET"
        ));
        assert!(blocker.should_block(
            "https://cdn.test/banner.gif",
            "https://site.test/",
            "image",
            "GET"
        ));
    }

    #[test]
    fn leaves_ordinary_requests_alone() {
        let blocker = Blocker::from_lists(["||ads.example.com^"]);
        assert!(!blocker.should_block(
            "https://site.test/app.js",
            "https://site.test/",
            "script",
            "GET"
        ));
    }

    #[test]
    fn an_empty_blocker_allows_everything() {
        // The state a fresh install starts in: it must not break browsing.
        let blocker = Blocker::empty();
        assert!(!blocker.should_block(
            "https://ads.example.com/x.js",
            "https://site.test/",
            "script",
            "GET"
        ));
    }

    #[test]
    fn a_malformed_url_is_allowed_rather_than_dropped() {
        let blocker = Blocker::from_lists(["||ads.example.com^"]);
        assert!(!blocker.should_block("not a url", "also not", "script", "GET"));
    }
}
