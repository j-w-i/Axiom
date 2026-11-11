import QtQuick

// role: body | muted | title | heading | caption
Text {
    id: root
    property string role: "body"

    color: role === "muted" || role === "caption" ? Theme.textMuted : Theme.text
    font.family: Theme.fontFamily
    font.pixelSize: role === "heading" ? Theme.fontHeading
                  : role === "title" ? Theme.fontTitle
                  : role === "caption" ? Theme.fontCaption
                  : Theme.fontBody
    font.weight: role === "heading" || role === "title" ? Theme.fontWeightStrong
                                                          : Theme.fontWeightRegular
    elide: Text.ElideRight
    wrapMode: Text.NoWrap
    Accessible.role: Accessible.StaticText
    Accessible.name: text
}
