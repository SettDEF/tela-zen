import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

pragma ComponentBehavior: Bound

// The extensions list, in the shape Firefox uses: icon, name, what it can
// reach, and a control per row.
//
// The site-access line is the point of this panel. A list that shows only names
// tells you nothing about what you have granted, which is the one thing worth
// glancing at. Everything else here is in service of scanning that quickly:
// enabled first, a filter once the list outgrows a glance, and a toggle in
// reach rather than buried behind a gear.
Popup {
    id: root

    required property var theme
    required property var extensions

    signal manageRequested()
    signal configureRequested(string id)
    signal actionRequested(string id, string popup, string name)

    padding: 0
    modal: true
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    implicitWidth: 400
    implicitHeight: Math.min(600, body.implicitHeight)

    property string filter: ""

    readonly property var allBundles: {
        try {
            return JSON.parse(root.extensions.bundlesJson);
        } catch (error) {
            return [];
        }
    }

    // Enabled first, then alphabetical — but settled when the panel opens, not
    // continuously. Sorting live meant a row jumped out from under the cursor
    // the instant you toggled it, which reads as the click having gone
    // somewhere else. The new order is applied next time the panel opens.
    property var rowOrder: []

    function currentOrder() {
        return root.allBundles
            .slice()
            .sort((a, b) => {
                if (a.enabled !== b.enabled)
                    return a.enabled ? -1 : 1;
                return (a.name || "").localeCompare(b.name || "");
            })
            .map(b => b.id);
    }

    readonly property var bundles: {
        const needle = root.filter.toLowerCase();
        const order = root.rowOrder;
        // An extension installed while the panel is open has no place yet, so
        // it goes to the end rather than disappearing.
        const rank = id => {
            const at = order.indexOf(id);
            return at === -1 ? order.length : at;
        };
        return root.allBundles
            .filter(b => needle.length === 0
                      || (b.name || "").toLowerCase().includes(needle))
            .slice()
            .sort((a, b) => rank(a.id) - rank(b.id));
    }

    readonly property int enabledCount: root.allBundles.filter(b => b.enabled).length

    onOpened: {
        root.filter = "";
        root.rowOrder = root.currentOrder();
    }

    background: Rectangle {
        color: root.theme.surfaceHigh
        radius: root.theme.rounding
        border.color: root.theme.border
        border.width: 1
    }

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: root.theme.durFast }
            NumberAnimation {
                property: "scale"; from: 0.97; to: 1.0
                duration: root.theme.durFast
                easing.type: Easing.BezierSpline
                easing.bezierCurve: root.theme.curveEffects
            }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: root.theme.durFast }
    }

    ColumnLayout {
        id: body
        width: parent.width
        spacing: 0

        // ── Header ─────────────────────────────────────────────────────
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            Layout.bottomMargin: 8
            spacing: 8

            Label {
                text: "extension"
                color: root.theme.accent
                font.family: root.theme.fontIcon
                font.pixelSize: root.theme.iconSize
            }

            Label {
                Layout.fillWidth: true
                text: "Extensions"
                color: root.theme.text
                font.family: root.theme.fontMain
                font.pixelSize: root.theme.fontNormal
                font.weight: Font.DemiBold
            }

            Label {
                text: root.enabledCount + " of " + root.allBundles.length + " on"
                color: root.theme.textFaint
                font.family: root.theme.fontMain
                font.pixelSize: root.theme.fontSmallest
            }
        }

        // ── Filter ─────────────────────────────────────────────────────
        // Only once the list is long enough that scanning it stops working.
        Rectangle {
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.bottomMargin: 10
            implicitHeight: 30
            visible: root.allBundles.length > 6
            radius: root.theme.roundingSmall
            color: root.theme.surface

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 10
                anchors.rightMargin: 8
                spacing: 8

                Label {
                    text: "search"
                    color: root.theme.textFaint
                    font.family: root.theme.fontIcon
                    font.pixelSize: root.theme.iconSizeSmall
                }

                TextField {
                    Layout.fillWidth: true
                    placeholderText: "Filter"
                    color: root.theme.text
                    placeholderTextColor: root.theme.textFaint
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontSmall
                    background: null
                    onTextEdited: root.filter = text
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.border
        }

        // ── Empty state ────────────────────────────────────────────────
        ColumnLayout {
            Layout.fillWidth: true
            Layout.topMargin: 30
            Layout.bottomMargin: 30
            visible: root.bundles.length === 0
            spacing: 8

            Label {
                Layout.alignment: Qt.AlignHCenter
                text: root.allBundles.length === 0 ? "extension_off" : "search_off"
                color: root.theme.textFaint
                font.family: root.theme.fontIcon
                font.pixelSize: 30
            }
            Label {
                Layout.alignment: Qt.AlignHCenter
                text: root.allBundles.length === 0
                      ? "No extensions installed" : "Nothing matches that"
                color: root.theme.textDim
                font.family: root.theme.fontMain
                font.pixelSize: root.theme.fontSmall
            }
        }

        // ── List ───────────────────────────────────────────────────────
        ListView {
            id: list

            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, 430)
            visible: root.bundles.length > 0
            clip: true
            model: root.bundles
            boundsBehavior: Flickable.StopAtBounds

            ScrollBar.vertical: ScrollBar {
                id: scroller
                policy: list.contentHeight > list.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff
                width: 4
                contentItem: Rectangle {
                    radius: 2
                    color: root.theme.textFaint
                    opacity: scroller.pressed ? 0.7 : 0.3
                }
            }

            delegate: Rectangle {
                id: row

                required property var modelData

                readonly property bool on: row.modelData.enabled === true
                readonly property var issues: row.modelData.issues ?? []

                width: ListView.view.width
                height: 60
                color: rowHover.hovered ? root.theme.surfaceHover : "transparent"

                Behavior on color { ColorAnimation { duration: root.theme.durFast } }

                HoverHandler { id: rowHover }
                TapHandler {
                    // Clicking the row opens the extension's own popup, which
                    // is what the icon means in every other browser.
                    enabled: row.on && (row.modelData.popup ?? "").length > 0
                    onTapped: root.actionRequested(row.modelData.id,
                                                   row.modelData.popup,
                                                   row.modelData.name)
                }

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 14
                    anchors.rightMargin: 10
                    spacing: 12

                    // Icon, with a lettered fallback when the bundle ships none
                    // we could resolve on disk.
                    Item {
                        Layout.preferredWidth: 30
                        Layout.preferredHeight: 30
                        Layout.alignment: Qt.AlignVCenter
                        opacity: row.on ? 1 : 0.4

                        Image {
                            id: icon
                            anchors.fill: parent
                            source: row.modelData.icon ?? ""
                            visible: status === Image.Ready
                            sourceSize.width: 64
                            sourceSize.height: 64
                            smooth: true
                        }

                        Rectangle {
                            anchors.fill: parent
                            visible: !icon.visible
                            radius: 8
                            color: root.theme.accentContainer

                            Label {
                                anchors.centerIn: parent
                                text: (row.modelData.name || "?").charAt(0).toUpperCase()
                                color: root.theme.onAccentContainer
                                font.family: root.theme.fontMain
                                font.pixelSize: root.theme.fontSmall
                                font.weight: Font.DemiBold
                            }
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 1

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 6

                            Label {
                                Layout.fillWidth: true
                                text: row.modelData.name ?? ""
                                color: row.on ? root.theme.text : root.theme.textFaint
                                font.family: root.theme.fontMain
                                font.pixelSize: root.theme.fontSmall
                                elide: Text.ElideRight
                            }

                            Label {
                                visible: row.issues.length > 0
                                text: "warning"
                                color: root.theme.tertiary
                                font.family: root.theme.fontIcon
                                font.pixelSize: root.theme.iconSizeSmall

                                HoverHandler { id: warnHover }
                                ToolTip.visible: warnHover.hovered
                                ToolTip.delay: 300
                                ToolTip.text: row.issues.map(i => i.feature).join(", ")
                                              + " not supported here"
                            }
                        }

                        Label {
                            Layout.fillWidth: true
                            text: !row.modelData.loadable ? "Cannot run here"
                                : !row.on ? "Disabled"
                                : rowHover.hovered && (row.modelData.popup ?? "").length > 0
                                  ? "Open extension"
                                  : (row.modelData.hostAccess ?? "")
                            color: !row.modelData.loadable ? root.theme.error : root.theme.textDim
                            font.family: root.theme.fontMain
                            font.pixelSize: root.theme.fontSmallest
                            elide: Text.ElideRight
                        }
                    }

                    // Enable/disable inline — the action people come here for.
                    Rectangle {
                        Layout.alignment: Qt.AlignVCenter
                        implicitWidth: 40
                        implicitHeight: 22
                        radius: 11
                        color: row.on ? root.theme.accent : root.theme.surfaceActive
                        opacity: row.modelData.loadable ? 1 : 0.35

                        Behavior on color { ColorAnimation { duration: root.theme.durFast } }

                        TapHandler {
                            enabled: row.modelData.loadable
                            onTapped: root.extensions.setEnabled(row.modelData.id, !row.on)
                        }

                        Rectangle {
                            y: 3
                            x: row.on ? parent.width - width - 3 : 3
                            width: 16; height: 16; radius: 8
                            color: row.on ? root.theme.onAccent : root.theme.textDim

                            Behavior on x {
                                NumberAnimation {
                                    duration: root.theme.durFast
                                    easing.type: Easing.BezierSpline
                                    easing.bezierCurve: root.theme.curveSpatial
                                }
                            }
                        }
                    }

                    Rectangle {
                        Layout.preferredWidth: 26
                        Layout.preferredHeight: 26
                        Layout.alignment: Qt.AlignVCenter
                        radius: root.theme.roundingSmall
                        color: gearHover.hovered ? root.theme.surfaceActive : "transparent"

                        HoverHandler { id: gearHover }
                        TapHandler { onTapped: root.configureRequested(row.modelData.id) }

                        ToolTip.visible: gearHover.hovered
                        ToolTip.text: "Details"
                        ToolTip.delay: 500

                        Label {
                            anchors.centerIn: parent
                            text: "settings"
                            color: root.theme.textDim
                            font.family: root.theme.fontIcon
                            font.pixelSize: root.theme.iconSizeSmall
                        }
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.border
        }

        // ── Footer ─────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 42
            color: manageHover.hovered ? root.theme.surfaceHover : "transparent"
            bottomLeftRadius: root.theme.rounding
            bottomRightRadius: root.theme.rounding

            Behavior on color { ColorAnimation { duration: root.theme.durFast } }

            HoverHandler { id: manageHover }
            TapHandler {
                onTapped: {
                    root.close();
                    root.manageRequested();
                }
            }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 14
                spacing: 10

                Label {
                    text: "tune"
                    color: root.theme.textDim
                    font.family: root.theme.fontIcon
                    font.pixelSize: root.theme.iconSizeSmall
                }
                Label {
                    Layout.fillWidth: true
                    text: "Manage extensions"
                    color: root.theme.text
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontSmall
                }
                Label {
                    text: "chevron_right"
                    color: root.theme.textFaint
                    font.family: root.theme.fontIcon
                    font.pixelSize: root.theme.iconSizeSmall
                }
            }
        }
    }
}
