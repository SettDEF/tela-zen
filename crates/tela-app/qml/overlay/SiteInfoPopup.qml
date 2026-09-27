import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Tela

pragma ComponentBehavior: Bound

// What this site is, what it may do, and what it has stored — the panel behind
// the address-bar indicators.
//
// Grouped the way the question is usually asked: who am I talking to, what have
// I let them do, and how do I take it back. The "take it back" half is the
// reason the panel exists, so it is not hidden behind a settings page.
//
// A plain Popup is safe here only because nothing inside is a WebEngineView —
// see ExtensionActionPopup for what happens otherwise.
Popup {
    id: root

    required property var theme
    required property var page
    required property var browserProfile
    required property var config

    padding: 0
    modal: true
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    implicitWidth: 340

    // The host the panel is about. Everything below is scoped to this.
    readonly property string host: {
        const url = root.page ? String(root.page.url) : "";
        const start = url.indexOf("://");
        if (start < 0)
            return "";
        const after = url.slice(start + 3);
        const slash = after.indexOf("/");
        return slash < 0 ? after : after.slice(0, slash);
    }

    readonly property string origin: {
        const url = root.page ? String(root.page.url) : "";
        const start = url.indexOf("://");
        return start < 0 ? "" : url.slice(0, start + 3) + root.host + "/";
    }

    readonly property bool secure: {
        const url = root.page ? String(root.page.url) : "";
        return url.startsWith("https://");
    }

    // Recounted whenever the panel opens: cookies change while it is closed.
    // Which indicator opened this. Each one asks a different question, so each
    // gets a panel answering only that question rather than four routes into
    // one wall of settings.
    property string mode: "security"

    readonly property bool showsBlocking: root.mode === "security"
    readonly property bool showsStorage: root.mode === "security"
    readonly property bool showsPermissions: root.mode !== "security"

    // Values of QWebEnginePermission::PermissionType this panel cares about.
    // The combined audio+video type belongs to both device panels.
    readonly property var permissionFilter:
        root.mode === "camera" ? [2, 3, 4, 5]
        : root.mode === "microphone" ? [1, 3, 5]
        : []

    readonly property var shownPermissions:
        root.permissionFilter.length === 0
        ? root.permissions
        : root.permissions.filter(function (entry) {
              return root.permissionFilter.indexOf(entry.permissionType) !== -1;
          })

    readonly property string heading:
        root.mode === "camera" ? "Camera"
        : root.mode === "microphone" ? "Microphone"
        : root.mode === "permissions" ? "Permissions"
        : "Security"

    property int cookieCount: 0
    property var permissions: []
    property bool confirmingAll: false
    property bool clearedCache: false

    Cookies { id: cookieStore }

    onOpened: root.refresh()

    function refresh() {
        root.cookieCount = root.host.length > 0
                           ? cookieStore.countForHost(root.host) : 0;
        root.permissions = root.readPermissions();
        root.confirmingAll = false;
        root.clearedCache = false;
    }

    // Chromium keeps permission state itself, so the panel reports what the
    // engine actually holds rather than a copy that can drift out of step.
    //
    // Listed rather than queried per type: QWebEnginePermission's enums are
    // scoped, which QML cannot name, and listing answers the more useful
    // question anyway — what has this site asked for.
    function readPermissions() {
        if (!root.browserProfile || root.origin.length === 0)
            return [];
        try {
            return root.browserProfile.listPermissionsForOrigin(root.origin) || [];
        } catch (error) {
            return [];
        }
    }

    // Values from QWebEnginePermission::PermissionType, in declaration order.
    // Read numerically because the scoped enum has no QML name to compare to;
    // an unknown value falls through to its number rather than a wrong label.
    function permissionLabel(type) {
        const names = ["Unsupported", "Microphone", "Camera", "Camera and microphone",
                       "Screen sharing", "Screen and audio sharing", "Pointer lock",
                       "Notifications", "Location", "Clipboard", "Local fonts"];
        return names[type] !== undefined ? names[type] : "Permission " + type;
    }

    // QWebEnginePermission::State.
    function stateLabel(state) {
        return state === 2 ? "Allowed" : state === 3 ? "Blocked" : "Ask";
    }

    readonly property var allowlist: {
        try {
            return JSON.parse(root.config.blockingAllowlistJson) || [];
        } catch (error) {
            return [];
        }
    }

    // Blocking is per page-host, so the switch reflects this exact site.
    readonly property bool filtered: {
        const host = root.host.toLowerCase();
        return !root.allowlist.some(function (entry) {
            const allowed = String(entry).toLowerCase();
            return host === allowed || host.endsWith("." + allowed);
        });
    }

    function setFiltered(on) {
        const host = root.host.toLowerCase();
        let list = root.allowlist.slice();
        if (on) {
            // Drop anything covering this host, or the switch flips back.
            list = list.filter(function (entry) {
                const allowed = String(entry).toLowerCase();
                return !(host === allowed || host.endsWith("." + allowed));
            });
        } else if (host.length > 0) {
            list.push(host);
        }
        root.config.setSetting("blocking.allowlist", JSON.stringify(list));
    }

    background: Rectangle {
        color: root.theme.surfaceHigh
        radius: root.theme.rounding
        border.color: root.theme.border
        border.width: 1
    }

    contentItem: ColumnLayout {
        spacing: 0

        // ── Identity ───────────────────────────────────────────────────
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            spacing: 10

            Label {
                text: root.secure ? "lock" : "lock_open_right"
                color: root.secure ? root.theme.text : root.theme.error
                font.family: root.theme.fontIcon
                font.pixelSize: root.theme.iconSize
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1

                Label {
                    Layout.fillWidth: true
                    text: root.host.length > 0 ? root.host : "No site"
                    color: root.theme.text
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontNormal
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }

                Label {
                    Layout.fillWidth: true
                    text: root.mode === "security"
                          ? (root.secure ? "Connection is encrypted"
                                         : "Connection is not encrypted")
                          : root.heading
                    color: root.mode !== "security" ? root.theme.textDim
                           : root.secure ? root.theme.textDim : root.theme.error
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontSmall
                    elide: Text.ElideRight
                }
            }
        }

        Divider { theme: root.theme; visible: root.showsBlocking }

        // ── Content blocking ───────────────────────────────────────────
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            spacing: 10
            visible: root.showsBlocking

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1

                Label {
                    text: "Block ads and trackers"
                    color: root.theme.text
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontNormal
                }
                Label {
                    Layout.fillWidth: true
                    text: root.filtered ? "Filtering this site"
                                        : "Turned off for this site"
                    color: root.theme.textDim
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontSmall
                    elide: Text.ElideRight
                }
            }

            Switch {
                checked: root.filtered
                enabled: root.host.length > 0
                onToggled: root.setFiltered(checked)
            }
        }

        Divider { theme: root.theme; visible: root.showsPermissions }

        // ── Permissions ────────────────────────────────────────────────
        Label {
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.topMargin: 12
            visible: root.showsPermissions
            text: root.heading
            color: root.theme.textDim
            font.family: root.theme.fontMain
            font.pixelSize: root.theme.fontSmall
            font.weight: Font.DemiBold
        }

        Label {
            Layout.fillWidth: true
            Layout.margins: 14
            Layout.topMargin: 6
            visible: root.showsPermissions && root.shownPermissions.length === 0
            text: root.mode === "permissions"
                  ? "This site has not asked for anything."
                  : "This site has not asked for the " + root.heading.toLowerCase() + "."
            color: root.theme.textFaint
            font.family: root.theme.fontMain
            font.pixelSize: root.theme.fontSmall
            wrapMode: Text.Wrap
        }

        Repeater {
            model: root.showsPermissions ? root.shownPermissions : []

            delegate: RowLayout {
                id: permissionRow
                required property var modelData

                Layout.fillWidth: true
                Layout.leftMargin: 14
                Layout.rightMargin: 14
                Layout.topMargin: 6
                spacing: 8

                Label {
                    Layout.fillWidth: true
                    text: root.permissionLabel(permissionRow.modelData.permissionType)
                    color: root.theme.text
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontSmall
                    elide: Text.ElideRight
                }

                Label {
                    text: root.stateLabel(permissionRow.modelData.state)
                    color: permissionRow.modelData.state === 2
                           ? root.theme.accent : root.theme.textDim
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontSmall
                }

                PanelButton {
                    theme: root.theme
                    // Granting is the useful direction: a request the page made
                    // was answered with a refusal, and this is where it is
                    // reconsidered.
                    label: permissionRow.modelData.state === 2 ? "Block" : "Allow"
                    onClicked: {
                        if (permissionRow.modelData.state === 2)
                            permissionRow.modelData.deny();
                        else
                            permissionRow.modelData.grant();
                        root.permissions = root.readPermissions();
                    }
                }
            }
        }

        Item { Layout.preferredHeight: 8 }

        Divider { theme: root.theme; visible: root.showsStorage }

        // ── Cookies ────────────────────────────────────────────────────
        Label {
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.topMargin: 12
            visible: root.showsStorage
            text: "Cookies and cache"
            color: root.theme.textDim
            font.family: root.theme.fontMain
            font.pixelSize: root.theme.fontSmall
            font.weight: Font.DemiBold
        }

        Label {
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.topMargin: 4
            visible: root.showsStorage
            text: root.cookieCount === 0
                  ? "Nothing stored for this site."
                  : root.cookieCount + (root.cookieCount === 1 ? " cookie" : " cookies")
                    + " stored, including its subdomains."
            color: root.theme.textFaint
            font.family: root.theme.fontMain
            font.pixelSize: root.theme.fontSmall
            wrapMode: Text.Wrap
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            Layout.topMargin: 10
            spacing: 8
            visible: root.showsStorage

            PanelButton {
                theme: root.theme
                label: "Delete for this site"
                enabled: root.cookieCount > 0
                onClicked: {
                    cookieStore.deleteForHost(root.host);
                    root.refresh();
                }
            }

            PanelButton {
                theme: root.theme
                // Qt exposes no per-site variant, so this is the whole cache.
                // Labelled for what it does rather than what the section
                // implies, since the two differ here.
                label: root.clearedCache ? "Cache cleared" : "Clear all cache"
                onClicked: {
                    if (root.browserProfile)
                        root.browserProfile.clearHttpCache();
                    root.clearedCache = true;
                }
            }

            Item { Layout.fillWidth: true }

            PanelButton {
                theme: root.theme
                // Two-step, because this signs the user out of everything they
                // are logged into and there is no undo.
                label: root.confirmingAll ? "Really delete all?" : "Delete all"
                danger: true
                onClicked: {
                    if (!root.confirmingAll) {
                        root.confirmingAll = true;
                        return;
                    }
                    cookieStore.deleteAll();
                    root.refresh();
                }
            }
        }

    }

    component Divider: Rectangle {
        required property var theme
        Layout.fillWidth: true
        Layout.preferredHeight: 1
        color: theme.border
    }

    component PanelButton: Rectangle {
        id: button
        required property var theme
        property string label: ""
        property bool danger: false
        signal clicked()

        implicitWidth: buttonText.implicitWidth + 20
        implicitHeight: 28
        radius: button.theme.roundingSmall
        opacity: button.enabled ? 1 : 0.4
        color: buttonHover.hovered && button.enabled
               ? button.theme.surfaceActive : button.theme.surface
        border.color: button.theme.border
        border.width: 1

        HoverHandler { id: buttonHover; enabled: button.enabled }
        TapHandler {
            enabled: button.enabled
            onTapped: button.clicked()
        }

        Label {
            id: buttonText
            anchors.centerIn: parent
            text: button.label
            color: button.danger ? button.theme.error : button.theme.text
            font.family: button.theme.fontMain
            font.pixelSize: button.theme.fontSmall
        }
    }
}
