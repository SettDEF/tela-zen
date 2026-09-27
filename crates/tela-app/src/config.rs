//! Tela's own configuration, under `$XDG_CONFIG_HOME/tela`.
//!
//! ```text
//! ~/.config/tela/
//! ├── config.json          appearance, behaviour, content blocking
//! └── themes/
//!     └── tela-dark.json   colour tokens
//! ```
//!
//! Both files are written with defaults on first run, so there is always
//! something concrete on disk to edit. The theme uses Material You token names,
//! which means a palette can be copied in from any Material You generator —
//! including the one the desktop already runs — but Tela owns the file either
//! way. Set `appearance.followSystemColors` to true to opt into tracking the
//! desktop palette instead.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::SystemTime;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QColor, QString};

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    // Without this, cxx-qt exposes Rust's snake_case names verbatim to QML
    // (`surface_container_low`). Camel-casing at the bridge keeps the Rust side
    // idiomatic and the QML side conventional.
    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        // ── Colour tokens ─────────────────────────────────────────────
        #[qproperty(QColor, background)]
        #[qproperty(QColor, surface)]
        #[qproperty(QColor, surface_container_lowest)]
        #[qproperty(QColor, surface_container_low)]
        #[qproperty(QColor, surface_container)]
        #[qproperty(QColor, surface_container_high)]
        #[qproperty(QColor, surface_container_highest)]
        #[qproperty(QColor, surface_bright)]
        #[qproperty(QColor, background_fg)]
        #[qproperty(QColor, surface_fg)]
        #[qproperty(QColor, surface_variant_fg)]
        #[qproperty(QColor, outline)]
        #[qproperty(QColor, outline_variant)]
        #[qproperty(QColor, primary)]
        #[qproperty(QColor, primary_fg)]
        #[qproperty(QColor, primary_container)]
        #[qproperty(QColor, primary_container_fg)]
        #[qproperty(QColor, secondary)]
        #[qproperty(QColor, secondary_container)]
        #[qproperty(QColor, secondary_container_fg)]
        #[qproperty(QColor, tertiary)]
        #[qproperty(QColor, error)]
        #[qproperty(QColor, error_fg)]
        // ── Appearance ────────────────────────────────────────────────
        #[qproperty(QString, font_main)]
        #[qproperty(QString, font_mono)]
        #[qproperty(QString, font_icon)]
        #[qproperty(i32, icon_size)]
        #[qproperty(i32, rounding_small)]
        #[qproperty(i32, rounding_normal)]
        #[qproperty(i32, rounding_large)]
        #[qproperty(i32, sidebar_width)]
        #[qproperty(i32, rail_width)]
        /// "sidebar" | "sidebar-compact" | "sidebar-auto" | "top"
        #[qproperty(QString, tab_bar_mode)]
        #[qproperty(bool, shy_chrome)]
        /// 0 = fullbleed content, no inset and no rounded card.
        #[qproperty(i32, page_inset)]
        /// Margin around a fullscreen video (media full view). 0 fills the
        /// whole screen; a non-zero value frames it with that gap on every side.
        #[qproperty(i32, full_screen_gap)]
        /// Keep a border around the fullscreen video (needs a non-zero gap and
        /// panel borders enabled to be visible). Off by default: a clean rect.
        #[qproperty(bool, full_screen_border)]
        /// Round the fullscreen video's corners. Off by default.
        #[qproperty(bool, full_screen_rounded)]
        #[qproperty(bool, border_enabled)]
        #[qproperty(i32, border_width)]
        #[qproperty(i32, animation_speed)]
        #[qproperty(bool, right_bar_enabled)]
        #[qproperty(i32, right_bar_width)]
        #[qproperty(bool, follow_system_colors)]
        // ── Tabs ──────────────────────────────────────────────────────
        #[qproperty(i32, tab_height)]
        #[qproperty(bool, show_favicons)]
        /// "hover" | "always" | "never"
        #[qproperty(QString, close_button_mode)]
        #[qproperty(i32, auto_archive_days)]
        // ── Performance ───────────────────────────────────────────────
        /// Suspend a background tab after this many minutes idle, releasing its
        /// renderer process. 0 disables suspension.
        #[qproperty(i32, suspend_after_minutes)]
        // ── Behaviour ─────────────────────────────────────────────────
        #[qproperty(bool, restore_session)]
        #[qproperty(QString, home_url)]
        #[qproperty(QString, search_template)]
        #[qproperty(bool, blocking_enabled)]
        /// Inject extension content scripts into pages.
        ///
        /// Off by default: content scripts currently call
        /// runtime.sendMessage and get no reply, so extensions that depend on
        /// a response fail partway through — and page-restyling ones leave the
        /// page visibly broken when they do. Background contexts are
        /// unaffected and stay on.
        #[qproperty(bool, inject_content_scripts)]
        /// Let pages call window.open without a user gesture. Off: this is how
        /// ad popups arrive, and filter lists cannot see them.
        #[qproperty(bool, allow_popups)]
        /// System theming from a colour picked in the browser. Off by default:
        /// it is the only path from the browser to a system command.
        #[qproperty(bool, system_theme_enabled)]
        #[qproperty(QString, system_theme_extension)]
        #[qproperty(QString, system_theme_script)]
        /// "" keeps Qt's default. "firefox" sends a Firefox UA.
        #[qproperty(QString, user_agent_mode)]
        #[qproperty(QString, user_agent)]
        // ── Start page ────────────────────────────────────────────────
        #[qproperty(bool, start_page_enabled)]
        #[qproperty(QString, start_greeting)]
        #[qproperty(i32, start_columns)]
        /// Shortcut tiles, passed through as raw JSON for QML to parse.
        /// A list model would be nicer; this keeps the bridge scalar-only
        /// until tiles need reordering from the UI.
        #[qproperty(QString, start_shortcuts_json)]
        /// none | aurora | stars | mesh
        /// Your own HTML start page. Empty means the built-in one.
        #[qproperty(QString, start_custom_url)]
        /// The profile this run uses; empty for the default one.
        #[qproperty(QString, profile_name)]
        /// Every profile on disk, as a JSON array.
        #[qproperty(QString, profile_list_json)]
        #[qproperty(QString, start_background)]
        #[qproperty(f64, start_background_intensity)]
        /// Hostnames exempt from content blocking, as a JSON array.
        #[qproperty(QString, blocking_allowlist_json)]
        /// Bookmark subscription URLs, as a JSON array.
        #[qproperty(QString, bookmark_subscriptions_json)]
        #[qproperty(i32, bookmark_sync_hours)]
        /// A URL passed on the command line, or "" — used once at startup.
        #[qproperty(QString, launch_url)]
        // ── Status ────────────────────────────────────────────────────
        #[qproperty(QString, config_dir)]
        type Config = super::ConfigRust;

        /// Re-read config and theme if either changed on disk. Cheap on a timer:
        /// it stats the files and returns early when nothing moved.
        #[qinvokable]
        fn reload(self: Pin<&mut Config>) -> bool;

        /// Write the default config and theme if they don't exist yet.
        #[qinvokable]
        fn ensure_defaults(self: Pin<&mut Config>) -> bool;

        /// Write one setting back to config.json and reload.
        ///
        /// `path` is dot-separated (`appearance.tabBar.mode`); `json_value` is
        /// the value JSON-encoded, so QML can pass bools, numbers and strings
        /// through a single entry point.
        #[qinvokable]
        fn set_setting(self: Pin<&mut Config>, path: &QString, json_value: &QString) -> bool;

        /// Start another instance on `name`, the way Firefox opens a profile
        /// in its own window. Creates it first if it does not exist.
        #[qinvokable]
        fn open_profile(self: Pin<&mut Config>, name: &QString) -> bool;

        /// Open a URL in the system's real browser — Firefox or Chrome — rather
        /// than in Tela. For pages Tela's engine cannot get past, chiefly
        /// Google sign-in, which blocks every Qt WebEngine browser alike.
        /// Returns false if no standalone browser was found.
        #[qinvokable]
        fn open_in_system_browser(self: Pin<&mut Config>, url: &QString) -> bool;

        /// The name of the browser `open_in_system_browser` would use, for the
        /// button label. Empty if none is installed.
        #[qinvokable]
        fn system_browser_name(self: Pin<&mut Config>) -> QString;

        /// Whether Tela is the system's default web browser.
        #[qinvokable]
        fn is_default_browser(self: Pin<&mut Config>) -> bool;

        /// Make Tela the default handler for web links and http/https. Returns
        /// false if the desktop tooling to do so is missing.
        #[qinvokable]
        fn set_as_default_browser(self: Pin<&mut Config>) -> bool;
    }
}

pub struct ConfigRust {
    background: QColor,
    surface: QColor,
    surface_container_lowest: QColor,
    surface_container_low: QColor,
    surface_container: QColor,
    surface_container_high: QColor,
    surface_container_highest: QColor,
    surface_bright: QColor,
    background_fg: QColor,
    surface_fg: QColor,
    surface_variant_fg: QColor,
    outline: QColor,
    outline_variant: QColor,
    primary: QColor,
    primary_fg: QColor,
    primary_container: QColor,
    primary_container_fg: QColor,
    secondary: QColor,
    secondary_container: QColor,
    secondary_container_fg: QColor,
    tertiary: QColor,
    error: QColor,
    error_fg: QColor,

    font_main: QString,
    font_mono: QString,
    font_icon: QString,
    icon_size: i32,
    rounding_small: i32,
    rounding_normal: i32,
    rounding_large: i32,
    sidebar_width: i32,
    rail_width: i32,
    tab_bar_mode: QString,
    shy_chrome: bool,
    page_inset: i32,
    full_screen_gap: i32,
    full_screen_border: bool,
    full_screen_rounded: bool,
    border_enabled: bool,
    border_width: i32,
    animation_speed: i32,
    right_bar_enabled: bool,
    right_bar_width: i32,

    tab_height: i32,
    show_favicons: bool,
    close_button_mode: QString,
    auto_archive_days: i32,
    suspend_after_minutes: i32,
    restore_session: bool,
    follow_system_colors: bool,

    home_url: QString,
    search_template: QString,
    blocking_enabled: bool,
    inject_content_scripts: bool,
    allow_popups: bool,
    system_theme_enabled: bool,
    system_theme_extension: QString,
    system_theme_script: QString,
    user_agent_mode: QString,
    user_agent: QString,

    start_page_enabled: bool,
    start_greeting: QString,
    start_columns: i32,
    start_shortcuts_json: QString,
    start_custom_url: QString,
    profile_name: QString,
    profile_list_json: QString,
    start_background: QString,
    start_background_intensity: f64,
    blocking_allowlist_json: QString,
    bookmark_subscriptions_json: QString,
    bookmark_sync_hours: i32,
    launch_url: QString,

    config_dir: QString,

    config_modified: Option<SystemTime>,
    theme_modified: Option<SystemTime>,
    theme_name: String,
}

// ── Defaults ───────────────────────────────────────────────────────────

/// Shipped theme. Warm neutral dark, Material You token names.
///
/// Note the `r##"…"##` delimiter: every value here contains `"#`, which would
/// close a single-hash raw string mid-literal.
const DEFAULT_THEME: &str = r##"{
  "background": "#141311",
  "surface": "#141311",
  "surface_container_lowest": "#0f0e0c",
  "surface_container_low": "#1c1b19",
  "surface_container": "#211f1d",
  "surface_container_high": "#2b2a27",
  "surface_container_highest": "#363532",
  "surface_bright": "#3b3936",
  "on_background": "#e6e2dd",
  "on_surface": "#e6e2dd",
  "on_surface_variant": "#cac6c1",
  "outline": "#93908c",
  "outline_variant": "#484643",
  "primary": "#cec6ab",
  "on_primary": "#34311d",
  "primary_container": "#4b4732",
  "on_primary_container": "#ebe2c6",
  "secondary": "#ccc6b5",
  "secondary_container": "#4a4739",
  "on_secondary_container": "#e9e2d0",
  "tertiary": "#d0c7a2",
  "error": "#ffb4ab",
  "on_error": "#690005"
}
"##;

/// Tiles shown on a fresh install. Deliberately generic.
/// Bookmark files to follow. A subscription is merged, never authoritative:
/// nothing you added yourself is removed by a sync.
const DEFAULT_SUBSCRIPTIONS: &str = r#"[
  "https://raw.githubusercontent.com/fmhy/bookmarks/main/fmhy_in_bookmarks_starred_only.html"
]"#;

const DEFAULT_SHORTCUTS: &str = r#"[
  {"title": "Search", "url": "https://duckduckgo.com", "glyph": "search"},
  {"title": "Mail", "url": "https://mail.google.com", "glyph": "mail"},
  {"title": "Calendar", "url": "https://calendar.google.com", "glyph": "calendar_month"},
  {"title": "GitHub", "url": "https://github.com", "glyph": "code"},
  {"title": "Docs", "url": "https://doc.qt.io/qt-6/", "glyph": "menu_book"},
  {"title": "Video", "url": "https://youtube.com", "glyph": "smart_display"}
]"#;

const DEFAULT_CONFIG: &str = r#"{
  "appearance": {
    "theme": "tela-dark",
    "followSystemColors": false,
    "fonts": {
      "main": "Google Sans Flex",
      "monospace": "JetBrains Mono NF",
      "icon": "Material Symbols Rounded"
    },
    "iconSize": 20,
    "rounding": {
      "small": 12,
      "normal": 17,
      "large": 23
    },
    "sidebarWidth": 260,
    "railWidth": 52,
    "tabBar": {
      "mode": "sidebar",
      "shyChrome": false
    },
    "pageInset": 8,
    "fullScreenGap": 0,
    "fullScreenBorder": false,
    "fullScreenRounded": false,
    "border": {
      "enabled": true,
      "width": 1
    },
    "animationSpeed": 100,
    "rightBar": {
      "enabled": true,
      "width": 48
    }
  },
  "tabs": {
    "height": 34,
    "showFavicons": true,
    "closeButton": "hover",
    "autoArchiveDays": 0
  },
  "performance": {
    "suspendAfterMinutes": 30
  },
  "behavior": {
    "restoreSession": true,
    "homeUrl": "about:blank",
    "searchTemplate": "https://duckduckgo.com/?q={query}",
    "userAgent": "chrome"
  },
  "blocking": {
    "enabled": true,
    "allowlist": []
  },
  "extensions": {
    "injectContentScripts": true,
    "allowPopups": false
  },
  "integration": {
    "systemTheme": {
      "enabled": false,
      "extension": "tinct-picker",
      "applyScript": "~/.config/quickshell/ii/scripts/colors/applycolor.sh"
    }
  },
  "bookmarks": {
    "subscriptions": [
      "https://raw.githubusercontent.com/fmhy/bookmarks/main/fmhy_in_bookmarks_starred_only.html"
    ],
    "syncHours": 24
  },
  "startPage": {
    "enabled": true,
    "custom": "",
    "background": "aurora",
    "backgroundIntensity": 1.0,
    "greeting": "",
    "columns": 6,
    "shortcuts": [
      {"title": "Search", "url": "https://duckduckgo.com", "glyph": "search"},
      {"title": "Mail", "url": "https://mail.google.com", "glyph": "mail"},
      {"title": "Calendar", "url": "https://calendar.google.com", "glyph": "calendar_month"},
      {"title": "GitHub", "url": "https://github.com", "glyph": "code"},
      {"title": "Docs", "url": "https://doc.qt.io/qt-6/", "glyph": "menu_book"},
      {"title": "Video", "url": "https://youtube.com", "glyph": "smart_display"}
    ]
  }
}
"#;

/// Identifies as Firefox on a matching platform.
///
/// addons.mozilla.org — and a fair number of other sites — gate on this string
/// and will not serve an XPI to a browser they read as Chromium. Since Tela's
/// extension format *is* Firefox's, claiming it is defensible. The tradeoff is
/// real though: sites may then send Gecko-specific CSS and JS to what is a
/// Chromium engine. Set `behavior.userAgent` to "default" to stop.
const FIREFOX_UA: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:149.0) Gecko/20100101 Firefox/149.0";

/// The Firefox User-Agent presented on Google's sign-in host — see
/// `set_signin_user_agent` and the interceptor. A recent Firefox, because
/// Google's block, like everyone's, eventually flags stale versions too.
pub fn signin_user_agent() -> &'static str {
    FIREFOX_UA
}

/// Build a Chrome User-Agent that agrees with the engine actually running.
///
/// The engine here *is* Chromium, so claiming Chrome is the one choice where
/// every signal a site can check lines up: `navigator.vendor` is already
/// "Google Inc.", `window.chrome` already exists, and `navigator.userAgentData`
/// already reports Chromium. Claiming Firefox contradicts all three, and three
/// lines of JavaScript are enough to notice — which is why sign-in pages that
/// screen for embedded webviews reject it.
///
/// Only the major version is used. Chrome's own UA has frozen the minor parts
/// at ".0.0.0" since v101, so a full version string would itself stand out.
pub fn chrome_ua(chromium_version: &str) -> String {
    let major = chromium_version
        .split('.')
        .next()
        .filter(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        // Only reached if Qt reports something unparseable; a plausible recent
        // major is better than emitting a malformed UA.
        .unwrap_or("140");

    format!(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
         (KHTML, like Gecko) Chrome/{major}.0.0.0 Safari/537.36"
    )
}

/// A real standalone browser to hand a URL off to, never Tela itself — which
/// would loop, and cannot sign in to Google anyway. Firefox first: it is the
/// one most likely to be accepted where Tela is not.
fn detect_system_browser() -> Option<String> {
    const CANDIDATES: [&str; 6] = [
        "firefox",
        "librewolf",
        "google-chrome-stable",
        "google-chrome",
        "chromium",
        "chromium-browser",
    ];
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for bin in CANDIDATES {
            let candidate = dir.join(bin);
            if candidate.is_file() {
                return Some(bin.to_owned());
            }
        }
    }
    None
}

/// A display name for the button, from the binary name.
fn friendly_browser_name(binary: &str) -> &'static str {
    match binary {
        "firefox" => "Firefox",
        "librewolf" => "LibreWolf",
        "google-chrome-stable" | "google-chrome" => "Chrome",
        "chromium" | "chromium-browser" => "Chromium",
        _ => "your browser",
    }
}

/// The Chrome version Tela claims to be — the real one installed here, if there
/// is one.
///
/// Qt's bundled Chromium runs several major versions behind the current Chrome
/// release, and Google's sign-in flow rejects a browser claiming a stale
/// version: no real Chrome is ever that far behind, because it auto-updates. So
/// when a Google Chrome (or Chromium) is installed, claim *its* exact version.
/// Tela then presents the same identity as the browser already on the machine,
/// and follows it forward as it updates. With none installed, fall back to the
/// engine's own version, which is at least truthful.
///
/// Detected once: it shells out to the browser, which is cheap but not free.
pub fn claimed_chrome_version() -> String {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION
        .get_or_init(|| {
            for bin in ["google-chrome-stable", "google-chrome", "chromium", "chromium-browser"] {
                let Ok(out) = std::process::Command::new(bin).arg("--version").output() else {
                    continue;
                };
                if !out.status.success() {
                    continue;
                }
                let Ok(text) = String::from_utf8(out.stdout) else {
                    continue;
                };
                // "Google Chrome 150.0.7871.186" → the dotted version token.
                if let Some(v) = text.split_whitespace().find(|token| {
                    token.contains('.')
                        && token.chars().next().is_some_and(|c| c.is_ascii_digit())
                }) {
                    return v.trim().to_owned();
                }
            }
            crate::shell::chromium_version()
        })
        .clone()
}

/// The three version-bearing client-hint headers, matched to a real Chrome.
///
/// Chromium builds these from its own product name and version — "Chromium",
/// several majors stale — so all three disagree with a Chrome that a site
/// expects. Returned as `(sec-ch-ua, sec-ch-ua-full-version-list,
/// sec-ch-ua-full-version)` for the interceptor to set on every request. Brand
/// order matches Chrome's: the greased entry, then Chromium, then Google Chrome.
pub fn client_hint_headers() -> (String, String, String) {
    let full = claimed_chrome_version();
    let major = full.split('.').next().unwrap_or("140");
    let short = format!(
        "\"Not;A=Brand\";v=\"8\", \"Chromium\";v=\"{major}\", \"Google Chrome\";v=\"{major}\""
    );
    let list = format!(
        "\"Not;A=Brand\";v=\"8.0.0.0\", \"Chromium\";v=\"{full}\", \"Google Chrome\";v=\"{full}\""
    );
    (short, list, full)
}

fn hex(value: &str) -> Option<QColor> {
    let value = value.trim().trim_start_matches('#');
    if value.len() != 6 {
        return None;
    }
    let component = |range: std::ops::Range<usize>| i32::from_str_radix(&value[range], 16).ok();
    Some(QColor::from_rgb(
        component(0..2)?,
        component(2..4)?,
        component(4..6)?,
    ))
}

fn baked(value: &str) -> QColor {
    hex(value).unwrap_or_else(|| QColor::from_rgb(0, 0, 0))
}

/// First command-line argument that looks like something to open.
///
/// Deliberately strict: a desktop file, a browser MIME handler and `xdg-open`
/// all pass a URL here, and anything else would be an odd thing to navigate to.
/// Options are skipped so future flags do not get treated as addresses.
pub(crate) fn launch_url_from_args() -> String {
    for arg in std::env::args().skip(1) {
        if arg.starts_with('-') {
            continue;
        }
        let lower = arg.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("file://")
        {
            return arg;
        }
        // A bare path from a file manager.
        if std::path::Path::new(&arg).exists() {
            if let Ok(absolute) = std::fs::canonicalize(&arg) {
                return format!("file://{}", absolute.to_string_lossy());
            }
        }
    }
    String::new()
}

/// The User-Agent to send, read straight from config.json at startup.
///
/// Qt's default advertises "QtWebEngine", which some large sites do not
/// recognise and answer with a blank page. Identifying as Firefox is the
/// default because Tela's extension format is Firefox's; set
/// `behavior.userAgent` to "default" to send Qt's instead.
pub fn configured_user_agent() -> String {
    let Some(dir) = config_dir() else {
        return FIREFOX_UA.to_owned();
    };
    let configured = std::fs::read_to_string(dir.join("config.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| json["behavior"]["userAgent"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "firefox".to_owned());

    match configured.as_str() {
        "" | "default" => String::new(),
        // Matches the real Chrome installed here, so the version is current
        // rather than the engine's stale one — see claimed_chrome_version.
        "chrome" => chrome_ua(&claimed_chrome_version()),
        // Kept because it is occasionally useful for testing how a site treats
        // Gecko — but it is detectable, and sign-in pages will reject it.
        "firefox" => FIREFOX_UA.to_owned(),
        custom => custom.to_owned(),
    }
}

/// Languages to advertise. Derived from the locale so it matches the rest of
/// the session rather than asserting an unrelated one.
pub fn accept_language() -> String {
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();

    // "de_DE.UTF-8" -> "de-DE"
    let tag = locale
        .split('.')
        .next()
        .unwrap_or("")
        .replace('_', "-");

    if tag.is_empty() || tag == "C" || tag == "POSIX" {
        return "en-US,en;q=0.9".to_owned();
    }

    let base = tag.split('-').next().unwrap_or("en").to_ascii_lowercase();
    if base == "en" {
        format!("{tag},en;q=0.9")
    } else {
        // Real browsers list the base language, then English as a fallback.
        format!("{tag},{base};q=0.9,en-US;q=0.8,en;q=0.7")
    }
}

/// System-theming settings, read at call time so a config edit takes effect
/// without a restart.
pub struct SystemThemeSettings {
    pub enabled: bool,
    pub extension: String,
    pub script: String,
}

pub fn system_theme_settings() -> SystemThemeSettings {
    let json = config_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join("config.json")).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let theme = &json["integration"]["systemTheme"];

    SystemThemeSettings {
        enabled: theme["enabled"].as_bool().unwrap_or(false),
        extension: theme["extension"].as_str().unwrap_or("tinct-picker").to_owned(),
        script: theme["applyScript"].as_str().unwrap_or("").to_owned(),
    }
}

/// The profile this run uses: `--profile <name>`, or `TELA_PROFILE`.
///
/// Empty means the default one. A name is restricted to a single path segment;
/// it selects a directory, so it must not be able to name another.
pub fn profile_name() -> String {
    let mut args = std::env::args().skip(1);
    let mut chosen = String::new();
    while let Some(arg) = args.next() {
        if let Some(name) = arg.strip_prefix("--profile=") {
            chosen = name.to_owned();
        } else if arg == "--profile" {
            chosen = args.next().unwrap_or_default();
        }
    }
    if chosen.is_empty() {
        chosen = std::env::var("TELA_PROFILE").unwrap_or_default();
    }

    let chosen = chosen.trim();
    if chosen.is_empty()
        || chosen == "default"
        || chosen.contains(['/', '\\'])
        || chosen.starts_with('.')
    {
        return String::new();
    }
    chosen.to_owned()
}

/// Everything this profile owns — settings, extensions, bookmarks, session.
///
/// The default profile keeps the original location, so existing installs are
/// untouched; named ones live beside it under `profiles/`.
pub fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    let root = base.join("tela");

    let name = profile_name();
    if name.is_empty() {
        Some(root)
    } else {
        Some(root.join("profiles").join(name))
    }
}

/// Profiles that exist on disk, the default one first.
pub fn profile_names() -> Vec<String> {
    let mut out = vec!["default".to_owned()];
    let Some(base) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    else {
        return out;
    };
    if let Ok(entries) = std::fs::read_dir(base.join("tela").join("profiles")) {
        let mut named: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        named.sort();
        out.extend(named);
    }
    out
}

/// The desktop's Material You output, read only when the user opts in.
fn system_colors_path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))?;
    Some(state.join("quickshell/user/generated/colors.json"))
}

impl Default for ConfigRust {
    fn default() -> Self {
        let dir = config_dir()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();

        Self {
            background: baked("#141311"),
            surface: baked("#141311"),
            surface_container_lowest: baked("#0f0e0c"),
            surface_container_low: baked("#1c1b19"),
            surface_container: baked("#211f1d"),
            surface_container_high: baked("#2b2a27"),
            surface_container_highest: baked("#363532"),
            surface_bright: baked("#3b3936"),
            background_fg: baked("#e6e2dd"),
            surface_fg: baked("#e6e2dd"),
            surface_variant_fg: baked("#cac6c1"),
            outline: baked("#93908c"),
            outline_variant: baked("#484643"),
            primary: baked("#cec6ab"),
            primary_fg: baked("#34311d"),
            primary_container: baked("#4b4732"),
            primary_container_fg: baked("#ebe2c6"),
            secondary: baked("#ccc6b5"),
            secondary_container: baked("#4a4739"),
            secondary_container_fg: baked("#e9e2d0"),
            tertiary: baked("#d0c7a2"),
            error: baked("#ffb4ab"),
            error_fg: baked("#690005"),

            font_main: QString::from("Google Sans Flex"),
            font_mono: QString::from("JetBrains Mono NF"),
            font_icon: QString::from("Material Symbols Rounded"),
            icon_size: 20,
            rounding_small: 12,
            rounding_normal: 17,
            rounding_large: 23,
            sidebar_width: 260,
            rail_width: 52,
            tab_bar_mode: QString::from("sidebar"),
            shy_chrome: false,
            page_inset: 8,
            full_screen_gap: 0,
            full_screen_border: false,
            full_screen_rounded: false,
            border_enabled: true,
            border_width: 1,
            animation_speed: 100,
            right_bar_enabled: true,
            right_bar_width: 48,

            tab_height: 34,
            show_favicons: true,
            close_button_mode: QString::from("hover"),
            auto_archive_days: 0,
            suspend_after_minutes: 30,
            restore_session: true,
            follow_system_colors: false,

            home_url: QString::from("about:blank"),
            search_template: QString::from("https://duckduckgo.com/?q={query}"),
            blocking_enabled: true,
            inject_content_scripts: true,
            allow_popups: false,
            system_theme_enabled: false,
            system_theme_extension: QString::from("tinct-picker"),
            system_theme_script: QString::from(
                "~/.config/quickshell/ii/scripts/colors/applycolor.sh",
            ),
            user_agent_mode: QString::from("chrome"),
            user_agent: QString::from(FIREFOX_UA),

            start_page_enabled: true,
            start_greeting: QString::from(""),
            start_columns: 6,
            start_shortcuts_json: QString::from(DEFAULT_SHORTCUTS),
            start_custom_url: QString::default(),
            profile_name: QString::from(&profile_name()),
            profile_list_json: QString::from(
                &serde_json::to_string(&profile_names()).unwrap_or_else(|_| "[]".into()),
            ),
            start_background: QString::from("aurora"),
            start_background_intensity: 1.0,
            blocking_allowlist_json: QString::from("[]"),
            bookmark_subscriptions_json: QString::from(DEFAULT_SUBSCRIPTIONS),
            bookmark_sync_hours: 24,
            launch_url: QString::from(&launch_url_from_args()),

            config_dir: QString::from(&dir),

            config_modified: None,
            theme_modified: None,
            theme_name: "tela-dark".to_owned(),
        }
    }
}

/// Set a dot-separated path inside a JSON object, creating intermediate
/// objects as needed. Returns false for an empty path.
///
/// Split out from the invokable so it can be tested without a Qt runtime.
fn apply_path(root: &mut serde_json::Value, path: &str, value: serde_json::Value) -> bool {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    let Some((last, parents)) = segments.split_last() else {
        return false;
    };

    let mut cursor = root;
    for segment in parents {
        if !cursor[*segment].is_object() {
            cursor[*segment] = serde_json::json!({});
        }
        cursor = &mut cursor[*segment];
    }
    cursor[*last] = value;
    true
}

fn modified_at(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

/// Create `~/.config/tela` with default files if they don't exist.
///
/// Called from `main()` rather than QML: the config has to exist before the
/// first frame reads it, and seeding shouldn't depend on component lifecycle.
pub fn ensure_default_files() -> bool {
    let Some(dir) = config_dir() else {
        return false;
    };
    let themes = dir.join("themes");
    if std::fs::create_dir_all(&themes).is_err() {
        return false;
    }

    let mut wrote = false;
    let config_file = dir.join("config.json");
    if !config_file.exists() && std::fs::write(&config_file, DEFAULT_CONFIG).is_ok() {
        wrote = true;
    }
    let theme_file = themes.join("tela-dark.json");
    if !theme_file.exists() && std::fs::write(&theme_file, DEFAULT_THEME).is_ok() {
        wrote = true;
    }
    wrote
}

impl ffi::Config {
    fn ensure_defaults(self: Pin<&mut Self>) -> bool {
        ensure_default_files()
    }

    fn open_profile(self: Pin<&mut Self>, name: &QString) -> bool {
        let name = name.to_string();
        let name = name.trim();
        // The name selects a directory, so it must not be able to name another.
        if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
            return false;
        }

        if name != "default" {
            let Some(base) = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            else {
                return false;
            };
            if std::fs::create_dir_all(base.join("tela").join("profiles").join(name)).is_err() {
                return false;
            }
        }

        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        let mut command = std::process::Command::new(exe);
        if name != "default" {
            command.arg("--profile").arg(name);
        }
        command.spawn().is_ok()
    }

    fn open_in_system_browser(self: Pin<&mut Self>, url: &QString) -> bool {
        let url = url.to_string();
        // http/https only, so this can never be turned into a way to launch an
        // arbitrary program with arbitrary arguments.
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return false;
        }
        let Some(browser) = detect_system_browser() else {
            return false;
        };
        std::process::Command::new(browser)
            .arg(&url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
    }

    fn system_browser_name(self: Pin<&mut Self>) -> QString {
        QString::from(
            detect_system_browser()
                .map(|b| friendly_browser_name(&b))
                .unwrap_or_default(),
        )
    }

    fn is_default_browser(self: Pin<&mut Self>) -> bool {
        std::process::Command::new("xdg-settings")
            .args(["get", "default-web-browser"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim() == "tela.desktop")
            .unwrap_or(false)
    }

    fn set_as_default_browser(self: Pin<&mut Self>) -> bool {
        // xdg-settings is the portable way; it updates the desktop default and
        // the http/https scheme handlers together.
        let ok = std::process::Command::new("xdg-settings")
            .args(["set", "default-web-browser", "tela.desktop"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        // Belt and braces: some environments honour the mimeapps entry but not
        // xdg-settings, so register the scheme handlers directly too. Best
        // effort — its failure does not undo a successful xdg-settings.
        let _ = std::process::Command::new("xdg-mime")
            .args([
                "default",
                "tela.desktop",
                "x-scheme-handler/http",
                "x-scheme-handler/https",
                "text/html",
            ])
            .status();

        ok
    }

    fn set_setting(mut self: Pin<&mut Self>, path: &QString, json_value: &QString) -> bool {
        let Some(dir) = config_dir() else {
            return false;
        };
        let file = dir.join("config.json");

        let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_value.to_string()) else {
            return false;
        };

        // Start from what's on disk so concurrent hand-edits aren't clobbered
        // by whatever this process last parsed.
        let mut root = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .unwrap_or_else(|| serde_json::json!({}));

        if !root.is_object() {
            root = serde_json::json!({});
        }

        if !apply_path(&mut root, &path.to_string(), value) {
            return false;
        }

        let Ok(text) = serde_json::to_string_pretty(&root) else {
            return false;
        };

        // Write via a temp file in the same directory, then rename: a crash
        // mid-write must not leave a truncated config behind.
        let temp = file.with_extension("json.tmp");
        if std::fs::write(&temp, text + "\n").is_err() {
            return false;
        }
        if std::fs::rename(&temp, &file).is_err() {
            let _ = std::fs::remove_file(&temp);
            return false;
        }

        // Force the next reload to re-read rather than trust the stamp.
        self.as_mut().rust_mut().config_modified = None;
        self.reload();
        true
    }

    fn reload(mut self: Pin<&mut Self>) -> bool {
        let Some(dir) = config_dir() else {
            return false;
        };

        let config_file = dir.join("config.json");
        let config_stamp = modified_at(&config_file);
        let config_changed = config_stamp != self.rust().config_modified;

        if config_changed {
            if let Some(json) = std::fs::read_to_string(&config_file)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            {
                self.as_mut().apply_settings(&json);
            }
            self.as_mut().rust_mut().config_modified = config_stamp;
        }

        // Theme source depends on a setting the block above may have just changed.
        let theme_file = if self.rust().follow_system_colors {
            system_colors_path()
        } else {
            let name = self.rust().theme_name.clone();
            Some(dir.join("themes").join(format!("{name}.json")))
        };

        let Some(theme_file) = theme_file else {
            return config_changed;
        };
        let theme_stamp = modified_at(&theme_file);
        let theme_changed = theme_stamp != self.rust().theme_modified;

        if theme_changed {
            if let Some(json) = std::fs::read_to_string(&theme_file)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            {
                self.as_mut().apply_theme(&json);
            }
            self.as_mut().rust_mut().theme_modified = theme_stamp;
        }

        config_changed || theme_changed
    }

    fn apply_settings(mut self: Pin<&mut Self>, json: &serde_json::Value) {
        let appearance = &json["appearance"];

        if let Some(name) = appearance["theme"].as_str() {
            // Switching theme must invalidate the cached stamp, or the new file
            // is never read when both happen to share a modification time.
            if name != self.rust().theme_name {
                self.as_mut().rust_mut().theme_name = name.to_owned();
                self.as_mut().rust_mut().theme_modified = None;
            }
        }
        if let Some(follow) = appearance["followSystemColors"].as_bool() {
            if follow != self.rust().follow_system_colors {
                self.as_mut().set_follow_system_colors(follow);
                self.as_mut().rust_mut().theme_modified = None;
            }
        }
        if let Some(font) = appearance["fonts"]["main"].as_str() {
            self.as_mut().set_font_main(QString::from(font));
        }
        if let Some(font) = appearance["fonts"]["monospace"].as_str() {
            self.as_mut().set_font_mono(QString::from(font));
        }
        if let Some(font) = appearance["fonts"]["icon"].as_str() {
            self.as_mut().set_font_icon(QString::from(font));
        }
        if let Some(value) = appearance["iconSize"].as_i64() {
            self.as_mut().set_icon_size(value.clamp(10, 48) as i32);
        }
        if let Some(value) = appearance["rounding"]["small"].as_i64() {
            self.as_mut().set_rounding_small(value as i32);
        }
        if let Some(value) = appearance["rounding"]["normal"].as_i64() {
            self.as_mut().set_rounding_normal(value as i32);
        }
        if let Some(value) = appearance["rounding"]["large"].as_i64() {
            self.as_mut().set_rounding_large(value as i32);
        }
        if let Some(value) = appearance["sidebarWidth"].as_i64() {
            self.as_mut().set_sidebar_width(value as i32);
        }
        if let Some(value) = appearance["railWidth"].as_i64() {
            self.as_mut().set_rail_width(value as i32);
        }
        if let Some(value) = appearance["tabBar"]["mode"].as_str() {
            // Unknown values fall back to the docked sidebar rather than
            // leaving the UI with no tab bar at all.
            let mode = match value {
                "sidebar" | "sidebar-compact" | "sidebar-auto" | "top" => value,
                _ => "sidebar",
            };
            self.as_mut().set_tab_bar_mode(QString::from(mode));
        }
        if let Some(value) = appearance["tabBar"]["shyChrome"].as_bool() {
            self.as_mut().set_shy_chrome(value);
        }
        if let Some(value) = appearance["pageInset"].as_i64() {
            self.as_mut().set_page_inset(value.clamp(0, 64) as i32);
        }
        if let Some(value) = appearance["fullScreenGap"].as_i64() {
            self.as_mut().set_full_screen_gap(value.clamp(0, 200) as i32);
        }
        if let Some(value) = appearance["fullScreenBorder"].as_bool() {
            self.as_mut().set_full_screen_border(value);
        }
        if let Some(value) = appearance["fullScreenRounded"].as_bool() {
            self.as_mut().set_full_screen_rounded(value);
        }
        if let Some(value) = appearance["border"]["enabled"].as_bool() {
            self.as_mut().set_border_enabled(value);
        }
        if let Some(value) = appearance["border"]["width"].as_i64() {
            self.as_mut().set_border_width(value.clamp(0, 8) as i32);
        }
        if let Some(value) = appearance["animationSpeed"].as_i64() {
            self.as_mut().set_animation_speed(value.clamp(0, 300) as i32);
        }
        if let Some(value) = appearance["rightBar"]["enabled"].as_bool() {
            self.as_mut().set_right_bar_enabled(value);
        }
        if let Some(value) = appearance["rightBar"]["width"].as_i64() {
            self.as_mut().set_right_bar_width(value as i32);
        }

        let tabs = &json["tabs"];
        if let Some(value) = tabs["height"].as_i64() {
            self.as_mut().set_tab_height(value.clamp(20, 80) as i32);
        }
        if let Some(value) = tabs["showFavicons"].as_bool() {
            self.as_mut().set_show_favicons(value);
        }
        if let Some(value) = tabs["closeButton"].as_str() {
            let mode = match value {
                "hover" | "always" | "never" => value,
                _ => "hover",
            };
            self.as_mut().set_close_button_mode(QString::from(mode));
        }
        if let Some(value) = tabs["autoArchiveDays"].as_i64() {
            self.as_mut().set_auto_archive_days(value.clamp(0, 365) as i32);
        }

        if let Some(value) = json["performance"]["suspendAfterMinutes"].as_i64() {
            self.as_mut()
                .set_suspend_after_minutes(value.clamp(0, 1440) as i32);
        }

        let behavior = &json["behavior"];
        if let Some(value) = behavior["restoreSession"].as_bool() {
            self.as_mut().set_restore_session(value);
        }
        if let Some(value) = behavior["homeUrl"].as_str() {
            self.as_mut().set_home_url(QString::from(value));
        }
        if let Some(value) = behavior["searchTemplate"].as_str() {
            self.as_mut().set_search_template(QString::from(value));
        }
        if let Some(value) = behavior["userAgent"].as_str() {
            self.as_mut().set_user_agent_mode(QString::from(value));
            let resolved = match value {
                "" | "default" => "",
                "firefox" => FIREFOX_UA,
                custom => custom,
            };
            self.as_mut().set_user_agent(QString::from(resolved));
        }

        let blocking = &json["blocking"];
        let blocking_enabled = blocking["enabled"].as_bool().unwrap_or(true);
        self.as_mut().set_blocking_enabled(blocking_enabled);
        // Hand both to the interceptor. Without this the shield in the toolbar
        // toggles a property nothing on the request path ever reads.
        let allowlist: Vec<String> = blocking["allowlist"]
            .as_array()
            .map(|hosts| {
                hosts
                    .iter()
                    .filter_map(|host| host.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        self.as_mut()
            .set_blocking_allowlist_json(QString::from(&serde_json::json!(allowlist).to_string()));
        crate::blocking::configure(blocking_enabled, allowlist);
        if let Some(value) = json["extensions"]["injectContentScripts"].as_bool() {
            self.as_mut().set_inject_content_scripts(value);
        }
        if let Some(value) = json["behavior"]["allowPopups"].as_bool() {
            self.as_mut().set_allow_popups(value);
        }

        let theme = &json["integration"]["systemTheme"];
        if let Some(value) = theme["enabled"].as_bool() {
            self.as_mut().set_system_theme_enabled(value);
        }
        if let Some(value) = theme["extension"].as_str() {
            self.as_mut().set_system_theme_extension(QString::from(value));
        }
        if let Some(value) = theme["applyScript"].as_str() {
            self.as_mut().set_system_theme_script(QString::from(value));
        }

        let start = &json["startPage"];
        if let Some(value) = start["enabled"].as_bool() {
            self.as_mut().set_start_page_enabled(value);
        }
        if let Some(value) = start["greeting"].as_str() {
            self.as_mut().set_start_greeting(QString::from(value));
        }
        if let Some(value) = start["columns"].as_i64() {
            self.as_mut().set_start_columns(value.clamp(2, 12) as i32);
        }
        if let Some(value) = start["custom"].as_str() {
            // A path is as good as a URL here; people will write either.
            let resolved = if value.is_empty() || value.contains("://") {
                value.to_owned()
            } else {
                format!("file://{}",
                        crate::system_theme::expand_home(
                            value, std::env::var("HOME").ok().as_deref()))
            };
            self.as_mut().set_start_custom_url(QString::from(&resolved));
        }
        if let Some(value) = start["background"].as_str() {
            self.as_mut().set_start_background(QString::from(value));
        }
        if let Some(value) = start["backgroundIntensity"].as_f64() {
            self.as_mut().set_start_background_intensity(value.clamp(0.0, 2.0));
        }
        if start["shortcuts"].is_array() {
            let encoded = serde_json::to_string(&start["shortcuts"]).unwrap_or_default();
            self.as_mut().set_start_shortcuts_json(QString::from(&encoded));
        }

        let bookmarks = &json["bookmarks"];
        if bookmarks["subscriptions"].is_array() {
            let encoded = serde_json::to_string(&bookmarks["subscriptions"]).unwrap_or_default();
            self.as_mut().set_bookmark_subscriptions_json(QString::from(&encoded));
        }
        if let Some(value) = bookmarks["syncHours"].as_i64() {
            self.as_mut().set_bookmark_sync_hours(value.clamp(0, 24 * 30) as i32);
        }
    }

    fn apply_theme(mut self: Pin<&mut Self>, json: &serde_json::Value) {
        let Some(map) = json.as_object() else {
            return;
        };
        let color = |key: &str| map.get(key).and_then(|v| v.as_str()).and_then(hex);

        macro_rules! apply {
            ($($key:literal => $setter:ident),* $(,)?) => {
                $(if let Some(value) = color($key) {
                    self.as_mut().$setter(value);
                })*
            };
        }

        apply! {
            "background" => set_background,
            "surface" => set_surface,
            "surface_container_lowest" => set_surface_container_lowest,
            "surface_container_low" => set_surface_container_low,
            "surface_container" => set_surface_container,
            "surface_container_high" => set_surface_container_high,
            "surface_container_highest" => set_surface_container_highest,
            "surface_bright" => set_surface_bright,
            "on_background" => set_background_fg,
            "on_surface" => set_surface_fg,
            "on_surface_variant" => set_surface_variant_fg,
            "outline" => set_outline,
            "outline_variant" => set_outline_variant,
            "primary" => set_primary,
            "on_primary" => set_primary_fg,
            "primary_container" => set_primary_container,
            "on_primary_container" => set_primary_container_fg,
            "secondary" => set_secondary,
            "secondary_container" => set_secondary_container,
            "on_secondary_container" => set_secondary_container_fg,
            "tertiary" => set_tertiary,
            "error" => set_error,
            "on_error" => set_error_fg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sets_a_nested_value_in_place() {
        let mut root = json!({ "appearance": { "iconSize": 20, "rounding": { "normal": 17 } } });
        assert!(apply_path(&mut root, "appearance.rounding.normal", json!(24)));
        assert_eq!(root["appearance"]["rounding"]["normal"], 24);
        // Siblings survive.
        assert_eq!(root["appearance"]["iconSize"], 20);
    }

    #[test]
    fn creates_missing_intermediate_objects() {
        let mut root = json!({});
        assert!(apply_path(&mut root, "tabs.closeButton", json!("always")));
        assert_eq!(root["tabs"]["closeButton"], "always");
    }

    #[test]
    fn replaces_a_non_object_on_the_path() {
        // A scalar where an object is needed must not silently drop the write.
        let mut root = json!({ "appearance": 5 });
        assert!(apply_path(&mut root, "appearance.tabBar.mode", json!("top")));
        assert_eq!(root["appearance"]["tabBar"]["mode"], "top");
    }

    #[test]
    fn sets_a_top_level_key() {
        let mut root = json!({});
        assert!(apply_path(&mut root, "blocking", json!({ "enabled": false })));
        assert_eq!(root["blocking"]["enabled"], false);
    }

    #[test]
    fn rejects_an_empty_path() {
        let mut root = json!({});
        assert!(!apply_path(&mut root, "", json!(1)));
    }

    #[test]
    fn parses_hex_colours_and_rejects_junk() {
        assert!(hex("#141311").is_some());
        assert!(hex("141311").is_some());
        assert!(hex("#14131").is_none());
        assert!(hex("#zzzzzz").is_none());
    }
}

#[cfg(test)]
mod ua_tests {
    use super::{accept_language, chrome_ua};

    #[test]
    fn uses_the_engines_own_major_version() {
        // The whole point: what we claim must match what
        // navigator.userAgentData reports, or the two contradict each other.
        assert!(chrome_ua("140.0.7339.207").contains("Chrome/140.0.0.0"));
        assert!(chrome_ua("152.0.1.2").contains("Chrome/152.0.0.0"));
    }

    #[test]
    fn looks_like_a_real_chrome_ua() {
        let ua = chrome_ua("140.0.7339.207");
        assert!(ua.starts_with("Mozilla/5.0 (X11; Linux x86_64)"));
        assert!(ua.contains("AppleWebKit/537.36 (KHTML, like Gecko)"));
        assert!(ua.ends_with("Safari/537.36"));
        // A stray newline or double space from the format string would make
        // the header malformed and stand out immediately.
        assert!(!ua.contains('\n') && !ua.contains("  "));
    }

    #[test]
    fn falls_back_rather_than_emitting_junk() {
        for odd in ["", "not-a-version", ".", "abc.1.2"] {
            let ua = chrome_ua(odd);
            assert!(ua.contains("Chrome/140.0.0.0"), "bad fallback for {odd:?}");
        }
    }
}
