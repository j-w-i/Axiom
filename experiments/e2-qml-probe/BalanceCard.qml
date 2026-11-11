import QtQuick

Rectangle {
    id: card
    property string name
    property real balance
    width: 340; height: 96; radius: 8; color: "white"
    Text { x: 12; y: 10; text: card.name; font.pixelSize: 14 }
    Text { x: 12; y: 30; text: "$" + card.balance.toFixed(2); font.pixelSize: 24 }
    Rectangle {
        objectName: "transferButton"
        x: 12; y: 64; width: 80; height: 24; radius: 4; color: "#2563eb"
        Text { anchors.centerIn: parent; text: "Transfer"; color: "white" }
    }
}
