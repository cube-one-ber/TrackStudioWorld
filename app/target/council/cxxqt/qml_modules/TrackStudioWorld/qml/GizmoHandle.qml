import QtQuick
import QtQuick3D
import TrackStudioWorld

// One axis of the gizmo: shaft plus a cone (move) or cube (scale) tip, along local +Y.
Node {
    id: h
    required property int axis
    required property color color
    required property var shaftGeo
    required property var tipGeo
    property bool cubeTip: false

    Model {
        property int gizmoAxis: h.axis
        pickable: true
        geometry: h.shaftGeo
        position: Qt.vector3d(0, 0.42, 0)
        scale: Qt.vector3d(0.045, 0.84, 0.045)
        materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: h.color }
    }
    Model {
        property int gizmoAxis: h.axis
        pickable: true
        geometry: h.tipGeo
        position: Qt.vector3d(0, 0.92, 0)
        eulerRotation.x: h.cubeTip ? 0 : -90
        scale: h.cubeTip ? Qt.vector3d(0.13, 0.13, 0.13) : Qt.vector3d(0.15, 0.15, 0.3)
        materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: h.color }
    }
}
