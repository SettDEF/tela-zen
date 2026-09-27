import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

pragma ComponentBehavior: Bound

// Themed context menu.
//
// Built on Popup rather than Controls' Menu so it inherits the same surface,
// radius, motion and icon font as the rest of the chrome — a stock Menu would
// be the one piece of UI that looked like a different application.
//
// Layout is a row of columns, not one long list: a browser context menu has
// more entries than fit comfortably in a single column, and splitting them by
// topic reads faster than scrolling. An optional icon row sits on top for the
// actions used constantly.
//
//   quickActions: [ { id: "back", glyph: "arrow_back", tip: "Back" } ]
//   columns: [
//     { title: "Page",  entries: [ { id, label, glyph, shortcut, enabled } ] },
//     { title: "Link",  entries: [ ... ] }
//   ]
Popup {
    id: root

    required property var theme

    property var quickActions: []
    property var columns: []

    signal triggered(string id)

    padding: 8
    modal: true
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    // Popup sizes contentItem for us, so the columns get a real width. Doing
    // this by hand is how the rows previously collapsed to zero and stacked
    // every icon on top of its label.
    implicitWidth: Math.max(240, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: implicitContentHeight + topPadding + bottomPadding

    function open2(x, y, quick, groups) {
        quickActions = quick ?? [];
        columns = (groups ?? []).filter(group => group && group.entries && group.entries.length > 0);

        // Keep the menu on screen: flip rather than clip near an edge.
        const host = root.parent;
        const w = root.implicitWidth;
        const h = root.implicitHeight;
        root.x = (host && x + w > host.width) ? Math.max(0, x - w) : x;
        root.y = (host && y + h > host.height) ? Math.max(0, y - h) : y;
        root.open();
    }

    function activate(entry) {
        if (!entry || entry.enabled === false)
            return;
        root.close();
        root.triggered(entry.id);
    }

    background: Rectangle {
        color: root.theme.surfaceHigh
        radius: root.theme.roundingSmall + 6
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

    Keys.onEscapePressed: root.close()

    contentItem: ColumnLayout {
        spacing: 0

        // ── Quick action row ───────────────────────────────────────────
        RowLayout {
            Layout.fillWidth: true
            Layout.bottomMargin: root.quickActions.length > 0 ? 6 : 0
            visible: root.quickActions.length > 0
            spacing: 2

            Repeater {
                model: root.quickActions

                delegate: Rectangle {
                    id: quick

                    required property var modelData

                    readonly property bool isEnabled: modelData.enabled !== false

                    Layout.fillWidth: true
                    Layout.preferredHeight: 34
                    radius: root.theme.roundingSmall
                    color: quickHover.hovered && isEnabled ? root.theme.surfaceHover : "transparent"
                    opacity: isEnabled ? 1 : 0.35

                    Behavior on color { ColorAnimation { duration: root.theme.durFast } }

                    HoverHandler { id: quickHover; enabled: quick.isEnabled }
                    TapHandler {
                        enabled: quick.isEnabled
                        onTapped: root.activate(quick.modelData)
                    }

                    ToolTip.visible: quickHover.hovered && (quick.modelData.tip ?? "").length > 0
                    ToolTip.text: quick.modelData.tip ?? ""
                    ToolTip.delay: 400

                    Label {
                        anchors.centerIn: parent
                        text: quick.modelData.glyph ?? ""
                        color: root.theme.text
                        font.family: root.theme.fontIcon
                        font.pixelSize: root.theme.iconSize
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            Layout.bottomMargin: 6
            visible: root.quickActions.length > 0 && root.columns.length > 0
            color: root.theme.border
        }

        // ── Columns ────────────────────────────────────────────────────
        RowLayout {
            Layout.fillWidth: true
            spacing: 6

            Repeater {
                model: root.columns

                delegate: ColumnLayout {
                    id: group

                    required property int index
                    required property var modelData

                    Layout.alignment: Qt.AlignTop
                    Layout.minimumWidth: 190
                    spacing: 0

                    Label {
                        Layout.fillWidth: true
                        Layout.leftMargin: 10
                        Layout.bottomMargin: 4
                        visible: (group.modelData.title ?? "").length > 0
                        text: (group.modelData.title ?? "").toUpperCase()
                        color: root.theme.textFaint
                        font.family: root.theme.fontMain
                        font.pixelSize: root.theme.fontSmallest
                        font.weight: Font.DemiBold
                        font.letterSpacing: 0.9
                    }

                    Repeater {
                        model: group.modelData.entries

                        delegate: Item {
                            id: row

                            required property var modelData

                            readonly property bool isSeparator: modelData.separator === true
                            readonly property bool isEnabled: !isSeparator && modelData.enabled !== false

                            Layout.fillWidth: true
                            Layout.preferredHeight: isSeparator ? 9 : 30

                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.leftMargin: 8
                                anchors.rightMargin: 8
                                height: 1
                                visible: row.isSeparator
                                color: root.theme.border
                            }

                            Rectangle {
                                anchors.fill: parent
                                visible: !row.isSeparator
                                radius: root.theme.roundingSmall
                                color: rowHover.hovered && row.isEnabled
                                       ? root.theme.surfaceHover : "transparent"
                                opacity: row.isEnabled ? 1 : 0.4

                                Behavior on color { ColorAnimation { duration: root.theme.durFast } }

                                HoverHandler { id: rowHover; enabled: row.isEnabled }
                                TapHandler {
                                    enabled: row.isEnabled
                                    onTapped: root.activate(row.modelData)
                                }

                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: 10
                                    anchors.rightMargin: 10
                                    spacing: 10

                                    Label {
                                        Layout.preferredWidth: root.theme.iconSize
                                        horizontalAlignment: Text.AlignHCenter
                                        text: row.modelData.glyph ?? ""
                                        color: root.theme.textDim
                                        font.family: root.theme.fontIcon
                                        font.pixelSize: root.theme.iconSizeSmall
                                    }

                                    Label {
                                        Layout.fillWidth: true
                                        text: row.modelData.label ?? ""
                                        color: root.theme.text
                                        font.family: root.theme.fontMain
                                        font.pixelSize: root.theme.fontSmall
                                        elide: Text.ElideRight
                                    }

                                    Label {
                                        visible: (row.modelData.shortcut ?? "").length > 0
                                        text: row.modelData.shortcut ?? ""
                                        color: root.theme.textFaint
                                        font.family: root.theme.fontMono
                                        font.pixelSize: root.theme.fontSmallest
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
