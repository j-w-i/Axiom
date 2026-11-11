import QtQuick
import QtQuick.Layouts

// A titled strip at the top of a side bar or panel. Extra controls go into
// the default slot at the right.
Rectangle {
    id: root
    property string title
    default property alias actions: actionRow.data

    implicitHeight: Theme.controlHeight + Theme.space1
    color: Theme.bgSubtle

    Accessible.role: Accessible.Heading
    Accessible.name: title

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.space3
        anchors.rightMargin: Theme.space2
        spacing: Theme.space2
        AxLabel {
            Layout.fillWidth: true
            text: root.title.toUpperCase()
            role: "caption"
            font.weight: Theme.fontWeightStrong
            Accessible.ignored: true
        }
        RowLayout {
            id: actionRow
            spacing: Theme.space1
        }
    }
    AxSeparator {
        anchors.bottom: parent.bottom
        width: parent.width
    }
}
