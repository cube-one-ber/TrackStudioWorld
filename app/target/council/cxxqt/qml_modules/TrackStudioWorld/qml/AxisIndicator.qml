pragma ComponentBehavior: Bound

import QtQuick

// World axes as seen by the camera (bottom-left of the viewport), drawn flat like a navigation
// gizmo: a labelled ball on a shaft per positive axis, a faint ball per negative axis.
Item {
    id: ax
    property quaternion camRotation
    implicitWidth: 76
    implicitHeight: 76

    readonly property real reach: width / 2 - 10
    readonly property var axes: {
        const inv = camRotation.inverted();
        const defs = [["X", Qt.vector3d(1, 0, 0), "#e5534b", "#66e5534b"],
                      ["Y", Qt.vector3d(0, 1, 0), "#57ab5a", "#6657ab5a"],
                      ["Z", Qt.vector3d(0, 0, 1), "#539bf5", "#66539bf5"]];
        const list = [];
        for (const d of defs) {
            const v = inv.times(d[1]);  // camera space: x right, y up, z toward the viewer
            list.push({ label: d[0], x: v.x, y: -v.y, z: v.z, color: d[2], fill: d[2], pos: true });
            list.push({ label: "", x: -v.x, y: v.y, z: -v.z, color: d[2], fill: d[3], pos: false });
        }
        return list.sort((a, b) => a.z - b.z);  // far side first
    }

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: Qt.rgba(0, 0, 0, 0.3)
        border.color: Qt.rgba(1, 1, 1, 0.12)
    }
    Repeater {
        model: ax.axes
        delegate: Item {
            id: axis
            required property var modelData
            required property int index
            anchors.fill: parent
            z: index
            Rectangle {
                visible: axis.modelData.pos
                x: ax.width / 2
                y: ax.height / 2 - 1
                width: ax.reach * Math.hypot(axis.modelData.x, axis.modelData.y)
                height: 2
                transformOrigin: Item.Left
                rotation: Math.atan2(axis.modelData.y, axis.modelData.x) * 180 / Math.PI
                color: axis.modelData.color
                antialiasing: true
            }
            Rectangle {
                readonly property real d: axis.modelData.pos ? 17 : 11
                x: ax.width / 2 + axis.modelData.x * ax.reach - d / 2
                y: ax.height / 2 + axis.modelData.y * ax.reach - d / 2
                width: d
                height: d
                radius: d / 2
                color: axis.modelData.fill
                border.color: axis.modelData.color
                antialiasing: true
                Text {
                    anchors.centerIn: parent
                    text: axis.modelData.label
                    color: "#16181c"
                    font.pixelSize: 11
                    font.bold: true
                }
            }
        }
    }
}
