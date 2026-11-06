pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import org.mkw.trackeditor

// Viewport visibility options (gear button).
QQC2.Menu {
    id: menu
    required property Editor editor
    property var st: ({ rails: [], categories: [] })
    onAboutToShow: st = JSON.parse(editor.viewState())

    QQC2.MenuItem { text: "Stage models"; checkable: true; checked: menu.st.stage === true; onTriggered: menu.editor.setViewOption("stage", checked) }
    QQC2.MenuItem { text: "Object models (off: boxes)"; checkable: true; checked: menu.st.models === true; onTriggered: menu.editor.setViewOption("models", checked) }
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
