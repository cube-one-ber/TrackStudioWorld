pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import TrackStudioWorld

Kirigami.Dialog {
    id: dlg
    required property Editor editor
    title: "Settings"
    preferredWidth: Kirigami.Units.gridUnit * 40
    padding: Kirigami.Units.largeSpacing
    standardButtons: Kirigami.Dialog.Apply | Kirigami.Dialog.Cancel
    property string error: ""
    onOpened: {
        romfs.text = editor.romfsPath;
        models.text = editor.modelsPath;
        mods.text = editor.modsPath;
        ns2jb.text = editor.ns2jbPath;
        ns2jbOnSave.checked = editor.ns2jbOnSave;
        error = "";
    }
    onApplied: {
        error = editor.setExport(ns2jb.text, ns2jbOnSave.checked);
        // changing the game folders reloads them; the export target alone does not
        if (error.length === 0 && (romfs.text !== editor.romfsPath || models.text !== editor.modelsPath || mods.text !== editor.modsPath))
            error = editor.applySettings(romfs.text, models.text, mods.text);
        if (error.length === 0)
            dlg.close();
    }
    FolderDialog {
        id: picker
        property var target
        onAccepted: target.text = decodeURIComponent(selectedFolder.toString().replace("file://", ""))
    }
    ColumnLayout {
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: dlg.error.length > 0
            type: Kirigami.MessageType.Error
            text: dlg.error
        }
        Kirigami.FormLayout {
            Layout.fillWidth: true
            RowLayout {
                Kirigami.FormData.label: "Game romfs:"
                QQC2.TextField { id: romfs; Layout.fillWidth: true; Layout.minimumWidth: Kirigami.Units.gridUnit * 24 }
                QQC2.Button { icon.name: "folder-open"; onClicked: { picker.target = romfs; picker.open(); } }
            }
            RowLayout {
                Kirigami.FormData.label: "Exported models:"
                QQC2.TextField { id: models; Layout.fillWidth: true }
                QQC2.Button { icon.name: "folder-open"; onClicked: { picker.target = models; picker.open(); } }
            }
            RowLayout {
                Kirigami.FormData.label: "Mods folder:"
                QQC2.TextField { id: mods; Layout.fillWidth: true }
                QQC2.Button { icon.name: "folder-open"; onClicked: { picker.target = mods; picker.open(); } }
            }
            QQC2.Label {
                Layout.fillWidth: true
                // a label's implicit width is its unwrapped text: cap it so the dialog keeps its width
                Layout.maximumWidth: Kirigami.Units.gridUnit * 30
                wrapMode: Text.Wrap
                opacity: 0.7
                text: "Mods are written to <mods folder>/<mod name>/romfs. Point the mods folder at ~/.local/share/eden/load/0400C3F00006E000 to save straight into the emulator."
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: "Switch 2 (ns2jb-modloader)"
            }
            RowLayout {
                Kirigami.FormData.label: "SD card:"
                QQC2.TextField { id: ns2jb; Layout.fillWidth: true; placeholderText: "/run/media/you/SDCARD" }
                QQC2.Button { icon.name: "folder-open"; onClicked: { picker.target = ns2jb; picker.open(); } }
            }
            QQC2.CheckBox {
                id: ns2jbOnSave
                text: "Export on every save"
            }
            QQC2.Label {
                Layout.fillWidth: true
                // a label's implicit width is its unwrapped text: cap it so the dialog keeps its width
                Layout.maximumWidth: Kirigami.Units.gridUnit * 30
                wrapMode: Text.Wrap
                opacity: 0.7
                text: "Copies the mod to <SD>/ns2jb-modloader/contents/0400c3f00006e000/romfs, next to any other Mario Kart World mods there (the resource size table is merged, not replaced). Then load it with ns2jb-modloader: node load.cjs, wait for READY, open the game."
            }
        }
    }
}
