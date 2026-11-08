pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import TrackStudioWorld

// Course checks: what blocks a save, and what no shipped course does (Track Studio's "possible
// errors" lists, checked live). Click a row to select what it is about.
ColumnLayout {
    id: pp
    required property Editor editor
    signal focusRequested()
    property var rows: []
    spacing: 0

    function refresh() {
        rows = JSON.parse(editor.problems());
    }
    // checks run a moment after the last edit, not on every drag step
    Timer {
        id: later
        interval: 250
        onTriggered: pp.refresh()
    }
    Connections {
        target: pp.editor
        function onRevisionChanged() { later.restart(); }
        function onCourseTitleChanged() { later.restart(); }
    }

    QQC2.ScrollView {
        Layout.fillWidth: true
        Layout.fillHeight: true
        ListView {
            id: list
            clip: true
            model: pp.rows
            boundsBehavior: Flickable.StopAtBounds
            delegate: QQC2.ItemDelegate {
                id: row
                required property var modelData
                width: ListView.view.width
                readonly property bool canSelect: modelData.items.length > 0
                contentItem: RowLayout {
                    spacing: Kirigami.Units.largeSpacing
                    Kirigami.Icon {
                        source: row.modelData.severity === "error" ? "data-error" : row.modelData.severity === "warning" ? "data-warning" : "data-information"
                        implicitWidth: Kirigami.Units.iconSizes.small
                        implicitHeight: Kirigami.Units.iconSizes.small
                    }
                    QQC2.Label {
                        text: row.modelData.message
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: row.modelData.severity === "error" ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
                    }
                    QQC2.Label {
                        visible: row.canSelect
                        text: row.modelData.items.length + (row.modelData.items.length === 1 ? " item" : " items")
                        color: Kirigami.Theme.disabledTextColor
                        font.pointSize: Kirigami.Theme.smallFont.pointSize
                    }
                    QQC2.Button {
                        visible: row.modelData.fix === "renumber"
                        text: "Renumber"
                        icon.name: "format-list-ordered"
                        onClicked: pp.editor.pathTool("renumber")
                        QQC2.ToolTip.text: "Number the checkpoints in order along the lap"
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                    QQC2.Button {
                        visible: row.modelData.fix === "orient"
                        text: "Orient"
                        icon.name: "transform-rotate"
                        onClicked: {
                            pp.editor.selectItems(JSON.stringify(row.modelData.items));
                            pp.editor.pathTool("orient");
                        }
                        QQC2.ToolTip.text: "Turn these points to face along their path"
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                }
                onClicked: {
                    if (!canSelect)
                        return;
                    pp.editor.selectItems(JSON.stringify(modelData.items));
                    pp.focusRequested();
                }
                QQC2.ToolTip.text: canSelect ? "Click to select and show" : ""
                QQC2.ToolTip.visible: hovered && canSelect
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }
    }
    Kirigami.PlaceholderMessage {
        Layout.fillWidth: true
        Layout.fillHeight: true
        visible: pp.rows.length === 0
        icon.name: pp.editor.courseTitle.length > 0 ? "checkmark" : "document-open"
        text: pp.editor.courseTitle.length > 0 ? "No problems found" : "No course open"
        explanation: pp.editor.courseTitle.length > 0 ? "Checked against how the shipped courses are built." : ""
    }
}
