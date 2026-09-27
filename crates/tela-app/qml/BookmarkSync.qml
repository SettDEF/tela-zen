import QtQuick

pragma ComponentBehavior: Bound

// Bookmark files followed by URL.
//
// A subscription is merged, never authoritative: importing only ever adds, so
// a sync cannot remove anything you saved yourself. Fetched here rather than
// in a page, because QML's XMLHttpRequest has no page origin and so no CORS.
Item {
    id: root

    required property var bookmarks
    required property var config

    // Emitted per source, so the UI can say what changed.
    signal synced(string source, int added)
    signal failed(string source, string reason)

    readonly property var sources: {
        try {
            const list = JSON.parse(root.config.bookmarkSubscriptionsJson);
            return Array.isArray(list) ? list.filter(u => String(u).length > 0) : [];
        } catch (error) {
            return [];
        }
    }

    function syncNow() {
        for (const source of root.sources)
            root.fetchOne(String(source));
    }

    function fetchOne(source) {
        const xhr = new XMLHttpRequest();
        xhr.open("GET", source, true);
        xhr.onreadystatechange = function () {
            if (xhr.readyState !== XMLHttpRequest.DONE)
                return;
            if (xhr.status < 200 || xhr.status >= 300) {
                root.failed(source, "HTTP " + xhr.status);
                return;
            }
            const added = root.bookmarks.importText(xhr.responseText || "");
            if (added < 0)
                root.failed(source, "not a bookmark file");
            else
                root.synced(source, added);
        };
        try {
            xhr.send();
        } catch (error) {
            root.failed(source, String(error));
        }
    }

    // Not at startup: a browser opening should not wait on the network.
    property Timer initial: Timer {
        interval: 6000
        running: root.sources.length > 0
        repeat: false
        onTriggered: root.syncNow()
    }

    property Timer periodic: Timer {
        interval: Math.max(1, root.config.bookmarkSyncHours) * 3600 * 1000
        running: root.config.bookmarkSyncHours > 0 && root.sources.length > 0
        repeat: true
        onTriggered: root.syncNow()
    }
}
