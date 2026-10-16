import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Window {
    id: visualSearchWin
    title: "Visual Effect Search - G'MIC Video"
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
    property bool onlyWithPreview: false

    signal effectSelected(var filter)

    // Filtered list of effects based on search query, category, and preview availability
    property var filteredFilters: {
        var query = currentSearchText.toLowerCase().trim();
        var cat = selectedCategory;
        var onlyPrev = onlyWithPreview;
        var result = [];

        for (var i = 0; i < allFilters.length; i++) {
            var f = allFilters[i];

            // Category filter
            if (cat !== "All Categories" && f.category !== cat) {
                continue;
            }

            // Preview filter
            if (onlyPrev && (!f.preview_path || f.preview_path === "")) {
                continue;
            }

            // Search query filter
            if (query !== "") {
                var nameMatch = f.name && f.name.toLowerCase().indexOf(query) !== -1;
                var catMatch = f.category && f.category.toLowerCase().indexOf(query) !== -1;
                var cmdMatch = f.command && f.command.toLowerCase().indexOf(query) !== -1;
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
                placeholderText: "Search effects visually (e.g. bokeh, warp, glitch, blur, halftone)..."
                onTextChanged: visualSearchWin.currentSearchText = text
            }

            ComboBox {
                id: catCombo
                model: visualSearchWin.allCategories
                currentIndex: 0
                onActivated: visualSearchWin.selectedCategory = model[currentIndex]
            }

            CheckBox {
                id: previewToggle
                text: "With Previews Only"
                checked: visualSearchWin.onlyWithPreview
                onToggled: visualSearchWin.onlyWithPreview = checked
            }

            Button {
                text: "Close"
                icon.name: "window-close"
                onClicked: visualSearchWin.close()
            }
        }

        // Status Header / Summary Bar
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: "Showing " + visualSearchWin.filteredFilters.length + " effects" + 
                      (visualSearchWin.selectedCategory !== "All Categories" ? (" in " + visualSearchWin.selectedCategory) : "")
                font.bold: true
                font.pixelSize: 11
                opacity: 0.75
            }
            Item { Layout.fillWidth: true }
            Label {
                text: "Click any effect card to apply it to your video"
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
                                        text: "No preview"
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
                                        text: modelData.category || ""
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
                                text: modelData.name || "Effect"
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
