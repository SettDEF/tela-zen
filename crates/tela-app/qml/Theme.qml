import QtQuick
import Tela

pragma ComponentBehavior: Bound

// Design tokens, resolved from Tela's own config in ~/.config/tela.
//
// The Config QObject (Rust) owns loading and file-watching; this wraps it in the
// derived values the UI actually reaches for — hover states, elevation steps,
// motion curves — so QML never recomputes them at each call site.
QtObject {
    id: root

    readonly property Config config: Config {}

    // ── Surfaces ───────────────────────────────────────────────────────
    readonly property color bg: config.background
    readonly property color sidebar: config.surfaceContainerLow
    readonly property color surface: config.surfaceContainer
    readonly property color surfaceHigh: config.surfaceContainerHigh
    readonly property color surfaceHighest: config.surfaceContainerHighest

    // Material's state layers: blend the foreground into the surface rather than
    // using separate hand-picked hover colours, so hovers stay correct when the
    // palette changes underneath.
    readonly property color surfaceHover: mix(surface, config.surfaceFg, 0.92)
    readonly property color surfaceActive: mix(surface, config.surfaceFg, 0.85)
    readonly property color sidebarHover: mix(sidebar, config.surfaceFg, 0.93)

    // ── Content ────────────────────────────────────────────────────────
    readonly property color text: config.surfaceFg
    readonly property color textDim: config.surfaceVariantFg
    readonly property color textFaint: config.outline

    // ── Accent ─────────────────────────────────────────────────────────
    readonly property color accent: config.primary
    readonly property color onAccent: config.primaryFg
    readonly property color accentContainer: config.primaryContainer
    readonly property color onAccentContainer: config.primaryContainerFg
    readonly property color border: config.outlineVariant
    readonly property color error: config.error
    // Used for advisory states — a compatibility gap that is worth noticing
    // but is not an error.
    readonly property color tertiary: config.tertiary

    // ── Shape ──────────────────────────────────────────────────────────
    readonly property int roundingSmall: config.roundingSmall
    readonly property int rounding: config.roundingNormal
    readonly property int roundingLarge: config.roundingLarge
    readonly property int sidebarWidth: config.sidebarWidth
    readonly property int railWidth: config.railWidth
    readonly property int rowHeight: config.tabHeight
    readonly property bool showFavicons: config.showFavicons
    readonly property string closeButtonMode: config.closeButtonMode
    readonly property bool restoreSession: config.restoreSession
    readonly property bool startPageEnabled: config.startPageEnabled
    readonly property bool injectContentScripts: config.injectContentScripts
    readonly property bool allowPopups: config.allowPopups
    readonly property int autoArchiveDays: config.autoArchiveDays
    readonly property int suspendAfterMinutes: config.suspendAfterMinutes

    // ── Layout mode ────────────────────────────────────────────────────
    // "sidebar" | "sidebar-compact" | "sidebar-auto" | "top"
    readonly property string tabBarMode: config.tabBarMode
    readonly property bool shyChrome: config.shyChrome

    readonly property bool tabsOnTop: tabBarMode === "top"
    readonly property bool tabsInSidebar: !tabsOnTop
    // How much horizontal space the sidebar permanently claims. Overlay modes
    // claim none — they float above the page instead of reflowing it.
    readonly property int pageInset: config.pageInset
    readonly property int fullScreenGap: config.fullScreenGap
    readonly property bool fullScreenBorder: config.fullScreenBorder
    readonly property bool fullScreenRounded: config.fullScreenRounded
    readonly property bool fullbleed: pageInset === 0
    // Panels (page, sidebar, right rail) share one card treatment so they read
    // as siblings rather than as a page with decorations attached.
    readonly property bool borderEnabled: config.borderEnabled
    readonly property int borderWidth: borderEnabled ? config.borderWidth : 0
    readonly property int cardRadius: fullbleed ? 0 : rounding
    readonly property bool rightBarEnabled: config.rightBarEnabled
    readonly property int rightBarWidth: config.rightBarWidth

    readonly property int sidebarReserved: tabBarMode === "sidebar" ? sidebarWidth
                                         : tabBarMode === "sidebar-compact" ? railWidth
                                         : 0

    // ── Type ───────────────────────────────────────────────────────────
    readonly property string fontMain: config.fontMain
    readonly property string fontMono: config.fontMono
    // Material Symbols is a ligature font: the glyph name is the text.
    readonly property string fontIcon: config.fontIcon
    readonly property int iconSize: config.iconSize
    readonly property int iconSizeSmall: Math.round(config.iconSize * 0.8)
    readonly property int fontSmallest: 10
    readonly property int fontSmall: 12
    readonly property int fontNormal: 13
    readonly property int fontLarge: 15

    // ── Motion ─────────────────────────────────────────────────────────
    // Material 3 "expressive" curves. The spatial one overshoots slightly
    // (second control point above 1.0), which is what gives the shell its
    // characteristic settle rather than a flat ease-out.
    readonly property var curveEffects: [0.34, 0.80, 0.34, 1.00, 1, 1]
    readonly property var curveSpatial: [0.38, 1.21, 0.22, 1.00, 1, 1]
    readonly property var curveDecel: [0.05, 0.7, 0.1, 1, 1, 1]

    // All durations scale by appearance.animationSpeed (percent). 0 disables
    // tweens outright, which is the accessible/instant option.
    readonly property real animationScale: config.animationSpeed / 100.0
    readonly property int durFast: Math.round(200 * animationScale)
    readonly property int durNormal: Math.round(350 * animationScale)
    readonly property int durSpatial: Math.round(500 * animationScale)

    function mix(a, b, weight) {
        return Qt.rgba(
            a.r * weight + b.r * (1 - weight),
            a.g * weight + b.g * (1 - weight),
            a.b * weight + b.b * (1 - weight),
            a.a * weight + b.a * (1 - weight));
    }

    // Config files are plain JSON meant to be edited by hand; picking these up
    // without a restart is the whole point of keeping them on disk.
    // triggeredOnStart does the initial load too, so nothing depends on
    // component-completion ordering.
    property Timer watcher: Timer {
        interval: 1500
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.config.reload()
    }
}
