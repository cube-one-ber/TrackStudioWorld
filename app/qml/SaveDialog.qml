pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.mkw.trackeditor

// Result of a save: what was written, and how to load the mod in an emulator.
Kirigami.Dialog {
    id: dlg
    required property Editor editor
    property var saveResult: ({ ok: false })
    readonly property bool ok: saveResult.ok === true
    // width for text: long paths wrap or elide instead of stretching the dialog
    readonly property real textWidth: Kirigami.Units.gridUnit * 30
    title: ok ? "Mod saved" : "Not saved"
    preferredWidth: Kirigami.Units.gridUnit * 34
    padding: Kirigami.Units.largeSpacing
    standardButtons: Kirigami.Dialog.Close
    customFooterActions: [
        Kirigami.Action {
            text: "Open Mod Folder"
            icon.name: "folder-open"
            enabled: dlg.ok
            onTriggered: dlg.editor.openFolder("mods")
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        // always visible and never animated: a Kirigami.Dialog only grows to fit content that has
        // a height when it opens (an InlineMessage animates in from zero)
        RowLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: dlg.textWidth
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Icon {
                source: dlg.ok ? "dialog-ok" : "dialog-error"
                implicitWidth: Kirigami.Units.iconSizes.medium
                implicitHeight: Kirigami.Units.iconSizes.medium
            }
            QQC2.Label {
                Layout.fillWidth: true
                Layout.preferredWidth: dlg.textWidth
                wrapMode: Text.Wrap
                text: dlg.ok ? "Saved " + (dlg.saveResult.files || []).length + " file(s) to the mod \"" + dlg.saveResult.mod + "\"."
                             : ((dlg.saveResult.errors || []).join("\n") || "Nothing was written.")
            }
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: dlg.textWidth
            visible: dlg.ok
            QQC2.Label {
                Kirigami.FormData.label: "Mod folder:"
                Layout.fillWidth: true
                Layout.preferredWidth: dlg.textWidth
                Layout.maximumWidth: Kirigami.Units.gridUnit * 24
                text: dlg.saveResult.modDir || ""
                elide: Text.ElideMiddle
                QQC2.ToolTip.text: text
                QQC2.ToolTip.visible: dirHover.hovered
                HoverHandler { id: dirHover }
            }
            ColumnLayout {
                Kirigami.FormData.label: "Files:"
                spacing: 2
                Repeater {
                    model: dlg.saveResult.files || []
                    delegate: RowLayout {
                        required property string modelData
                        required property int index
                        spacing: Kirigami.Units.smallSpacing
                        Kirigami.Icon {
                            source: "text-x-generic"
                            implicitWidth: Kirigami.Units.iconSizes.small
                            implicitHeight: Kirigami.Units.iconSizes.small
                        }
                        QQC2.Label {
                            text: modelData
                            elide: Text.ElideMiddle
                            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                        }
                    }
                }
            }
        }

        Repeater {
            model: dlg.saveResult.notes || []
            delegate: QQC2.Label {
                required property string modelData
                text: "• " + modelData
                Layout.fillWidth: true
                Layout.preferredWidth: dlg.textWidth
                wrapMode: Text.Wrap
            }
        }
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            Layout.preferredWidth: dlg.textWidth
            visible: (dlg.saveResult.warnings || []).length > 0
            type: Kirigami.MessageType.Warning
            text: (dlg.saveResult.warnings || []).slice(0, 12).join("\n") + ((dlg.saveResult.warnings || []).length > 12 ? "\n…" : "")
        }

        Kirigami.Separator {
            Layout.fillWidth: true
            Layout.preferredWidth: dlg.textWidth
            visible: dlg.ok
        }
        // Switch 2: the ns2jb-modloader export (when set up in Settings)
        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: dlg.textWidth
            visible: dlg.ok
            spacing: Kirigami.Units.smallSpacing
            readonly property var ex: dlg.saveResult.ns2jb || null
            QQC2.Label {
                text: "Play it on Switch 2"
                font.weight: Font.DemiBold
            }
            QQC2.Label {
                Layout.fillWidth: true
                Layout.preferredWidth: dlg.textWidth
                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                color: parent.ex && !parent.ex.ok ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
                text: !parent.ex ? "To export for ns2jb-modloader on every save, choose the SD card in Settings → Switch 2."
                    : parent.ex.ok ? "Exported " + parent.ex.files.length + " file(s) for ns2jb-modloader to:"
                    : "ns2jb-modloader export failed: " + parent.ex.errors.join("; ")
            }
            QQC2.Label {
                visible: !!parent.ex && parent.ex.ok
                Layout.fillWidth: true
                Layout.preferredWidth: dlg.textWidth
                text: parent.ex ? parent.ex.dir : ""
                elide: Text.ElideMiddle
                font.family: "monospace"
                font.pointSize: Kirigami.Theme.smallFont.pointSize
                QQC2.ToolTip.text: text
                QQC2.ToolTip.visible: exHover.hovered
                HoverHandler { id: exHover }
            }
            QQC2.Label {
                visible: !!parent.ex && parent.ex.ok
                Layout.fillWidth: true
                Layout.preferredWidth: dlg.textWidth
                wrapMode: Text.Wrap
                text: "Put the card in the Switch 2, run node load.cjs from ns2jb-modloader's ModLoader folder, wait for READY, then open the game."
            }
        }
        Kirigami.Separator {
            Layout.fillWidth: true
            Layout.preferredWidth: dlg.textWidth
            visible: dlg.ok
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: dlg.textWidth
            visible: dlg.ok
            spacing: Kirigami.Units.smallSpacing
            QQC2.Label {
                text: "Play it in an emulator"
                font.weight: Font.DemiBold
            }
            QQC2.Label {
                Layout.fillWidth: true
                Layout.preferredWidth: dlg.textWidth
                wrapMode: Text.Wrap
                text: dlg.saveResult.installed ? "The mod is already in the emulator's mod folder."
                    : "Eden-based emulators such as Eglantine load mods from their load folder. This command copies the mod there:"
            }
            RowLayout {
                visible: !dlg.saveResult.installed
                Layout.fillWidth: true
                Layout.preferredWidth: dlg.textWidth
                spacing: Kirigami.Units.smallSpacing
                QQC2.TextArea {
                    id: cmd
                    Layout.fillWidth: true
                    Layout.preferredWidth: dlg.textWidth
                    readOnly: true
                    // one command per line; a line ending in && continues in the shell
                    text: (dlg.saveResult.install || "").replace(" && ", " &&\n")
                    wrapMode: TextEdit.WrapAnywhere
                    font.family: "monospace"
                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                }
                QQC2.Button {
                    Layout.alignment: Qt.AlignTop
                    text: copied.running ? "Copied" : "Copy"
                    icon.name: copied.running ? "dialog-ok-apply" : "edit-copy"
                    onClicked: {
                        cmd.selectAll();
                        cmd.copy();
                        cmd.deselect();
                        copied.restart();
                    }
                    Timer { id: copied; interval: 1500 }
                }
            }
        }
    }
}
