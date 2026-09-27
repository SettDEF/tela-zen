mod blocking;
mod cookies;
mod file_picker;
mod firefox_import;
mod history;
mod bookmarks;
mod config;
mod debug;
mod extapi;
mod extensions;
mod session;
mod shell;
mod single_instance;
mod system_theme;
mod webengine;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    // Order matters: Qt WebEngine sets up its shared GPU context here, and it has
    // to happen before any QGuiApplication exists.
    // Both of these must precede the QGuiApplication: WebEngine sets up its
    // shared GPU context, and Chromium locks its URL scheme registry.
    // A fractional display scale (1.6667 here) is rounded by Qt's default
    // policy while Chromium renders at the exact value. The two disagree, and
    // the page comes up oversized until something forces a relayout — which is
    // the "sometimes it renders huge while loading" symptom. PassThrough makes
    // both use the same fractional factor.
    //
    // Set rather than overridden: an explicit choice in the environment wins.
    if std::env::var_os("QT_SCALE_FACTOR_ROUNDING_POLICY").is_none() {
        // Safety: single-threaded here — nothing else has started yet, and this
        // must precede QGuiApplication reading it.
        unsafe { std::env::set_var("QT_SCALE_FACTOR_ROUNDING_POLICY", "PassThrough") };
    }

    // Let embedded cross-site sign-in flows reach their cookies. "Sign in with
    // Google" loads accounts.google.com in an iframe on a third-party site and
    // needs the Google session cookie there; Chromium's third-party storage
    // partitioning and cookie phase-out hide it, so gsi/transform hangs on a
    // blank page. Disabling those features restores the classic third-party
    // cookie behaviour these auth flows still depend on.
    //
    // Set rather than overridden: explicit flags in the environment win.
    if std::env::var_os("QTWEBENGINE_CHROMIUM_FLAGS").is_none() {
        // Safety: single-threaded here, before QtWebEngineQuick::initialize.
        unsafe {
            std::env::set_var(
                "QTWEBENGINE_CHROMIUM_FLAGS",
                // Third-party storage/cookies for embedded sign-in (GSI), plus
                // hardware-accelerated video decode. Chromium leaves VA-API off
                // by default on Linux, so video decodes on the CPU — the biggest
                // avoidable battery drain on a laptop. Enabling VaapiVideoDecoder
                // and ignoring the GPU blocklist offloads H.264/VP9/AV1 to the
                // GPU's fixed-function decoder. Harmless where VA-API is absent:
                // Chromium falls back to software.
                "--disable-features=ThirdPartyStoragePartitioning,TrackingProtection3pcd \
                 --enable-features=VaapiVideoDecoder,VaapiVideoDecodeLinuxGL,AcceleratedVideoDecodeLinuxGL \
                 --ignore-gpu-blocklist",
            );
        }
    }

    // Route Qt's file dialogs — including WebEngine's `<input type=file>` picker
    // — through the XDG desktop portal, so a page upload opens the desktop's own
    // native file chooser instead of Qt's built-in one. The portal and Qt's
    // platform-theme plugin are what make this native; without the theme, Qt
    // falls back to its Quick dialog. Also the roadmap's "files via portal".
    //
    // Set rather than overridden: an explicit theme in the environment wins.
    if std::env::var_os("QT_QPA_PLATFORMTHEME").is_none() {
        // Safety: as above — single-threaded, before QGuiApplication reads it.
        unsafe { std::env::set_var("QT_QPA_PLATFORMTHEME", "xdgdesktopportal") };
    }

    // If a Tela for this profile is already running, hand it our URL to open as
    // a tab and stop here — no second window. Must be the very first thing, so a
    // handed-off launch does nothing expensive before exiting.
    if single_instance::try_handoff() {
        return;
    }

    shell::register_scheme();
    webengine::ffi::init_web_engine();

    // Config must exist on disk before the first frame binds to it.
    config::ensure_default_files();

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/Tela/qml/Main.qml"));
    }

    // The profile is declared in QML, so it exists only once the engine has
    // loaded. Everything below attaches to it, and all of it runs before the
    // event loop starts — the objects exist by now, but nothing has been
    // fetched yet.
    if let Some(engine) = engine.as_mut() {
        if !webengine::ffi::adopt_profile(engine) {
            eprintln!("tela: no browser profile found — extensions and blocking are inactive");
        }
    }

    // Has to follow adoption: the store belongs to the profile. Starts the
    // mirror the site panel matches against when clearing a site's cookies.
    cookies::track();

    // Needs a live application, so it cannot go with the calls above.
    if let Some(dir) = config::config_dir() {
        let root = dir.join("extensions");
        let _ = std::fs::create_dir_all(&root);
        shell::install_scheme_handler(&root.to_string_lossy());
    }

    // Must follow application construction, and must target the same profile
    // the scheme handler was installed on.
    {
        // Built before the interceptor is installed, so no request can reach a
        // half-initialised engine.
        let lists = blocking::init();
        if lists > 0 {
            // "Loaded", not "active": whether it enforces depends on
            // `blocking.enabled`, which the config applies a moment later.
            // Reporting this as active made a disabled blocker look like a
            // working one.
            eprintln!("tela: filter engine loaded ({lists} lists)");
        } else {
            eprintln!("tela: no filter lists found — blocking is inactive");
        }
        // Client hints must reach the interceptor before it is installed, so
        // the very first request already carries a current-Chrome identity.
        // Only when the UA is claiming Chrome — otherwise the hints would
        // contradict a Firefox or default UA.
        let agent = config::configured_user_agent();
        if agent.contains("Chrome/") {
            let (brands, full_list, full_version) = config::client_hint_headers();
            shell::set_client_hints(&brands, &full_list, &full_version);
            // Google's sign-in host rejects embedded Chromium however current
            // its identity; present as Firefox there instead. Only meaningful
            // when the rest of the browser is claiming Chrome.
            shell::set_signin_user_agent(config::signin_user_agent());
        }
        shell::install_request_interceptor();

        shell::set_accept_language(&config::accept_language());

        if !agent.is_empty() {
            shell::set_user_agent(&agent);
        }
    }


    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
