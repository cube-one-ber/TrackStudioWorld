pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.mkw.trackeditor

// Track Studio's "Change Object": swap the selected objects' type, keeping placement.
Kirigami.Dialog {
    id: dlg
    required property Editor editor
    title: "Change Object"
    preferredWidth: Kirigami.Units.gridUnit * 34
    preferredHeight: Kirigami.Units.gridUnit * 30
    padding: Kirigami.Units.smallSpacing
    standardButtons: Kirigami.Dialog.Cancel
    property var items: []
    property string error: ""
    onOpened: {
        error = "";
        items = JSON.parse(editor.assets(search.text, "", true));
        search.forceActiveFocus();
    }
    ColumnLayout {
        Kirigami.SearchField {
            id: search
            Layout.fillWidth: true
            onTextChanged: dlg.items = JSON.parse(dlg.editor.assets(text, "", true))
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
                clip: true
                model: dlg.items
                delegate: QQC2.ItemDelegate {
                    id: d
                    required property var modelData
                    width: ListView.view.width
                    contentItem: RowLayout {
                        Rectangle { width: 10; height: 10; radius: 2; color: d.modelData.color }
                        QQC2.Label { text: d.modelData.name; Layout.preferredWidth: Kirigami.Units.gridUnit * 16; elide: Text.ElideRight }
                        QQC2.Label { text: d.modelData.label; opacity: 0.65; Layout.fillWidth: true; elide: Text.ElideRight }
                    }
                    onClicked: {
                        dlg.error = dlg.editor.changeObject(modelData.name);
                        if (dlg.error.length === 0)
                            dlg.close();
                    }
                }
            }
        }
    }
}
