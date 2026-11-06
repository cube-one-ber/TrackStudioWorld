import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.mkw.trackeditor

QQC2.ScrollView {
    id: cp
    required property Editor editor
    QQC2.TextArea {
        id: area
        readOnly: true
        text: cp.editor.consoleText
        font.family: "monospace"
        wrapMode: TextEdit.Wrap
        background: null
        onTextChanged: cursorPosition = length
    }
}
