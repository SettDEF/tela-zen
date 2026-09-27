import QtQuick
import QtQuick.Effects
import QtQuick.Particles

pragma ComponentBehavior: Bound

// Animated start-page backdrops, drawn from the theme's own palette so they
// change with it rather than fighting it.
//
// Everything stops when the page is not showing: an idle animation behind a
// hidden page is pure battery.
Item {
    id: root

    required property var theme
    // none | aurora | stars | mesh
    property string style: "aurora"
    property real intensity: 1.0
    // Set false when the page is hidden.
    property bool active: true

    readonly property bool animating: root.active && root.style !== "none"

    clip: true

    Rectangle {
        anchors.fill: parent
        color: root.theme.surface
    }

    // ── Aurora ─────────────────────────────────────────────────────────
    // Slow drifting colour fields, blurred until the shapes are gone and only
    // the light is left.
    Item {
        id: aurora
        anchors.fill: parent
        visible: false
        layer.enabled: root.style === "aurora"

        Repeater {
            model: [
                { colour: root.theme.accent,          size: 1.10, from: 0.08, to: 0.62, secs: 23 },
                { colour: root.theme.tertiary,        size: 0.85, from: 0.70, to: 0.20, secs: 31 },
                { colour: root.theme.accentContainer, size: 0.95, from: 0.35, to: 0.85, secs: 27 }
            ]

            delegate: Rectangle {
                id: blob

                required property var modelData

                width: Math.max(parent.width, parent.height) * blob.modelData.size
                height: width
                radius: width / 2
                color: blob.modelData.colour
                opacity: 0.55 * root.intensity
                y: parent.height * 0.5 - height / 2

                SequentialAnimation on x {
                    running: root.animating
                    loops: Animation.Infinite
                    NumberAnimation {
                        from: blob.parent.width * blob.modelData.from - blob.width / 2
                        to: blob.parent.width * blob.modelData.to - blob.width / 2
                        duration: blob.modelData.secs * 1000
                        easing.type: Easing.InOutSine
                    }
                    NumberAnimation {
                        from: blob.parent.width * blob.modelData.to - blob.width / 2
                        to: blob.parent.width * blob.modelData.from - blob.width / 2
                        duration: blob.modelData.secs * 1000
                        easing.type: Easing.InOutSine
                    }
                }

                SequentialAnimation on y {
                    running: root.animating
                    loops: Animation.Infinite
                    NumberAnimation {
                        to: blob.parent.height * 0.15 - blob.height / 2
                        duration: blob.modelData.secs * 700
                        easing.type: Easing.InOutSine
                    }
                    NumberAnimation {
                        to: blob.parent.height * 0.80 - blob.height / 2
                        duration: blob.modelData.secs * 700
                        easing.type: Easing.InOutSine
                    }
                }
            }
        }
    }

    MultiEffect {
        anchors.fill: parent
        source: aurora
        visible: root.style === "aurora"
        blurEnabled: true
        blur: 1.0
        blurMax: 64
        saturation: 0.2
    }

    // ── Stars ──────────────────────────────────────────────────────────
    ParticleSystem {
        id: sky
        running: root.animating && root.style === "stars"
    }

    Emitter {
        anchors.fill: parent
        system: sky
        enabled: root.style === "stars" && root.animating
        emitRate: 14 * root.intensity
        lifeSpan: 9000
        lifeSpanVariation: 3000
        size: 2
        sizeVariation: 2
        velocity: PointDirection { x: 8; y: -6; xVariation: 10; yVariation: 10 }
    }

    ItemParticle {
        system: sky
        visible: root.style === "stars"
        delegate: Rectangle {
            width: 3
            height: 3
            radius: 1.5
            color: root.theme.text
            opacity: 0.55
        }
    }

    // ── Mesh ───────────────────────────────────────────────────────────
    // A gradient that turns slowly. The cheapest of the three by a distance.
    Rectangle {
        id: mesh
        anchors.fill: parent
        visible: root.style === "mesh"
        opacity: 0.65 * root.intensity

        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0.0; color: root.theme.accentContainer }
            GradientStop { position: 0.5; color: root.theme.surface }
            GradientStop { position: 1.0; color: root.theme.tertiary }
        }

        RotationAnimation on rotation {
            running: root.animating && root.style === "mesh"
            loops: Animation.Infinite
            from: 0
            to: 360
            duration: 90000
        }
        transformOrigin: Item.Center
        scale: 1.8
    }
}
