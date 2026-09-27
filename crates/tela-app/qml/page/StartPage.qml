import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

pragma ComponentBehavior: Bound

// Start page, laid out like an Android home screen: a clock block, a search
// pill, a grid of shortcut tiles, and a widget row beneath.
//
// Rendered in QML rather than as an HTML page so it inherits the theme, costs
// no web engine process, and can reach app state directly.
Item {
    id: root

    required property var theme

    signal navigate(string url)
    signal paletteRequested()
    signal customiseRequested()

    property var shortcuts: {
        try {
            return JSON.parse(root.theme.config.startShortcutsJson);
        } catch (e) {
            return [];
        }
    }

    LiveBackground {
        anchors.fill: parent
        theme: root.theme
        style: root.theme.config.startBackground
        intensity: root.theme.config.startBackgroundIntensity
        // Nothing animates behind a page you cannot see.
        active: root.visible
    }

    Flickable {
        anchors.fill: parent
        contentHeight: column.implicitHeight + 96
        clip: true

        ColumnLayout {
            id: column

            width: Math.min(parent.width - 64, 900)
            x: (parent.width - width) / 2
            y: 64
            spacing: 0

            // ── Clock ──────────────────────────────────────────────────
            Label {
                Layout.alignment: Qt.AlignHCenter
                text: clock.time
                color: root.theme.text
                font.family: root.theme.fontMain
                font.pixelSize: 64
                font.weight: Font.Light
            }

            Label {
                Layout.alignment: Qt.AlignHCenter
                Layout.bottomMargin: 36
                text: root.theme.config.startGreeting.length > 0
                      ? root.theme.config.startGreeting
                      : clock.date
                color: root.theme.textDim
                font.family: root.theme.fontMain
                font.pixelSize: root.theme.fontNormal
            }

            // ── Search pill ────────────────────────────────────────────
            Rectangle {
                Layout.fillWidth: true
                Layout.maximumWidth: 560
                Layout.alignment: Qt.AlignHCenter
                Layout.bottomMargin: 44
                implicitHeight: 52
                radius: height / 2
                color: searchHover.hovered ? root.theme.surfaceHover : root.theme.surfaceHigh
                border.color: root.theme.border
                border.width: root.theme.borderWidth

                Behavior on color { ColorAnimation { duration: root.theme.durFast } }

                HoverHandler { id: searchHover }
                TapHandler { onTapped: root.paletteRequested() }

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 20
                    anchors.rightMargin: 20
                    spacing: 12

                    Label {
                        text: "search"
                        color: root.theme.textDim
                        font.family: root.theme.fontIcon
                        font.pixelSize: root.theme.iconSize
                    }
                    Label {
                        Layout.fillWidth: true
                        text: "Search or enter address"
                        color: root.theme.textDim
                        font.family: root.theme.fontMain
                        font.pixelSize: root.theme.fontNormal
                    }
                    Label {
                        text: "Ctrl K"
                        color: root.theme.textFaint
                        font.family: root.theme.fontMono
                        font.pixelSize: root.theme.fontSmallest
                    }
                }
            }

            // ── Shortcut grid ──────────────────────────────────────────
            GridLayout {
                Layout.fillWidth: true
                Layout.bottomMargin: 44
                columns: root.theme.config.startColumns
                columnSpacing: 8
                rowSpacing: 16

                Repeater {
                    model: root.shortcuts

                    delegate: Item {
                        id: tile

                        required property var modelData

                        Layout.fillWidth: true
                        Layout.preferredHeight: 96

                        ColumnLayout {
                            anchors.centerIn: parent
                            spacing: 8

                            Rectangle {
                                Layout.alignment: Qt.AlignHCenter
                                width: 56; height: 56
                                radius: 18
                                color: tileHover.hovered ? root.theme.accentContainer
                                                         : root.theme.surfaceHigh
                                scale: tileHover.hovered ? 1.06 : 1.0

                                Behavior on color { ColorAnimation { duration: root.theme.durFast } }
                                Behavior on scale {
                                    NumberAnimation {
                                        duration: root.theme.durNormal
                                        easing.type: Easing.BezierSpline
                                        easing.bezierCurve: root.theme.curveSpatial
                                    }
                                }

                                Label {
                                    anchors.centerIn: parent
                                    text: tile.modelData.glyph ?? "public"
                                    color: tileHover.hovered ? root.theme.onAccentContainer
                                                             : root.theme.textDim
                                    font.family: root.theme.fontIcon
                                    font.pixelSize: 26
                                }
                            }

                            Label {
                                Layout.alignment: Qt.AlignHCenter
                                Layout.maximumWidth: 88
                                text: tile.modelData.title ?? ""
                                color: root.theme.textDim
                                font.family: root.theme.fontMain
                                font.pixelSize: root.theme.fontSmall
                                elide: Text.ElideRight
                                horizontalAlignment: Text.AlignHCenter
                            }
                        }

                        HoverHandler { id: tileHover }
                        TapHandler { onTapped: root.navigate(tile.modelData.url) }
                    }
                }
            }

            // ── News widget ────────────────────────────────────────────
            // Placeholder until a feed source is chosen. Kept as a real slot
            // rather than omitted, so wiring a source is a data change.
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 120
                radius: root.theme.rounding
                color: root.theme.surfaceHigh
                border.color: root.theme.border
                border.width: root.theme.borderWidth

                ColumnLayout {
                    anchors.centerIn: parent
                    spacing: 6

                    Label {
                        Layout.alignment: Qt.AlignHCenter
                        text: "newsmode"
                        color: root.theme.textFaint
                        font.family: root.theme.fontIcon
                        font.pixelSize: 26
                    }
                    Label {
                        Layout.alignment: Qt.AlignHCenter
                        text: "No feed configured"
                        color: root.theme.textDim
                        font.family: root.theme.fontMain
                        font.pixelSize: root.theme.fontSmall
                    }
                }
            }
        }
    }

    // Quiet corner affordance: visible on hover, out of the way otherwise.
    Rectangle {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 18
        implicitWidth: 34
        implicitHeight: 34
        radius: 17
        color: customiseHover.hovered ? root.theme.surfaceHigh : "transparent"
        border.color: root.theme.border
        border.width: customiseHover.hovered ? root.theme.borderWidth : 0
        opacity: customiseHover.hovered ? 1 : 0.45

        Behavior on opacity { NumberAnimation { duration: root.theme.durFast } }
        Behavior on color { ColorAnimation { duration: root.theme.durFast } }

        HoverHandler { id: customiseHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.customiseRequested() }

        ToolTip.visible: customiseHover.hovered
        ToolTip.text: "Background"
        ToolTip.delay: 400

        Label {
            anchors.centerIn: parent
            text: "wallpaper"
            color: root.theme.textDim
            font.family: root.theme.fontIcon
            font.pixelSize: root.theme.iconSize
        }
    }

    // Ticks once a minute rather than once a second: the display has no
    // seconds, so anything faster is wasted wakeups.
    QtObject {
        id: clock
        property string time: Qt.formatTime(new Date(), "HH:mm")
        property string date: Qt.formatDate(new Date(), "dddd, d MMMM")
    }

    Timer {
        interval: 15000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: {
            clock.time = Qt.formatTime(new Date(), "HH:mm");
            clock.date = Qt.formatDate(new Date(), "dddd, d MMMM");
        }
    }
}
