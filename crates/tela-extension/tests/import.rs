//! End-to-end import: a `.user.js` on disk becomes a parsed, injectable script.

use std::path::PathBuf;

use tela_extension::{UserScript, discover_userscripts, import_userscripts, userscript};

/// A scratch directory that cleans itself up.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        // Process id keeps concurrent test binaries from colliding.
        let path = std::env::temp_dir().join(format!("tela-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temp dir");
        TempDir(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const SCRIPT: &str = r#"
// ==UserScript==
// @name         Import Me
// @namespace    tela.test
// @version      0.3
// @match        *://*.example.com/*
// @run-at       document-end
// @grant        GM_addStyle
// ==/UserScript==

GM_addStyle("body { outline: 1px solid red; }");
"#;

#[test]
fn imports_userscripts_and_ignores_everything_else() {
    let downloads = TempDir::new("downloads");
    let installed = TempDir::new("installed");

    std::fs::write(downloads.join("thing.user.js"), SCRIPT).unwrap();
    // Neither of these is a userscript and neither should be imported.
    std::fs::write(downloads.join("notes.txt"), "hello").unwrap();
    std::fs::write(downloads.join("library.js"), "export const x = 1;").unwrap();
    // A .user.js with no metadata block is not a userscript either.
    std::fs::write(downloads.join("bare.user.js"), "console.log(1);").unwrap();

    assert_eq!(discover_userscripts(&downloads.0).len(), 2, "finds both .user.js files");

    let imported = import_userscripts(&downloads.0, &installed.0);
    assert_eq!(imported.len(), 1, "only the one with a metadata block parses");

    let script = &imported[0];
    assert_eq!(script.name, "Import Me");
    assert_eq!(script.version.as_deref(), Some("0.3"));
    assert!(script.matches_url("https://www.example.com/page"));
    assert!(!script.matches_url("https://other.com/"));

    // The file landed in the install directory, so a later scan finds it.
    let installed_file = installed.join("thing.user.js");
    assert!(installed_file.exists(), "script copied into place");

    let round_tripped = UserScript::parse(&std::fs::read_to_string(installed_file).unwrap())
        .expect("re-parses from disk");
    assert_eq!(round_tripped.id(), "tela.test/Import Me");
}

#[test]
fn wrapped_output_is_ready_to_inject() {
    let script = UserScript::parse(SCRIPT).unwrap();
    let wrapped = userscript::wrap(&script);

    // The granted API is bound, the body is present, and a throwing script
    // cannot take down the others sharing the page.
    assert!(wrapped.contains("GM_addStyle"));
    assert!(wrapped.contains("outline: 1px solid red"));
    assert!(wrapped.contains("catch (error)"));
    assert_eq!(script.run_at.injection_point(), "DocumentReady");
}

#[test]
fn importing_from_a_missing_directory_is_not_an_error() {
    let installed = TempDir::new("installed-missing");
    let imported = import_userscripts(&PathBuf::from("/nonexistent/path"), &installed.0);
    assert!(imported.is_empty());
}
