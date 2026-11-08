pragma ComponentBehavior: Bound

import QtQuick
import QtQuick3D
import TrackStudioWorld

// Every object that uses one model, drawn instanced: one geometry per part.
Node {
    id: group

    required property var modelData
    required property Editor editor
    property var entries: ({})

    InstanceList {
        id: ilist
    }
    Component {
        id: entryComp
        InstanceListEntry {}
    }

    function build() {
        const list = [];
        const map = {};
        for (const i of modelData.inst) {
            const e = entryComp.createObject(group, {
                position: Qt.vector3d(i.p[0], i.p[1], i.p[2]),
                rotation: Qt.quaternion(i.q[0], i.q[1], i.q[2], i.q[3]),
                scale: Qt.vector3d(i.s[0], i.s[1], i.s[2])
            });
            list.push(e);
            map[i.h] = e;
        }
        entries = map;
        ilist.instances = list;
    }
    function moveInstances(list) {
        for (const t of list) {
            const e = entries[t.h];
            if (!e)
                continue;
            e.position = Qt.vector3d(t.p[0], t.p[1], t.p[2]);
            e.rotation = Qt.quaternion(t.q[0], t.q[1], t.q[2], t.q[3]);
            e.scale = Qt.vector3d(t.s[0], t.s[1], t.s[2]);
        }
    }
    Component.onCompleted: build()

    Repeater3D {
        model: group.modelData.parts
        delegate: MeshPart {
            required property int index
            editor: group.editor
            rel: group.modelData.rel
            part: index
            instancing: ilist
        }
    }
}
