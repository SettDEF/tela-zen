//! Extension-overhead measurement.
//!
//! This is the cost paid on **every navigation**, so it is the number that
//! decides whether extensions make the browser feel slow. Run with:
//!
//! ```sh
//! cargo run --release -p tela-extension --bench matching
//! ```
//!
//! Deliberately a plain binary rather than a `#[bench]` harness so it runs on
//! stable and prints numbers a human can read.

use std::time::Instant;

use tela_extension::{UserScript, matcher::Pattern, userscript};

/// Roughly what a heavy userscript user has installed.
const SCRIPT_COUNT: usize = 40;

/// URLs a navigation-heavy session would hit.
const URLS: &[&str] = &[
    "https://news.ycombinator.com/item?id=123456",
    "https://github.com/rust-lang/rust/pull/1",
    "https://www.reddit.com/r/rust/comments/abc/title/",
    "https://doc.qt.io/qt-6/qtwebengine-index.html",
    "https://example.com/",
    "https://mail.google.com/mail/u/0/#inbox",
    "file:///home/user/notes.html",
    "https://deeply.nested.subdomain.example.com/a/b/c?q=1",
];

fn build_script(index: usize) -> UserScript {
    let host = match index % 4 {
        0 => "*://*.example.com/*",
        1 => "*://github.com/*",
        2 => "*://*.reddit.com/*",
        _ => "*://news.ycombinator.com/*",
    };
    let source = format!(
        "// ==UserScript==\n\
         // @name        Script {index}\n\
         // @namespace   bench\n\
         // @match       {host}\n\
         // @exclude     *://admin.example.com/*\n\
         // @grant       GM_addStyle\n\
         // ==/UserScript==\n\
         document.body.dataset.script{index} = '1';\n"
    );
    UserScript::parse(&source).expect("bench script should parse")
}

fn main() {
    println!("extension overhead — the per-navigation cost\n");

    // ── Load-time work, paid once at startup ──────────────────────────
    let start = Instant::now();
    let scripts: Vec<UserScript> = (0..SCRIPT_COUNT).map(build_script).collect();
    let parse_time = start.elapsed();

    let start = Instant::now();
    let wrapped: Vec<String> = scripts.iter().map(userscript::wrap).collect();
    let wrap_time = start.elapsed();

    let payload: usize = wrapped.iter().map(String::len) .sum();

    println!("load ({SCRIPT_COUNT} scripts, once at startup)");
    println!("  parse:  {:>8.2?}", parse_time);
    println!("  wrap:   {:>8.2?}", wrap_time);
    println!("  source: {:>6} KB wrapped\n", payload / 1024);

    // ── Per-navigation work ───────────────────────────────────────────
    const ROUNDS: usize = 10_000;
    let start = Instant::now();
    let mut matched = 0usize;
    for round in 0..ROUNDS {
        let url = URLS[round % URLS.len()];
        for script in &scripts {
            if script.matches_url(url) {
                matched += 1;
            }
        }
    }
    let elapsed = start.elapsed();
    let per_navigation = elapsed / ROUNDS as u32;

    println!("match ({SCRIPT_COUNT} scripts against one URL)");
    println!("  per navigation: {:>8.2?}", per_navigation);
    println!("  matches found:  {}", matched / ROUNDS);
    println!(
        "  budget:         {:.4}% of a 16.7 ms frame\n",
        per_navigation.as_secs_f64() * 1000.0 / 16.7 * 100.0
    );

    // ── The common case: a page nothing matches ───────────────────────
    let start = Instant::now();
    for _ in 0..ROUNDS {
        for script in &scripts {
            let _ = script.matches_url("https://unmatched.invalid/page");
        }
    }
    let miss = start.elapsed() / ROUNDS as u32;
    println!("miss (page matching no script — the common case)");
    println!("  per navigation: {:>8.2?}\n", miss);

    // ── Pattern parsing, for completeness ─────────────────────────────
    let start = Instant::now();
    for _ in 0..ROUNDS {
        let _ = Pattern::parse_match("*://*.example.com/path/*");
    }
    println!("parse one match pattern: {:?}", start.elapsed() / ROUNDS as u32);

    println!("\nRule of thumb: per-navigation cost should stay far under 1 ms.");
    println!("If it approaches a frame, injection has to move off the hot path.");
}
