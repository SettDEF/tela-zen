import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

pragma ComponentBehavior: Bound

// In-app settings editor.
//
// Every control writes straight back to ~/.config/tela/config.json through
// Config.setSetting, so the file stays the single source of truth — editing it
// by hand and editing it here are the same operation.
Popup {
    id: root

    required property var theme
    required property var extensions

    property int section: 0

    // Read once when Settings opens; flipped locally when the user sets it, so
    // the button settles without re-querying.
    property bool telaIsDefault: root.theme.config.isDefaultBrowser()

    /// Asks the window to open an extension's own settings page in a tab.
    /// Emitted rather than navigated here so Settings stays a view and the
    /// window keeps sole responsibility for tabs.
    signal openExtensionOptions(string id, string page)
    readonly property var profiles: {
        try {
            return JSON.parse(root.theme.config.profileListJson);
        } catch (error) {
            return ["default"];
        }
    }

    function createProfile(name) {
        const trimmed = String(name).trim();
        if (trimmed.length > 0)
            root.theme.config.openProfile(trimmed);
    }

    readonly property var sections: ["Appearance", "Tabs", "Behaviour", "Blocking", "Extensions", "Profiles"]

    anchors.centerIn: Overlay.overlay
    width: Math.min(880, Overlay.overlay ? Overlay.overlay.width - 80 : 880)
    height: Math.min(600, Overlay.overlay ? Overlay.overlay.height - 80 : 600)
    padding: 0
    modal: true
    focus: true

    function write(path, value) {
        root.theme.config.setSetting(path, JSON.stringify(value));
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

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // ── Section nav ────────────────────────────────────────────────
        Rectangle {
            Layout.preferredWidth: 200
            Layout.fillHeight: true
            color: root.theme.sidebar
            topLeftRadius: root.theme.roundingLarge
            bottomLeftRadius: root.theme.roundingLarge

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 12
                spacing: 4

                Label {
                    Layout.fillWidth: true
                    Layout.bottomMargin: 8
                    Layout.leftMargin: 8
                    text: "Settings"
                    color: root.theme.text
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontLarge
                    font.weight: Font.DemiBold
                }

                Repeater {
                    model: root.sections

                    delegate: Rectangle {
                        id: navItem

                        required property int index
                        required property string modelData

                        Layout.fillWidth: true
                        Layout.preferredHeight: 34
                        radius: root.theme.rounding
                        color: index === root.section ? root.theme.accentContainer
                             : navHover.hovered ? root.theme.sidebarHover
                             : "transparent"

                        Behavior on color { ColorAnimation { duration: root.theme.durFast } }

                        HoverHandler { id: navHover }
                        TapHandler { onTapped: root.section = navItem.index }

                        Label {
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.left: parent.left
                            anchors.leftMargin: 12
                            text: navItem.modelData
                            color: navItem.index === root.section ? root.theme.onAccentContainer
                                                                  : root.theme.textDim
                            font.family: root.theme.fontMain
                            font.pixelSize: root.theme.fontSmall
                        }
                    }
                }

                Item { Layout.fillHeight: true }

                Label {
                    Layout.fillWidth: true
                    Layout.leftMargin: 8
                    text: root.theme.config.configDir
                    color: root.theme.textFaint
                    font.family: root.theme.fontMono
                    font.pixelSize: root.theme.fontSmallest
                    elide: Text.ElideMiddle
                }
            }
        }

        // ── Section body ───────────────────────────────────────────────
        Flickable {
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: body.implicitHeight + 32
            clip: true

            ColumnLayout {
                id: body
                width: parent.width
                x: 0
                y: 16
                spacing: 2

                // ── Appearance ─────────────────────────────────────────
                SettingGroup { visible: root.section === 0; theme: root.theme; title: "Palette" }
                SettingToggle {
                    visible: root.section === 0
                    theme: root.theme
                    label: "Follow desktop colours"
                    detail: "Track the system Material You palette instead of Tela's own theme file"
                    value: root.theme.config.followSystemColors
                    onToggled: v => root.write("appearance.followSystemColors", v)
                }
                SettingSlider {
                    visible: root.section === 0
                    theme: root.theme
                    label: "Icon size"
                    from: 12; to: 32
                    value: root.theme.iconSize
                    onCommitted: v => root.write("appearance.iconSize", v)
                }
                SettingSlider {
                    visible: root.section === 0
                    theme: root.theme
                    label: "Corner radius"
                    from: 0; to: 32
                    value: root.theme.rounding
                    onCommitted: v => root.write("appearance.rounding.normal", v)
                }

                SettingGroup { visible: root.section === 0; theme: root.theme; title: "Frame" }
                SettingSlider {
                    visible: root.section === 0
                    theme: root.theme
                    label: "Content margin & panel gap"
                    detail: "The gap around the page and between the panels — one value. 0 makes content fullbleed with no rounded card"
                    from: 0; to: 48
                    value: root.theme.pageInset
                    onCommitted: v => root.write("appearance.pageInset", v)
                }
                SettingSlider {
                    visible: root.section === 0
                    theme: root.theme
                    label: "Fullscreen video margin"
                    detail: "Gap around a video in fullscreen. 0 fills the whole screen"
                    from: 0; to: 120
                    value: root.theme.fullScreenGap
                    onCommitted: v => root.write("appearance.fullScreenGap", v)
                }
                SettingToggle {
                    visible: root.section === 0
                    theme: root.theme
                    label: "Rounded corners in fullscreen"
                    detail: "Round the fullscreen video's corners"
                    value: root.theme.fullScreenRounded
                    onToggled: v => root.write("appearance.fullScreenRounded", v)
                }
                SettingToggle {
                    visible: root.section === 0
                    theme: root.theme
                    label: "Border in fullscreen"
                    detail: "Keep the panel border around the fullscreen video"
                    value: root.theme.fullScreenBorder
                    onToggled: v => root.write("appearance.fullScreenBorder", v)
                }
                SettingToggle {
                    visible: root.section === 0
                    theme: root.theme
                    label: "Panel borders"
                    value: root.theme.borderEnabled
                    onToggled: v => root.write("appearance.border.enabled", v)
                }
                SettingToggle {
                    visible: root.section === 0
                    theme: root.theme
                    label: "Right action bar"
                    value: root.theme.rightBarEnabled
                    onToggled: v => root.write("appearance.rightBar.enabled", v)
                }
                SettingSlider {
                    visible: root.section === 0
                    theme: root.theme
                    label: "Animation speed"
                    detail: "Percent of normal. 0 disables all transitions"
                    from: 0; to: 200
                    value: root.theme.config.animationSpeed
                    onCommitted: v => root.write("appearance.animationSpeed", v)
                }

                // ── Tabs ───────────────────────────────────────────────
                SettingGroup { visible: root.section === 1; theme: root.theme; title: "Placement" }
                SettingChoice {
                    visible: root.section === 1
                    theme: root.theme
                    label: "Tab bar"
                    options: [
                        { value: "sidebar",         label: "Sidebar" },
                        { value: "sidebar-compact", label: "Rail" },
                        { value: "sidebar-auto",    label: "Hidden" },
                        { value: "top",             label: "Top" }
                    ]
                    value: root.theme.tabBarMode
                    onPicked: v => root.write("appearance.tabBar.mode", v)
                }
                SettingSlider {
                    visible: root.section === 1
                    theme: root.theme
                    label: "Sidebar width"
                    from: 180; to: 420
                    value: root.theme.sidebarWidth
                    onCommitted: v => root.write("appearance.sidebarWidth", v)
                }
                SettingToggle {
                    visible: root.section === 1
                    theme: root.theme
                    label: "Shy chrome"
                    detail: "Hide the toolbar until the pointer comes near"
                    value: root.theme.shyChrome
                    onToggled: v => root.write("appearance.tabBar.shyChrome", v)
                }

                SettingGroup { visible: root.section === 1; theme: root.theme; title: "Rows" }
                SettingSlider {
                    visible: root.section === 1
                    theme: root.theme
                    label: "Tab height"
                    from: 24; to: 56
                    value: root.theme.rowHeight
                    onCommitted: v => root.write("tabs.height", v)
                }
                SettingToggle {
                    visible: root.section === 1
                    theme: root.theme
                    label: "Show favicons"
                    value: root.theme.showFavicons
                    onToggled: v => root.write("tabs.showFavicons", v)
                }
                SettingChoice {
                    visible: root.section === 1
                    theme: root.theme
                    label: "Close button"
                    options: [
                        { value: "hover",  label: "On hover" },
                        { value: "always", label: "Always" },
                        { value: "never",  label: "Never" }
                    ]
                    value: root.theme.closeButtonMode
                    onPicked: v => root.write("tabs.closeButton", v)
                }
                SettingSlider {
                    visible: root.section === 1
                    theme: root.theme
                    label: "Auto-archive after"
                    detail: "Days a tab can sit untouched before it leaves the strip. 0 disables"
                    from: 0; to: 90
                    value: root.theme.autoArchiveDays
                    onCommitted: v => root.write("tabs.autoArchiveDays", v)
                }

                // ── Behaviour ──────────────────────────────────────────
                SettingGroup { visible: root.section === 2; theme: root.theme; title: "Startup" }
                SettingToggle {
                    visible: root.section === 2
                    theme: root.theme
                    label: "Restore previous session"
                    value: root.theme.restoreSession
                    onToggled: v => root.write("behavior.restoreSession", v)
                }
                SettingText {
                    visible: root.section === 2
                    theme: root.theme
                    label: "Home page"
                    value: root.theme.config.homeUrl
                    onCommitted: v => root.write("behavior.homeUrl", v)
                }
                SettingText {
                    visible: root.section === 2
                    theme: root.theme
                    label: "Search"
                    detail: "{query} is replaced with the encoded search terms"
                    value: root.theme.config.searchTemplate
                    onCommitted: v => root.write("behavior.searchTemplate", v)
                }

                SettingGroup {
                    visible: root.section === 2
                    theme: root.theme
                    title: "System"
                }
                SettingButton {
                    visible: root.section === 2
                    theme: root.theme
                    label: "Default browser"
                    detail: root.telaIsDefault
                            ? "Tela opens web links from other apps."
                            : "Make Tela handle web links and http/https."
                    // Once Tela is default the action is done, so show a settled
                    // state rather than a button that would re-run to no effect.
                    done: root.telaIsDefault
                    doneText: "Default ✓"
                    buttonText: "Set as default"
                    onClicked: {
                        if (root.theme.config.setAsDefaultBrowser())
                            root.telaIsDefault = true;
                    }
                }

                // ── Blocking ───────────────────────────────────────────
                SettingGroup { visible: root.section === 3; theme: root.theme; title: "Content blocking" }
                SettingToggle {
                    visible: root.section === 3
                    theme: root.theme
                    label: "Block ads and trackers"
                    detail: "Uses EasyList/uBlock filter syntax, matched natively"
                    value: root.theme.config.blockingEnabled
                    onToggled: v => root.write("blocking.enabled", v)
                }

                // ── Extensions ─────────────────────────────────────────
                SettingGroup {
                    visible: root.section === 4
                    theme: root.theme
                    title: "Installed extensions"
                }

                Repeater {
                    model: {
                        if (root.section !== 4)
                            return [];
                        try {
                            return JSON.parse(root.extensions.bundlesJson);
                        } catch (e) {
                            return [];
                        }
                    }

                    delegate: Item {
                        id: bundleRow

                        required property var modelData

                        visible: root.section === 4
                        Layout.fillWidth: true
                        Layout.leftMargin: 24
                        Layout.rightMargin: 24
                        implicitHeight: bundleBody.implicitHeight + 18

                        RowLayout {
                            id: bundleBody
                            anchors.fill: parent
                            anchors.topMargin: 9
                            anchors.bottomMargin: 9
                            spacing: 12

                            Item {
                                Layout.preferredWidth: 30
                                Layout.preferredHeight: 30
                                Layout.alignment: Qt.AlignTop

                                Image {
                                    id: bundleIcon
                                    anchors.fill: parent
                                    source: bundleRow.modelData.icon ?? ""
                                    visible: status === Image.Ready
                                    sourceSize.width: 64
                                    sourceSize.height: 64
                                    smooth: true
                                    opacity: bundleRow.modelData.enabled ? 1 : 0.4
                                }
                                Rectangle {
                                    anchors.fill: parent
                                    visible: !bundleIcon.visible
                                    radius: 8
                                    color: root.theme.accentContainer
                                    opacity: bundleRow.modelData.enabled ? 1 : 0.4

                                    Label {
                                        anchors.centerIn: parent
                                        text: (bundleRow.modelData.name || "?").charAt(0).toUpperCase()
                                        color: root.theme.onAccentContainer
                                        font.family: root.theme.fontMain
                                        font.pixelSize: root.theme.fontSmall
                                        font.weight: Font.DemiBold
                                    }
                                }
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 3

                                Label {
                                    Layout.fillWidth: true
                                    text: (bundleRow.modelData.name ?? "")
                                          + "  v" + (bundleRow.modelData.version ?? "")
                                    color: root.theme.text
                                    font.family: root.theme.fontMain
                                    font.pixelSize: root.theme.fontNormal
                                    elide: Text.ElideRight
                                }

                                Label {
                                    Layout.fillWidth: true
                                    text: bundleRow.modelData.hostAccess ?? ""
                                    color: root.theme.textDim
                                    font.family: root.theme.fontMain
                                    font.pixelSize: root.theme.fontSmallest
                                    wrapMode: Text.WordWrap
                                }

                                // What the browser actually enforces, as opposed
                                // to what the manifest asks for.
                                Label {
                                    Layout.fillWidth: true
                                    visible: (bundleRow.modelData.apiAccess ?? "") !== ""
                                    text: "APIs: " + (bundleRow.modelData.apiAccess ?? "")
                                    color: root.theme.textDim
                                    font.family: root.theme.fontMain
                                    font.pixelSize: root.theme.fontSmallest
                                    wrapMode: Text.WordWrap
                                }

                                // Extensions like uBlock keep their real settings
                                // on their own page; without this there is no way
                                // to reach it.
                                //
                                // Hand-styled like every other control here. It
                                // was a bare Quick Controls Button — the one
                                // unstyled thing in this panel — which renders as
                                // a white slab against a dark theme.
                                Rectangle {
                                    id: prefsButton

                                    Layout.topMargin: 6
                                    Layout.bottomMargin: 2
                                    visible: (bundleRow.modelData.optionsPage ?? "") !== ""
                                    implicitWidth: prefsRow.implicitWidth + 22
                                    implicitHeight: 28
                                    radius: root.theme.roundingSmall
                                    color: prefsHover.hovered ? root.theme.surfaceActive
                                                              : root.theme.surfaceHigh
                                    border.color: root.theme.border
                                    border.width: root.theme.borderWidth

                                    Behavior on color {
                                        ColorAnimation { duration: root.theme.durFast }
                                    }

                                    HoverHandler {
                                        id: prefsHover
                                        cursorShape: Qt.PointingHandCursor
                                    }
                                    TapHandler {
                                        onTapped: root.openExtensionOptions(
                                            bundleRow.modelData.id,
                                            bundleRow.modelData.optionsPage)
                                    }

                                    RowLayout {
                                        id: prefsRow
                                        anchors.centerIn: parent
                                        spacing: 6

                                        Label {
                                            text: "tune"
                                            color: root.theme.textDim
                                            font.family: root.theme.fontIcon
                                            font.pixelSize: root.theme.iconSizeSmall
                                        }
                                        Label {
                                            text: "Preferences"
                                            color: root.theme.text
                                            font.family: root.theme.fontMain
                                            font.pixelSize: root.theme.fontSmall
                                        }
                                    }
                                }

                                // The compatibility report, spelled out. This is
                                // the difference between "it does not work" and
                                // knowing exactly which API is absent.
                                Repeater {
                                    model: bundleRow.modelData.issues ?? []

                                    delegate: Label {
                                        required property var modelData

                                        Layout.fillWidth: true
                                        text: (modelData.blocking ? "✕ " : "! ")
                                              + modelData.feature + " — " + modelData.detail
                                        color: modelData.blocking ? root.theme.error
                                                                  : root.theme.tertiary
                                        font.family: root.theme.fontMain
                                        font.pixelSize: root.theme.fontSmallest
                                        wrapMode: Text.WordWrap
                                    }
                                }

                                Label {
                                    Layout.fillWidth: true
                                    visible: (bundleRow.modelData.contentScripts ?? 0) > 0
                                             || bundleRow.modelData.hasBackground
                                    text: [
                                        (bundleRow.modelData.contentScripts ?? 0) > 0
                                            ? bundleRow.modelData.contentScripts + " content scripts" : "",
                                        bundleRow.modelData.hasBackground ? "background context" : ""
                                    ].filter(Boolean).join(" · ")
                                    color: root.theme.textFaint
                                    font.family: root.theme.fontMono
                                    font.pixelSize: root.theme.fontSmallest
                                }
                            }

                            // Enable toggle, mirroring the userscript rows.
                            Rectangle {
                                Layout.alignment: Qt.AlignTop
                                Layout.topMargin: 2
                                width: 46; height: 26; radius: 13
                                color: bundleRow.modelData.enabled ? root.theme.accent
                                                                   : root.theme.surfaceHover
                                border.color: root.theme.border
                                border.width: 1
                                opacity: bundleRow.modelData.loadable ? 1 : 0.4

                                Behavior on color { ColorAnimation { duration: root.theme.durFast } }

                                TapHandler {
                                    enabled: bundleRow.modelData.loadable
                                    onTapped: root.extensions.setEnabled(bundleRow.modelData.id,
                                                                        !bundleRow.modelData.enabled)
                                }

                                Rectangle {
                                    y: 3
                                    x: bundleRow.modelData.enabled ? parent.width - width - 3 : 3
                                    width: 20; height: 20; radius: 10
                                    color: bundleRow.modelData.enabled ? root.theme.onAccent
                                                                       : root.theme.textDim

                                    Behavior on x {
                                        NumberAnimation {
                                            duration: root.theme.durFast
                                            easing.type: Easing.BezierSpline
                                            easing.bezierCurve: root.theme.curveSpatial
                                        }
                                    }
                                }
                            }
                        }

                        Rectangle {
                            anchors.bottom: parent.bottom
                            anchors.left: parent.left
                            anchors.right: parent.right
                            height: 1
                            color: root.theme.border
                            opacity: 0.5
                        }
                    }
                }

                SettingGroup { visible: root.section === 5; theme: root.theme; title: "Profiles" }

                Label {
                    visible: root.section === 5
                    Layout.fillWidth: true
                    Layout.bottomMargin: 8
                    text: "Each profile keeps its own cookies, history, extensions and "
                        + "settings. Opening one starts a separate window."
                    color: root.theme.textDim
                    font.family: root.theme.fontMain
                    font.pixelSize: root.theme.fontSmall
                    wrapMode: Text.WordWrap
                }

                Repeater {
                    model: root.section === 5 ? root.profiles : []

                    delegate: Rectangle {
                        id: profileRow

                        required property string modelData

                        readonly property bool current:
                            profileRow.modelData === (root.theme.config.profileName.length > 0
                                                      ? root.theme.config.profileName : "default")

                        Layout.fillWidth: true
                        Layout.preferredHeight: 44
                        radius: root.theme.roundingSmall
                        color: profileHover.hovered ? root.theme.surfaceHover : "transparent"

                        HoverHandler { id: profileHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler {
                            enabled: !profileRow.current
                            onTapped: root.theme.config.openProfile(profileRow.modelData)
                        }

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 12
                            spacing: 10

                            Label {
                                text: profileRow.current ? "person" : "person_outline"
                                color: profileRow.current ? root.theme.accent : root.theme.textDim
                                font.family: root.theme.fontIcon
                                font.pixelSize: root.theme.iconSize
                            }
                            Label {
                                Layout.fillWidth: true
                                text: profileRow.modelData
                                color: root.theme.text
                                font.family: root.theme.fontMain
                                font.pixelSize: root.theme.fontNormal
                            }
                            Label {
                                text: profileRow.current ? "In use" : "Open"
                                color: root.theme.textFaint
                                font.family: root.theme.fontMain
                                font.pixelSize: root.theme.fontSmallest
                            }
                        }
                    }
                }

                RowLayout {
                    visible: root.section === 5
                    Layout.fillWidth: true
                    Layout.topMargin: 10
                    spacing: 8

                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: 34
                        radius: root.theme.roundingSmall
                        color: root.theme.surface
                        border.color: root.theme.border
                        border.width: root.theme.borderWidth

                        TextField {
                            id: newProfile
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            placeholderText: "New profile name"
                            color: root.theme.text
                            placeholderTextColor: root.theme.textFaint
                            font.family: root.theme.fontMain
                            font.pixelSize: root.theme.fontSmall
                            background: null
                            onAccepted: root.createProfile(text)
                        }
                    }

                    Rectangle {
                        implicitWidth: 96
                        implicitHeight: 34
                        radius: root.theme.roundingSmall
                        color: createHover.hovered ? root.theme.accent : root.theme.accentContainer

                        HoverHandler { id: createHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: root.createProfile(newProfile.text) }

                        Label {
                            anchors.centerIn: parent
                            text: "Create"
                            color: createHover.hovered ? root.theme.onAccent
                                                       : root.theme.onAccentContainer
                            font.family: root.theme.fontMain
                            font.pixelSize: root.theme.fontSmall
                        }
                    }
                }

                SettingGroup { visible: root.section === 4; theme: root.theme; title: "Userscripts" }

                SettingRow {
                    visible: root.section === 4
                    theme: root.theme
                    label: root.extensions.count === 0
                           ? "No scripts installed"
                           : root.extensions.enabledCount + " of " + root.extensions.count + " enabled"
                    detail: root.extensions.scriptsDir

                    Rectangle {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        implicitWidth: importLabel.implicitWidth + 28
                        implicitHeight: 30
                        radius: root.theme.roundingSmall
                        color: importHover.hovered ? root.theme.accent : root.theme.accentContainer

                        Behavior on color { ColorAnimation { duration: root.theme.durFast } }

                        HoverHandler { id: importHover }
                        TapHandler {
                            onTapped: {
                                const added = root.extensions.importFromDownloads();
                                importLabel.flash = added > 0
                                    ? "Imported " + added
                                    : "None found";
                                flashTimer.restart();
                            }
                        }

                        Label {
                            id: importLabel
                            property string flash: ""
                            anchors.centerIn: parent
                            text: flash.length > 0 ? flash : "Import from Downloads"
                            color: importHover.hovered ? root.theme.onAccent
                                                       : root.theme.onAccentContainer
                            font.family: root.theme.fontMain
                            font.pixelSize: root.theme.fontSmall
                        }

                        Timer {
                            id: flashTimer
                            interval: 2000
                            onTriggered: importLabel.flash = ""
                        }
                    }
                }

                Repeater {
                    model: {
                        if (root.section !== 4)
                            return [];
                        try {
                            return JSON.parse(root.extensions.listJson);
                        } catch (e) {
                            return [];
                        }
                    }

                    delegate: SettingToggle {
                        required property var modelData

                        visible: root.section === 4
                        theme: root.theme
                        label: modelData.name + (modelData.version ? "  v" + modelData.version : "")
                        detail: modelData.description
                                ?? (modelData.grants && modelData.grants.length > 0
                                    ? "Grants: " + modelData.grants.join(", ")
                                    : "")
                        value: modelData.enabled
                        onToggled: v => root.extensions.setEnabled(modelData.id, v)
                    }
                }
            }
        }
    }

    Keys.onEscapePressed: root.close()

    // ── Row components ─────────────────────────────────────────────────

    component SettingGroup: Label {
        required property var theme
        required property string title

        Layout.fillWidth: true
        Layout.leftMargin: 24
        Layout.topMargin: 20
        Layout.bottomMargin: 4
        text: title.toUpperCase()
        color: theme.textFaint
        font.family: theme.fontMain
        font.pixelSize: theme.fontSmallest
        font.weight: Font.DemiBold
        font.letterSpacing: 0.8
    }

    component SettingRow: Item {
        required property var theme
        required property string label
        property string detail: ""

        default property alias content: control.data

        Layout.fillWidth: true
        Layout.leftMargin: 24
        Layout.rightMargin: 24
        implicitHeight: Math.max(44, text.implicitHeight + 20)

        RowLayout {
            anchors.fill: parent
            spacing: 16

            ColumnLayout {
                id: text
                Layout.fillWidth: true
                spacing: 2

                Label {
                    Layout.fillWidth: true
                    text: label
                    color: theme.text
                    font.family: theme.fontMain
                    font.pixelSize: theme.fontNormal
                }
                Label {
                    Layout.fillWidth: true
                    visible: detail.length > 0
                    text: detail
                    color: theme.textFaint
                    font.family: theme.fontMain
                    font.pixelSize: theme.fontSmallest
                    wrapMode: Text.WordWrap
                }
            }

            Item {
                id: control
                Layout.preferredWidth: 240
                Layout.preferredHeight: 32
                Layout.alignment: Qt.AlignVCenter
            }
        }
    }

    component SettingButton: SettingRow {
        id: buttonRow
        property string buttonText: ""
        property string doneText: ""
        property bool done: false
        signal clicked()

        Rectangle {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            implicitWidth: buttonRowLabel.implicitWidth + 28
            width: implicitWidth
            height: 32
            radius: buttonRow.theme.roundingSmall
            color: buttonRow.done
                   ? "transparent"
                   : (buttonHover.hovered ? buttonRow.theme.accent
                                          : buttonRow.theme.accentContainer)
            border.color: buttonRow.done ? buttonRow.theme.border : "transparent"
            border.width: 1

            HoverHandler { id: buttonHover; enabled: !buttonRow.done }
            TapHandler {
                enabled: !buttonRow.done
                onTapped: buttonRow.clicked()
            }

            Label {
                id: buttonRowLabel
                anchors.centerIn: parent
                text: buttonRow.done ? buttonRow.doneText : buttonRow.buttonText
                color: buttonRow.done
                       ? buttonRow.theme.textDim
                       : (buttonHover.hovered ? buttonRow.theme.onAccent
                                              : buttonRow.theme.onAccentContainer)
                font.family: buttonRow.theme.fontMain
                font.pixelSize: buttonRow.theme.fontSmall
                font.weight: Font.DemiBold
            }
        }
    }

    component SettingToggle: SettingRow {
        id: toggleRow
        required property bool value
        signal toggled(bool value)

        Rectangle {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: 46; height: 26; radius: 13
            color: toggleRow.value ? toggleRow.theme.accent : toggleRow.theme.surfaceHover
            border.color: toggleRow.theme.border
            border.width: 1

            Behavior on color { ColorAnimation { duration: toggleRow.theme.durFast } }

            TapHandler { onTapped: toggleRow.toggled(!toggleRow.value) }

            Rectangle {
                y: 3
                x: toggleRow.value ? parent.width - width - 3 : 3
                width: 20; height: 20; radius: 10
                color: toggleRow.value ? toggleRow.theme.onAccent : toggleRow.theme.textDim

                Behavior on x {
                    NumberAnimation {
                        duration: toggleRow.theme.durFast
                        easing.type: Easing.BezierSpline
                        easing.bezierCurve: toggleRow.theme.curveSpatial
                    }
                }
            }
        }
    }

    component SettingSlider: SettingRow {
        id: sliderRow
        required property int value
        property int from: 0
        property int to: 100
        signal committed(int value)

        RowLayout {
            anchors.fill: parent
            spacing: 10

            Slider {
                id: slider
                Layout.fillWidth: true
                from: sliderRow.from
                to: sliderRow.to
                stepSize: 1
                value: sliderRow.value
                // Commit on release only: writing per pixel would rewrite the
                // config file dozens of times per drag.
                onPressedChanged: if (!pressed) sliderRow.committed(Math.round(value))

                background: Rectangle {
                    x: slider.leftPadding
                    y: slider.topPadding + slider.availableHeight / 2 - height / 2
                    width: slider.availableWidth
                    height: 4
                    radius: 2
                    color: sliderRow.theme.surfaceHover

                    Rectangle {
                        width: slider.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: sliderRow.theme.accent
                    }
                }

                handle: Rectangle {
                    x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
                    y: slider.topPadding + slider.availableHeight / 2 - height / 2
                    width: 16; height: 16; radius: 8
                    color: sliderRow.theme.accent
                    border.color: sliderRow.theme.surfaceHigh
                    border.width: 2
                }
            }

            Label {
                Layout.preferredWidth: 34
                horizontalAlignment: Text.AlignRight
                text: Math.round(slider.value)
                color: sliderRow.theme.textDim
                font.family: sliderRow.theme.fontMono
                font.pixelSize: sliderRow.theme.fontSmall
            }
        }
    }

    component SettingChoice: SettingRow {
        id: choiceRow
        required property string value
        property var options: []
        signal picked(string value)

        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4

            Repeater {
                model: choiceRow.options

                delegate: Rectangle {
                    id: chip

                    required property var modelData

                    readonly property bool active: modelData.value === choiceRow.value

                    width: chipLabel.implicitWidth + 22
                    height: 28
                    radius: choiceRow.theme.roundingSmall
                    color: active ? choiceRow.theme.accentContainer
                         : chipHover.hovered ? choiceRow.theme.surfaceHover
                         : "transparent"
                    border.color: active ? "transparent" : choiceRow.theme.border
                    border.width: 1

                    Behavior on color { ColorAnimation { duration: choiceRow.theme.durFast } }

                    HoverHandler { id: chipHover }
                    TapHandler { onTapped: choiceRow.picked(chip.modelData.value) }

                    Label {
                        id: chipLabel
                        anchors.centerIn: parent
                        text: chip.modelData.label
                        color: chip.active ? choiceRow.theme.onAccentContainer
                                           : choiceRow.theme.textDim
                        font.family: choiceRow.theme.fontMain
                        font.pixelSize: choiceRow.theme.fontSmall
                    }
                }
            }
        }
    }

    component SettingText: SettingRow {
        id: textRow
        required property string value
        signal committed(string value)

        TextField {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: 240
            text: textRow.value
            color: textRow.theme.text
            font.family: textRow.theme.fontMain
            font.pixelSize: textRow.theme.fontSmall
            leftPadding: 10
            rightPadding: 10

            background: Rectangle {
                radius: textRow.theme.roundingSmall
                color: textRow.theme.surfaceHover
                border.color: parent.activeFocus ? textRow.theme.accent : textRow.theme.border
                border.width: 1
            }

            onEditingFinished: if (text !== textRow.value) textRow.committed(text)
        }
    }
}
