pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import TrackStudioWorld

Kirigami.Dialog {
    id: dlg
    // dim the editor behind the dialog (set here: the default fade did not always end dimmed)
    QQC2.Overlay.modal: Rectangle {
        color: Qt.rgba(0, 0, 0, 0.45)
    }
    required property Editor editor
    // called with the open as a function, to ask about unsaved changes first
    property var guard: null
    title: "Open Course"
    preferredWidth: Kirigami.Units.gridUnit * 48
    preferredHeight: Kirigami.Units.gridUnit * 34
    padding: Kirigami.Units.smallSpacing
    onAccepted: dlg.openCurrent()
    footer: DialogFooter {
        acceptText: "Open"
        acceptIcon: "document-open"
        acceptEnabled: list.currentIndex >= 0
        onAccepted: dlg.accept()
        onRejected: dlg.reject()
    }
    property var files: []

    function refresh() {
        files = JSON.parse(editor.courseList(search.text));
        list.currentIndex = files.length > 0 ? 0 : -1;
    }
    function openCurrent() {
        if (list.currentIndex >= 0 && list.currentIndex < files.length)
            openItem(files[list.currentIndex]);
    }
    function browseFile() {
        fileDialog.open();
    }
    onOpened: {
        mods.model = ["Game files"].concat(JSON.parse(editor.modList()));
        refresh();
        search.forceActiveFocus();
    }
    function openItem(item) {
        dlg.close();
        let m = mods.currentIndex > 0 ? mods.currentText : "";
        // Custom course slots only exist in mods: open from the mod that provides them.
        if (m.length === 0 && item.custom === true && item.mods && item.mods.length > 0)
            m = item.mods[0];
        const go = () => dlg.editor.openCourse(item.file, m);
        if (dlg.guard)
            dlg.guard(go);
        else
            go();
    }
    function openExternalPath(path) {
        dlg.close();
        const go = () => dlg.editor.openCourseFile(path);
        if (dlg.guard)
            dlg.guard(go);
        else
            go();
    }

    FileDialog {
        id: fileDialog
        title: "Open Course File"
        nameFilters: ["Course files (*.bcett.byml.zs *.bcett.byml)", "All files (*)"]
        onAccepted: dlg.openExternalPath(decodeURIComponent(selectedFile.toString()))
    }

    ColumnLayout {
        spacing: Kirigami.Units.smallSpacing
        RowLayout {
            Layout.fillWidth: true
            Kirigami.SearchField {
                id: search
                Layout.fillWidth: true
                placeholderText: "Search (FirstCircuit, Race_, Rainbow, custom slots…)"
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
            QQC2.Button {
                text: "Open File…"
                icon.name: "document-open-folder"
                onClicked: fileDialog.open()
                QQC2.ToolTip.text: "Open any .bcett.byml.zs or .bcett.byml course file from disk (including custom track slots)"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
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
                        Kirigami.Chip {
                            visible: d.modelData.custom === true
                            text: "Custom slot"
                            closable: false
                            checkable: false
                        }
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
                    onDoubleClicked: dlg.openItem(modelData)
                }
            }
        }
    }
}
