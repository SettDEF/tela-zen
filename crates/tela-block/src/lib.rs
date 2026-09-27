//! Content blocking, backed by the same engine Brave ships.
//!
//! This is the piece that replaces uBlock-as-an-extension: it consumes ordinary
//! EasyList/uBlock filter-list syntax, so the filter lists people already rely on
//! work here without an extension host existing at all.

use std::collections::HashSet;

use adblock::Engine;
use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;

pub struct Blocker {
    engine: Engine,
}

/// The element-hiding half of a filter list — the part a network blocker never
/// reads.
///
/// Cancelling a request removes the ad; it does not remove the space the ad was
/// going to occupy, the "advertisement" label above it, or the container that
/// collapses to a grey box. Those come from `##`-syntax rules, and they are the
/// difference between a page with no ads and a page with no ad-shaped holes.
#[derive(Default)]
pub struct Cosmetic {
    /// Selectors to hide, from rules naming this hostname specifically.
    pub hide_selectors: Vec<String>,
    /// Class and id selectors that generic rules must *not* apply to. Passed
    /// straight back to `generic_hide_selectors`.
    pub exceptions: HashSet<String>,
    /// True when a `$generichide` exception covers this page: hostname rules
    /// still apply, generic ones must not be asked for at all.
    pub generichide: bool,
}

impl Blocker {
    /// A blocker with no rules loaded — allows everything.
    pub fn empty() -> Self {
        Self {
            engine: Engine::new_with_filter_set(FilterSet::new(false)),
        }
    }

    /// Build from raw filter-list text (one list per entry, newline-separated rules).
    pub fn from_lists<I, S>(lists: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut set = FilterSet::new(false);
        for list in lists {
            set.add_filter_list(list.into(), ParseOptions::default());
        }
        Self {
            engine: Engine::new_with_filter_set(set),
        }
    }

    /// Pack the built engine into bytes.
    ///
    /// The engine holds `Rc`/`RefCell` internally, so it is neither `Send` nor
    /// `Sync` and cannot simply be shared with the thread that intercepts
    /// requests. Serialising once and rebuilding per thread costs far less than
    /// re-parsing megabytes of filter text would.
    pub fn serialize(&self) -> Vec<u8> {
        self.engine.serialize()
    }

    /// Rebuild from `serialize` output. Returns `None` if the bytes are not a
    /// engine of this version — callers then fall back to allowing everything.
    pub fn from_serialized(bytes: &[u8]) -> Option<Self> {
        let mut engine = Engine::new_with_filter_set(FilterSet::new(false));
        engine.deserialize(bytes).ok()?;
        Some(Self { engine })
    }

    /// Decide whether a request should be cancelled.
    ///
    /// `request_type` uses the resource-type vocabulary from the filter-list
    /// spec: "script", "image", "stylesheet", "sub_frame", "xmlhttprequest", …
    pub fn should_block(
        &self,
        url: &str,
        source_url: &str,
        request_type: &str,
        method: &str,
    ) -> bool {
        match Request::new(url, source_url, request_type, method) {
            Ok(request) => self.engine.check_network_request(&request).should_block(),
            // A URL the engine can't parse is not something we should silently drop.
            Err(_) => false,
        }
    }

    /// Element-hiding rules that name this page's hostname.
    ///
    /// Generic rules — `##.ad-banner`, matching everywhere — are deliberately
    /// not included: there are hundreds of thousands of them, and serving them
    /// all to every page would cost more than the ads do. They are looked up on
    /// demand from the classes and ids the page actually uses, which is what
    /// `generic_hide_selectors` is for.
    pub fn cosmetic_for(&self, url: &str) -> Cosmetic {
        let resources = self.engine.url_cosmetic_resources(url);
        Cosmetic {
            hide_selectors: resources.hide_selectors.into_iter().collect(),
            exceptions: resources.exceptions,
            generichide: resources.generichide,
            // `procedural_actions` (`:has-text`, `:upward`, style rewrites) are
            // dropped for now: they are a filter language of their own and need
            // an interpreter on the page, not a stylesheet.
        }
    }

    /// Generic rules activated by the classes and ids present on a page.
    ///
    /// `exceptions` comes from the matching [`Cosmetic`]: a site that has an
    /// exception for `.ad` must not have `.ad` hidden by a generic rule.
    pub fn generic_hide_selectors(
        &self,
        classes: &[String],
        ids: &[String],
        exceptions: &HashSet<String>,
    ) -> Vec<String> {
        self.engine
            .hidden_class_id_selectors(classes, ids, exceptions)
    }
}

impl Default for Blocker {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "||ads.example.com^\n||tracker.example/track.js";

    #[test]
    fn blocks_listed_domain() {
        let blocker = Blocker::from_lists([LIST]);
        assert!(blocker.should_block(
            "https://ads.example.com/banner.png",
            "https://news.example",
            "image",
            "GET"
        ));
    }

    #[test]
    fn allows_unlisted_domain() {
        let blocker = Blocker::from_lists([LIST]);
        assert!(!blocker.should_block(
            "https://cdn.example.com/app.js",
            "https://news.example",
            "script",
            "GET"
        ));
    }

    const COSMETIC: &str = "example.com##.ad-slot\n##.generic-ad\nexample.com#@#.sponsored";

    #[test]
    fn hostname_rules_are_served_for_that_hostname_only() {
        let blocker = Blocker::from_lists([COSMETIC]);
        let here = blocker.cosmetic_for("https://example.com/watch");
        assert!(here.hide_selectors.iter().any(|s| s == ".ad-slot"));

        let elsewhere = blocker.cosmetic_for("https://other.test/watch");
        assert!(!elsewhere.hide_selectors.iter().any(|s| s == ".ad-slot"));
    }

    #[test]
    fn generic_rules_come_from_the_classes_a_page_uses() {
        let blocker = Blocker::from_lists([COSMETIC]);
        let present = blocker.generic_hide_selectors(
            &["generic-ad".to_owned()],
            &[],
            &HashSet::new(),
        );
        assert!(present.iter().any(|s| s == ".generic-ad"));

        // A page using none of the classes a rule names gets nothing, which is
        // the point of asking per page rather than shipping every rule.
        let absent =
            blocker.generic_hide_selectors(&["headline".to_owned()], &[], &HashSet::new());
        assert!(absent.is_empty());
    }

    #[test]
    fn cosmetic_rules_survive_the_engine_cache() {
        // The packed engine is what actually runs: it is built once and cached
        // to disk. If serialisation dropped cosmetic rules, element hiding
        // would work on the first run after a list change and silently do
        // nothing every run after that.
        let packed = Blocker::from_lists([COSMETIC]).serialize();
        let restored = Blocker::from_serialized(&packed).expect("engine rebuilds");

        assert!(
            restored
                .cosmetic_for("https://example.com/watch")
                .hide_selectors
                .iter()
                .any(|s| s == ".ad-slot")
        );
        assert!(
            restored
                .generic_hide_selectors(&["generic-ad".to_owned()], &[], &HashSet::new())
                .iter()
                .any(|s| s == ".generic-ad")
        );
    }

    #[test]
    fn empty_blocker_allows_everything() {
        let blocker = Blocker::empty();
        assert!(!blocker.should_block(
            "https://ads.example.com/banner.png",
            "https://news.example",
            "image",
            "GET"
        ));
    }
}
