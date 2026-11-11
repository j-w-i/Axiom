import QtQuick
import QtQuick.Controls.Basic as Basic
import QtQuick.Layouts

// A modal dialog with a title, body content and a row of buttons. With no
// custom `buttons` it shows a single Close button.
//   buttons: [{ text, variant, action }]   — `action` is reported by `chosen(action)`.
Basic.Dialog {
    id: root
    default property alias body: bodyColumn.data
    property var buttons: []
    signal chosen(string action)
    // Set when the dialog is closed without choosing (Escape, click outside).
    property string dismissAction: ""

    modal: true
    anchors.centerIn: Basic.Overlay.overlay
    implicitWidth: 400
    padding: Theme.space4
    closePolicy: dismissAction === "" ? (Basic.Popup.CloseOnEscape | Basic.Popup.CloseOnPressOutside)
                                      : Basic.Popup.CloseOnEscape

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
        elide: Text.ElideRight
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
        Repeater {
            model: root.buttons.length > 0 ? root.buttons : [{ text: qsTr("Close"), variant: "primary", action: "" }]
            delegate: AxButton {
                required property var modelData
                text: modelData.text
                variant: modelData.variant
                Layout.topMargin: Theme.space3
                Layout.bottomMargin: Theme.space3
                Layout.rightMargin: Theme.space2
                onClicked: {
                    root.chosen(modelData.action)
                    root.close()
                }
            }
        }
    }
    onRejected: if (dismissAction !== "") chosen(dismissAction)
    Basic.Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, Theme.dark ? 0.5 : 0.3) }
}
