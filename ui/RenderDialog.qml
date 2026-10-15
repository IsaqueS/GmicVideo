import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.OverlaySheet {
    id: renderSheet
    title: "Rendering Video"

    property string stage: "idle"
    property real percent: 0.0
    property string statusMessage: "Preparing..."
    property int totalFrames: 0
    property int completedFrames: 0
    property real fps: 0.0
    property real etaSecs: 0.0
    property real elapsedSecs: 0.0
    property string outputPath: ""
    property bool isFinished: false
    property bool isError: false

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        implicitWidth: 460

        Kirigami.Heading {
            level: 3
            text: isFinished ? "Render Completed!" : (isError ? "Render Failed" : "Processing G'MIC Video...")
        }

        ProgressBar {
            Layout.fillWidth: true
            from: 0
            to: 100
            value: renderSheet.percent
            indeterminate: renderSheet.stage === "extracting" || renderSheet.stage === "encoding"
        }

        RowLayout {
            Layout.fillWidth: true
            Label {
                text: renderSheet.percent.toFixed(1) + "%"
                font.bold: true
            }
            Item { Layout.fillWidth: true }
            Label {
                text: renderSheet.completedFrames + " / " + renderSheet.totalFrames + " frames"
                visible: renderSheet.totalFrames > 0
            }
        }

        Label {
            text: renderSheet.statusMessage
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }

        RowLayout {
            Layout.fillWidth: true
            visible: !renderSheet.isFinished && !renderSheet.isError
            spacing: Kirigami.Units.largeSpacing

            Label {
                text: "Speed: " + renderSheet.fps.toFixed(1) + " fps"
                opacity: 0.8
            }
            Label {
                text: "Elapsed: " + renderSheet.elapsedSecs.toFixed(1) + "s"
                opacity: 0.8
            }
            Label {
                text: "ETA: " + (renderSheet.etaSecs > 0 ? renderSheet.etaSecs.toFixed(1) + "s" : "--")
                opacity: 0.8
            }
        }

        // Action Buttons
        RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }

            Button {
                text: "Cancel"
                visible: !renderSheet.isFinished && !renderSheet.isError
                icon.name: "dialog-cancel"
                onClicked: {
                    Backend.cancelRender();
                    renderSheet.close();
                }
            }

            Button {
                text: "Open Output Folder"
                visible: renderSheet.isFinished
                icon.name: "folder-open"
                onClicked: {
                    Backend.openFolder(renderSheet.outputPath);
                }
            }

            Button {
                text: "Play Video"
                visible: renderSheet.isFinished
                highlighted: true
                icon.name: "media-playback-start"
                onClicked: {
                    Backend.openFile(renderSheet.outputPath);
                }
            }

            Button {
                text: "Close"
                visible: renderSheet.isFinished || renderSheet.isError
                onClicked: renderSheet.close()
            }
        }
    }
}
