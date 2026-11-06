pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Key reference, grouped like Track Studio's help dock; keys are drawn as keycaps.
QQC2.ScrollView {
    id: help
    contentWidth: availableWidth
    QQC2.ScrollBar.horizontal.policy: QQC2.ScrollBar.AlwaysOff

    ColumnLayout {
        width: help.availableWidth
        spacing: 0
        Repeater {
            // [keys, mouse action, what it does]. Keys: "Ctrl+Space" is a combination (drawn with +),
            // "W A S D" are separate keys.
            model: [
                { title: "Camera", keys: [["W A S D", "", "Move"], ["Space", "", "Up"], ["Ctrl+Space", "", "Down"], ["Shift", "", "Move faster"], ["", "Right drag", "Look around"], ["", "Middle drag", "Pan"], ["", "Wheel", "Zoom to cursor"]] },
                { title: "Selection", keys: [["", "Click", "Select"], ["Ctrl", "Click", "Add to selection"], ["F", "", "Focus selection"], ["Home", "", "Frame course"], ["Esc", "", "Cancel or deselect"]] },
                { title: "Editing", keys: [["Alt", "Click", "Place current object"], ["Q", "", "Place at view centre"], ["1 2 3", "", "Move, rotate, scale"], ["Del", "", "Delete"], ["Ctrl+D", "", "Duplicate"], ["G", "", "Drop to ground"]] },
                { title: "Paths", keys: [["I", "", "Insert point after"], ["L", "", "Link to next click"]] },
                { title: "File", keys: [["Ctrl+O", "", "Open course"], ["Ctrl+S", "", "Save mod"], ["Ctrl+Z", "", "Undo"], ["Ctrl+Shift+Z", "", "Redo"]] }
            ]
            delegate: ColumnLayout {
                id: group
                required property var modelData
                Layout.fillWidth: true
                spacing: 3
                Kirigami.ListSectionHeader {
                    text: group.modelData.title
                    Layout.fillWidth: true
                }
                Repeater {
                    model: group.modelData.keys
                    delegate: RowLayout {
                        id: line
                        required property var modelData
                        // caps and "+" signs, then the mouse action
                        readonly property var parts: {
                            const out = [];
                            const k = modelData[0];
                            if (k.length > 0) {
                                const combo = k.includes("+");
                                k.split(combo ? "+" : " ").forEach((c, i) => {
                                    if (i > 0 && combo)
                                        out.push({ kind: "plus", text: "+" });
                                    out.push({ kind: "key", text: c });
                                });
                            }
                            if (modelData[1].length > 0) {
                                if (out.length > 0)
                                    out.push({ kind: "plus", text: "+" });
                                out.push({ kind: "mouse", text: modelData[1] });
                            }
                            return out;
                        }
                        Layout.fillWidth: true
                        Layout.leftMargin: Kirigami.Units.largeSpacing
                        Layout.rightMargin: Kirigami.Units.smallSpacing
                        spacing: Kirigami.Units.largeSpacing
                        Row {
                            Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                            spacing: 3
                            Repeater {
                                model: line.parts
                                delegate: Rectangle {
                                    id: part
                                    required property var modelData
                                    // fixed accent: the theme's highlight dims while the window is inactive
                                    readonly property color accent: "#4f9cf0"
                                    anchors.verticalCenter: parent.verticalCenter
                                    implicitWidth: modelData.kind === "plus" ? partText.implicitWidth : Math.max(partText.implicitWidth + 10, height)
                                    implicitHeight: partText.implicitHeight + 4
                                    radius: modelData.kind === "mouse" ? height / 2 : 3
                                    // keys: neutral keycaps; mouse actions: accent pills
                                    color: modelData.kind === "key" ? Kirigami.Theme.alternateBackgroundColor
                                         : modelData.kind === "mouse" ? Qt.rgba(accent.r, accent.g, accent.b, 0.3) : "transparent"
                                    border.color: modelData.kind === "key" ? Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.28)
                                                : modelData.kind === "mouse" ? Qt.rgba(accent.r, accent.g, accent.b, 0.8) : "transparent"
                                    QQC2.Label {
                                        id: partText
                                        anchors.centerIn: parent
                                        text: part.modelData.text
                                        font.pointSize: Kirigami.Theme.smallFont.pointSize
                                        font.weight: part.modelData.kind === "plus" ? Font.Normal : Font.DemiBold
                                        color: part.modelData.kind === "plus" ? Kirigami.Theme.disabledTextColor : Kirigami.Theme.textColor
                                    }
                                }
                            }
                        }
                        QQC2.Label {
                            text: line.modelData[2]
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                        }
                    }
                }
                Item { implicitHeight: Kirigami.Units.smallSpacing }
            }
        }
    }
}
