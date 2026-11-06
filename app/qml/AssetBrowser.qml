pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.mkw.trackeditor

// Map-object browser with model thumbnails (Track Studio's Assets dock).
ColumnLayout {
    id: ab
    required property Editor editor
    property string currentAsset
    signal assetChosen(string name)
    signal assetActivated(string name)
    signal changeRequested(string name)
    property var items: []
    spacing: 0

    function refresh() {
        items = JSON.parse(editor.assets(search.text, category.currentIndex > 0 ? category.currentText : "", placed.checked));
    }
    Connections {
        target: ab.editor
        function onReadyChanged() {
            category.model = ["All categories"].concat(JSON.parse(ab.editor.categories()));
            ab.refresh();
        }
        function onCourseTitleChanged() { ab.refresh(); }
        function onThumbsDoneChanged() { if (ab.editor.thumbsDone % 100 === 0 || ab.editor.thumbsDone === ab.editor.thumbsTotal) ab.refresh(); }
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.margins: Kirigami.Units.smallSpacing
        Kirigami.SearchField {
            id: search
            Layout.fillWidth: true
            placeholderText: "Search name or Japanese label"
            onTextChanged: ab.refresh()
        }
        QQC2.ComboBox {
            id: category
            model: ["All categories"]
            onActivated: ab.refresh()
        }
        QQC2.CheckBox {
            id: placed
            text: "Used in shipped courses"
            checked: true
            onToggled: ab.refresh()
            QQC2.ToolTip.text: "Only list objects that at least one of the game's courses places"
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
        // legend for the dot on the cells
        Rectangle {
            implicitWidth: 8
            implicitHeight: 8
            radius: 4
            color: Kirigami.Theme.positiveTextColor
            Layout.leftMargin: Kirigami.Units.smallSpacing
        }
        QQC2.Label {
            text: "In this course"
            color: Kirigami.Theme.disabledTextColor
        }
        Kirigami.Separator {
            Layout.preferredHeight: Kirigami.Units.gridUnit
            Layout.leftMargin: Kirigami.Units.smallSpacing
            Layout.rightMargin: Kirigami.Units.smallSpacing
        }
        QQC2.ToolButton {
            display: QQC2.AbstractButton.TextBesideIcon
            text: ab.editor.thumbsTotal > 0 && ab.editor.thumbsDone < ab.editor.thumbsTotal ? "Thumbnails " + ab.editor.thumbsDone + "/" + ab.editor.thumbsTotal : "Build Thumbnails"
            icon.name: "view-preview"
            enabled: ab.editor.ready && !(ab.editor.thumbsTotal > 0 && ab.editor.thumbsDone < ab.editor.thumbsTotal)
            onClicked: ab.editor.buildThumbnails()
            QQC2.ToolTip.text: "Render a preview of every object model (runs in the background, cached)"
            QQC2.ToolTip.visible: hovered
        }
    }
    GridView {
        id: grid
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        leftMargin: Kirigami.Units.smallSpacing
        rightMargin: Kirigami.Units.smallSpacing
        cellWidth: Kirigami.Units.gridUnit * 6
        cellHeight: Kirigami.Units.gridUnit * 7.2
        model: ab.items
        boundsBehavior: Flickable.StopAtBounds
        QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
        delegate: QQC2.ItemDelegate {
            id: cell
            required property var modelData
            width: grid.cellWidth
            height: grid.cellHeight
            highlighted: ab.currentAsset === modelData.name
            padding: 4
            // a card per object; the current one (used by Add / Q / Alt+Click) is outlined
            background: Rectangle {
                anchors.fill: parent
                anchors.margins: 2
                radius: Kirigami.Units.cornerRadius
                readonly property color hl: Kirigami.Theme.highlightColor
                color: cell.highlighted ? Qt.rgba(hl.r, hl.g, hl.b, 0.28)
                     : cell.hovered ? Qt.rgba(hl.r, hl.g, hl.b, 0.12)
                     : Kirigami.Theme.alternateBackgroundColor
                border.color: cell.highlighted ? hl : "transparent"
                border.width: 1
            }
            contentItem: ColumnLayout {
                spacing: 1
                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.minimumHeight: Kirigami.Units.gridUnit * 2
                    Image {
                        anchors.fill: parent
                        anchors.margins: 2
                        source: cell.modelData.thumb
                        visible: cell.modelData.thumb.length > 0
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                        smooth: true
                    }
                    Rectangle {
                        anchors.centerIn: parent
                        width: Math.min(parent.width, parent.height) * 0.62
                        height: width
                        radius: 4
                        visible: cell.modelData.thumb.length === 0
                        color: cell.modelData.color
                        opacity: 0.85
                        Kirigami.Icon {
                            anchors.centerIn: parent
                            width: parent.width * 0.6
                            height: width
                            source: cell.modelData.model ? "box" : "draw-cuboid"
                            color: "white"
                        }
                    }
                    Rectangle {
                        visible: cell.modelData.inCourse
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 2
                        width: 8
                        height: 8
                        radius: 4
                        color: Kirigami.Theme.positiveTextColor
                    }
                }
                QQC2.Label {
                    // zero-width spaces between word parts, so "AudienceKinopio" wraps as "Audience / Kinopio"
                    text: cell.modelData.short.replace(/([a-z])([A-Z0-9])/g, "$1\u200B$2")
                    Layout.fillWidth: true
                    // always two lines tall, so the thumbnails line up
                    Layout.preferredHeight: twoLines.implicitHeight
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignTop
                    wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                    QQC2.Label {
                        id: twoLines
                        visible: false
                        text: "A\nA"
                        font: parent.font
                    }
                }
            }
            QQC2.ToolTip.text: modelData.name + "\n" + modelData.label + "\n" + modelData.category + (modelData.inCourse ? "\n(in this course)" : "")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: 500
            onClicked: ab.assetChosen(modelData.name)
            onDoubleClicked: ab.assetActivated(modelData.name)
            TapHandler {
                acceptedButtons: Qt.RightButton
                onTapped: menu.popup()
            }
            QQC2.Menu {
                id: menu
                QQC2.MenuItem { text: "Add at view centre"; icon.name: "list-add"; onTriggered: ab.assetActivated(cell.modelData.name) }
                QQC2.MenuItem { text: "Change selected objects to this"; icon.name: "exchange-positions"; onTriggered: ab.changeRequested(cell.modelData.name) }
            }
        }
    }
}
