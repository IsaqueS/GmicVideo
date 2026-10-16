import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.ApplicationWindow {
    id: appWindow
    width: 1280
    height: 800
    minimumWidth: 960
    minimumHeight: 650
    title: videoInfo ? i18n.t("G'MIC Video - Effects Workspace") : i18n.t("G'MIC Video - Import Screen")
    pageStack.globalToolBar.style: Kirigami.ApplicationHeaderStyle.None

    // App state
    property var videoInfo: null
    property var allFilters: []
    property var allCategories: ["All Categories"]
    property string selectedCategory: "All Categories"
    property string searchQuery: ""
    property var selectedFilter: null
    property var paramValues: ({})
    property var paramTweens: ({})
    property string previewRawPath: ""
    property string previewEffectPath: ""
    property real currentScrubTime: 0.0
    property real cutStartTime: 0.0
    property real cutEndTime: 0.0
    property bool splitPreview: false
    property bool isPreviewLoading: false

    // Render Queue state
    property var renderQueue: []
    property int activeRenderCount: 0
    property real activeRenderPercent: 0.0

    // Tween easing names
    property var easeList: [
        { key: "linear", label: "Linear" },
        { key: "ease_in_quad", label: "Ease In Quad" },
        { key: "ease_out_quad", label: "Ease Out Quad" },
        { key: "ease_in_out_quad", label: "Ease In-Out Quad" },
        { key: "ease_in_cubic", label: "Ease In Cubic" },
        { key: "ease_out_cubic", label: "Ease Out Cubic" },
        { key: "ease_in_out_cubic", label: "Ease In-Out Cubic" },
        { key: "ease_in_quart", label: "Ease In Quart" },
        { key: "ease_out_quart", label: "Ease Out Quart" },
        { key: "ease_in_out_quart", label: "Ease In-Out Quart" },
        { key: "ease_in_sine", label: "Ease In Sine" },
        { key: "ease_out_sine", label: "Ease Out Sine" },
        { key: "ease_in_out_sine", label: "Ease In-Out Sine" },
        { key: "ease_in_expo", label: "Ease In Expo" },
        { key: "ease_out_expo", label: "Ease Out Expo" },
        { key: "ease_in_out_expo", label: "Ease In-Out Expo" },
        { key: "ease_in_circ", label: "Ease In Circ" },
        { key: "ease_out_circ", label: "Ease Out Circ" },
        { key: "ease_in_out_circ", label: "Ease In-Out Circ" },
        { key: "ease_in_back", label: "Ease In Back" },
        { key: "ease_out_back", label: "Ease Out Back" },
        { key: "ease_in_out_back", label: "Ease In-Out Back" },
        { key: "ease_in_elastic", label: "Ease In Elastic" },
        { key: "ease_out_elastic", label: "Ease Out Elastic" },
        { key: "ease_in_out_elastic", label: "Ease In-Out Elastic" },
        { key: "ease_in_bounce", label: "Ease In Bounce" },
        { key: "ease_out_bounce", label: "Ease Out Bounce" },
        { key: "ease_in_out_bounce", label: "Ease In-Out Bounce" },
        { key: "custom_bezier", label: "Custom Bezier Curve" }
    ]

    Component.onCompleted: {
        loadFilters();
    }

    function loadFilters() {
        var rawJson = Backend.getEffects();
        if (rawJson && rawJson.length > 2) {
            var data = JSON.parse(rawJson);
            allFilters = data.filters || [];
            var cats = ["All Categories"];
            if (data.categories) {
                for (var i = 0; i < data.categories.length; i++) {
                    cats.push(data.categories[i]);
                }
            }
            allCategories = cats;

            // Default to UltraWarp 2 or Halftone
            for (var j = 0; j < allFilters.length; j++) {
                if (allFilters[j].name === "UltraWarp 2" || allFilters[j].command === "fx_ultrawarptwo") {
                    selectFilter(allFilters[j]);
                    break;
                }
            }
            if (!selectedFilter && allFilters.length > 0) {
                selectFilter(allFilters[0]);
            }
        }
    }

    function selectFilter(filter) {
        selectedFilter = filter;
        var newValues = {};
        var newTweens = {};
        if (filter && filter.parameters) {
            for (var i = 0; i < filter.parameters.length; i++) {
                var p = filter.parameters[i];
                var ptype = p.param_type.type;
                var pdata = p.param_type.data;
                var defVal = 0.0;
                if (ptype === "float" || ptype === "int") {
                    defVal = pdata.default;
                } else if (ptype === "choice") {
                    defVal = pdata.default;
                } else if (ptype === "bool") {
                    defVal = pdata.default ? 1.0 : 0.0;
                }
                newValues[p.id] = defVal;

                newTweens[p.id] = {
                    mode: "disabled",
                    enabled: false,
                    start_value: (ptype === "float" || ptype === "int") ? (pdata.min !== undefined ? pdata.min : 0.0) : defVal,
                    end_value: (ptype === "float" || ptype === "int") ? (pdata.max !== undefined ? pdata.max : 100.0) : defVal,
                    ease_type: "linear",
                    bezier_points: [0.25, 0.25, 0.75, 0.75]
                };
            }
        }
        paramValues = newValues;
        paramTweens = newTweens;
        requestPreviewUpdate();
    }

    function loadVideo(filePath) {
        if (!filePath) return;
        var infoJson = Backend.probeVideo(filePath);
        var info = JSON.parse(infoJson);
        if (info.error) {
            showPassiveNotification("Failed to open video: " + info.error, "short");
            return;
        }
        videoInfo = info;
        currentScrubTime = 0.0;
        cutStartTime = 0.0;
        cutEndTime = info.duration_secs;
        requestPreviewUpdate();
    }

    function requestPreviewUpdate() {
        if (!videoInfo || !selectedFilter) return;
        previewTimer.restart();
    }

    Timer {
        id: previewTimer
        interval: 150
        repeat: false
        onTriggered: {
            if (!videoInfo || !selectedFilter) return;
            appWindow.isPreviewLoading = true;
            Backend.renderPreview(
                videoInfo.path,
                appWindow.currentScrubTime,
                videoInfo.duration_secs,
                selectedFilter.id,
                JSON.stringify(appWindow.paramValues),
                JSON.stringify(appWindow.paramTweens)
            );
        }
    }

    Connections {
        target: Backend
        function onPreviewReady(rawPath, effectPath, timeSecs) {
            appWindow.previewRawPath = "file://" + rawPath;
            appWindow.previewEffectPath = "file://" + effectPath;
            appWindow.isPreviewLoading = false;
        }
        function onQueueUpdated(jsonStr) {
            try {
                var items = JSON.parse(jsonStr) || [];
                appWindow.renderQueue = items;
                renderDialog.queueItems = items;

                var activeCount = 0;
                var currPercent = 0.0;
                for (var i = 0; i < items.length; i++) {
                    var item = items[i];
                    if (item.status === "rendering" || item.status === "queued") {
                        activeCount++;
                    }
                    if (item.status === "rendering" && item.progress) {
                        currPercent = item.progress.percent || 0.0;
                    }
                }
                appWindow.activeRenderCount = activeCount;
                appWindow.activeRenderPercent = currPercent;
            } catch (e) {
                console.error("Error parsing queue update: " + e);
            }
        }
    }

    // Modal dialogs
    BezierCurveEditor {
        id: curveEditor
        onCurveApplied: function(px1, py1, px2, py2) {
            if (activeTweenParamId !== null) {
                var tws = Object.assign({}, appWindow.paramTweens);
                var tw = tws[activeTweenParamId] || {};
                tw.bezier_points = [px1, py1, px2, py2];
                tw.ease_type = "custom_bezier";
                tws[activeTweenParamId] = tw;
                appWindow.paramTweens = tws;
                requestPreviewUpdate();
            }
        }
    }
    property var activeTweenParamId: null

    VisualSearchWindow {
        id: visualSearchWindow
        allFilters: appWindow.allFilters
        allCategories: appWindow.allCategories
        onEffectSelected: function(filter) {
            appWindow.selectFilter(filter);
            appWindow.selectedCategory = filter.category;
            if (categoryComboBox && categoryComboBox.model) {
                for (var i = 0; i < categoryComboBox.model.length; i++) {
                    if (categoryComboBox.model[i] === filter.category) {
                        categoryComboBox.currentIndex = i;
                        break;
                    }
                }
            }
        }
    }

    RenderDialog {
        id: renderDialog
        objectName: "renderDialog"
        videoInfo: appWindow.videoInfo
        selectedFilter: appWindow.selectedFilter
        paramValues: appWindow.paramValues
        paramTweens: appWindow.paramTweens
        queueItems: appWindow.renderQueue
        cutStartTime: appWindow.cutStartTime
        cutEndTime: appWindow.cutEndTime
    }

    // Top Header
    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.mediumSpacing

            ToolButton {
                text: i18n.t("Import Video")
                icon.name: "document-open"
                onClicked: {
                    var file = Backend.chooseVideoFile();
                    if (file) loadVideo(file);
                }
            }

            Rectangle {
                visible: videoInfo !== null
                color: Kirigami.Theme.highlightColor
                opacity: 0.15
                radius: 4
                Layout.preferredHeight: 30
                Layout.preferredWidth: badgeLayout.implicitWidth + 16

                RowLayout {
                    id: badgeLayout
                    anchors.centerIn: parent
                    spacing: 8
                    Kirigami.Icon {
                        source: "video-x-generic"
                        implicitWidth: 16
                        implicitHeight: 16
                    }
                    Label {
                        text: videoInfo ? (videoInfo.filename + " (" + videoInfo.width + "x" + videoInfo.height + ", " + videoInfo.fps.toFixed(0) + " fps, " + videoInfo.duration_secs.toFixed(1) + "s)") : ""
                        font.bold: true
                    }
                }
            }

            Item { Layout.fillWidth: true }

            ComboBox {
                id: langSelector
                model: i18n.availableLanguages()
                textRole: "name"
                currentIndex: {
                    for (var i = 0; i < model.length; i++) {
                        if (model[i].code === i18n.currentLanguage) return i;
                    }
                    return 0;
                }
                onActivated: {
                    i18n.setLanguage(model[currentIndex].code);
                }
            }

            Button {
                id: renderBtn
                text: appWindow.activeRenderCount > 0 ? (i18n.t("Render (%1 in queue | %2%)").replace("%1", appWindow.activeRenderCount).replace("%2", appWindow.activeRenderPercent.toFixed(0))) : i18n.t("Render")
                icon.name: appWindow.activeRenderCount > 0 ? "system-run" : "media-record"
                highlighted: true
                onClicked: {
                    renderDialog.open();
                }
            }
        }
    }

    // Main Body: If no video is loaded, show welcoming selection screen; otherwise show full effects workspace!
    pageStack.initialPage: Kirigami.Page {
        title: ""
        globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

        // Video Selection View
        Item {
            anchors.fill: parent
            visible: videoInfo === null

            Kirigami.Card {
                anchors.centerIn: parent
                implicitWidth: Math.min(parent.width - 40, 600)

                header: RowLayout {
                    spacing: Kirigami.Units.mediumSpacing
                    Kirigami.Icon {
                        source: "video-x-generic"
                        implicitWidth: 48
                        implicitHeight: 48
                    }
                    ColumnLayout {
                        Kirigami.Heading {
                            level: 2
                            text: i18n.t("Import a Video to Apply G'MIC Effects")
                        }
                        Label {
                            text: i18n.t("Select a video to start adding animated G'MIC effects with custom tweening, easing and curves.")
                            opacity: 0.7
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }
                    }
                }

                actions: [
                    Kirigami.Action {
                        text: i18n.t("Import Video File...")
                        icon.name: "document-open"
                        onTriggered: {
                            var f = Backend.chooseVideoFile();
                            if (f) loadVideo(f);
                        }
                    },
                    Kirigami.Action {
                        text: i18n.t("Load Sample: Original.mp4")
                        icon.name: "media-playback-start"
                        onTriggered: {
                            loadVideo("/home/isaques/Code/Projetos/Python/ultrawarp-batch/Exemplos de Videos/Original.mp4");
                        }
                    }
                ]
            }
        }

        // Active Workspace View (3 columns: Categories/Effects | Parameters & Tweens | Preview & Timeline)
        SplitView {
            id: mainWorkspaceSplit
            anchors.fill: parent
            visible: videoInfo !== null
            orientation: Qt.Horizontal

            handle: Rectangle {
                implicitWidth: 6
                color: SplitHandle.pressed ? Kirigami.Theme.highlightColor : (SplitHandle.hovered ? Kirigami.Theme.hoverColor : Kirigami.Theme.separatorColor)
                opacity: SplitHandle.hovered || SplitHandle.pressed ? 1.0 : 0.35

                Behavior on opacity {
                    NumberAnimation { duration: 150 }
                }

                Rectangle {
                    anchors.centerIn: parent
                    width: 2
                    height: 28
                    radius: 1
                    color: SplitHandle.pressed ? "white" : Kirigami.Theme.textColor
                    opacity: 0.6
                }
            }

            // Left Column: Filter Search & Categories
            Rectangle {
                id: leftColumnRect
                SplitView.preferredWidth: 260
                SplitView.minimumWidth: 180
                SplitView.maximumWidth: 420
                color: palette.window
                border.color: palette.mid
                border.width: 1
                radius: 4
                clip: true

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Kirigami.Units.smallSpacing
                    spacing: Kirigami.Units.smallSpacing

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 4

                        Kirigami.SearchField {
                            id: effectSearchField
                            Layout.fillWidth: true
                            placeholderText: i18n.t("Search effects...")
                            onTextChanged: appWindow.searchQuery = text.toLowerCase()
                        }

                        Button {
                            id: visualSearchBtn
                            icon.name: "view-preview"
                            text: i18n.t("Visual")
                            highlighted: true
                            onClicked: visualSearchWindow.open()
                        }
                    }

                    Label {
                        text: i18n.t("Category:")
                        font.bold: true
                        opacity: 0.8
                    }

                    ComboBox {
                        id: categoryComboBox
                        Layout.fillWidth: true
                        model: appWindow.allCategories
                        onCurrentTextChanged: appWindow.selectedCategory = currentText
                    }

                    Label {
                        text: i18n.t("Effects:")
                        font.bold: true
                        opacity: 0.8
                    }

                    ListView {
                        id: filterListView
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        model: {
                            var list = [];
                            var q = appWindow.searchQuery;
                            var cat = appWindow.selectedCategory;
                            for (var i = 0; i < appWindow.allFilters.length; i++) {
                                var f = appWindow.allFilters[i];
                                var matchCat = (cat === "All Categories" || f.category === cat);
                                var matchQuery = (!q || f.name.toLowerCase().indexOf(q) !== -1 || f.command.toLowerCase().indexOf(q) !== -1);
                                if (matchCat && matchQuery) {
                                    list.push(f);
                                }
                            }
                            return list;
                        }

                        delegate: ItemDelegate {
                            width: filterListView.width
                            highlighted: selectedFilter && selectedFilter.id === modelData.id

                            contentItem: ColumnLayout {
                                spacing: 2
                                Label {
                                    text: modelData.name
                                    font.bold: highlighted
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                                Label {
                                    text: modelData.category + " • " + modelData.command
                                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                                    opacity: 0.6
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                            }

                            onClicked: selectFilter(modelData)
                        }
                    }
                }
            }

            // Center Column: Selected Effect Parameters & Tweening Options
            Rectangle {
                id: centerColumnRect
                SplitView.fillWidth: true
                SplitView.minimumWidth: 300
                color: palette.window
                border.color: palette.mid
                border.width: 1
                radius: 4
                clip: true

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Kirigami.Units.mediumSpacing
                    spacing: Kirigami.Units.smallSpacing

                    // Effect Header Row (Title)
                    RowLayout {
                        Layout.fillWidth: true
                        Kirigami.Heading {
                            level: 2
                            text: selectedFilter ? selectedFilter.name : i18n.t("Select an Effect")
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                        }
                    }

                    // Effect Actions Toolbar (Save, Load, Reset Defaults)
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing

                        Button {
                            text: i18n.t("Save .geffect...")
                            icon.name: "document-save"
                            enabled: selectedFilter !== null
                            onClicked: {
                                var sName = selectedFilter ? (selectedFilter.id + ".geffect") : "preset.geffect";
                                var savedPath = Backend.savePresetFile(
                                    selectedFilter.id,
                                    selectedFilter.name,
                                    JSON.stringify(appWindow.paramValues),
                                    JSON.stringify(appWindow.paramTweens),
                                    sName
                                );
                                if (savedPath) {
                                    showPassiveNotification(i18n.t("Saved preset to: %1").replace("%1", savedPath), "short");
                                }
                            }
                        }

                        Button {
                            text: i18n.t("Load .geffect...")
                            icon.name: "document-open"
                            onClicked: {
                                var resJson = Backend.loadPresetFile();
                                if (!resJson) return;
                                try {
                                    var res = JSON.parse(resJson);
                                    if (res.error) {
                                        showPassiveNotification(i18n.t("Failed to load preset: %1").replace("%1", res.error), "long");
                                        return;
                                    }
                                    // Find filter
                                    var fId = res.filter_id;
                                    var targetFilter = null;
                                    for (var i = 0; i < allFilters.length; i++) {
                                        if (allFilters[i].id === fId) {
                                            targetFilter = allFilters[i];
                                            break;
                                        }
                                    }
                                    if (!targetFilter) {
                                        showPassiveNotification(i18n.t("Preset requires filter '%1' which is not found!").replace("%1", fId), "long");
                                        return;
                                    }
                                    selectedFilter = targetFilter;
                                    var parsedValues = {};
                                    for (var k in res.base_values) {
                                        parsedValues[parseInt(k)] = res.base_values[k];
                                    }
                                    var parsedTweens = {};
                                    for (var tk in res.tweens) {
                                        parsedTweens[parseInt(tk)] = res.tweens[tk];
                                    }
                                    paramValues = parsedValues;
                                    paramTweens = parsedTweens;
                                    requestPreviewUpdate();
                                    showPassiveNotification(i18n.t("Loaded preset: %1").replace("%1", (res.filter_name || fId)), "short");
                                } catch (e) {
                                    showPassiveNotification(i18n.t("Preset parse error: %1").replace("%1", e), "long");
                                }
                            }
                        }

                        Button {
                            text: i18n.t("Reset Defaults")
                            icon.name: "edit-undo"
                            onClicked: {
                                if (selectedFilter) selectFilter(selectedFilter);
                            }
                        }

                        Item { Layout.fillWidth: true }
                    }

                    Label {
                        text: selectedFilter && selectedFilter.description ? selectedFilter.description : i18n.t("Customize effect parameters and tween values over the video duration.")
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        opacity: 0.7
                    }

                    Kirigami.Separator {
                        Layout.fillWidth: true
                    }

                    // Parameters List
                    ScrollView {
                        id: paramsScrollView
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                        ScrollBar.vertical.policy: ScrollBar.AsNeeded

                        ColumnLayout {
                            id: paramsCol
                            width: paramsScrollView.availableWidth - 8
                            spacing: Kirigami.Units.mediumSpacing

                            Repeater {
                                model: selectedFilter ? selectedFilter.parameters : []

                                delegate: ColumnLayout {
                                    id: paramDelegate
                                    Layout.fillWidth: true
                                    width: paramsCol.width
                                    spacing: Kirigami.Units.smallSpacing

                                    property var p: modelData
                                    property var ptype: p.param_type.type
                                    property var pdata: p.param_type.data
                                    property var curTween: appWindow.paramTweens[p.id] || {}
                                    property string animMode: curTween.mode ? curTween.mode : (curTween.enabled ? "curve" : "disabled")

                                    visible: ptype !== "separator" && ptype !== "note"

                                    // Parameter Header Row
                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: Kirigami.Units.smallSpacing

                                        Label {
                                            text: p.name
                                            font.bold: true
                                            Layout.fillWidth: true
                                            elide: Text.ElideRight
                                        }

                                        // Current evaluated value badge
                                        Label {
                                            text: {
                                                if (appWindow.paramValues && appWindow.paramValues[p.id] !== undefined) {
                                                    var val = appWindow.paramValues[p.id];
                                                    return ptype === "float" ? val.toFixed(2) : Math.round(val);
                                                }
                                                return "";
                                            }
                                            opacity: 0.7
                                            visible: ptype === "float" || ptype === "int"
                                        }

                                        // Animate Dropdown Button (Disabled, Random, Curve)
                                        Button {
                                            id: animMenuBtn
                                            visible: ptype === "float" || ptype === "int"
                                            text: animMode === "random" ? i18n.t("Animate: Random") : (animMode === "curve" ? i18n.t("Animate: Curve") : i18n.t("Animate"))
                                            icon.name: animMode === "random" ? "roll" : (animMode === "curve" ? "draw-bezier-curves" : "media-playback-start")
                                            highlighted: animMode !== "disabled"
                                            onClicked: animMenu.open()

                                            Menu {
                                                id: animMenu
                                                y: animMenuBtn.height

                                                MenuItem {
                                                    text: i18n.t("Disabled (Static Value)")
                                                    icon.name: "dialog-cancel"
                                                    checked: animMode === "disabled"
                                                    onTriggered: {
                                                        var tws = Object.assign({}, appWindow.paramTweens);
                                                        var cur = Object.assign({}, tws[p.id] || {});
                                                        cur.mode = "disabled";
                                                        cur.enabled = false;
                                                        tws[p.id] = cur;
                                                        appWindow.paramTweens = tws;
                                                        requestPreviewUpdate();
                                                    }
                                                }

                                                MenuItem {
                                                    text: i18n.t("Random (New Value Every Frame)")
                                                    icon.name: "roll"
                                                    checked: animMode === "random"
                                                    onTriggered: {
                                                        var tws = Object.assign({}, appWindow.paramTweens);
                                                        var cur = Object.assign({}, tws[p.id] || {});
                                                        cur.mode = "random";
                                                        cur.enabled = true;
                                                        if (cur.start_value === undefined) cur.start_value = (pdata.min !== undefined ? pdata.min : 0.0);
                                                        if (cur.end_value === undefined) cur.end_value = (pdata.max !== undefined ? pdata.max : 100.0);
                                                        tws[p.id] = cur;
                                                        appWindow.paramTweens = tws;
                                                        requestPreviewUpdate();
                                                    }
                                                }

                                                MenuItem {
                                                    text: i18n.t("Curve (Keyframe & Ease Curve)")
                                                    icon.name: "draw-bezier-curves"
                                                    checked: animMode === "curve"
                                                    onTriggered: {
                                                        var tws = Object.assign({}, appWindow.paramTweens);
                                                        var cur = Object.assign({}, tws[p.id] || {});
                                                        cur.mode = "curve";
                                                        cur.enabled = true;
                                                        if (cur.start_value === undefined) cur.start_value = (pdata.min !== undefined ? pdata.min : 0.0);
                                                        if (cur.end_value === undefined) cur.end_value = (pdata.max !== undefined ? pdata.max : 100.0);
                                                        if (!cur.ease_type) cur.ease_type = "linear";
                                                        if (!cur.bezier_points) cur.bezier_points = [0.25, 0.25, 0.75, 0.75];
                                                        tws[p.id] = cur;
                                                        appWindow.paramTweens = tws;
                                                        requestPreviewUpdate();
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Normal control (when not tweening or base value)
                                    RowLayout {
                                        Layout.fillWidth: true
                                        visible: animMode === "disabled"

                                        // Float control
                                        Slider {
                                            visible: ptype === "float"
                                            Layout.fillWidth: true
                                            from: pdata.min !== undefined ? pdata.min : 0
                                            to: pdata.max !== undefined ? pdata.max : 100
                                            value: appWindow.paramValues[p.id] !== undefined ? appWindow.paramValues[p.id] : (pdata.default || 0)
                                            onMoved: {
                                                var vals = Object.assign({}, appWindow.paramValues);
                                                vals[p.id] = value;
                                                appWindow.paramValues = vals;
                                                requestPreviewUpdate();
                                            }
                                        }

                                        // Int control
                                        Slider {
                                            visible: ptype === "int"
                                            Layout.fillWidth: true
                                            from: pdata.min !== undefined ? pdata.min : 0
                                            to: pdata.max !== undefined ? pdata.max : 100
                                            stepSize: 1
                                            value: appWindow.paramValues[p.id] !== undefined ? appWindow.paramValues[p.id] : (pdata.default || 0)
                                            onMoved: {
                                                var vals = Object.assign({}, appWindow.paramValues);
                                                vals[p.id] = Math.round(value);
                                                appWindow.paramValues = vals;
                                                requestPreviewUpdate();
                                            }
                                        }

                                        // Choice control
                                        ComboBox {
                                            visible: ptype === "choice"
                                            Layout.fillWidth: true
                                            model: pdata.options || []
                                            currentIndex: appWindow.paramValues[p.id] !== undefined ? Math.round(appWindow.paramValues[p.id]) : (pdata.default || 0)
                                            onActivated: {
                                                var vals = Object.assign({}, appWindow.paramValues);
                                                vals[p.id] = currentIndex;
                                                appWindow.paramValues = vals;
                                                requestPreviewUpdate();
                                            }
                                        }

                                        // Bool control
                                        Switch {
                                            visible: ptype === "bool"
                                            checked: (appWindow.paramValues && appWindow.paramValues[p.id] !== undefined) ? (appWindow.paramValues[p.id] >= 0.5) : (pdata.default ? true : false)
                                            onToggled: {
                                                var vals = Object.assign({}, appWindow.paramValues);
                                                vals[p.id] = checked ? 1.0 : 0.0;
                                                appWindow.paramValues = vals;
                                                requestPreviewUpdate();
                                            }
                                        }
                                    }

                                    // Embedded Inline Curve Editor (appears only when Animation Mode is Curve)
                                    Rectangle {
                                        visible: animMode === "curve"
                                        Layout.fillWidth: true
                                        implicitHeight: inlineCurve.implicitHeight + 16
                                        color: Kirigami.Theme.backgroundColor
                                        border.color: Kirigami.Theme.highlightColor
                                        border.width: 1
                                        radius: 4

                                        InlineCurveEditor {
                                            id: inlineCurve
                                            anchors.fill: parent
                                            anchors.margins: 8

                                            paramMin: pdata.min !== undefined ? pdata.min : 0.0
                                            paramMax: pdata.max !== undefined ? pdata.max : 100.0
                                            currentStart: curTween.start_value !== undefined ? curTween.start_value : paramMin
                                            currentEnd: curTween.end_value !== undefined ? curTween.end_value : paramMax
                                            currentEase: curTween.ease_type || "linear"
                                            pX1: curTween.bezier_points ? curTween.bezier_points[0] : 0.25
                                            pY1: curTween.bezier_points ? curTween.bezier_points[1] : 0.25
                                            pX2: curTween.bezier_points ? curTween.bezier_points[2] : 0.75
                                            pY2: curTween.bezier_points ? curTween.bezier_points[3] : 0.75

                                            onCurveChanged: function(sVal, eVal, eType, x1, y1, x2, y2) {
                                                var tws = Object.assign({}, appWindow.paramTweens);
                                                var cur = Object.assign({}, tws[p.id] || {});
                                                cur.mode = "curve";
                                                cur.enabled = true;
                                                cur.start_value = sVal;
                                                cur.end_value = eVal;
                                                cur.ease_type = eType;
                                                cur.bezier_points = [x1, y1, x2, y2];
                                                tws[p.id] = cur;
                                                appWindow.paramTweens = tws;
                                                requestPreviewUpdate();
                                            }
                                        }
                                    }

                                    // Random Mode Info Card (appears when Animation Mode is Random)
                                    Rectangle {
                                        visible: animMode === "random"
                                        Layout.fillWidth: true
                                        implicitHeight: randomLayout.implicitHeight + 14
                                        color: Kirigami.Theme.backgroundColor
                                        border.color: "#9b59b6"
                                        border.width: 1
                                        radius: 4

                                        RowLayout {
                                            id: randomLayout
                                            anchors.fill: parent
                                            anchors.margins: 8
                                            spacing: 8

                                            Kirigami.Icon {
                                                source: "roll"
                                                implicitWidth: 16
                                                implicitHeight: 16
                                            }

                                            Label {
                                                text: i18n.t("Random Mode Active: Generating pseudo-random values between %1 and %2 on every video frame.")
                                                    .replace("%1", (pdata.min !== undefined ? pdata.min : 0))
                                                    .replace("%2", (pdata.max !== undefined ? pdata.max : 100))
                                                font.pixelSize: 11
                                                opacity: 0.85
                                                wrapMode: Text.WordWrap
                                                Layout.fillWidth: true
                                            }
                                        }
                                    }

                                    Kirigami.Separator {
                                        Layout.fillWidth: true
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Right Column: Video Preview & Timeline Scrubber
            Rectangle {
                id: rightColumnRect
                SplitView.preferredWidth: 460
                SplitView.minimumWidth: 280
                color: palette.window
                border.color: palette.mid
                border.width: 1
                radius: 4
                clip: true

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Kirigami.Units.mediumSpacing
                    spacing: Kirigami.Units.smallSpacing

                    // Preview Controls Bar
                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            text: i18n.t("Preview")
                            font.bold: true
                            Layout.fillWidth: true
                        }

                        Button {
                            text: appWindow.splitPreview ? i18n.t("Split View") : i18n.t("Full View")
                            icon.name: appWindow.splitPreview ? "view-split-left-right" : "view-fullscreen"
                            checkable: true
                            checked: appWindow.splitPreview
                            onClicked: appWindow.splitPreview = !appWindow.splitPreview
                        }

                        Button {
                            icon.name: "view-refresh"
                            ToolTip.text: i18n.t("Refresh Preview")
                            onClicked: requestPreviewUpdate()
                        }
                    }

                    // Video Frame Display Area
                    Rectangle {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        color: "#0a0a0a"
                        radius: 4
                        clip: true

                        // Full effect preview
                        Image {
                            anchors.fill: parent
                            visible: !appWindow.splitPreview
                            source: appWindow.previewEffectPath
                            fillMode: Image.PreserveAspectFit
                            cache: false
                        }

                        // Split view preview (Side-by-side)
                        Row {
                            anchors.fill: parent
                            visible: appWindow.splitPreview

                            Image {
                                width: parent.width / 2
                                height: parent.height
                                source: appWindow.previewRawPath
                                fillMode: Image.PreserveAspectFit
                                cache: false
                                Label {
                                    anchors.bottom: parent.bottom
                                    anchors.left: parent.left
                                    anchors.margins: 8
                                    text: i18n.t("Original")
                                    color: "white"
                                    font.bold: true
                                    background: Rectangle { color: "#80000000"; radius: 3 }
                                }
                            }

                            Rectangle {
                                width: 2
                                height: parent.height
                                color: Kirigami.Theme.highlightColor
                            }

                            Image {
                                width: parent.width / 2 - 2
                                height: parent.height
                                source: appWindow.previewEffectPath
                                fillMode: Image.PreserveAspectFit
                                cache: false
                                Label {
                                    anchors.bottom: parent.bottom
                                    anchors.right: parent.right
                                    anchors.margins: 8
                                    text: i18n.t("G'MIC Effect")
                                    color: "white"
                                    font.bold: true
                                    background: Rectangle { color: "#80000000"; radius: 3 }
                                }
                            }
                        }

                        // Loading spinner
                        BusyIndicator {
                            anchors.centerIn: parent
                            running: appWindow.isPreviewLoading
                        }
                    }

                    // Scrubber Timeline Controls
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 4

                        RowLayout {
                            Layout.fillWidth: true
                            ToolButton {
                                id: playBtn
                                icon.name: scrubTimer.running ? "media-playback-pause" : "media-playback-start"
                                onClicked: {
                                    if (scrubTimer.running) {
                                        scrubTimer.stop();
                                    } else {
                                        scrubTimer.start();
                                    }
                                }
                            }

                            Slider {
                                id: timeScrubber
                                Layout.fillWidth: true
                                from: 0.0
                                to: videoInfo ? videoInfo.duration_secs : 1.0
                                value: appWindow.currentScrubTime
                                onMoved: {
                                    appWindow.currentScrubTime = value;
                                    requestPreviewUpdate();
                                }
                            }
                        }

                        Timer {
                            id: scrubTimer
                            interval: 500
                            repeat: true
                            onTriggered: {
                                if (!videoInfo) return;
                                var step = 1.0;
                                var nextT = appWindow.currentScrubTime + step;
                                if (nextT > videoInfo.duration_secs) {
                                    nextT = 0.0;
                                }
                                appWindow.currentScrubTime = nextT;
                                requestPreviewUpdate();
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            Label {
                                text: {
                                    if (!videoInfo) return "00:00.00";
                                    var curFrame = Math.round(appWindow.currentScrubTime * videoInfo.fps);
                                    return i18n.t("Frame: %1 / %2").replace("%1", curFrame).replace("%2", videoInfo.frame_count);
                                }
                                font.pointSize: Kirigami.Theme.smallFont.pointSize
                                opacity: 0.8
                            }
                            Item { Layout.fillWidth: true }
                            Label {
                                text: {
                                    if (!videoInfo) return "00:00.00 / 00:00.00";
                                    return appWindow.currentScrubTime.toFixed(2) + "s / " + videoInfo.duration_secs.toFixed(2) + "s";
                                }
                                font.pointSize: Kirigami.Theme.smallFont.pointSize
                                font.bold: true
                            }
                        }

                        Kirigami.Separator {
                            Layout.fillWidth: true
                            visible: videoInfo !== null
                        }

                        // Video Cut Range (In / Out points)
                        ColumnLayout {
                            Layout.fillWidth: true
                            visible: videoInfo !== null
                            spacing: 4

                            RowLayout {
                                Layout.fillWidth: true
                                Kirigami.Icon {
                                    source: "edit-cut"
                                    implicitWidth: 16
                                    implicitHeight: 16
                                }
                                Label {
                                    text: i18n.t("Cut Range (Trim Video):")
                                    font.bold: true
                                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                                }
                                Item { Layout.fillWidth: true }
                                Label {
                                    text: {
                                        if (!videoInfo) return "";
                                        var dur = (appWindow.cutEndTime - appWindow.cutStartTime).toFixed(2);
                                        return i18n.t("Cut Range: %1s -> %2s (Duration: %3s)").replace("%1", appWindow.cutStartTime.toFixed(2)).replace("%2", appWindow.cutEndTime.toFixed(2)).replace("%3", dur);
                                    }
                                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                                    color: (appWindow.cutStartTime > 0.05 || (videoInfo && appWindow.cutEndTime < videoInfo.duration_secs - 0.05)) ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
                                    font.bold: true
                                }
                                ToolButton {
                                    icon.name: "edit-undo"
                                    ToolTip.text: i18n.t("Reset Cut to Full Video")
                                    visible: videoInfo && (appWindow.cutStartTime > 0.05 || appWindow.cutEndTime < videoInfo.duration_secs - 0.05)
                                    onClicked: {
                                        appWindow.cutStartTime = 0.0;
                                        appWindow.cutEndTime = videoInfo.duration_secs;
                                        cutRangeSlider.first.value = 0.0;
                                        cutRangeSlider.second.value = videoInfo.duration_secs;
                                    }
                                }
                            }

                            RangeSlider {
                                id: cutRangeSlider
                                Layout.fillWidth: true
                                from: 0.0
                                to: videoInfo ? videoInfo.duration_secs : 1.0
                                first.value: appWindow.cutStartTime
                                second.value: videoInfo ? (appWindow.cutEndTime > 0 ? appWindow.cutEndTime : videoInfo.duration_secs) : 1.0
                                stepSize: videoInfo && videoInfo.fps > 0 ? (1.0 / videoInfo.fps) : 0.05

                                first.onMoved: {
                                    appWindow.cutStartTime = first.value;
                                    if (appWindow.currentScrubTime < first.value) {
                                        appWindow.currentScrubTime = first.value;
                                        requestPreviewUpdate();
                                    }
                                }

                                second.onMoved: {
                                    appWindow.cutEndTime = second.value;
                                    if (appWindow.currentScrubTime > second.value) {
                                        appWindow.currentScrubTime = second.value;
                                        requestPreviewUpdate();
                                    }
                                }
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                Label {
                                    text: i18n.t("Start: %1s").replace("%1", appWindow.cutStartTime.toFixed(2))
                                    font.pixelSize: 11
                                    opacity: 0.75
                                }
                                Item { Layout.fillWidth: true }
                                Label {
                                    text: i18n.t("End: %1s").replace("%1", appWindow.cutEndTime.toFixed(2))
                                    font.pixelSize: 11
                                    opacity: 0.75
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
