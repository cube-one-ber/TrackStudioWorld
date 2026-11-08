pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Templates as T
import org.kde.kirigami as Kirigami

// Dialog buttons in the same order in every dialog: extra actions on the left, then Cancel, then
// the confirming button on the right, drawn in the accent colour so the default action stands out.
QQC2.Control {
    id: f
    property string acceptText: ""
    property string acceptIcon: "dialog-ok"
    property bool acceptEnabled: true
    property string rejectText: "Cancel"
    property string rejectIcon: "dialog-cancel"
    // actions shown on the left
    property list<T.Action> actions
    signal accepted
    signal rejected

    padding: Kirigami.Units.largeSpacing
    contentItem: RowLayout {
        spacing: Kirigami.Units.smallSpacing
        Repeater {
            model: f.actions
            delegate: QQC2.Button {
                required property T.Action modelData
                action: modelData
            }
        }
        Item {
            Layout.fillWidth: true
        }
        QQC2.Button {
            visible: f.rejectText.length > 0
            text: f.rejectText
            icon.name: f.rejectIcon
            onClicked: f.rejected()
        }
        QQC2.Button {
            id: accept
            visible: f.acceptText.length > 0
            text: f.acceptText
            icon.name: f.acceptIcon
            enabled: f.acceptEnabled
            // filled with the accent colour (the desktop style ignores `highlighted`); the style
            // draws a button's label as part of its background, so both are drawn here
            padding: Kirigami.Units.smallSpacing
            leftPadding: Kirigami.Units.largeSpacing
            rightPadding: Kirigami.Units.largeSpacing
            background: Rectangle {
                readonly property color hl: Kirigami.Theme.highlightColor
                implicitWidth: Kirigami.Units.gridUnit * 4
                implicitHeight: Kirigami.Units.gridUnit * 1.6
                radius: Kirigami.Units.cornerRadius
                color: !accept.enabled ? Qt.rgba(hl.r, hl.g, hl.b, 0.3)
                     : accept.down ? Qt.darker(hl, 1.15)
                     : accept.hovered ? Qt.lighter(hl, 1.12) : hl
                border.color: accept.visualFocus ? Kirigami.Theme.focusColor : Qt.darker(hl, 1.25)
                border.width: 1
            }
            contentItem: RowLayout {
                spacing: Kirigami.Units.smallSpacing
                opacity: accept.enabled ? 1 : 0.55
                Kirigami.Icon {
                    source: accept.icon.name
                    color: Kirigami.Theme.highlightedTextColor
                    implicitWidth: Kirigami.Units.iconSizes.small
                    implicitHeight: Kirigami.Units.iconSizes.small
                }
                QQC2.Label {
                    text: accept.text
                    color: Kirigami.Theme.highlightedTextColor
                }
            }
            onClicked: f.accepted()
        }
    }
}
