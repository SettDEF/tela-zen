import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtWebEngine
import Tela

pragma ComponentBehavior: Bound

// One tab: the start page, the web view, and everything that has to happen
// around a page load — content-script injection, the message bridge that
// reaches an isolated world, and the popout hand-off.
//
// Owns its row of the tab model and writes back to it, which is why the model
// comes in rather than being reached for. Everything the *window* has to decide
// — where a new tab goes, when to save the session, which menu to raise —
// leaves as a signal.
Item {
    id: root

    // ── The row this page is ───────────────────────────────────────────
    required property int index
    required property url pageUrl
    // What extensions in this page call this tab.
    required property int tabId
    // Restored tabs start deferred so a restore does not spawn a renderer per
    // tab. Declared, because it was only ever a model role: `root.deferred`
    // read as undefined, so the view was never actually held back and the
    // clearing code in Main could never fire.
    required property bool deferred
    // Was live, then its renderer was released to reclaim memory. Orthogonal to
    // `deferred` (never-spawned): a suspended view keeps its url and reloads
    // from the navigation entry the moment its lifecycleState goes Active again.
    required property bool suspended
    // Whether the user has muted this tab. Bound onto the view below, so a mute
    // set from the tab strip takes effect here.
    required property bool muted
    required property var tabs

    // ── Collaborators ──────────────────────────────────────────────────
    required property var theme
    required property var extensions
    required property var api
    required property var events
    required property var popout

    // The address that means "show the start page rather than load anything".
    required property url startUrl
    required property var pageProfile

    signal newTabRequested(string url)
    signal contextMenuRequested(var page, var request)
    signal paletteRequested()
    signal customiseRequested()
    /// A video asking for the whole window. `on` is false when it gives it back.
    signal fullScreenRequested(bool on)
    // Something worth persisting changed; something worth re-publishing to
    // extensions changed. Kept separate because they are not the same cost.
    signal sessionDirty()
    signal tabsDirty()
    // A page finished loading — recorded in history by the window.
    signal visited(string url, string title)

    // A tab showing the start page has no web view loaded at all, which is why
    // this is asked so often below: it decides whether `view` even exists yet.
    function isStartUrl(target) {
        const u = String(target);
        return u === String(root.startUrl) || u === "about:blank" || u === "";
    }

    // StackLayout sizes its children, but only ones that ask. Without these
    // the delegate keeps whatever size it was first given, so widening the
    // window leaves the page at its old width and the rest of it unpainted.
    Layout.fillWidth: true
    Layout.fillHeight: true

    readonly property bool startPage:
        root.theme.startPageEnabled && root.isStartUrl(root.pageUrl)

    // Your own HTML start page instead of the built-in one. Opt-in, because it
    // costs the renderer process the built-in page exists to avoid.
    readonly property bool customStart:
        root.startPage && String(root.theme.config.startCustomUrl).length > 0

    // Exposed so the toolbar and palette can drive
    // whichever surface this tab is currently showing.
    readonly property var webView: startPage ? null : view
    readonly property url url: startPage ? root.startUrl : view.url
    readonly property bool loading: startPage ? false : view.loading
    readonly property int loadProgress: startPage ? 100 : view.loadProgress
    // Where this tab has been, kept ourselves rather than read from the engine.
    //
    // Two reasons Qt's own history cannot do this job: its list model exposes
    // no row accessor to JavaScript, so QML cannot read it at all, and it is
    // per-renderer, so it dies with the process and never survives a restart.
    // A restored tab therefore had exactly one entry and nothing to go back to,
    // which is why Back was dead after every restart.
    property var trail: []
    property int trailIndex: -1
    // Set while we are the ones navigating, so a trail-driven move is not
    // mistaken for the user following a link and appended as a new entry.
    property bool followingTrail: false

    // Scroll offset, captured before a suspend and before session save, so the
    // page comes back where it was left — whether the renderer was reclaimed or
    // the whole browser restarted. Restored on the next successful load.
    property real scrollX: 0
    property real scrollY: 0
    property bool pendingScrollRestore: false
    // True through the reload that resuming a suspended tab triggers, so the
    // placeholder can cover the blank frame until content paints.
    property bool resuming: false

    // Whether the page is making noise now. Drives the tab-strip audio icon and
    // exempts the tab from suspension.
    readonly property bool audible: !root.startPage && view.recentlyAudible

    // Reader mode: the page's article, re-rendered clean. On because we rewrote
    // the live DOM, so leaving it is a plain reload.
    property bool readerActive: false

    // The extractor, run in the page's own world. Finds the densest article
    // container, strips chrome and attributes, and rebuilds the document as a
    // single centred column with the theme's colours (%TOKENS% filled in at call
    // time). Pure DOM work — no bridge back to the browser, per the extension
    // model's rule. Returns true when it found something to show.
    readonly property string readerScript:
        "(function(){try{"
        + "function tl(el){var p=el.querySelectorAll('p'),n=0;for(var i=0;i<p.length;i++)n+=p[i].textContent.length;return n;}"
        + "var sel=['article','main','[role=\"main\"]','.post-content','.article-body','.entry-content','.article','.post','#content','.content'];"
        + "var best=null,bs=200;"
        + "for(var s=0;s<sel.length;s++){var e=document.querySelectorAll(sel[s]);for(var i=0;i<e.length;i++){var sc=tl(e[i]);if(sc>bs){bs=sc;best=e[i];}}}"
        + "if(!best){var d=document.querySelectorAll('div');for(var i=0;i<d.length;i++){var sc=tl(d[i]);if(sc>bs){bs=sc;best=d[i];}}}"
        + "if(!best)return false;"
        + "var h=document.querySelector('h1');var title=h?h.textContent.trim():(document.title||'');"
        + "var c=best.cloneNode(true);"
        + "var junk=c.querySelectorAll('script,style,noscript,nav,aside,footer,header,form,iframe,button,svg,[role=\"navigation\"],[class*=\"share\"],[class*=\"comment\"],[class*=\"related\"],[class*=\"promo\"],[class*=\"newsletter\"],[aria-hidden=\"true\"]');"
        + "for(var i=0;i<junk.length;i++)junk[i].remove();"
        + "var all=c.querySelectorAll('*'),keep=['href','src','alt'];"
        + "for(var i=0;i<all.length;i++){var el=all[i];for(var a=el.attributes.length-1;a>=0;a--){if(keep.indexOf(el.attributes[a].name)<0)el.removeAttribute(el.attributes[a].name);}}"
        + "var content=c.innerHTML;"
        + "var ls=document.querySelectorAll('link[rel=\"stylesheet\"],style');for(var i=0;i<ls.length;i++)ls[i].remove();"
        + "var esc=title.replace(/&/g,'&amp;').replace(/</g,'&lt;');"
        + "document.body.setAttribute('style','');"
        + "document.body.innerHTML='<div id=\"tela-reader\"><h1>'+esc+'</h1>'+content+'</div>';"
        + "var st=document.createElement('style');"
        + "st.textContent='html,body{background:%BG% !important;margin:0;padding:0;}"
        + "#tela-reader{max-width:720px;margin:0 auto;padding:64px 24px 140px;color:%TEXT%;"
        + "font-family:Georgia,\"Times New Roman\",serif;font-size:19px;line-height:1.7;}"
        + "#tela-reader h1{font-size:33px;line-height:1.25;margin:0 0 28px;font-weight:700;}"
        + "#tela-reader h2,#tela-reader h3{line-height:1.3;margin:38px 0 14px;}"
        + "#tela-reader p{margin:0 0 22px;}"
        + "#tela-reader img{max-width:100%;height:auto;border-radius:8px;margin:8px 0;}"
        + "#tela-reader a{color:%ACCENT%;}"
        + "#tela-reader blockquote{border-left:3px solid %ACCENT%;margin:0 0 22px;padding:2px 0 2px 20px;opacity:.85;}"
        + "#tela-reader pre,#tela-reader code{font-family:ui-monospace,monospace;font-size:15px;background:%CODE%;border-radius:6px;}"
        + "#tela-reader pre{padding:14px;overflow:auto;}';"
        + "document.head.appendChild(st);window.scrollTo(0,0);return true;"
        + "}catch(e){return false;}})();"

    function toggleReader() {
        if (root.startPage)
            return;
        if (root.readerActive) {
            view.reload();
            root.readerActive = false;
            return;
        }
        const js = root.readerScript
            .replace(/%BG%/g, String(root.theme.bg))
            .replace(/%TEXT%/g, String(root.theme.text))
            .replace(/%ACCENT%/g, String(root.theme.accent))
            .replace(/%CODE%/g, String(root.theme.surfaceHigh));
        view.runJavaScript(js, function (ok) {
            if (ok === true)
                root.readerActive = true;
        });
    }

    readonly property bool canGoBack: startPage ? false : root.trailIndex > 0
    readonly property bool canGoForward:
        startPage ? false : root.trailIndex >= 0
                            && root.trailIndex < root.trail.length - 1

    // Remember a committed navigation. Anything that moves within the trail
    // clears the flag instead of recording, so going back three times does not
    // bury the forward entries it is meant to keep.
    function recordVisit(target) {
        if (root.followingTrail) {
            root.followingTrail = false;
            return;
        }
        const address = String(target);
        if (address.length === 0 || address === "about:blank")
            return;
        if (root.trailIndex >= 0 && root.trail[root.trailIndex] === address)
            return;
        // A new destination discards whatever Forward used to lead to.
        const next = root.trail.slice(0, root.trailIndex + 1);
        next.push(address);
        root.trail = next;
        root.trailIndex = next.length - 1;
        root.persistTrail();
    }

    function persistTrail() {
        if (root.startPage)
            return;
        root.tabs.setProperty(root.index, "historyJson", JSON.stringify(root.trail));
        root.tabs.setProperty(root.index, "historyIndex", root.trailIndex);
    }

    // Seeded from the session so a restored tab keeps the trail it had.
    Component.onCompleted: {
        const entry = root.tabs.get(root.index);
        if (!entry)
            return;
        try {
            const saved = JSON.parse(entry.historyJson || "[]");
            if (saved.length > 0) {
                root.trail = saved;
                root.trailIndex = Math.max(0, Math.min(entry.historyIndex, saved.length - 1));
            }
        } catch (error) {
            // A malformed trail is not worth losing the tab over.
        }
        // A restored tab loads once, deferred; restore its scroll on that load.
        root.scrollX = entry.scrollX || 0;
        root.scrollY = entry.scrollY || 0;
        if (root.scrollX !== 0 || root.scrollY !== 0)
            root.pendingScrollRestore = true;
    }

    // Wayland hands the view's surface back when the window is hidden, and a
    // workspace switch is enough to do it. Qt does not reliably produce a fresh
    // frame on return, so the page comes back unpainted — black, or torn from
    // whatever was last composited.
    //
    // Dropping the view for one frame forces a new surface and a repaint. Held
    // as a property inside the `visible` binding rather than assigned to
    // `visible` directly, so the binding survives.
    property bool surfaceStale: false

    function repaint() {
        if (root.startPage || root.surfaceStale)
            return;
        root.surfaceStale = true;
        Qt.callLater(function () { root.surfaceStale = false; });
    }

    // Run host-supplied source in this page's isolated world — the same world
    // content scripts get, so injected code sees its own extension globals
    // rather than the page's. This is what `tabs.executeScript` becomes.
    function runInPage(source) {
        if (root.startPage)
            return;
        view.runJavaScript(source, WebEngineScript.UserWorld);
    }

    // Add or drop a stylesheet in this page — `tabs.insertCSS`/`removeCSS`.
    //
    // Kept in the isolated world's own registry keyed by the stylesheet text,
    // so the same sheet is never applied twice and removal can find what it is
    // meant to remove.
    function applyCss(css, remove) {
        if (root.startPage)
            return;
        view.runJavaScript(
            "(function(){"
            + "  var store = window.__telaCss || (window.__telaCss = new Map());"
            + "  var css = " + JSON.stringify(css) + ";"
            + "  if (" + (remove ? "true" : "false") + ") {"
            + "    var held = store.get(css);"
            + "    if (held) { held.remove(); store.delete(css); }"
            + "    return;"
            + "  }"
            + "  if (store.has(css)) return;"
            + "  var el = document.createElement('style');"
            + "  el.textContent = css;"
            + "  (document.head || document.documentElement).appendChild(el);"
            + "  store.set(css, el);"
            + "})()",
            WebEngineScript.UserWorld);
    }

    // The engine's own history is used whenever it has the entry — it restores
    // scroll position and form state, which re-navigating cannot. The trail is
    // the fallback for entries from before this renderer existed.
    function goBack() {
        if (!root.canGoBack)
            return;
        root.followingTrail = true;
        root.trailIndex -= 1;
        if (view.canGoBack)
            view.goBack();
        else
            root.navigate(root.trail[root.trailIndex]);
        root.persistTrail();
    }

    function goForward() {
        if (!root.canGoForward)
            return;
        root.followingTrail = true;
        root.trailIndex += 1;
        if (view.canGoForward)
            view.goForward();
        else
            root.navigate(root.trail[root.trailIndex]);
        root.persistTrail();
    }
    // A restored tab is deferred: its view holds no URL until first shown, so
    // view.reload() has nothing to reload and Reload appears to do nothing.
    // Clearing the deferral lets the url binding load it, which is what the
    // user meant by "reload" on a tab that has never rendered.
    function wakeIfDeferred() {
        if (root.suspended) {
            root.resume();
            return true;
        }
        if (!root.deferred && String(view.url).length > 0)
            return false;
        root.deferred = false;
        root.tabs.setProperty(root.index, "deferred", false);
        return true;
    }

    // Release this tab's renderer process. Captures scroll while the renderer is
    // still alive to answer, then discards — Qt keeps the title/url/favicon and
    // navigation entry, and reloads from it when resumed. Only ever called on an
    // off-screen tab (see the sweep in Main.qml); discarding a visible view is
    // refused by Qt and would blank the window.
    function suspend() {
        if (root.startPage || root.deferred || root.suspended || root.poppedOut)
            return;
        if (view.lifecycleState === WebEngineView.LifecycleState.Discarded)
            return;
        view.runJavaScript("[window.scrollX, window.scrollY]", function (pos) {
            if (pos) {
                root.scrollX = pos[0];
                root.scrollY = pos[1];
                root.persistScroll();
            }
            root.suspended = true;
            root.tabs.setProperty(root.index, "suspended", true);
            view.lifecycleState = WebEngineView.LifecycleState.Discarded;
        });
    }

    // Wake a suspended tab: setting the state Active makes Qt reload from the
    // navigation entry; scroll is restored when that load succeeds.
    function resume() {
        if (!root.suspended)
            return;
        root.suspended = false;
        root.resuming = true;
        root.pendingScrollRestore = true;
        root.tabs.setProperty(root.index, "suspended", false);
        view.lifecycleState = WebEngineView.LifecycleState.Active;
    }

    function persistScroll() {
        if (root.startPage)
            return;
        root.tabs.setProperty(root.index, "scrollX", root.scrollX);
        root.tabs.setProperty(root.index, "scrollY", root.scrollY);
        root.sessionDirty();
    }

    function reload() {
        if (root.startPage)
            return;
        if (root.wakeIfDeferred())
            return;
        view.reload();
    }
    // Refetches everything, ignoring the cache — for a page serving stale
    // assets rather than stale HTML.
    function hardReload() {
        if (root.startPage)
            return;
        if (root.wakeIfDeferred())
            return;
        view.reloadAndBypassCache();
    }
    function stop() { if (!startPage) view.stop(); }
    function navigate(target) {
        root.tabs.setProperty(root.index, "pageUrl", String(target));
    }

    readonly property bool poppedOut: view.parent !== viewHost

    function popOut() {
        if (root.startPage || root.poppedOut)
            return;
        root.popout.attach(view, viewHost, root.tabs.get(root.index).title);
    }

    function popIn() {
        view.parent = viewHost;
    }

    // Rebuild this view's script collection for the
    // page it is about to show. Called once per
    // navigation, never per frame.
    // `injectNow` covers the case where scripts
    // arrive after the page already loaded — an
    // extension being enabled, or bundles finishing
    // their scan. WebEngineScript only injects on
    // the next navigation, so the page would
    // otherwise stay untouched until reloaded.
    // True once an extension content script is on
    // this page, which is the only time the bridge
    // below needs to run at all.
    property bool bridgeActive: false

    // Drains calls queued by content scripts and
    // answers them. This exists because Qt only
    // injects its WebChannel transport into the
    // view that owns the channel — a page view's
    // isolated world never gets one. runJavaScript
    // does reach that world, so it is the transport.
    function pumpBridge() {
        if (root.startPage || (!root.bridgeActive && !root.extensionPage))
            return;
        root.runInPageWorld(
            "window.__telaDrain ? window.__telaDrain() : '[]'",
            function (encoded) {
                if (!encoded || encoded === "[]")
                    return;
                let calls = [];
                try {
                    calls = JSON.parse(encoded);
                } catch (error) {
                    return;
                }
                for (const call of calls)
                    root.answerCall(call);
            });
    }

    // Which extension this page is, from its address. Empty for a web page.
    readonly property string pageExtensionId: {
        const url = String(root.pageUrl);
        if (!url.startsWith("tela-extension://"))
            return "";
        const rest = url.slice("tela-extension://".length);
        const end = rest.indexOf("/");
        return end < 0 ? rest : rest.slice(0, end);
    }

    function answerCall(call) {
        const a = call.args || [];
        let result;

        // The caller names the extension it speaks for. On an extension's own
        // page, replace that claim with the address — otherwise uBlock's
        // dashboard could ask for Privacy Badger's storage and get it.
        if (root.pageExtensionId.length > 0 && a.length > 0)
            a[0] = root.pageExtensionId;

        switch (call.method) {
        case "storageGet":    result = root.api.storageGet(a[0], a[1]); break;
        case "storageSet":    result = root.api.storageSet(a[0], a[1]); break;
        case "storageRemove": result = root.api.storageRemove(a[0], a[1]); break;
        case "storageClear":  result = root.api.storageClear(a[0]); break;
        case "tabsQuery":     result = root.api.tabsQuery(a[0]); break;
        case "postMessage":
            root.api.postMessage(a[0], a[1], a[2]);
            result = true;
            break;
        case "postResponse":
            root.api.postResponse(a[0], a[1], a[2]);
            result = true;
            break;
        case "applySystemTheme":
            result = root.api.applySystemTheme(a[0], a[1]);
            break;
        // Only this path can answer it: the host
        // object is shared by every view, but the
        // queue is drained per view, so "which tab
        // am I in" is answerable exactly here.
        case "tabId":
            result = root.tabId;
            break;
        // Same reason: a content script that names no tab means the one it is
        // running in, and only this delegate knows which that is. A background
        // page reaches these through the channel instead.
        case "executeScript":
            result = root.api.executeScript(a[0], a[1] >= 0 ? a[1] : root.tabId, a[2]);
            break;
        case "insertCss":
            result = root.api.insertCss(a[0], a[1] >= 0 ? a[1] : root.tabId, a[2]);
            break;
        case "removeCss":
            result = root.api.removeCss(a[0], a[1] >= 0 ? a[1] : root.tabId, a[2]);
            break;
        default:
            // Unknown methods resolve rather than
            // hang, so a shim/host mismatch shows up
            // as undefined instead of a dead promise.
            result = null;
        }

        root.runInPageWorld(
            "window.__telaResolve(" + JSON.stringify(call.id) + ","
                + JSON.stringify(result) + ")");
    }

    // Deliver a host-side message into this page's
    // content scripts.
    // An extension's own page — a dashboard, a logger, an options page — shown
    // in an ordinary tab. It gets its shim from the scheme handler as plain
    // <script> tags, so its `browser.*` lives in the **main** world, and it
    // hosts no content scripts at all.
    //
    // Both facts broke delivery into it: `bridgeActive` is false there, and
    // UserWorld is the wrong world. uBlock's dashboard hides its whole body
    // behind `body.notReady` until its background answers `readyToFilter`, so
    // an undelivered reply is not an error — it is a page that stays black
    // forever, waiting.
    readonly property bool extensionPage:
        String(root.pageUrl).startsWith("tela-extension://")

    // Where this page's `browser.*` lives, which is not the same for the two
    // kinds of page a tab can hold. An extension's own page gets its shim from
    // the scheme handler as ordinary <script> tags — main world. A web page
    // gets it as an injected content script — isolated world, so page code
    // cannot reach it.
    function runInPageWorld(source, callback) {
        if (root.extensionPage) {
            if (callback)
                view.runJavaScript(source, callback);
            else
                view.runJavaScript(source);
            return;
        }
        if (callback)
            view.runJavaScript(source, WebEngineScript.UserWorld, callback);
        else
            view.runJavaScript(source, WebEngineScript.UserWorld);
    }

    function deliverInto(call) {
        if (root.startPage)
            return;
        if (root.extensionPage || root.bridgeActive)
            root.runInPageWorld(call);
    }

    // Reports back whether the page actually took the message.
    //
    // A broadcast to tabs reaches pages that have no bridge for this extension
    // and pages whose shim has not landed yet. Both used to swallow it — the
    // early return here, and the `window.__telaDeliver &&` guard — so a sender
    // expecting a reply waited out its timeout against nobody. The host needs
    // to know that happened so it can answer instead.
    function deliverMessage(ext, payload, onDone) {
        if (root.startPage || !(root.extensionPage || root.bridgeActive)) {
            if (onDone)
                onDone(false);
            return;
        }
        root.runInPageWorld(
            "(function(){ if (!window.__telaDeliver) return false;"
            + " window.__telaDeliver(" + JSON.stringify(ext) + ","
            + JSON.stringify(payload) + "); return true; })()",
            onDone ? function (took) { onDone(took === true); } : undefined);
    }

    function deliverResponse(ext, request, payload) {
        root.deliverInto("window.__telaResponse && window.__telaResponse("
                         + JSON.stringify(ext) + "," + JSON.stringify(request)
                         + "," + JSON.stringify(payload) + ")");
    }

    Timer {
        // Runs on the two kinds of page that have a `browser.*` to drain: one
        // hosting an extension content script, and an extension's own page.
        // The latter has no WebChannel of its own — only background contexts
        // and the action popup are given one — so this poll is its *only* way
        // to reach the host. Without it, uBlock's dashboard asks whether the
        // engine is ready and the question never leaves the page.
        running: (root.bridgeActive || root.extensionPage) && !root.startPage
        interval: 120
        repeat: true
        onTriggered: root.pumpBridge()
    }

    // Origins where no extension code runs, ever.
    //
    // This is the actual substance behind the
    // "browser is not secure" screen: an embedded
    // webview can inject script into a sign-in page
    // and read the password out of the DOM. Refusing
    // to inject on credential origins is what makes
    // that untrue here, rather than merely
    // unreported.
    //
    // Matched on host, not on the full URL — a
    // substring test would let evil-accounts.google
    // .com.attacker.net through.
    function isCredentialOrigin(target) {
        let host = "";
        try {
            host = new URL(target).hostname.toLowerCase();
        } catch (error) {
            return false;
        }
        const guarded = [
            "accounts.google.com",
            "accounts.youtube.com",
            "login.microsoftonline.com",
            "appleid.apple.com",
            "github.com",
            "login.live.com",
            "signin.aws.amazon.com"
        ];
        return guarded.some((entry) =>
            host === entry || host.endsWith("." + entry));
    }

    // A single, browser-level identity fix — not extension code, not the shim.
    //
    // Google's sign-in screens Chromium engines out by reading
    // navigator.userAgentData.brands from JavaScript: it lists "Chromium" but
    // not "Google Chrome", which contradicts the User-Agent and trips the
    // "browser may not be secure" wall. The sec-ch-ua header can be corrected
    // from the interceptor, but this JS object cannot — it is engine-generated.
    //
    // This runs at document creation in the main world, and does one thing:
    // add the "Google Chrome" brand so the JS identity matches the header and
    // the engine. It reads nothing, touches no form, sends nothing. It is the
    // deliberate, user-approved exception to "no code on credential origins" —
    // scoped to Google hosts by its own first line, and to this one purpose.
    readonly property var identityScript: ({
        name: "tela:identity",
        sourceCode:
            "(function(){try{"
            + "var h=location.hostname.toLowerCase();"
            // Not on the sign-in host: it is presented as Firefox by the
            // interceptor, so Chrome JS here would contradict it — and keeping
            // this script off the one credential origin restores the rule that
            // no injected code runs on them.
            + "if(h==='accounts.google.com'||h==='accounts.youtube.com')return;"
            + "if(!(h==='google.com'||h.endsWith('.google.com')||"
            + "h==='youtube.com'||h.endsWith('.youtube.com')))return;"
            + "var r=navigator.userAgentData;"
            + "if(!r||!Array.isArray(r.brands))return;"
            + "if(r.brands.some(function(b){return b.brand==='Google Chrome';}))return;"
            // Version comes from the UA, which the interceptor and profile set
            // to a current Chrome, so the JS identity matches the headers rather
            // than the engine's stale number.
            + "var mm=(navigator.userAgent.match(/Chrome\\/(\\d+)/)||[])[1];"
            + "var m=mm||'140';r.brands.forEach(function(b){if(b.brand==='Chromium')m=b.version;});"
            + "if(mm)m=mm;"
            + "var brands=r.brands.map(function(b){"
            + "return b.brand==='Chromium'?{brand:'Chromium',version:m}:b;})"
            + ".concat([{brand:'Google Chrome',version:m}]);"
            + "var fake={brands:brands,mobile:r.mobile,platform:r.platform,"
            + "getHighEntropyValues:function(hints){return r.getHighEntropyValues(hints)"
            + ".then(function(v){try{if(v&&Array.isArray(v.fullVersionList)&&"
            + "!v.fullVersionList.some(function(b){return b.brand==='Google Chrome';})){"
            + "var fv='140.0.0.0';v.fullVersionList.forEach(function(b){"
            + "if(b.brand==='Chromium')fv=b.version;});"
            + "v.fullVersionList=v.fullVersionList.concat([{brand:'Google Chrome',version:fv}]);"
            + "}}catch(e){}return v;});},"
            + "toJSON:function(){return{brands:brands,mobile:r.mobile,platform:r.platform};}};"
            + "Object.defineProperty(navigator,'userAgentData',{value:fake,configurable:true});"
            // Real Chrome exposes chrome.loadTimes/csi/app; an embedded engine
            // leaves window.chrome an empty object, which is the canonical
            // "is this a real browser" tell. Provide the same surface. These
            // are legacy no-op-ish APIs — real Chrome's own loadTimes is
            // deprecated — so plausible shapes are enough.
            + "var c=window.chrome;if(c){"
            + "if(!c.loadTimes)c.loadTimes=function(){var t=performance.timing||{};"
            + "var s=(t.navigationStart||Date.now())/1000;return{requestTime:s,"
            + "startLoadTime:s,commitLoadTime:s,finishDocumentLoadTime:s,finishLoadTime:s,"
            + "firstPaintTime:s,firstPaintAfterLoadTime:0,navigationType:'Other',"
            + "wasFetchedViaSpdy:true,wasNpnNegotiated:true,npnNegotiatedProtocol:'h2',"
            + "wasAlternateProtocolAvailable:false,connectionInfo:'h2'};};"
            + "if(!c.csi)c.csi=function(){return{startE:Date.now(),onloadT:Date.now(),"
            + "pageT:performance.now(),tick:0};};"
            + "if(!c.app)c.app={isInstalled:false,"
            + "InstallState:{DISABLED:'disabled',INSTALLED:'installed',NOT_INSTALLED:'not_installed'},"
            + "RunningState:{CANNOT_RUN:'cannot_run',READY_TO_RUN:'ready_to_run',RUNNING:'running'}};}"
            // A real desktop Chrome ships the PDF viewer, so plugins is non-empty
            // and pdfViewerEnabled is true. Empty is another webview tell.
            + "try{Object.defineProperty(navigator,'pdfViewerEnabled',"
            + "{get:function(){return true;},configurable:true});}catch(e){}"
            + "}catch(e){}})();",
        injectionPoint: WebEngineScript.DocumentCreation,
        worldId: WebEngineScript.MainWorld,
        runOnSubframes: true
    })

    // The JSON of the scripts currently installed on
    // this view. Assigning userScripts.collection makes
    // Qt recompile every script in it, and the set
    // rarely changes between navigations — same-site
    // links almost always match the same extensions.
    // Re-parsing ~1.7 MB of JavaScript per navigation
    // was costing about four seconds a page.
    property string installedKey: ""

    function applyUserScripts(target, injectNow) {
        if (root.isCredentialOrigin(target)) {
            // Extension code stays gated here — only the identity script runs,
            // and only because it is the one exception the user approved. The
            // sentinel key avoids recompiling it on every same-origin hop.
            root.bridgeActive = false;
            if (root.installedKey !== "identity-only") {
                root.installedKey = "identity-only";
                view.userScripts.collection = [root.identityScript];
            }
            return;
        }
        // Userscripts still run; only extension
        // content scripts are gated, because those
        // are the ones that break pages today.
        const encoded = root.theme.injectContentScripts
            ? root.extensions.scriptsForUrl(target)
            : "[]";

        // Identical set: the compiled scripts already
        // on the view are exactly the ones wanted, so
        // touching the collection would only pay the
        // compile cost again for no change.
        if (encoded === root.installedKey && !injectNow)
            return;

        if (encoded === "[]") {
            root.bridgeActive = false;
            // No extension scripts, but the identity script still rides along.
            if (root.installedKey !== "identity-only") {
                root.installedKey = "identity-only";
                view.userScripts.collection = [root.identityScript];
            }
            return;
        }

        let list = [];
        try {
            list = JSON.parse(encoded);
        } catch (error) {
            console.warn("userscript payload was not valid JSON");
            return;
        }

        // Any entry naming an extension means the
        // bridge is needed on this page.
        root.bridgeActive = encoded.indexOf("tela:webchannel") >= 0;

        if (injectNow && !view.loading) {
            for (const entry of list) {
                view.runJavaScript(entry.source,
                                   WebEngineScript.UserWorld);
            }
        }

        root.installedKey = encoded;
        // Identity script first, then the extension content scripts. Different
        // worlds (main vs isolated), so they do not interfere.
        view.userScripts.collection = [root.identityScript].concat(list.map(entry => ({
            name: entry.name,
            sourceCode: entry.source,
            injectionPoint: entry.injectionPoint === "DocumentCreation"
                ? WebEngineScript.DocumentCreation
                : entry.injectionPoint === "DocumentReady"
                  ? WebEngineScript.DocumentReady
                  : WebEngineScript.Deferred,
            // UserWorld keeps scripts in an isolated
            // context: same DOM, separate globals, so
            // page code cannot reach GM_* or be
            // clobbered by the script.
            worldId: WebEngineScript.UserWorld,
            // Top document only, for now — *not* what the manifest asks for.
            //
            // `scripts_for_url` does report each script's `all_frames`, and
            // honouring it looks obviously right: uBlock declares it, and ads
            // are built inside `srcdoc` iframes precisely because that is where
            // a blocker cannot see them. Measured, it made things worse —
            // cineby went from 4 hiding stylesheets to none.
            //
            // The reason is `frameId`. The port sender reports 0 unconditionally
            // because Qt gives no per-frame identity to a user script, so with
            // subframe injection on, *every* frame claims to be the top one.
            // uBlock then binds some inner frame as the page and computes the
            // wrong document's filters. Enabling this needs real frame ids
            // first; until then the honest answer is the top document.
            runOnSubframes: false
        })));
    }

    // Stand-in while the view lives in the root.popout.
    ColumnLayout {
        anchors.centerIn: parent
        visible: root.poppedOut
        spacing: 10

        Label {
            Layout.alignment: Qt.AlignHCenter
            text: "picture_in_picture"
            color: root.theme.textFaint
            font.family: root.theme.fontIcon
            font.pixelSize: 34
        }
        Label {
            Layout.alignment: Qt.AlignHCenter
            text: "Playing in a popout window"
            color: root.theme.textDim
            font.family: root.theme.fontMain
            font.pixelSize: root.theme.fontSmall
        }
        Rectangle {
            Layout.alignment: Qt.AlignHCenter
            implicitWidth: returnLabel.implicitWidth + 26
            implicitHeight: 30
            radius: root.theme.roundingSmall
            color: returnHover.hovered ? root.theme.accent
                                       : root.theme.accentContainer

            Behavior on color { ColorAnimation { duration: root.theme.durFast } }

            HoverHandler { id: returnHover }
            TapHandler { onTapped: root.popout.returnHome() }

            Label {
                id: returnLabel
                anchors.centerIn: parent
                text: "Bring it back"
                color: returnHover.hovered ? root.theme.onAccent
                                           : root.theme.onAccentContainer
                font.family: root.theme.fontMain
                font.pixelSize: root.theme.fontSmall
            }
        }
    }

    // Covers the blank frame while a resumed tab reloads its discarded renderer.
    // A plain layout parented into root — never a Popup, never clipped — so it
    // cannot punch the compositor hole a WebEngineView in a Popup would (see
    // CLAUDE.md). Only visible on the current tab during its own reload; a
    // suspended background tab is off-screen and shows nothing.
    ColumnLayout {
        anchors.centerIn: parent
        visible: root.resuming && root.loading
        spacing: 10

        Image {
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: 32
            Layout.preferredHeight: 32
            source: root.tabs.get(root.index) ? root.tabs.get(root.index).iconUrl : ""
            fillMode: Image.PreserveAspectFit
            visible: status === Image.Ready
        }
        Label {
            Layout.alignment: Qt.AlignHCenter
            text: (root.tabs.get(root.index) ? root.tabs.get(root.index).title : "") || "Waking tab…"
            color: root.theme.textDim
            font.family: root.theme.fontMain
            font.pixelSize: root.theme.fontSmall
        }
    }

    StartPage {
        anchors.fill: parent
        visible: root.startPage && !root.customStart
        theme: root.theme

        onNavigate: target => root.navigate(target)
        onPaletteRequested: root.paletteRequested()
        onCustomiseRequested: root.customiseRequested()

        Component.onCompleted: if (root.startPage)
            root.tabs.setProperty(root.index, "title", "New Tab")
    }

    // The desktop's own file chooser (zenity/kdialog) for a page's file input.
    // The engine's built-in picker never gives the system dialog, so this calls
    // it directly and feeds the chosen paths back to the request.
    FilePicker {
        id: filePicker
        // The engine request awaiting an answer; cleared once answered.
        property var request: null

        onPicked: paths => {
            if (!filePicker.request)
                return;
            const list = String(paths).length > 0
                       ? String(paths).split("\n").filter(p => p.length > 0) : [];
            if (list.length > 0)
                filePicker.request.dialogAccept(list);
            else
                filePicker.request.dialogReject();
            filePicker.request = null;
        }
    }

    // Host item the view normally lives in. Kept
    // separate so the view can be moved out to the
    // popout window and back again.
    Item {
        id: viewHost
        anchors.fill: parent
    }

    WebEngineView {
        id: view

        parent: viewHost
        profile: root.pageProfile
        width: parent ? parent.width : 0
        height: parent ? parent.height : 0
        visible: (!root.startPage || root.customStart) && !root.surfaceStale

        // Mute follows the model role, so the tab strip's speaker toggle and a
        // Ctrl+M both reach the page through the same bit.
        audioMuted: root.muted
        // Publish whether the page is making noise, so the tab strip can show
        // the speaker icon. Guarded against the start page, which has no view.
        onRecentlyAudibleChanged: if (!root.startPage)
            root.tabs.setProperty(root.index, "audible", recentlyAudible)
        // Leaving the start page unloaded keeps a
        // renderer process from spawning per tab.
        url: root.customStart ? root.theme.config.startCustomUrl
           : (root.startPage || root.deferred) ? ""
           : root.pageUrl

        settings {
            // A network blocker cannot stop an ad
            // popup: window.open is not a request.
            // Only a genuine user gesture may open a
            // window, which is what every mainstream
            // browser does and what stops the popup
            // ads that filter lists never see.
            javascriptCanOpenWindows: root.theme.allowPopups
        }

        // A window the page asked for. Anything not
        // triggered by a click arrives here only when
        // popups are allowed; the rest never get this
        // far. Opening it as a tab rather than a bare
        // window keeps it inside the session and the
        // blocker.
        onNewWindowRequested: function (request) {
            root.newTabRequested(String(request.requestedUrl));
        }

        // A page's <input type=file>. Taken over so it opens the desktop's own
        // file chooser (via QtQuick.Dialogs + the XDG portal) rather than the
        // engine's built-in picker. Folder uploads have no FileDialog equivalent,
        // so those are declined.
        onFileDialogRequested: function (request) {
            request.accepted = true;
            // WebEngineFileDialogRequest.FileMode as plain integers — the QML
            // enum name does not resolve here and referencing it throws, which
            // silently killed the handler before it could open anything:
            //   0 open one · 1 open many · 2 upload folder · 3 save
            const m = request.mode;
            if (m === 2) {
                // No folder-picker equivalent in the zenity call; decline.
                request.dialogReject();
                return;
            }
            const mode = m === 3 ? "save" : m === 1 ? "multiple" : "open";
            filePicker.request = request;
            filePicker.open(mode, mode === "save" ? "Save file" : "Open file", "");
        }

        // The File System Access API (window.showOpenFilePicker / showSaveFilePicker),
        // which apps like ComfyUI use for Open/Save instead of a plain file input.
        // Unhandled, Qt denies it and the page's picker call simply resolves to
        // nothing — "Open opened nothing". Granting access to the file the user
        // is choosing is the expected behaviour; the actual chooser still comes
        // through onFileDialogRequested above.
        onFileSystemAccessRequested: request => request.accept()

        // Everything a content script says or throws lands here and nowhere
        // else — until now it went nowhere at all, which is why an extension
        // failing inside a page looked exactly like an extension doing nothing.
        // Resolved once per tab rather than per message: an ordinary page logs
        // constantly.
        readonly property bool tracePage: root.api.debugEnabled("page")

        onJavaScriptConsoleMessage: (level, message, line, source) => {
            if (!view.tracePage)
                return;
            // The source file is the useful identity here — a content script
            // names its extension, and a page script names the page.
            const where = String(source).split("/").pop() + ":" + line;
            root.api.debugLog(level >= 2 ? "err" : "trace", "page",
                              "tab" + root.tabId, where, message);
        }

        onTitleChanged: {
            if (title.length > 0 && !root.startPage) {
                root.tabs.setProperty(root.index, "title", title);
                root.sessionDirty();
                root.tabsDirty();
            }
        }
        onIconChanged: root.tabs.setProperty(root.index, "iconUrl", String(icon))
        onLoadingChanged: function (info) {
            root.tabs.setProperty(root.index, "loading", loading);
            if (root.startPage || !info)
                return;
            // Committed, then finished — the two
            // points an extension rebuilds its
            // picture of this tab at.
            if (info.status === WebEngineView.LoadStartedStatus)
                root.events.navigated(root.index, String(info.url));
            else if (info.status === WebEngineView.LoadSucceededStatus) {
                root.events.loaded(root.index, String(info.url));
                // Record the visit; the store ignores internal/credential URLs.
                root.visited(String(info.url), view.title);
                // Put the page back where it was — after a resume-reload, or the
                // first load of a restored tab.
                if (root.pendingScrollRestore) {
                    view.runJavaScript("window.scrollTo(" + root.scrollX
                                       + "," + root.scrollY + ")");
                    root.pendingScrollRestore = false;
                }
                root.resuming = false;
            }
        }
        // Until now a page asking for the camera, microphone, location or
        // notifications got no answer at all, and Qt leaves an unanswered
        // request pending — the page waits forever on a promise that never
        // settles, which is indistinguishable from the feature being broken.
        //
        // Denied rather than prompted: a prompt is its own piece of UI, and
        // refusing by default is the safer half to ship first. Chromium
        // persists the refusal, so the site panel can list it and grant it.
        onPermissionRequested: permission => {
            permission.deny();
            if (view.tracePage)
                root.api.debugLog("trace", "page", "tab" + root.tabId,
                                  "permission",
                                  "denied " + permission.permissionType
                                  + " for " + permission.origin);
        }

        // Without this a video's fullscreen button does nothing at all: Qt
        // waits for the embedder to accept, and an unanswered request is
        // simply never granted. Accepting is what actually puts the page in
        // fullscreen; the signal is only so the window can get out of its way.
        onFullScreenRequested: request => {
            request.accept();
            root.fullScreenRequested(request.toggleOn);
        }

        onContextMenuRequested: request => {
            // Taking the request means Qt will not
            // raise its own unthemed menu.
            request.accepted = true;
            root.contextMenuRequested(root, request);
        }

        onUrlChanged: {
            if (!root.startPage) {
                root.tabs.setProperty(root.index, "pageUrl", String(url));
                root.recordVisit(url);
            }
            // A real navigation leaves reader mode behind.
            root.readerActive = false;
            root.applyUserScripts(String(url), false);
            root.tabsDirty();
        }
    }

    // Google refuses its sign-in to every Qt WebEngine browser — qutebrowser and
    // Falkon included, verified — so there is nothing Tela can do in-engine to
    // pass. Rather than leave a dead end, offer the one thing that works: hand
    // the page to a standalone browser that Google accepts. Shown only on the
    // sign-in host, and only when such a browser is installed.
    readonly property bool googleSignInBlocked: {
        if (root.startPage)
            return false;
        let host = "";
        let path = "";
        try {
            const parsed = new URL(String(root.url));
            host = parsed.hostname.toLowerCase();
            path = parsed.pathname;
        } catch (error) {
            return false;
        }
        // Google Identity Services (the embedded "Sign in with Google" button /
        // One-Tap) is not a login the user initiates here — showing the handoff
        // banner over it, and spoofing its UA, only breaks it.
        if (path.startsWith("/gsi/"))
            return false;
        return host === "accounts.google.com" || host === "accounts.youtube.com";
    }

    readonly property string systemBrowser:
        root.theme.config.systemBrowserName()

    FirefoxImport { id: firefoxImport }

    // Status of the last import attempt, shown in place of the prompt.
    property string handoffStatus: ""
    // Whether the import button is asking for confirmation. The import copies a
    // live account session, so it takes a deliberate second click, never one.
    property bool confirmingImport: false

    Rectangle {
        id: handoffBanner
        visible: root.googleSignInBlocked && root.systemBrowser.length > 0
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.margins: 10
        height: visible ? bannerCol.implicitHeight + 20 : 0
        radius: root.theme.rounding
        color: root.theme.surfaceHigh
        border.color: root.theme.border
        border.width: 1
        z: 50

        ColumnLayout {
            id: bannerCol
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: 14
            anchors.rightMargin: 10
            spacing: 8

            RowLayout {
                Layout.fillWidth: true
                spacing: 12

                Label {
                    text: "info"
                    color: root.theme.accent
                    font.family: root.theme.fontIcon
                    font.pixelSize: root.theme.iconSize
                }

                Label {
                    Layout.fillWidth: true
                    text: root.confirmingImport
                          ? "This copies your Google login from " + root.systemBrowser
                            + " into Tela. Continue?"
                          : root.handoffStatus.length > 0
                            ? root.handoffStatus
                            : "Google blocks sign-in in every Qt-based browser, this "
                              + "one included. 1. Sign in with " + root.systemBrowser
                              + ".  2. Import the session here."
                    color: root.theme.text
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontSmall
                    wrapMode: Text.Wrap
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 34
                spacing: 8

                BannerButton {
                    theme: root.theme
                    visible: !root.confirmingImport
                    label: "1. Sign in with " + root.systemBrowser
                    onClicked: root.theme.config.openInSystemBrowser(String(root.url))
                }

                BannerButton {
                    theme: root.theme
                    visible: !root.confirmingImport
                    label: "2. Import session"
                    primary: true
                    // Only offered when Firefox is the browser holding the
                    // session — its cookies are the ones this can read.
                    enabled: firefoxImport.firefoxAvailable()
                    // First click only arms the confirm; the copy happens on the
                    // explicit "Yes, import" below.
                    onClicked: root.confirmingImport = true
                }

                BannerButton {
                    theme: root.theme
                    visible: root.confirmingImport
                    label: "Yes, import"
                    primary: true
                    onClicked: {
                        root.confirmingImport = false;
                        root.handoffStatus = firefoxImport.import();
                        if (root.url && String(root.url).length > 0)
                            root.reload();
                    }
                }

                BannerButton {
                    theme: root.theme
                    visible: root.confirmingImport
                    label: "Cancel"
                    onClicked: root.confirmingImport = false
                }

                Item { Layout.fillWidth: true }
            }
        }
    }

    component BannerButton: Rectangle {
        id: bannerButton
        required property var theme
        property string label: ""
        property bool primary: false
        signal clicked()

        implicitWidth: bannerButtonLabel.implicitWidth + 24
        implicitHeight: 30
        radius: bannerButton.theme.roundingSmall
        opacity: bannerButton.enabled ? 1 : 0.4
        color: !bannerButton.primary
               ? (bannerButtonHover.hovered ? bannerButton.theme.surfaceActive
                                            : bannerButton.theme.surface)
               : (bannerButtonHover.hovered ? bannerButton.theme.accent
                                            : bannerButton.theme.accentContainer)
        border.color: bannerButton.primary ? "transparent" : bannerButton.theme.border
        border.width: 1

        HoverHandler { id: bannerButtonHover; enabled: bannerButton.enabled }
        TapHandler {
            enabled: bannerButton.enabled
            onTapped: bannerButton.clicked()
        }

        Label {
            id: bannerButtonLabel
            anchors.centerIn: parent
            text: bannerButton.label
            color: bannerButton.primary
                   ? (bannerButtonHover.hovered ? bannerButton.theme.onAccent
                                                : bannerButton.theme.onAccentContainer)
                   : bannerButton.theme.text
            font.family: bannerButton.theme.fontMain
            font.pixelSize: bannerButton.theme.fontSmall
            font.weight: bannerButton.primary ? Font.DemiBold : Font.Normal
        }
    }
}
