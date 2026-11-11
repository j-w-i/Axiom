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
                        onTriggered: documents.requestOpenFolder(root)
                    }
                }
            }
            Basic.Action {
                text: qsTr("&Save")
                enabled: documents.count > 0 && documents.editorReady
                // Inside the editor Monaco handles Ctrl+S itself; this covers the rest of the window.
                shortcut: "Ctrl+S"
                onTriggered: documents.save(documents.activePath)
            }
            Basic.Action {
                text: qsTr("Save A&ll")
                enabled: documents.hasDirty && documents.editorReady
                onTriggered: documents.saveAll()
            }
            Basic.MenuSeparator {}
            Basic.Action {
                text: qsTr("&Close Folder")
                enabled: workspace.open
                onTriggered: documents.requestCloseFolder()
            }
            Basic.MenuSeparator {}
            Basic.Action {
                text: qsTr("&Quit")
                shortcut: StandardKey.Quit
                onTriggered: documents.requestQuit()
            }
        }
        Basic.Menu {
            title: qsTr("&Edit")
            delegate: AxMenuItem {}
            background: AxMenuBackground {}
            Basic.Action { text: qsTr("&Undo");       enabled: documents.count > 0 && documents.editorReady; onTriggered: documents.runCommand("undo") }
            Basic.Action { text: qsTr("&Redo");       enabled: documents.count > 0 && documents.editorReady; onTriggered: documents.runCommand("redo") }
            Basic.MenuSeparator {}
            Basic.Action { text: qsTr("&Find");       enabled: documents.count > 0 && documents.editorReady; onTriggered: documents.runCommand("find") }
            Basic.Action { text: qsTr("&Replace");    enabled: documents.count > 0 && documents.editorReady; onTriggered: documents.runCommand("replace") }
            Basic.MenuSeparator {}
            Basic.Action { text: qsTr("Select &All"); enabled: documents.count > 0 && documents.editorReady; onTriggered: documents.runCommand("selectAll") }
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
            title: qsTr("&Run")
            delegate: AxMenuItem {}
            background: AxMenuBackground {}
            Basic.Action {
                text: qsTr("Run &Command")
                enabled: workspace.open
                shortcut: "Ctrl+Shift+R"
                onTriggered: { window.panelVisible = true; panel.focusCommand() }
            }
            Basic.Action {
                text: qsTr("&Rerun Last")
                enabled: tasks.canRun && tasks.history.length > 0
                shortcut: "F5"
                onTriggered: tasks.rerunLast()
            }
            Basic.Action {
                text: qsTr("&Stop")
                enabled: tasks.running
                shortcut: "Shift+F5"
                onTriggered: tasks.stop()
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
            visible: documents.bannerText.length > 0
            variant: "warning"
            text: documents.bannerText
            actionText: qsTr("Dismiss")
            onActionTriggered: documents.dismissBanner()
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
                        onFileActivated: (path) => documents.open(path)
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

                    EditorArea {
                        id: editorArea
                        anchors.fill: parent
                        visible: workspace.open
                    }
                    WelcomeView {
                        anchors.fill: parent
                        visible: !workspace.open
                        onOpenFolderRequested: folderDialog.open()
                        onOpenRecent: (root) => documents.requestOpenFolder(root)
                    }
                }

                // Bottom panel: command bar and streamed output.
                OutputPanel {
                    id: panel
                    objectName: "outputPanel"
                    visible: window.panelVisible
                    Basic.SplitView.preferredHeight: 220
                    Basic.SplitView.minimumHeight: 100
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
                AxLabel {
                    objectName: "taskStatus"
                    visible: tasks.statusText.length > 0
                    text: tasks.statusText
                    role: "muted"
                    color: tasks.hasLastStatus && tasks.lastStatus.outcome === "failed" ? Theme.danger : Theme.textMuted
                    Accessible.name: qsTr("Task: %1").arg(tasks.statusText)
                }
                AxLabel {
                    objectName: "cursorText"
                    visible: documents.activePath !== ""
                    text: documents.cursorText
                    role: "muted"
                    Accessible.name: qsTr("Cursor position: %1").arg(documents.cursorText)
                }
            }
        }
    }

    FolderDialog {
        id: folderDialog
        title: qsTr("Open Folder")
        onAccepted: documents.requestOpenFolderUrl(selectedFolder)
    }

    Connections {
        target: workspace
        function onErrorOccurred(message) { window.workspaceError = message }
        function onOpenChanged() { if (workspace.open) window.workspaceError = "" }
    }

    // ---- closing the window with unsaved work asks first
    property bool quitting: false
    onClosing: (close) => {
        if (!quitting && documents.hasDirty) {
            close.accepted = false
            documents.requestQuit()
        }
    }
    Connections {
        target: documents
        function onQuitConfirmed() { window.quitting = true; Qt.quit() }
    }

    // ---- the editor follows the OS appearance until settings exist (M1-SHELL-005)
    function syncAppearance() { documents.setAppearance(Theme.dark, 13, "", false) }
    Connections { target: Theme; function onDarkChanged() { window.syncAppearance() } }
    Connections { target: documents; function onEditorReadyChanged() { if (documents.editorReady) window.syncAppearance() } }
    Component.onCompleted: syncAppearance()

    // ---- conflict: the file changed on disk since it was opened
    AxDialog {
        id: conflictDialog
        objectName: "conflictDialog"
        title: qsTr("%1 changed on disk").arg(documents.conflictName)
        visible: documents.conflictActive
        dismissAction: "cancel"
        buttons: [
            { text: qsTr("Cancel"), variant: "quiet", action: "cancel" },
            { text: qsTr("Reload"), variant: "default", action: "reload" },
            { text: qsTr("Overwrite"), variant: "primary", action: "overwrite" }
        ]
        onChosen: (action) => documents.resolveConflict(action)
        AxLabel {
            Layout.fillWidth: true
            text: qsTr("Someone else changed this file after you opened it. Overwrite replaces their version with yours; Reload discards your changes and shows theirs.")
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }
    }

    // ---- unsaved work: closing a tab, closing/switching the folder, quitting
    AxDialog {
        id: unsavedDialog
        objectName: "unsavedDialog"
        title: documents.promptKind === "close" ? qsTr("Save changes to %1?").arg(documents.promptNames[0] || "")
                                               : qsTr("Save changes before closing?")
        visible: documents.promptActive
        dismissAction: "cancel"
        buttons: [
            { text: qsTr("Cancel"), variant: "quiet", action: "cancel" },
            { text: qsTr("Discard"), variant: "default", action: "discard" },
            { text: qsTr("Save"), variant: "primary", action: "save" }
        ]
        onChosen: (action) => documents.resolvePrompt(action)
        AxLabel {
            Layout.fillWidth: true
            visible: documents.promptKind !== "close"
            text: qsTr("These documents have unsaved changes:")
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }
        Repeater {
            model: documents.promptKind !== "close" ? documents.promptNames : []
            delegate: AxLabel { required property string modelData; text: "• " + modelData; Layout.fillWidth: true }
        }
        AxLabel {
            Layout.fillWidth: true
            visible: documents.promptKind === "close"
            text: qsTr("Your changes will be lost if you do not save them.")
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }
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
