import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Window {
    id: visualSearchWin
    title: i18n.t("Visual Effect Search - G'MIC Video")
    width: 1040
    height: 720
    minimumWidth: 680
    minimumHeight: 480
    visible: false
    color: Kirigami.Theme.backgroundColor

    function open() {
        show();
        raise();
        requestActivate();
        searchField.forceActiveFocus();
    }

    property var allFilters: []
    property var allCategories: ["All Categories"]
    property string currentSearchText: ""
    property string selectedCategory: "All Categories"

    signal effectSelected(var filter)

    // Filtered list of effects based on search query and category
    // Guarantees all displayed effects have thumbnails loaded
    // Allows searching by either English name or translated name, as well as command and category
    property var filteredFilters: {
        var query = currentSearchText.toLowerCase().trim();
        var cat = selectedCategory;
        var result = [];

        for (var i = 0; i < allFilters.length; i++) {
            var f = allFilters[i];

            // Visual search is strictly visual: only show effects with valid preview thumbnails
            if (!f.preview_path || f.preview_path === "") {
                continue;
            }

            // Category filter
            if (cat !== "All Categories" && f.category !== cat && i18n.t(f.category) !== cat) {
                continue;
            }

            // Search query filter: matches both translated name and English name, plus command & category
            if (query !== "") {
                var transName = (i18n.t(f.name) || "").toLowerCase();
                var origName = (f.name || "").toLowerCase();
                var catTrans = (i18n.t(f.category) || "").toLowerCase();
                var catOrig = (f.category || "").toLowerCase();
                var cmd = (f.command || "").toLowerCase();

                var nameMatch = transName.indexOf(query) !== -1 || origName.indexOf(query) !== -1;
                var catMatch = catTrans.indexOf(query) !== -1 || catOrig.indexOf(query) !== -1;
                var cmdMatch = cmd.indexOf(query) !== -1;
                if (!nameMatch && !catMatch && !cmdMatch) {
                    continue;
                }
            }

            result.push(f);
        }
        return result;
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        spacing: Kirigami.Units.mediumSpacing

        // Top Search & Controls Bar
        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Kirigami.SearchField {
                id: searchField
                Layout.fillWidth: true
                placeholderText: i18n.t("Search effects visually (e.g. bokeh, warp, glitch, blur, halftone)...")
                onTextChanged: visualSearchWin.currentSearchText = text
            }

            ComboBox {
                id: catCombo
                model: {
                    var list = [];
                    for (var i = 0; i < visualSearchWin.allCategories.length; i++) {
                        list.push(i18n.t(visualSearchWin.allCategories[i]));
                    }
                    return list;
                }
                currentIndex: {
                    for (var i = 0; i < visualSearchWin.allCategories.length; i++) {
                        if (visualSearchWin.allCategories[i] === visualSearchWin.selectedCategory) return i;
                    }
                    return 0;
                }
                onActivated: {
                    if (currentIndex >= 0 && currentIndex < visualSearchWin.allCategories.length) {
                        visualSearchWin.selectedCategory = visualSearchWin.allCategories[currentIndex];
                    }
                }
            }

            Button {
                text: i18n.t("Close")
                icon.name: "window-close"
                onClicked: visualSearchWin.close()
            }
        }

        // Status Header / Summary Bar
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: visualSearchWin.selectedCategory !== "All Categories" ?
                      i18n.t("Showing %1 effects in %2").replace("%1", visualSearchWin.filteredFilters.length).replace("%2", i18n.t(visualSearchWin.selectedCategory)) :
                      i18n.t("Showing %1 effects").replace("%1", visualSearchWin.filteredFilters.length)
                font.bold: true
                font.pixelSize: 11
                opacity: 0.75
            }
            Item { Layout.fillWidth: true }
            Label {
                text: i18n.t("Click any effect card to apply it to your video")
                font.pixelSize: 11
                opacity: 0.5
            }
        }

        Kirigami.Separator { Layout.fillWidth: true }

        // Grid View of Effects
        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

            GridView {
                id: grid
                width: parent.width
                cellWidth: Math.floor(width / Math.max(1, Math.floor(width / 200)))
                cellHeight: 220
                model: visualSearchWin.filteredFilters

                delegate: Item {
                    width: grid.cellWidth
                    height: grid.cellHeight

                    Rectangle {
                        id: card
                        anchors.fill: parent
                        anchors.margins: 5
                        radius: 6
                        color: cardMouseArea.containsMouse ? Kirigami.Theme.highlightColor : Kirigami.Theme.backgroundColor
                        border.color: cardMouseArea.containsMouse ? Kirigami.Theme.highlightColor : Kirigami.Theme.separatorColor
                        border.width: cardMouseArea.containsMouse ? 2 : 1

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: 6
                            spacing: 4

                            // Preview Image Container
                            Rectangle {
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                radius: 4
                                clip: true
                                color: "#141414"

                                Image {
                                    id: previewImg
                                    anchors.fill: parent
                                    source: modelData.preview_path || ""
                                    asynchronous: true
                                    sourceSize: Qt.size(240, 240)
                                    fillMode: Image.PreserveAspectCrop
                                    visible: modelData.preview_path !== undefined && modelData.preview_path !== ""
                                }

                                // Placeholder if no preview image
                                ColumnLayout {
                                    anchors.centerIn: parent
                                    visible: !previewImg.visible
                                    spacing: 4
                                    Kirigami.Icon {
                                        source: "color-management"
                                        implicitWidth: 32
                                        implicitHeight: 32
                                        Layout.alignment: Qt.AlignHCenter
                                        opacity: 0.35
                                    }
                                    Label {
                                        text: i18n.t("No preview")
                                        font.pixelSize: 10
                                        opacity: 0.4
                                        Layout.alignment: Qt.AlignHCenter
                                    }
                                }

                                // Category tag badge
                                Rectangle {
                                    anchors.top: parent.top
                                    anchors.left: parent.left
                                    anchors.margins: 4
                                    color: "#b0000000"
                                    radius: 3
                                    implicitWidth: catBadge.implicitWidth + 8
                                    implicitHeight: catBadge.implicitHeight + 4

                                    Label {
                                        id: catBadge
                                        anchors.centerIn: parent
                                        text: modelData.category ? i18n.t(modelData.category) : ""
                                        font.pixelSize: 9
                                        font.bold: true
                                        color: "white"
                                        elide: Text.ElideRight
                                    }
                                }
                            }

                            // Effect Title and Info
                            Label {
                                Layout.fillWidth: true
                                text: modelData.name ? i18n.t(modelData.name) : i18n.t("Effect")
                                font.bold: true
                                font.pixelSize: 12
                                elide: Text.ElideRight
                                maximumLineCount: 2
                                wrapMode: Text.WordWrap
                                color: cardMouseArea.containsMouse ? "white" : Kirigami.Theme.textColor
                            }

                            Label {
                                Layout.fillWidth: true
                                text: modelData.command ? ("gmic " + modelData.command) : ""
                                font.pixelSize: 10
                                opacity: 0.6
                                elide: Text.ElideRight
                                color: cardMouseArea.containsMouse ? "#eeeeee" : Kirigami.Theme.textColor
                            }
                        }

                        MouseArea {
                            id: cardMouseArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                visualSearchWin.effectSelected(modelData);
                                visualSearchWin.close();
                            }
                        }
                    }
                }
            }
        }
    }
}
