pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import TrackStudioWorld

// Place copies of the current asset along the selected path (a line of coins, say).
Kirigami.Dialog {
    id: dlg
    // dim the editor behind the dialog (set here: the default fade did not always end dimmed)
    QQC2.Overlay.modal: Rectangle {
        color: Qt.rgba(0, 0, 0, 0.45)
    }
    required property Editor editor
    property string asset
    property string error: ""
    title: "Place Along Path"
    preferredWidth: Kirigami.Units.gridUnit * 26
    padding: Kirigami.Units.largeSpacing
    footer: DialogFooter {
        acceptText: "Place"
        acceptIcon: "list-add"
        onAccepted: dlg.accept()
        onRejected: dlg.reject()
    }
    onOpened: error = ""
    onAccepted: {
        error = editor.placeAlong(asset, spacing.realValue);
        if (error.length > 0)
            open();
    }
    ColumnLayout {
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: dlg.error.length > 0
            type: Kirigami.MessageType.Error
            text: dlg.error
        }
        Kirigami.FormLayout {
            Layout.fillWidth: true
            QQC2.Label {
                Kirigami.FormData.label: "Object:"
                text: dlg.asset
                elide: Text.ElideMiddle
                Layout.maximumWidth: Kirigami.Units.gridUnit * 16
            }
            QQC2.SpinBox {
                id: spacing
                Kirigami.FormData.label: "Every:"
                from: 5
                to: 5000
                stepSize: 5
                value: 50
                editable: true
                readonly property real realValue: value / 10
                textFromValue: (v) => (v / 10).toFixed(1) + " m"
                valueFromText: (t) => Math.round(parseFloat(t) * 10)
            }
        }
        QQC2.Label {
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
            wrapMode: Text.Wrap
            opacity: 0.7
            text: "From the path's first point to its last, following its curves. Each object faces along the path and sits on the stage surface when it is loaded. Pick another object in Assets first to place that instead."
        }
    }
}
