import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtWebEngine

pragma ComponentBehavior: Bound

// The download shelf: what is arriving, how far along, and where it went.
//
// A plain Popup, deliberately — it holds no web view, so it is free of the
// compositor-hole problem that forces the extension-action popup to be a bare
// Item (see CLAUDE.md). The rows are the live WebEngineDownloadRequest objects
// themselves, so progress and state bind straight through with no mirror model
// to keep in sync.
Popup {
    id: root

    required property var theme

    // The live download requests, newest first. Reassigned (not mutated) on
    // add, so the ListView refreshes; each row binds to its object's own
    // reactive properties.
    property var items: []
    // Bumped whenever any download changes state or byte count, so the derived
    // active count re-evaluates — a plain array does not notify on that.
    property int revision: 0

    readonly property int activeCount: {
        root.revision; // establish the dependency
        let n = 0;
        for (const d of root.items)
            if (d && d.state === WebEngineDownloadRequest.DownloadInProgress)
                n++;
        return n;
    }

    // Take a download the profile is offering. Accepts it to the default
    // download directory and starts tracking it.
    function accept(download) {
        download.accept();
        root.items = [download].concat(root.items);
        download.stateChanged.connect(function () { root.revision++; });
        download.receivedBytesChanged.connect(function () { root.revision++; });
        root.revision++;
    }

    function clearFinished() {
        root.items = root.items.filter(function (d) {
            return d && d.state === WebEngineDownloadRequest.DownloadInProgress;
        });
        root.revision++;
    }

    function showNear(item) {
        if (item) {
            const point = item.mapToItem(Overlay.overlay, 0, item.height);
            x = Math.max(8, Math.min(point.x - width + item.width,
                                     Overlay.overlay.width - width - 8));
            y = point.y + 6;
        } else {
            // No anchor button given (keyboard/menu open, or auto-open on a new
            // download): sit at the top-right, by the rail where the button is.
            x = Overlay.overlay.width - width - 56;
            y = 60;
        }
        open();
    }

    function toggleNear(item) {
        if (opened)
            close();
        else
            showNear(item);
    }

    parent: Overlay.overlay
    padding: 0
    modal: true
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    implicitWidth: 380
    implicitHeight: Math.min(520, body.implicitHeight)

    background: Rectangle {
        color: root.theme.surface
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

    // Human-readable byte size.
    function humanSize(bytes) {
        if (bytes <= 0)
            return "0 B";
        const units = ["B", "KB", "MB", "GB"];
        let n = bytes, i = 0;
        while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
        return (i === 0 ? n : n.toFixed(1)) + " " + units[i];
    }

    ColumnLayout {
        id: body
        width: parent.width
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            Layout.bottomMargin: 8

            Label {
                Layout.fillWidth: true
                text: "Downloads"
                color: root.theme.text
                font.family: root.theme.fontMain
                font.pixelSize: root.theme.fontNormal
                font.weight: Font.DemiBold
            }
            Label {
                visible: root.items.length > 0
                text: "Clear"
                color: clearHover.hovered ? root.theme.accent : root.theme.textDim
                font.family: root.theme.fontMain
                font.pixelSize: root.theme.fontSmall
                HoverHandler { id: clearHover }
                TapHandler { onTapped: root.clearFinished() }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: root.theme.border
            visible: root.items.length > 0
        }

        Label {
            visible: root.items.length === 0
            Layout.fillWidth: true
            Layout.margins: 24
            horizontalAlignment: Text.AlignHCenter
            text: "Nothing downloaded yet"
            color: root.theme.textFaint
            font.family: root.theme.fontMain
            font.pixelSize: root.theme.fontSmall
        }

        ListView {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(420, contentHeight)
            visible: root.items.length > 0
            clip: true
            model: root.items
            boundsBehavior: Flickable.StopAtBounds

            delegate: Item {
                id: row
                required property var modelData
                width: ListView.view.width
                height: 60

                readonly property bool done:
                    modelData.state === WebEngineDownloadRequest.DownloadCompleted
                readonly property bool inProgress:
                    modelData.state === WebEngineDownloadRequest.DownloadInProgress
                readonly property bool failed:
                    modelData.state === WebEngineDownloadRequest.DownloadCancelled
                    || modelData.state === WebEngineDownloadRequest.DownloadInterrupted
                readonly property string filePath:
                    modelData.downloadDirectory + "/" + modelData.downloadFileName

                HoverHandler { id: rowHover }
                Rectangle {
                    anchors.fill: parent
                    color: rowHover.hovered ? root.theme.surfaceHover : "transparent"
                }

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 14
                    anchors.rightMargin: 10
                    spacing: 10

                    Label {
                        text: row.done ? "task_alt" : row.failed ? "error" : "download"
                        color: row.done ? root.theme.accent
                             : row.failed ? root.theme.textFaint : root.theme.textDim
                        font.family: root.theme.fontIcon
                        font.pixelSize: 20
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 3

                        Label {
                            Layout.fillWidth: true
                            text: row.modelData.downloadFileName
                            color: root.theme.text
                            font.family: root.theme.fontMain
                            font.pixelSize: root.theme.fontSmall
                            elide: Text.ElideMiddle
                        }

                        // Progress bar while downloading; a size/status line once done.
                        Rectangle {
                            visible: row.inProgress
                            Layout.fillWidth: true
                            height: 4
                            radius: 2
                            color: root.theme.surfaceActive

                            Rectangle {
                                height: parent.height
                                radius: 2
                                color: root.theme.accent
                                width: parent.width * (row.modelData.totalBytes > 0
                                       ? row.modelData.receivedBytes / row.modelData.totalBytes
                                       : 0)
                            }
                        }

                        Label {
                            visible: !row.inProgress
                            text: row.done ? root.humanSize(row.modelData.receivedBytes)
                                 : row.failed ? "Cancelled" : ""
                            color: root.theme.textFaint
                            font.family: root.theme.fontMain
                            font.pixelSize: root.theme.fontSmallest
                        }
                    }

                    // Cancel while in progress; open-folder once complete.
                    Label {
                        text: row.inProgress ? "close" : "folder_open"
                        visible: row.inProgress || row.done
                        color: actionHover.hovered ? root.theme.text : root.theme.textDim
                        font.family: root.theme.fontIcon
                        font.pixelSize: 18
                        HoverHandler { id: actionHover }
                        TapHandler {
                            onTapped: {
                                if (row.inProgress)
                                    row.modelData.cancel();
                                else
                                    Qt.openUrlExternally("file://" + row.modelData.downloadDirectory);
                            }
                        }
                    }
                }
            }
        }
    }
}
