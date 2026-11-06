pragma ComponentBehavior: Bound

import QtQuick
import QtQuick3D
import QtQuick3D.Helpers
import TrackStudioWorld

// Paths: lines, direction chevrons, checkpoint gates and point markers, one node per path.
Node {
    id: layer

    required property Editor editor
    property int count: 0

    function refreshRail(i) {
        const r = rep.objectAt(i);
        if (r)
            r.refresh();
    }

    Repeater3D {
        id: rep
        model: layer.count
        delegate: Node {
            id: rail
            required property int index
            property var d: layer.editor.railMesh(index)
            function refresh() {
                d = layer.editor.railMesh(index);
            }
            LineModel {
                lines: rail.d.lines
            }
            Model {
                visible: rail.d.markers.count > 0
                geometry: ProceduralMesh {
                    positions: rail.d.markers.positions
                    colors: rail.d.markers.colors
                }
                materials: PrincipledMaterial {
                    lighting: PrincipledMaterial.NoLighting
                    vertexColorsEnabled: true
                    cullMode: Material.NoCulling
                }
            }
        }
    }
}
