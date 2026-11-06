pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.mkw.trackeditor

// Selection properties: Transform / Edit / Object / Parameters / Links / Path data / Raw.
QQC2.ScrollView {
    id: pp
    required property Editor editor
    signal focusRequested()
    signal changeObjectRequested()
    signal duplicateRequested()
    property var s: ({ kind: "none" })
    // parameter names are identifiers: the label column fits the longest one (up to 45% of the panel)
    readonly property real paramLabelWidth: {
        let w = Kirigami.Units.gridUnit * 5.5;
        for (const p of (s.params || []))
            w = Math.max(w, boldMetrics.advanceWidth(p.name) + 2);
        return Math.min(Math.ceil(w), availableWidth * 0.45);
    }
    FontMetrics {
        id: boldMetrics
        font.weight: Font.DemiBold
    }
    property string paramError: ""
    contentWidth: availableWidth

    function refresh() {
        s = JSON.parse(editor.selection());
    }
    Connections {
        target: pp.editor
        function onRevisionChanged() { pp.refresh(); }
    }
    Component.onCompleted: refresh()

    ColumnLayout {
        width: pp.availableWidth
        // with no course the empty state fills the panel, so it sits in the middle
        height: pp.s.kind === "none" ? pp.availableHeight : implicitHeight
        spacing: Kirigami.Units.smallSpacing

        // header: thumbnail or kind icon, name, game label, tags
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            Layout.bottomMargin: Kirigami.Units.smallSpacing
            spacing: Kirigami.Units.largeSpacing
            visible: pp.s.kind !== "none"
            Rectangle {
                Layout.alignment: Qt.AlignTop
                implicitWidth: Kirigami.Units.gridUnit * 3.2
                implicitHeight: implicitWidth
                radius: Kirigami.Units.cornerRadius
                color: Kirigami.Theme.alternateBackgroundColor
                Image {
                    anchors.fill: parent
                    anchors.margins: 3
                    visible: !!pp.s.thumb
                    source: pp.s.thumb || ""
                    sourceSize: Qt.size(128, 128)
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                }
                Kirigami.Icon {
                    anchors.centerIn: parent
                    visible: !pp.s.thumb
                    implicitWidth: Kirigami.Units.iconSizes.medium
                    implicitHeight: Kirigami.Units.iconSizes.medium
                    source: pp.s.kind === "point" || pp.s.kind === "rail" ? "draw-polyline"
                          : pp.s.kind === "multi" ? "edit-select-all"
                          : pp.s.kind === "course" ? "map-flat" : "draw-cuboid"
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Kirigami.Heading {
                    level: 4
                    Layout.fillWidth: true
                    wrapMode: Text.WrapAnywhere
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    text: pp.s.kind === "actor" ? pp.s.name
                        : pp.s.kind === "point" ? pp.s.rail + "  ·  point " + pp.s.index
                        : pp.s.kind === "rail" ? pp.s.name
                        : pp.s.kind === "multi" ? pp.s.count + " selected"
                        : pp.s.kind === "course" ? pp.s.file.replace("Banc/", "").replace(".bcett.byml.zs", "") : ""
                }
                QQC2.Label {
                    visible: pp.s.kind === "actor" && !!pp.s.label
                    text: pp.s.label || ""
                    color: Kirigami.Theme.disabledTextColor
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                }
                Flow {
                    Layout.fillWidth: true
                    Layout.topMargin: 2
                    spacing: Kirigami.Units.smallSpacing
                    Repeater {
                        model: [
                            pp.s.category || "",
                            pp.s.collect ? "counts as " + pp.s.collect : "",
                            pp.s.kind === "actor" && !pp.s.hasModel ? "no exported model" : "",
                            pp.s.kind === "point" || pp.s.kind === "rail" ? (pp.s.count || 0) + " points" : "",
                            pp.s.kind === "rail" && pp.s.closed === true ? "closed loop" : ""
                        ].filter(t => t.length > 0)
                        delegate: Rectangle {
                            id: tag
                            required property string modelData
                            implicitWidth: tagText.implicitWidth + Kirigami.Units.largeSpacing
                            implicitHeight: tagText.implicitHeight + 4
                            radius: height / 2
                            color: Qt.rgba(Kirigami.Theme.highlightColor.r, Kirigami.Theme.highlightColor.g, Kirigami.Theme.highlightColor.b, 0.3)
                            border.color: Qt.rgba(Kirigami.Theme.highlightColor.r, Kirigami.Theme.highlightColor.g, Kirigami.Theme.highlightColor.b, 0.7)
                            QQC2.Label {
                                id: tagText
                                anchors.centerIn: parent
                                text: tag.modelData
                                font.pointSize: Kirigami.Theme.smallFont.pointSize
                            }
                        }
                    }
                }
                QQC2.Label {
                    visible: !!pp.s.hash
                    text: "Hash " + (pp.s.hash || "")
                    color: Kirigami.Theme.disabledTextColor
                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                    font.features: { "tnum": 1 }
                    textFormat: Text.PlainText
                    QQC2.ToolTip.text: "Unique ID of this object in the course file"
                    QQC2.ToolTip.visible: hashHover.hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    HoverHandler { id: hashHover }
                }
            }
        }

        // nothing selected: course summary
        Kirigami.FormLayout {
            visible: pp.s.kind === "course"
            Layout.fillWidth: true
            QQC2.Label { Kirigami.FormData.label: "Objects:"; text: pp.s.objects || 0 }
            QQC2.Label { Kirigami.FormData.label: "Paths:"; text: (pp.s.paths || 0) + " (" + (pp.s.points || 0) + " points)" }
            QQC2.Label { Kirigami.FormData.label: "Collects:"; text: pp.s.collects || "–"; wrapMode: Text.Wrap }
            QQC2.Label { Kirigami.FormData.label: "Path data:"; text: pp.s.padFile ? pp.s.padEntries + " entries" : "none"; wrapMode: Text.Wrap }
            QQC2.Label { Kirigami.FormData.label: "Source:"; text: pp.s.fromMod ? "mod \"" + pp.editor.modName + "\"" : "game files" }
        }
        Kirigami.PlaceholderMessage {
            visible: pp.s.kind === "none"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.margins: Kirigami.Units.largeSpacing
            icon.name: "document-open"
            text: "No course open"
            explanation: "Open a course to edit its objects and paths."
        }
        Kirigami.PlaceholderMessage {
            visible: pp.s.kind === "course"
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            icon.name: "edit-select"
            text: "Nothing selected"
            explanation: "Click an object or path point. Ctrl+Click adds to the selection."
        }

        Section {
            title: "Transform"
            visible: pp.s.kind === "actor" || pp.s.kind === "point"
            VecRow {
                label: "Translate"
                values: pp.s.translate || [0, 0, 0]
                onCommitted: (v) => pp.editor.setVector("Translate", v[0], v[1], v[2])
            }
            VecRow {
                label: "Rotate °"
                values: pp.s.rotate || [0, 0, 0]
                onCommitted: (v) => pp.editor.setVector("Rotate", v[0], v[1], v[2])
            }
            VecRow {
                visible: pp.s.kind === "actor"
                label: "Scale"
                values: pp.s.scale || [1, 1, 1]
                onCommitted: (v) => pp.editor.setVector("Scale", v[0], v[1], v[2])
            }
            VecRow {
                visible: !!pp.s.rectSize
                label: "Rect size"
                count: 2
                values: pp.s.rectSize || [0, 0]
                onCommitted: (v) => pp.editor.setVector("RectSize", v[0], v[1], 0)
            }
            VecRow {
                visible: !!pp.s.rectOffset
                label: "Rect offset"
                count: 2
                values: pp.s.rectOffset || [0, 0]
                onCommitted: (v) => pp.editor.setVector("RectOffset", v[0], v[1], 0)
            }
            VecRow {
                visible: !!pp.s.control0
                label: "Curve in"
                values: pp.s.control0 || [0, 0, 0]
                onCommitted: (v) => pp.editor.setVector("Control0", v[0], v[1], v[2])
            }
            VecRow {
                visible: !!pp.s.control1
                label: "Curve out"
                values: pp.s.control1 || [0, 0, 0]
                onCommitted: (v) => pp.editor.setVector("Control1", v[0], v[1], v[2])
            }
        }

        Section {
            title: "Edit"
            visible: pp.s.kind === "actor" || pp.s.kind === "point" || pp.s.kind === "rail" || pp.s.kind === "multi"
            // equal cells, so buttons keep their places from one selection to the next
            GridLayout {
                Layout.fillWidth: true
                columns: 2
                uniformCellWidths: true
                columnSpacing: Kirigami.Units.smallSpacing
                rowSpacing: Kirigami.Units.smallSpacing
                QQC2.Button { visible: pp.s.kind === "actor"; Layout.fillWidth: true; text: "Change Object…"; icon.name: "exchange-positions"; onClicked: pp.changeObjectRequested() }
                QQC2.Button { visible: pp.s.kind === "point"; Layout.fillWidth: true; text: "Insert Point After"; icon.name: "node-add"; onClicked: pp.editor.insertPoint() }
                QQC2.Button { visible: pp.s.kind === "point"; Layout.fillWidth: true; text: pp.s.control0 ? "Remove Curve Handles" : "Add Curve Handles"; icon.name: "draw-bezier-curves"; onClicked: pp.editor.toggleCurve() }
                QQC2.Button { visible: pp.s.kind === "point"; Layout.fillWidth: true; text: "Select Path"; icon.name: "draw-polyline"; onClicked: pp.editor.selectHash(pp.s.railHash, false) }
                QQC2.Button { visible: pp.s.kind === "point" || pp.s.kind === "rail"; Layout.fillWidth: true; text: "Reverse Path"; icon.name: "media-playlist-repeat"; onClicked: pp.editor.reverseRail() }
                QQC2.Button { Layout.fillWidth: true; text: "Drop to Ground"; icon.name: "go-bottom"; enabled: pp.editor.groundReady; onClicked: pp.editor.dropToGround() }
                QQC2.Button { Layout.fillWidth: true; text: "Focus Selection"; icon.name: "crosshairs"; onClicked: pp.focusRequested() }
                QQC2.Button { Layout.fillWidth: true; text: "Duplicate"; icon.name: "edit-copy"; onClicked: pp.duplicateRequested() }
                QQC2.Button { Layout.fillWidth: true; text: "Delete"; icon.name: "edit-delete"; onClicked: pp.editor.deleteSelected() }
            }
            QQC2.CheckBox {
                visible: pp.s.kind === "rail"
                text: "Closed loop"
                checked: pp.s.closed === true
                onToggled: pp.editor.setClosed(checked)
            }
        }

        Section {
            title: "Parameters"
            visible: (pp.s.kind === "actor" || pp.s.kind === "point" || pp.s.kind === "rail")
            QQC2.Label {
                Layout.fillWidth: true
                Layout.bottomMargin: 2
                visible: !!pp.s.paramTable && pp.s.paramTable !== pp.s.name
                text: "From the " + (pp.s.paramTable || "") + " parameter table"
                color: Kirigami.Theme.disabledTextColor
                font.pointSize: Kirigami.Theme.smallFont.pointSize
                elide: Text.ElideMiddle
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: pp.paramError.length > 0
                type: Kirigami.MessageType.Error
                text: pp.paramError
            }
            QQC2.Label {
                visible: !pp.s.params || pp.s.params.length === 0
                text: "This type has no parameter table."
                opacity: 0.6
            }
            Repeater {
                model: pp.s.params || []
                delegate: ParamRow {
                    required property var modelData
                    p: modelData
                    labelWidth: pp.paramLabelWidth
                    onSetValue: (name, kind, text) => pp.paramError = pp.editor.setParam(name, kind, text)
                    onUnset: (name) => pp.editor.removeParam(name)
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.smallSpacing
                spacing: Kirigami.Units.smallSpacing
                QQC2.TextField { id: newKey; placeholderText: "Add a parameter…"; Layout.fillWidth: true }
                QQC2.ComboBox { id: newType; model: ["s32", "f32", "bool", "u32", "str"]; Layout.preferredWidth: Kirigami.Units.gridUnit * 4.5 }
                QQC2.Button {
                    icon.name: "list-add"
                    text: "Add"
                    enabled: newKey.text.length > 0
                    onClicked: {
                        pp.paramError = pp.editor.setParam(newKey.text, newType.currentText, newType.currentText === "bool" ? "false" : (newType.currentText === "str" ? "" : "0"));
                        newKey.text = "";
                    }
                }
            }
        }

        Section {
            title: "Links (" + (pp.s.links ? pp.s.links.length : 0) + ")"
            visible: pp.s.kind === "actor" || pp.s.kind === "point"
            Repeater {
                model: pp.s.links || []
                delegate: RowLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    QQC2.Label { text: modelData.type; Layout.preferredWidth: Kirigami.Units.gridUnit * 5.5; elide: Text.ElideRight }
                    Kirigami.LinkButton {
                        text: modelData.target
                        enabled: modelData.found
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        onClicked: pp.editor.selectHash(modelData.dst, false)
                    }
                    QQC2.ToolButton { icon.name: "edit-delete-remove"; onClicked: pp.editor.removeLink(modelData.i) }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                QQC2.ComboBox { id: linkType; model: ["BasicSignal", "MessageLink", "CreateLink"] }
                QQC2.Button {
                    Layout.fillWidth: true
                    text: pp.editor.linkPicking ? "Click a target…" : "Link To…"
                    icon.name: "link"
                    highlighted: pp.editor.linkPicking
                    onClicked: pp.editor.linkPicking ? pp.editor.cancelLink() : pp.editor.startLink(linkType.currentText)
                }
            }
        }

        Section {
            title: "Path Data"
            visible: pp.s.kind === "point" && (pp.s.pad || []).length > 0
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.7
                text: "Per-point widths and clearances from Banc/PathAdditionalData. Moving the point does not regenerate them."
            }
            Repeater {
                model: pp.s.kind === "point" ? (pp.s.pad || []) : []
                delegate: RowLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    QQC2.Label { text: modelData.key; Layout.preferredWidth: Kirigami.Units.gridUnit * 5.5; Layout.maximumWidth: Kirigami.Units.gridUnit * 5.5; elide: Text.ElideRight }
                    QQC2.TextField {
                        Layout.fillWidth: true
                        text: String(modelData.value)
                        onEditingFinished: if (text !== String(modelData.value)) pp.paramError = pp.editor.setPad(modelData.group, modelData.key, modelData.type, text)
                    }
                }
            }
        }

        Section {
            title: "Other Fields"
            expanded: false
            visible: pp.s.raw !== undefined
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.7
                text: "Typed JSON: plain numbers are f32; {\"$s32\":1}, {\"$u32\":1}, {\"$u64\":\"…\"} keep other BYML types."
            }
            QQC2.TextArea {
                id: raw
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 10
                text: pp.s.raw || ""
                font.family: "monospace"
                wrapMode: TextEdit.Wrap
            }
            RowLayout {
                QQC2.Button {
                    text: "Apply"
                    icon.name: "dialog-ok-apply"
                    onClicked: pp.paramError = pp.editor.setRaw(raw.text)
                }
                QQC2.Label { text: pp.paramError; color: Kirigami.Theme.negativeTextColor; Layout.fillWidth: true; wrapMode: Text.Wrap }
            }
        }

        Item { Layout.fillHeight: true }
    }
}
