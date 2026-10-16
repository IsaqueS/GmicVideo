import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Window {
    id: renderWindow
    title: "Render Manager & Task Queue - G'MIC Video"
    width: 720
    height: 680
    minimumWidth: 560
    minimumHeight: 480
    visible: false
    color: Kirigami.Theme.backgroundColor

    function open() {
        show();
        raise();
        requestActivate();
    }

    property var videoInfo: null
    property var selectedFilter: null
    property var paramValues: ({})
    property var paramTweens: ({})
    property var queueItems: []
    property real cutStartTime: 0.0
    property real cutEndTime: videoInfo ? videoInfo.duration_secs : 0.0

property string selectedFormat: "h264"
    readonly property var formatModels: [
        { key: "h264", label: "H.264 (MP4)", ext: "mp4" },
        { key: "h265", label: "H.265 / HEVC (MP4)", ext: "mp4" },
        { key: "vp9", label: "VP9 (WebM)", ext: "webm" },
        { key: "av1", label: "AV1 (MP4)", ext: "mp4" },
        { key: "lossless", label: "Lossless RGB (MP4)", ext: "mp4" }
    ]

    function currentFormatExt() {
        for (var i = 0; i < formatModels.length; i++) {
            if (formatModels[i].key === selectedFormat) return formatModels[i].ext;
        }
        return "mp4";
    }

    onSelectedFormatChanged: {
        var newExt = currentFormatExt();
        if (chosenOutputPath !== "") {
            chosenOutputPath = chosenOutputPath.replace(/\.[^/.]+$/, "." + newExt);
            destPathField.text = chosenOutputPath;
        } else {
            destPathField.text = defaultOutputPath;
        }
    }

    property string defaultOutputPath: {
        var vname = videoInfo ? videoInfo.filename.replace(/\.[^/.]+$/, "") : "video";
        var fname = selectedFilter ? selectedFilter.id : "effect";
        var ext = currentFormatExt();
        if (videoInfo && videoInfo.path && videoInfo.path.indexOf("/") !== -1) {
            var dir = videoInfo.path.substring(0, videoInfo.path.lastIndexOf("/"));
            return dir + "/" + vname + "_" + fname + "." + ext;
        }
        return vname + "_" + fname + "." + ext;
    }

    property string chosenOutputPath: ""
    property int qualityLevel: 7
    property int speedLevel: 7

    function qualityDescription(val) {
        if (val <= 3) return "Draft Quality (Faster, larger compression artifacts)";
        if (val <= 6) return "Standard Quality (Balanced, recommended for sharing)";
        if (val <= 8) return "High Quality (Crisp details, high bitrate - Recommended)";
        return "Maximum Quality (Visually lossless, large file size)";
    }

    function speedDescription(val) {
        if (val <= 2) return "Maximum Compression (Slowest encode, smallest file)";
        if (val <= 4) return "Slow (Thorough compression)";
        if (val <= 6) return "Balanced (Medium speed and size)";
        if (val <= 8) return "Fast (Faster encoding - Recommended)";
        return "Ultrafast (Instant encode, ideal for quick testing)";
    }

    function hasFinishedJobs() {
        for (var i = 0; i < queueItems.length; i++) {
            var st = queueItems[i].status;
            if (st === "completed" || st === "failed" || st === "canceled") {
                return true;
            }
        }
        return false;
    }

    function getActiveJobCount() {
        var count = 0;
        for (var i = 0; i < queueItems.length; i++) {
            var st = queueItems[i].status;
            if (st === "rendering" || st === "queued") {
                count++;
            }
        }
        return count;
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        spacing: Kirigami.Units.mediumSpacing

        // Top Navigation Tabs
        TabBar {
            id: renderTabs
            Layout.fillWidth: true
            currentIndex: 0

            TabButton {
                text: "New Render"
                icon.name: "media-record"
            }
            TabButton {
                text: {
                    var c = renderWindow.getActiveJobCount();
                    return c > 0 ? "Task Queue (" + c + ")" : "Task Queue (" + renderWindow.queueItems.length + ")";
                }
                icon.name: "view-list-details"
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: renderTabs.currentIndex

            // TAB 0: Configure New Render
            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                contentWidth: availableWidth

                ColumnLayout {
                    width: parent.width
                    spacing: Kirigami.Units.largeSpacing

                    // Video & Effect Summary Card
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: summaryLayout.implicitHeight + 20
                        color: Kirigami.Theme.backgroundColor
                        border.color: Kirigami.Theme.separatorColor
                        border.width: 1
                        radius: 6

                        ColumnLayout {
                            id: summaryLayout
                            anchors.fill: parent
                            anchors.margins: 10
                            spacing: 6

                            RowLayout {
                                spacing: 8
                                Kirigami.Icon {
                                    source: "video-x-generic"
                                    implicitWidth: 18
                                    implicitHeight: 18
                                }
                                Label {
                                    text: videoInfo ? (videoInfo.filename + " (" + videoInfo.width + "x" + videoInfo.height + ", " + videoInfo.duration_secs.toFixed(1) + "s, " + videoInfo.fps.toFixed(0) + " fps)") : "No video imported"
                                    font.bold: true
                                }
                            }

                            RowLayout {
                                spacing: 8
                                Kirigami.Icon {
                                    source: "color-management"
                                    implicitWidth: 18
                                    implicitHeight: 18
                                }
                                Label {
                                    text: selectedFilter ? ("Effect: " + selectedFilter.name + " [" + selectedFilter.category + "]") : "No effect selected"
                                    opacity: 0.85
                                }
                            }

                            RowLayout {
                                spacing: 8
                                visible: videoInfo !== null
                                Kirigami.Icon {
                                    source: "edit-cut"
                                    implicitWidth: 18
                                    implicitHeight: 18
                                }
                                Label {
                                    text: {
                                        if (!videoInfo) return "";
                                        var isCut = (renderWindow.cutStartTime > 0.05 || (renderWindow.cutEndTime > 0.05 && renderWindow.cutEndTime < videoInfo.duration_secs - 0.05));
                                        if (isCut) {
                                            var dur = (renderWindow.cutEndTime - renderWindow.cutStartTime).toFixed(2);
                                            return "Cut Range: " + renderWindow.cutStartTime.toFixed(2) + "s -> " + renderWindow.cutEndTime.toFixed(2) + "s (Duration: " + dur + "s)";
                                        }
                                        return "Cut Range: Entire Video (0.00s -> " + videoInfo.duration_secs.toFixed(2) + "s)";
                                    }
                                    opacity: 0.85
                                    color: (renderWindow.cutStartTime > 0.05 || (videoInfo && renderWindow.cutEndTime < videoInfo.duration_secs - 0.05)) ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
                                }
                            }
                        }
                    }

                    // Output Destination
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 4

                        Label {
                            text: "Output Destination File:"
                            font.bold: true
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing

                            TextField {
                                id: destPathField
                                Layout.fillWidth: true
                                text: renderWindow.chosenOutputPath !== "" ? renderWindow.chosenOutputPath : renderWindow.defaultOutputPath
                                placeholderText: renderWindow.defaultOutputPath
                                onTextEdited: renderWindow.chosenOutputPath = text
                            }

                            ComboBox {
                                id: formatCombo
                                model: renderWindow.formatModels
                                textRole: "label"
                                currentIndex: {
                                    for (var i = 0; i < model.length; i++) {
                                        if (model[i].key === renderWindow.selectedFormat) return i;
                                    }
                                    return 0;
                                }
                                onActivated: {
                                    var chosen = model[currentIndex];
                                    renderWindow.selectedFormat = chosen.key;
                                }
                            }

                            Button {
                                text: "Browse..."
                                icon.name: "document-save"
                                onClicked: {
                                    var res = Backend.chooseOutputFile(renderWindow.defaultOutputPath, renderWindow.currentFormatExt());
                                    if (res) {
                                        renderWindow.chosenOutputPath = res;
                                        destPathField.text = res;
                                    }
                                }
                            }
                        }
                    }

                    Kirigami.Separator { Layout.fillWidth: true }

                    // Quality & Speed Settings (faded out smoothly when Lossless is selected)
                    ColumnLayout {
                        id: encodingSettingsBox
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.mediumSpacing
                        enabled: renderWindow.selectedFormat !== "lossless"
                        opacity: renderWindow.selectedFormat === "lossless" ? 0.25 : 1.0

                        Behavior on opacity {
                            NumberAnimation { duration: 250; easing.type: Easing.InOutQuad }
                        }

                        // Simplified Slider 1: Quality
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 4

                            RowLayout {
                                Layout.fillWidth: true
                                Label {
                                    text: "Quality"
                                    font.bold: true
                                }
                                Item { Layout.fillWidth: true }
                                Label {
                                    text: renderWindow.qualityDescription(qualitySlider.value)
                                    font.bold: true
                                    color: Kirigami.Theme.highlightColor
                                }
                            }

                            Slider {
                                id: qualitySlider
                                Layout.fillWidth: true
                                from: 1
                                to: 10
                                stepSize: 1
                                value: renderWindow.qualityLevel
                                onValueChanged: renderWindow.qualityLevel = value
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                Label {
                                    text: "Draft (Smaller file)"
                                    opacity: 0.6
                                    font.pixelSize: 11
                                }
                                Item { Layout.fillWidth: true }
                                Label {
                                    text: "Balanced"
                                    opacity: 0.6
                                    font.pixelSize: 11
                                }
                                Item { Layout.fillWidth: true }
                                Label {
                                    text: "Maximum Quality"
                                    opacity: 0.6
                                    font.pixelSize: 11
                                }
                            }
                        }

                        // Simplified Slider 2: Speed
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 4

                            RowLayout {
                                Layout.fillWidth: true
                                Label {
                                    text: "Encoding Speed"
                                    font.bold: true
                                }
                                Item { Layout.fillWidth: true }
                                Label {
                                    text: renderWindow.speedDescription(speedSlider.value)
                                    font.bold: true
                                    color: Kirigami.Theme.highlightColor
                                }
                            }

                            Slider {
                                id: speedSlider
                                Layout.fillWidth: true
                                from: 1
                                to: 10
                                stepSize: 1
                                value: renderWindow.speedLevel
                                onValueChanged: renderWindow.speedLevel = value
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                Label {
                                    text: "Max Compression"
                                    opacity: 0.6
                                    font.pixelSize: 11
                                }
                                Item { Layout.fillWidth: true }
                                Label {
                                    text: "Balanced"
                                    opacity: 0.6
                                    font.pixelSize: 11
                                }
                                Item { Layout.fillWidth: true }
                                Label {
                                    text: "Ultrafast Encode"
                                    opacity: 0.6
                                    font.pixelSize: 11
                                }
                            }
                        }
                    }

                    // Lossless Info Note (visible when Lossless is selected)
                    Rectangle {
                        visible: renderWindow.selectedFormat === "lossless"
                        Layout.fillWidth: true
                        implicitHeight: losslessNoteLayout.implicitHeight + 14
                        color: Kirigami.Theme.backgroundColor
                        border.color: Kirigami.Theme.highlightColor
                        border.width: 1
                        radius: 4

                        RowLayout {
                            id: losslessNoteLayout
                            anchors.fill: parent
                            anchors.margins: 8
                            spacing: 8

                            Kirigami.Icon {
                                source: "dialog-information"
                                implicitWidth: 16
                                implicitHeight: 16
                            }

                            Label {
                                Layout.fillWidth: true
                                text: "Lossless RGB encoding active: Quality and speed presets are bypassed to guarantee exact 1:1 mathematical pixel fidelity."
                                font.pixelSize: 11
                                opacity: 0.85
                                wrapMode: Text.WordWrap
                            }
                        }
                    }

                    Kirigami.Separator { Layout.fillWidth: true }

                    // Actions
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.mediumSpacing

                        Item { Layout.fillWidth: true }

                        Button {
                            text: "Add to Render Queue & Start"
                            icon.name: "media-record"
                            highlighted: true
                            enabled: videoInfo !== null && selectedFilter !== null
                            onClicked: {
                                var outPath = destPathField.text.trim();
                                if (!outPath) outPath = renderWindow.defaultOutputPath;

                                var isCut = (renderWindow.cutStartTime > 0.05 || (videoInfo && renderWindow.cutEndTime < videoInfo.duration_secs - 0.05));
                                var cStart = isCut ? renderWindow.cutStartTime : 0.0;
                                var cEnd = isCut ? renderWindow.cutEndTime : -1.0;

                                var jobId = Backend.queueRender(
                                    videoInfo.path,
                                    outPath,
                                    renderWindow.selectedFormat,
                                    renderWindow.qualityLevel,
                                    renderWindow.speedLevel,
                                    selectedFilter.id,
                                    selectedFilter.name,
                                    JSON.stringify(renderWindow.paramValues),
                                    JSON.stringify(renderWindow.paramTweens),
                                    cStart,
                                    cEnd
                                );

                                if (jobId) {
                                    renderTabs.currentIndex = 1;
                                }
                            }
                        }
                    }
                }
            }

            // TAB 1: Task Queue & History
            ColumnLayout {
                spacing: Kirigami.Units.mediumSpacing
                Layout.fillWidth: true
                Layout.fillHeight: true

                RowLayout {
                    Layout.fillWidth: true
                    Kirigami.Heading {
                        text: "Render Queue (" + renderWindow.queueItems.length + " items)"
                        level: 4
                    }
                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Clear Finished"
                        icon.name: "edit-clear-all"
                        visible: renderWindow.hasFinishedJobs()
                        onClicked: Backend.clearCompletedJobs()
                    }
                }

                // Placeholder when queue is empty
                Rectangle {
                    visible: renderWindow.queueItems.length === 0
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    color: Kirigami.Theme.backgroundColor
                    border.color: Kirigami.Theme.separatorColor
                    border.width: 1
                    radius: 6

                    ColumnLayout {
                        anchors.centerIn: parent
                        spacing: 8
                        Kirigami.Icon {
                            source: "view-list-details"
                            implicitWidth: 48
                            implicitHeight: 48
                            Layout.alignment: Qt.AlignHCenter
                            opacity: 0.4
                        }
                        Label {
                            text: "No render tasks in queue"
                            font.bold: true
                            Layout.alignment: Qt.AlignHCenter
                            opacity: 0.7
                        }
                        Label {
                            text: "Use the 'New Render' tab to add a video rendering task to the background queue."
                            font.pixelSize: 12
                            Layout.alignment: Qt.AlignHCenter
                            opacity: 0.5
                        }
                    }
                }

                // Scrollable Task List
                ScrollView {
                    visible: renderWindow.queueItems.length > 0
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true

                    ListView {
                        id: queueListView
                        width: parent.width
                        spacing: 10
                        model: renderWindow.queueItems

                        delegate: Rectangle {
                            id: jobCard
                            width: queueListView.width
                            implicitHeight: cardContent.implicitHeight + 16
                            color: {
                                if (modelData.status === "rendering") return Kirigami.Theme.highlightColor;
                                if (modelData.status === "completed") return "#1e3a1e";
                                if (modelData.status === "failed") return "#3a1e1e";
                                return Kirigami.Theme.backgroundColor;
                            }
                            opacity: modelData.status === "rendering" ? 0.2 : 0.95
                            border.color: {
                                if (modelData.status === "rendering") return Kirigami.Theme.highlightColor;
                                if (modelData.status === "completed") return "#2ecc71";
                                if (modelData.status === "failed") return "#e74c3c";
                                return Kirigami.Theme.separatorColor;
                            }
                            border.width: modelData.status === "rendering" ? 2 : 1
                            radius: 6

                            ColumnLayout {
                                id: cardContent
                                anchors.fill: parent
                                anchors.margins: 10
                                spacing: 8

                                // Title Row & Badge
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 8

                                    Kirigami.Icon {
                                        source: {
                                            if (modelData.status === "rendering") return "system-run";
                                            if (modelData.status === "completed") return "emblem-checked";
                                            if (modelData.status === "failed") return "dialog-error";
                                            if (modelData.status === "canceled") return "dialog-cancel";
                                            return "hourglass";
                                        }
                                        implicitWidth: 18
                                        implicitHeight: 18
                                    }

                                    ColumnLayout {
                                        spacing: 2
                                        Layout.fillWidth: true
                                        Label {
                                            text: modelData.name || "Render Task"
                                            font.bold: true
                                            elide: Text.ElideMiddle
                                            Layout.fillWidth: true
                                        }
                                        Label {
                                            text: "Effect: " + modelData.filter_name + (modelData.format ? (" | " + modelData.format.toUpperCase()) : "") + " | Preset: " + modelData.speed_preset
                                            font.pixelSize: 11
                                            opacity: 0.7
                                        }
                                    }

                                    Rectangle {
                                        color: {
                                            if (modelData.status === "rendering") return "#3498db";
                                            if (modelData.status === "completed") return "#2ecc71";
                                            if (modelData.status === "failed") return "#e74c3c";
                                            if (modelData.status === "canceled") return "#7f8c8d";
                                            return "#f39c12";
                                        }
                                        radius: 4
                                        implicitWidth: statusText.implicitWidth + 12
                                        implicitHeight: 22

                                        Label {
                                            id: statusText
                                            anchors.centerIn: parent
                                            text: modelData.status.toUpperCase()
                                            font.bold: true
                                            font.pixelSize: 10
                                            color: "white"
                                        }
                                    }

                                    Button {
                                        icon.name: "dialog-cancel"
                                        text: "Cancel"
                                        visible: modelData.status === "rendering" || modelData.status === "queued"
                                        onClicked: Backend.cancelRenderJob(modelData.id)
                                    }
                                }

                                // Active rendering progress bar & stats
                                ColumnLayout {
                                    visible: modelData.status === "rendering"
                                    Layout.fillWidth: true
                                    spacing: 4

                                    ProgressBar {
                                        Layout.fillWidth: true
                                        from: 0
                                        to: 100
                                        value: modelData.progress ? modelData.progress.percent : 0
                                        indeterminate: modelData.progress ? (modelData.progress.stage === "extracting") : false
                                    }

                                    RowLayout {
                                        Layout.fillWidth: true
                                        Label {
                                            text: (modelData.progress ? modelData.progress.percent.toFixed(1) : "0.0") + "%"
                                            font.bold: true
                                        }
                                        Item { Layout.fillWidth: true }
                                        Label {
                                            text: modelData.progress ? (modelData.progress.completed_frames + " / " + modelData.progress.total_frames + " frames") : ""
                                            font.pixelSize: 11
                                            opacity: 0.8
                                        }
                                        Item { Layout.fillWidth: true }
                                        Label {
                                            text: modelData.progress ? ("Speed: " + modelData.progress.fps.toFixed(1) + " fps") : ""
                                            font.pixelSize: 11
                                            opacity: 0.8
                                        }
                                        Item { Layout.fillWidth: true }
                                        Label {
                                            text: modelData.progress && modelData.progress.eta_secs > 0 ? ("ETA: " + modelData.progress.eta_secs.toFixed(0) + "s") : ""
                                            font.pixelSize: 11
                                            opacity: 0.8
                                        }
                                    }

                                    Label {
                                        text: modelData.progress ? modelData.progress.message : ""
                                        font.pixelSize: 11
                                        opacity: 0.85
                                        wrapMode: Text.WordWrap
                                        Layout.fillWidth: true
                                    }
                                }

                                // Completed actions
                                RowLayout {
                                    visible: modelData.status === "completed"
                                    Layout.fillWidth: true
                                    spacing: 8

                                    Label {
                                        text: modelData.progress ? modelData.progress.message : "Finished"
                                        font.pixelSize: 11
                                        opacity: 0.8
                                        Layout.fillWidth: true
                                        elide: Text.ElideMiddle
                                    }

                                    Button {
                                        text: "Open Folder"
                                        icon.name: "folder-open"
                                        onClicked: Backend.openFolder(modelData.output_path)
                                    }

                                    Button {
                                        text: "Play Video"
                                        icon.name: "media-playback-start"
                                        highlighted: true
                                        onClicked: Backend.openFile(modelData.output_path)
                                    }
                                }

                                // Error message if failed
                                Label {
                                    visible: modelData.status === "failed"
                                    text: "Error: " + (modelData.error || (modelData.progress ? modelData.progress.message : "Unknown error"))
                                    color: "#e74c3c"
                                    font.pixelSize: 11
                                    wrapMode: Text.WordWrap
                                    Layout.fillWidth: true
                                }
                            }
                        }
                    }
                }
            }
        }

        // Bottom Status & Window Controls
        Kirigami.Separator { Layout.fillWidth: true }

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.mediumSpacing

            Label {
                text: {
                    var c = renderWindow.getActiveJobCount();
                    if (c > 0) return c + " active task" + (c > 1 ? "s" : "") + " rendering in background...";
                    return "Ready - Rendering runs independently in the background";
                }
                opacity: 0.7
                font.pixelSize: 11
            }

            Item { Layout.fillWidth: true }

            Button {
                text: "Close Window"
                icon.name: "window-close"
                onClicked: renderWindow.close()
            }
        }
    }
}
