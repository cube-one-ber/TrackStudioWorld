pragma ComponentBehavior: Bound

import QtQuick
import QtQuick3D
import QtQuick3D.Helpers
import TrackStudioWorld

// One material part of a model, geometry handed over from Rust.
Model {
    id: mp

    required property Editor editor
    required property string rel
    required property int part
    readonly property var d: editor.meshPart(rel, part)

    geometry: ProceduralMesh {
        positions: mp.d.positions
        normals: mp.d.normals
        uv0s: mp.d.uvs
        indexes: mp.d.indexes
    }
    materials: PrincipledMaterial {
        baseColor: mp.d.texture.length > 0 ? "white" : "#b8b8b8"
        baseColorMap: mp.d.texture.length > 0 ? tex : null
        roughness: 1
        metalness: 0
        cullMode: Material.NoCulling
        alphaMode: mp.d.cutout ? PrincipledMaterial.Mask : PrincipledMaterial.Opaque
        alphaCutoff: mp.d.alphaCutoff
    }
    Texture {
        id: tex
        source: mp.d.texture
        // model UVs put the texture's top row at V = 0 (as the game and the thumbnail renderer
        // sample it); Qt Quick 3D samples with the bottom row at 0
        flipV: true
        generateMipmaps: true
        mipFilter: Texture.Linear
    }
}
