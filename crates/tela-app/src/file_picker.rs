//! The desktop's own file chooser for a page's `<input type=file>`.
//!
//! Qt WebEngine draws its own file picker and ignores the platform theme, and
//! its QtQuick.Dialogs fallback is no more native. To get the *system* file
//! manager's dialog we call it directly — `zenity` (GTK), falling back to
//! `kdialog` (KDE) — and hand the chosen paths back to the page.
//!
//! Run off the Qt thread so selecting a file does not freeze the browser; the
//! result is delivered back on the Qt thread through the `picked` signal.

use std::pin::Pin;
use std::process::Command;

use cxx_qt::Threading;
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
        type FilePicker = super::FilePickerRust;

        /// The chosen path(s), newline-separated. Empty means cancelled (or no
        /// chooser was available). Delivered on the Qt thread.
        #[qsignal]
        fn picked(self: Pin<&mut FilePicker>, paths: QString);

        /// Open the system chooser. `mode` is "open", "multiple" or "save";
        /// `dir` is a starting directory (may be empty). Returns immediately;
        /// the answer arrives via `picked`.
        #[qinvokable]
        fn open(self: Pin<&mut FilePicker>, mode: &QString, title: &QString, dir: &QString);
    }

    impl cxx_qt::Threading for FilePicker {}
}

#[derive(Default)]
pub struct FilePickerRust;

/// Build the zenity invocation for a mode.
fn zenity(mode: &str, title: &str, dir: &str) -> Command {
    let mut cmd = Command::new("zenity");
    cmd.arg("--file-selection");
    cmd.arg(format!("--title={title}"));
    // Newline is the one separator a path cannot contain, so multi-select round
    // trips unambiguously.
    cmd.arg("--separator=\n");
    match mode {
        "multiple" => {
            cmd.arg("--multiple");
        }
        "save" => {
            cmd.arg("--save");
            cmd.arg("--confirm-overwrite");
        }
        _ => {}
    }
    if !dir.is_empty() {
        // zenity seeds the folder from --filename ending in a slash.
        let seed = if dir.ends_with('/') { dir.to_owned() } else { format!("{dir}/") };
        cmd.arg(format!("--filename={seed}"));
    }
    cmd
}

/// The KDE equivalent, used when zenity is not installed.
fn kdialog(mode: &str, title: &str, dir: &str) -> Command {
    let mut cmd = Command::new("kdialog");
    cmd.arg(format!("--title={title}"));
    match mode {
        "multiple" => {
            cmd.arg("--multiple");
            cmd.arg("--separate-output");
            cmd.arg("--getopenfilename");
        }
        "save" => {
            cmd.arg("--getsavefilename");
        }
        _ => {
            cmd.arg("--getopenfilename");
        }
    }
    cmd.arg(if dir.is_empty() { "." } else { dir });
    cmd
}

impl ffi::FilePicker {
    fn open(self: Pin<&mut Self>, mode: &QString, title: &QString, dir: &QString) {
        let mode = mode.to_string();
        let title = title.to_string();
        let dir = dir.to_string();
        let thread = self.qt_thread();

        std::thread::spawn(move || {
            // zenity first, kdialog as a fallback. A non-zero exit (cancel) or a
            // missing binary both yield no paths.
            let output = zenity(&mode, &title, &dir)
                .output()
                .or_else(|_| kdialog(&mode, &title, &dir).output());

            let paths = match output {
                Ok(out) if out.status.success() => {
                    String::from_utf8_lossy(&out.stdout).trim().to_owned()
                }
                _ => String::new(),
            };

            let _ = thread.queue(move |qobject: Pin<&mut ffi::FilePicker>| {
                qobject.picked(QString::from(&paths));
            });
        });
    }
}
