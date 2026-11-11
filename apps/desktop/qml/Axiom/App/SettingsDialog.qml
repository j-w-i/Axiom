import QtQuick
import QtQuick.Controls.Basic as Basic
import QtQuick.Layouts
import Axiom.Ui

// Settings apply immediately (the daemon confirms, then the store changes), so
// the only button is Close. Every control has an associated visible label.
AxDialog {
    id: root
    objectName: "settingsDialog"
    title: qsTr("Settings")
    implicitWidth: 460

    // Shown when the daemon rejected a value.
    property string errorText

    Connections {
        target: settings
        function onErrorOccurred(message) { root.errorText = message }
        function onChanged() { root.errorText = "" }
    }
    onOpened: errorText = ""

    AxBanner {
        Layout.fillWidth: true
        visible: root.errorText.length > 0
        variant: "warning"
        text: root.errorText
    }

    // ---- Appearance
    AxLabel { text: qsTr("Appearance"); role: "title"; Layout.topMargin: Theme.space2 }
    GridLayout {
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.space3
        rowSpacing: Theme.space2
        AxLabel { text: qsTr("Theme"); Layout.alignment: Qt.AlignVCenter }
        Basic.ComboBox {
            id: themeBox
            objectName: "themeBox"
            Layout.fillWidth: true
            implicitHeight: Theme.controlHeight
            model: [ { text: qsTr("System"), value: "system" },
                     { text: qsTr("Light"), value: "light" },
                     { text: qsTr("Dark"), value: "dark" } ]
            textRole: "text"
            valueRole: "value"
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontBody
            focusPolicy: Qt.StrongFocus
            Accessible.role: Accessible.ComboBox
            Accessible.name: qsTr("Theme")
            // The daemon is the source of truth: follow the store, never the user's click.
            currentIndex: indexOfValue(settings.theme)
            onActivated: (i) => { settings.theme = valueAt(i); currentIndex = Qt.binding(() => indexOfValue(settings.theme)) }
            contentItem: Text {
                text: themeBox.displayText
                font: themeBox.font
                color: Theme.text
                verticalAlignment: Text.AlignVCenter
                leftPadding: Theme.space2
            }
            indicator: Text { x: themeBox.width - width - Theme.space2; y: (themeBox.height - height) / 2; text: "▾"; color: Theme.textMuted; font.pixelSize: Theme.fontBody }
            background: Rectangle {
                radius: Theme.radiusS
                color: Theme.bg
                border.width: Theme.borderWidth
                border.color: themeBox.activeFocus ? Theme.accent : Theme.border
            }
            delegate: Basic.ItemDelegate {
                id: opt
                required property var modelData
                required property int index
                width: themeBox.width
                height: Theme.controlHeight
                highlighted: themeBox.highlightedIndex === index
                contentItem: Text { text: opt.modelData.text; font: themeBox.font; color: Theme.text; verticalAlignment: Text.AlignVCenter }
                background: Rectangle { color: opt.highlighted ? Theme.bgHover : "transparent" }
                Accessible.role: Accessible.ListItem
                Accessible.name: opt.modelData.text
            }
            popup: Basic.Popup {
                y: themeBox.height
                width: themeBox.width
                padding: 1
                contentItem: ListView { clip: true; implicitHeight: contentHeight; model: themeBox.popup.visible ? themeBox.delegateModel : null; currentIndex: themeBox.highlightedIndex }
                background: Rectangle { color: Theme.bgElevated; border.width: Theme.borderWidth; border.color: Theme.border; radius: Theme.radiusS }
            }
        }
    }

    // ---- Editor
    AxLabel { text: qsTr("Editor"); role: "title"; Layout.topMargin: Theme.space3 }
    GridLayout {
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.space3
        rowSpacing: Theme.space2

        AxLabel { text: qsTr("Font size"); Layout.alignment: Qt.AlignVCenter }
        AxSpinBox {
            id: fontSizeBox
            objectName: "fontSizeBox"
            name: qsTr("Font size")
            from: 8
            to: 40
            // Follow the store; the spin box never keeps a value the daemon did not accept.
            value: settings.editorFontSize
            onValueModified: { settings.editorFontSize = value; value = Qt.binding(() => settings.editorFontSize) }
        }

        AxLabel { text: qsTr("Font family"); Layout.alignment: Qt.AlignVCenter }
        AxTextField {
            id: fontFamilyField
            objectName: "fontFamilyField"
            name: qsTr("Font family")
            Layout.fillWidth: true
            // The placeholder shows what an empty value resolves to.
            placeholderText: Theme.monoFamily
            text: settings.editorFontFamily
            onEditingFinished: { if (text !== settings.editorFontFamily) settings.editorFontFamily = text }
        }

        Item { Layout.preferredWidth: 1 }
        AxCheckBox {
            id: wrapBox
            objectName: "wrapBox"
            text: qsTr("Word wrap")
            checked: settings.editorWordWrap
            onToggled: { settings.editorWordWrap = checked; checked = Qt.binding(() => settings.editorWordWrap) }
        }
    }

    // ---- Workspace
    AxLabel { text: qsTr("Workspace"); role: "title"; Layout.topMargin: Theme.space3 }
    AxCheckBox {
        id: reopenBox
        objectName: "reopenBox"
        text: qsTr("Reopen last folder at startup")
        checked: settings.reopenLast
        onToggled: { settings.reopenLast = checked; checked = Qt.binding(() => settings.reopenLast) }
    }
}
