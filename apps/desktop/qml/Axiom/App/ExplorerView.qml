import QtQuick
import QtQuick.Controls.Basic as Basic
import Axiom.Ui

// The workspace file tree. All data comes from `workspace.tree` (FileTreeModel).
FocusScope {
    id: root

    // Emitted for Enter / double click on a file; handled by the editor (M1-SHELL-003).
    signal fileActivated(string path)

    property alias view: tree
    readonly property var model: workspace.tree

    Accessible.role: Accessible.Tree
    Accessible.name: qsTr("File explorer")

    Basic.ScrollView {
        anchors.fill: parent
        clip: true

        TreeView {
            id: tree
            objectName: "explorerTree"
            model: root.model
            focus: true
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            selectionModel: ItemSelectionModel { model: tree.model }
            selectionBehavior: TableView.SelectRows
            keyNavigationEnabled: true
            pointerNavigationEnabled: true
            reuseItems: false
            columnWidthProvider: () => tree.width
            activeFocusOnTab: true
            Accessible.role: Accessible.Tree
            Accessible.name: qsTr("File explorer")

            // The tree's own signals are the single source of truth for expansion
            // (keyboard, mouse and programmatic expand all arrive here).
            onExpanded: (row, depth) => root.model.setExpanded(tree.index(row, 0), true)
            onCollapsed: (row, recursive) => root.model.setExpanded(tree.index(row, 0), false)

            Connections {
                target: root.model
                // A restored directory appeared: show it expanded.
                function onExpandRequested(index) {
                    const row = tree.rowAtIndex(index)
                    if (row >= 0)
                        tree.expand(row)
                }
            }

            function activateCurrent() {
                const row = tree.currentRow
                const idx = tree.index(row, 0)
                if (!idx.valid)
                    return
                if (root.model.isDirectory(idx)) {
                    if (tree.isExpanded(row))
                        tree.collapse(row)
                    else
                        tree.expand(row)
                } else if (root.model.isFile(idx)) {
                    root.fileActivated(root.model.pathAt(idx))
                }
            }

            Keys.onPressed: (event) => {

                const row = tree.currentRow
                if (row < 0)
                    return
                const idx = tree.index(row, 0)
                if (event.key === Qt.Key_Right) {
                    if (root.model.isDirectory(idx)) {
                        if (!tree.isExpanded(row))
                            tree.expand(row)
                        else
                            tree.selectionModel.setCurrentIndex(tree.index(row + 1, 0), ItemSelectionModel.ClearAndSelect)
                    }
                    event.accepted = true
                } else if (event.key === Qt.Key_Left) {
                    if (root.model.isDirectory(idx) && tree.isExpanded(row)) {
                        tree.collapse(row)
                    } else {
                        // Go to the parent directory.
                        const parentIdx = root.model.parent(idx)
                        if (parentIdx.valid)
                            tree.selectionModel.setCurrentIndex(parentIdx, ItemSelectionModel.ClearAndSelect)
                    }
                    event.accepted = true
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    activateCurrent()
                    event.accepted = true
                } else if (event.key === Qt.Key_Home) {
                    tree.selectionModel.setCurrentIndex(tree.index(0, 0), ItemSelectionModel.ClearAndSelect)
                    event.accepted = true
                } else if (event.key === Qt.Key_End) {
                    tree.selectionModel.setCurrentIndex(tree.index(tree.rows - 1, 0), ItemSelectionModel.ClearAndSelect)
                    event.accepted = true
                }
            }

            delegate: AxListRow {
                id: row
                required property string name
                required property string path
                required property string kind
                required property bool ignored
                required property bool loading

                label: name
                directory: kind === "directory"
                errorRow: kind === "error"
                detail: path.length > 0 && !errorRow ? path : name
                ignored: row.ignored

                onClicked: {
                    tree.selectionModel.setCurrentIndex(tree.index(row.row, 0), ItemSelectionModel.ClearAndSelect)
                    tree.forceActiveFocus()
                    if (row.directory)
                        tree.toggleExpanded(row.row)
                }
                TapHandler {
                    acceptedButtons: Qt.LeftButton
                    gesturePolicy: TapHandler.ReleaseWithinBounds
                    onDoubleTapped: if (!row.directory && !row.errorRow) root.fileActivated(row.path)
                }
            }
        }
    }
}
