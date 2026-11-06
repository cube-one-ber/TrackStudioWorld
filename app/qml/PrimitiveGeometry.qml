import QtQuick
import QtQuick3D.Helpers
import TrackStudioWorld

// Unit primitives built in Rust (box, sphere, cone along +Z, cylinder along Y, torus).
ProceduralMesh {
    required property Editor editor
    required property string shape
    readonly property var d: editor.primitive(shape)
    positions: d.positions
    normals: d.normals
}
