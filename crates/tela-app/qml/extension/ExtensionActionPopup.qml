import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtWebEngine

pragma ComponentBehavior: Bound

// An extension's own popup — the real HTML the extension ships, not a
// re-creation of it.
//
// The page is served over tela-extension://, so it gets a proper origin, and
// the transport plus browser.* shim are injected ahead of it exactly as they
// are for a background context. Without those the popup loads and then dies on
// its first API call, which is worse than not showing it.
//
// Deliberately an Item, not a Popup. A Popup renders into the overlay layer,
// which a WebEngineView cannot composite into — the view comes up blank and
// punches a hole straight through the window to the desktop behind it. Staying
// in the ordinary scene graph costs a hand-written scrim and is the only way
// the page actually renders.
Item {
    id: root

    // Hiding the window releases the view's surface, and it comes back blank.
    // Dropping the view for one frame forces a fresh one; kept as a property
    // inside the `visible` binding so the binding survives. See
    // TabPage.repaint(), which has the same problem for the same reason.
    property bool surfaceStale: false

    function repaint() {
        if (!root.visible || root.surfaceStale)
            return;
        root.surfaceStale = true;
        Qt.callLater(function () { root.surfaceStale = false; });
    }

    anchors.fill: parent
    visible: false
    z: 900

    required property var theme
    required property var extensions
    required property var channel
    required property var browserProfile

    property string extensionId: ""
    property string popupPath: ""
    property string title: ""

    /// A link the popup tried to open elsewhere. Main turns it into a tab.
    signal newTabRequested(string url)

    // An extension's popup carries its own dimensions in its CSS — Proton VPN
    // is not uBlock is not SponsorBlock — so the panel is measured from the
    // document once it has laid out, rather than forced to one size that is
    // wrong for everything. The bounds are Firefox's: below the minimum a popup
    // is unusable, above the maximum it stops being a popup.
    readonly property int minWidth: 240
    // Below the point where a responsive popup gives up on a side-by-side
    // layout. uBlock lays out landscape by default — a 340px panel beside a
    // 480px dynamic-filtering pane — and only collapses to one column once the
    // viewport is under (340 + 480) / 1.1 ≈ 745. Measuring the landscape
    // layout and then growing to fit it kept the popup just wide enough to
    // stay landscape, with the pane sitting there empty.
    readonly property int maxWidth: 720
    readonly property int minHeight: 120
    readonly property int maxHeight: 600

    // Starting size, replaced as soon as the page reports its own.
    property int panelWidth: 420
    property int panelHeight: 560

    /// Ask the page how big it wants to be, and fit to that.
    function fitToContent() {
        view.runJavaScript(
            "(function(){\
               var d = document.documentElement, b = document.body;\
               if (!d) return null;\
               var box = b ? b.getBoundingClientRect() : null;\
               /* The rendered body box, not the scroll extent: uBlock lays\
                  out a collapsed pane beside its content, and scrollWidth\
                  counts it, leaving a dead column in the panel. */\
               var w = box && box.width > 0\
                       ? box.width\
                       : Math.max(d.scrollWidth, b ? b.scrollWidth : 0);\
               /* The rendered body box for height too — symmetric with width.\
                  scrollHeight is the scroll extent, which a body stretched to\
                  the panel height reports back unchanged, so the popup could\
                  never shrink below whatever it opened at (Violentmonkey's\
                  short popup sat at the 600px maximum). box.height is the real\
                  content height; fall back to scrollHeight only if the body has\
                  not laid out yet, so a half-built page never collapses to a\
                  sliver. */\
               var h = box && box.height > 40\
                       ? box.height\
                       : Math.max(d.scrollHeight, b ? b.scrollHeight : 0);\
               return [w, h];\
             })()",
            function (size) {
                if (!size || size.length !== 2)
                    return;
                // A page still laying out reports 0; keep what we have rather
                // than collapsing the panel to nothing.
                if (size[0] > 0)
                    root.panelWidth = Math.max(root.minWidth,
                                               Math.min(root.maxWidth, Math.round(size[0])));
                if (size[1] > 0)
                    root.panelHeight = Math.max(root.minHeight,
                                                Math.min(root.maxHeight, Math.round(size[1])));
            });
    }

    function open() { root.visible = true; }
    function close() { root.visible = false; root.extensionId = ""; }

    // Stands in for Popup's modal dim and click-outside-to-close.
    Rectangle {
        anchors.fill: parent
        color: "#000000"
        opacity: 0.45
        TapHandler { onTapped: root.close() }
    }

    // Escape has to be a Shortcut, not Keys.onEscapePressed: the WebEngineView
    // holds focus whenever the popup is up, so the key never reaches this Item.
    // Scoped to `visible` so the element picker keeps Escape once we are gone.
    Shortcut {
        sequence: "Escape"
        enabled: root.visible
        onActivated: root.close()
    }

    function deliverMessage(ext, payload) {
        if (ext !== extensionId || extensionId.length === 0)
            return;
        view.runJavaScript("window.__telaDeliver && window.__telaDeliver("
                           + JSON.stringify(ext) + "," + JSON.stringify(payload) + ")");
    }

    function deliverResponse(ext, request, payload) {
        if (ext !== extensionId || extensionId.length === 0)
            return;
        view.runJavaScript("window.__telaResponse && window.__telaResponse("
                           + JSON.stringify(ext) + "," + JSON.stringify(request)
                           + "," + JSON.stringify(payload) + ")");
    }

    function show(id, path, label) {
        extensionId = id;
        popupPath = path;
        title = label;
        // Reset to the starting size on every open. Without this the previous
        // popup's measured size carried over, and a page that measures large
        // (e.g. an extension whose popup cannot finish loading) left the panel
        // creeping bigger and drifting off-centre on each successive open.
        root.panelWidth = 420;
        root.panelHeight = 560;
        // Shim must be in the collection before the URL binding navigates.
        view.installShim();
        open();
    }

    onVisibleChanged: {
        if (!visible) {
            // Drop the page so its scripts stop and its renderer can go away.
            popupPath = "";
        }
    }

    // The panel itself. A child of the scene, not the overlay, so the view
    // inside it composites like any other item.
    Rectangle {
        id: panel

        anchors.centerIn: parent
        width: root.panelWidth
        height: root.panelHeight
        color: root.theme.surfaceHigh
        radius: root.theme.rounding
        border.color: root.theme.border
        border.width: 1
        clip: false

        // Swallows clicks so they do not reach the scrim behind.
        TapHandler { onTapped: {} }

        opacity: root.visible ? 1 : 0
        scale: root.visible ? 1.0 : 0.97
        Behavior on opacity {
            NumberAnimation { duration: root.theme.durFast }
        }
        Behavior on scale {
            NumberAnimation {
                duration: root.theme.durNormal
                easing.type: Easing.BezierSpline
                easing.bezierCurve: root.theme.curveSpatial
            }
        }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ── Title bar ──────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 38
            color: root.theme.surface
            topLeftRadius: root.theme.rounding
            topRightRadius: root.theme.rounding

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 8
                spacing: 8

                Label {
                    Layout.fillWidth: true
                    text: root.title
                    color: root.theme.text
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontSmall
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }

                Rectangle {
                    implicitWidth: 26
                    implicitHeight: 26
                    radius: root.theme.roundingSmall
                    color: closeHover.hovered ? root.theme.surfaceActive : "transparent"

                    HoverHandler { id: closeHover }
                    TapHandler { onTapped: root.close() }

                    Label {
                        anchors.centerIn: parent
                        text: "close"
                        color: root.theme.textDim
                        font.family: root.theme.fontIcon
                        font.pixelSize: root.theme.iconSizeSmall
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.border
        }

        // ── The extension's page ───────────────────────────────────────
        WebEngineView {
            id: view

            Layout.fillWidth: true
            Layout.fillHeight: true
            profile: root.browserProfile
            // Same surface loss as a page view: hiding the window releases it,
            // and the popup comes back blank. See TabPage.repaint().
            visible: !root.surfaceStale

            url: root.popupPath.length > 0 && root.extensionId.length > 0
                 ? "tela-extension://" + root.extensionId + "/"
                   + root.popupPath.replace(/^\//, "")
                 : ""

            webChannel: root.channel
            backgroundColor: root.theme.surfaceHigh

            onJavaScriptConsoleMessage: (level, message) => {
                console.log("[" + root.extensionId + " popup]", message);
            }

            // An extension popup dismisses itself with window.close(), and
            // several features depend on it happening: uBlock's element picker
            // closes the popup and *then* injects itself into the page, so a
            // popup that ignores this sits over the picker and eats every click
            // meant for it.
            onWindowCloseRequested: root.close()

            // A link out of a popup belongs in a tab — uBlock's dashboard,
            // logger and "report an issue" all leave this way. Main.openTab
            // closes the popup, matching Firefox: the panel goes away when it
            // has sent you somewhere.
            onNewWindowRequested: request => root.newTabRequested(String(request.requestedUrl))

            // The popup stays on the extension that owns it. Anything else is a
            // link the user followed, so it becomes a tab and the panel closes.
            // Without this a popup can be navigated onto the open web while
            // still holding the extension's shim.
            onNavigationRequested: request => {
                const target = String(request.url);
                if (target.startsWith("tela-extension://" + root.extensionId + "/")) {
                    request.accept();
                    return;
                }
                request.reject();
                if (target.length > 0 && target !== "about:blank")
                    root.newTabRequested(target);
            }

            // A dead renderer leaves an empty panel that looks like a popup with
            // nothing in it. Say so and get out of the way.
            onRenderProcessTerminated: (status, code) => {
                console.warn("popup renderer gone:", root.extensionId, status, code);
                root.close();
            }

            // ── Refusals ──────────────────────────────────────────────────
            // A popup is a transient panel. Everything below either cannot work
            // in one or should never be granted to one, and each must be
            // answered explicitly: an unhandled request in Qt WebEngine leaves
            // the page waiting on a reply that never comes, which reads as the
            // extension having hung.

            // alert/confirm/prompt have no dialog here; leaving them unanswered
            // blocks the popup's script mid-call.
            onJavaScriptDialogRequested: request => {
                request.accepted = true;
                request.dialogReject();
            }
            onColorDialogRequested: request => {
                request.accepted = true;
                request.dialogReject();
            }
            onFileDialogRequested: request => {
                request.accepted = true;
                request.dialogReject();
            }
            onAuthenticationDialogRequested: request => {
                request.accepted = true;
                request.dialogReject();
            }

            onPermissionRequested: permission => permission.deny()
            onFullScreenRequested: request => request.reject()
            onQuotaRequested: request => request.reject()
            onRegisterProtocolHandlerRequested: request => request.reject()
            onFileSystemAccessRequested: request => request.reject()
            onDesktopMediaRequested: request => request.cancel()
            onWebAuthUxRequested: request => request.cancel()
            onSelectClientCertificate: selection => selection.selectNone()
            onCertificateError: error => error.rejectCertificate()

            // Taking the request stops Qt raising its own unthemed menu, the
            // same reason TabPage takes it.
            onContextMenuRequested: request => { request.accepted = true; }
            onTouchSelectionMenuRequested: request => { request.accepted = true; }

            // The panel is sized from the document by fitToContent; letting the
            // page ask for a geometry would fight it.
            onGeometryChangeRequested: { }
            onPrintRequested: { }

            // The transport and shim have to be in place before the popup's own
            // scripts run, so they are injected at document creation with the
            // extension's identity baked in.
            // Injected at document creation, in the main world: an extension
            // page is the extension's own origin, so there is nothing to
            // isolate it from — and its scripts expect `browser` to be global.
            function installShim() {
                if (root.extensionId.length === 0) {
                    view.userScripts.collection = [];
                    return;
                }
                let list = [];
                try {
                    list = JSON.parse(root.extensions.pageScripts(root.extensionId));
                } catch (error) {
                    console.warn("could not build popup shim");
                    return;
                }
                view.userScripts.collection = list.map(entry => ({
                    name: entry.name,
                    sourceCode: entry.source,
                    injectionPoint: WebEngineScript.DocumentCreation,
                    worldId: WebEngineScript.MainWorld,
                    runOnSubframes: false
                }));
            }

            Component.onCompleted: installShim()

            onLoadingChanged: loadRequest => {
                if (loadRequest.status === WebEngineView.LoadFailedStatus) {
                    console.warn("popup failed to load:", loadRequest.errorString);
                    return;
                }
                if (loadRequest.status === WebEngineView.LoadSucceededStatus) {
                    root.fitToContent();
                    resizeSettle.restart();
                }
            }

            // The page telling us it relaid out, rather than us guessing when.
            // Extension popups build their UI in script and keep resizing as
            // they fill in — uBlock grows as it counts, and its panes expand on
            // demand — so a one-shot measure after load is always a snapshot of
            // a half-built panel.
            //
            // Only the trigger, not the measurement: contentsSize is the scroll
            // size, which counts uBlock's collapsed pane. fitToContent measures
            // the rendered body box instead.
            onContentsSizeChanged: resizeSettle.restart()

            // Coalesces a burst of relayouts into one measurement, and outlasts
            // the reflow that follows the last one.
            Timer {
                id: resizeSettle
                interval: 80
                repeat: false
                onTriggered: root.fitToContent()
            }
        }
    }
}
}
