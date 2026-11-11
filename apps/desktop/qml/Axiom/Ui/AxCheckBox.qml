import QtQuick
import QtQuick.Controls.Basic as Basic

Basic.CheckBox {
    id: root
    implicitHeight: Theme.controlHeight
    spacing: Theme.space2
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontBody

    Accessible.role: Accessible.CheckBox
    Accessible.name: text
    Accessible.checked: checked
    Accessible.onToggleAction: root.toggle()

    indicator: Rectangle {
        x: root.leftPadding
        y: (root.height - height) / 2
        implicitWidth: Theme.iconSize + 2
        implicitHeight: Theme.iconSize + 2
        radius: Theme.radiusS
        color: root.checked ? Theme.accent : Theme.bg
        border.width: Theme.borderWidth
        border.color: root.visualFocus ? Theme.accent : Theme.border
        Text {
            anchors.centerIn: parent
            visible: root.checked
            text: "✓"
            color: Theme.textOnAccent
            font.pixelSize: Theme.fontBody
            font.weight: Theme.fontWeightStrong
        }
        Rectangle { // focus ring outside
            anchors.fill: parent
            anchors.margins: -Theme.focusRingWidth
            radius: Theme.radiusS + Theme.focusRingWidth
            color: "transparent"
            border.width: Theme.focusRingWidth
            border.color: Theme.accent
            visible: root.visualFocus
        }
    }
    contentItem: Text {
        text: root.text
        font: root.font
        color: root.enabled ? Theme.text : Theme.textMuted
        verticalAlignment: Text.AlignVCenter
        leftPadding: root.indicator.width + root.spacing
    }
}
