pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import TrackStudioWorld

// Track Studio's "Change Object": swap the selected objects' type, keeping placement.
Kirigami.Dialog {
    id: dlg
    // dim the editor behind the dialog (set here: the default fade did not always end dimmed)
    QQC2.Overlay.modal: Rectangle {
        color: Qt.rgba(0, 0, 0, 0.45)
    }
    required property Editor editor
    // what gets replaced, e.g. "StartPosition" or "3 objects"
    property string target
    title: "Change Object"
    preferredWidth: Kirigami.Units.gridUnit * 34
    preferredHeight: Kirigami.Units.gridUnit * 26
    padding: Kirigami.Units.smallSpacing
    property var items: []
    property string error: ""
    footer: DialogFooter {
        acceptText: "Change"
        acceptIcon: "exchange-positions"
        acceptEnabled: list.currentIndex >= 0
        onAccepted: dlg.apply()
        onRejected: dlg.reject()
    }
    function refresh() {
        items = JSON.parse(editor.assets(search.text, "", true));
        list.currentIndex = -1;
    }
    function apply() {
        if (list.currentIndex < 0 || list.currentIndex >= items.length)
            return;
        error = editor.changeObject(items[list.currentIndex].name);
        if (error.length === 0)
            close();
    }
    onOpened: {
        error = "";
        refresh();
        search.forceActiveFocus();
    }
    ColumnLayout {
        QQC2.Label {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.smallSpacing
            visible: dlg.target.length > 0
            text: "Replacing <b>" + dlg.target + "</b>. Position, rotation, scale and links are kept."
            textFormat: Text.StyledText
            elide: Text.ElideRight
        }
        Kirigami.SearchField {
            id: search
            Layout.fillWidth: true
            onTextChanged: dlg.refresh()
            Keys.onDownPressed: list.incrementCurrentIndex()
            Keys.onUpPressed: list.decrementCurrentIndex()
            onAccepted: dlg.apply()
        }
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: dlg.error.length > 0
            type: Kirigami.MessageType.Error
            text: dlg.error
        }
        QQC2.ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            ListView {
                id: list
                clip: true
                model: dlg.items
                currentIndex: -1
                delegate: QQC2.ItemDelegate {
                    id: d
                    required property var modelData
                    required property int index
                    width: ListView.view.width
                    highlighted: ListView.isCurrentItem
                    // the picked row in the accent colour, as in Open Course
                    background: Rectangle {
                        readonly property color hl: Kirigami.Theme.highlightColor
                        color: d.highlighted ? hl : d.hovered ? Qt.rgba(hl.r, hl.g, hl.b, 0.18) : "transparent"
                        radius: 3
                    }
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Item {
                            implicitWidth: Kirigami.Units.iconSizes.medium
                            implicitHeight: Kirigami.Units.iconSizes.medium
                            Image {
                                anchors.fill: parent
                                visible: d.modelData.thumb.length > 0
                                source: d.modelData.thumb
                                sourceSize: Qt.size(64, 64)
                                fillMode: Image.PreserveAspectFit
                                asynchronous: true
                            }
                            // no thumbnail: a dot in the category colour
                            Rectangle {
                                anchors.centerIn: parent
                                visible: d.modelData.thumb.length === 0
                                width: 10
                                height: 10
                                radius: 5
                                color: d.modelData.color
                            }
                        }
                        QQC2.Label { text: d.modelData.short; color: d.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor; Layout.preferredWidth: Kirigami.Units.gridUnit * 13; elide: Text.ElideRight }
                        QQC2.Label { text: d.modelData.label; color: d.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor; Layout.fillWidth: true; elide: Text.ElideRight }
                        QQC2.Label { text: d.modelData.category; color: d.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor; font.pointSize: Kirigami.Theme.smallFont.pointSize }
                    }
                    // full label and internal name, below the row so it covers no other row's text
                    QQC2.ToolTip {
                        parent: d
                        y: d.height
                        x: Kirigami.Units.gridUnit * 2
                        visible: d.hovered
                        delay: Kirigami.Units.toolTipDelay
                        text: d.modelData.label + "\n" + d.modelData.name
                    }
                    onClicked: list.currentIndex = index
                    onDoubleClicked: {
                        list.currentIndex = index;
                        dlg.apply();
                    }
                }
            }
        }
    }
}
