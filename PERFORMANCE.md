# Performance

Speed is a feature here, not an accident, and it is the thing most likely to be
lost quietly. This file records the baseline so regressions are visible.

Measure with `scripts/bench-startup.sh [runs] [binary]`. It times to
**window-mapped** — the moment the compositor shows the window — because that is
what "fast" actually means to the person using it.

## Baseline

Recorded 2026-07-27, Qt 6.11.1, Arch, Hyprland.

| Build | Startup (mean) | RSS at idle | Child processes | Binary |
|---|---|---|---|---|
| debug | 303 ms | 335 MB | 2 | 66 MB |
| release | 270 ms | 329 MB | 2 | 3.6 MB |

For scale: Firefox and Chrome typically take 1–2 s cold and start above 400 MB.

Note how little separates debug from release: ~11% of startup and almost no
memory. Nearly all of both figures is Qt WebEngine initialising its own
machinery, not Tela's code — which is the useful reading. It means our Rust and
QML are close to free today, and equally that the headroom for keeping it fast
lies in *what we ask WebEngine to do* (renderer processes, injected scripts,
background contexts) rather than in optimising our own hot paths.

### After the extension host landed

| Build | Startup (mean) | RSS at idle |
|---|---|---|
| debug, before extension host | 303 ms | 335 MB |
| debug, with extension host | 270 ms | 334 MB |

No regression — the difference is inside run-to-run noise.

## Extension overhead

Run with `cargo bench -p tela-extension`, then execute the printed binary (the
bench is a plain program, not a `#[bench]` harness, so it runs on stable).

This is the cost paid on **every navigation**, so it is the number that decides
whether extensions make the browser feel slow.

Recorded 2026-07-27, release build, 40 installed userscripts:

| Measurement | Cost |
|---|---|
| Parse 40 scripts (startup, once) | 49 µs |
| Wrap 40 scripts (startup, once) | 23 µs |
| **Match 40 scripts against a URL (per navigation)** | **3.9 µs** |
| Same, for a page matching nothing | 4.5 µs |
| Parse one match pattern | 32 ns |

3.9 µs is 0.02% of a 16.7 ms frame. Extensions are currently free at this scale.

**Budget:** per-navigation cost stays far under 1 ms. If it approaches a frame,
injection moves off the hot path — that is the trigger to act, not a vague sense
that things feel slower.

### Costs the extension host adds

Now that background contexts and the API bridge exist, two costs are real and
neither is covered by the bench above:

- **A background context is a renderer process.** One per enabled extension that
  declares background scripts. This is the single largest cost of enabling an
  extension, and it is why hidden contexts are created only for extensions that
  actually declare one.
- **The content-script API bridge polls at 120 ms.** Qt only injects its
  WebChannel transport into the view owning the channel, so a page view's
  isolated world has none; `runJavaScript` is used as the transport instead and
  the queue has to be drained on a timer. The timer runs **only on pages where
  an extension content script actually matched** — ordinary browsing pays
  nothing. Replacing it with a real transport would remove the poll entirely,
  and is the reason to keep chasing the Qt side.

Measured after the host landed: startup 266 ms, unchanged within noise.

### Measured cost of installed extensions

2026-07-27, debug build, two tabs open on real sites:

| Installed extensions | Total RSS |
|---|---|
| 0 | 610 MB |
| 14 | 1007 MB |

**≈28 MB per extension that declares a background context**, because each one
is a renderer process. Fourteen of them also pushed V8 into a zone OOM.

This is not a Tela overhead — Firefox and Chrome pay the same per-extension
price. But it means the honest guidance is: install the extensions you use, not
the ones you might. The extensions popup shows enabled count against total for
exactly this reason, and disabling one frees its process immediately.

Worth building later: hibernate a background context when its extension has
been idle, the same way tab hibernation works.

## Why it is currently fast

Worth writing down, because each of these is easy to give away:

- **The chrome is QML, not HTML.** Sidebar, palette, settings and start page are
  scene-graph items. No renderer process, no DOM, no CSS engine for any of the
  UI. Browsers built on HTML chrome pay a full web stack before showing a pixel.
- **The start page loads no web content.** A new tab keeps its `WebEngineView`
  at an empty URL, so no renderer process spawns until you actually navigate.
- **Config is scalar properties, not a document.** Theme values are `QColor` and
  `int` properties on one QObject; QML binds directly to them. No JSON parsing
  per frame, no string-keyed lookups in hot paths.
- **Blocking is native.** The filter engine is compiled Rust in-process, not an
  extension running in a JavaScript context per tab.

## What will threaten it

In rough order of risk:

1. **The extension host.** Every loaded extension adds a background context (a
   hidden page with its own renderer), content-script injection on every
   navigation, and shim marshalling on every API call. This is the single
   biggest threat to the numbers above. Mitigations: keep background contexts
   lazy, evaluate match patterns in Rust before injecting anything, and never
   inject the shim into pages that have no content script.
2. **Per-tab renderer processes.** Real tabs cost real processes. Tab
   hibernation and auto-archive are performance features, not tidiness ones.
3. **Effects on live web content.** Any `MultiEffect`/`layer.enabled` over a
   `WebEngineView` costs a full render pass per frame. The rounded corners are
   drawn with a `Shape` overlay specifically to avoid this.
4. **Filter list size.** Loading many large lists raises memory and startup.
   Measure before adding lists by default.

## Rules

- Run the benchmark before and after anything touching startup, the tab model,
  or the extension host. Record the delta in the PR.
- A change that costs more than 10% of startup needs a reason written down.
- Memory at idle with one tab should stay under 400 MB.
