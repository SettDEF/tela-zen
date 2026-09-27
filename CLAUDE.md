# Tela — working notes for Claude

A keyboard-first browser: Rust core, QML front end, Qt WebEngine underneath.
`ROADMAP.md`, `SECURITY.md` and `PERFORMANCE.md` hold the design intent — read
those for *why*. This file is the operational knowledge that is easy to lose
between sessions and expensive to rediscover.

Open work lives in `TASKS.md`, not here.

## Layout

```
crates/tela-app/          the application
  src/*.rs                cxx-qt bridges; each file in build.rs is a bridge
  cpp/*.cpp               Qt/Chromium glue reached from Rust
  qml/Main.qml            window, tabs, the web views
crates/tela-block/        filter engine wrapper (Brave's adblock crate)
crates/tela-extension/    manifest parsing, bundle install
```

Build: `cargo build`. Test: `cargo test`. Binary lands in
`$CARGO_TARGET_DIR/debug/tela` (or `target/debug/tela`).

## Things that will bite you

**A `WebEngineView` cannot render inside a QML `Popup`, and cannot be clipped.**
Both put it somewhere it has no compositor surface: the view comes up blank and
punches a hole through the window to the desktop behind it. `Popup` renders into
the overlay layer; `clip: true` forces a render target. This cost two separate
regressions. `ExtensionActionPopup.qml` is therefore a plain `Item` parented to
`window.contentItem`, with a hand-written scrim, escape handling and
transitions standing in for what `Popup` gave for free. Other popups
(`ExtensionsPopup`, `ContextMenu`, `Settings`) are still `Popup`s — fine only
because they contain no web view.

**Generated files go stale.** `write_runtime_files` regenerates `_tela/shim.js`,
`_tela/prelude.js` and `_tela_background.html` into every bundle on *every*
load, not just at install. Before that, an install made prior to a shim change
kept running the old code forever — looking current, behaving old. If a shim
change appears to do nothing, check the generated file on disk first.

**Extensions declare `background: { page }` as often as `{ scripts }`.** uBlock
uses the former. Code gated on `scripts.is_empty()` silently skips exactly the
extensions people care about.

**QML `format!` argument alignment.** Several injection sites build HTML with
positional `{}`. Adding a placeholder without adding its argument shifts every
following value silently — it compiles and produces nonsense.

**A `WebEngineProfile` must be declared in QML, or it has no cookies.** A
profile built in C++ — `new`, parented to `qApp`, via `QQmlComponent`, with or
without the engine's root context — reports itself correctly (`otr=0`,
`ForcePersistentCookies`, valid storage path) and persists History and
Favicons, but **never creates a cookie store**. No error, no warning. Every
restart then signs you out of everything. `Main.qml` declares it as
`browserProfile`; `tela::adopt_profile` takes it up by objectName after
`engine.load()`, which is safe because `load()` builds objects synchronously
while URL fetching waits for the event loop — so the scheme handler and
interceptor are installed before any request. All three view types
(`TabPage`, `BackgroundHost`, `ExtensionActionPopup`) must bind to it: a view
on another profile silently loses both the extension scheme and the blocker.

**`qWarning` output is swallowed in this build.** It prints nothing, which
reads exactly like code that never ran — it cost a wrong conclusion once
already. Use `fprintf(stderr, ...)` when checking whether C++ executed.

**Verify visually.** Two rendering bugs measured as "fine" numerically
(geometry matched, no QML errors) while being obviously broken on screen. For
anything visual, ask for a screenshot rather than trusting instrumentation.

## Extension model

- The shim (`extapi.rs: BROWSER_SHIM`) provides `browser.*`. It is written into
  each bundle's `_tela/` and injected into background pages, content scripts,
  and any `.html` the scheme handler serves.
- `_tela/prelude.js` carries per-extension globals: id, manifest, locale
  messages, and granted permissions.
- Permissions are enforced by deleting ungranted namespaces from the API object.
  Ungranted means *absent*, not a throwing stub — extensions feature-detect.
  `tabs`/`windows`/`commands` are deliberately **not** gated: `tabs` gates
  sensitive properties rather than the namespace, and `commands` comes from a
  manifest key. Gating them broke SponsorBlock and Midnight Lizard.
- `document_start` scripts are wrapped to wait for `document.head`. Chromium's
  DocumentCreation fires earlier than WebExtensions' `document_start`, where
  `head` is guaranteed; without the wrapper every theming extension throws on
  `appendChild`.
- The script collection is memoised by its JSON. Reassigning it recompiles every
  script — ~1.7 MB — and doing that per navigation cost about four seconds a
  page.
- No extension code runs on credential origins (`accounts.google.com`,
  `github.com`, …), matched by hostname, never substring.

## Blocking

Native, in the request interceptor — not an extension. uBlock's own blocking has
never worked here and cannot: the shim's `webRequest` only observes. Filter lists
live in `~/.config/tela/filters/*.txt`, seeded once from an installed uBlock.
The built engine is cached as `.engine.bin` (startup 1.5s → 0.33s) and
invalidated by list mtime. `TELA_LOG_BLOCKS=1` prints every cancelled request.

A network blocker cannot stop popup ads — `window.open` is not a request. That
is what `behavior.allowPopups` (default off) is for.

## Identity

The engine is Chromium, so the UA claims Chrome, built from
`qWebEngineChromiumVersion()` rather than hardcoded. Claiming Firefox
contradicts `navigator.vendor`, `window.chrome` and `navigator.userAgentData`,
which is why sign-in pages rejected it. Only the major version is used — real
Chrome freezes the rest at `.0.0.0`.

## Environment

- `QT_SCALE_FACTOR_ROUNDING_POLICY=PassThrough` is set before the
  `QGuiApplication` exists. The display scale here is fractional (1.667); Qt
  rounds it while Chromium does not, and pages come up oversized until something
  forces a relayout.
- **The host filesystem has no unallocated btrfs space** (`btrfs filesystem
  usage /` → `Device unallocated: 1.00MiB`). `df` shows free space, but writes
  fail with ENOSPC anyway. This has already truncated 12 shim files to zero
  bytes — producing a convincing but entirely false picture of broken
  extensions — and OOM-killed a build. Suspect it before suspecting the code.
  The fix is `sudo btrfs balance start -dusage=50 /`, which is the user's to run.

## Testing habits

- `pkill -f <pattern>` matches the shell running it. Use `pkill -x tela`.
- Session restore changes what loads between runs; set
  `behavior.restoreSession: false` before any comparative measurement.
- Filter EasyList tests against real ad domains. Synthetic paths like
  `127.0.0.1/ads/banner.gif` match nothing, which reads as "blocking is broken".
