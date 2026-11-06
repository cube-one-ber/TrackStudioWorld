pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import TrackStudioWorld

Kirigami.Dialog {
    id: dlg
    required property Editor editor
    title: "Open Course"
    preferredWidth: Kirigami.Units.gridUnit * 46
    preferredHeight: Kirigami.Units.gridUnit * 34
    padding: Kirigami.Units.smallSpacing
    standardButtons: Kirigami.Dialog.Open | Kirigami.Dialog.Cancel
    onAccepted: dlg.openCurrent()
    Component.onCompleted: {
        const open = standardButton(Kirigami.Dialog.Open);
        if (open) {
            open.highlighted = true;  // the default (Enter) action
            open.enabled = Qt.binding(() => list.currentIndex >= 0);
        }
    }
    property var files: []

    function refresh() {
        files = JSON.parse(editor.courseList(search.text));
        list.currentIndex = files.length > 0 ? 0 : -1;
    }
    function openCurrent() {
        if (list.currentIndex >= 0 && list.currentIndex < files.length)
            openFile(files[list.currentIndex].file);
    }
    onOpened: {
        mods.model = ["Game files"].concat(JSON.parse(editor.modList()));
        refresh();
        search.forceActiveFocus();
    }
    function openFile(f) {
        dlg.close();
        editor.openCourse(f, mods.currentIndex > 0 ? mods.currentText : "");
    }

    ColumnLayout {
        spacing: Kirigami.Units.smallSpacing
        RowLayout {
            Layout.fillWidth: true
            Kirigami.SearchField {
                id: search
                Layout.fillWidth: true
                placeholderText: "Search (FirstCircuit, Race_, Rainbow, 荒野…)"
                // accepted is Enter only; with autoAccept it fires on every keystroke
                autoAccept: false
                onTextChanged: dlg.refresh()
                onAccepted: dlg.openCurrent()
                // arrows move through the list while typing
                Keys.onDownPressed: list.incrementCurrentIndex()
                Keys.onUpPressed: list.decrementCurrentIndex()
            }
            QQC2.Label { text: "From:" }
            QQC2.ComboBox {
                id: mods
                model: ["Game files"]
            }
        }
        QQC2.ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: Kirigami.Units.gridUnit * 28
            ListView {
                id: list
                clip: true
                model: dlg.files
                section.property: "group"
                section.delegate: Kirigami.ListSectionHeader {
                    required property string section
                    text: section
                    width: ListView.view.width
                }
                highlightMoveDuration: 0
                keyNavigationEnabled: true
                delegate: QQC2.ItemDelegate {
                    id: d
                    required property var modelData
                    required property int index
                    width: ListView.view.width
                    highlighted: ListView.isCurrentItem
                    background: Rectangle {
                        readonly property color hl: Kirigami.Theme.highlightColor
                        color: d.highlighted ? hl : d.hovered ? Qt.rgba(hl.r, hl.g, hl.b, 0.15) : "transparent"
                    }
                    contentItem: RowLayout {
                        QQC2.Label { text: d.modelData.name; color: d.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor; Layout.preferredWidth: Kirigami.Units.gridUnit * 20; elide: Text.ElideRight }
                        QQC2.Label { text: d.modelData.display; color: d.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor; Layout.fillWidth: true; elide: Text.ElideRight }
                        Repeater {
                            model: d.modelData.mods
                            delegate: Kirigami.Chip {
                                required property string modelData
                                text: modelData
                                closable: false
                                checkable: false
                            }
                        }
                    }
                    // click selects, double-click (or Enter, or Open) opens
                    onClicked: list.currentIndex = index
                    onDoubleClicked: dlg.openFile(modelData.file)
                }
            }
        }
    }
}
