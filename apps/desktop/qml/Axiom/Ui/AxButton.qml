import QtQuick
import QtQuick.Controls.Basic as Basic

// variant: primary | default | quiet
Basic.Button {
    id: root
    property string variant: "default"

    implicitHeight: Theme.controlHeight
    implicitWidth: Math.max(implicitHeight, contentItem.implicitWidth + 2 * Theme.space3)
    leftPadding: Theme.space3
    rightPadding: Theme.space3
    topPadding: 0
    bottomPadding: 0
    focusPolicy: Qt.StrongFocus
    hoverEnabled: true

    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.onPressAction: root.clicked()

    contentItem: Text {
        text: root.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontBody
        font.weight: root.variant === "primary" ? Theme.fontWeightStrong : Theme.fontWeightRegular
        color: !root.enabled ? Theme.textMuted
             : root.variant === "primary" ? Theme.textOnAccent
             : Theme.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    background: Rectangle {
        radius: Theme.radiusS
        color: root.variant === "primary"
               ? (root.down ? Qt.darker(Theme.accent, 1.2)
                            : root.hovered ? Qt.lighter(Theme.accent, 1.1) : Theme.accent)
               : (root.down ? Theme.bgSelected
                            : root.hovered ? Theme.bgHover
                            : root.variant === "quiet" ? "transparent" : Theme.bgElevated)
        border.width: root.variant === "default" ? Theme.borderWidth : 0
        border.color: Theme.border
        opacity: root.enabled ? 1 : 0.6

        // Focus ring, drawn outside the control.
        Rectangle {
            anchors.fill: parent
            anchors.margins: -Theme.focusRingWidth
            radius: Theme.radiusS + Theme.focusRingWidth
            color: "transparent"
            border.width: Theme.focusRingWidth
            border.color: Theme.accent
            visible: root.visualFocus
        }
        Behavior on color { ColorAnimation { duration: Theme.durationFast } }
    }

    ToolTipArea { text: root.text; shown: root.contentItem.truncated }
}
