pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.mkw.trackeditor

Kirigami.ApplicationWindow {
    id: root

    width: 1680
    height: 1000
    minimumWidth: 1180
    minimumHeight: 720
    title: (editor.dirty ? "● " : "") + (editor.courseTitle.length > 0 ? editor.courseTitle + " — " : "") + "MKW Track Editor"
    pageStack.globalToolBar.style: Kirigami.ApplicationHeaderStyle.None

    property string currentAsset: "CmnObj_ItemBox000_A_00"
    property string cursorText: ""

    Editor {
        id: editor
        Component.onCompleted: start()
        onReadyChanged: {
            if (!ready)
                return;
            const first = startupCourse();
            if (first.length > 0)
                openCourse(first, "");
            else
                openDialog.open();
        }
    }

    // ------------------------------------------------------------ actions

    Kirigami.Action {
        id: openAction
        text: "Open Course…"
        icon.name: "document-open"
        shortcut: "Ctrl+O"
        enabled: editor.ready
        onTriggered: openDialog.open()
    }
    Kirigami.Action {
        id: saveAction
        text: "Save Mod"
        icon.name: "document-save"
        shortcut: "Ctrl+S"
        enabled: editor.courseTitle.length > 0
        onTriggered: {
            saveDialog.saveResult = JSON.parse(editor.save());
            saveDialog.open();
        }
    }
    Kirigami.Action {
        id: undoAction
        text: "Undo"
        icon.name: "edit-undo"
        shortcut: StandardKey.Undo
        enabled: editor.canUndo
        onTriggered: editor.undo()
    }
    Kirigami.Action {
        id: redoAction
        text: "Redo"
        icon.name: "edit-redo"
        shortcut: "Ctrl+Shift+Z"
        enabled: editor.canRedo
        onTriggered: editor.redo()
    }
    // what is selected, as the Properties panel shows it
    readonly property var sel: props.s
    readonly property bool hasSel: ["actor", "point", "rail", "multi"].includes(sel.kind)

    Kirigami.Action {
        id: duplicateAction
        text: "Duplicate"
        icon.name: "edit-copy"
        shortcut: "Ctrl+D"
        enabled: root.hasSel
        onTriggered: viewport.duplicate()
    }
    Kirigami.Action {
        id: deleteAction
        text: "Delete"
        icon.name: "edit-delete"
        shortcut: "Del"
        enabled: root.hasSel
        onTriggered: editor.deleteSelected()
    }
    Kirigami.Action {
        id: dropAction
        text: "Drop to Ground"
        icon.name: "go-bottom"
        shortcut: "G"
        enabled: editor.groundReady && root.hasSel
        onTriggered: editor.dropToGround()
    }
    Kirigami.Action {
        id: addAction
        text: "Add " + viewport.shortName(root.currentAsset)
        icon.name: "list-add"
        shortcut: "Q"
        enabled: editor.courseTitle.length > 0
        onTriggered: viewport.addAtCenter(root.currentAsset)
    }
    Kirigami.Action {
        id: newPathAction
        text: "New Path…"
        icon.name: "draw-polyline"
        enabled: editor.courseTitle.length > 0
        onTriggered: newPathDialog.open()
    }
    Kirigami.Action {
        id: insertPointAction
        text: "Insert Point After"
        icon.name: "node-add"
        shortcut: "I"
        enabled: root.sel.kind === "point"
        onTriggered: editor.insertPoint()
    }
    Kirigami.Action {
        id: curveAction
        text: root.sel.control0 ? "Remove Curve Handles" : "Add Curve Handles"
        icon.name: "draw-bezier-curves"
        enabled: root.sel.kind === "point"
        onTriggered: editor.toggleCurve()
    }
    Kirigami.Action {
        id: linkAction
        text: "Link To…"
        icon.name: "link"
        shortcut: "L"
        enabled: root.sel.kind === "actor" || root.sel.kind === "point"
        onTriggered: editor.startLink("BasicSignal")
    }
    Kirigami.Action {
        id: reverseAction
        text: "Reverse Path"
        icon.name: "media-playlist-repeat"
        enabled: root.sel.kind === "point" || root.sel.kind === "rail"
        onTriggered: editor.reverseRail()
    }
    Kirigami.Action {
        id: focusAction
        text: "Focus Selection"
        icon.name: "crosshairs"
        shortcut: "F"
        enabled: root.hasSel
        onTriggered: viewport.focusSelection()
    }
    Kirigami.Action {
        id: frameAction
        text: "Frame Course"
        icon.name: "zoom-fit-best"
        shortcut: "Home"
        enabled: editor.courseTitle.length > 0
        onTriggered: viewport.frameAll()
    }
    Kirigami.Action {
        id: settingsAction
        text: "Settings…"
        icon.name: "configure"
        onTriggered: settingsDialog.open()
    }

    Shortcut {
        sequence: "Ctrl+Y"
        onActivated: editor.redo()
    }

    // ------------------------------------------------------------ menus

    menuBar: QQC2.MenuBar {
        QQC2.Menu {
            title: "&File"
            QQC2.MenuItem { action: openAction }
            QQC2.MenuItem { action: saveAction }
            QQC2.MenuSeparator {}
            QQC2.MenuItem {
                text: "Open Mod Folder"
                icon.name: "folder-open"
                onTriggered: editor.openFolder("mods")
            }
            QQC2.MenuItem {
                text: "Open Emulator Mod Folder"
                icon.name: "folder-games"
                onTriggered: editor.openFolder("emulator")
            }
            QQC2.MenuItem {
                text: "Export for Switch 2 (ns2jb-modloader)"
                icon.name: "media-flash-sd-mmc"
                enabled: editor.courseTitle.length > 0 && editor.ns2jbPath.length > 0
                onTriggered: {
                    const r = JSON.parse(editor.exportNs2jb());
                    exportDialog.title = r.ok ? "Exported for ns2jb-modloader" : "Export failed";
                    exportDialog.subtitle = r.ok
                        ? r.files.length + " file(s) copied to " + r.dir + "." + (editor.dirty ? "\n\nUnsaved changes are not included: save first, then export again." : "") + "\n\nOn the PC, run node load.cjs from ns2jb-modloader's ModLoader folder, wait for READY, then open the game."
                        : r.errors.join("\n");
                    exportDialog.open();
                }
            }
            QQC2.MenuSeparator {}
            QQC2.MenuItem { action: settingsAction }
            QQC2.MenuItem {
                text: "Quit"
                icon.name: "application-exit"
                onTriggered: Qt.quit()
            }
        }
        QQC2.Menu {
            title: "&Edit"
            QQC2.MenuItem { action: undoAction }
            QQC2.MenuItem { action: redoAction }
            QQC2.MenuSeparator {}
            QQC2.MenuItem { action: addAction }
            QQC2.MenuItem { action: duplicateAction }
            QQC2.MenuItem { action: deleteAction }
            QQC2.MenuItem { action: dropAction }
            QQC2.MenuSeparator {}
            QQC2.MenuItem {
                text: "Select None"
                icon.name: "edit-select-none"
                enabled: root.hasSel
                onTriggered: editor.clearSelection()
            }
        }
        QQC2.Menu {
            title: "&Paths"
            QQC2.MenuItem { action: newPathAction }
            QQC2.MenuSeparator {}
            QQC2.MenuItem { action: insertPointAction }
            QQC2.MenuItem { action: curveAction }
            QQC2.MenuItem { action: reverseAction }
            QQC2.MenuSeparator {}
            QQC2.MenuItem { action: linkAction }
        }
        QQC2.Menu {
            title: "&View"
            QQC2.MenuItem { action: focusAction }
            QQC2.MenuItem { action: frameAction }
            QQC2.MenuSeparator {}
            QQC2.MenuItem {
                text: "Build Asset Thumbnails"
                icon.name: "view-preview"
                onTriggered: editor.buildThumbnails()
            }
        }
        QQC2.Menu {
            title: "&Help"
            QQC2.MenuItem {
                text: "Controls"
                icon.name: "help-keybord-shortcuts"
                onTriggered: helpDialog.open()
            }
            QQC2.MenuItem {
                text: "About MKW Track Editor"
                icon.name: "help-about"
                onTriggered: aboutDialog.open()
            }
        }
    }

    // ------------------------------------------------------------ layout

    pageStack.initialPage: Kirigami.Page {
        padding: 0
        globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

        QQC2.SplitView {
            anchors.fill: parent
            orientation: Qt.Horizontal

            QQC2.SplitView {
                orientation: Qt.Vertical
                QQC2.SplitView.preferredWidth: Kirigami.Units.gridUnit * 18.5
                QQC2.SplitView.minimumWidth: Kirigami.Units.gridUnit * 13

                Panel {
                    title: "Outliner"
                    iconName: "view-list-tree"
                    QQC2.SplitView.fillHeight: true
                    Outliner {
                        anchors.fill: parent
                        editor: editor
                        onOpenRequested: openDialog.open()
                    }
                }
                Panel {
                    title: "Help"
                    iconName: "help-contents"
                    QQC2.SplitView.preferredHeight: Kirigami.Units.gridUnit * 14.5
                    HelpPanel {
                        anchors.fill: parent
                    }
                }
            }

            QQC2.SplitView {
                orientation: Qt.Vertical
                QQC2.SplitView.fillWidth: true

                Panel {
                    title: "Viewport"
                    iconName: "globe"
                    QQC2.SplitView.fillHeight: true
                    Viewport {
                        id: viewport
                        anchors.fill: parent
                        editor: editor
                        currentAsset: root.currentAsset
                        onCursorChanged: (text) => root.cursorText = text
                        onRequestNewPath: newPathDialog.open()
                    }
                }
                Panel {
                    id: bottomPanel
                    QQC2.SplitView.preferredHeight: Kirigami.Units.gridUnit * 14
                    tabs: ["Assets", "Console"]
                    iconName: "folder-images"
                    StackLayout {
                        anchors.fill: parent
                        currentIndex: bottomPanel.currentTab
                        AssetBrowser {
                            editor: editor
                            currentAsset: root.currentAsset
                            onAssetChosen: (name) => root.currentAsset = name
                            onAssetActivated: (name) => {
                                root.currentAsset = name;
                                viewport.addAtCenter(name);
                            }
                            onChangeRequested: (name) => editor.changeObject(name)
                        }
                        ConsolePanel {
                            editor: editor
                        }
                    }
                }
            }

            Panel {
                title: "Properties"
                iconName: "document-properties"
                QQC2.SplitView.preferredWidth: Kirigami.Units.gridUnit * 24.5
                QQC2.SplitView.minimumWidth: Kirigami.Units.gridUnit * 18
                PropertiesPanel {
                    id: props
                    anchors.fill: parent
                    editor: editor
                    onFocusRequested: viewport.focusSelection()
                    onChangeObjectRequested: changeDialog.open()
                    onDuplicateRequested: viewport.duplicate()
                }
            }
        }
    }

    footer: QQC2.ToolBar {
        position: QQC2.ToolBar.Footer
        RowLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.smallSpacing
            Kirigami.Icon {
                source: editor.linkPicking ? "link" : (editor.busy ? "view-refresh" : "dialog-information")
                implicitWidth: Kirigami.Units.iconSizes.small
                implicitHeight: Kirigami.Units.iconSizes.small
                color: editor.linkPicking ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.textColor
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: editor.status
                elide: Text.ElideRight
                color: editor.linkPicking ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.textColor
                QQC2.ToolTip.text: editor.status
                QQC2.ToolTip.visible: statusHover.hovered && truncated
                HoverHandler { id: statusHover }
            }
            QQC2.ProgressBar {
                visible: editor.modelsWanted > 0 && editor.modelsLoaded < editor.modelsWanted
                from: 0
                to: Math.max(1, editor.modelsWanted)
                value: editor.modelsLoaded
                Layout.preferredWidth: Kirigami.Units.gridUnit * 8
            }
            QQC2.Label {
                visible: editor.modelsWanted > 0 && editor.modelsLoaded < editor.modelsWanted
                text: "models " + editor.modelsLoaded + "/" + editor.modelsWanted
                color: Kirigami.Theme.disabledTextColor
            }
            // ground position under the mouse
            QQC2.ToolSeparator {}
            QQC2.Label {
                text: "Cursor"
                color: Kirigami.Theme.disabledTextColor
            }
            QQC2.Label {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 10
                text: root.cursorText.length > 0 ? root.cursorText : "–"
                font.features: { "tnum": 1 }
                elide: Text.ElideLeft
                QQC2.ToolTip.text: "Stage surface under the mouse (X, Y, Z)"
                QQC2.ToolTip.visible: cursorHover.hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                HoverHandler { id: cursorHover }
            }
            QQC2.ToolSeparator {}
            QQC2.Label {
                text: "Mod"
                color: Kirigami.Theme.disabledTextColor
            }
            QQC2.TextField {
                text: editor.modName
                Layout.preferredWidth: Kirigami.Units.gridUnit * 9
                onEditingFinished: editor.modName = text
                QQC2.ToolTip.text: "Mods are saved to <mods folder>/<this name>/romfs"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
            QQC2.ToolButton {
                action: saveAction
                display: QQC2.AbstractButton.TextBesideIcon
                highlighted: editor.dirty
            }
        }
    }

    // ------------------------------------------------------------ dialogs

    OpenCourseDialog {
        id: openDialog
        editor: editor
    }
    SaveDialog {
        id: saveDialog
        editor: editor
    }
    SettingsDialog {
        id: settingsDialog
        editor: editor
    }
    NewPathDialog {
        id: newPathDialog
        editor: editor
        onChosen: (name) => viewport.newPath(name)
    }
    ChangeObjectDialog {
        id: changeDialog
        editor: editor
    }
    Kirigami.Dialog {
        id: helpDialog
        title: "Controls"
        preferredWidth: Kirigami.Units.gridUnit * 30
        padding: Kirigami.Units.largeSpacing
        HelpPanel {
            implicitHeight: Kirigami.Units.gridUnit * 24
        }
    }
    Kirigami.PromptDialog {
        id: exportDialog
        standardButtons: Kirigami.Dialog.Ok
    }
    Kirigami.PromptDialog {
        id: aboutDialog
        title: "MKW Track Editor"
        subtitle: "A course editor for Mario Kart World Banc files, in the spirit of Track Studio for Mario Kart 8 Deluxe.\n\nRust core and CXX-Qt bridge, Kirigami " + "(KDE Frameworks 6) interface, Qt Quick 3D viewport.\n\nFormat findings are checked against the shipped game files; see the README."
        standardButtons: Kirigami.Dialog.Ok
    }
}
