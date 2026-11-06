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
    signal cursorChanged(string text)
    signal requestNewPath()

    // camera: position + yaw/pitch (degrees); Qt cameras look down -Z
    property vector3d camPos: Qt.vector3d(0, 400, 600)
    property real yaw: 0
    property real pitch: -35
    property real moveSpeed: 40
    property bool xray: true
    property var stagesModel: []
    property var groupsModel: []
    property var boxesModel: []
    property int railCount: 0
    property var linksData: ({})
    property var selectionData: ({})
    property var gizmoInfo: ({ visible: false })
    property var keys: ({})
    property real gizmoScale: 1

    readonly property bool hasCourse: editor.courseTitle.length > 0
    readonly property bool hasSelection: (gizmoInfo.count || 0) > 0

    // CmnObj_ItemBox000_B_00 -> "ItemBox000 B" (as in the asset browser)
    function shortName(n) {
        let s = n.replace(/^(CmnObj|SystemObj)_/, "").replace(/_00$/, "");
        return s.replace("000_A", "").replace(/_/g, " ");
    }

    readonly property vector3d forward: Qt.vector3d(-Math.sin(yaw * Math.PI / 180) * Math.cos(pitch * Math.PI / 180), Math.sin(pitch * Math.PI / 180), -Math.cos(yaw * Math.PI / 180) * Math.cos(pitch * Math.PI / 180))
    readonly property vector3d rightDir: Qt.vector3d(Math.cos(yaw * Math.PI / 180), 0, -Math.sin(yaw * Math.PI / 180))

    function refreshStages() {
        stagesModel = JSON.parse(editor.stages());
    }
    function refreshScene() {
        groupsModel = JSON.parse(editor.actorGroups());
        boxesModel = JSON.parse(editor.boxes());
        xray = JSON.parse(editor.viewState()).xray !== false;
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

    Connections {
        target: vp.editor
        function onStagesRevisionChanged() { vp.refreshStages(); }
        function onSceneRevisionChanged() { vp.refreshScene(); }
        function onRailsRevisionChanged() { vp.refreshRails(); }
        function onOverlayRevisionChanged() { vp.refreshOverlay(); }
        function onCourseTitleChanged() { Qt.callLater(vp.frameAll); }
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
        const a = cam.mapFromViewport(Qt.vector3d(nx, ny, 0));
        const b = cam.mapFromViewport(Qt.vector3d(nx, ny, 100));
        const d = b.minus(a).normalized();
        return { o: a, d: d };
    }
    function lookAt(target, distance, dir) {
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
    Timer {
        id: settle
        interval: 150
        onTriggered: vp.poke = !vp.poke
    }

    function frameAll() {
        const t = JSON.parse(editor.focusTarget(true));
        if (!t)
            return;
        lookAt(Qt.vector3d(t.c[0], t.c[1], t.c[2]), t.r * 1.9, Qt.vector3d(-0.45, -0.62, -0.65));
    }
    function focusSelection() {
        const t = JSON.parse(editor.focusTarget(false));
        if (!t)
            return;
        lookAt(Qt.vector3d(t.c[0], t.c[1], t.c[2]), Math.max(12, t.r * 3.0), null);
    }
    function centerRay() {
        return rayAt(view3d.width / 2, view3d.height / 2);
    }
    function addAtCenter(name) {
        const r = centerRay();
        editor.addActor(name, r.o, r.d, Math.atan2(vp.forward.x, vp.forward.z));
        vp.forceActiveFocus();
    }
    function newPath(name) {
        const r = centerRay();
        editor.newRail(name, r.o, r.d, Math.atan2(vp.forward.x, vp.forward.z));
    }
    function duplicate() {
        editor.duplicate(vp.rightDir.x, vp.rightDir.z);
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

                component Tip: QQC2.ToolTip {
                    delay: Kirigami.Units.toolTipDelay
                }
                component Sep: Kirigami.Separator {
                    Layout.fillHeight: true
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    Layout.bottomMargin: Kirigami.Units.smallSpacing
                    Layout.leftMargin: Kirigami.Units.smallSpacing
                    Layout.rightMargin: Kirigami.Units.smallSpacing
                }

                QQC2.ToolButton {
                    icon.name: "view-visible"
                    text: "Show"
                    display: QQC2.AbstractButton.TextBesideIcon
                    enabled: vp.hasCourse
                    onClicked: viewMenu.popup(this, 0, height)
                    QQC2.ToolTip.text: "What the viewport shows: stage, models, path types, object categories"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    ViewMenu {
                        id: viewMenu
                        editor: vp.editor
                    }
                }
                Sep {}
                QQC2.ToolButton {
                    icon.name: "edit-undo"
                    enabled: vp.editor.canUndo
                    onClicked: vp.editor.undo()
                    QQC2.ToolTip.text: "Undo (Ctrl+Z)"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                QQC2.ToolButton {
                    icon.name: "edit-redo"
                    enabled: vp.editor.canRedo
                    onClicked: vp.editor.redo()
                    QQC2.ToolTip.text: "Redo (Ctrl+Shift+Z)"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                Sep {}
                Repeater {
                    model: [
                        { mode: "translate", icon: "transform-move", tip: "Move (1)" },
                        { mode: "rotate", icon: "object-rotate-right", tip: "Rotate (2)" },
                        { mode: "scale", icon: "transform-scale", tip: "Scale (3)" }
                    ]
                    delegate: QQC2.ToolButton {
                        required property var modelData
                        icon.name: modelData.icon
                        checkable: true
                        checked: vp.editor.gizmoMode === modelData.mode
                        onClicked: vp.editor.setGizmo(modelData.mode, vp.editor.gizmoLocal)
                        QQC2.ToolTip.text: modelData.tip
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                }
                Sep {}
                QQC2.ToolButton {
                    icon.name: "list-add"
                    text: vp.shortName(vp.currentAsset)
                    display: QQC2.AbstractButton.TextBesideIcon
                    enabled: vp.hasCourse
                    Layout.maximumWidth: Kirigami.Units.gridUnit * 10
                    onClicked: vp.addAtCenter(vp.currentAsset)
                    QQC2.ToolTip.text: "Add " + vp.currentAsset + " at the centre of the view (Q).\nPick another object in Assets."
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                QQC2.ToolButton {
                    icon.name: "draw-polyline"
                    enabled: vp.hasCourse
                    onClicked: vp.requestNewPath()
                    QQC2.ToolTip.text: "New path…"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                Sep {}
                QQC2.ToolButton {
                    icon.name: "edit-copy"
                    enabled: vp.hasSelection
                    onClicked: vp.duplicate()
                    QQC2.ToolTip.text: "Duplicate (Ctrl+D)"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                QQC2.ToolButton {
                    icon.name: "edit-delete"
                    enabled: vp.hasSelection
                    onClicked: vp.editor.deleteSelected()
                    QQC2.ToolTip.text: "Delete (Del)"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                QQC2.ToolButton {
                    icon.name: "go-bottom"
                    enabled: vp.editor.groundReady && vp.hasSelection
                    onClicked: vp.editor.dropToGround()
                    QQC2.ToolTip.text: "Drop the selection onto the stage surface (G)"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                Sep {}
                QQC2.CheckBox {
                    text: "Snap to Ground"
                    enabled: vp.hasCourse
                    checked: JSON.parse(vp.editor.viewState()).snap === true
                    onToggled: vp.editor.setViewOption("snap", checked)
                    QQC2.ToolTip.text: "While moving, keep objects and points on the stage surface (Track Studio's Drop to Collision)"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                Item { Layout.fillWidth: true }
                QQC2.ToolButton {
                    icon.name: "crosshairs"
                    enabled: vp.hasSelection
                    onClicked: vp.focusSelection()
                    QQC2.ToolTip.text: "Focus the selection (F)"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                QQC2.ToolButton {
                    icon.name: "zoom-fit-best"
                    enabled: vp.hasCourse
                    onClicked: vp.frameAll()
                    QQC2.ToolTip.text: "Frame the whole course (Home)"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
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
                camera: cam
                environment: SceneEnvironment {
                    clearColor: "#6f8396"
                    backgroundMode: SceneEnvironment.Color
                    antialiasingMode: SceneEnvironment.MSAA
                    antialiasingQuality: SceneEnvironment.High
                }

                PerspectiveCamera {
                    id: cam
                    position: vp.camPos
                    rotation: Quaternion.fromEulerAngles(Qt.vector3d(vp.pitch, vp.yaw, 0))
                    fieldOfView: 55
                    clipNear: 0.5
                    clipFar: 30000
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
                camera: ocam
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
                    camera: ocam
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
            Row {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.margins: Kirigami.Units.smallSpacing
                spacing: Kirigami.Units.smallSpacing
                visible: vp.hasCourse
                z: 2  // above the viewport's mouse area
                // read-only state: plain text with an outline, readable over any scenery
                Repeater {
                    model: [
                        { key: "View", value: "Perspective" },
                        { key: "Camera", value: "Fly" },
                        { key: "Editor", value: vp.editor.linkPicking ? "Pick link target" : "Map Objects", alert: vp.editor.linkPicking }
                    ]
                    delegate: Row {
                        id: info
                        required property var modelData
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 4
                        leftPadding: Kirigami.Units.smallSpacing
                        rightPadding: Kirigami.Units.largeSpacing
                        QQC2.Label {
                            text: info.modelData.key
                            color: "#c8d0d8"
                            style: Text.Outline
                            styleColor: "#c0101216"
                            font.pointSize: Kirigami.Theme.smallFont.pointSize
                        }
                        QQC2.Label {
                            text: info.modelData.value
                            color: info.modelData.alert ? "#ffc04d" : "white"
                            style: Text.Outline
                            styleColor: "#c0101216"
                            font.pointSize: Kirigami.Theme.smallFont.pointSize
                            font.weight: Font.DemiBold
                        }
                    }
                }
                // options: a chip with an arrow opens a menu
                Repeater {
                    model: [
                        { key: "Gizmo", value: vp.editor.gizmoLocal ? "Local" : "World", options: ["Local", "World"] },
                        { key: "Paths", value: vp.xray ? "X-ray" : "Depth", options: ["X-ray", "Depth"] }
                    ]
                    delegate: Rectangle {
                        id: chip
                        required property var modelData
                        color: chipHover.hovered ? Qt.rgba(0.08, 0.09, 0.11, 0.9) : Qt.rgba(0.08, 0.09, 0.11, 0.7)
                        border.color: Qt.rgba(1, 1, 1, chipHover.hovered ? 0.5 : 0.25)
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
                                color: "white"
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
                        QQC2.ToolTip.text: chip.modelData.key === "Gizmo" ? "Gizmo axes follow the object (Local) or the world (World)" : "Paths drawn on top of everything (X-ray) or hidden behind scenery (Depth)"
                        QQC2.ToolTip.visible: chipHover.hovered
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        QQC2.Menu {
                            id: chipMenu
                            Repeater {
                                model: chip.modelData.options
                                delegate: QQC2.MenuItem {
                                    required property string modelData
                                    text: modelData
                                    checkable: true
                                    checked: modelData === chip.modelData.value
                                    onTriggered: {
                                        if (chip.modelData.key === "Gizmo")
                                            vp.editor.setGizmo(vp.editor.gizmoMode, modelData === "Local");
                                        else
                                            vp.editor.setViewOption("xray", modelData === "X-ray");
                                    }
                                }
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
                camRotation: cam.rotation
            }
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.gridUnit * 8
                visible: vp.editor.courseTitle.length === 0
                icon.name: vp.editor.ready ? "document-open" : "view-refresh"
                text: vp.editor.ready ? "Open a course" : "Reading the game files…"
                explanation: vp.editor.ready ? "File → Open Course (Ctrl+O)" : vp.editor.status
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
                property double hoverTime: 0

                onPressed: (e) => {
                    vp.forceActiveFocus();
                    last = Qt.point(e.x, e.y);
                    pressAt = last;
                    moved = false;
                    if (e.button === Qt.LeftButton && !(e.modifiers & Qt.AltModifier)) {
                        const hit = overlay.pick(e.x, e.y);
                        if (hit.objectHit && hit.objectHit.gizmoAxis !== undefined) {
                            const r = vp.rayAt(e.x, e.y);
                            dragging = vp.editor.beginGizmo(hit.objectHit.gizmoAxis, r.o, r.d);
                            gizmo.activeAxis = hit.objectHit.gizmoAxis;
                        }
                    }
                }
                onPositionChanged: (e) => {
                    const dx = e.x - last.x, dy = e.y - last.y;
                    last = Qt.point(e.x, e.y);
                    if (Math.abs(e.x - pressAt.x) + Math.abs(e.y - pressAt.y) > 3)
                        moved = true;
                    if (dragging) {
                        const r = vp.rayAt(e.x, e.y);
                        vp.editor.dragGizmo(r.o, r.d);
                    } else if (e.buttons & Qt.RightButton) {
                        vp.yaw -= dx * 0.25;
                        vp.pitch = Math.max(-89, Math.min(89, vp.pitch - dy * 0.25));
                    } else if (e.buttons & Qt.MiddleButton || (e.buttons & Qt.LeftButton && e.modifiers & Qt.ShiftModifier)) {
                        const k = vp.moveSpeed * 0.004;
                        const up = cam.up;
                        vp.camPos = vp.camPos.minus(vp.rightDir.times(dx * k)).plus(up.times(dy * k));
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
                    if (e.button !== Qt.LeftButton || moved)
                        return;
                    const r = vp.rayAt(e.x, e.y);
                    if (e.modifiers & Qt.AltModifier)
                        vp.editor.addActor(vp.currentAsset, r.o, r.d, Math.atan2(vp.forward.x, vp.forward.z));
                    else
                        vp.editor.pick(r.o, r.d, (e.modifiers & (Qt.ControlModifier | Qt.ShiftModifier)) !== 0);
                }
                onDoubleClicked: (e) => {
                    if (e.button === Qt.LeftButton)
                        vp.focusSelection();
                }
                onWheel: (e) => {
                    const r = vp.rayAt(e.x, e.y);
                    const step = vp.moveSpeed * 0.25 * (e.angleDelta.y / 120);
                    vp.camPos = vp.camPos.plus(r.d.times(step));
                }
            }
        }
    }

    // ------------------------------------------------------------ keyboard

    Keys.onPressed: (e) => {
        if (e.isAutoRepeat) {
            e.accepted = [Qt.Key_W, Qt.Key_A, Qt.Key_S, Qt.Key_D, Qt.Key_Space, Qt.Key_E].includes(e.key);
            return;
        }
        const k = Object.assign({}, keys);
        switch (e.key) {
        case Qt.Key_W: case Qt.Key_A: case Qt.Key_S: case Qt.Key_D: case Qt.Key_Space: case Qt.Key_E:
            if (e.modifiers & Qt.ControlModifier && e.key !== Qt.Key_Space)
                return;
            k[e.key] = true;
            keys = k;
            e.accepted = true;
            return;
        // Del, Q, F, Home, G, I and L are window shortcuts on the menu actions (Main.qml)
        case Qt.Key_1: vp.editor.setGizmo("translate", vp.editor.gizmoLocal); break;
        case Qt.Key_2: vp.editor.setGizmo("rotate", vp.editor.gizmoLocal); break;
        case Qt.Key_3: vp.editor.setGizmo("scale", vp.editor.gizmoLocal); break;
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
            const step = vp.moveSpeed * fast * frameTime;
            let m = Qt.vector3d(0, 0, 0);
            if (k[Qt.Key_W]) m = m.plus(vp.forward);
            if (k[Qt.Key_S]) m = m.minus(vp.forward);
            if (k[Qt.Key_D]) m = m.plus(vp.rightDir);
            if (k[Qt.Key_A]) m = m.minus(vp.rightDir);
            if (k[Qt.Key_Space]) m = m.plus(Qt.vector3d(0, (Qt.application.keyboardModifiers & Qt.ControlModifier) ? -1 : 1, 0));
            if (k[Qt.Key_E]) m = m.plus(Qt.vector3d(0, 1, 0));
            if (m.length() > 0)
                vp.camPos = vp.camPos.plus(m.normalized().times(step));
        }
    }
}
