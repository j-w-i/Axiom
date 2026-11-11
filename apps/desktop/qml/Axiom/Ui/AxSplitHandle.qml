import QtQuick
import QtQuick.Controls.Basic as Basic

// Handle for SplitView.handle. Shows a focus-ring-coloured line while
// hovered or dragged.
Rectangle {
    id: root
    property bool vertical: false   // true when it separates items stacked top/bottom
    implicitWidth: vertical ? 0 : Theme.splitHandleSize
    implicitHeight: vertical ? Theme.splitHandleSize : 0
    color: Basic.SplitHandle.pressed ? Theme.accent
         : Basic.SplitHandle.hovered ? Theme.bgHover : Theme.bgSubtle
    Accessible.ignored: true

    Rectangle {
        anchors.centerIn: parent
        width: root.vertical ? parent.width : Theme.borderWidth
        height: root.vertical ? Theme.borderWidth : parent.height
        color: Theme.border
    }
    Behavior on color { ColorAnimation { duration: Theme.durationFast } }
}
