# Open work

Ordered roughly by what the user notices. Background in `CLAUDE.md`.

## In progress

### Bookmark import/export with a UI popup
Netscape bookmark HTML — the format Firefox and Chrome both read and write —
reachable from the start page. Nothing written yet beyond this note.

Remember the popup constraint in `CLAUDE.md`: if the dialog contains no web
view, a `Popup` is fine.

### uBlock's content scripts were never injected
**A manifest path that starts with `/`.** uBlock declares its content scripts as
`/js/vapi.js`; `Path::join` with an absolute path discards the base, so the read
was of `/js/vapi.js`, which does not exist. The block loaded no source and was
skipped — silently, because a block that fails to load looks exactly like a
block that matched nothing.

It is the only extension of the thirteen installed that writes them rooted,
which is why uBlock alone filtered nothing while the others worked. `asset_path`
in `extensions.rs` now strips the root and refuses traversal, and a `fixme` on
the `script` channel fires when a block loads no source at all.

Three things had to exist before that mattered, and now do:

- **Ports.** `runtime.connect` was a stub whose `onMessage` discarded its
  listeners. uBlock runs *all* content-script traffic over a port — including
  the request for a page's cosmetic filters — so nothing could ever come back.
  Real bidirectional ports now, over the same envelope transport as
  sendMessage, with connections held if they arrive before the background page
  has registered its handler.
- **Tab identity.** `sender.tab.id` and `sender.frameId` are read
  unconditionally by uBlock's connect handler. Tabs now carry a stable id that
  survives closing and reordering, answered per view through the queue
  transport — the only path that can tell views apart. This is also what the
  `Failed to get tab ID` message from adaptive-tab-bar-colour was about.
- **Lifecycle events.** `retrieveContentScriptParameters` returns nothing
  unless uBlock holds a page store for the tab, and it builds those from
  `webNavigation.onCommitted` and `tabs.onUpdated`. Those were inert stubs;
  `ExtensionEvents.qml` now raises them.

And one more, which was the last link: **`tabs.insertCSS` was a stub that
resolved silently.** That call *is* element hiding — uBlock computes a page's
cosmetic filters and pushes them as a stylesheet through it — so uBlock could
run flawlessly, answer every message, inject its procedural-filter code, and
still hide nothing. It is implemented now, along with `removeCSS`, resolving
`file` against the calling extension's own bundle.

Measured on cineby, with the native blocker off so only uBlock could act. The
probe counts stylesheets containing `display:none`, which is uBlock's
fingerprint regardless of which ads a page happens to carry:

```
before:  styleSheetsWithHiding=0  sizes=[]
after:   styleSheetsWithHiding=4  sizes=[19979,63,48,63]
```

A 20 KB hiding stylesheet, applied to the live page. Element hiding works.

Worth remembering how this was chased, because the same trap is everywhere in
this code: *every* missing piece failed silently. A rooted path, a port with no
listener, an event nobody raised, a CSS call that resolved and did nothing —
none of them logged, and all four looked identical from outside, namely "uBlock
doesn't block". Inference from partial evidence was wrong twice; only the probe
settled it.

`tabs.executeScript` is now real: `file` is resolved against the calling
extension's own bundle (never another's — the id comes from the host), and the
source is run in that tab's isolated world. It was returning `[]`, which made
every caller look like it had run. uBlock loads its procedural-filter code
through it; Violentmonkey injects **every** userscript through it, so nothing
it manages could have run at all. Built, not yet verified against a real
userscript.

### uBlock's popup counts zero
The popup renders correctly; the numbers are the problem — "Blocked on this
page: 0", "Domains connected: 0 out of 1". Those come from uBlock's *own*
`webRequest` observations, and every `webRequest` event in the shim is still an
inert stub, so it is never told a request happened. Tela's engine is blocking on
that page; uBlock has no idea.

Feeding it observation events would make the popup and the logger meaningful,
and its counts would match the native engine's since both read the same lists.
The cost is the reason it is not done yet: the interceptor decides on
Chromium's IO thread, so every request has to be marshalled to the UI thread and
broadcast into a background page — hundreds per page load, over a bridge that
polls at 120 ms. Needs batching, and needs measuring against the sluggishness
just removed.

### uBlock's dashboard rendered black — messages never reached extension pages
It was never crashing. `dashboard.js` opens with:

```js
await new Promise(resolve => { const check = async () => {
    const response = await vAPI.messaging.send('dashboard', { what: 'readyToFilter' });
    if ( response ) { return resolve(true); }
    ...
```

and `dashboard.css` carries `body.notReady { display: none; }`. The page hides
its entire body until its background answers, so a reply that never arrives is
not an error — it is a black page, waiting forever. The stray
`reading 'id' of undefined` was a red herring from elsewhere.

The reply could not arrive, for two compounding reasons in
`TabPage.deliverMessage`. It returned early unless `bridgeActive`, which is only
true when *content scripts* were injected — and a `tela-extension://` page has
none. And it delivered into `WebEngineScript.UserWorld`, while an extension page
gets its shim from the scheme handler as ordinary `<script>` tags, so its
`browser.*` lives in the **main** world.

`deliverInto()` now routes on `extensionPage`: main world for an extension's own
page, isolated world for a content script, nothing for the start page.

Chased far too long by inference. Ruled out along the way, all correct but all
irrelevant: the scheme is registered `SecureScheme | CorsEnabled |
FetchApiAllowed`, MIME types come from `QMimeDatabase`, the generated `_tela`
files were current rather than truncated, and there is no CSP. The thing that
actually found it was reading the extension's own source and noticing what it
was waiting for.

The shim also reports uncaught errors and unhandled rejections **with their
stack** now, on the `page`/`ext` channels. That is what identified Privacy
Badger's missing enum and auto-tab-discard's Proxy target within seconds.

### Extension identity was whatever the caller claimed
`extapi.rs` says the id "is supplied by the host when it creates the channel,
not by the calling script". It was not. `storageGet(a[0], …)` took the id from
the page, and `window.__telaMakeApi(id, grants)` was a global — so a script on
one extension's page could mint another extension's `browser.*`, pass `null`
grants to skip permission gating, and read its storage.

Closed in two layers: the factory refuses any id but the page's own (content
scripts are exempt — several extensions share one isolated world and none set
`__telaExtensionId`), and `answerCall` overwrites the claimed id with the one
in the page's address.

**Still open:** background pages and the action popup reach `ExtensionApi`
*directly* over the shared `extensionChannel`, which QML cannot intercept. Any
script there can build its own `QWebChannel` and call `telaApi.storageGet` for
another extension. Closing it needs a per-extension `ExtensionApi` instance, or
an id bound at channel creation.

Content scripts, separately, are **not** injected into `tela-extension://`
pages — `scripts_for_url` only accepts http/https/file — so one extension's
content script cannot reach another's dashboard.

### A `Connections` cannot be a component root
Extracting the extension routing into `ExtensionRequests.qml` with `Connections`
as the file's root type — `target: root.api`, handlers as `function onX()`,
plus its own `signal openTab(...)` — **silently dropped every extension
message**. No QML error, no warning, clean build, app runs. Outbound traffic
still worked (ports opened, so the logs looked healthy), and only the replies
vanished, which reads exactly like "uBlock stopped filtering".

Measured: 0 routed messages, and cineby's hiding stylesheets went 4 → 0.
Restructured as `Item { … Connections { target: root.api … } }`: 367 messages,
hiding restored at ~20 KB.

Worth remembering because the failure is invisible in every way this project
normally checks — it compiles, it loads, it logs, and it does nothing.

### Opening a tab leaves panels covering it
`openTab` now closes the settings panel, the extensions popup and any open
action popup. It matters most when the tab came from one of them — an
extension's "Preferences" opens its options page, and leaving the panel on top
is the wrong answer to a click that meant "take me there".

### Extension network requests were subject to page CORS
An extension page is an origin, so the web's rules applied to it: every
cross-origin request failed. That is not how a browser works — host permissions
exempt them. It broke Privacy Badger's config, uBlock's list updates, ClearURLs'
rules and Violentmonkey's `@require` fetches, all at once.

`fetch` and `XMLHttpRequest` in extension contexts now route through the host,
where QML's own XHR has no page origin and so no CORS. Verified: 3 blocked
requests → 0, with all three extensions fetching successfully.

Gated, because this is the one privilege the web does not hand out: `may_fetch`
checks the URL against the extension's declared `host_permissions` using the
same match-pattern engine content scripts use, and non-http(s) schemes are
refused outright so nothing reaches `file://` or another extension this way.
Content scripts and web pages never get it — extension contexts only.

### Message senders were empty
The shim passed `{ id: ext }` to `runtime.onMessage` listeners: no `url`, no
`origin`, no `tab`. Privacy Badger branches on exactly those, hit
`sender.url.startsWith(…)` on undefined, and threw *inside its handler* — so it
never replied and its content script saw a dead background page.

The sending side now attaches a real MessageSender. `startsWith` crashes 4 → 0,
`badger is not defined` many → 0. Unanswered messages also log `tela:timeout`
with the extension and the message rather than silently resolving undefined
after five seconds; that is what found this.

### Bookmarks: import, export, and subscriptions
Netscape HTML both ways — the format Firefox and Chrome read. Folders are
flattened (this is a flat store by design), entities decoded, `ADD_DATE`
preserved. **`javascript:` URLs are refused**: importing a file someone sent you
must not import code.

Subscriptions follow a bookmark file by URL — `bookmarks.subscriptions`, synced
6 s after start and every `syncHours`. Merged, never authoritative: a sync only
adds, so nothing you saved yourself is removed by one. FMHY ships as the
default. Measured: 8 → 1875 bookmarks, no duplicates, no bookmarklets.

Still missing: the import/export **UI**. The engine is there and tested; nothing
in the interface reaches it yet.

### Start page: live backgrounds, and an HTML escape hatch
`page/LiveBackground.qml` draws four backdrops from the theme's own palette —
aurora (drifting blurred colour fields), mesh, stars, none — and stops entirely
when the page is not showing. `overlay/BackgroundPicker.qml` lists them, each
previewed by the animation itself rather than a still of it.

`startPage.custom` points at your own HTML file instead. Opt-in, because it
costs the renderer process the built-in page exists to avoid — which is the
whole reason the start page is QML and not HTML.

Still missing: editing the shortcut tiles, and Firefox-style drag-and-drop
customisation of the toolbar (the right bar is already config-driven, including
`divider` and `spacer` items — what is missing is the editor).

### `about:` pages
Requested, not yet scoped. `about:blank` already resolves as a start-page URL;
what is missing is Tela's own — settings, extensions, a filter view.

### Web views come back blank after the window is hidden
Switching workspaces releases each view's surface, and Qt does not reliably
produce a fresh frame on return: the page comes back black or torn, and so does
an open extension popup. `repaint()` on `TabPage` and `ExtensionActionPopup`
drops the view for a single frame to force a new surface, driven from the
window's `onActiveChanged`.

Held as a `surfaceStale` property folded into the `visible` binding rather than
assigned to `visible` directly — assigning would destroy the binding, which is
how the "not clipped, not in a Popup" rules in `CLAUDE.md` got broken twice
before.

### Debugging extensions
`TELA_DEBUG` takes `WINEDEBUG`'s grammar — `+port`, `trace+port,+event`,
`+all,-storage` — over channels `page`, `ext`, `port`, `script`. `err` and
`fixme` print by default, `warn` and `trace` do not. Output is elapsed time, the
gap since the last line on that channel, then class/channel/subject/where, in
fixed columns with colour when stderr is a terminal.

The gap column is the point: it makes a stall obvious instead of something you
work out by subtracting timestamps. Before this existed, a content script that
threw was completely invisible — `TabPage` had no console handler at all.

### Verify every installed extension actually works
Loading is not working. Established so far, by measurement:

| | |
|---|---|
| Content scripts run | verified |
| Page theming (Dark Reader, Midnight Lizard) | verified — a `#ffffff` page rendered `rgb(5,6,6)` |
| Ad/tracker blocking | verified — 25 requests cancelled on cineby |
| Permissions enforced | verified |
| Load without errors | 12 of 15 |

Still failing: **privacy-badger (11 errors)**, auto-tab-discard (1),
proton-vpn (1 — a `'use background'` build artifact, possibly genuinely
incompatible).

Unverified visually, both need a screenshot to confirm:
- Proton VPN's popup fitting its content after the content-sizing change
- uBlock's dashboard rendering after the shim-injection fix

### Intermittent unresponsiveness
Three causes found and fixed: per-navigation script recompilation (4457 ms →
~450 ms), eager session restore (now deferred until a tab is selected), and a
**143 MB `storage.json`** (below).

The shim aliased `storage.managed` to `storage.local`. uBlock reads managed
storage at startup and caches what it finds back into local as
`cachedManagedStorage` — so it cached the whole store inside itself, once per
run, squaring the file each time. Every `storage.set` then rewrote 143 MB of
pretty-printed JSON, and the first `get` of a session parsed it. `managed` is
now a separate read-only area (empty, as a browser with no admin policy
reports) and `session` is in memory rather than on disk. `heal()` in
`extapi.rs` drops the poisoned key on load, so existing profiles shrink on the
next run — the bloated file is parsed once more, then never again.

Unverified: whether this is also why uBlock's own cosmetic filtering never
appeared to work. Every one of its storage calls was going through that file.

What remains: **~24 renderer processes**, one per extension background page.
That is the baseline memory and CPU cost, and it is not tabs — 8 restored tabs
cost fewer renderers than one eager tab did. The honest options are fewer
always-on extensions or lazily starting background pages.

The btrfs condition in `CLAUDE.md` is very likely contributing.

## Next

### uBlock Origin fully functional
The dashboard opens and now gets a working `browser.*`. What is missing is its
own request blocking, which needs a genuinely cancelling `webRequest` — the shim
only observes.

**A cancelling `webRequest` is not reachable here.** Confirmed against the Qt
headers, not from memory:

```cpp
virtual void interceptRequest(QWebEngineUrlRequestInfo &info) = 0;  // void
void block(bool shouldBlock);                                       // inline
```

The interceptor decides during the call, on the IO thread, with nothing to
suspend or resume. Firefox can do this because blocking `webRequest` is wired
into its own network stack, which holds the channel open while an extension in
another process answers; Qt exposes no equivalent and no extension runtime at
all. The only route to a suspendable http(s) path is a local MITM proxy with its
own root CA — invasive, breaks pinning, and slower.

So the shape to aim for is **uBlock decides, Tela enforces**: its
`selectedFilterLists`, user filters and whitelist drive the engine, and its
dashboard stays the control surface. Everything needed is already in its store
(`selectedFilterLists` is there and correct); the raw lists it caches are in
uBO's own LZ4/"UOSC" format, so either decode that or match its list keys
against `availableFilterLists[key].contentURL`.

Cosmetic filtering: `tela-block` now exposes `cosmetic_for` and
`generic_hide_selectors` (hostname rules, plus generic rules looked up from the
classes and ids a page actually uses), with tests covering the engine cache.
**Deliberately not wired up yet** — uBlock's own element hiding may start
working now the storage bug is fixed, and doing it twice is worse than either.

### Property-level `tabs` gating
An extension without the `tabs` permission can still read tab URLs. Namespace
gating is done; per-property gating of `url`/`title`/`favIconUrl` is not. Real
gap, called out rather than left implied.

### Start page customisation
Requested, not yet scoped.

## Verify before trusting

- **Startup benchmark has not run** since the blocking and DPI changes.
  `scripts/bench-startup.sh` polls `hyprctl` and hangs without compositor
  access. Numbers in `PERFORMANCE.md` predate this work.
- **Rendering fixes are visually unconfirmed**: the `clip` revert, the
  `Layout.fillWidth/fillHeight` on the tab delegate, the DPI rounding policy,
  and the popup restructure. Each was verified by clean QML load and matching
  geometry — which is exactly what was true while the window was showing the
  desktop through itself.
