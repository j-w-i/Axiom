import QtQuick
import QtQuick.Controls.Basic as Basic

// An editable combo box with the design-system look. Choosing an entry fills
// the field; Enter (accepted) is reported by `submitted`.
Basic.ComboBox {
    id: root
    property string name            // accessible name (required: the field has no visible label)
    signal submitted(string text)

    editable: true
    implicitHeight: Theme.controlHeight
    implicitWidth: 240
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontBody

    Accessible.role: Accessible.ComboBox
    Accessible.name: root.name
    Accessible.editable: true

    onAccepted: root.submitted(editText)

    contentItem: Basic.TextField {
        id: field
        text: root.editText
        font: root.font
        color: Theme.text
        selectionColor: Theme.accent
        selectedTextColor: Theme.textOnAccent
        placeholderTextColor: Theme.textMuted
        verticalAlignment: Text.AlignVCenter
        leftPadding: Theme.space2
        rightPadding: Theme.space2
        topPadding: 0
        bottomPadding: 0
        background: null
        selectByMouse: true
        Accessible.name: root.name
        onTextEdited: root.editText = text
        Keys.onPressed: (event) => {
            if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && (event.modifiers & Qt.ControlModifier)) {
                root.submitted(root.editText)
                event.accepted = true
            }
        }
    }

    indicator: Text {
        x: root.width - width - Theme.space2
        y: (root.height - height) / 2
        text: "▾"
        font.pixelSize: Theme.fontBody
        color: Theme.textMuted
    }

    background: Rectangle {
        radius: Theme.radiusS
        color: Theme.bg
        border.width: Theme.borderWidth
        border.color: root.activeFocus ? Theme.accent : Theme.border
        Rectangle { // focus ring outside the control
            anchors.fill: parent
            anchors.margins: -Theme.focusRingWidth
            radius: Theme.radiusS + Theme.focusRingWidth
            color: "transparent"
            border.width: Theme.focusRingWidth
            border.color: Theme.accent
            visible: root.visualFocus
        }
    }

    delegate: Basic.ItemDelegate {
        id: item
        required property var modelData
        required property int index
        width: root.width
        height: Theme.rowHeight + Theme.space1
        highlighted: root.highlightedIndex === index
        hoverEnabled: true
        contentItem: Text {
            text: item.modelData
            font: root.font
            color: Theme.text
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle { color: item.highlighted || item.hovered ? Theme.bgHover : "transparent" }
        Accessible.role: Accessible.ListItem
        Accessible.name: item.modelData
    }

    popup: Basic.Popup {
        y: root.height
        width: root.width
        implicitHeight: Math.min(contentItem.implicitHeight + 2, 240)
        padding: 1
        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: root.popup.visible ? root.delegateModel : null
            currentIndex: root.highlightedIndex
        }
        background: Rectangle {
            color: Theme.bgElevated
            border.width: Theme.borderWidth
            border.color: Theme.border
            radius: Theme.radiusS
        }
    }
}
