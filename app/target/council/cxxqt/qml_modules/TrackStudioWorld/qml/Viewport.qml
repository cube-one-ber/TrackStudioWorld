pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick3D
import QtQuick3D.Helpers
import org.kde.kirigami as Kirigami
import TrackStudioWorld

FocusScope {
    id: vp

    required property Editor editor
    property string currentAsset
    // what is selected, as the Properties panel shows it
    property var selection: ({ kind: "none" })
    // the window's actions, for the context menu (copy, paste, delete...)
    property var actions: ({})
    signal cursorChanged(string text)
    signal requestNewPath()
    signal openRequested()

    // camera: position + yaw/pitch (degrees); Qt cameras look down -Z
    property vector3d camPos: Qt.vector3d(0, 400, 600)
    property real yaw: 0
    property real pitch: -35
    property real moveSpeed: 40
    property real fov: 55
    property real speedScale: 1
    // top view: an orthographic camera straight above camPos; topZoom is pixels per metre
    property bool topView: false
    property real topZoom: 1
    readonly property real topHeight: 30000
    property var perspectiveBefore: null
    // "lit", "unlit", "normals" or "wireframe"
    property string shading: "lit"
    property bool xray: true
    property var stagesModel: []
    property var groupsModel: []
    property var boxesModel: []
    property int railCount: 0
    property var linksData: ({})
    property var selectionData: ({})
    property var gizmoInfo: ({ visible: false })
    property var viewInfo: ({ rails: [], categories: [] })
    property var keys: ({})

    // G, R and T: move, rotate or scale with the mouse ("" when not active)
    property string modal: ""
    property int modalAxis: 3
    property point hoverPos: Qt.point(width / 2, height / 2)
    // where the context menu was opened, for "Add here" and "Paste here"
    property var menuRay: null

    readonly property bool hasCourse: editor.courseTitle.length > 0
    readonly property bool hasSelection: (gizmoInfo.count || 0) > 0
    readonly property Camera activeCam: topView ? orthoCam : cam
    readonly property Camera overlayCam: topView ? oOrtho : ocam

    // CmnObj_ItemBox000_B_00 -> "ItemBox000 B" (as in the asset browser)
    function shortName(n) {
        let s = n.replace(/^(CmnObj|SystemObj)_/, "").replace(/_00$/, "");
        return s.replace("000_A", "").replace(/_/g, " ");
    }

    readonly property vector3d forward: Qt.vector3d(-Math.sin(yaw * Math.PI / 180) * Math.cos(pitch * Math.PI / 180), Math.sin(pitch * Math.PI / 180), -Math.cos(yaw * Math.PI / 180) * Math.cos(pitch * Math.PI / 180))
    readonly property vector3d flatForward: Qt.vector3d(-Math.sin(yaw * Math.PI / 180), 0, -Math.cos(yaw * Math.PI / 180))
    readonly property vector3d rightDir: Qt.vector3d(Math.cos(yaw * Math.PI / 180), 0, -Math.sin(yaw * Math.PI / 180))

    function refreshStages() {
        stagesModel = JSON.parse(editor.stages());
    }
    function refreshScene() {
        groupsModel = JSON.parse(editor.actorGroups());
        boxesModel = JSON.parse(editor.boxes());
        viewInfo = JSON.parse(editor.viewState());
        xray = viewInfo.xray !== false;
    }
    function refreshRails() {
        railCount = 0;
        railCount = editor.railCount();
    }
    function refreshOverlay() {
        linksData = editor.linksMesh();
        selectionData = editor.selectionMesh();
        gizmoInfo = JSON.parse(editor.gizmo());
    }

    Component.onCompleted: {
        const f = JSON.parse(editor.pref("fov"));
        if (f)
            fov = f;
        const s = JSON.parse(editor.pref("speedScale"));
        if (s)
            speedScale = s;
        const sh = JSON.parse(editor.pref("shading"));
        if (sh)
            shading = sh;
        const snap = JSON.parse(editor.pref("snap"));
        if (snap)
            editor.setSnap(snap.step || 0, snap.angle || 0);
    }

    Connections {
        target: vp.editor
        function onStagesRevisionChanged() { vp.refreshStages(); }
        function onSceneRevisionChanged() { vp.refreshScene(); }
        function onRailsRevisionChanged() { vp.refreshRails(); }
        function onOverlayRevisionChanged() { vp.refreshOverlay(); }
        function onRevisionChanged() { vp.viewInfo = JSON.parse(vp.editor.viewState()); }
        function onCourseTitleChanged() {
            vp.modal = "";
            Qt.callLater(vp.frameAll);
        }
        function onTransformsMoved(json) {
            const list = JSON.parse(json);
            for (let g = 0; g < groupRepeater.count; ++g) {
                const node = groupRepeater.objectAt(g);
                if (node)
                    node.moveInstances(list);
            }
            for (const t of list) {
                const b = boxIndex[t.h];
                if (b !== undefined) {
                    const item = boxRepeater.objectAt(b);
                    if (item)
                        item.place(t.bp, t.bq, t.bs);
                }
            }
        }
        function onRailsMoved(json) {
            const layer = vp.xray ? overlayRails : sceneRails;
            for (const i of JSON.parse(json))
                layer.refreshRail(i);
        }
    }
    property var boxIndex: {
        const m = {};
        for (let i = 0; i < boxesModel.length; ++i)
            m[boxesModel[i].h] = i;
        return m;
    }

    // ------------------------------------------------------------ camera helpers

    function rayAt(x, y) {
        const nx = x / view3d.width, ny = y / view3d.height;
        const c = vp.activeCam;
        const a = c.mapFromViewport(Qt.vector3d(nx, ny, 0));
        const b = c.mapFromViewport(Qt.vector3d(nx, ny, 100));
        const d = b.minus(a).normalized();
        return { o: a, d: d };
    }
    function lookAt(target, distance, dir) {
        if (vp.topView) {
            camPos = Qt.vector3d(target.x, camPos.y, target.z);
            topZoom = Math.max(0.005, Math.min(view3d.width, view3d.height) / Math.max(4, distance));
            return;
        }
        const d = dir ? dir.normalized() : vp.forward;
        camPos = target.minus(d.times(distance));
        yaw = Math.atan2(-d.x, -d.z) * 180 / Math.PI;
        pitch = Math.asin(Math.max(-1, Math.min(1, d.y))) * 180 / Math.PI;
        moveSpeed = Math.max(10, distance * 0.6);
    }
    // A camera move sometimes leaves a View3D on its previous frame (seen after focusing;
    // changing the scene in the same frame does not help). Changing an empty node in each view
    // shortly after the camera settles renders it again.
    property bool poke: false
    onCamPosChanged: settle.restart()
    onYawChanged: settle.restart()
    onPitchChanged: settle.restart()
    onTopZoomChanged: settle.restart()
    onTopViewChanged: settle.restart()
    Timer {
        id: settle
        interval: 150
        onTriggered: vp.poke = !vp.poke
    }

    function frameAll() {
        const t = JSON.parse(editor.focusTarget(true));
        if (!t)
            return;
        lookAt(Qt.vector3d(t.c[0], t.c[1], t.c[2]), t.r * (vp.topView ? 2.2 : 1.9), Qt.vector3d(-0.45, -0.62, -0.65));
    }
    function focusSelection() {
        const t = JSON.parse(editor.focusTarget(false));
        if (!t)
            return;
        lookAt(Qt.vector3d(t.c[0], t.c[1], t.c[2]), Math.max(12, t.r * 3.0), null);
    }
    // Top view on or off. On: straight down over what the camera looks at. Off: back to the
    // perspective camera, looking at where the top view was centred.
    function setTopView(on) {
        if (on === vp.topView)
            return;
        if (on) {
            perspectiveBefore = { pos: camPos, yaw: yaw, pitch: pitch };
            const g = groundAtScreen(view3d.width / 2, view3d.height / 2);
            const c = g ? g : camPos.plus(forward.times(Math.max(30, moveSpeed * 2)));
            const dist = camPos.minus(c).length();
            topView = true;
            camPos = Qt.vector3d(c.x, camPos.y, c.z);
            topZoom = Math.max(0.005, view3d.height / Math.max(20, dist * 1.1));
        } else {
            const span = view3d.height / topZoom;
            const g = groundAtScreen(view3d.width / 2, view3d.height / 2);
            const c = g ? g : Qt.vector3d(camPos.x, perspectiveBefore ? perspectiveBefore.pos.y - 50 : 0, camPos.z);
            topView = false;
            const p = perspectiveBefore;
            const d = p ? Qt.vector3d(-Math.sin(p.yaw * Math.PI / 180) * Math.cos(p.pitch * Math.PI / 180), Math.sin(p.pitch * Math.PI / 180), -Math.cos(p.yaw * Math.PI / 180) * Math.cos(p.pitch * Math.PI / 180)) : Qt.vector3d(-0.45, -0.62, -0.65);
            lookAt(c, span * 0.9, d);
        }
    }
    // the stage surface under a screen position (null when there is none)
    function groundAtScreen(x, y) {
        const r = rayAt(x, y);
        const s = editor.groundAt(r.o, r.d);
        if (s.length === 0)
            return null;
        const v = s.split(",").map(Number);
        return Qt.vector3d(v[0], v[1], v[2]);
    }
    function centerRay() {
        return rayAt(view3d.width / 2, view3d.height / 2);
    }
    function heading() {
        return Math.atan2(vp.flatForward.x, vp.flatForward.z);
    }
    function addAtCenter(name) {
        const r = centerRay();
        editor.addActor(name, r.o, r.d, heading());
        vp.forceActiveFocus();
    }
    function newPath(name, shape) {
        const r = centerRay();
        editor.newRail(name, shape || "line", r.o, r.d, heading());
        vp.forceActiveFocus();
    }
    function duplicate() {
        editor.duplicate(vp.rightDir.x, vp.rightDir.z);
    }
    function pasteAtCenter(inPlace) {
        const r = vp.menuRay ? vp.menuRay : centerRay();
        vp.menuRay = null;
        editor.paste(r.o, r.d, inPlace);
        vp.forceActiveFocus();
    }
    // Alt+Click: a path point after the selected one, a new path in a path editor, or the
    // current object
    function altClick(x, y) {
        const r = rayAt(x, y);
        const filter = vp.editor.pickFilter;
        if (vp.selection.kind === "point" && vp.editor.addPointAt(r.o, r.d))
            return;
        if (filter.length > 0 && filter !== "objects" && filter !== "paths") {
            vp.editor.newRail(filter, "point", r.o, r.d, heading());
            return;
        }
        vp.editor.addActor(vp.currentAsset, r.o, r.d, heading());
    }

    // a choice in one of the chips over the view
    function chooseOption(key, id) {
        switch (key) {
        case "View": setTopView(id === "top"); break;
        case "Camera": {
            const [what, v] = id.split(":");
            if (what === "fov") {
                fov = Number(v);
                editor.setPref("fov", v);
            } else {
                speedScale = Number(v);
                editor.setPref("speedScale", v);
            }
            break;
        }
        case "Shading": shading = id; editor.setPref("shading", JSON.stringify(id)); break;
        case "Editor": editor.setPickFilterFromQml(id); break;
        case "Gizmo": editor.setGizmo(editor.gizmoMode, id === "local"); break;
        case "Paths": editor.setViewOption("xray", id === "xray"); break;
        }
    }

    // ------------------------------------------------------------ modal move / rotate / scale

    function modalDefaultAxis(mode) {
        return mode === "rotate" ? 1 : 3;
    }
    function startModal(mode) {
        if (vp.modal.length > 0 || !vp.hasSelection)
            return false;
        const r = rayAt(hoverPos.x, hoverPos.y);
        const axis = modalDefaultAxis(mode);
        if (!editor.beginModal(mode, axis, r.o, r.d))
            return false;
        vp.modal = mode;
        vp.modalAxis = axis;
        gizmo.activeAxis = axis;
        return true;
    }
    function modalLock(axis) {
        const a = vp.modalAxis === axis ? modalDefaultAxis(vp.modal) : axis;
        const r = rayAt(hoverPos.x, hoverPos.y);
        if (editor.restartModal(a, r.o, r.d)) {
            vp.modalAxis = a;
            gizmo.activeAxis = a;
            editor.dragGizmo(r.o, r.d, false);
        }
    }
    function endModal(confirm) {
        if (vp.modal.length === 0)
            return;
        if (confirm)
            editor.endGizmo();
        else
            editor.cancelGizmo();
        vp.modal = "";
        gizmo.activeAxis = -1;
    }
    // E: a new point after the selected one, then move it with the mouse
    function extrude() {
        if (vp.selection.kind !== "point" || !editor.extrude())
            return;
        Qt.callLater(() => vp.startModal("translate"));
    }

    // ------------------------------------------------------------ toolbar

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        QQC2.ToolBar {
            Layout.fillWidth: true
            RowLayout {
                anchors.fill: parent
                spacing: 2

                component Sep: Kirigami.Separator {
                    Layout.fillHeight: true
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    Layout.bottomMargin: Kirigami.Units.smallSpacing
                    Layout.leftMargin: Kirigami.Units.smallSpacing
                    Layout.rightMargin: Kirigami.Units.smallSpacing
                }
                component TipButton: QQC2.ToolButton {
                    property string tip
                    QQC2.ToolTip.text: tip
                    QQC2.ToolTip.visible: hovered && tip.length > 0
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }

                TipButton {
                    icon.name: "view-visible"
                    text: "Show"
                    display: QQC2.AbstractButton.TextBesideIcon
                    enabled: vp.hasCourse
                    onClicked: viewMenu.popup(this, 0, height)
                    tip: "What the viewport shows: stage, models, path types, object categories"
                    ViewMenu {
                        id: viewMenu
                        editor: vp.editor
                    }
                }
                Sep {}
                TipButton {
                    icon.name: "edit-undo"
                    enabled: vp.editor.canUndo
                    onClicked: vp.editor.undo()
                    tip: "Undo " + vp.editor.undoLabel + " (Ctrl+Z)"
                }
                TipButton {
                    icon.name: "edit-redo"
                    enabled: vp.editor.canRedo
                    onClicked: vp.editor.redo()
                    tip: "Redo " + vp.editor.redoLabel + " (Ctrl+Shift+Z)"
                }
                Sep {}
                Repeater {
                    model: [
                        { mode: "translate", icon: "transform-move", tip: "Move (1). G moves with the mouse." },
                        { mode: "rotate", icon: "object-rotate-right", tip: "Rotate (2). R rotates with the mouse." },
                        { mode: "scale", icon: "transform-scale", tip: "Scale (3). T scales with the mouse." }
                    ]
                    delegate: TipButton {
                        required property var modelData
                        icon.name: modelData.icon
                        checkable: true
                        checked: vp.editor.gizmoMode === modelData.mode
                        onClicked: vp.editor.setGizmo(modelData.mode, vp.editor.gizmoLocal)
                        tip: modelData.tip
                    }
                }
                TipButton {
                    id: snapButton
                    readonly property real step: vp.viewInfo.snapStep || 0
                    readonly property real angle: vp.viewInfo.snapAngle || 0
                    icon.name: "snap-nodes-cusp"
                    text: step > 0 || angle > 0 ? (step > 0 ? step + " m" : "") + (step > 0 && angle > 0 ? " · " : "") + (angle > 0 ? angle + "°" : "") : "Snap"
                    display: QQC2.AbstractButton.TextBesideIcon
                    checkable: true
                    checked: step > 0 || angle > 0
                    enabled: vp.hasCourse
                    onClicked: {
                        checked = Qt.binding(() => step > 0 || angle > 0);
                        snapMenu.popup(this, 0, height);
                    }
                    tip: "Snap moves to a grid step and rotations to an angle step. Hold Ctrl while dragging to snap (or not) for one drag."
                    QQC2.Menu {
                        id: snapMenu
                        QQC2.MenuItem { text: "Move step"; enabled: false }
                        Repeater {
                            model: [0, 0.25, 0.5, 1, 2, 5, 10]
                            delegate: QQC2.MenuItem {
                                required property real modelData
                                text: modelData === 0 ? "Off" : modelData + " m"
                                checkable: true
                                checked: snapButton.step === modelData
                                onTriggered: vp.editor.setSnap(modelData, snapButton.angle)
                            }
                        }
                        QQC2.MenuSeparator {}
                        QQC2.MenuItem { text: "Rotation step"; enabled: false }
                        Repeater {
                            model: [0, 5, 15, 30, 45, 90]
                            delegate: QQC2.MenuItem {
                                required property real modelData
                                text: modelData === 0 ? "Off" : modelData + "°"
                                checkable: true
                                checked: snapButton.angle === modelData
                                onTriggered: vp.editor.setSnap(snapButton.step, modelData)
                            }
                        }
                    }
                }
                QQC2.CheckBox {
                    text: "Stick to Ground"
                    enabled: vp.hasCourse
                    checked: vp.viewInfo.snap === true
                    onToggled: vp.editor.setViewOption("snap", checked)
                    QQC2.ToolTip.text: "While moving, keep objects and points on the stage surface (Track Studio's Drop to Collision)"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                Sep {}
                TipButton {
                    icon.name: "list-add"
                    text: "Add " + vp.shortName(vp.currentAsset)
                    display: QQC2.AbstractButton.TextBesideIcon
                    enabled: vp.hasCourse
                    Layout.maximumWidth: Kirigami.Units.gridUnit * 10
                    onClicked: vp.addAtCenter(vp.currentAsset)
                    tip: "Add " + vp.currentAsset + " at the centre of the view (Q), or Alt+Click where it goes.\nPick another object in Assets, or drag one into the view."
                }
                TipButton {
                    icon.name: "draw-polyline"
                    enabled: vp.hasCourse
                    onClicked: vp.requestNewPath()
                    tip: "New path…"
                }
                Sep {}
                TipButton {
                    icon.name: "edit-copy"
                    enabled: vp.hasSelection
                    onClicked: vp.duplicate()
                    tip: "Duplicate (Ctrl+D)"
                }
                TipButton {
                    icon.name: "edit-delete"
                    enabled: vp.hasSelection
                    onClicked: vp.editor.deleteSelected()
                    tip: "Delete (Del)"
                }
                TipButton {
                    icon.name: "go-bottom"
                    enabled: vp.editor.groundReady && vp.hasSelection
                    onClicked: vp.editor.dropToGround()
                    tip: "Drop the selection onto the stage surface (End)"
                }
                Item { Layout.fillWidth: true }
                TipButton {
                    icon.name: "view-split-top-bottom"
                    checkable: true
                    checked: vp.topView
                    enabled: vp.hasCourse
                    onClicked: vp.setTopView(!vp.topView)
                    tip: "Top view, straight down without perspective (Ctrl+T)"
                }
                TipButton {
                    icon.name: "crosshairs"
                    enabled: vp.hasSelection
                    onClicked: vp.focusSelection()
                    tip: "Focus the selection (F)"
                }
                TipButton {
                    icon.name: "zoom-fit-best"
                    enabled: vp.hasCourse
                    onClicked: vp.frameAll()
                    tip: "Frame the whole course (Home)"
                }
            }
        }

        Item {
            id: area
            Layout.fillWidth: true
            Layout.fillHeight: true

            // ---------------------------------------------------- main scene
            View3D {
                id: view3d
                anchors.fill: parent
                camera: vp.activeCam
                environment: SceneEnvironment {
                    clearColor: "#6f8396"
                    backgroundMode: SceneEnvironment.Color
                    antialiasingMode: SceneEnvironment.MSAA
                    antialiasingQuality: SceneEnvironment.High
                    debugSettings: DebugSettings {
                        materialOverride: vp.shading === "unlit" ? DebugSettings.BaseColor : vp.shading === "normals" ? DebugSettings.Normals : DebugSettings.None
                        wireframeEnabled: vp.shading === "wireframe"
                    }
                }

                PerspectiveCamera {
                    id: cam
                    position: vp.camPos
                    rotation: Quaternion.fromEulerAngles(Qt.vector3d(vp.pitch, vp.yaw, 0))
                    fieldOfView: vp.fov
                    clipNear: 0.5
                    clipFar: 30000
                }
                OrthographicCamera {
                    id: orthoCam
                    position: Qt.vector3d(vp.camPos.x, vp.topHeight, vp.camPos.z)
                    rotation: Quaternion.fromEulerAngles(Qt.vector3d(-90, vp.yaw, 0))
                    horizontalMagnification: vp.topZoom
                    verticalMagnification: vp.topZoom
                    clipNear: 1
                    clipFar: vp.topHeight * 2
                }
                DirectionalLight {
                    eulerRotation: Qt.vector3d(-55, -35, 0)
                    brightness: 1.25
                    ambientColor: Qt.rgba(0.32, 0.34, 0.38, 1)
                }
                DirectionalLight {
                    eulerRotation: Qt.vector3d(25, 145, 0)
                    brightness: 0.35
                }

                // Re-render poke (see `settle`)
                Node {
                    position: Qt.vector3d(0, vp.poke ? 0.001 : 0, 0)
                }

                PrimitiveGeometry { id: boxGeo; shape: "box"; editor: vp.editor }
                PrimitiveGeometry { id: sphereGeo; shape: "sphere"; editor: vp.editor }
                PrimitiveGeometry { id: coneGeo; shape: "cone"; editor: vp.editor }
                PrimitiveGeometry { id: cylGeo; shape: "cylinder"; editor: vp.editor }

                Repeater3D {
                    model: vp.stagesModel
                    delegate: Node {
                        id: stage
                        required property var modelData
                        position: Qt.vector3d(modelData.p[0], modelData.p[1], modelData.p[2])
                        rotation: Qt.quaternion(modelData.q[0], modelData.q[1], modelData.q[2], modelData.q[3])
                        scale: Qt.vector3d(modelData.s[0], modelData.s[1], modelData.s[2])
                        Repeater3D {
                            model: stage.modelData.parts
                            delegate: MeshPart {
                                required property int index
                                editor: vp.editor
                                rel: stage.modelData.rel
                                part: index
                            }
                        }
                    }
                }

                Repeater3D {
                    id: groupRepeater
                    model: vp.groupsModel
                    delegate: ActorGroup {
                        editor: vp.editor
                    }
                }

                Repeater3D {
                    id: boxRepeater
                    model: vp.boxesModel
                    delegate: Model {
                        id: boxItem
                        required property var modelData
                        function place(p, q, s) {
                            position = Qt.vector3d(p[0], p[1], p[2]);
                            rotation = Qt.quaternion(q[0], q[1], q[2], q[3]);
                            scale = Qt.vector3d(s[0], s[1], s[2]);
                        }
                        Component.onCompleted: place(modelData.p, modelData.q, modelData.s)
                        geometry: modelData.shape === "sphere" ? sphereGeo : modelData.shape === "cone" ? coneGeo : modelData.shape === "cylinder" ? cylGeo : boxGeo
                        materials: PrincipledMaterial {
                            baseColor: Qt.rgba(boxItem.modelData.c[0], boxItem.modelData.c[1], boxItem.modelData.c[2], 0.5)
                            alphaMode: PrincipledMaterial.Blend
                            roughness: 0.9
                            cullMode: Material.NoCulling
                        }
                    }
                }

                RailLayer {
                    id: sceneRails
                    editor: vp.editor
                    count: vp.xray ? 0 : vp.railCount
                }
            }

            // ---------------------------------------------------- overlay: paths, links, selection, gizmo (always on top)
            View3D {
                id: overlay
                anchors.fill: parent
                camera: vp.overlayCam
                environment: SceneEnvironment {
                    backgroundMode: SceneEnvironment.Transparent
                    antialiasingMode: SceneEnvironment.MSAA
                }
                PerspectiveCamera {
                    id: ocam
                    position: cam.position
                    rotation: cam.rotation
                    fieldOfView: cam.fieldOfView
                    clipNear: cam.clipNear
                    clipFar: cam.clipFar
                }
                OrthographicCamera {
                    id: oOrtho
                    position: orthoCam.position
                    rotation: orthoCam.rotation
                    horizontalMagnification: orthoCam.horizontalMagnification
                    verticalMagnification: orthoCam.verticalMagnification
                    clipNear: orthoCam.clipNear
                    clipFar: orthoCam.clipFar
                }
                // Re-render poke (see `settle`): this view goes stale after a camera jump too
                Node {
                    position: Qt.vector3d(0, vp.poke ? 0.001 : 0, 0)
                }
                DirectionalLight {
                    eulerRotation: Qt.vector3d(-50, -30, 0)
                    brightness: 1.0
                    ambientColor: Qt.rgba(0.5, 0.5, 0.5, 1)
                }
                RailLayer {
                    id: overlayRails
                    editor: vp.editor
                    count: vp.xray ? vp.railCount : 0
                }
                LineModel { lines: vp.linksData }
                LineModel { lines: vp.selectionData }
                Gizmo {
                    id: gizmo
                    editor: vp.editor
                    info: vp.gizmoInfo
                    camera: vp.overlayCam
                    // in the top view the gizmo keeps its size on screen through the zoom
                    orthoScale: vp.topView ? 0.125 * view3d.height / vp.topZoom : 0
                    visible: info.visible === true && (vp.modal.length === 0 || vp.modal === vp.editor.gizmoMode)
                }
            }

            // ---------------------------------------------------- overlay labels (Track Studio style); the ones with a menu have an arrow
            Rectangle {
                // soft scrim so the labels stay calm over busy scenery
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                height: Kirigami.Units.gridUnit * 2.4
                visible: vp.hasCourse
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.rgba(0.05, 0.06, 0.08, 0.5) }
                    GradientStop { position: 1; color: "transparent" }
                }
            }
            Flow {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: Kirigami.Units.smallSpacing
                spacing: Kirigami.Units.smallSpacing
                visible: vp.hasCourse
                z: 2  // above the viewport's mouse area
                // options: a chip with an arrow opens a menu of choices
                Repeater {
                    model: [
                        { key: "View", value: vp.topView ? "Top" : "Perspective", tip: "Perspective camera, or straight down without perspective (Ctrl+T)",
                          options: [{ text: "Perspective", on: !vp.topView, id: "persp" }, { text: "Top", on: vp.topView, id: "top" }] },
                        { key: "Camera", value: vp.fov + "°  ×" + vp.speedScale, tip: "Field of view and fly speed (W A S D)",
                          options: [45, 55, 70, 90].map(f => ({ text: "Field of view " + f + "°", on: vp.fov === f, id: "fov:" + f }))
                              .concat([0.25, 0.5, 1, 2, 4].map(s => ({ text: "Speed ×" + s, on: vp.speedScale === s, id: "speed:" + s }))) },
                        { key: "Shading", value: ({ lit: "Lit", unlit: "Unlit", normals: "Normals", wireframe: "Wireframe" })[vp.shading], tip: "How the stage and models are drawn",
                          options: [["lit", "Lit"], ["unlit", "Unlit (texture colour)"], ["normals", "Normals"], ["wireframe", "Wireframe"]].map(o => ({ text: o[1], on: vp.shading === o[0], id: o[0] })) },
                        { key: "Editor", value: vp.editor.linkPicking ? "Pick link target" : vp.editor.pickFilter === "" ? "All" : vp.editor.pickFilter === "objects" ? "Map Objects" : vp.editor.pickFilter === "paths" ? "Paths" : vp.editor.pickFilter, alert: vp.editor.linkPicking,
                          tip: "What clicks and box selection pick (Track Studio's editor list). In a path editor, Alt+Click draws that path type.",
                          options: [{ text: "All", on: vp.editor.pickFilter === "", id: "" }, { text: "Map Objects", on: vp.editor.pickFilter === "objects", id: "objects" }, { text: "Paths", on: vp.editor.pickFilter === "paths", id: "paths" }]
                              .concat((vp.viewInfo.rails || []).map(r => ({ text: r.name, on: vp.editor.pickFilter === r.name, id: r.name }))) },
                        { key: "Gizmo", value: vp.editor.gizmoLocal ? "Local" : "World", tip: "Gizmo axes follow the object (Local) or the world (World)",
                          options: [{ text: "Local", on: vp.editor.gizmoLocal, id: "local" }, { text: "World", on: !vp.editor.gizmoLocal, id: "world" }] },
                        { key: "Paths", value: vp.xray ? "X-ray" : "Depth", tip: "Paths drawn on top of everything (X-ray) or hidden behind scenery (Depth)",
                          options: [{ text: "X-ray", on: vp.xray, id: "xray" }, { text: "Depth", on: !vp.xray, id: "depth" }] }
                    ]
                    delegate: Rectangle {
                        id: chip
                        required property var modelData
                        color: chipHover.hovered ? Qt.rgba(0.08, 0.09, 0.11, 0.9) : Qt.rgba(0.08, 0.09, 0.11, 0.7)
                        border.color: chip.modelData.alert ? "#ffc04d" : Qt.rgba(1, 1, 1, chipHover.hovered ? 0.5 : 0.25)
                        radius: 4
                        implicitWidth: chipRow.implicitWidth + 14
                        implicitHeight: chipRow.implicitHeight + 8
                        Row {
                            id: chipRow
                            anchors.centerIn: parent
                            spacing: 5
                            QQC2.Label {
                                text: chip.modelData.key
                                color: "#aab4be"
                                font.pointSize: Kirigami.Theme.smallFont.pointSize
                            }
                            QQC2.Label {
                                text: chip.modelData.value
                                color: chip.modelData.alert ? "#ffc04d" : "white"
                                font.pointSize: Kirigami.Theme.smallFont.pointSize
                                font.weight: Font.DemiBold
                            }
                            Kirigami.Icon {
                                anchors.verticalCenter: parent.verticalCenter
                                source: "arrow-down"
                                color: "#aab4be"
                                isMask: true
                                implicitWidth: 10
                                implicitHeight: 10
                            }
                        }
                        HoverHandler {
                            id: chipHover
                            cursorShape: Qt.PointingHandCursor
                        }
                        TapHandler {
                            onTapped: chipMenu.popup(chip, 0, chip.height + 2)
                        }
                        QQC2.ToolTip.text: chip.modelData.tip
                        QQC2.ToolTip.visible: chipHover.hovered
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        QQC2.Menu {
                            id: chipMenu
                            Instantiator {
                                model: chip.modelData.options
                                delegate: QQC2.MenuItem {
                                    required property var modelData
                                    text: modelData.text
                                    checkable: true
                                    checked: modelData.on
                                    onTriggered: vp.chooseOption(chip.modelData.key, modelData.id)
                                }
                                onObjectAdded: (i, o) => chipMenu.insertItem(i, o)
                                onObjectRemoved: (i, o) => chipMenu.removeItem(o)
                            }
                        }
                    }
                }
            }
            AxisIndicator {
                anchors.left: parent.left
                anchors.bottom: parent.bottom
                anchors.margins: Kirigami.Units.largeSpacing
                visible: vp.hasCourse
                camRotation: vp.activeCam.rotation
            }
            // what G, R and T are doing, and how to finish
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: Kirigami.Units.largeSpacing
                visible: vp.modal.length > 0
                z: 2
                color: Qt.rgba(0.08, 0.09, 0.11, 0.85)
                border.color: "#ffe14d"
                radius: 4
                implicitWidth: modalText.implicitWidth + 20
                implicitHeight: modalText.implicitHeight + 10
                QQC2.Label {
                    id: modalText
                    anchors.centerIn: parent
                    color: "white"
                    text: ({ translate: "Move", rotate: "Rotate", scale: "Scale" })[vp.modal] + " " + (vp.modalAxis === 3 ? (vp.modal === "scale" ? "uniformly" : vp.modal === "rotate" ? "around the view" : "in the view plane") : (vp.modal === "rotate" ? "around " : "along ") + ["X", "Y", "Z"][vp.modalAxis])
                        + "  ·  X Y Z: axis  ·  Ctrl: snap  ·  click: done  ·  Esc: cancel"
                }
            }
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.gridUnit * 8
                visible: vp.editor.courseTitle.length === 0
                icon.name: vp.editor.ready ? "document-open" : "view-refresh"
                text: vp.editor.ready ? "Open a course" : "Reading the game files…"
                explanation: vp.editor.ready ? "Pick a course from the game files or from one of your mods." : vp.editor.status
                helpfulAction: vp.editor.ready ? openCourseHelp : null
                Kirigami.Action {
                    id: openCourseHelp
                    text: "Open Course…"
                    icon.name: "document-open"
                    onTriggered: vp.openRequested()
                }
            }
            // box selection
            Rectangle {
                id: band
                visible: false
                z: 3
                color: Qt.rgba(0.31, 0.61, 0.94, 0.15)
                border.color: "#4f9cf0"
                border.width: 1
            }

            // assets dragged from the Assets panel land where they are dropped
            DropArea {
                anchors.fill: parent
                keys: ["text/x-trackstudioworld-asset"]
                onDropped: (drop) => {
                    const name = drop.getDataAsString("text/x-trackstudioworld-asset");
                    if (name.length === 0)
                        return;
                    const r = vp.rayAt(drop.x, drop.y);
                    vp.editor.addActor(name, r.o, r.d, vp.heading());
                    drop.accept(Qt.CopyAction);
                    vp.forceActiveFocus();
                }
            }

            // ---------------------------------------------------- input
            MouseArea {
                id: mouse
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                hoverEnabled: true
                property point last
                property point pressAt
                property bool dragging: false
                property bool moved: false
                property bool boxing: false
                property bool orbiting: false
                property vector3d pivot
                property bool handleDrag: false
                property double hoverTime: 0

                function ctrl(e) {
                    return (e.modifiers & Qt.ControlModifier) !== 0;
                }

                onPressed: (e) => {
                    vp.forceActiveFocus();
                    last = Qt.point(e.x, e.y);
                    pressAt = last;
                    moved = false;
                    boxing = false;
                    orbiting = false;
                    if (vp.modal.length > 0) {
                        vp.endModal(e.button === Qt.LeftButton);
                        dragging = false;
                        moved = true;  // the release must not also click
                        return;
                    }
                    if (e.button === Qt.LeftButton && (e.modifiers & Qt.AltModifier) && !vp.topView) {
                        // orbit around the selection, else the stage point at the centre
                        const t = vp.hasSelection ? JSON.parse(vp.editor.focusTarget(false)) : null;
                        const g = t ? Qt.vector3d(t.c[0], t.c[1], t.c[2]) : vp.groundAtScreen(view3d.width / 2, view3d.height / 2);
                        pivot = g ? g : vp.camPos.plus(vp.forward.times(Math.max(20, vp.moveSpeed)));
                        return;
                    }
                    if (e.button === Qt.LeftButton && !(e.modifiers & Qt.AltModifier)) {
                        const hit = overlay.pick(e.x, e.y);
                        if (hit.objectHit && hit.objectHit.gizmoAxis !== undefined) {
                            const r = vp.rayAt(e.x, e.y);
                            handleDrag = vp.gizmoInfo.handle === true;
                            dragging = vp.editor.beginGizmo(hit.objectHit.gizmoAxis, r.o, r.d, handleDrag && ctrl(e));
                            gizmo.activeAxis = hit.objectHit.gizmoAxis;
                        }
                    }
                }
                onPositionChanged: (e) => {
                    const dx = e.x - last.x, dy = e.y - last.y;
                    last = Qt.point(e.x, e.y);
                    vp.hoverPos = last;
                    if (Math.abs(e.x - pressAt.x) + Math.abs(e.y - pressAt.y) > 3)
                        moved = true;
                    if (vp.modal.length > 0) {
                        const r = vp.rayAt(e.x, e.y);
                        vp.editor.dragGizmo(r.o, r.d, ctrl(e));
                        return;
                    }
                    if (dragging) {
                        const r = vp.rayAt(e.x, e.y);
                        vp.editor.dragGizmo(r.o, r.d, !handleDrag && ctrl(e));
                    } else if (e.buttons & Qt.RightButton) {
                        vp.yaw -= dx * 0.25;
                        if (!vp.topView)
                            vp.pitch = Math.max(-89, Math.min(89, vp.pitch - dy * 0.25));
                    } else if (e.buttons & Qt.MiddleButton || (e.buttons & Qt.LeftButton && e.modifiers & Qt.ShiftModifier && !(e.modifiers & Qt.ControlModifier))) {
                        if (vp.topView) {
                            vp.camPos = vp.camPos.minus(vp.rightDir.times(dx / vp.topZoom)).plus(vp.flatForward.times(dy / vp.topZoom));
                        } else {
                            const k = vp.moveSpeed * 0.004;
                            vp.camPos = vp.camPos.minus(vp.rightDir.times(dx * k)).plus(cam.up.times(dy * k));
                        }
                    } else if (e.buttons & Qt.LeftButton && e.modifiers & Qt.AltModifier && !vp.topView) {
                        if (!moved)
                            return;
                        orbiting = true;
                        const dist = vp.camPos.minus(pivot).length();
                        vp.yaw -= dx * 0.3;
                        vp.pitch = Math.max(-89, Math.min(89, vp.pitch - dy * 0.3));
                        vp.camPos = pivot.minus(vp.forward.times(dist));
                    } else if (e.buttons & Qt.LeftButton) {
                        if (!moved || !vp.hasCourse)
                            return;
                        boxing = true;
                        band.x = Math.min(pressAt.x, e.x);
                        band.y = Math.min(pressAt.y, e.y);
                        band.width = Math.abs(e.x - pressAt.x);
                        band.height = Math.abs(e.y - pressAt.y);
                        band.visible = true;
                    } else if (e.buttons === Qt.NoButton) {
                        const now = Date.now();
                        if (now - hoverTime > 80 && vp.editor.groundReady) {
                            hoverTime = now;
                            const r = vp.rayAt(e.x, e.y);
                            vp.cursorChanged(vp.editor.groundAt(r.o, r.d));
                        }
                        const hit = overlay.pick(e.x, e.y);
                        gizmo.hoverAxis = hit.objectHit && hit.objectHit.gizmoAxis !== undefined ? hit.objectHit.gizmoAxis : -1;
                    }
                }
                onReleased: (e) => {
                    if (dragging) {
                        dragging = false;
                        gizmo.activeAxis = -1;
                        vp.editor.endGizmo();
                        return;
                    }
                    if (boxing) {
                        boxing = false;
                        band.visible = false;
                        const x0 = band.x, y0 = band.y, x1 = band.x + band.width, y1 = band.y + band.height;
                        const corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(c => {
                            const r = vp.rayAt(c[0], c[1]);
                            return [r.o.x, r.o.y, r.o.z, r.d.x, r.d.y, r.d.z];
                        });
                        const mode = ctrl(e) ? ((e.modifiers & Qt.ShiftModifier) ? 2 : 1) : 0;
                        vp.editor.boxSelect(JSON.stringify(corners), mode);
                        return;
                    }
                    if (moved || orbiting)
                        return;
                    const r = vp.rayAt(e.x, e.y);
                    if (e.button === Qt.RightButton) {
                        vp.editor.pickForMenu(r.o, r.d);
                        vp.menuRay = r;
                        contextMenu.popup();
                        return;
                    }
                    if (e.button !== Qt.LeftButton)
                        return;
                    if (e.modifiers & Qt.AltModifier)
                        vp.altClick(e.x, e.y);
                    else
                        vp.editor.pick(r.o, r.d, (e.modifiers & (Qt.ControlModifier | Qt.ShiftModifier)) !== 0);
                }
                // another item took the mouse (a popup opening): forget the gesture
                onCanceled: {
                    if (dragging) {
                        dragging = false;
                        gizmo.activeAxis = -1;
                        vp.editor.endGizmo();
                    }
                    boxing = false;
                    band.visible = false;
                    orbiting = false;
                }
                onDoubleClicked: (e) => {
                    if (e.button === Qt.LeftButton && vp.modal.length === 0 && !(e.modifiers & Qt.AltModifier))
                        vp.focusSelection();
                }
                onWheel: (e) => {
                    const notches = e.angleDelta.y / 120;
                    if (vp.topView) {
                        // zoom about the point under the mouse
                        const z0 = vp.topZoom;
                        const z1 = Math.max(0.005, Math.min(400, z0 * Math.pow(1.18, notches)));
                        const sx = e.x - view3d.width / 2, sy = e.y - view3d.height / 2;
                        const k = 1 / z0 - 1 / z1;
                        vp.camPos = vp.camPos.plus(vp.rightDir.times(sx * k)).minus(vp.flatForward.times(sy * k));
                        vp.topZoom = z1;
                        return;
                    }
                    const r = vp.rayAt(e.x, e.y);
                    const step = vp.moveSpeed * 0.25 * notches;
                    vp.camPos = vp.camPos.plus(r.d.times(step));
                }
            }

            QQC2.Menu {
                id: contextMenu
                readonly property string kind: vp.selection.kind
                readonly property bool any: ["actor", "point", "rail", "multi"].includes(kind)
                onClosed: Qt.callLater(() => vp.menuRay = null)
                QQC2.MenuItem { action: vp.actions.focus || null; visible: contextMenu.any; height: visible ? implicitHeight : 0 }
                QQC2.MenuSeparator { visible: contextMenu.any }
                QQC2.MenuItem { action: vp.actions.cut || null }
                QQC2.MenuItem { action: vp.actions.copy || null }
                QQC2.MenuItem {
                    text: "Paste Here"
                    icon.name: "edit-paste"
                    enabled: vp.editor.hasClipboard
                    onTriggered: vp.pasteAtCenter(false)
                }
                QQC2.MenuItem { action: vp.actions.duplicate || null }
                QQC2.MenuItem { action: vp.actions.del || null }
                QQC2.MenuSeparator {}
                QQC2.MenuItem {
                    text: "Add " + vp.shortName(vp.currentAsset) + " Here"
                    icon.name: "list-add"
                    enabled: vp.hasCourse
                    onTriggered: {
                        const r = vp.menuRay || vp.centerRay();
                        vp.editor.addActor(vp.currentAsset, r.o, r.d, vp.heading());
                    }
                }
                QQC2.MenuItem { action: vp.actions.drop || null }
                QQC2.MenuItem { action: vp.actions.hide || null }
                QQC2.MenuItem { action: vp.actions.unhide || null }
                QQC2.MenuSeparator { visible: contextMenu.kind === "actor" }
                QQC2.MenuItem { action: vp.actions.change || null; visible: contextMenu.kind === "actor"; height: visible ? implicitHeight : 0 }
                QQC2.MenuItem { action: vp.actions.link || null; visible: contextMenu.kind === "actor" || contextMenu.kind === "point"; height: visible ? implicitHeight : 0 }
                QQC2.Menu {
                    title: "Path"
                    enabled: contextMenu.kind === "point" || contextMenu.kind === "rail" || (contextMenu.kind === "multi" && (vp.selection.rails || 0) > 0)
                    QQC2.MenuItem { action: vp.actions.extrude || null }
                    QQC2.MenuItem { action: vp.actions.insert || null }
                    QQC2.MenuItem { action: vp.actions.connect || null }
                    QQC2.MenuItem { action: vp.actions.split || null }
                    QQC2.MenuItem { action: vp.actions.join || null }
                    QQC2.MenuSeparator {}
                    QQC2.MenuItem { action: vp.actions.orient || null }
                    QQC2.MenuItem { action: vp.actions.smooth || null }
                    QQC2.MenuItem { action: vp.actions.straighten || null }
                    QQC2.MenuItem { action: vp.actions.subdivide || null }
                    QQC2.MenuItem { action: vp.actions.reverse || null }
                    QQC2.MenuSeparator {}
                    QQC2.MenuItem { action: vp.actions.selectPath || null }
                    QQC2.MenuItem { action: vp.actions.placeAlong || null }
                    QQC2.MenuItem { action: vp.actions.changePathType || null }
                }
                QQC2.MenuSeparator {}
                QQC2.MenuItem { action: vp.actions.selectAll || null }
            }
        }
    }

    // ------------------------------------------------------------ keyboard
    // Window shortcuts on the menu actions (Main.qml) cover Del, Q, F, Home, End, I, L, C, H, O
    // and the Ctrl ones; these keys need the mouse position or only make sense in the view.

    Keys.onPressed: (e) => {
        const camKeys = [Qt.Key_W, Qt.Key_A, Qt.Key_S, Qt.Key_D, Qt.Key_Space];
        if (e.isAutoRepeat) {
            e.accepted = camKeys.includes(e.key);
            return;
        }
        const plain = (e.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) === 0;
        if (vp.modal.length > 0) {
            switch (e.key) {
            case Qt.Key_X: vp.modalLock(0); break;
            case Qt.Key_Y: vp.modalLock(1); break;
            case Qt.Key_Z: vp.modalLock(2); break;
            case Qt.Key_Escape: vp.endModal(false); break;
            case Qt.Key_Return: case Qt.Key_Enter: vp.endModal(true); break;
            default:
                if (!camKeys.includes(e.key))
                    return;
            }
            if (!camKeys.includes(e.key)) {
                e.accepted = true;
                return;
            }
        }
        const k = Object.assign({}, keys);
        switch (e.key) {
        case Qt.Key_W: case Qt.Key_A: case Qt.Key_S: case Qt.Key_D: case Qt.Key_Space:
            if (e.modifiers & Qt.ControlModifier && e.key !== Qt.Key_Space)
                return;
            k[e.key] = true;
            keys = k;
            e.accepted = true;
            return;
        case Qt.Key_1: if (!plain) return; vp.editor.setGizmo("translate", vp.editor.gizmoLocal); break;
        case Qt.Key_2: if (!plain) return; vp.editor.setGizmo("rotate", vp.editor.gizmoLocal); break;
        case Qt.Key_3: if (!plain) return; vp.editor.setGizmo("scale", vp.editor.gizmoLocal); break;
        case Qt.Key_G: if (!plain || !vp.startModal("translate")) return; break;
        case Qt.Key_R: if (!plain || !vp.startModal("rotate")) return; break;
        case Qt.Key_T: if (!plain || !vp.startModal("scale")) return; break;
        case Qt.Key_E: if (!plain) return; vp.extrude(); break;
        case Qt.Key_Escape:
            if (vp.editor.linkPicking) vp.editor.cancelLink(); else vp.editor.clearSelection();
            break;
        default: return;
        }
        e.accepted = true;
    }
    Keys.onReleased: (e) => {
        if (e.isAutoRepeat)
            return;
        if (keys[e.key]) {
            const k = Object.assign({}, keys);
            delete k[e.key];
            keys = k;
        }
    }
    onActiveFocusChanged: if (!activeFocus) keys = ({})

    FrameAnimation {
        running: Object.keys(vp.keys).length > 0
        onTriggered: {
            const k = vp.keys;
            const fast = Qt.application.keyboardModifiers & Qt.ShiftModifier ? 4 : 1;
            let m = Qt.vector3d(0, 0, 0);
            const fwd = vp.topView ? vp.flatForward : vp.forward;
            if (k[Qt.Key_W]) m = m.plus(fwd);
            if (k[Qt.Key_S]) m = m.minus(fwd);
            if (k[Qt.Key_D]) m = m.plus(vp.rightDir);
            if (k[Qt.Key_A]) m = m.minus(vp.rightDir);
            if (k[Qt.Key_Space] && !vp.topView) m = m.plus(Qt.vector3d(0, (Qt.application.keyboardModifiers & Qt.ControlModifier) ? -1 : 1, 0));
            if (m.length() === 0)
                return;
            const speed = vp.topView ? 0.6 * view3d.height / vp.topZoom : vp.moveSpeed;
            vp.camPos = vp.camPos.plus(m.normalized().times(speed * vp.speedScale * fast * frameTime));
            if (vp.modal.length > 0) {
                const r = vp.rayAt(vp.hoverPos.x, vp.hoverPos.y);
                vp.editor.dragGizmo(r.o, r.d, false);
            }
        }
    }
}
