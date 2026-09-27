import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

pragma ComponentBehavior: Bound

// The whole browsing history, searchable. A plain Popup — no web view — so the
// compositor-hole rule does not apply. Search runs against the FTS index in the
// History store; picking a row hands the URL back to the window, deleting a row
// forgets it. It chooses and reports, like the command palette: what a result
// does belongs to the window.
Popup {
    id: root

    required property var theme
    required property var history

    signal urlSelected(string url)

    anchors.centerIn: Overlay.overlay
    width: 720
    height: Math.min(Overlay.overlay.height - 120, 680)
    padding: 0
    modal: true
    dim: true
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    property var results: []

    function refresh() {
        try {
            root.results = JSON.parse(root.history.search(searchField.text, 300));
        } catch (error) {
            root.results = [];
        }
    }

    function open() {
        // Popup.open is shadowed by this; call the base explicitly.
        visible = true;
        searchField.text = "";
        root.refresh();
        searchField.forceActiveFocus();
    }

    // A relative-ish, human timestamp.
    function when(unixSeconds) {
        const then = new Date(unixSeconds * 1000);
        const now = new Date();
        const sameDay = then.toDateString() === now.toDateString();
        if (sameDay)
            return then.toLocaleTimeString(Qt.locale(), Locale.ShortFormat);
        return then.toLocaleDateString(Qt.locale(), Locale.ShortFormat);
    }

    background: Rectangle {
        color: root.theme.surfaceHigh
        radius: root.theme.roundingLarge
        border.color: root.theme.border
        border.width: 1
    }

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: root.theme.durFast }
            NumberAnimation {
                property: "scale"; from: 0.97; to: 1.0; duration: root.theme.durNormal
                easing.type: Easing.BezierSpline; easing.bezierCurve: root.theme.curveSpatial
            }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: root.theme.durFast }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ── Header: title, search, clear-all ──────────────────────────────
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 18
            Layout.bottomMargin: 12
            spacing: 12

            Label {
                text: "history"
                color: root.theme.accent
                font.family: root.theme.fontIcon
                font.pixelSize: root.theme.iconSize
            }
            Label {
                text: "History"
                color: root.theme.text
                font.family: root.theme.fontMain
                font.pixelSize: root.theme.fontLarge
                font.weight: Font.DemiBold
            }
            Item { Layout.fillWidth: true }
            Rectangle {
                Layout.preferredHeight: 30
                implicitWidth: clearLabel.implicitWidth + 22
                radius: root.theme.roundingSmall
                color: clearHover.hovered ? root.theme.surfaceActive : root.theme.surface
                border.color: root.theme.border
                border.width: 1
                visible: root.results.length > 0

                HoverHandler { id: clearHover }
                TapHandler {
                    onTapped: {
                        root.history.clear();
                        root.refresh();
                    }
                }
                Label {
                    id: clearLabel
                    anchors.centerIn: parent
                    text: "Clear all"
                    color: root.theme.text
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontSmall
                }
            }
        }

        TextField {
            id: searchField
            Layout.fillWidth: true
            Layout.leftMargin: 18
            Layout.rightMargin: 18
            placeholderText: "Search history…"
            color: root.theme.text
            placeholderTextColor: root.theme.textFaint
            font.family: root.theme.fontMain
            font.pixelSize: root.theme.fontNormal
            leftPadding: 14
            rightPadding: 14
            topPadding: 10
            bottomPadding: 10
            background: Rectangle {
                color: root.theme.surface
                radius: root.theme.roundingSmall
                border.color: searchField.activeFocus ? root.theme.accent : root.theme.border
                border.width: 1
            }
            onTextEdited: root.refresh()
            Keys.onEscapePressed: root.close()
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.topMargin: 12
            height: 1
            color: root.theme.border
        }

        Label {
            visible: root.results.length === 0
            Layout.fillWidth: true
            Layout.margins: 40
            horizontalAlignment: Text.AlignHCenter
            text: searchField.text.length > 0 ? "No matches" : "No history yet"
            color: root.theme.textFaint
            font.family: root.theme.fontMain
            font.pixelSize: root.theme.fontSmall
        }

        // ── Results ───────────────────────────────────────────────────────
        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.margins: 8
            visible: root.results.length > 0
            clip: true
            model: root.results
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }

            delegate: Rectangle {
                id: row
                required property int index
                required property var modelData

                width: ListView.view.width
                height: 52
                radius: root.theme.rounding
                color: rowHover.hovered ? root.theme.surfaceHover : "transparent"

                HoverHandler { id: rowHover }
                TapHandler {
                    onTapped: {
                        root.urlSelected(row.modelData.url);
                        root.close();
                    }
                }

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 8
                    spacing: 12

                    Label {
                        text: "history"
                        color: root.theme.textFaint
                        font.family: root.theme.fontIcon
                        font.pixelSize: 16
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 1
                        Label {
                            Layout.fillWidth: true
                            text: row.modelData.title || row.modelData.url
                            color: root.theme.text
                            font.family: root.theme.fontMain
                            font.pixelSize: root.theme.fontSmall
                            elide: Text.ElideRight
                        }
                        Label {
                            Layout.fillWidth: true
                            text: row.modelData.url
                            color: root.theme.textFaint
                            font.family: root.theme.fontMain
                            font.pixelSize: root.theme.fontSmallest
                            elide: Text.ElideRight
                        }
                    }

                    Label {
                        text: root.when(row.modelData.lastVisit)
                        color: root.theme.textFaint
                        font.family: root.theme.fontMain
                        font.pixelSize: root.theme.fontSmallest
                    }

                    // Forget this entry.
                    Rectangle {
                        Layout.preferredWidth: 24
                        Layout.preferredHeight: 24
                        radius: 6
                        color: delHover.hovered ? root.theme.surfaceActive : "transparent"
                        opacity: rowHover.hovered ? 1 : 0

                        HoverHandler { id: delHover }
                        TapHandler {
                            onTapped: {
                                root.history.remove(row.modelData.url);
                                root.refresh();
                            }
                        }
                        Label {
                            anchors.centerIn: parent
                            text: "close"
                            color: root.theme.textDim
                            font.family: root.theme.fontIcon
                            font.pixelSize: 15
                        }
                    }
                }
            }
        }
    }
}
