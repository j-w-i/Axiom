import QtQuick
import QtQuick.Controls.Basic as Basic
import QtQuick.Layouts
import QtWebEngine
import QtWebChannel
import Axiom.Ui

// Tab strip above one WebEngineView hosting the editor page. The page only
// ever sees the `host` object (documents.bridge).
FocusScope {
    id: root
    property alias view: webView

    readonly property bool hasDocuments: documents.count > 0

    // Keyboard: Ctrl+W closes, Ctrl+Tab / Ctrl+Shift+Tab cycle (also while Monaco has focus).
    Shortcut {
        sequences: ["Ctrl+W"]
        context: Qt.WindowShortcut
        enabled: root.hasDocuments
        onActivated: documents.close(documents.activePath)
    }
    Shortcut {
        sequences: ["Ctrl+Tab", "Ctrl+PgDown"]
        enabled: documents.count > 1
        onActivated: documents.cycle(1)
    }
    Shortcut {
        sequences: ["Ctrl+Shift+Tab", "Ctrl+PgUp"]
        enabled: documents.count > 1
        onActivated: documents.cycle(-1)
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ---- tab strip
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: Theme.controlHeight + Theme.space1
            visible: root.hasDocuments
            color: Theme.bgSubtle
            Accessible.role: Accessible.PageTabList
            Accessible.name: qsTr("Open documents")

            Basic.TabBar {
                id: tabs
                anchors.fill: parent
                background: null
                currentIndex: -1
                Repeater {
                    model: documents
                    delegate: AxTabButton {
                        required property string path
                        required property string title
                        required property bool dirty
                        required property bool changedOnDisk
                        required property bool active
                        text: title
                        fullPath: path
                        checked: active
                        onClicked: documents.activate(path)
                        onCloseRequested: documents.close(path)
                    }
                }
            }
            AxSeparator { anchors.bottom: parent.bottom; width: parent.width }
        }

        // ---- editor surface or empty state
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            WebChannel { id: channel }

            WebEngineView {
                id: webView
                objectName: "editorView"
                anchors.fill: parent
                visible: root.hasDocuments && !documents.rendererCrashed && editorAvailable
                focus: true
                webChannel: channel
                url: editorAvailable ? "axiom://editor/index.html" : "about:blank"
                backgroundColor: Theme.bg
                settings.localStorageEnabled: false
                settings.javascriptCanAccessClipboard: true
                settings.errorPageEnabled: false

                Component.onCompleted: channel.registerObject("host", documents.bridge)

                // Only the editor page may load: anything else is rejected.
                onNavigationRequested: (request) => {
                    if (request.url.toString().indexOf("axiom://editor/") !== 0)
                        request.reject()
                }
                // No page-provided windows or generic context menu: Monaco has its own.
                onNewWindowRequested: (request) => {}
                onContextMenuRequested: (request) => {
                    if (request.mediaType === ContextMenuRequest.MediaTypeNone && !request.isContentEditable)
                        request.accepted = true
                }
                onLoadingChanged: (loadRequest) => {
                    if (loadRequest.status === WebEngineView.LoadStartedStatus)
                        documents.editorUnavailable()
                    else if (loadRequest.status === WebEngineView.LoadFailedStatus)
                        documents.showBanner(qsTr("The editor failed to load (%1).").arg(loadRequest.errorString))
                }
                onRenderProcessTerminated: (status, exitCode) => documents.editorCrashed()
                onJavaScriptConsoleMessage: (level, message, line, source) => {
                    if (level >= WebEngineView.ErrorMessageLevel)
                        console.warn("[editor]", message, source + ":" + line)
                }
            }

            // Focus follows the active document into the page.
            Connections {
                target: documents
                function onActivePathChanged() { if (documents.activePath !== "") webView.forceActiveFocus() }
            }

            // ---- renderer crash / missing assets panel
            Rectangle {
                anchors.fill: parent
                visible: root.hasDocuments && (documents.rendererCrashed || !editorAvailable)
                color: Theme.bg
                Accessible.role: Accessible.Pane
                Accessible.name: qsTr("Editor unavailable")

                ColumnLayout {
                    anchors.centerIn: parent
                    width: Math.min(parent.width - 2 * Theme.space5, 420)
                    spacing: Theme.space3
                    AxLabel {
                        Layout.fillWidth: true
                        text: editorAvailable ? qsTr("The editor stopped unexpectedly")
                                              : qsTr("The editor files were not found")
                        role: "title"
                        horizontalAlignment: Text.AlignHCenter
                    }
                    AxLabel {
                        Layout.fillWidth: true
                        text: editorAvailable ? qsTr("Your unsaved changes are kept and will return when the editor reloads.")
                                              : qsTr("Pass --editor-assets <dir> or build editor/dist (pnpm run build).")
                        role: "muted"
                        wrapMode: Text.Wrap
                        elide: Text.ElideNone
                        horizontalAlignment: Text.AlignHCenter
                    }
                    AxButton {
                        Layout.alignment: Qt.AlignHCenter
                        visible: editorAvailable
                        objectName: "reloadEditorButton"
                        text: qsTr("Reload editor")
                        variant: "primary"
                        onClicked: webView.reload()
                    }
                }
            }

            // ---- no document open: the welcome view or an empty hint
            Loader {
                anchors.fill: parent
                active: !root.hasDocuments
                sourceComponent: Rectangle {
                    color: Theme.bg
                    AxLabel {
                        anchors.centerIn: parent
                        text: qsTr("Select a file in the Explorer to open it")
                        role: "muted"
                    }
                }
            }
        }
    }
}
