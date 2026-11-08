pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Label plus 2 or 3 number fields (X, Y, Z).
RowLayout {
    id: vr
    property string label
    property var values: [0, 0, 0]
    property int count: 3
    property int decimals: 3
    signal committed(var v)
    Layout.fillWidth: true
    spacing: Kirigami.Units.smallSpacing

    QQC2.Label {
        text: vr.label
        Layout.preferredWidth: Kirigami.Units.gridUnit * 5.5
        Layout.maximumWidth: Kirigami.Units.gridUnit * 5.5
        elide: Text.ElideRight
    }
    Repeater {
        model: vr.count
        delegate: QQC2.TextField {
            id: tf
            required property int index
            readonly property color axis: ["#e5534b", "#57ab5a", "#539bf5"][index]
            Layout.fillWidth: true
            Layout.minimumWidth: Kirigami.Units.gridUnit * 3
            horizontalAlignment: Text.AlignRight
            leftPadding: 7
            rightPadding: 5
            font.features: { "tnum": 1 }
            // "-0.000" reads as a bug: show it as "0.000"
            text: vr.values && vr.values[index] !== undefined && vr.values[index] !== null ? Number(vr.values[index]).toFixed(vr.decimals).replace(/^-(0\.?0*)$/, "$1") : ""
            validator: DoubleValidator { notation: DoubleValidator.StandardNotation }
            selectByMouse: true
            QQC2.ToolTip.text: ["X", "Y", "Z"][index]
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            // neutral field with a thin axis-coloured edge (X red, Y green, Z blue)
            background: Rectangle {
                color: Qt.darker(Kirigami.Theme.backgroundColor, tf.activeFocus ? 1.3 : 1.15)
                border.color: tf.activeFocus ? Kirigami.Theme.focusColor
                            : tf.hovered ? Kirigami.Theme.hoverColor
                            : Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.14)
                border.width: 1
                radius: 3
                Rectangle {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.margins: 1
                    width: 3
                    radius: 1.5
                    color: tf.axis
                }
            }
            onEditingFinished: {
                const v = vr.values.slice(0, vr.count).map(Number);
                const x = parseFloat(text);
                if (isNaN(x) || x === v[index])
                    return;
                v[index] = x;
                vr.committed(v);
            }
        }
    }
    // two fields: an empty third slot, so the columns line up with the X, Y, Z rows
    Item {
        visible: vr.count < 3
        Layout.fillWidth: true
        Layout.minimumWidth: Kirigami.Units.gridUnit * 3
    }
}
