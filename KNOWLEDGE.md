# Knowledge

Things learned the hard way, kept because they cost hours and would cost them
again. `CLAUDE.md` holds operational notes for working in this repo; this file
holds the underlying facts — about Qt, Chromium, WebExtensions, and about how to
investigate them — that stay true beyond any one change.

Each entry says what is true, how it fails, and how to check it. The failures
here share one shape: **they are silent**. Nothing throws, nothing logs, the
code looks right, and the feature simply does less than it should.

---

## Qt WebEngine

### A profile must be declared in QML or it has no cookies

A `QQuickWebEngineProfile` built in C++ — by `new`, parented to `qApp`, via
`QQmlComponent`, with or without the engine's root context — reports itself
perfectly: `isOffTheRecord() == false`, `ForcePersistentCookies`, a valid
`persistentStoragePath`. It writes `History` and `Favicons` to that path. It
**never creates a cookie store**. No error, no warning.

Only a profile written into the loaded QML document gets one.

*Check:* after visiting a site, look for a `Cookies` file under the profile's
storage path. Absent means memory-only, and every restart signs the user out.

*Corollary:* C++ can still own the profile — declare it in QML with an
`objectName`, then adopt it by name after `engine.load()`. That is safe because
`load()` builds objects synchronously while URL fetching waits for the event
loop, so handlers installed between `load()` and `exec()` are in place before
any request goes out.

### Every view must share one profile

Scheme handlers and request interceptors attach *per profile*. A view on a
different profile silently loses the custom scheme and the interceptor — pages
fail to resolve and requests stop being filtered, with no diagnostic. Whenever a
profile is introduced, audit every `WebEngineView` in the tree.

### The default profile is off-the-record and cannot be changed

Off-the-record is fixed at construction. `defaultProfile()` is off-the-record,
so nothing that must persist can live on it. Persistence requires a *named*
profile.

### A `WebEngineView` cannot be inside a `Popup`, and cannot be clipped

Both put it where it has no compositor surface: it renders blank and punches a
hole through the window to the desktop. `Popup` draws into the overlay layer;
`clip: true` forces a render target. Text and ordinary items are unaffected —
clipping a `TextField` is fine and necessary to stop long text escaping.

### Wayland releases the surface when the window is hidden

Switching workspaces is enough. Qt does not reliably produce a fresh frame on
return, so views come back black or torn. Dropping the view for a single frame
forces a new surface. Hold that as a property inside the `visible` binding —
assigning to `visible` destroys the binding.

### `qWarning` output can be swallowed

In this build it prints nothing, which is indistinguishable from code that never
ran — it produced a confidently wrong conclusion once. Use
`fprintf(stderr, …)` when the question is "did this execute".

---

## QML

### A `Connections` cannot be a component root

Using `Connections` as a file's root type, with `target:` bound and handlers as
`function onX()`, silently connects nothing. It compiles, loads, logs no error,
and drops every signal. Wrap it: `Item { Connections { target: … } }`.

*How it presents:* outbound traffic works, replies vanish. Logs look healthy
because the outbound half is what gets logged.

### `children` does not notify

A binding like `children[currentIndex]` is evaluated once and never again when a
`Repeater` fills the list. Bind through something that does notify — a
`Repeater`'s `count`, or `itemAt()` guarded by it. The stale value is usually
the `Repeater` itself, so the symptom is a property reading `undefined`.

### Required properties fail at creation, not at compile time

A delegate whose required property is unsatisfied is not created at all. If a
model gains a role that a delegate requires, every append site must supply it.

---

## WebExtensions

### Blocking `webRequest` is impossible on Qt WebEngine

`QWebEngineUrlRequestInterceptor::interceptRequest` returns `void` and decides
during the call, on the IO thread. There is nothing to suspend or resume, so an
extension in another process cannot answer in time. Firefox can because blocking
`webRequest` is wired into its own network stack.

Consequence: an ad blocker's *network* blocking must be native. Its **cosmetic**
filtering, which needs no network hook, can be genuine.

### Extension network requests are not subject to page CORS

In a real browser, host permissions exempt them. An embedded engine treats an
extension page as an ordinary origin, so every cross-origin fetch fails — filter
lists, config, script updates. Route them through the host, where no page origin
is involved, and gate on the extension's declared `host_permissions`.

### A stub that resolves is worse than a missing function

Extensions feature-detect. A missing API is handled; an API that returns `null`
or `[]` and does nothing reports success while doing nothing. Nearly every
extension bug in this project was a stub of that shape: `tabs.create`,
`executeScript`, `insertCSS`, `tabs.update`. If a call cannot be implemented,
prefer absent over fake.

### Option enums are read, not called

`hasOwn(chrome.webRequest.OnResponseStartedOptions, 'EXTRA_HEADERS')` throws if
the enum object is missing, rather than answering false. One absent enum object
prevented Privacy Badger from constructing, and every one of its later errors
was the resulting cascade.

### `runtime.onMessage` senders must be real

Extensions branch on `sender.url` and `sender.origin` to tell their own pages
from a content script. An empty `{ id }` makes them throw *inside the listener*,
so no reply is sent and the caller concludes the background page is dead.

### Manifest paths may be rooted

`/js/vapi.js` is as common as `js/vapi.js`. `Path::join` with an absolute path
discards the base, the read fails, and the whole content-script block is
skipped — silently, because a block that fails to load looks exactly like one
that matched nothing. Strip the leading separator; refuse traversal.

### An extension's own pages are a different world

An extension page served over a custom scheme gets its shim as ordinary
`<script>` tags, so its `browser.*` lives in the **main** world. A content script
gets it injected into an **isolated** world. Delivery must pick the right one,
and gating delivery on "has content scripts" locks extension pages out entirely.

### Extension identity must come from the host

If an API call names the extension it speaks for, and that name comes from the
caller, any extension page can mint another extension's API — with no
permissions — and read its storage. Derive identity from the context: the page's
address, or the channel it arrived on.

### Aliasing storage areas corrupts data

`storage.managed` aliased to `storage.local` made uBlock cache the entire store
inside itself once per run, squaring its size. It reached 143 MB, which every
subsequent write rewrote in full. It presented as "the browser is randomly
slow", not as a storage bug. Keep areas distinct: `managed` is read-only and
empty without an admin policy; `session` is memory.

---

## Investigating

### Measure the thing, not a proxy for it

Repeated wrong conclusions in this project came from inference over measurement:
"the profile is configured correctly, therefore cookies persist" (they did not);
"I cropped the window rectangle, therefore this is a hole" (the crop included
the compositor's border). Check the artifact itself — the file on disk, the
pixel inside the surface, the value read back.

### Verify the instrument before trusting the result

Before concluding "the cookie did not persist", confirm the probe actually wrote
one. A test that silently does nothing produces the same output as the bug it is
looking for.

### Bisect from the working end

Removing suspects from a broken system means testing a system with many unknown
variables. Starting from a minimal case that *works* and adding pieces isolates
the cause far faster. A ~30-line standalone Qt app settled in one run what a
dozen edits inside the browser had not.

### Distinguish "not configured" from "not supported"

Getting a stock, minimal version of the same setup working proves the platform
can do it, and turns "maybe it is impossible" into "something here breaks it".

### Watch the test harness itself

Several dead ends were the harness, not the code: session restore *appends* to
default tabs rather than replacing them; the app rewrites its session file while
running; `is_restorable` deliberately drops custom-scheme URLs; launch arguments
accept only http/https/file; overriding `XDG_DATA_HOME` without `XDG_CACHE_HOME`
splits a profile across two roots. When a test shows nothing, first ask whether
it ran.

### Silence is not evidence

A component that logs nothing may be absent, filtered, or simply quiet. Make it
say something before drawing a conclusion from its silence.
