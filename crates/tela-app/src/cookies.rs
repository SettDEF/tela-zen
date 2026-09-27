//! Cookie removal, exposed to the site-information panel.
//!
//! Deliberately narrow: count and delete, nothing that reads a value. A panel
//! that offers "forget this site" needs to know whether there is anything to
//! forget and then forget it, and neither answer involves the contents.

use std::pin::Pin;

use cxx_qt_lib::QString;

/// Begin mirroring the profile's cookie store. Call after the profile exists.
pub fn track() {
    ffi::cookies_track();
}

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    // Namespaced per-block, not per-bridge, so cxx does not resolve QString as
    // tela::QString too. Same reason as the shell bridge.
    #[namespace = "tela"]
    unsafe extern "C++" {
        include!("tela_cookies.h");

        fn cookies_track();
        fn cookies_count_for_host(host: &QString) -> i32;
        fn cookies_delete_for_host(host: &QString) -> i32;
        fn cookies_delete_all() -> i32;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type Cookies = super::CookiesRust;

        /// How many cookies this site can read, its parent domains and
        /// subdomains included.
        #[qinvokable]
        fn count_for_host(self: Pin<&mut Cookies>, host: &QString) -> i32;

        /// Forget this site's cookies. Returns how many were removed.
        #[qinvokable]
        fn delete_for_host(self: Pin<&mut Cookies>, host: &QString) -> i32;

        /// Forget every cookie in this profile — the sign-out-of-everything
        /// button, so it is worth confirming before calling.
        #[qinvokable]
        fn delete_all(self: Pin<&mut Cookies>) -> i32;
    }
}

#[derive(Default)]
pub struct CookiesRust;

impl ffi::Cookies {
    fn count_for_host(self: Pin<&mut Self>, host: &QString) -> i32 {
        ffi::cookies_count_for_host(host)
    }

    fn delete_for_host(self: Pin<&mut Self>, host: &QString) -> i32 {
        ffi::cookies_delete_for_host(host)
    }

    fn delete_all(self: Pin<&mut Self>) -> i32 {
        ffi::cookies_delete_all()
    }
}
