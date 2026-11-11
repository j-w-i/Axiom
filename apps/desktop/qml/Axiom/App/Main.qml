import QtQuick
import QtQuick.Controls.Basic as Basic
import QtQuick.Layouts
import QtQuick.Dialogs
import Axiom.Ui

Basic.ApplicationWindow {
    id: window

    // Below this width the side bar starts collapsed (design-system.md).
    readonly property int narrowWidth: 900

    property bool sideBarVisible: width >= narrowWidth
    property bool panelVisible: true
    property string workspaceError

    // `backend` is a context property provided by the shell (src/Backend.h).
    readonly property string statusText: backend.starting ? qsTr("Connecting…")
        : backend.ready ? qsTr("Connected — axiomd %1").arg(backend.serverVersion)
        : qsTr("Backend stopped")

    title: workspace.open ? qsTr("%1 — Axiom").arg(workspace.name) : qsTr("Axiom")
    width: 1100
    height: 720
    minimumWidth: 720
    minimumHeight: 480
    visible: true
    color: Theme.bg

    menuBar: Basic.MenuBar {
        background: Rectangle {
            color: Theme.bgSubtle
            AxSeparator { anchors.bottom: parent.bottom; width: parent.width }
        }
        delegate: Basic.MenuBarItem {
            id: barItem
            implicitHeight: Theme.controlHeight
            contentItem: Text {
                text: barItem.text.replace("&", "")
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                color: Theme.text
                verticalAlignment: Text.AlignVCenter
            }
            background: Rectangle { color: barItem.highlighted ? Theme.bgHover : "transparent" }
        }

        Basic.Menu {
            title: qsTr("&File")
            delegate: AxMenuItem {}
            background: AxMenuBackground {}
            Basic.Action {
                text: qsTr("&Open Folder…")
                shortcut: StandardKey.Open
                enabled: backend.ready
                onTriggered: folderDialog.open()
            }
            Basic.Menu {
                id: recentMenu
                title: qsTr("Open &Recent")
                enabled: backend.ready && workspace.recent.count > 0
                delegate: AxMenuItem {}
                background: AxMenuBackground {}
                Repeater {
                    model: workspace.recent
                    delegate: AxMenuItem {
                        required property string root
                        required property string name
                        text: name + "  —  " + root
                        onTriggered: workspace.openFolder(root)
                    }
                }
            }
            Basic.Action {
                text: qsTr("&Close Folder")
                enabled: workspace.open
                onTriggered: workspace.close()
            }
            Basic.MenuSeparator {}
            Basic.Action {
                text: qsTr("&Quit")
                shortcut: StandardKey.Quit
                onTriggered: Qt.quit()
            }
        }
        Basic.Menu {
            title: qsTr("&View")
            delegate: AxMenuItem {}
            background: AxMenuBackground {}
            Basic.Action {
                text: qsTr("Toggle Side &Bar")
                shortcut: "Ctrl+B"
                onTriggered: window.sideBarVisible = !window.sideBarVisible
            }
            Basic.Action {
                text: qsTr("Toggle &Panel")
                shortcut: "Ctrl+J"
                onTriggered: window.panelVisible = !window.panelVisible
            }
        }
        Basic.Menu {
            title: qsTr("&Help")
            delegate: AxMenuItem {}
            background: AxMenuBackground {}
            Basic.Action {
                text: qsTr("&About Axiom")
                onTriggered: aboutDialog.open()
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        AxBanner {
            Layout.fillWidth: true
            visible: backend.failed
            variant: "danger"
            text: backend.errorText
            actionText: qsTr("Restart backend")
            onActionTriggered: backend.restart()
        }

        AxBanner {
            Layout.fillWidth: true
            visible: window.workspaceError.length > 0
            variant: "warning"
            text: window.workspaceError
            actionText: qsTr("Dismiss")
            onActionTriggered: window.workspaceError = ""
        }

        Basic.SplitView {
            id: horizontalSplit
            Layout.fillWidth: true
            Layout.fillHeight: true
            orientation: Qt.Horizontal
            handle: AxSplitHandle {}

            // Side bar: the Explorer arrives with M1-SHELL-002.
            Rectangle {
                id: sideBar
                visible: window.sideBarVisible
                Basic.SplitView.preferredWidth: 260
                Basic.SplitView.minimumWidth: 160
                Basic.SplitView.maximumWidth: 600
                color: Theme.bgSubtle
                Accessible.role: Accessible.Pane
                Accessible.name: qsTr("Explorer")

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 0
                    AxPanelHeader {
                        Layout.fillWidth: true
                        title: qsTr("Explorer")
                    }
                    AxLabel {
                        Layout.fillWidth: true
                        Layout.margins: Theme.space3
                        visible: !workspace.open
                        text: qsTr("No folder open")
                        role: "muted"
                    }
                    ExplorerView {
                        id: explorer
                        objectName: "explorerView"
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        visible: workspace.open
                    }
                }
            }

            Basic.SplitView {
                id: verticalSplit
                Basic.SplitView.fillWidth: true
                Basic.SplitView.minimumWidth: 240
                orientation: Qt.Vertical
                handle: AxSplitHandle { vertical: true }

                // Content area: the editor arrives with M1-SHELL-003.
                Rectangle {
                    Basic.SplitView.fillHeight: true
                    Basic.SplitView.minimumHeight: 120
                    color: Theme.bg
                    Accessible.role: Accessible.Pane
                    Accessible.name: qsTr("Content")

                    WelcomeView {
                        anchors.fill: parent
                        visible: !workspace.open
                        onOpenFolderRequested: folderDialog.open()
                        onOpenRecent: (root) => workspace.openFolder(root)
                    }
                }

                // Bottom panel: command output arrives with M1-SHELL-004.
                Rectangle {
                    id: panel
                    visible: window.panelVisible
                    Basic.SplitView.preferredHeight: 200
                    Basic.SplitView.minimumHeight: 80
                    color: Theme.bgSubtle
                    Accessible.role: Accessible.Pane
                    Accessible.name: qsTr("Output")

                    ColumnLayout {
                        anchors.fill: parent
                        spacing: 0
                        AxPanelHeader {
                            Layout.fillWidth: true
                            title: qsTr("Output")
                        }
                        Item { Layout.fillHeight: true }
                    }
                }
            }
        }

        AxSeparator { Layout.fillWidth: true }

        Rectangle {
            id: statusBar
            Layout.fillWidth: true
            implicitHeight: Theme.controlHeight
            color: Theme.bgSubtle
            Accessible.role: Accessible.StatusBar
            Accessible.name: window.statusText

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.space3
                anchors.rightMargin: Theme.space3
                spacing: Theme.space2

                // A glyph and text, never colour alone.
                AxLabel {
                    text: backend.ready ? "●" : backend.starting ? "○" : "■"
                    color: backend.ready ? Theme.success : backend.starting ? Theme.textMuted : Theme.danger
                    Accessible.ignored: true
                }
                AxLabel {
                    id: statusLabel
                    objectName: "statusText"
                    Layout.fillWidth: true
                    text: window.statusText
                    role: "muted"
                    Accessible.ignored: true
                }
            }
        }
    }

    FolderDialog {
        id: folderDialog
        title: qsTr("Open Folder")
        onAccepted: workspace.openFolderUrl(selectedFolder)
    }

    Connections {
        target: workspace
        function onErrorOccurred(message) { window.workspaceError = message }
        function onOpenChanged() { if (workspace.open) window.workspaceError = "" }
    }

    AxDialog {
        id: aboutDialog
        title: qsTr("About Axiom")
        AxLabel {
            text: qsTr("Axiom %1").arg(backend.appVersion)
            role: "heading"
        }
        AxLabel {
            text: backend.ready ? qsTr("Backend: axiomd %1").arg(backend.serverVersion)
                                : qsTr("Backend: not connected")
            role: "muted"
        }
    }
}
