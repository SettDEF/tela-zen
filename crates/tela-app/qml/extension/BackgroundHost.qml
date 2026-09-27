import QtQuick
import QtWebEngine

pragma ComponentBehavior: Bound

// One hidden view per enabled extension that declares background scripts.
//
// These are real renderer processes, so they exist only for extensions that
// actually need one — see PERFORMANCE.md. This is also the only place that
// knows which context belongs to which extension, so all delivery aimed at a
// background page goes through here.
Repeater {
    id: root

    required property var extensions
    required property var channel
    required property var theme
    // The host object, for the debug channels.
    required property var api
    required property var profile

    // Resolved once: a background page can be chatty, and asking per message
    // would put a call on the path of every line an extension prints.
    readonly property bool traceExt: root.api.debugEnabled("ext")

    model: {
        try {
            return JSON.parse(root.extensions.bundlesJson)
                .filter(bundle => bundle.enabled && bundle.hasBackground);
        } catch (error) {
            return [];
        }
    }

    function deliver(ext, payload) {
        for (let i = 0; i < count; i++) {
            const view = itemAt(i);
            if (view && view.extensionId === ext)
                view.deliverMessage(payload);
        }
    }

    function deliverResponse(ext, request, payload) {
        for (let i = 0; i < count; i++) {
            const view = itemAt(i);
            if (view && view.extensionId === ext)
                view.deliverResponse(request, payload);
        }
    }

    // Tab and navigation lifecycle, pushed to every extension that is
    // listening. Straight to the background contexts rather than through the
    // message bus: they are the only contexts that care, and each already
    // carries its own identity.
    function broadcastEvent(name, args) {
        if (count === 0)
            return;
        const payload = JSON.stringify({ body: { __telaEvent: name, args: args } });
        for (let i = 0; i < count; i++) {
            const view = itemAt(i);
            if (view)
                view.deliverMessage(payload);
        }
    }

    delegate: WebEngineView {
        id: backgroundView

        required property var modelData

        readonly property string extensionId: modelData.id

        // A background page cannot receive anything until its shim exists, and
        // it is still loading while content scripts are already talking — a
        // restore starts every tab and every background page at once. The
        // delivery calls guard on `window.__telaDeliver &&`, so before that
        // point the message was dropped on the floor: no error, no retry, and
        // the sender sat out its timeout. Extensions that block on a reply —
        // Dark Reader keeps a page blacked out until its theme arrives — then
        // never recovered.
        property bool ready: false
        property var queued: []

        // Bounded so a page that never loads cannot grow this without limit.
        readonly property int queueLimit: 200

        function send(source) {
            if (backgroundView.ready) {
                runJavaScript(source);
                return;
            }
            if (backgroundView.queued.length >= backgroundView.queueLimit) {
                console.warn("dropping queued message for", backgroundView.extensionId,
                             "— background page still not ready");
                return;
            }
            // Reassigned rather than pushed: a plain push does not notify.
            backgroundView.queued = backgroundView.queued.concat([source]);
        }

        function flush() {
            const waiting = backgroundView.queued;
            backgroundView.queued = [];
            for (const source of waiting)
                runJavaScript(source);
        }

        function deliverMessage(payload) {
            backgroundView.send("window.__telaDeliver && window.__telaDeliver("
                                + JSON.stringify(extensionId) + ","
                                + JSON.stringify(payload) + ")");
        }

        function deliverResponse(request, payload) {
            backgroundView.send("window.__telaResponse && window.__telaResponse("
                                + JSON.stringify(extensionId) + ","
                                + JSON.stringify(request) + ","
                                + JSON.stringify(payload) + ")");
        }

        profile: root.profile
        visible: false
        width: 0
        height: 0
        url: "tela-extension://" + modelData.id + "/_tela_background.html"

        webChannel: root.channel
        backgroundColor: root.theme.surface

        // The shim goes in at document creation, but runJavaScript has nowhere
        // to land until the document exists — so readiness is tied to the load
        // finishing, and anything sent before then waits in the queue.
        //
        // Cleared on a fresh load as well: a background page that reloads drops
        // its listeners, and delivering into the old document would vanish the
        // same silent way.
        onLoadingChanged: function (info) {
            if (!info)
                return;
            if (info.status === WebEngineView.LoadStartedStatus) {
                backgroundView.ready = false;
            } else if (info.status === WebEngineView.LoadSucceededStatus) {
                backgroundView.ready = true;
                backgroundView.flush();
            } else if (info.status === WebEngineView.LoadFailedStatus) {
                // Nothing is coming. Say so rather than letting every sender
                // sit out its timeout against a page that will never answer.
                console.warn("background page failed to load:",
                             backgroundView.extensionId, info.errorString);
                backgroundView.queued = [];
            }
        }

        // A background page's own output. On the `ext` channel rather than
        // unconditionally: this is the only window into an extension that has
        // no UI, but it is also the noisiest thing in the browser.
        onJavaScriptConsoleMessage: (level, message, line, source) => {
            if (!root.traceExt)
                return;
            root.api.debugLog(level >= 2 ? "err" : "trace", "ext",
                              backgroundView.extensionId,
                              String(source).split("/").pop() + ":" + line,
                              message);
        }
    }
}
