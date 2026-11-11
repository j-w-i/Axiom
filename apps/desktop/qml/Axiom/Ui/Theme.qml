pragma Singleton
import QtQuick

// Design tokens (docs/architecture/design-system.md). Application views use
// only these; no literal colours, sizes or radii outside Axiom.Ui.
QtObject {
    id: theme

    // Set by the shell. Until settings exist (M1-SHELL-005) it follows the OS.
    property bool dark: Qt.styleHints.colorScheme === Qt.Dark

    // ---- colour roles
    readonly property color bg:           dark ? "#1e1f22" : "#ffffff"
    readonly property color bgSubtle:     dark ? "#25262a" : "#f5f6f8"
    readonly property color bgElevated:   dark ? "#2c2d32" : "#ffffff"
    readonly property color bgHover:      dark ? "#33353b" : "#e9ebef"
    readonly property color bgSelected:   dark ? "#2f3d63" : "#dbe4ff"
    readonly property color border:       dark ? "#3a3c43" : "#d9dce3"
    readonly property color text:         dark ? "#e3e5ea" : "#1c1e24"
    readonly property color textMuted:    dark ? "#9aa1b0" : "#5f6675"
    readonly property color textOnAccent: "#ffffff"
    readonly property color accent:       dark ? "#6b8cff" : "#2f5fe0"
    readonly property color danger:       dark ? "#f0717b" : "#c62f3b"
    readonly property color warning:      dark ? "#e0b34f" : "#9a6700"
    readonly property color success:      dark ? "#57c08a" : "#1a7f4b"

    // ---- spacing, radius, sizes
    readonly property int space1: 4
    readonly property int space2: 8
    readonly property int space3: 12
    readonly property int space4: 16
    readonly property int space5: 24
    readonly property int space6: 32
    readonly property int radiusS: 4
    readonly property int radiusM: 6
    readonly property int controlHeight: 28
    readonly property int rowHeight: 24
    readonly property int iconSize: 16
    readonly property int borderWidth: 1
    readonly property int focusRingWidth: 2
    readonly property int splitHandleSize: 5

    // ---- typography
    readonly property string fontFamily: Qt.application.font.family
    // Resolved in C++ at startup (first installed of the design system's list).
    property string monoFamily: "monospace"
    readonly property int fontCaption: 11
    readonly property int fontBody: 13
    readonly property int fontTitle: 15
    readonly property int fontHeading: 20
    readonly property int fontWeightRegular: Font.Normal
    readonly property int fontWeightStrong: Font.DemiBold

    // ---- motion (disabled when the platform asks for reduced motion)
    readonly property bool reducedMotion: false
    readonly property int durationFast: reducedMotion ? 0 : 80
    readonly property int durationNormal: reducedMotion ? 0 : 160
}
