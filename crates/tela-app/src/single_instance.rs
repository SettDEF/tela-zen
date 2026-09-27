//! One window, not one per click.
//!
//! Tela is a normal program: launch it again — which is exactly what the
//! desktop does for every external link once Tela is the default browser — and
//! you get a second process and a second window. This makes later launches hand
//! their URL to the first instance instead, so an external link opens as a
//! **tab** in the window you already have.
//!
//! The transport is a Unix socket keyed by profile (so `--profile work` stays a
//! separate window on purpose). A later launch connects, writes its URL, and
//! exits; the first instance listens and raises an `open_url` signal QML routes
//! to `openTab`. An empty URL means "just come to the front".

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use cxx_qt::Threading;
use cxx_qt_lib::QString;

/// The per-profile socket. Under `$XDG_RUNTIME_DIR` (tmpfs, cleaned on logout);
/// falls back to a temp dir keyed by uid.
fn socket_path() -> PathBuf {
    let profile = crate::config::profile_name();
    let name = if profile.is_empty() {
        "tela.sock".to_owned()
    } else {
        format!("tela-{profile}.sock")
    };
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            // The user name keeps it from colliding with another user on a
            // shared temp dir when XDG_RUNTIME_DIR is unset (rare on Linux).
            let user = std::env::var("USER").unwrap_or_else(|_| "user".to_owned());
            std::env::temp_dir().join(format!("tela-{user}"))
        });
    let _ = std::fs::create_dir_all(&base);
    base.join(name)
}

/// If another instance of this profile is already running, hand it this launch's
/// URL and return `true` — the caller should then exit without opening a window.
/// Returns `false` when we are the first instance (nobody was listening).
pub fn try_handoff() -> bool {
    let path = socket_path();
    match UnixStream::connect(&path) {
        Ok(mut stream) => {
            let url = crate::config::launch_url_from_args();
            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
            // One line: the URL (possibly empty), newline-terminated.
            let _ = stream.write_all(url.as_bytes());
            let _ = stream.write_all(b"\n");
            let _ = stream.flush();
            true
        }
        // No listener, or a stale socket file from a crash: we are primary.
        Err(_) => false,
    }
}

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
        type SingleInstance = super::SingleInstanceRust;

        /// A later launch asked us to open this URL. Empty = just raise the
        /// window. Delivered on the Qt thread, safe to touch UI from.
        #[qsignal]
        fn open_url(self: Pin<&mut SingleInstance>, url: QString);

        /// Begin accepting hand-offs from later launches. Call once from QML,
        /// after the window exists. A no-op if the socket cannot be claimed
        /// (another primary won a startup race).
        #[qinvokable]
        fn start(self: Pin<&mut SingleInstance>);
    }

    impl cxx_qt::Threading for SingleInstance {}
}

#[derive(Default)]
pub struct SingleInstanceRust;

impl ffi::SingleInstance {
    fn start(self: Pin<&mut Self>) {
        let path = socket_path();
        // A leftover socket from a crash refuses bind with EADDRINUSE even
        // though nobody is listening; try_handoff already proved that, so
        // clearing it here is safe.
        let _ = std::fs::remove_file(&path);
        let listener = match UnixListener::bind(&path) {
            Ok(listener) => listener,
            Err(_) => return,
        };

        let thread = self.qt_thread();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(stream) = conn else {
                    continue;
                };
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let url = line.trim().to_owned();
                // Emit on the Qt thread — the listener runs on its own.
                let _ = thread.queue(move |qobject: Pin<&mut ffi::SingleInstance>| {
                    qobject.open_url(QString::from(&url));
                });
            }
        });
    }
}
