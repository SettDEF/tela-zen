import QtQuick

pragma ComponentBehavior: Bound

// What extensions ask the browser to do.
//
// The mirror of ExtensionEvents, which is what they are *told*. Everything an
// extension can make happen — run a script in a page, hide elements, open,
// navigate, close or reload a tab, deliver a message to another context —
// arrives here as a signal from the host object and is routed to whichever
// surface owns it.
//
// Anything that is the *window's* decision rather than a routing question —
// what happens when the last tab would close, where a new tab goes — leaves
// again as a signal.
Item {
    id: root

    required property var api
    required property var tabs
    // The StackLayout holding the page delegates.
    required property var pages
    required property var backgrounds
    required property var actionPopup

    signal openTab(string url)
    signal closeTab(int index)

    // ── Addressing ─────────────────────────────────────────────────────
    // Extensions address tabs by the stable id; the model is indexed by row,
    // and the two drift apart the moment anything is closed or reordered. A
    // negative id means "the tab I am in", which for a background caller is
    // the active one.
    function indexForTab(tabId) {
        if (tabId < 0)
            return root.pages.currentIndex;
        for (let i = 0; i < root.tabs.count; i++) {
            if (root.tabs.get(i).tabId === tabId)
                return i;
        }
        return -1;
    }

    function pageForTab(tabId) {
        const at = root.indexForTab(tabId);
        if (at < 0)
            return null;
        const wanted = root.tabs.get(at).tabId;
        for (let i = 0; i < root.pages.children.length; i++) {
            const page = root.pages.children[i];
            // The Repeater is a child too, and has none of these.
            if (page && page.applyCss && page.tabId === wanted)
                return page;
        }
        return null;
    }

    Connections {
        target: root.api

        // ── Messaging ──────────────────────────────────────────────────────
        // A message from a content script or popup goes to that extension's
        // background context; a message from the background goes out to pages.
        function onMessagePosted(ext, target, payload) {
            if (target === "background") {
                root.backgrounds.deliver(ext, payload);
                return;
            }

            // Only a request needs answering; a plain notification does not.
            let requestId = "";
            try {
                requestId = String(JSON.parse(payload).__telaReq || "");
            } catch (error) {
                requestId = "";
            }

            // The popup counts as a recipient when it is showing this
            // extension, so a message it handles is not answered twice.
            let took = root.actionPopup.extensionId === ext;
            root.actionPopup.deliverMessage(ext, payload);

            let outstanding = 0;
            const settle = function (accepted) {
                if (accepted)
                    took = true;
                outstanding -= 1;
                if (outstanding > 0 || took || requestId.length === 0)
                    return;
                // No context has this extension's bridge, so no reply is ever
                // coming. Answer undefined now — what a real browser does —
                // rather than leaving the sender on its timeout.
                root.api.postResponse(ext, requestId, "null");
            };

            for (let i = 0; i < root.pages.children.length; i++) {
                const page = root.pages.children[i];
                if (page && page.deliverMessage) {
                    outstanding += 1;
                    page.deliverMessage(ext, payload, settle);
                }
            }

            if (outstanding === 0 && !took && requestId.length > 0)
                root.api.postResponse(ext, requestId, "null");
        }

        // A reply has no target: whichever context is waiting on that id will
        // claim it, and the rest ignore it.
        function onResponsePosted(ext, request, payload) {
            root.backgrounds.deliverResponse(ext, request, payload);
            for (let i = 0; i < root.pages.children.length; i++) {
                const page = root.pages.children[i];
                if (page && page.deliverResponse)
                    page.deliverResponse(ext, request, payload);
            }
            root.actionPopup.deliverResponse(ext, request, payload);
        }

        // ── Reaching into a page ───────────────────────────────────────────
        function onScriptRequested(ext, tabId, source) {
            const page = root.pageForTab(tabId);
            if (page)
                page.runInPage(source);
        }

        // Element hiding, on its way to one page.
        function onCssRequested(ext, tabId, css, remove) {
            const page = root.pageForTab(tabId);
            if (page)
                page.applyCss(css, remove);
        }

        // ── Network on an extension's behalf ───────────────────────────────
        // QML's own XMLHttpRequest is not a page, so no CORS preflight applies.
        // That is the whole point: uBlock fetching a filter list is not a web
        // page reaching across origins, and a real browser exempts it.
        function onFetchRequested(ext, requestId, spec) {
            let request;
            try {
                request = JSON.parse(spec);
            } catch (error) {
                root.api.postResponse(ext, requestId,
                                      JSON.stringify({ error: "bad request" }));
                return;
            }

            const xhr = new XMLHttpRequest();
            xhr.open(request.method || "GET", request.url, true);
            const headers = request.headers || {};
            for (const name in headers)
                xhr.setRequestHeader(name, String(headers[name]));

            xhr.onreadystatechange = function () {
                if (xhr.readyState !== XMLHttpRequest.DONE)
                    return;
                // status 0 is a transport failure, not an HTTP answer.
                const payload = xhr.status === 0
                    ? { error: "network" }
                    : {
                        status: xhr.status,
                        statusText: xhr.statusText || "",
                        body: xhr.responseText || "",
                        headers: xhr.getAllResponseHeaders() || ""
                    };
                root.api.postResponse(ext, requestId, JSON.stringify(payload));
            };

            try {
                xhr.send(request.body ? request.body : null);
            } catch (error) {
                root.api.postResponse(ext, requestId,
                                      JSON.stringify({ error: String(error) }));
            }
        }

        // ── Tabs ───────────────────────────────────────────────────────────
        function onTabOpenRequested(ext, url) {
            root.openTab(String(url));
        }

        function onTabUpdateRequested(tabId, url, select) {
            const at = root.indexForTab(tabId);
            if (at < 0)
                return;
            if (String(url).length > 0)
                root.tabs.setProperty(at, "pageUrl", String(url));
            if (select)
                root.pages.currentIndex = at;
        }

        function onTabCloseRequested(tabId) {
            const at = root.indexForTab(tabId);
            if (at >= 0)
                root.closeTab(at);
        }

        function onTabReloadRequested(tabId) {
            const page = root.pageForTab(tabId);
            if (page)
                page.reload();
        }
    }
}
