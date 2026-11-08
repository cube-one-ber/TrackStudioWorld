import QtQuick
import QtQuick3D
import QtQuick3D.Helpers

// Coloured line segments from Rust ({positions, colors, count}).
Model {
    id: lm
    property var lines: ({})
    visible: lines.count !== undefined && lines.count > 0
    geometry: ProceduralMesh {
        primitiveMode: ProceduralMesh.Lines
        positions: lm.lines.positions !== undefined ? lm.lines.positions : []
        colors: lm.lines.colors !== undefined ? lm.lines.colors : []
    }
    materials: PrincipledMaterial {
        lighting: PrincipledMaterial.NoLighting
        vertexColorsEnabled: true
        baseColor: "white"
    }
}
