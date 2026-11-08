import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// A dock-style panel: title bar (or tab strip) over its content, like Track Studio's docks.
QQC2.Control {
    id: panel

    property string title
    property string iconName
    property var tabs: []
    property int currentTab: 0
    default property alias content: body.data
    property alias headerTools: tools.data
    // a collapsible panel folds to its title bar when the title bar is clicked
    property bool collapsible: false
    property bool collapsed: false
    readonly property real headerHeight: header.implicitHeight

    padding: 0
    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    background: Rectangle {
        color: Kirigami.Theme.backgroundColor
        border.color: Qt.darker(Kirigami.Theme.backgroundColor, 1.35)
    }

    contentItem: ColumnLayout {
        spacing: 0

        Rectangle {
            id: header
            Layout.fillWidth: true
            implicitHeight: Kirigami.Units.gridUnit * 1.6
            Kirigami.Theme.colorSet: Kirigami.Theme.Header
            Kirigami.Theme.inherit: false
            color: Kirigami.Theme.backgroundColor

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Kirigami.Units.smallSpacing
                anchors.rightMargin: Kirigami.Units.smallSpacing
                spacing: Kirigami.Units.smallSpacing

                Kirigami.Icon {
                    visible: panel.collapsible
                    source: panel.collapsed ? "arrow-right" : "arrow-down"
                    implicitWidth: Kirigami.Units.iconSizes.small
                    implicitHeight: Kirigami.Units.iconSizes.small
                }
                Kirigami.Icon {
                    source: panel.iconName
                    visible: panel.iconName.length > 0 && panel.tabs.length === 0
                    implicitWidth: Kirigami.Units.iconSizes.small
                    implicitHeight: Kirigami.Units.iconSizes.small
                }
                QQC2.Label {
                    visible: panel.tabs.length === 0
                    text: panel.title
                    font.weight: Font.DemiBold
                }
                Repeater {
                    model: panel.tabs
                    delegate: QQC2.ToolButton {
                        required property string modelData
                        required property int index
                        text: modelData
                        checkable: true
                        checked: panel.currentTab === index
                        font.weight: checked ? Font.DemiBold : Font.Normal
                        onClicked: panel.currentTab = index
                        Layout.preferredHeight: Kirigami.Units.gridUnit * 1.5
                    }
                }
                Item {
                    Layout.fillWidth: true
                }
                RowLayout {
                    id: tools
                    spacing: 0
                }
            }
            Kirigami.Separator {
                anchors.bottom: parent.bottom
                width: parent.width
            }
            TapHandler {
                enabled: panel.collapsible
                onTapped: panel.collapsed = !panel.collapsed
            }
            HoverHandler {
                enabled: panel.collapsible
                cursorShape: Qt.PointingHandCursor
            }
        }

        Item {
            id: body
            visible: !panel.collapsed
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
        }
    }
}
