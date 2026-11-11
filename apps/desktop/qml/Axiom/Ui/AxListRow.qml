import QtQuick
import QtQuick.Controls.Basic as Basic

// A row of the explorer tree (TreeViewDelegate with design-system styling).
// `ignored` entries are muted; `errorRow` shows an inline error message.
Basic.TreeViewDelegate {
    id: root

    property bool ignored: false
    property bool errorRow: false
    property bool directory: false
    property string label
    property string detail            // full text for the tooltip / accessible description

    implicitHeight: Theme.rowHeight
    indentation: Theme.iconSize
    leftMargin: Theme.space2
    rightMargin: Theme.space2
    spacing: Theme.space1
    hoverEnabled: true
    focusPolicy: Qt.NoFocus // the TreeView owns focus; rows are not tab stops

    Accessible.role: errorRow ? Accessible.StaticText : Accessible.TreeItem
    Accessible.name: errorRow ? qsTr("Error: %1").arg(label)
                              : directory ? qsTr("%1, folder, %2").arg(label).arg(expanded ? qsTr("expanded") : qsTr("collapsed"))
                                          : label
    Accessible.selected: root.current
    Accessible.focusable: true

    indicator: Item {
        readonly property real __indent: root.leftMargin + root.depth * root.indentation
        x: __indent
        y: (root.height - height) / 2
        implicitWidth: Theme.iconSize
        implicitHeight: Theme.iconSize
        visible: root.isTreeNode && root.hasChildren && !root.errorRow
        Text {
            anchors.centerIn: parent
            text: "›"
            font.pixelSize: Theme.fontTitle
            color: Theme.textMuted
            rotation: root.expanded ? 90 : 0
            Behavior on rotation { NumberAnimation { duration: Theme.durationFast } }
        }
    }

    contentItem: Text {
        text: root.label
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontBody
        font.italic: root.errorRow
        color: root.errorRow ? Theme.danger
             : root.ignored ? Theme.textMuted
             : Theme.text
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
        leftPadding: 0
    }

    background: Rectangle {
        color: root.current || root.selected ? Theme.bgSelected
             : root.hovered ? Theme.bgHover : "transparent"
        // Visible focus: ring around the current row while the tree has focus.
        Rectangle {
            anchors.fill: parent
            color: "transparent"
            border.width: Theme.focusRingWidth
            border.color: Theme.accent
            visible: root.current && root.treeView.activeFocus
        }
    }

    ToolTipArea { text: root.detail.length > 0 ? root.detail : root.label; shown: contentItem.truncated }
}
