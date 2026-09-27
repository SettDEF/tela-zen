//! Inspect an .xpi/.crx without installing it.
//!
//! Answers the only question that matters before shipping an extension host:
//! what would actually work?
//!
//! Usage: cargo run -p tela-extension --example inspect -- <file>...

use std::path::PathBuf;

use tela_extension::{Manifest, Severity, install_bundle};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: inspect <file.xpi>...");
        std::process::exit(2);
    }

    let scratch = std::env::temp_dir().join(format!("tela-inspect-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&scratch);

    for arg in &args {
        let path = PathBuf::from(arg);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

        match install_bundle(&path, &scratch) {
            Ok(bundle) => report(&bundle.manifest, &name),
            Err(error) => println!("\n{name}\n  FAILED TO LOAD: {error}"),
        }
    }

    let _ = std::fs::remove_dir_all(&scratch);
}

fn report(manifest: &Manifest, file: &str) {
    println!("\n─── {} {} ─────────────────────", manifest.name, manifest.version);
    println!("  file:      {file}");
    println!("  manifest:  v{}", manifest.manifest_version);

    let content_scripts = manifest.content_scripts.len();
    let background = if manifest.background.service_worker.is_some() {
        "service worker (MV3)"
    } else if !manifest.background.scripts.is_empty() {
        "background scripts (MV2)"
    } else if manifest.background.page.is_some() {
        "background page"
    } else {
        "none"
    };

    println!("  content scripts: {content_scripts}");
    println!("  background:      {background}");
    println!("  popup:           {}", manifest.action.as_ref()
        .and_then(|a| a.default_popup.as_deref()).unwrap_or("none"));
    println!("  permissions:     {}", if manifest.permissions.is_empty() {
        "none".to_owned()
    } else {
        manifest.permissions.join(", ")
    });

    let issues = manifest.compatibility();
    if issues.is_empty() {
        println!("  compatibility:   no declared blockers");
    } else {
        println!("  compatibility:");
        for issue in &issues {
            let tag = if issue.severity == Severity::Blocking { "BLOCKS" } else { "degraded" };
            println!("    [{tag}] {}", issue.feature);
        }
    }

    // Which browser.* namespaces the host actually implements. Structure alone
    // says nothing: an extension can load fine and still fail on the first call
    // to an API that does not exist.
    const IMPLEMENTED: &[&str] = &[
        "storage", "tabs", "runtime", "i18n", "alarms", "idle",
        "notifications", "contextMenus", "menus", "scripting",
    ];
    // Several manifest permissions are declarations rather than callable
    // namespaces — they grant a capability or relax a quota and have nothing to
    // implement. Counting them as missing overstates the gap.
    const DECLARATION_ONLY: &[&str] = &[
        "unlimitedStorage", "clipboardWrite", "clipboardRead",
        "webRequestBlocking", "activeTab", "theme", "privacy",
    ];
    let missing: Vec<&str> = manifest
        .permissions
        .iter()
        .map(String::as_str)
        .filter(|p| !IMPLEMENTED.contains(p) && !DECLARATION_ONLY.contains(p))
        .collect();

    if !missing.is_empty() {
        println!("  unimplemented:   {}", missing.join(", "));
    }

    // What today's host could actually deliver.
    let verdict = if !manifest.is_loadable() {
        "will not load"
    } else if !missing.is_empty() {
        "loads and runs, but will fail on the unimplemented APIs above"
    } else if content_scripts > 0 && background != "none" {
        "content scripts and background both run"
    } else if content_scripts > 0 {
        "content scripts run"
    } else if background != "none" {
        "background runs"
    } else {
        "nothing to run"
    };
    println!("  → today:         {verdict}");
}
