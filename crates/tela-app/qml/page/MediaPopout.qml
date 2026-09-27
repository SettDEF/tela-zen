import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

pragma ComponentBehavior: Bound

// Floating always-on-top window for video and streams.
//
// The tab's live WebEngineView is *reparented* into this window rather than a
// second view being created for the same URL. That keeps the existing renderer,
// so playback continues from where it was instead of restarting — which is the
// whole point of a popout, and what a naive "open the media URL in a new
// window" implementation gets wrong (and which fails outright for embedded
// players, where there is no direct media URL to open).
Window {
    id: root

    required property var theme

    /// The item currently living in this window, or null.
    property var attached: null
    /// Where a re-parented view should be placed. Exposed directly so callers
    /// do not have to walk the child tree to find it.
    property alias hostItem: host
    /// Called when the view should go home; the caller re-parents it back.
    signal returnRequested()

    property bool pinned: true

    width: 480
    height: 270
    minimumWidth: 240
    minimumHeight: 135
    visible: false
    title: "Tela — popout"

    // Frameless so the chrome is ours; transparent so the rounded corners of
    // the panel inside are what the compositor shows.
    flags: Qt.Window | Qt.FramelessWindowHint
           | (pinned ? Qt.WindowStaysOnTopHint : 0)
    color: "transparent"

    Rectangle {
        id: shell

        anchors.fill: parent
        radius: root.theme.rounding
        color: root.theme.surface
        border.color: root.theme.border
        border.width: root.theme.borderWidth
        clip: true

        // Where the reparented view lives. It binds its size to its parent, so
        // moving it here resizes it without touching anchors.
        Item {
            id: host
            objectName: "popoutHost"
            anchors.fill: parent
            anchors.margins: root.theme.borderWidth
        }

        // ── Hover chrome ───────────────────────────────────────────────
        // Hidden until the pointer is over the window, so the video is
        // unobstructed while you are actually watching it.
        HoverHandler { id: windowHover }

        Rectangle {
            id: topBar

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: 36
            opacity: windowHover.hovered ? 1 : 0
            visible: opacity > 0

            gradient: Gradient {
                GradientStop { position: 0.0; color: Qt.rgba(0, 0, 0, 0.75) }
                GradientStop { position: 1.0; color: "transparent" }
            }

            Behavior on opacity {
                NumberAnimation {
                    duration: root.theme.durNormal
                    easing.type: Easing.BezierSpline
                    easing.bezierCurve: root.theme.curveDecel
                }
            }

            // Wayland does not let a client place its own window, so dragging
            // has to be handed to the compositor.
            DragHandler {
                target: null
                onActiveChanged: if (active) root.startSystemMove()
            }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 8
                spacing: 4

                Label {
                    text: "drag_indicator"
                    color: "#ffffff"
                    opacity: 0.7
                    font.family: root.theme.fontIcon
                    font.pixelSize: root.theme.iconSizeSmall
                }

                Label {
                    Layout.fillWidth: true
                    text: root.title
                    color: "#ffffff"
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontSmallest
                    elide: Text.ElideRight
                }

                PopoutButton {
                    theme: root.theme
                    glyph: root.pinned ? "keep" : "keep_off"
                    tip: root.pinned ? "Unpin" : "Keep on top"
                    onTriggered: root.pinned = !root.pinned
                }
                PopoutButton {
                    theme: root.theme
                    glyph: "open_in_full"
                    tip: "Return to tab"
                    onTriggered: root.returnRequested()
                }
            }
        }

        // ── Resize grip ────────────────────────────────────────────────
        Item {
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            width: 20
            height: 20
            opacity: windowHover.hovered ? 0.7 : 0

            Behavior on opacity { NumberAnimation { duration: root.theme.durNormal } }

            DragHandler {
                target: null
                onActiveChanged: if (active) root.startSystemResize(Qt.BottomEdge | Qt.RightEdge)
            }

            Canvas {
                anchors.fill: parent
                onPaint: {
                    const context = getContext("2d");
                    context.reset();
                    context.strokeStyle = "#ffffff";
                    context.lineWidth = 1.5;
                    for (let offset = 6; offset <= 14; offset += 4) {
                        context.beginPath();
                        context.moveTo(width - offset, height - 3);
                        context.lineTo(width - 3, height - offset);
                        context.stroke();
                    }
                }
            }
        }
    }

    /// Size presets, applied around the current centre where possible.
    function setAspect(width, height) {
        root.width = width;
        root.height = height;
    }

    component PopoutButton: Rectangle {
        id: button

        required property var theme
        required property string glyph
        property string tip: ""

        signal triggered()

        implicitWidth: 26
        implicitHeight: 26
        radius: 6
        color: buttonHover.hovered ? Qt.rgba(1, 1, 1, 0.18) : "transparent"

        Behavior on color { ColorAnimation { duration: button.theme.durFast } }

        HoverHandler { id: buttonHover }
        TapHandler { onTapped: button.triggered() }

        ToolTip.visible: buttonHover.hovered && button.tip.length > 0
        ToolTip.text: button.tip
        ToolTip.delay: 500

        Label {
            anchors.centerIn: parent
            text: button.glyph
            color: "#ffffff"
            font.family: button.theme.fontIcon
            font.pixelSize: button.theme.iconSizeSmall
        }
    }
}
