pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import TrackStudioWorld

Kirigami.Dialog {
    id: dlg
    required property Editor editor
    signal chosen(string name)
    title: "New Path"
    preferredWidth: Kirigami.Units.gridUnit * 26
    padding: Kirigami.Units.smallSpacing
    standardButtons: Kirigami.Dialog.Cancel
    property var types: []
    onOpened: types = JSON.parse(editor.railTypes())
    ColumnLayout {
        QQC2.Label {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.smallSpacing
            wrapMode: Text.Wrap
            opacity: 0.75
            text: "Starts with two points in front of the camera. Select the last point and press I to extend it; L links it to another path."
        }
        Repeater {
            model: dlg.types
            delegate: QQC2.ItemDelegate {
                id: d
                required property var modelData
                Layout.fillWidth: true
                contentItem: RowLayout {
                    Rectangle { width: 12; height: 12; radius: 2; color: d.modelData.color }
                    QQC2.Label { text: d.modelData.name; Layout.fillWidth: true }
                    QQC2.Label { text: d.modelData.type + (d.modelData.count > 0 ? "  ·  " + d.modelData.count + " here" : ""); opacity: 0.6 }
                }
                onClicked: {
                    dlg.close();
                    dlg.chosen(modelData.name);
                }
            }
        }
    }
}
