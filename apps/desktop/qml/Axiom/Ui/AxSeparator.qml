import QtQuick

Rectangle {
    id: root
    property int orientation: Qt.Horizontal

    implicitWidth: orientation === Qt.Horizontal ? 0 : Theme.borderWidth
    implicitHeight: orientation === Qt.Horizontal ? Theme.borderWidth : 0
    color: Theme.border
    Accessible.ignored: true
}
