pragma ComponentBehavior: Bound

import QtQuick
import QtQuick3D
import TrackStudioWorld

// Move / rotate / scale gizmo drawn in the overlay view; Rust does the drag maths.
Node {
    id: g

    required property Editor editor
    property var info: ({ visible: false })
    required property Camera camera
    // gizmo size in the top view (0 for the perspective camera, where it follows distance)
    property real orthoScale: 0
    property int hoverAxis: -1
    property int activeAxis: -1
    readonly property string mode: editor.gizmoMode

    position: visible ? Qt.vector3d(info.p[0], info.p[1], info.p[2]) : Qt.vector3d(0, 0, 0)
    rotation: visible ? Qt.quaternion(info.q[0], info.q[1], info.q[2], info.q[3]) : Qt.quaternion(1, 0, 0, 0)
    readonly property real s: orthoScale > 0 ? orthoScale : Math.max(0.01, camera.position.minus(position).length() * 0.13)
    scale: Qt.vector3d(s, s, s)

    function col(axis, base) {
        return activeAxis === axis || hoverAxis === axis ? "#ffe14d" : base;
    }

    PrimitiveGeometry { id: cyl; shape: "cylinder"; editor: g.editor }
    PrimitiveGeometry { id: cone; shape: "cone"; editor: g.editor }
    PrimitiveGeometry { id: cube; shape: "box"; editor: g.editor }
    PrimitiveGeometry { id: ball; shape: "sphere"; editor: g.editor }
    PrimitiveGeometry { id: ring; shape: "torus"; editor: g.editor }

    Node {
        visible: g.mode !== "rotate"
        GizmoHandle { axis: 0; color: g.col(0, "#ea4a3c"); shaftGeo: cyl; tipGeo: g.mode === "scale" ? cube : cone; cubeTip: g.mode === "scale"; eulerRotation.z: -90 }
        GizmoHandle { axis: 1; color: g.col(1, "#5bc945"); shaftGeo: cyl; tipGeo: g.mode === "scale" ? cube : cone; cubeTip: g.mode === "scale" }
        GizmoHandle { axis: 2; color: g.col(2, "#3d82ec"); shaftGeo: cyl; tipGeo: g.mode === "scale" ? cube : cone; cubeTip: g.mode === "scale"; eulerRotation.x: 90 }
        Model {
            visible: g.mode === "translate"
            property int gizmoAxis: 3
            pickable: true
            geometry: ball
            scale: Qt.vector3d(0.16, 0.16, 0.16)
            materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: g.col(3, "#f2f2f2") }
        }
    }
    Node {
        visible: g.mode === "rotate"
        Model {
            property int gizmoAxis: 0
            pickable: true
            geometry: ring
            eulerRotation.z: 90
            scale: Qt.vector3d(1.7, 1.7, 1.7)
            materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: g.col(0, "#ea4a3c"); cullMode: Material.NoCulling }
        }
        Model {
            property int gizmoAxis: 1
            pickable: true
            geometry: ring
            scale: Qt.vector3d(1.7, 1.7, 1.7)
            materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: g.col(1, "#5bc945"); cullMode: Material.NoCulling }
        }
        Model {
            property int gizmoAxis: 2
            pickable: true
            geometry: ring
            eulerRotation.x: 90
            scale: Qt.vector3d(1.7, 1.7, 1.7)
            materials: PrincipledMaterial { lighting: PrincipledMaterial.NoLighting; baseColor: g.col(2, "#3d82ec"); cullMode: Material.NoCulling }
        }
    }
}
