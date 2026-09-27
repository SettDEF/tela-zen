//! Desktop-facing operations, exposed to QML as a single small object.
//!
//! Everything the browser can do *to* the system lives behind this one type, so
//! the surface stays enumerable. Adding a method here is granting a capability.

use cxx_qt_lib::QString;

/// Register the extension URL scheme. Chromium locks its scheme registry during
/// initialisation, so this has to happen before anything else touches Qt.
pub fn register_scheme() {
    ffi::register_extension_scheme();
}

/// Apply a User-Agent override to the default profile.
pub fn set_user_agent(agent: &str) {
    ffi::set_user_agent(&QString::from(agent));
}

/// The Chromium version actually running, so a spoofed UA can agree with what
/// `navigator.userAgentData` reports.
pub fn chromium_version() -> String {
    ffi::chromium_version().to_string()
}

/// Put the content blocker on the request path.
pub fn install_request_interceptor() {
    ffi::install_request_interceptor();
}

/// Hand the interceptor the client-hint headers to stamp, so they name a
/// current Chrome. Call before `install_request_interceptor`.
pub fn set_client_hints(brands: &str, full_version_list: &str, full_version: &str) {
    ffi::set_client_hints(
        &QString::from(brands),
        &QString::from(full_version_list),
        &QString::from(full_version),
    );
}

/// Present this Firefox UA on Google's sign-in host, which blocks embedded
/// Chromium. Call before `install_request_interceptor`.
pub fn set_signin_user_agent(firefox_ua: &str) {
    ffi::set_signin_user_agent(&QString::from(firefox_ua));
}

/// Apply an Accept-Language header to the default profile.
pub fn set_accept_language(languages: &str) {
    ffi::set_accept_language(&QString::from(languages));
}

/// Point the scheme handler at the directory holding unpacked bundles.
pub fn install_scheme_handler(root: &str) {
    ffi::install_extension_handler(&QString::from(root));
}

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    // Namespaced per-block, not per-bridge: a bridge-wide namespace would make
    // cxx resolve QString as tela::QString too.
    #[namespace = "tela"]
    unsafe extern "C++" {
        include!("tela_shell.h");
        include!("tela_scheme.h");
        include!("tela_intercept.h");

        fn set_clipboard_text(text: &QString);

        /// Must run before the QGuiApplication exists.
        fn register_extension_scheme();

        /// Requires a live application; installs on the default profile.
        fn install_extension_handler(root: &QString);

        fn set_user_agent(agent: &QString);

        fn chromium_version() -> QString;

        fn set_accept_language(languages: &QString);

        fn set_client_hints(
            brands: &QString,
            full_version_list: &QString,
            full_version: &QString,
        );

        fn set_signin_user_agent(firefox_ua: &QString);

        fn install_request_interceptor();
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type Shell = super::ShellRust;

        #[qinvokable]
        fn copy_text(self: &Shell, text: &QString);
    }
}

#[derive(Default)]
pub struct ShellRust;

impl ffi::Shell {
    fn copy_text(&self, text: &QString) {
        ffi::set_clipboard_text(text);
    }
}
