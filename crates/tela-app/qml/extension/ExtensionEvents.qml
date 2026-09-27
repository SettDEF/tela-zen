import QtQuick

pragma ComponentBehavior: Bound

// The browser as an extension sees it.
//
// An extension keeps its own model of tabs and navigation, and answers its
// content scripts from that model rather than by asking. uBlock will not hand a
// page its cosmetic filters until it has been told that page's tab navigated —
// so this is not bookkeeping, it is the difference between filtering and not.
//
// Two shapes come out of here, and they must agree: the snapshot `tabs.query`
// answers from, and the lifecycle events pushed as they happen.
QtObject {
    id: root

    // The tab model, and which row is showing.
    required property var tabs
    required property int currentIndex

    // Where events are delivered — anything with `broadcastEvent(name, args)`.
    required property var sink
    // The host object whose `tabsJson` backs `tabs.query`.
    required property var api

    // One tab, in the shape WebExtensions describes. `id` is the stable tab
    // id, never the row: an extension holding id 4 must not find itself
    // talking about whatever tab later occupies position 4.
    function describe(index) {
        if (index < 0 || index >= root.tabs.count)
            return null;
        const tab = root.tabs.get(index);
        return {
            id: tab.tabId,
            index: index,
            windowId: 0,
            url: String(tab.pageUrl),
            title: tab.title,
            active: index === root.currentIndex,
            status: tab.loading ? "loading" : "complete",
            incognito: false,
            pinned: tab.pinned === true
        };
    }

    // Rebuild the snapshot `tabs.query` reads. Cheap, and called on a debounce
    // rather than per property change.
    function refresh() {
        const list = [];
        for (let i = 0; i < root.tabs.count; i++)
            list.push(root.describe(i));
        root.api.tabsJson = JSON.stringify(list);
    }

    function navigated(index, url) {
        const tab = root.describe(index);
        if (!tab)
            return;
        root.sink.broadcastEvent("webNavigation.onCommitted", [{
            tabId: tab.id,
            frameId: 0,
            parentFrameId: -1,
            url: url,
            transitionType: "link",
            transitionQualifiers: [],
            timeStamp: Date.now()
        }]);
        root.sink.broadcastEvent("tabs.onUpdated",
                                 [tab.id, { status: "loading", url: url }, tab]);
    }

    function loaded(index, url) {
        const tab = root.describe(index);
        if (!tab)
            return;
        const details = { tabId: tab.id, frameId: 0, url: url, timeStamp: Date.now() };
        root.sink.broadcastEvent("webNavigation.onDOMContentLoaded", [details]);
        root.sink.broadcastEvent("webNavigation.onCompleted", [details]);
        root.sink.broadcastEvent("tabs.onUpdated",
                                 [tab.id, { status: "complete", url: url }, tab]);
    }

    function created(index) {
        const tab = root.describe(index);
        if (tab)
            root.sink.broadcastEvent("tabs.onCreated", [tab]);
    }

    // Takes the id rather than the row: by the time a tab is gone there is no
    // row left to name it by.
    function removed(tabId) {
        if (tabId >= 0)
            root.sink.broadcastEvent("tabs.onRemoved",
                                     [tabId, { windowId: 0, isWindowClosing: false }]);
    }

    function activated(index) {
        const tab = root.describe(index);
        if (tab)
            root.sink.broadcastEvent("tabs.onActivated", [{ tabId: tab.id, windowId: 0 }]);
    }
}
