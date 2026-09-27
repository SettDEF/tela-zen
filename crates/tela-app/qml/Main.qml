import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Shapes
import QtQuick.Window
import QtWebEngine
import QtWebChannel
import Tela

pragma ComponentBehavior: Bound

ApplicationWindow {
    id: window

    width: 1360
    height: 880
    visible: true
    color: appTheme.bg
    title: tabs.count > 0 ? tabs.get(pages.currentIndex).title : "Tela"

    Theme { id: appTheme }

    // Explicit opaque backdrop rather than relying on the window's clear
    // colour. On Wayland the surface carries an alpha channel, so any region
    // no item paints — the gaps between panels, and newly exposed area during
    // a resize — composites as a hole rather than as the window colour.
    Rectangle {
        anchors.fill: parent
        color: appTheme.bg
        z: -1
    }

    // Userscript / content-script host. Match evaluation lives in Rust, so a
    // page matching nothing costs one call and injects nothing at all.
    Shell { id: shell }

    Bookmarks {
        id: bookmarks
        Component.onCompleted: reload()
    }

    // The profile everything runs on. Declared here deliberately: a profile
    // built in C++ never gets a cookie store, so cookies would not survive a
    // restart. C++ adopts this one by objectName once the engine has loaded.
    WebEngineProfile {
        id: browserProfile
        objectName: "browserProfile"
        // Per profile, so each keeps its own cookies, history and cache.
        storageName: appTheme.config.profileName.length > 0
                     ? "tela-" + appTheme.config.profileName : "tela"
        offTheRecord: false
        persistentCookiesPolicy: WebEngineProfile.ForcePersistentCookies

        // A file the page wants to save. Handed to the downloads panel, which
        // accepts it to the default download directory and tracks its progress.
        onDownloadRequested: download => {
            downloadsPanel.accept(download);
            downloadsPanel.showNear(null);
        }
    }

    Session { id: session }

    // Browsing history with full-text search, opened once at startup.
    History {
        id: history
        Component.onCompleted: open()
    }

    // Bookmark files followed by URL; merged, never authoritative.
    BookmarkSync {
        bookmarks: bookmarks
        config: appTheme.config

        onSynced: (source, added) => {
            if (added > 0)
                console.log("bookmarks: +" + added + " from " + source);
        }
    }

    // Saving is debounced rather than immediate: navigation fires several
    // property changes in a row, and rewriting the file on each one would turn
    // one page load into a handful of disk writes.
    Timer {
        id: sessionSaver
        interval: 800
        repeat: false
        onTriggered: window.saveSession()
    }

    // The snapshot extensions query was previously rebuilt only when a tab
    // opened, closed or was selected — never when one navigated. An extension
    // looking its own tab up by URL therefore searched the *previous* page's
    // address and found nothing: "Failed to get tab ID of <url>". Debounced
    // because one navigation moves url and title separately.
    Timer {
        id: tabsRefresher
        interval: 150
        repeat: false
        onTriggered: extensionEvents.refresh()
    }

    function scheduleTabsRefresh() {
        tabsRefresher.restart();
    }

    // Retire idle background tabs, releasing their renderer processes. One sweep
    // for all tabs rather than a timer per tab. Runs only when suspension is
    // enabled; the per-tab guards below decide what is actually eligible.
    Timer {
        id: suspendSweep
        interval: 60000
        repeat: true
        running: appTheme.suspendAfterMinutes > 0
        onTriggered: {
            const mins = appTheme.suspendAfterMinutes;
            if (mins <= 0)
                return;
            const cutoff = Date.now() - mins * 60000;
            for (let i = 0; i < tabs.count; i++) {
                if (i === pages.currentIndex)
                    continue;
                const page = pageViews.itemAt(i);
                if (!page || page.startPage || page.deferred || page.suspended)
                    continue;
                if (page.poppedOut || page.loading || page.audible)
                    continue;
                const t = tabs.get(i);
                if (!t || t.pinned === true)
                    continue;
                // Extension pages gate their body on a background ready-state
                // and are cheap; leave them alone for now.
                if (page.extensionPage)
                    continue;
                // Qt already withholds Active for audio, fullscreen, active
                // downloads and pending beforeunload — honour that verdict.
                const wv = page.webView;
                if (wv && wv.recommendedState === WebEngineView.LifecycleState.Active)
                    continue;
                const last = t.lastActiveMs || 0;
                if (last === 0) {
                    // Live background tab that was never foregrounded (a seed or
                    // background-opened tab): start its idle clock now so it can
                    // retire a threshold later, rather than living forever.
                    tabs.setProperty(i, "lastActiveMs", Date.now());
                } else if (last < cutoff) {
                    page.suspend();
                }
            }
        }
    }

    // Coming back to the window — from another workspace, from being hidden —
    // finds every web view holding a surface the compositor already took back.
    // Nothing repaints on its own, so the page is black or torn until
    // something disturbs it.
    onActiveChanged: if (active) window.repaintSurfaces()

    function repaintSurfaces() {
        for (let i = 0; i < pages.children.length; i++) {
            const page = pages.children[i];
            // The Repeater is a child too, and has no view to repaint.
            if (page && page.repaint)
                page.repaint();
        }
        actionPopup.repaint();
    }

    // Everything extensions are told about tabs and navigation lives here.
    ExtensionEvents {
        id: extensionEvents

        tabs: tabs
        currentIndex: pages.currentIndex
        sink: backgroundContexts
        api: extensionApi
    }

    function saveSession() {
        const list = [];
        for (let i = 0; i < tabs.count; i++) {
            const tab = tabs.get(i);
            let history = [];
            try {
                history = JSON.parse(tab.historyJson || "[]");
            } catch (error) {
                history = [];
            }
            list.push({
                title: tab.title,
                url: String(tab.pageUrl),
                pinned: tab.pinned === true,
                // Back and Forward were dead after a restart without these:
                // the engine's history dies with its renderer.
                history: history,
                historyIndex: tab.historyIndex !== undefined ? tab.historyIndex : -1,
                // So a restored tab reopens at the scroll position it was left.
                scrollX: tab.scrollX || 0,
                scrollY: tab.scrollY || 0,
                // The last-known favicon, so a restored-but-unloaded tab shows
                // its icon instead of the placeholder dot until it loads.
                iconUrl: String(tab.iconUrl || "")
            });
        }
        session.save(JSON.stringify(list), pages.currentIndex);
    }

    function scheduleSessionSave() {
        if (appTheme.restoreSession)
            sessionSaver.restart();
    }

    // Restore before anything navigates, so the default tabs are replaced
    // rather than appended to.
    function restoreSession() {
        if (!appTheme.restoreSession)
            return false;
        if (!session.load() || session.restoredCount === 0)
            return false;

        let restored = [];
        try {
            restored = JSON.parse(session.restoredJson);
        } catch (error) {
            return false;
        }
        if (restored.length === 0)
            return false;

        tabs.clear();
        for (const tab of restored) {
            tabs.append({
                title: tab.title || tab.url,
                pageUrl: tab.url,
                iconUrl: tab.iconUrl || "",
                loading: false,
                pinned: tab.pinned === true,
                tabId: window.nextTabId(),
                historyJson: JSON.stringify(tab.history || []),
                historyIndex: tab.historyIndex !== undefined ? tab.historyIndex : -1,
                // Restoring nine tabs used to spawn nine renderer processes at
                // once, each fetching and parsing a page before the window was
                // usable. Deferred tabs cost nothing until you click them.
                deferred: true,
                suspended: false,
                audible: false,
                muted: false,
                // Age unvisited restored tabs from now, so the sweep can retire
                // ones the user never returns to.
                lastActiveMs: Date.now(),
                scrollX: tab.scrollX || 0,
                scrollY: tab.scrollY || 0
            });
        }
        const active = Math.min(session.restoredIndex, tabs.count - 1);
        tabs.setProperty(active, "deferred", false);
        pages.currentIndex = active;
        return true;
    }

    Extensions {
        id: extensionHost

        Component.onCompleted: reload()

        // Bundles finish loading after the first tab has already navigated, so
        // the scripts that navigation asked for were computed against an empty
        // list. Re-apply once the list is known.
        onBundlesJsonChanged: {
            for (let i = 0; i < pages.children.length; i++) {
                const page = pages.children[i];
                if (page && page.applyUserScripts && page.url)
                    page.applyUserScripts(String(page.url), true);
            }
        }
    }

    // ── Extension API host ─────────────────────────────────────────────
    // One API object shared by every extension context. Calls carry the
    // extension id, which the host supplies when it builds each binding —
    // scripts cannot claim another extension's identity.
    ExtensionApi {
        id: extensionApi

        // The name the shim looks up as channel.objects.telaApi.
        WebChannel.id: "telaApi"

        // The snapshot tabs.query answers from is built by ExtensionEvents,
        // which owns everything else extensions are told about tabs.
    }

    Connections {
        target: tabs
        function onCountChanged() {
            extensionEvents.refresh();
            window.scheduleSessionSave();
        }
    }

    Connections {
        target: pages
        function onCurrentIndexChanged() {
            extensionEvents.refresh();
            window.scheduleSessionSave();
            extensionEvents.activated(pages.currentIndex);
        }
    }

    // Everything extensions ask the browser to do.
    ExtensionRequests {
        api: extensionApi
        tabs: tabs
        pages: pages
        backgrounds: backgroundContexts
        actionPopup: actionPopup

        onOpenTab: url => window.openTab(url)
        onCloseTab: index => window.closeTab(index)
    }

    // A second launch of this profile (e.g. clicking an external link while
    // Tela is the default browser) hands its URL here instead of opening a new
    // window. Empty means "just come to the front".
    SingleInstance {
        id: singleInstance
        Component.onCompleted: start()
        onOpenUrl: url => {
            if (String(url).length > 0)
                window.openTab(String(url));
            window.show();
            window.raise();
            window.requestActivate();
        }
    }

    Component.onCompleted: {
        const launch = appTheme.config.launchUrl;

        // A URL on the command line is an explicit instruction and wins over a
        // restored session — but it is added to it, not instead of it, so
        // opening a link never costs you your tabs.
        const restored = window.restoreSession();

        if (launch.length > 0) {
            if (restored) {
                window.openTab(launch);
            } else {
                tabs.setProperty(0, "pageUrl", launch);
                tabs.setProperty(0, "title", launch);
                pages.currentIndex = 0;
            }
        }

        extensionEvents.refresh();
    }

    WebChannel {
        id: extensionChannel
        registeredObjects: [extensionApi]
    }

    // ── Background contexts ────────────────────────────────────────────
    BackgroundHost {
        id: backgroundContexts

        extensions: extensionHost
        channel: extensionChannel
        theme: appTheme
        api: extensionApi
        profile: browserProfile
    }

    // Tab state lives in QML for now. A Rust-backed model takes this over when
    // session restore and per-tab history need to outlive the window.
    ListModel {
        id: tabs
        ListElement {
            title: "Qt WebEngine"
            pageUrl: "https://doc.qt.io/qt-6/qtwebengine-index.html"
            iconUrl: ""
            loading: false
            pinned: false
            tabId: 1
            // Every role the delegate requires must be here: a ListElement
            // missing one means TabPage is never created, and the tab just
            // does not exist — with no error to say so.
            deferred: false
            suspended: false
            audible: false
            muted: false
            lastActiveMs: 0
            scrollX: 0
            scrollY: 0
            historyJson: "[]"
            historyIndex: -1
        }
        ListElement {
            title: "Hacker News"
            pageUrl: "https://news.ycombinator.com"
            iconUrl: ""
            loading: false
            pinned: false
            tabId: 2
            // Every role the delegate requires must be here: a ListElement
            // missing one means TabPage is never created, and the tab just
            // does not exist — with no error to say so.
            deferred: false
            suspended: false
            audible: false
            muted: false
            lastActiveMs: 0
            scrollX: 0
            scrollY: 0
            historyJson: "[]"
            historyIndex: -1
        }
    }

    // Tab identity as extensions see it, and it has to survive closing and
    // reordering: an extension holding id 4 must not find itself talking about
    // whatever tab later occupies position 4. Row index is not that.
    property int lastTabId: 2

    function nextTabId() {
        return ++window.lastTabId;
    }

    // ── Tab bar state ──────────────────────────────────────────────────
    // Four modes, set by appearance.tabBar.mode:
    //   sidebar          docked, always expanded
    //   sidebar-compact  docked icon rail, expands over the page on hover
    //   sidebar-auto     fully hidden, slides in from the edge on hover
    //   top              horizontal strip, no sidebar
    QtObject {
        id: tabBar

        // Manual override via Ctrl+B; cleared when the pointer leaves.
        property bool pinnedOpen: false

        readonly property bool overlayMode: appTheme.tabBarMode === "sidebar-auto"
        readonly property bool railMode: appTheme.tabBarMode === "sidebar-compact"

        readonly property bool revealed: pinnedOpen || edgeHover.hovered || sidebarHover.hovered

        readonly property int width: !appTheme.tabsInSidebar ? 0
                                   : appTheme.tabBarMode === "sidebar" ? appTheme.sidebarWidth
                                   : revealed ? appTheme.sidebarWidth
                                   : railMode ? appTheme.railWidth
                                   : 0
    }

    function resolveQuery(raw) {
        const query = raw.trim();
        if (query.length === 0)
            return "";
        if (query.includes("://") || query.startsWith("about:"))
            return query;
        if (query.includes(".") && !query.includes(" "))
            return "https://" + query;
        return appTheme.config.searchTemplate.replace("{query}", encodeURIComponent(query));
    }

    readonly property string startUrl: "tela://start"

    // Flip a tab's mute. TabPage binds view.audioMuted to this role, so the
    // change reaches the page; a muted tab keeps its state until toggled back.
    function toggleMute(index) {
        if (index < 0 || index >= tabs.count)
            return;
        tabs.setProperty(index, "muted", !tabs.get(index).muted);
    }

    function isStartUrl(url) {
        const u = String(url);
        return u === "tela://start" || u === "about:blank" || u === "";
    }

    function openTab(url) {
        tabs.append({ title: "New Tab", pageUrl: url, iconUrl: "", loading: false,
                      pinned: false, tabId: window.nextTabId(),
                      historyJson: "[]", historyIndex: -1, deferred: false,
                      suspended: false, audible: false, muted: false, lastActiveMs: 0, scrollX: 0, scrollY: 0 });
        pages.currentIndex = tabs.count - 1;
        extensionEvents.created(tabs.count - 1);
        // Whatever was open over the page was for the tab you just left. This
        // matters most when the tab came *from* one of them — an extension's
        // "Preferences" opens its options page, and leaving the panel covering
        // it is the wrong answer to a click that meant "take me there".
        settingsPanel.close();
        extensionsPopup.close();
        actionPopup.close();
    }

    // A video has the whole window. Every piece of chrome checks this, and the
    // previous visibility is kept so leaving fullscreen restores a maximised
    // window as maximised rather than dropping it to a plain one.
    property bool mediaFullScreen: false
    property int visibilityBeforeFullScreen: Window.Windowed

    function setMediaFullScreen(on) {
        if (on === window.mediaFullScreen)
            return;
        if (on) {
            window.visibilityBeforeFullScreen = window.visibility;
            window.mediaFullScreen = true;
            window.visibility = Window.FullScreen;
        } else {
            window.mediaFullScreen = false;
            window.visibility = window.visibilityBeforeFullScreen;
        }
    }

    function toggleBookmark() {
        if (!pages.currentView)
            return;
        const url = String(pages.currentView.url);
        if (url.length === 0 || window.isStartUrl(url))
            return;
        const title = tabs.count > 0 ? tabs.get(pages.currentIndex).title : url;
        bookmarks.toggle(url, title);
        // `contains` is a call, not a bound property, so the star has to be
        // told the answer changed.
        bookmarkRevision++;
    }

    property int bookmarkRevision: 0

    // Recently closed tabs, most-recent last, for Ctrl+Shift+T. Bounded so it
    // cannot grow without limit over a long session.
    property var closedStack: []

    function closeTab(index) {
        if (tabs.count === 1)
            return;
        // Read before the row goes: afterwards there is nothing to name.
        const closing = index >= 0 && index < tabs.count ? tabs.get(index).tabId : -1;
        if (index >= 0 && index < tabs.count) {
            const row = tabs.get(index);
            const url = String(row.pageUrl);
            if (url.length > 0 && !window.isStartUrl(url)) {
                const stack = window.closedStack.slice();
                stack.push({ url: url, index: index });
                while (stack.length > 25)
                    stack.shift();
                window.closedStack = stack;
            }
        }
        tabs.remove(index);
        pages.currentIndex = Math.min(pages.currentIndex, tabs.count - 1);
        extensionEvents.removed(closing);
    }

    // Reopen the last-closed tab at (roughly) where it was, and focus it.
    function reopenClosedTab() {
        if (window.closedStack.length === 0)
            return;
        const stack = window.closedStack.slice();
        const entry = stack.pop();
        window.closedStack = stack;
        window.openTab(entry.url);
    }

    // The web view of the current tab, or null on the start page.
    function currentWebView() {
        return pages.currentView ? pages.currentView.webView : null;
    }

    function zoomBy(delta) {
        const v = window.currentWebView();
        if (!v)
            return;
        v.zoomFactor = Math.max(0.25, Math.min(5, v.zoomFactor + delta));
        zoomIndicator.flash(v.zoomFactor);
    }

    function zoomReset() {
        const v = window.currentWebView();
        if (!v)
            return;
        v.zoomFactor = 1.0;
        zoomIndicator.flash(1.0);
    }

    onClosing: {
        // The debounce timer will not fire after this point.
        if (appTheme.restoreSession)
            window.saveSession();
    }

    Shortcut { sequences: ["Ctrl+K", "Ctrl+L"]; onActivated: commandPalette.open() }
    Shortcut {
        sequences: ["Ctrl+T"]
        onActivated: window.openTab(appTheme.startPageEnabled && window.isStartUrl(appTheme.config.homeUrl)
                                    ? window.startUrl : appTheme.config.homeUrl)
    }
    Shortcut { sequences: ["Ctrl+W"]; onActivated: window.closeTab(pages.currentIndex) }
    Shortcut { sequences: ["Ctrl+B"]; onActivated: tabBar.pinnedOpen = !tabBar.pinnedOpen }
    Shortcut { sequences: ["Ctrl+,"]; onActivated: settingsPanel.open() }
    Shortcut {
        sequences: ["Ctrl+R", "F5"]
        onActivated: if (pages.currentView) pages.currentView.reload()
    }
    Shortcut {
        sequences: ["Ctrl+Shift+R", "Ctrl+F5", "Shift+F5"]
        onActivated: if (pages.currentView) pages.currentView.hardReload()
    }
    Shortcut { sequences: ["Ctrl+F"]; onActivated: findBar.show() }
    Shortcut { sequences: ["Ctrl+D"]; onActivated: window.toggleBookmark() }
    Shortcut { sequences: ["Ctrl+Shift+A"]; onActivated: extensionsPopup.showNear(null) }
    Shortcut { sequences: ["Escape"]; onActivated: if (findBar.visible) findBar.hide() }
    Shortcut {
        sequences: ["Ctrl+Shift+P"]
        onActivated: {
            if (popout.attached)
                popout.returnHome();
            else if (pages.currentView)
                pages.currentView.popOut();
        }
    }

    // Zoom. Ctrl+= and Ctrl++ both zoom in (the key is the same one, shifted or
    // not), Ctrl+- out, Ctrl+0 back to 100%.
    Shortcut { sequences: ["Ctrl+=", "Ctrl++"]; onActivated: window.zoomBy(0.1) }
    Shortcut { sequences: ["Ctrl+-"]; onActivated: window.zoomBy(-0.1) }
    Shortcut { sequences: ["Ctrl+0"]; onActivated: window.zoomReset() }

    // Mute the current tab.
    Shortcut { sequences: ["Ctrl+M"]; onActivated: window.toggleMute(pages.currentIndex) }

    // Browsing history.
    Shortcut { sequences: ["Ctrl+H"]; onActivated: historyPanel.open() }

    // Reader mode on the current page.
    Shortcut {
        sequences: ["F9"]
        onActivated: if (pages.currentView && pages.currentView.toggleReader)
            pages.currentView.toggleReader()
    }

    // Reopen the last closed tab.
    Shortcut { sequences: ["Ctrl+Shift+T"]; onActivated: window.reopenClosedTab() }

    // Cycle tabs.
    Shortcut {
        sequences: ["Ctrl+Tab"]
        onActivated: if (tabs.count > 0)
            pages.currentIndex = (pages.currentIndex + 1) % tabs.count
    }
    Shortcut {
        sequences: ["Ctrl+Shift+Tab"]
        onActivated: if (tabs.count > 0)
            pages.currentIndex = (pages.currentIndex - 1 + tabs.count) % tabs.count
    }

    // Ctrl+1..8 jump to the nth tab; Ctrl+9 to the last, as every browser does.
    Repeater {
        model: 8
        Shortcut {
            required property int index
            sequence: "Ctrl+" + (index + 1)
            onActivated: if (index < tabs.count) pages.currentIndex = index
        }
    }
    Shortcut { sequences: ["Ctrl+9"]; onActivated: if (tabs.count > 0) pages.currentIndex = tabs.count - 1 }

    // A brief readout of the current zoom, shown while zooming and then faded.
    Rectangle {
        id: zoomIndicator
        z: 200
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        anchors.topMargin: 18
        width: zoomLabel.implicitWidth + 30
        height: 34
        radius: appTheme.rounding
        color: appTheme.surfaceHigh
        border.color: appTheme.border
        border.width: 1
        opacity: 0
        visible: opacity > 0
        property string label: "100%"

        function flash(factor) {
            zoomIndicator.label = Math.round(factor * 100) + "%";
            zoomIndicator.opacity = 1;
            zoomHide.restart();
        }

        Behavior on opacity { NumberAnimation { duration: appTheme.durFast } }
        Timer { id: zoomHide; interval: 1200; onTriggered: zoomIndicator.opacity = 0 }

        Label {
            id: zoomLabel
            anchors.centerIn: parent
            text: zoomIndicator.label
            color: appTheme.text
            font.family: appTheme.fontMain
            font.pixelSize: appTheme.fontNormal
            font.weight: Font.DemiBold
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        TabStrip {
            Layout.fillWidth: true
            visible: appTheme.tabsOnTop && !window.mediaFullScreen
            theme: appTheme
            tabs: tabs
            currentIndex: pages.currentIndex

            onTabActivated: index => pages.currentIndex = index
            onTabClosed: index => window.closeTab(index)
            onTabMuteToggled: index => window.toggleMute(index)
            onNewTabRequested: window.openTab(appTheme.config.homeUrl)
            onPaletteRequested: commandPalette.open()
            onTabMenuRequested: (index, point) => tabMenu.showFor(index, point)
        }

        Item {
            id: body

            Layout.fillWidth: true
            Layout.fillHeight: true

            // ── Page card ──────────────────────────────────────────────
            // Content sits on its own rounded surface, inset from the window
            // edge — the same layering the shell uses for its panels. Only
            // docked sidebar modes reserve space; overlay modes float above.
            Item {
                id: pageCard

                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.right: parent.right
                anchors.left: parent.left
                // In media fullscreen the panels are hidden, so their width must
                // not be reserved — the video fills the screen, or floats with
                // the configured gap. Otherwise reserve the docked panels plus
                // the normal page inset.
                anchors.leftMargin: window.mediaFullScreen
                    ? appTheme.fullScreenGap
                    : appTheme.pageInset
                      + (appTheme.sidebarReserved > 0 ? appTheme.sidebarReserved + appTheme.pageInset : 0)
                anchors.topMargin: window.mediaFullScreen ? appTheme.fullScreenGap : appTheme.pageInset
                anchors.rightMargin: window.mediaFullScreen
                    ? appTheme.fullScreenGap
                    : appTheme.pageInset
                      + (appTheme.rightBarEnabled ? appTheme.rightBarWidth + appTheme.pageInset : 0)
                anchors.bottomMargin: window.mediaFullScreen ? appTheme.fullScreenGap : appTheme.pageInset

                Behavior on anchors.leftMargin {
                    NumberAnimation {
                        duration: appTheme.durSpatial
                        easing.type: Easing.BezierSpline
                        easing.bezierCurve: appTheme.curveSpatial
                    }
                }

                Rectangle {
                    anchors.fill: parent
                    // A fullscreen video is a clean rectangle by default; the
                    // toggle keeps the rounded card if wanted.
                    radius: window.mediaFullScreen && !appTheme.fullScreenRounded
                            ? 0 : appTheme.cardRadius
                    color: appTheme.surface
                }

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 0

                    // Chrome. In shy mode it collapses to nothing until the
                    // pointer comes near or the palette takes focus.
                    Item {
                        id: chrome

                        // Nothing above the video while it owns the window.
                        visible: !window.mediaFullScreen

                        readonly property bool revealed: !appTheme.shyChrome
                                                       || chromeHover.hovered
                                                       || commandPalette.opened

                        Layout.fillWidth: true
                        Layout.preferredHeight: revealed ? 40 : 0
                        clip: true

                        Behavior on Layout.preferredHeight {
                            NumberAnimation {
                                duration: appTheme.durNormal
                                easing.type: Easing.BezierSpline
                                easing.bezierCurve: appTheme.curveDecel
                            }
                        }

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 8
                            anchors.rightMargin: 14
                            spacing: 2

                            NavButton {
                                theme: appTheme
                                label: "arrow_back"
                                active: pages.currentView ? pages.currentView.canGoBack : false
                                onTriggered: pages.currentView.goBack()
                            }
                            NavButton {
                                theme: appTheme
                                label: "arrow_forward"
                                active: pages.currentView ? pages.currentView.canGoForward : false
                                onTriggered: pages.currentView.goForward()
                            }
                            NavButton {
                                theme: appTheme
                                label: pages.currentView && pages.currentView.loading ? "close" : "refresh"
                                active: true
                                // Shift bypasses the cache, as it does in every
                                // other browser.
                                onTriggered: modifiers => {
                                    if (pages.currentView.loading)
                                        pages.currentView.stop();
                                    else if (modifiers & Qt.ShiftModifier)
                                        pages.currentView.hardReload();
                                    else
                                        pages.currentView.reload();
                                }
                            }

                            Item { Layout.preferredWidth: 6 }

                            AddressBar {
                                id: addressBar

                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignVCenter

                                theme: appTheme
                                page: pages.currentView
                                blockingEnabled: appTheme.config.blockingEnabled
                                bookmarked: pages.currentView
                                            && bookmarks.contains(String(pages.currentView.url))

                                onAddressActivated: commandPalette.open()
                                onBookmarkToggled: window.toggleBookmark()
                                onBlockingToggled: appTheme.config.setSetting(
                                    "blocking.enabled",
                                    JSON.stringify(!appTheme.config.blockingEnabled))
                                onPanelRequested: mode => siteInfo.showFor(mode, addressBar)
                            }
                        }
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 2
                        color: "transparent"

                        Rectangle {
                            height: parent.height
                            width: parent.width * ((pages.currentView ? pages.currentView.loadProgress : 100) / 100)
                            color: appTheme.accent
                            opacity: pages.currentView && pages.currentView.loading ? 1 : 0

                            Behavior on width {
                                NumberAnimation {
                                    duration: appTheme.durFast
                                    easing.type: Easing.BezierSpline
                                    easing.bezierCurve: appTheme.curveDecel
                                }
                            }
                            Behavior on opacity { NumberAnimation { duration: appTheme.durNormal } }
                        }
                    }

                    StackLayout {
                        id: pages

                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        currentIndex: 0
                        // Deliberately NOT clipped. A WebEngineView renders into
                        // its own surface, and clipping forces it through a
                        // render target that leaves parts of the window unpainted
                        // — the desktop shows through. Any transient overflow on
                        // resize is far less bad than a see-through window.

                        // Bound through the Repeater rather than `children`.
                        // `children` does not notify when the Repeater fills
                        // it in, so this binding used to be evaluated once —
                        // while the Repeater was the only child — and keep
                        // that answer. With one restored tab, currentIndex
                        // never changes to re-evaluate it, so the toolbar was
                        // reading a Repeater and showing "undefined" for the
                        // address. `count` does notify.
                        readonly property var currentView:
                            pageViews.count > 0 ? pageViews.itemAt(currentIndex) : null

                        // The tab we are leaving, so its idle clock can start.
                        property int previousIndex: 0

                        onCurrentIndexChanged: {
                            const child = pageViews.itemAt(currentIndex);
                            if (child) {
                                if (child.deferred) {
                                    child.deferred = false;
                                    tabs.setProperty(currentIndex, "deferred", false);
                                }
                                // Wake before the tab is shown, so the reload
                                // happens under the placeholder, not on a blank
                                // visible view.
                                if (child.suspended)
                                    child.resume();
                            }
                            // Stamp when the outgoing tab stopped being foreground.
                            if (previousIndex >= 0 && previousIndex < tabs.count
                                    && previousIndex !== currentIndex)
                                tabs.setProperty(previousIndex, "lastActiveMs", Date.now());
                            previousIndex = currentIndex;
                        }

                        Repeater {
                            id: pageViews

                            model: tabs

                            TabPage {
                                theme: appTheme
                                tabs: tabs
                                extensions: extensionHost
                                api: extensionApi
                                events: extensionEvents
                                popout: popout
                                startUrl: window.startUrl
                                pageProfile: browserProfile

                                onNewTabRequested: target => window.openTab(target)
                                onContextMenuRequested: (page, request) => pageMenu.show(page, request)
                                onPaletteRequested: commandPalette.open()
                                onCustomiseRequested: backgroundPicker.open()
                                onFullScreenRequested: on => window.setMediaFullScreen(on)
                                onSessionDirty: window.scheduleSessionSave()
                                onTabsDirty: window.scheduleTabsRefresh()
                                onVisited: (url, title) => history.recordVisit(url, title)
                            }
                        }
                    }
                }

                // Reveals the shy chrome without stealing input from the page:
                // a hover strip across the top of the card.
                HoverHandler { id: chromeHover }

                // Rounds the card's corners over live web content. A Shape with
                // an even-odd fill paints only the corner slivers in the window
                // colour, which keeps the WebEngineView fully interactive —
                // masking it with an effect would cost a render pass and
                // swallow input.
                Shape {
                    id: cornerMask
                    anchors.fill: parent
                    // No corner rounding on a fullscreen video unless enabled.
                    visible: !appTheme.fullbleed
                             && (!window.mediaFullScreen || appTheme.fullScreenRounded)
                    preferredRendererType: Shape.CurveRenderer

                    ShapePath {
                        fillColor: appTheme.bg
                        strokeWidth: 0
                        fillRule: ShapePath.OddEvenFill

                        PathRectangle {
                            width: cornerMask.width
                            height: cornerMask.height
                        }
                        PathRectangle {
                            width: cornerMask.width
                            height: cornerMask.height
                            radius: appTheme.cardRadius
                        }
                    }
                }

                // Drawn after the mask so the outline sits on top of the cut
                // corners rather than being painted over by them.
                Rectangle {
                    anchors.fill: parent
                    // No border around a fullscreen video unless enabled.
                    visible: appTheme.borderWidth > 0
                             && (!window.mediaFullScreen || appTheme.fullScreenBorder)
                    radius: appTheme.cardRadius
                    color: "transparent"
                    border.color: appTheme.border
                    border.width: appTheme.borderWidth
                }
            }

            RightBar {
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.rightMargin: appTheme.pageInset
                anchors.topMargin: appTheme.pageInset
                anchors.bottomMargin: appTheme.pageInset
                width: appTheme.rightBarWidth
                visible: appTheme.rightBarEnabled && !window.mediaFullScreen
                radius: appTheme.cardRadius
                border.color: appTheme.border
                border.width: appTheme.borderWidth
                z: 3

                theme: appTheme
                activeIds: [
                    ...(appTheme.config.blockingEnabled ? ["blocking"] : []),
                    ...(findBar.visible ? ["find"] : []),
                    ...(popout.attached ? ["popout"] : []),
                    ...(pages.currentView && pages.currentView.readerActive ? ["reader"] : [])
                ]

                items: [
                    { id: "palette",  glyph: "search",  tip: "Search  ⌃K" },
                    { id: "newTab",   glyph: "add",     tip: "New tab  ⌃T" },
                    { type: "divider" },
                    { id: "extensions", glyph: "extension", tip: "Extensions",
                      badge: extensionHost.bundleCount },
                    { id: "find",     glyph: "find_in_page", tip: "Find in page  ⌃F" },
                    { id: "reader",   glyph: "chrome_reader_mode", tip: "Reader mode  F9" },
                    { id: "popout",   glyph: "picture_in_picture_alt", tip: "Pop out  ⌃⇧P" },
                    { id: "blocking", glyph: "shield",  tip: "Content blocking" },
                    { type: "spacer" },
                    { id: "downloads", glyph: "download", tip: "Downloads",
                      badge: downloadsPanel.activeCount },
                    { type: "divider" },
                    { id: "settings", glyph: "settings", tip: "Settings  ⌃," }
                ]

                onActionTriggered: id => {
                    switch (id) {
                    case "palette":  commandPalette.open(); break;
                    case "newTab":   window.openTab(appTheme.config.homeUrl); break;
                    case "settings": settingsPanel.open(); break;
                    case "extensions": extensionsPopup.showNear(null); break;
                    case "find":
                        if (findBar.visible)
                            findBar.hide();
                        else
                            findBar.show();
                        break;
                    case "popout":
                        if (popout.attached)
                            popout.returnHome();
                        else if (pages.currentView)
                            pages.currentView.popOut();
                        break;
                    case "blocking":
                        appTheme.config.setSetting("blocking.enabled",
                                                   JSON.stringify(!appTheme.config.blockingEnabled));
                        break;
                    case "downloads": downloadsPanel.toggleNear(null); break;
                    case "reader":
                        if (pages.currentView && pages.currentView.toggleReader)
                            pages.currentView.toggleReader();
                        break;
                    }
                }
            }

            FindBar {
                id: findBar

                anchors.right: parent.right
                anchors.top: parent.top
                anchors.rightMargin: (appTheme.rightBarEnabled
                                      ? appTheme.rightBarWidth + appTheme.pageInset : 0)
                                     + appTheme.pageInset + 12
                anchors.topMargin: appTheme.pageInset + 12
                z: 4

                theme: appTheme
                page: pages.currentView
            }

            // ── Edge trigger for the hidden sidebar ────────────────────
            Item {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: 8
                visible: appTheme.tabsInSidebar && tabBar.overlayMode
                z: 2

                HoverHandler { id: edgeHover }
            }

            // ── Sidebar ────────────────────────────────────────────────
            // Always positioned at the left edge. In docked modes it lines up
            // with the space the page card reserved; in overlay modes it draws
            // on top of the card.
            Sidebar {
                id: sidebar

                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.leftMargin: appTheme.pageInset
                anchors.topMargin: appTheme.pageInset
                anchors.bottomMargin: appTheme.pageInset
                width: tabBar.width
                visible: appTheme.tabsInSidebar && width > 0 && !window.mediaFullScreen
                clip: true
                z: 3

                compact: tabBar.railMode && !tabBar.revealed

                // Same card treatment as the page and the right rail.
                radius: appTheme.cardRadius
                border.color: appTheme.border
                border.width: appTheme.borderWidth

                theme: appTheme
                tabs: tabs
                currentIndex: pages.currentIndex

                onTabActivated: index => pages.currentIndex = index
                onTabClosed: index => window.closeTab(index)
                onTabMuteToggled: index => window.toggleMute(index)
                onTabsReordered: (from, to) => { tabs.move(from, to, 1); pages.currentIndex = to; }
                onNewTabRequested: window.openTab(appTheme.config.homeUrl)
                onPaletteRequested: commandPalette.open()
                onTabMenuRequested: (index, point) => tabMenu.showFor(index, point)

                HoverHandler { id: sidebarHover }

                Behavior on width {
                    NumberAnimation {
                        duration: appTheme.durNormal
                        easing.type: Easing.BezierSpline
                        easing.bezierCurve: appTheme.curveSpatial
                    }
                }
            }
        }
    }

    // Live backdrops for the start page.
    BackgroundPicker {
        id: backgroundPicker

        theme: appTheme
        onPicked: style => appTheme.config.setSetting("startPage.background",
                                                      JSON.stringify(style))
    }

    // ── Command palette ────────────────────────────────────────────────
    CommandPalette {
        id: commandPalette

        theme: appTheme
        tabs: tabs
        bookmarks: bookmarks
        history: history
        currentUrl: pages.currentView ? String(pages.currentView.url) : ""

        onTabSelected: index => pages.currentIndex = index
        onUrlSelected: url => {
            if (pages.currentView)
                pages.currentView.navigate(url);
        }
        onQuerySelected: text => {
            const target = window.resolveQuery(text);
            if (target.length > 0 && pages.currentView)
                pages.currentView.navigate(target);
        }
    }

    // ── Page context menu ──────────────────────────────────────────────
    ContextMenu {
        id: pageMenu

        property var request: null
        property var page: null

        parent: Overlay.overlay
        theme: appTheme

        function show(sourcePage, contextRequest) {
            page = sourcePage;
            request = contextRequest;

            const hasLink = String(contextRequest.linkUrl).length > 0;
            const hasMedia = String(contextRequest.mediaUrl).length > 0;
            const mediaType = contextRequest.mediaType;
            const isImage = mediaType === ContextMenuRequest.MediaTypeImage;
            const isPlayable = mediaType === ContextMenuRequest.MediaTypeVideo
                            || mediaType === ContextMenuRequest.MediaTypeAudio;
            const hasSelection = contextRequest.selectedText.length > 0;
            const editable = contextRequest.isContentEditable;
            const flags = contextRequest.editFlags;

            // Row of constantly-used actions, icon only.
            const quick = [
                { id: "back", glyph: "arrow_back", tip: "Back", enabled: sourcePage.canGoBack },
                { id: "forward", glyph: "arrow_forward", tip: "Forward", enabled: sourcePage.canGoForward },
                { id: "reload", glyph: sourcePage.loading ? "close" : "refresh",
                  tip: sourcePage.loading ? "Stop" : "Reload" },
                { id: "tab-popout", glyph: "picture_in_picture_alt", tip: "Pop out tab" },
                { id: "page-new-tab", glyph: "content_copy", tip: "Duplicate tab" }
            ];

            const columns = [];

            // Context-specific column first — it is why the menu was opened.
            const contextEntries = [];
            if (hasLink) {
                contextEntries.push({ id: "link-new-tab", label: "Open in new tab", glyph: "tab" });
                contextEntries.push({ id: "link-popout", label: "Open in popout", glyph: "picture_in_picture" });
                contextEntries.push({ id: "link-copy", label: "Copy link", glyph: "link" });
                contextEntries.push({ id: "link-copy-text", label: "Copy link text", glyph: "title",
                                      enabled: contextRequest.linkText.length > 0 });
            }
            if (hasMedia) {
                if (contextEntries.length > 0)
                    contextEntries.push({ separator: true });
                contextEntries.push({
                    id: "media-open",
                    label: isImage ? "Open image in new tab" : "Open media in new tab",
                    glyph: isImage ? "image" : "movie"
                });
                contextEntries.push({ id: "media-copy", label: "Copy media address", glyph: "content_copy" });
                if (isPlayable) {
                    contextEntries.push({ id: "media-popout", label: "Pop out media",
                                          glyph: "picture_in_picture", shortcut: "⌃⇧P" });
                }
            }
            if (hasSelection) {
                if (contextEntries.length > 0)
                    contextEntries.push({ separator: true });
                contextEntries.push({ id: "selection-copy", label: "Copy", glyph: "content_copy",
                                      shortcut: "⌃C",
                                      enabled: (flags & ContextMenuRequest.CanCopy) !== 0 });
                contextEntries.push({ id: "selection-search", label: "Search for this", glyph: "search" });
                contextEntries.push({ id: "selection-open", label: "Open as address", glyph: "open_in_new" });
            }
            if (editable) {
                if (contextEntries.length > 0)
                    contextEntries.push({ separator: true });
                contextEntries.push({ id: "edit-undo", label: "Undo", glyph: "undo", shortcut: "⌃Z",
                                      enabled: (flags & ContextMenuRequest.CanUndo) !== 0 });
                contextEntries.push({ id: "edit-redo", label: "Redo", glyph: "redo", shortcut: "⌃⇧Z",
                                      enabled: (flags & ContextMenuRequest.CanRedo) !== 0 });
                contextEntries.push({ id: "edit-cut", label: "Cut", glyph: "content_cut", shortcut: "⌃X",
                                      enabled: (flags & ContextMenuRequest.CanCut) !== 0 });
                contextEntries.push({ id: "edit-copy", label: "Copy", glyph: "content_copy", shortcut: "⌃C",
                                      enabled: (flags & ContextMenuRequest.CanCopy) !== 0 });
                contextEntries.push({ id: "edit-paste", label: "Paste", glyph: "content_paste", shortcut: "⌃V",
                                      enabled: (flags & ContextMenuRequest.CanPaste) !== 0 });
                contextEntries.push({ id: "edit-select-all", label: "Select all", glyph: "select_all",
                                      shortcut: "⌃A",
                                      enabled: (flags & ContextMenuRequest.CanSelectAll) !== 0 });
            }

            if (contextEntries.length > 0) {
                const title = hasLink ? "Link"
                            : hasMedia ? (isImage ? "Image" : "Media")
                            : editable ? "Edit" : "Selection";
                columns.push({ title: title, entries: contextEntries });
            }

            columns.push({
                title: "Page",
                entries: [
                    { id: "page-copy-url", label: "Copy address", glyph: "link" },
                    { id: "page-find", label: "Find in page", glyph: "search", shortcut: "⌃F" },
                    { id: "page-zoom-in", label: "Zoom in", glyph: "zoom_in", shortcut: "⌃+" },
                    { id: "page-zoom-out", label: "Zoom out", glyph: "zoom_out", shortcut: "⌃-" },
                    { id: "page-zoom-reset", label: "Reset zoom", glyph: "restart_alt", shortcut: "⌃0" },
                    { separator: true },
                    { id: "page-view-source", label: "View source", glyph: "code" },
                    { id: "page-save", label: "Save page", glyph: "download", shortcut: "⌃S" },
                    { id: "page-print", label: "Print", glyph: "print", shortcut: "⌃P" }
                ]
            });

            const point = sourcePage.mapToItem(Overlay.overlay,
                                               contextRequest.position.x,
                                               contextRequest.position.y);
            open2(point.x, point.y, quick, columns);
        }

        onTriggered: id => {
            if (!request || !page)
                return;
            const view = page.webView;

            switch (id) {
            case "link-new-tab":     window.openTab(String(request.linkUrl)); break;
            case "link-popout":
                window.openTab(String(request.linkUrl));
                pages.currentView.popOut();
                break;
            case "link-copy":        shell.copyText(String(request.linkUrl)); break;
            case "link-copy-text":   shell.copyText(request.linkText); break;

            case "media-open":       window.openTab(String(request.mediaUrl)); break;
            case "media-copy":       shell.copyText(String(request.mediaUrl)); break;
            case "media-popout":
            case "tab-popout":       page.popOut(); break;

            case "selection-copy":
            case "edit-copy":        shell.copyText(request.selectedText); break;
            case "selection-search":
                window.openTab(window.resolveQuery(request.selectedText));
                break;
            case "selection-open":
                window.openTab(window.resolveQuery(request.selectedText.trim()));
                break;

            case "edit-undo":        if (view) view.triggerWebAction(WebEngineView.Undo); break;
            case "edit-redo":        if (view) view.triggerWebAction(WebEngineView.Redo); break;
            case "edit-cut":         if (view) view.triggerWebAction(WebEngineView.Cut); break;
            case "edit-paste":       if (view) view.triggerWebAction(WebEngineView.Paste); break;
            case "edit-select-all":  if (view) view.triggerWebAction(WebEngineView.SelectAll); break;

            case "back":             page.goBack(); break;
            case "forward":          page.goForward(); break;
            case "reload":
                if (page.loading)
                    page.stop();
                else
                    page.reload();
                break;

            case "page-copy-url":    shell.copyText(String(page.url)); break;
            case "page-new-tab":     window.openTab(String(page.url)); break;
            case "page-find":        findBar.show(); break;
            case "page-zoom-in":     if (view) view.zoomFactor = Math.min(5, view.zoomFactor + 0.1); break;
            case "page-zoom-out":    if (view) view.zoomFactor = Math.max(0.25, view.zoomFactor - 0.1); break;
            case "page-zoom-reset":  if (view) view.zoomFactor = 1.0; break;
            case "page-view-source": if (view) view.triggerWebAction(WebEngineView.ViewSource); break;
            case "page-save":        if (view) view.triggerWebAction(WebEngineView.SavePage); break;
            case "page-print":       if (view) view.printToPdf(""); break;
            }
        }
    }

    // ── Tab context menu ───────────────────────────────────────────────
    ContextMenu {
        id: tabMenu

        property int targetIndex: -1

        parent: Overlay.overlay
        theme: appTheme

        function showFor(index, globalPoint) {
            targetIndex = index;
            const only = tabs.count <= 1;

            open2(globalPoint.x, globalPoint.y, [
                { id: "reload", glyph: "refresh", tip: "Reload" },
                { id: "duplicate", glyph: "content_copy", tip: "Duplicate" },
                { id: "popout", glyph: "picture_in_picture_alt", tip: "Pop out" },
                { id: "close", glyph: "close", tip: "Close", enabled: !only }
            ], [{
                title: "Tab",
                entries: [
                    { id: "reload", label: "Reload", glyph: "refresh", shortcut: "⌃R" },
                    { id: "duplicate", label: "Duplicate", glyph: "content_copy" },
                    { id: "popout", label: "Pop out", glyph: "picture_in_picture_alt" },
                    { id: "copy-url", label: "Copy address", glyph: "link" },
                    { separator: true },
                    { id: "move-start", label: "Move to top", glyph: "vertical_align_top",
                      enabled: index > 0 },
                    { id: "move-end", label: "Move to bottom", glyph: "vertical_align_bottom",
                      enabled: index < tabs.count - 1 }
                ]
            }, {
                title: "Close",
                entries: [
                    { id: "close", label: "Close tab", glyph: "close", shortcut: "⌃W", enabled: !only },
                    { id: "close-others", label: "Close other tabs", glyph: "clear_all", enabled: !only },
                    { id: "close-right", label: "Close tabs below", glyph: "arrow_downward",
                      enabled: index < tabs.count - 1 }
                ]
            }]);
        }

        onTriggered: id => {
            const index = targetIndex;
            if (index < 0 || index >= tabs.count)
                return;
            const url = String(tabs.get(index).pageUrl);

            switch (id) {
            case "reload":
                if (index === pages.currentIndex && pages.currentView)
                    pages.currentView.reload();
                break;
            case "duplicate":   window.openTab(url); break;
            case "popout":
                pages.currentIndex = index;
                if (pages.currentView)
                    pages.currentView.popOut();
                break;
            case "copy-url":    shell.copyText(url); break;
            case "close":       window.closeTab(index); break;
            case "move-start":  tabs.move(index, 0, 1); pages.currentIndex = 0; break;
            case "move-end":
                tabs.move(index, tabs.count - 1, 1);
                pages.currentIndex = tabs.count - 1;
                break;
            case "close-others":
                for (let i = tabs.count - 1; i >= 0; i--) {
                    if (i !== index)
                        tabs.remove(i);
                }
                pages.currentIndex = 0;
                break;
            case "close-right":
                // Remove from the end so earlier indices stay valid.
                for (let i = tabs.count - 1; i > index; i--)
                    tabs.remove(i);
                pages.currentIndex = Math.min(pages.currentIndex, tabs.count - 1);
                break;
            }
        }
    }

    // ── Media popout ───────────────────────────────────────────────────
    MediaPopout {
        id: popout

        theme: appTheme

        // Where the view came from, so it can be put back exactly.
        property var homeHost: null

        function attach(view, host, label) {
            homeHost = host;
            attached = view;
            title = label && label.length > 0 ? label : "Tela — popout";
            if (!hostItem) {
                console.warn("popout has no host item; refusing to detach view");
                return;
            }
            view.parent = hostItem;
            show();
            raise();
        }

        function returnHome() {
            if (attached && homeHost)
                attached.parent = homeHost;
            attached = null;
            homeHost = null;
            hide();
        }

        onReturnRequested: returnHome()
        onClosing: returnHome()
    }

    SiteInfoPopup {
        id: siteInfo

        parent: Overlay.overlay
        theme: appTheme
        page: pages.currentView
        browserProfile: browserProfile
        config: appTheme.config

        // Hangs from the left edge of the indicators it belongs to, the way
        // Firefox drops its site panel under the padlock.
        function showFor(mode, item) {
            siteInfo.mode = mode;
            if (item) {
                const point = item.mapToItem(Overlay.overlay, 0, item.height);
                x = Math.max(8, Math.min(point.x, Overlay.overlay.width - width - 8));
                y = point.y + 6;
            } else {
                x = (Overlay.overlay.width - width) / 2;
                y = 60;
            }
            open();
        }
    }

    ExtensionsPopup {
        id: extensionsPopup

        parent: Overlay.overlay
        theme: appTheme
        extensions: extensionHost

        function showNear(item) {
            if (!item) {
                x = (Overlay.overlay.width - width) / 2;
                y = 60;
            } else {
                const point = item.mapToItem(Overlay.overlay, 0, item.height);
                // Anchor to the button's right edge so the panel hangs inward
                // from the rail rather than off the screen.
                x = Math.max(8, Math.min(point.x - width + item.width,
                                         Overlay.overlay.width - width - 8));
                y = point.y + 6;
            }
            open();
        }

        onManageRequested: {
            settingsPanel.section = 4;
            settingsPanel.open();
        }
        onConfigureRequested: id => {
            settingsPanel.section = 4;
            settingsPanel.open();
        }

        onActionRequested: (id, popup, name) => {
            close();
            actionPopup.show(id, popup, name);
        }
    }

    ExtensionActionPopup {
        id: actionPopup

        browserProfile: browserProfile

        // Kept in the window's own content item rather than Overlay.overlay:
        // the overlay is the layer a WebEngineView cannot render into.
        parent: window.contentItem
        theme: appTheme
        extensions: extensionHost
        channel: extensionChannel

        onNewTabRequested: url => window.openTab(url)
    }

    DownloadsPanel {
        id: downloadsPanel
        theme: appTheme
    }

    HistoryPanel {
        id: historyPanel
        parent: Overlay.overlay
        theme: appTheme
        history: history

        onUrlSelected: url => window.openTab(url)
    }

    Settings {
        id: settingsPanel

        onOpenExtensionOptions: function (id, page) {
            // Extension pages are served by the scheme handler, so this is an
            // ordinary navigation — no special surface required.
            window.openTab("tela-extension://" + id + "/" + page);
        }
        theme: appTheme
        extensions: extensionHost
    }

    component NavButton: Rectangle {
        id: navButton

        required property var theme
        required property string label
        required property bool active

        // Carries the modifiers held at the time, so a caller can tell a
        // plain click from a shift-click.
        signal triggered(int modifiers)

        implicitWidth: 30
        implicitHeight: 30
        radius: appTheme.roundingSmall
        color: navHover.hovered && active ? appTheme.surfaceHover : "transparent"
        opacity: active ? 1.0 : 0.3

        Behavior on color {
            ColorAnimation {
                duration: appTheme.durFast
                easing.type: Easing.BezierSpline
                easing.bezierCurve: appTheme.curveEffects
            }
        }

        HoverHandler { id: navHover; enabled: navButton.active }
        TapHandler {
            enabled: navButton.active
            onTapped: (point) => navButton.triggered(point.modifiers ?? Qt.NoModifier)
        }

        Label {
            anchors.centerIn: parent
            text: navButton.label
            color: navButton.theme.text
            font.family: navButton.theme.fontIcon
            font.pixelSize: navButton.theme.iconSize
        }
    }
}
