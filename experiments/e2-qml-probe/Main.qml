import QtQuick
import QtQuick.Window

Window {
    width: 380; height: 300; visible: true
    Column {
        x: 20; y: 20; spacing: 12
        Repeater {
            model: [{ name: "Cheque", balance: 1204.5 }, { name: "Savings", balance: 98311.07 }]
            BalanceCard { required property var modelData; name: modelData.name; balance: modelData.balance }
        }
    }
}
