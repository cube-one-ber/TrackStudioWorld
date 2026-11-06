pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import TrackStudioWorld

// Objects grouped by type and paths, with visibility checkboxes (Track Studio outliner).
ColumnLayout {
    id: ol
    required property Editor editor
    signal openRequested()
    property var rows: []
    property string revealed: ""
    spacing: 0

    function refresh() {
        const y = list.contentY;
        rows = JSON.parse(editor.outliner(search.text));
        list.contentY = y;
        // scroll a newly selected object or path point into view (Rust expands its group)
        const i = rows.findIndex(r => r.kind === "item" && r.selected);
        const key = i >= 0 ? rows[i].key : "";
        if (key !== revealed) {
            revealed = key;
            if (i >= 0)
                Qt.callLater(() => list.positionViewAtIndex(i, ListView.Contain));
        }
    }
    Connections {
        target: ol.editor
        function onRevisionChanged() { ol.refresh(); }
        function onReadyChanged() { ol.refresh(); }
    }

    Kirigami.SearchField {
        id: search
        Layout.fillWidth: true
        Layout.margins: Kirigami.Units.smallSpacing
        placeholderText: "Filter objects and paths"
        enabled: ol.editor.courseTitle.length > 0
        onTextChanged: ol.refresh()
    }

    Item {
        Layout.fillWidth: true
        Layout.fillHeight: true

        QQC2.ScrollView {
            anchors.fill: parent
            ListView {
                id: list
                clip: true
                model: ol.rows
                boundsBehavior: Flickable.StopAtBounds
                reuseItems: true
                delegate: QQC2.ItemDelegate {
                    id: row
                    required property var modelData
                    required property int index
                    readonly property bool isSection: modelData.kind === "section"
                    readonly property bool isGroup: modelData.kind === "group"
                    readonly property bool isItem: modelData.kind === "item"
                    readonly property bool isRail: (modelData.key || "").startsWith("r:")
                    width: ListView.view.width
                    height: isSection ? Kirigami.Units.gridUnit * 1.6 : Kirigami.Units.gridUnit * 1.35
                    padding: 0
                    hoverEnabled: !isSection
                    highlighted: modelData.selected === true && !isGroup
                    background: Rectangle {
                        readonly property color hl: Kirigami.Theme.highlightColor
                        color: row.highlighted ? hl
                             : row.isGroup && row.modelData.selected ? Qt.rgba(hl.r, hl.g, hl.b, 0.22)
                             : row.hovered ? Qt.rgba(hl.r, hl.g, hl.b, 0.12)
                             : row.isSection ? Kirigami.Theme.alternateBackgroundColor : "transparent"
                    }
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Item {
                            implicitWidth: row.isItem ? Kirigami.Units.gridUnit * 2.9 : Kirigami.Units.smallSpacing
                        }
                        // section: what it holds
                        Kirigami.Icon {
                            visible: row.isSection
                            source: row.modelData.key === "s:paths" ? "draw-polyline" : "draw-cuboid"
                            implicitWidth: Kirigami.Units.iconSizes.small
                            implicitHeight: Kirigami.Units.iconSizes.small
                        }
                        // group: expand arrow, visibility, thumbnail or colour key
                        Kirigami.Icon {
                            visible: row.isGroup
                            source: row.modelData.expanded ? "arrow-down" : "arrow-right"
                            implicitWidth: Kirigami.Units.iconSizes.small
                            implicitHeight: Kirigami.Units.iconSizes.small
                            TapHandler { onTapped: ol.editor.toggleExpand(row.modelData.key) }
                        }
                        QQC2.CheckBox {
                            visible: row.isGroup
                            checked: row.modelData.visible !== false
                            padding: 0
                            onToggled: ol.editor.setGroupVisible(row.modelData.key, checked)
                            QQC2.ToolTip.text: "Show in the viewport"
                            QQC2.ToolTip.visible: hovered
                            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                        Item {
                            visible: row.isGroup || row.isItem
                            implicitWidth: Kirigami.Units.iconSizes.small
                            implicitHeight: Kirigami.Units.iconSizes.small
                            Image {
                                anchors.fill: parent
                                visible: row.isGroup && !!row.modelData.thumb
                                source: row.isGroup ? (row.modelData.thumb || "") : ""
                                sourceSize: Qt.size(32, 32)
                                fillMode: Image.PreserveAspectFit
                                asynchronous: true
                            }
                            // paths: a bar in the path colour; objects without a thumbnail: a dot in the category colour
                            Rectangle {
                                anchors.centerIn: parent
                                visible: row.isGroup && !row.modelData.thumb
                                width: row.isRail ? parent.width : 9
                                height: row.isRail ? 4 : 9
                                radius: height / 2
                                color: row.modelData.color || "transparent"
                            }
                            Rectangle {
                                anchors.centerIn: parent
                                visible: row.isItem
                                width: 5
                                height: 5
                                radius: 2.5
                                color: row.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
                            }
                        }
                        QQC2.Label {
                            text: row.isSection ? row.modelData.label.toUpperCase() : row.modelData.label
                            font.weight: row.isSection || (row.isGroup && row.modelData.selected) ? Font.DemiBold : Font.Normal
                            font.pointSize: row.isSection ? Kirigami.Theme.smallFont.pointSize : Kirigami.Theme.defaultFont.pointSize
                            font.letterSpacing: row.isSection ? 0.6 : 0
                            font.features: { "tnum": 1 }
                            color: row.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
                            opacity: row.isSection ? 0.8 : 1
                            elide: Text.ElideRight
                            Layout.fillWidth: !(row.isItem && row.modelData.sub)
                        }
                        QQC2.Label {
                            visible: row.isItem && !!row.modelData.sub
                            text: row.modelData.sub || ""
                            color: row.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                            font.pointSize: Kirigami.Theme.smallFont.pointSize
                        }
                        QQC2.Label {
                            visible: row.modelData.count !== undefined
                            text: row.modelData.count !== undefined ? row.modelData.count : ""
                            color: Kirigami.Theme.disabledTextColor
                            font.pointSize: Kirigami.Theme.smallFont.pointSize
                            font.features: { "tnum": 1 }
                            horizontalAlignment: Text.AlignRight
                            Layout.minimumWidth: Kirigami.Units.gridUnit * 1.5
                            Layout.rightMargin: Kirigami.Units.smallSpacing
                        }
                    }
                    QQC2.ToolTip.text: row.isGroup && !row.isRail ? row.modelData.name + (row.modelData.sub ? "\n" + row.modelData.sub : "") + "\n" + row.modelData.category : ""
                    QQC2.ToolTip.visible: hovered && QQC2.ToolTip.text.length > 0
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    onClicked: {
                        if (isSection)
                            return;
                        const additive = (Qt.application.keyboardModifiers & (Qt.ControlModifier | Qt.ShiftModifier)) !== 0;
                        if (isGroup && modelData.key.startsWith("a:"))
                            ol.editor.selectGroup(modelData.key);
                        else
                            ol.editor.selectHash(modelData.hash, additive);
                    }
                    onDoubleClicked: if (isGroup) ol.editor.toggleExpand(modelData.key)
                }
            }
        }

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 2
            visible: ol.editor.courseTitle.length === 0 || ol.rows.length <= 2 && search.text.length > 0
            icon.name: ol.editor.courseTitle.length === 0 ? "document-open" : "edit-find"
            text: ol.editor.courseTitle.length === 0 ? "No course open" : "No matches"
            explanation: ol.editor.courseTitle.length === 0 ? "Objects and paths appear here." : "Nothing is named \"" + search.text + "\"."
            helpfulAction: ol.editor.courseTitle.length === 0 && ol.editor.ready ? openHelp : null
            Kirigami.Action {
                id: openHelp
                text: "Open Course…"
                icon.name: "document-open"
                onTriggered: ol.openRequested()
            }
        }
    }
}
