# Tela — roadmap

A keyboard-first browser with a tiling window manager at its core, built on
Qt WebEngine with a Rust core and a QML front end.

The name is a placeholder; renaming touches `Cargo.toml`, the QML module URI in
`build.rs`, and `~/.config/tela`.

## Shipped

- Qt WebEngine embedded via QML, driven from a Rust `main()` (CXX-Qt).
- Config system at `~/.config/tela/`, seeded on first run, hot-reloaded ~1.5 s
  after any edit. Colour tokens use Material You names, so a palette can be
  copied in from any generator; `appearance.followSystemColors` opts into
  tracking the desktop's generated palette instead.
- Four tab-bar modes (`appearance.tabBar.mode`): `sidebar`, `sidebar-compact`
  (icon rail that expands over the page on hover), `sidebar-auto` (hidden,
  slides in from the edge), `top`.
- Shy chrome (`appearance.tabBar.shyChrome`): toolbar collapses until hovered.
- Fullbleed content by default (`appearance.pageInset: 0`); a non-zero inset
  switches to the rounded-card look with corner masking.
- Right action rail (`appearance.rightBar`).
- Command palette (Ctrl+K / Ctrl+L) with a results dropdown over open tabs.
- Content-blocking engine (`tela-block`, Brave's `adblock` crate) — filter-list
  parsing and matching, with tests. Not yet wired to the interceptor.

## Core — the browser is not usable without these

1. **Session persistence.** Tabs, spaces, scroll position survive restart and
   crash. Rust-owned model backed by SQLite. Replaces the QML `ListModel`.
2. **Content blocking, wired up.** `QWebEngineUrlRequestInterceptor` in the C++
   shim calling into `tela-block`. Filter lists auto-updated on a schedule.
3. **History with full-text search** over titles *and* page text (SQLite FTS5).
   "That page about X I read Tuesday" should be one query.
4. **Downloads** and **find-in-page**.
5. **Spaces with real profile isolation** — separate `WebEngineProfile` per
   space, so cookie jars are genuinely separate, not just visually grouped.

## The tiling model — what makes this different

The page area is not "one view, optionally split". It is a tiled workspace, and
tabs are clients. Split view falls out of this as the two-client case.

Borrowed from awesome:

- **Tags, not workspaces.** A tab carries one or more tags; the view shows a set
  of tags. Spaces become a tag preset.
- **Layouts** applied to visible clients: `tile` (master + stack), `tile.left`,
  `max`, `fair`, `spiral`, `floating`. Cycle with a key.
- **Master/stack** with an adjustable master width factor.
- **Dynamic insertion.** A new tab takes its place in the layout; you never
  drag a splitter unless you want to.
- **Keyboard focus movement**, swap, and promote-to-master.

Architecture consequence: the layout engine is pure Rust — a function from
(client list, layout, geometry, master factor) to a set of rectangles. Fully
testable without Qt. QML positions `WebEngineView`s from those rectangles. This
is the single most important design decision in the project, and it should land
before the tab model hardens.

## Customisable chrome

Firefox-style, but better:

- Drag toolbar items between the top bar, the right rail, and an overflow menu.
- **Flexible spacers** as draggable items, so alignment is compositional rather
  than hardcoded.
- Layout persisted to `config.json` as an ordered item list — the current
  `RightBar` model shape is already this, deliberately.
- A customise mode that dims the page and makes chrome slots into drop targets.

## Workflow

- **Keyboard-complete.** Every action reachable without a mouse, including link
  hinting. If a feature needs the mouse, it is not finished.
- **Auto-archive.** Tabs untouched for N days leave the strip and remain in
  history. Tab bankruptcy should not be a thing you declare manually.
- **Peek.** Open a link in a dismissible overlay without losing your place.

## System integration — and its safety model

The rule: **expose structured data, accept an enumerated set of commands, never
execute anything a page or a config file names.**

- **Outbound:** D-Bus interface publishing current tab, tab list, playback
  state. MPRIS so the shell's bar controls browser audio.
- **Inbound:** a fixed verb set — `open <url>`, `search <query>`, `new-tab`,
  `switch-space`. No generic `eval`, no user-supplied handler execution.
- **Files:** XDG desktop portals for open/save. Never raw paths from web
  content.
- **Secrets:** libsecret, the same keyring the desktop already uses. Never a
  plaintext store.

Two things this project will not build, regardless of convenience: a
page-reachable command bridge, and a config file whose values are executed
rather than read. Both turn a browser into a persistence mechanism.

## Extensions — the honest position

Qt WebEngine ships Chromium's content layer, not its extensions subsystem.
There is no `browser.*` API, no XPI/CRX loading. Building a WebExtensions
subset is possible and bounded:

| Capability | Status |
|---|---|
| Content scripts, isolated world | `WebEngineScript` + `ScriptWorldId` — good fit |
| `moz-extension://` pages | `QWebEngineUrlSchemeHandler` — workable |
| `storage`, `tabs`, `runtime` messaging | Implementable |
| `declarativeNetRequest` | Maps onto the interceptor |
| Blocking `webRequest` | Partial — less context than Gecko |
| `webRequest.filterResponseData` | **Impossible.** Firefox-only; no Chromium equivalent |

That last row is why uBlock Origin's Firefox build cannot be fully reproduced on
a Chromium core — it streams and rewrites response bodies. Hence: replace it
natively via `tela-block` rather than porting it. Extension support, when it
comes, is for the long tail — and content scripts run with no host bridge, ever.
