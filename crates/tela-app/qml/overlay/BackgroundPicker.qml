import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

import Tela

pragma ComponentBehavior: Bound

// The live backdrops, each previewed by the thing itself rather than by a
// screenshot of it — a still image of an animation tells you nothing.
Popup {
    id: root

    required property var theme

    signal picked(string style)

    padding: 0
    modal: true
    dim: false
    focus: true
    anchors.centerIn: Overlay.overlay
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    implicitWidth: 520
    implicitHeight: body.implicitHeight

    readonly property var styles: [
        { id: "aurora", name: "Aurora",  note: "Drifting colour fields" },
        { id: "mesh",   name: "Mesh",    note: "A slowly turning gradient" },
        { id: "stars",  name: "Stars",   note: "Sparse drifting points" },
        { id: "none",   name: "None",    note: "Flat surface colour" }
    ]

    background: Rectangle {
        color: root.theme.surfaceHigh
        radius: root.theme.rounding
        border.color: root.theme.border
        border.width: 1
    }

    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: root.theme.durFast }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: root.theme.durFast }
    }

    ColumnLayout {
        id: body
        width: parent.width
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            spacing: 8

            Label {
                text: "wallpaper"
                color: root.theme.accent
                font.family: root.theme.fontIcon
                font.pixelSize: root.theme.iconSize
            }
            Label {
                Layout.fillWidth: true
                text: "Start page background"
                color: root.theme.text
                font.family: root.theme.fontMain
                font.pixelSize: root.theme.fontNormal
                font.weight: Font.DemiBold
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.border
        }

        GridLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            columns: 2
            columnSpacing: 12
            rowSpacing: 12

            Repeater {
                model: root.styles

                delegate: Rectangle {
                    id: card

                    required property var modelData

                    readonly property bool current:
                        root.theme.config.startBackground === card.modelData.id

                    Layout.fillWidth: true
                    Layout.preferredHeight: 132
                    radius: root.theme.roundingSmall
                    color: "transparent"
                    border.color: card.current ? root.theme.accent : root.theme.border
                    border.width: card.current ? 2 : root.theme.borderWidth
                    clip: true

                    LiveBackground {
                        anchors.fill: parent
                        anchors.margins: 2
                        theme: root.theme
                        style: card.modelData.id
                        // Previews run only while the picker is open.
                        active: root.visible
                    }

                    Rectangle {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        height: 44
                        color: Qt.rgba(0, 0, 0, 0.45)

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            spacing: 0

                            Label {
                                Layout.fillWidth: true
                                Layout.topMargin: 6
                                text: card.modelData.name
                                color: "white"
                                font.family: root.theme.fontMain
                                font.pixelSize: root.theme.fontSmall
                                font.weight: Font.DemiBold
                            }
                            Label {
                                Layout.fillWidth: true
                                text: card.modelData.note
                                color: "#ccffffff"
                                font.family: root.theme.fontMain
                                font.pixelSize: root.theme.fontSmallest
                                elide: Text.ElideRight
                            }
                        }
                    }

                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    TapHandler {
                        onTapped: {
                            root.picked(card.modelData.id);
                            root.close();
                        }
                    }
                }
            }
        }
    }
}
