import QtQuick
import QtQuick.Controls.Basic as Basic

Basic.MenuItem {
    id: root
    implicitHeight: Theme.controlHeight
    implicitWidth: Math.max(160, contentItem.implicitWidth + 2 * Theme.space3)
    hoverEnabled: true

    contentItem: Text {
        text: root.text.replace("&", "")
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontBody
        color: root.enabled ? Theme.text : Theme.textMuted
        verticalAlignment: Text.AlignVCenter
        leftPadding: Theme.space2
    }
    arrow: null
    indicator: null
    background: Rectangle {
        color: root.highlighted ? Theme.bgHover : "transparent"
    }
}
