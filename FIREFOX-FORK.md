# Tela on Firefox — fork plan

Decision (2026-08-01): rebuild Tela as a **Firefox fork** — Firefox source +
patches that graft Tela's keyboard-first chrome onto the Gecko frontend. The
Qt WebEngine (Chromium) shell is retired; the *ideas* carry over, the engine
does not — Gecko can't be embedded in the old QML shell, so this is a rebuild on
a new foundation, not a translation.

## Why a fork and not just a customized profile

A profile (userChrome.css + extensions + prefs) covers ~70% of "keyboard-first
Firefox". A **fork** is chosen because the command palette, the tab sidebar, and
deep key handling need frontend changes beyond what userChrome.css/extensions
can reach, and because we want to ship a single branded, self-updating binary.

## Tooling: `surfer` (the Zen/Pulse-Browser build tool)

Do **not** hand-manage mozilla-central + `mach` + a patch queue. Use **surfer**
(Node-based, wraps `mach`) — the same tool Zen Browser and Pulse use to maintain
lean forks:

- `surfer download` — fetch the pinned Firefox source
- `src/` — our patches, applied onto the source on `surfer import`
- `surfer build` — runs `mach build`
- `surfer export` — regenerate `src/` diffs after editing the tree
- branding/version live in `surfer.json`

Node v26 is present, so surfer runs. mozilla-central moved to **git** (2025), so
`hg` is **not** needed — our git is enough.

## The key realization: most of the Rust/Qt code is now unnecessary

Firefox already provides, natively and better, most of what we hand-built:

| Tela (Qt) feature                     | On the fork                                             |
|---------------------------------------|--------------------------------------------------------|
| Native ad-blocker (Brave adblock)     | **uBlock Origin** bundled — real blocking, incl. cosmetic + popups. Our interceptor/tela-block goes away. |
| Watch-tracker userscript              | Ship as a bundled **WebExtension** content script (convert from the `.user.js`) — runs on the real WebExtension API. |
| `GM_*` shim (extapi.rs)               | Gone — real `browser.*`/`GM_*` via a userscript manager or native content script. |
| Google sign-in handoff + Firefox import | **Deleted** — Gecko's TLS signs into Google natively. This whole saga disappears. |
| Cookie-store / profile QML quirks     | Gone — Firefox's profile & cookie store. |
| Single-instance, downloads, history, PiP, find-in-page, reader mode | All **built into Firefox** already. |
| Keyboard-first chrome, command palette, tab sidebar | **The actual work** — frontend patches in `src/browser/` + a bundled helper extension. |

So the fork's real scope is narrow: **the keyboard-first UX layer**. Everything
else is deletion or a bundled add-on.

## Build prerequisites (the current blocker)

Measured on this host 2026-08-01:

- **Disk: need ~40 GB, have ~10 GB** on the roomiest filesystem. Must
  free ~35 GB or attach another disk. Source checkout alone is ~6 GB; the object
  dir is the ~30 GB monster.
- **RAM: need 8–16 GB for the `libxul` link, have ~6 GB free** (+11 GB swap).
  Will be slow and OOM-prone — close other apps (incl. the running Tela) during
  the link, or add swap.
- Toolchain present: git ✓, python3 ✓, clang ✓, rust ✓, cbindgen ✓, node ✓.
  `hg` not needed; `mach` ships with the checkout.

Until disk is freed, the **compile** is blocked — but nothing below is.

## Milestones

- **M0 — capacity**: free ~35 GB / add disk; then `surfer download` the source.
- **M1 — brand & baseline build**: name = Tela, appId, icon; build vanilla to
  prove the toolchain end-to-end.
- **M2 — bundle features (no engine work)**: pin uBlock Origin as a system
  add-on; ship the watch-tracker as a WebExtension; keyboard-first prefs baseline
  (`user.js`) + Tridactyl-class defaults.
- **M3 — the fork proper**: port the command palette, tab sidebar, and keybinds
  as `src/browser/` frontend patches.
- **M4 — polish**: zoom indicator, per-host prefs, reader tweaks (most already
  in FF).

## Surfer gotchas (cost real time — don't rediscover)

- **npm blocks git deps here** (`allow-git: none`). `@zen-browser/surfer` pulls
  `is-apple-silicon` from GitHub, which we don't need on Linux. Fix: add
  `"overrides": { "is-apple-silicon": "1.1.2" }` to `package.json` (the registry
  has 1.1.2) — then `npm install` resolves. It's in the committed `package.json`.
- **`surfer download` needs `version.candidate === version.version`**, or
  `shouldUseCandidate()` returns true and it fetches the *candidate* archive path
  with an `undefined` build → 404. For a finalized release (ESR 140.13.0), set
  both fields to `140.13.0esr` in `surfer.json`. (Surfer's model: candidate path
  during pre-release dev, release path once `version === candidate`.)
- ESR release source is real: `archive.mozilla.org/pub/firefox/releases/
  140.13.0esr/source/firefox-140.13.0esr.source.tar.xz` (644 MB, HTTP 200).
- Fork repo lives in a sibling checkout, `tela-fox` (separate from the Qt
  `tela/`). `engine/` (the FF source) is gitignored.
- **`sharp@0.32.6` (branding image lib) won't build here** (native postinstall
  blocked). Override to `"sharp": "^0.33.5"` (prebuilt `@img/sharp-linux-x64`,
  no build) — in `package.json`. Needed or `surfer import`/`build` crash.
- **`surfer import` needs `src/` + `configs/`** to exist. Copy them from
  `node_modules/@zen-browser/surfer/template/` (configs/, branding.optional/);
  `mkdir src`. Custom UI CSS goes in `src/browser/themes/custom/shared/*.inc.css`.
- **mach hard-fails on Python 3.14** (`'Constant' object has no attribute 's'`).
  System has `python3.11`. Force it: a `pybin/` with `python3`→`python3.11`
  symlinks, prepended to PATH when running `surfer build`. Also delete any stale
  `~/.mozbuild/srcdirs/*/_virtualenvs/mach` created by a 3.14 attempt.
- **Never `pkill -f "surfer build"`** — it matches the shell running it and kills
  your own command (CLAUDE.md's `pkill -x` rule). Kill by PID.

## WORKING (2026-08-01 late) — Tela look achieved

- **Launch**: `run-tela.sh` OR detached: `systemd-run --user --collect --unit=tela-run
  --setenv=MOZ_ENABLE_WAYLAND=1 --setenv=LIBGL_ALWAYS_SOFTWARE=1 <bin> --no-remote
  -profile ~/.tela-fox <url>`. Profile `~/.tela-fox/user.js` holds Tela prefs.
- **The dark-content bug was GPU compositing** — fixed with `LIBGL_ALWAYS_SOFTWARE=1`
  (+ `gfx.webrender.software=true`). Without it the content area renders black.
- **Tela look shipped**: green "P" chip (`#tabbrowser-tabs::before` in shared.inc.css),
  green active-tab pill, rounded floating page-card (`.browserSidebarContainer`
  margin+radius), dark near-black surfaces, green `#3ddc84` accent, narrow rail via
  `sidebar.visibility="expand-on-hover"`.
- **Right rail**: injected a `<vbox id="tela-right-rail">` in
  `browser/base/content/browser-box.inc.xhtml` (after the tabbox), styled in
  shared.inc.css (`.tela-rail-btn`, chrome:// icons). Buttons: find/bookmark/print,
  downloads/settings.
- **Screenshotting tela on this Hyprland desktop is impossible** — window class
  changes post-map so rules don't match, it tiles onto random workspaces, grabs land
  on other windows. USE THE USER'S SCREENSHOTS. systemd-run keeps it alive (plain
  nohup/setsid get reaped when the Bash call ends).
- Still to refine: left-rail bottom tools duplicate the right rail; no visual search
  row under P; right-rail icons/commands are approximate.

## Earlier state (2026-08-01) — builds + runs, two open bugs

- **Vanilla + themed builds succeed.** `tela` binary at
  `engine/obj-x86_64-pc-linux-gnu/dist/bin/tela`. Loose (dev) build → chrome CSS
  is editable in `dist/bin/browser/chrome/.../browser-shared.css`; a `mach build`
  after editing `src/browser/themes/custom/shared/shared.inc.css` re-packages in
  ~10–20 s (fast iteration).
- **Theme wiring works**: `%include ../custom/shared/shared.inc.css` added to
  `browser/themes/shared/browser-shared.css` + `*` (preprocess) on its line in
  `shared/jar.inc.mn`. Tela dark palette + narrow icon rail (via pref
  `sidebar.visibility="expand-on-hover"`, tools stripped via `sidebar.main.tools=""`)
  render correctly.
- **DO NOT put `@namespace` in shared.inc.css** — it's %included mid-file and
  would change the default namespace for Firefox's own following rules.
- **OPEN BUG 1 — content doesn't fill window**: page paints only the top; the
  unpainted remainder is transparent (desktop shows). Not @namespace, not the
  floating-card rules, not fractional scale alone (persists under XWayland too,
  where it's also tiny). Suspect a Firefox-Wayland painting / CSD issue. Needs
  isolation with the theme fully removed + a vanilla-prefs launch, verified by a
  human (screenshotting on this multi-monitor Hyprland setup is unreliable).
- **OPEN BUG 2 — early exit**: tela exits ~15–25 s after launch with sidebar/
  vertical-tabs prefs + a real URL; log shows `NS_ERROR_FILE_TOO_BIG
  [nsIFaviconService.setFaviconForPage]`. `about:blank` + minimal prefs stayed up
  35 s. Needs a debug build + stack trace to confirm (favicon service vs
  vertical-tabs). Disk is NOT the cause (35 GB free).
- **Iteration lesson**: don't screenshot-and-iterate on the live desktop —
  windows re-parent, addresses change, captures grab the wrong app. Get a human's
  eyes, or run tela on its own workspace/monitor deliberately.

## What is NOT blocked by disk (do these now)

1. **Convert the watch-tracker `.user.js` → a Firefox WebExtension** — reusable
   in every path, needs no build. (Keep the `.user.js` too; both share logic.)
2. **Keyboard-first prefs baseline** (`user.js`) + uBlock pin config.
3. **Scaffold the surfer fork repo** (`surfer.json`, branding, `src/` layout,
   README with these prerequisites up top).
