import QtQuick
import QtQuick.Controls.Basic as Basic

Basic.TextField {
    id: root
    property string name   // accessible name, normally the label's text

    implicitHeight: Theme.controlHeight
    implicitWidth: 200
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontBody
    color: Theme.text
    placeholderTextColor: Theme.textMuted
    selectionColor: Theme.accent
    selectedTextColor: Theme.textOnAccent
    leftPadding: Theme.space2
    rightPadding: Theme.space2
    topPadding: 0
    bottomPadding: 0
    verticalAlignment: TextInput.AlignVCenter
    selectByMouse: true
    focusPolicy: Qt.StrongFocus

    Accessible.role: Accessible.EditableText
    Accessible.name: root.name.length > 0 ? root.name : placeholderText
    Accessible.editable: true

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
            visible: root.activeFocus
        }
    }
}
