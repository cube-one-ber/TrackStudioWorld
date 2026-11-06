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
        }

        Item {
            id: body
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
        }
    }
}
