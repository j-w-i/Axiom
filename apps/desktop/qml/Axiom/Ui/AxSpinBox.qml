import QtQuick
import QtQuick.Controls.Basic as Basic

Basic.SpinBox {
    id: root
    property string name

    implicitHeight: Theme.controlHeight
    implicitWidth: 110
    editable: true
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontBody
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus

    Accessible.role: Accessible.SpinBox
    Accessible.name: root.name
    Accessible.editable: true

    contentItem: TextInput {
        text: root.textFromValue(root.value, root.locale)
        font: root.font
        color: Theme.text
        selectionColor: Theme.accent
        selectedTextColor: Theme.textOnAccent
        horizontalAlignment: Qt.AlignHCenter
        verticalAlignment: Qt.AlignVCenter
        readOnly: !root.editable
        validator: root.validator
        inputMethodHints: Qt.ImhFormattedNumbersOnly
        Accessible.name: root.name
    }
    up.indicator: Rectangle {
        x: root.width - width
        height: root.height / 2
        width: Theme.controlHeight
        color: root.up.pressed ? Theme.bgSelected : root.up.hovered ? Theme.bgHover : "transparent"
        Text { anchors.centerIn: parent; text: "▴"; color: Theme.textMuted; font.pixelSize: Theme.fontBody }
    }
    down.indicator: Rectangle {
        x: 0
        height: root.height
        width: Theme.controlHeight
        color: root.down.pressed ? Theme.bgSelected : root.down.hovered ? Theme.bgHover : "transparent"
        Text { anchors.centerIn: parent; text: "▾"; color: Theme.textMuted; font.pixelSize: Theme.fontBody }
    }
    background: Rectangle {
        radius: Theme.radiusS
        color: Theme.bg
        border.width: Theme.borderWidth
        border.color: root.activeFocus ? Theme.accent : Theme.border
        Rectangle {
            anchors.fill: parent
            anchors.margins: -Theme.focusRingWidth
            radius: Theme.radiusS + Theme.focusRingWidth
            color: "transparent"
            border.width: Theme.focusRingWidth
            border.color: Theme.accent
            visible: root.visualFocus
        }
    }
}
