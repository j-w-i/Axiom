import QtQuick
import QtQuick.Controls.Basic as Basic

// Shows `text` as a tooltip while the parent is hovered, when `shown`.
// Used so that truncated text always has its full form available.
Item {
    id: root
    property string text
    property bool shown: true
    anchors.fill: parent
    Accessible.ignored: true

    HoverHandler { id: hover }
    Basic.ToolTip {
        visible: hover.hovered && root.shown && root.text.length > 0
        delay: 500
        text: root.text
    }
}
