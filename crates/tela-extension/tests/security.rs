//! Adversarial tests for the loader.
//!
//! A browser extension is untrusted code shipped as a zip. These cover the
//! attacks that archive actually enables.

use std::io::Write;
use std::path::PathBuf;

use tela_extension::{Manifest, install_bundle};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("tela-sec-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temp dir");
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Build a zip containing the given (name, contents) entries verbatim, so
/// hostile paths survive into the archive.
fn build_zip(path: &PathBuf, entries: &[(&str, &str)]) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for (name, contents) in entries {
        zip.start_file(*name, options).expect("start file");
        zip.write_all(contents.as_bytes()).expect("write");
    }
    zip.finish().expect("finish zip");
}

const MANIFEST: &str = r#"{"manifest_version":2,"name":"Evil","version":"1"}"#;

#[test]
fn zip_slip_cannot_escape_the_bundle_directory() {
    let work = TempDir::new("slip");
    let archive = work.0.join("evil.xpi");
    let install = work.0.join("installed");
    let canary = work.0.join("PWNED");

    build_zip(
        &archive,
        &[
            ("manifest.json", MANIFEST),
            // Classic zip-slip: climb out of the extraction directory.
            ("../../PWNED", "owned"),
            ("../PWNED", "owned"),
        ],
    );

    let bundle = install_bundle(&archive, &install).expect("installs despite hostile entries");

    assert!(!canary.exists(), "traversal entry escaped the install directory");
    assert!(!install.join("PWNED").exists(), "traversal entry landed one level up");
    assert!(bundle.root.starts_with(&install), "bundle root outside install dir");
    // The legitimate file still made it.
    assert!(bundle.root.join("manifest.json").exists());
}

#[test]
fn absolute_paths_in_the_archive_are_not_honoured() {
    let work = TempDir::new("abs");
    let archive = work.0.join("evil.xpi");
    let install = work.0.join("installed");

    build_zip(&archive, &[("manifest.json", MANIFEST), ("/tmp/tela-abs-canary", "owned")]);

    install_bundle(&archive, &install).expect("installs");
    assert!(
        !PathBuf::from("/tmp/tela-abs-canary").exists(),
        "absolute archive path was written outside the install directory"
    );
}

#[test]
fn a_bundle_without_a_manifest_is_rejected() {
    let work = TempDir::new("nomanifest");
    let archive = work.0.join("bad.xpi");
    let install = work.0.join("installed");

    build_zip(&archive, &[("readme.txt", "nothing here")]);
    assert!(install_bundle(&archive, &install).is_err());
}

#[test]
fn a_non_archive_is_rejected_rather_than_panicking() {
    let work = TempDir::new("garbage");
    let archive = work.0.join("not-a-zip.xpi");
    let install = work.0.join("installed");

    std::fs::write(&archive, b"this is not a zip file at all").unwrap();
    assert!(install_bundle(&archive, &install).is_err());
}

#[test]
fn dangerous_permissions_mark_an_extension_unloadable() {
    // nativeMessaging is arbitrary process execution reachable from extension
    // code; it must never be silently tolerated.
    let manifest = Manifest::parse(
        r#"{"manifest_version":2,"name":"Sneaky","version":"1",
             "permissions":["storage","nativeMessaging"]}"#,
    )
    .unwrap();

    assert!(!manifest.is_loadable());
    assert!(
        manifest
            .compatibility()
            .iter()
            .any(|issue| issue.feature == "nativeMessaging")
    );
}
