import QtQuick
import QtQuick.Window
import QtWebEngine
import QtWebChannel

Window {
    id: root
    width: 900; height: 600; visible: true
    title: "Axiom E1"
    WebChannel { id: channel }
    WebEngineView {
        id: view
        objectName: "editorView"
        anchors.fill: parent
        webChannel: channel
        url: "axiom://editor/index.html"
        onJavaScriptConsoleMessage: (level, message, line, source) => host.consoleMessage(level, message, source + ":" + line)
        Component.onCompleted: { channel.registerObject("host", host); forceActiveFocus(); }
    }
}
