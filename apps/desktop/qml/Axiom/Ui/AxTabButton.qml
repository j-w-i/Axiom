import QtQuick
import QtQuick.Controls.Basic as Basic
import QtQuick.Layouts

// One tab: title, a dirty dot, a close button. Middle click closes.
Basic.TabButton {
    id: root

    property bool dirty: false
    property bool changedOnDisk: false
    property string fullPath

    signal closeRequested()

    implicitHeight: Theme.controlHeight + Theme.space1
    implicitWidth: Math.min(220, content.implicitWidth + 2 * Theme.space3)
    padding: 0
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus

    Accessible.role: Accessible.PageTab
    Accessible.name: text + (dirty ? qsTr(", unsaved changes") : "")
                          + (changedOnDisk ? qsTr(", changed on disk") : "")
    Accessible.selected: checked
    Accessible.onPressAction: root.clicked()

    contentItem: RowLayout {
        id: content
        spacing: Theme.space1
        anchors.leftMargin: Theme.space3
        anchors.rightMargin: Theme.space1
        // Dirty is a dot AND text in the accessible name: never colour alone.
        AxLabel {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.space3
            text: root.text
            elide: Text.ElideRight
            color: root.changedOnDisk ? Theme.warning : Theme.text
            font.weight: root.checked ? Theme.fontWeightStrong : Theme.fontWeightRegular
            Accessible.ignored: true
        }
        AxLabel {
            visible: root.dirty
            text: "●"
            role: "caption"
            color: root.changedOnDisk ? Theme.warning : Theme.accent
            Accessible.ignored: true
        }
        AxIconButton {
            glyph: "×"
            name: qsTr("Close %1").arg(root.text)
            implicitWidth: Theme.iconSize + Theme.space1
            implicitHeight: Theme.iconSize + Theme.space1
            Layout.rightMargin: Theme.space1
            focusPolicy: Qt.NoFocus
            onClicked: root.closeRequested()
        }
    }

    background: Rectangle {
        color: root.checked ? Theme.bg : root.hovered ? Theme.bgHover : Theme.bgSubtle
        Rectangle { // active marker
            visible: root.checked
            width: parent.width
            height: Theme.focusRingWidth
            color: Theme.accent
        }
        Rectangle {
            anchors.fill: parent
            color: "transparent"
            border.width: Theme.focusRingWidth
            border.color: Theme.accent
            visible: root.visualFocus
        }
        AxSeparator { anchors.right: parent.right; height: parent.height; orientation: Qt.Vertical }
    }

    TapHandler {
        acceptedButtons: Qt.MiddleButton
        onTapped: root.closeRequested()
    }
    ToolTipArea {
        text: root.changedOnDisk ? qsTr("%1 — changed on disk").arg(root.fullPath) : root.fullPath
        shown: root.fullPath.length > 0
    }
}
