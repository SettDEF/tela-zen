# Tela

A keyboard-first browser. Rust core, QML front end, Qt WebEngine underneath.

Not a Firefox or Chromium fork and not a wrapper around one: the chrome is
written from scratch in QML, the logic lives in Rust, and Qt WebEngine is used
only as the page renderer. That means the whole browser outside the page area is
ordinary application code you can read in an afternoon.

**Status: early.** It browses, blocks ads, runs real WebExtensions and restores
your session — it is what I use daily — but it is one person's project at
version 0.1, and it will have rough edges a browser you trust with your banking
should not have. `TASKS.md` is the honest list of what is half-built.

## What works

- **Tabs and spaces**, with four chrome layouts (`sidebar`, `sidebar-compact`,
  `sidebar-auto`, `top`) and optional shy chrome that collapses until hovered.
- **Session persistence** — tabs and spaces survive a restart.
- **Content blocking** built on Brave's `adblock` engine, wired into a
  `QWebEngineUrlRequestInterceptor`, with filter-list parsing and matching.
- **WebExtensions**, enough of the API that uBlock Origin runs: content script
  injection, ports, `tabs.insertCSS`/`removeCSS`, `webNavigation` events, action
  popups, background pages. Userscripts too, including `@require`.
- **History** and **bookmarks**, with Netscape-HTML import.
- **Downloads** and **find in page**.
- **Command palette** on Ctrl+K / Ctrl+L.
- **Import from Firefox or Chrome** — bookmarks, and Firefox cookies so a
  signed-in session carries over. Firefox stores cookies as plain SQLite, so
  this is a read of your own profile on your own machine; nothing is sent
  anywhere. It only runs when you ask for it.
- **Theming** from `~/.config/tela/`, hot-reloaded about 1.5 s after an edit.
  Colour tokens use Material You names, so a palette from any generator drops
  in; `appearance.followSystemColors` tracks the desktop's palette instead.

## What does not work yet

See `TASKS.md`. The short version: the tiling page model described in
`ROADMAP.md` is not built, per-space profile isolation is visual rather than
real, and the extension API is a subset — an extension that reaches for
something unimplemented fails rather than degrading.

Two capabilities are refused on purpose and will not appear: `nativeMessaging`
and `GM_xmlhttpRequest`. `SECURITY.md` says why.

## Install

### One line

```sh
curl -fsSL https://raw.githubusercontent.com/SettDEF/tela-zen/main/install.sh | bash
```

It installs the dependencies, clones to `~/.local/share/tela-zen`, builds the
package and installs it. Re-run it to update. `--yes` takes every default,
`--run` launches afterwards, `--dir PATH` builds somewhere else; pass them
through the pipe with `bash -s -- --yes`.

### Arch, by hand

```sh
git clone https://github.com/SettDEF/tela-zen
cd tela-zen/packaging
makepkg -si
```

The PKGBUILD builds from the checkout it sits in, so `makepkg -si` after any
edit installs exactly what you have.

### From source

Runtime: `qt6-base`, `qt6-declarative`, `qt6-webengine`, `qt6-webchannel`,
`ttf-material-symbols-variable`.
Build: `rust`, `cargo`, `clang`.

```sh
cargo build --release --bin tela
./target/release/tela
```

`cargo test --workspace` runs the suite. `scripts/bench-startup.sh` measures
startup to the point the compositor maps the window, which is what "feels fast"
actually means.

## Layout

```
crates/tela-app/     the application
  src/*.rs           cxx-qt bridges, one per file listed in build.rs
  cpp/*.cpp          Qt and Chromium glue reached from Rust
  qml/               window, chrome, overlays, page views
crates/tela-block/   content-blocking engine wrapper
crates/tela-extension/  manifest parsing, bundle install, userscripts
```

`CLAUDE.md` holds the operational notes — the things that cost a regression to
learn, like why a `WebEngineView` cannot live inside a QML `Popup`.
`PERFORMANCE.md` and `SECURITY.md` hold the design intent.

## Licence

MIT or Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.

Tela links Qt 6 (LGPL-3.0) dynamically against your system libraries, and uses
Brave's `adblock` crate (MPL-2.0) unmodified as a dependency. Both licences are
satisfied by an ordinary source build; neither covers Tela's own code.

The name is a placeholder — see the note at the top of `ROADMAP.md`.
