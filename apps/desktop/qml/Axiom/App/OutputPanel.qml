import QtQuick
import QtQuick.Controls.Basic as Basic
import QtQuick.Layouts
import Axiom.Ui

// Command bar and streamed output. All state lives in `tasks` (TaskController)
// and `tasks.output` (TaskOutputModel); this file only presents it.
Rectangle {
    id: root
    color: Theme.bgSubtle

    // Focuses the command field (Run ▸ Run Command).
    function focusCommand() {
        commandField.forceActiveFocus()
        commandField.contentItem.selectAll()
    }

    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Output")

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        AxPanelHeader {
            Layout.fillWidth: true
            title: qsTr("Output")

            AxComboBox {
                id: commandField
                objectName: "commandField"
                name: qsTr("Command to run")
                model: tasks.history
                Layout.preferredWidth: 360
                Layout.minimumWidth: 120
                Layout.fillWidth: true
                enabled: workspace.open
                editText: ""
                onSubmitted: (text) => { if (tasks.canRun) tasks.run(text) }
                // Choosing from the history fills the field without running.
                onActivated: (i) => editText = model[i]
            }
            AxButton {
                objectName: "runButton"
                text: qsTr("Run")
                variant: "primary"
                enabled: tasks.canRun && commandField.editText.trim().length > 0
                onClicked: tasks.run(commandField.editText)
                Accessible.description: qsTr("Run the command in the workspace (F5 repeats the last one)")
            }
            AxButton {
                objectName: "stopButton"
                text: qsTr("Stop")
                enabled: tasks.running
                onClicked: tasks.stop()
            }
            AxButton {
                objectName: "clearButton"
                text: qsTr("Clear")
                variant: "quiet"
                enabled: tasks.output.count > 0
                onClicked: tasks.clear()
            }
        }

        // ---- output body
        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            color: Theme.bg

            ListView {
                id: list
                objectName: "outputList"
                anchors.fill: parent
                clip: true
                model: tasks.output
                boundsBehavior: Flickable.StopAtBounds
                reuseItems: true
                activeFocusOnTab: true
                focus: true
                keyNavigationEnabled: false
                Accessible.role: Accessible.List
                Accessible.name: qsTr("Command output")

                // Stick to the end unless the user scrolled up.
                property bool followTail: true
                onContentYChanged: {
                    if (moving || dragging || flicking)
                        followTail = atYEnd
                }
                onCountChanged: if (followTail) Qt.callLater(positionViewAtEnd)
                Connections {
                    target: tasks
                    function onRunningChanged() { if (tasks.running) list.followTail = true }
                }

                Basic.ScrollBar.vertical: Basic.ScrollBar {
                    policy: Basic.ScrollBar.AsNeeded
                    contentItem: Rectangle {
                        implicitWidth: Theme.space2
                        radius: Theme.radiusS
                        color: Theme.border
                    }
                }

                // Selection and copy: a row range, as in a terminal log.
                property int selStart: -1
                property int selEnd: -1
                function selectedText() {
                    if (selStart < 0)
                        return ""
                    const a = Math.min(selStart, selEnd), b = Math.max(selStart, selEnd)
                    let lines = []
                    for (let i = a; i <= b; ++i)
                        lines.push(tasks.output.textAt(i))
                    return lines.join("\n")
                }
                function copySelection() {
                    const t = selStart < 0 ? tasks.output.allText() : selectedText()
                    clipHelper.text = t
                    clipHelper.selectAll()
                    clipHelper.copy()
                }
                TextEdit { id: clipHelper; visible: false; Accessible.ignored: true }

                Keys.onPressed: (event) => {
                    if (event.matches(StandardKey.Copy)) { copySelection(); event.accepted = true }
                    else if (event.matches(StandardKey.SelectAll)) { selStart = 0; selEnd = count - 1; event.accepted = true }
                }

                delegate: Rectangle {
                    id: line
                    required property int index
                    required property string text
                    required property string stream
                    required property string kind

                    readonly property bool isError: stream === "stderr" || kind === "error" || kind === "statusFailed"
                    readonly property bool selected: list.selStart >= 0
                        && index >= Math.min(list.selStart, list.selEnd) && index <= Math.max(list.selStart, list.selEnd)

                    width: list.width
                    height: Math.max(Theme.rowHeight, label.implicitHeight + Theme.space1)
                    color: selected ? Theme.bgSelected : "transparent"

                    Accessible.role: Accessible.ListItem
                    Accessible.name: (isError && kind === "line" ? qsTr("error output: ") : "") + text

                    // Gutter: a marker AND colour for stderr; never colour alone.
                    Text {
                        id: gutter
                        x: Theme.space2
                        width: Theme.space4
                        height: parent.height
                        verticalAlignment: Text.AlignVCenter
                        horizontalAlignment: Text.AlignHCenter
                        font.family: Theme.monoFamily
                        font.pixelSize: Theme.fontBody
                        color: Theme.danger
                        text: kind === "line" && stream === "stderr" ? "!"
                            : kind === "error" ? "✖"
                            : ""
                        Accessible.ignored: true
                    }
                    Text {
                        id: label
                        anchors.left: gutter.right
                        anchors.right: parent.right
                        anchors.rightMargin: Theme.space2
                        anchors.verticalCenter: parent.verticalCenter
                        text: line.text
                        textFormat: Text.PlainText
                        wrapMode: Text.WrapAnywhere
                        font.family: line.kind === "line" || line.kind === "error" ? Theme.monoFamily : Theme.fontFamily
                        font.pixelSize: Theme.fontBody
                        font.weight: line.kind === "status" || line.kind === "statusFailed" ? Theme.fontWeightStrong : Theme.fontWeightRegular
                        font.italic: line.kind === "notice"
                        color: line.isError ? Theme.danger
                             : line.kind === "status" ? Theme.success
                             : line.kind === "notice" ? Theme.textMuted
                             : Theme.text
                        Accessible.ignored: true
                    }
                    TapHandler {
                        acceptedButtons: Qt.LeftButton
                        onTapped: (eventPoint, button) => {
                            list.forceActiveFocus()
                            list.selStart = line.index
                            list.selEnd = line.index
                        }
                    }
                    TapHandler {
                        acceptedButtons: Qt.LeftButton
                        gesturePolicy: TapHandler.WithinBounds
                        grabPermissions: PointerHandler.TakeOverForbidden
                        longPressThreshold: 0.25
                        onLongPressed: list.selStart = list.selEnd = line.index
                    }
                    DragHandler {
                        acceptedButtons: Qt.LeftButton
                        target: null
                        grabPermissions: PointerHandler.CanTakeOverFromAnything
                        onCentroidChanged: {
                            if (!active)
                                return
                            const p = list.mapFromItem(line, centroid.position)
                            const i = list.indexAt(p.x, p.y + list.contentY)
                            if (i >= 0) {
                                if (list.selStart < 0)
                                    list.selStart = line.index
                                list.selEnd = i
                            }
                        }
                    }
                }

                // Right-click: copy the selection or everything.
                TapHandler {
                    acceptedButtons: Qt.RightButton
                    onTapped: (eventPoint) => menu.popup()
                }
                Basic.Menu {
                    id: menu
                    delegate: AxMenuItem {}
                    background: AxMenuBackground {}
                    Basic.Action { text: qsTr("Copy"); enabled: list.selStart >= 0; onTriggered: list.copySelection() }
                    Basic.Action { text: qsTr("Copy all"); enabled: list.count > 0; onTriggered: { list.selStart = -1; list.copySelection() } }
                }
            }

            AxLabel {
                anchors.centerIn: parent
                visible: list.count === 0
                text: workspace.open ? qsTr("Run a command to see its output here")
                                     : qsTr("Open a folder to run commands")
                role: "muted"
            }
        }
    }
}
