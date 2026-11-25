pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import TrackStudioWorld

// Viewport visibility options (gear button).
QQC2.Menu {
    id: menu
    required property Editor editor
    property var st: ({ rails: [], categories: [] })
    onAboutToShow: st = JSON.parse(editor.viewState())

    QQC2.MenuItem { text: "Stage models"; checkable: true; checked: menu.st.stage === true; onTriggered: menu.editor.setViewOption("stage", checked) }
    QQC2.MenuItem { text: "Terrain"; checkable: true; checked: menu.st.terrain === true; onTriggered: menu.editor.setViewOption("terrain", checked) }
    QQC2.MenuItem { text: "High-quality terrain"; checkable: true; checked: menu.st.terrainHq === true; onTriggered: menu.editor.setViewOption("terrain_hq", checked) }
    QQC2.MenuItem { text: "Nearby Gridify pieces"; checkable: true; checked: menu.st.gridify === true; onTriggered: menu.editor.setViewOption("gridify", checked) }
    QQC2.MenuItem { text: "Surrounding terrain & pieces"; checkable: true; checked: menu.st.distantWorld === true; onTriggered: menu.editor.setViewOption("distant", checked) }
    QQC2.MenuSeparator {}
    QQC2.MenuItem { text: "Object models (off: boxes)"; checkable: true; checked: menu.st.models === true; onTriggered: menu.editor.setViewOption("models", checked) }
    QQC2.MenuItem { text: "Respawn points"; checkable: true; checked: menu.st.respawns !== false; onTriggered: menu.editor.setViewOption("respawns", checked) }
    QQC2.MenuItem { text: "Paths drawn on top"; checkable: true; checked: menu.st.xray === true; onTriggered: menu.editor.setViewOption("xray", checked) }
    QQC2.MenuItem { text: "Widths on every path"; checkable: true; checked: menu.st.widths === true; onTriggered: menu.editor.setViewOption("widths", checked) }
    QQC2.MenuSeparator {}
    QQC2.Menu {
        title: "Path types"
        Repeater {
            model: menu.st.rails
            delegate: QQC2.MenuItem {
                required property var modelData
                text: modelData.name
                checkable: true
                checked: modelData.visible
                onTriggered: menu.editor.setRailTypeVisible(modelData.name, checked)
            }
        }
    }
    QQC2.Menu {
        title: "Object categories"
        Repeater {
            model: menu.st.categories
            delegate: QQC2.MenuItem {
                required property var modelData
                text: modelData.name
                checkable: true
                checked: modelData.visible
                onTriggered: menu.editor.setCategoryVisible(modelData.name, checked)
            }
        }
    }
}
