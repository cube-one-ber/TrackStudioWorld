pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// One typed parameter (from the game's BancParam table). Unset parameters show their default
// as a placeholder; the arrow lists the values the shipped courses use.
RowLayout {
    id: pr
    required property var p
    signal setValue(string name, string kind, string text)
    signal unset(string name)
    property string error: ""
    property real labelWidth: Kirigami.Units.gridUnit * 5.5
    Layout.fillWidth: true
    Layout.preferredHeight: Kirigami.Units.gridUnit * 1.75
    spacing: Kirigami.Units.smallSpacing

    readonly property string kind: p.type
    readonly property bool isBool: kind === "Bool" || kind === "bool"
    readonly property var suggestions: (p.suggest || []).map(String)
    readonly property string defaultText: p.type === "Enum" ? (p.init + (p.initName ? " (" + p.initName + ")" : "")) : JSON.stringify(p.init)
    readonly property string valueText: p.set ? (Array.isArray(p.value) ? p.value.join(", ") : String(p.value)) : ""

    QQC2.Label {
        id: name
        text: pr.p.name
        Layout.preferredWidth: pr.labelWidth
        Layout.maximumWidth: pr.labelWidth
        elide: Text.ElideRight
        font.weight: pr.p.set ? Font.DemiBold : Font.Normal
        color: pr.error ? Kirigami.Theme.negativeTextColor
             : pr.p.set ? Kirigami.Theme.textColor
             : Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.75)
        QQC2.ToolTip.text: pr.p.name + (pr.p.label ? "\n" + pr.p.label : "") + "\n" + pr.p.type + (pr.p.enumName ? " " + pr.p.enumName : "") + ", default " + pr.defaultText + (pr.p.set ? "" : " (not set)") + (pr.error ? "\n\n" + pr.error : "")
        QQC2.ToolTip.visible: hov.hovered
        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        HoverHandler { id: hov }
    }
    QQC2.CheckBox {
        visible: pr.isBool
        checked: pr.p.set ? pr.p.value === true : pr.p.init === true
        onToggled: pr.setValue(pr.p.name, pr.kind, checked ? "true" : "false")
    }
    QQC2.ToolButton {
        visible: pr.isBool && pr.p.set === true
        icon.name: "edit-clear"
        display: QQC2.AbstractButton.IconOnly
        text: "Remove (use the default)"
        onClicked: pr.unset(pr.p.name)
        QQC2.ToolTip.text: text
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
    }
    Item {
        visible: pr.isBool
        Layout.fillWidth: true
    }
    QQC2.TextField {
        id: field
        visible: !pr.isBool
        Layout.fillWidth: true
        text: pr.valueText
        placeholderText: pr.defaultText
        selectByMouse: true
        rightPadding: tools.width + Kirigami.Units.smallSpacing
        onEditingFinished: if (text.length > 0 && text !== pr.valueText) pr.setValue(pr.p.name, pr.kind, text)

        // inside the field's trailing edge: reset (when set), then the list of known values (when any)
        Row {
            id: tools
            anchors.right: parent.right
            anchors.rightMargin: 2
            anchors.verticalCenter: parent.verticalCenter
            QQC2.ToolButton {
                visible: pr.p.set === true
                implicitWidth: field.height - 6
                implicitHeight: field.height - 6
                padding: 0
                icon.name: "edit-clear"
                icon.width: Kirigami.Units.iconSizes.small
                icon.height: Kirigami.Units.iconSizes.small
                display: QQC2.AbstractButton.IconOnly
                text: "Remove (use the default)"
                onClicked: pr.unset(pr.p.name)
                QQC2.ToolTip.text: text
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
            QQC2.ToolButton {
                visible: pr.suggestions.length > 0
                implicitWidth: field.height - 6
                implicitHeight: field.height - 6
                padding: 0
                icon.name: "arrow-down"
                icon.width: Kirigami.Units.iconSizes.small
                icon.height: Kirigami.Units.iconSizes.small
                display: QQC2.AbstractButton.IconOnly
                text: "Values used in the shipped courses"
                onClicked: menu.popup(field, field.width - menu.width, field.height)
                QQC2.ToolTip.text: text
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                QQC2.Menu {
                    id: menu
                    Instantiator {
                        model: pr.suggestions
                        delegate: QQC2.MenuItem {
                            required property string modelData
                            text: modelData
                            checkable: true
                            checked: modelData === pr.valueText
                            onTriggered: pr.setValue(pr.p.name, pr.kind, modelData)
                        }
                        onObjectAdded: (i, o) => menu.insertItem(i, o)
                        onObjectRemoved: (i, o) => menu.removeItem(o)
                    }
                }
            }
        }
    }
}
