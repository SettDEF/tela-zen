import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

pragma ComponentBehavior: Bound

// Centred over the page, input first, results growing underneath. The popup
// animates its own height so the dropdown expands rather than popping into
// place.
//
// Chooses, but does not act: what a result *means* — which tab to raise, how a
// typed query becomes a URL — belongs to the window, and arrives back as a
// signal.
Popup {
    id: root

    required property var theme
    required property var tabs
    required property var bookmarks
    required property var history
    // Prefilled on open, so the common case is "edit this address".
    property string currentUrl: ""

    signal tabSelected(int index)
    signal urlSelected(string url)
    // Typed text that still needs resolving into an address or a search.
    signal querySelected(string text)

    anchors.centerIn: Overlay.overlay
    width: 640
    padding: 0
    modal: true
    focus: true

    property var results: []
    property int selected: 0

    function rebuild(query) {
        const q = query.trim().toLowerCase();
        const out = [];

        if (q.length > 0) {
            out.push({
                kind: "navigate",
                label: query.trim(),
                hint: q.includes("://") || (q.includes(".") && !q.includes(" "))
                      ? "Open" : "Search"
            });
        }

        for (let i = 0; i < root.tabs.count; i++) {
            const tab = root.tabs.get(i);
            const url = String(tab.pageUrl);
            if (q.length === 0
                    || tab.title.toLowerCase().includes(q)
                    || url.toLowerCase().includes(q)) {
                out.push({ kind: "tab", index: i, label: tab.title, hint: url,
                           iconUrl: String(tab.iconUrl) });
            }
        }

        // Saved pages rank below open tabs: if it is already open, that is
        // almost always the one you meant.
        try {
            const saved = JSON.parse(root.bookmarks.search(query, 8));
            for (const entry of saved) {
                if (out.some(e => e.kind === "tab" && e.hint === entry.url))
                    continue;
                out.push({
                    kind: "bookmark",
                    label: entry.title || entry.url,
                    hint: entry.url
                });
            }
        } catch (error) {
            // A malformed store must not break the palette.
        }

        // History, only once something is typed — an empty query would dump the
        // whole backlog over the tabs and bookmarks that are the common target.
        if (q.length > 0) {
            try {
                const seen = JSON.parse(root.history.search(query, 8));
                for (const entry of seen) {
                    if (out.some(e => e.hint === entry.url))
                        continue;
                    out.push({
                        kind: "history",
                        label: entry.title || entry.url,
                        hint: entry.url
                    });
                }
            } catch (error) {
                // As with bookmarks, a bad store must not break the palette.
            }
        }

        root.results = out;
        root.selected = 0;
    }

    // The icon glyph that says what a result is at a glance.
    function glyphFor(entry) {
        switch (entry.kind) {
        case "tab":      return "tab";
        case "history":  return "history";
        case "bookmark": return "star";
        default:         return entry.hint === "Search" ? "search" : "public";
        }
    }

    // The word describing what pressing Enter does, shown on the active row.
    function actionFor(entry) {
        switch (entry.kind) {
        case "tab":      return "Switch";
        case "navigate": return entry.hint;
        default:         return "Open";
        }
    }

    // A colour hint per kind, kept subtle — the glyph carries the meaning.
    function tintFor(entry) {
        switch (entry.kind) {
        case "tab":      return root.theme.accent;
        case "bookmark": return root.theme.tertiary;
        case "history":  return root.theme.textDim;
        default:         return root.theme.textFaint;
        }
    }

    function activate(entry) {
        if (!entry)
            return;
        if (entry.kind === "tab")
            root.tabSelected(entry.index);
        else if (entry.kind === "bookmark" || entry.kind === "history")
            root.urlSelected(entry.hint);
        else
            root.querySelected(entry.label);
        root.close();
    }

    background: Rectangle {
        color: root.theme.surfaceHigh
        radius: root.theme.roundingLarge
        border.color: root.theme.border
        border.width: 1
    }

    enter: Transition {
        ParallelAnimation {
            NumberAnimation {
                property: "opacity"; from: 0; to: 1
                duration: root.theme.durFast
            }
            NumberAnimation {
                property: "scale"; from: 0.96; to: 1.0
                duration: root.theme.durNormal
                easing.type: Easing.BezierSpline
                easing.bezierCurve: root.theme.curveSpatial
            }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: root.theme.durFast }
    }

    onOpened: {
        paletteInput.text = root.currentUrl;
        paletteInput.selectAll();
        paletteInput.forceActiveFocus();
        root.rebuild(paletteInput.text);
    }

    // Height follows content so the dropdown grows smoothly.
    implicitHeight: paletteBody.implicitHeight
    Behavior on implicitHeight {
        NumberAnimation {
            duration: root.theme.durNormal
            easing.type: Easing.BezierSpline
            easing.bezierCurve: root.theme.curveDecel
        }
    }

    ColumnLayout {
        id: paletteBody
        width: parent.width
        spacing: 0

        TextField {
            id: paletteInput

            Layout.fillWidth: true
            placeholderText: "Search or enter URL…"
            color: root.theme.text
            placeholderTextColor: root.theme.textFaint
            font.family: root.theme.fontMain
            font.pixelSize: root.theme.fontLarge
            leftPadding: 20
            rightPadding: 20
            topPadding: 16
            bottomPadding: 16
            background: null
            // A URL longer than the field scrolls horizontally, and TextField
            // does not clip: without this the text paints straight through the
            // popup's rounded background and out over the page. Safe here —
            // the clip warning in CLAUDE.md is about web views, which have no
            // compositor surface once they are given a render target. Text has
            // no such problem.
            clip: true

            onTextEdited: root.rebuild(text)
            onAccepted: root.activate(root.results[root.selected])

            Keys.onEscapePressed: root.close()
            Keys.onDownPressed: root.selected =
                Math.min(root.selected + 1, root.results.length - 1)
            Keys.onUpPressed: root.selected = Math.max(root.selected - 1, 0)
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            visible: root.results.length > 0
            color: root.theme.border
        }

        ListView {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, 320)
            Layout.bottomMargin: root.results.length > 0 ? 8 : 0
            Layout.topMargin: root.results.length > 0 ? 8 : 0
            Layout.leftMargin: 8
            Layout.rightMargin: 8
            clip: true
            interactive: contentHeight > height
            model: root.results
            currentIndex: root.selected

            delegate: Rectangle {
                id: resultRow

                required property int index
                required property var modelData

                readonly property bool current: index === root.selected
                // The URL hint is redundant for a navigate row (the label already
                // is the URL) and for a tab whose title equals its address.
                readonly property bool showHint:
                    resultRow.modelData.kind !== "navigate"
                    && String(resultRow.modelData.hint || "") !== ""
                    && resultRow.modelData.hint !== resultRow.modelData.label

                width: ListView.view.width
                height: 48
                radius: root.theme.rounding
                color: current ? root.theme.surfaceHover : "transparent"

                Behavior on color { ColorAnimation { duration: root.theme.durFast } }

                HoverHandler { onHoveredChanged: if (hovered) root.selected = resultRow.index }
                TapHandler { onTapped: root.activate(resultRow.modelData) }

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 18
                    anchors.rightMargin: 16
                    spacing: 13

                    // Favicon when we have one (open tabs), a kind glyph otherwise.
                    Item {
                        Layout.preferredWidth: 18
                        Layout.preferredHeight: 18

                        Image {
                            id: rowFavicon
                            anchors.fill: parent
                            source: resultRow.modelData.iconUrl || ""
                            visible: status === Image.Ready
                            sourceSize.width: 32
                            sourceSize.height: 32
                            smooth: true
                        }
                        Label {
                            anchors.centerIn: parent
                            visible: !rowFavicon.visible
                            text: root.glyphFor(resultRow.modelData)
                            color: root.tintFor(resultRow.modelData)
                            font.family: root.theme.fontIcon
                            font.pixelSize: 17
                        }
                    }

                    // Title, and the address beneath it when it adds something.
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 1

                        Label {
                            Layout.fillWidth: true
                            text: resultRow.modelData.label
                            color: root.theme.text
                            font.family: root.theme.fontMain
                            font.pixelSize: root.theme.fontNormal
                            elide: Text.ElideRight
                        }
                        Label {
                            Layout.fillWidth: true
                            visible: resultRow.showHint
                            text: resultRow.modelData.hint
                            color: root.theme.textFaint
                            font.family: root.theme.fontMain
                            font.pixelSize: root.theme.fontSmallest
                            elide: Text.ElideRight
                        }
                    }

                    // What Enter will do — only on the active row, so the list
                    // stays quiet and the eye lands on the one that matters.
                    Row {
                        spacing: 6
                        opacity: resultRow.current ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: root.theme.durFast } }

                        Label {
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.actionFor(resultRow.modelData)
                            color: root.theme.textDim
                            font.family: root.theme.fontMain
                            font.pixelSize: root.theme.fontSmallest
                        }
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 18; height: 18
                            radius: 5
                            color: root.theme.surfaceActive
                            Label {
                                anchors.centerIn: parent
                                text: "↵"
                                color: root.theme.textDim
                                font.family: root.theme.fontMain
                                font.pixelSize: root.theme.fontSmallest
                            }
                        }
                    }
                }
            }
        }
    }
}
