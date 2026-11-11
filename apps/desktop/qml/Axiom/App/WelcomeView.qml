import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as Basic
import Axiom.Ui

// Shown in the content area while no workspace is open.
Rectangle {
    id: root
    color: Theme.bg

    signal openFolderRequested()
    signal openRecent(string root)

    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Welcome")

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - 2 * Theme.space5, 420)
        spacing: Theme.space4

        AxLabel {
            Layout.fillWidth: true
            text: qsTr("Axiom")
            role: "heading"
            horizontalAlignment: Text.AlignHCenter
        }
        AxButton {
            Layout.alignment: Qt.AlignHCenter
            objectName: "openFolderButton"
            text: qsTr("Open Folder…")
            variant: "primary"
            onClicked: root.openFolderRequested()
        }

        // Real entries only: with no recent workspaces this section does not exist.
        ColumnLayout {
            Layout.fillWidth: true
            visible: workspace.recent.count > 0
            spacing: Theme.space1

            AxLabel {
                text: qsTr("Recent")
                role: "caption"
                font.weight: Theme.fontWeightStrong
            }
            Repeater {
                model: workspace.recent
                delegate: Basic.ItemDelegate {
                    id: item
                    required property string root
                    required property string name
                    Layout.fillWidth: true
                    implicitHeight: Theme.rowHeight + Theme.space2
                    hoverEnabled: true
                    focusPolicy: Qt.StrongFocus
                    onClicked: parentRoot.openRecent(item.root)
                    readonly property var parentRoot: root

                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("Open recent folder %1, %2").arg(name).arg(item.root)
                    Accessible.onPressAction: item.clicked()

                    contentItem: RowLayout {
                        spacing: Theme.space3
                        AxLabel {
                            text: item.name
                            font.weight: Theme.fontWeightStrong
                            Accessible.ignored: true
                        }
                        AxLabel {
                            Layout.fillWidth: true
                            text: item.root
                            role: "muted"
                            elide: Text.ElideMiddle
                            Accessible.ignored: true
                        }
                    }
                    background: Rectangle {
                        radius: Theme.radiusS
                        color: item.down ? Theme.bgSelected : item.hovered ? Theme.bgHover : "transparent"
                        Rectangle {
                            anchors.fill: parent
                            radius: Theme.radiusS
                            color: "transparent"
                            border.width: Theme.focusRingWidth
                            border.color: Theme.accent
                            visible: item.visualFocus
                        }
                    }
                    ToolTipArea { text: item.root; shown: true }
                }
            }
        }
    }
}
