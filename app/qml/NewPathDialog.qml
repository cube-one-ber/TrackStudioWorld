pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import TrackStudioWorld

// Pick a path type: for a new path (with its starting shape), or to change the selected paths
// into another type (Track Studio's path conversions).
Kirigami.Dialog {
    id: dlg
    // dim the editor behind the dialog (set here: the default fade did not always end dimmed)
    QQC2.Overlay.modal: Rectangle {
        color: Qt.rgba(0, 0, 0, 0.45)
    }
    required property Editor editor
    // "new" or "change"
    property string mode: "new"
    signal chosen(string name, string shape)
    title: mode === "new" ? "New Path" : "Change Path Type"
    preferredWidth: Kirigami.Units.gridUnit * 28
    padding: Kirigami.Units.smallSpacing
    standardButtons: Kirigami.Dialog.Cancel
    property var types: []
    property string error: ""
    onOpened: {
        types = JSON.parse(editor.railTypes());
        error = "";
    }
    ColumnLayout {
        QQC2.Label {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.smallSpacing
            wrapMode: Text.Wrap
            opacity: 0.75
            text: dlg.mode === "new"
                ? "Starts in front of the camera. Alt+Click adds points after the selected one (E extends it ahead), C connects two selected points, and Split, Join and Orient are in the Paths menu."
                : "The selected paths and their points get the new type. Parameters the new type doesn't have are dropped."
        }
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: dlg.error.length > 0
            type: Kirigami.MessageType.Error
            text: dlg.error
        }
        RowLayout {
            visible: dlg.mode === "new"
            Layout.margins: Kirigami.Units.smallSpacing
            QQC2.Label { text: "Shape:" }
            QQC2.ButtonGroup { id: shapes }
            Repeater {
                model: [["point", "One point", "draw-dot"], ["line", "Line", "draw-line"], ["curve", "Curve", "draw-bezier-curves"], ["circle", "Loop", "draw-circle"]]
                delegate: QQC2.ToolButton {
                    required property var modelData
                    required property int index
                    property string shape: modelData[0]
                    text: modelData[1]
                    icon.name: modelData[2]
                    display: QQC2.AbstractButton.TextBesideIcon
                    checkable: true
                    checked: index === 1
                    QQC2.ButtonGroup.group: shapes
                }
            }
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
                    if (dlg.mode === "new") {
                        dlg.close();
                        dlg.chosen(modelData.name, shapes.checkedButton ? shapes.checkedButton.shape : "line");
                    } else {
                        dlg.error = dlg.editor.changePathType(modelData.name);
                        if (dlg.error.length === 0)
                            dlg.close();
                    }
                }
            }
        }
    }
}
