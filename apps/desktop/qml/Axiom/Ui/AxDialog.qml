import QtQuick
import QtQuick.Controls.Basic as Basic
import QtQuick.Layouts

// A modal dialog with a title, body content and a Close button.
Basic.Dialog {
    id: root
    default property alias body: bodyColumn.data

    modal: true
    anchors.centerIn: Basic.Overlay.overlay
    // A fixed width keeps the dialog's implicit size independent of its children.
    implicitWidth: 360
    padding: Theme.space4
    closePolicy: Basic.Popup.CloseOnEscape | Basic.Popup.CloseOnPressOutside

    background: Rectangle {
        color: Theme.bgElevated
        radius: Theme.radiusM
        border.width: Theme.borderWidth
        border.color: Theme.border
    }
    header: AxLabel {
        text: root.title
        role: "title"
        leftPadding: Theme.space4
        rightPadding: Theme.space4
        topPadding: Theme.space4
        bottomPadding: 0
    }
    contentItem: ColumnLayout {
        id: bodyColumn
        spacing: Theme.space2
        Accessible.role: Accessible.Dialog
        Accessible.name: root.title
    }
    footer: RowLayout {
        spacing: Theme.space2
        Item { Layout.fillWidth: true }
        AxButton {
            text: qsTr("Close")
            variant: "primary"
            Layout.margins: Theme.space3
            onClicked: root.close()
        }
    }
    Basic.Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, Theme.dark ? 0.5 : 0.3) }
}
