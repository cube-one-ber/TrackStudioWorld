import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Collapsible property section ("▼ Transform").
ColumnLayout {
    id: sec
    property string title
    property bool expanded: true
    default property alias content: body.data
    Layout.fillWidth: true
    spacing: 0

    QQC2.ItemDelegate {
        Layout.fillWidth: true
        implicitHeight: Kirigami.Units.gridUnit * 1.4
        Kirigami.Theme.colorSet: Kirigami.Theme.Header
        Kirigami.Theme.inherit: false
        background: Rectangle { color: Kirigami.Theme.backgroundColor }
        contentItem: RowLayout {
            Kirigami.Icon {
                source: sec.expanded ? "arrow-down" : "arrow-right"
                implicitWidth: Kirigami.Units.iconSizes.small
                implicitHeight: Kirigami.Units.iconSizes.small
            }
            QQC2.Label {
                text: sec.title
                font.weight: Font.DemiBold
                Layout.fillWidth: true
            }
        }
        onClicked: sec.expanded = !sec.expanded
    }
    ColumnLayout {
        id: body
        visible: sec.expanded
        Layout.fillWidth: true
        Layout.margins: Kirigami.Units.smallSpacing
        spacing: 2
    }
}
