import QtQuick
import QtQuick.Controls.Basic as Basic

// A square button showing a short glyph. `name` is the accessible name and
// the tooltip; it is required because the glyph alone says nothing.
Basic.Button {
    id: root
    property string glyph
    property string name

    implicitWidth: Theme.controlHeight
    implicitHeight: Theme.controlHeight
    padding: 0
    focusPolicy: Qt.StrongFocus
    hoverEnabled: true

    Accessible.role: Accessible.Button
    Accessible.name: root.name
    Accessible.onPressAction: root.clicked()

    contentItem: Text {
        text: root.glyph
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontTitle
        color: root.enabled ? Theme.text : Theme.textMuted
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }

    background: Rectangle {
        radius: Theme.radiusS
        color: root.down ? Theme.bgSelected : root.hovered ? Theme.bgHover : "transparent"
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

    Basic.ToolTip.visible: hovered && name.length > 0
    Basic.ToolTip.delay: 500
    Basic.ToolTip.text: name
}
