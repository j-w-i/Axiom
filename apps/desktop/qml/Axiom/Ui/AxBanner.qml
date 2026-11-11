import QtQuick
import QtQuick.Layouts

// variant: info | warning | danger. The action button appears when
// `actionText` is set and emits `actionTriggered`.
Rectangle {
    id: root
    property string variant: "info"
    property string text
    property string actionText
    signal actionTriggered()

    readonly property color tone: variant === "danger" ? Theme.danger
                                : variant === "warning" ? Theme.warning
                                : Theme.accent

    implicitHeight: row.implicitHeight + 2 * Theme.space2
    color: Theme.bgSubtle
    border.width: Theme.borderWidth
    border.color: tone

    Accessible.role: Accessible.AlertMessage
    Accessible.name: (variant === "danger" ? qsTr("Error") : variant === "warning" ? qsTr("Warning") : qsTr("Information"))
                     + ": " + text

    Rectangle {
        width: Theme.space1
        height: parent.height
        color: root.tone
    }

    RowLayout {
        id: row
        anchors.fill: parent
        anchors.leftMargin: Theme.space3 + Theme.space1
        anchors.rightMargin: Theme.space3
        anchors.topMargin: Theme.space2
        anchors.bottomMargin: Theme.space2
        spacing: Theme.space3

        // Text label as well as colour: never colour alone.
        AxLabel {
            text: root.variant === "danger" ? qsTr("Error") : root.variant === "warning" ? qsTr("Warning") : qsTr("Info")
            color: root.tone
            font.weight: Theme.fontWeightStrong
            Accessible.ignored: true
        }
        AxLabel {
            Layout.fillWidth: true
            text: root.text
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            maximumLineCount: 3
            Accessible.ignored: true
        }
        AxButton {
            visible: root.actionText.length > 0
            text: root.actionText
            variant: "primary"
            onClicked: root.actionTriggered()
        }
    }
}
