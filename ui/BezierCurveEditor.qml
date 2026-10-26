import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.OverlaySheet {
    id: curveSheet
    title: i18n.t("Curve Transition Editor")

    property real x1: 0.25
    property real y1: 0.1
    property real x2: 0.25
    property real y2: 1.0
    property string paramName: "Parameter"

    signal curveApplied(real pX1, real pY1, real pX2, real pY2)

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        implicitWidth: 480

        Kirigami.Heading {
            level: 3
            text: i18n.t("Cubic Bezier Transition for %1").replace("%1", curveSheet.paramName)
        }

        Label {
            text: i18n.t("Drag the control points P1 and P2 or choose a preset to sculpt value transitions over video duration.")
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
            opacity: 0.8
        }

        // Preset buttons
        RowLayout {
            spacing: Kirigami.Units.smallSpacing
            Layout.fillWidth: true

            Label { text: i18n.t("Presets:") }

            Button {
                text: i18n.t("Ease")
                onClicked: { curveSheet.x1 = 0.25; curveSheet.y1 = 0.1; curveSheet.x2 = 0.25; curveSheet.y2 = 1.0; canvas.requestPaint(); }
            }
            Button {
                text: i18n.t("Ease In")
                onClicked: { curveSheet.x1 = 0.42; curveSheet.y1 = 0.0; curveSheet.x2 = 1.0; curveSheet.y2 = 1.0; canvas.requestPaint(); }
            }
            Button {
                text: i18n.t("Ease Out")
                onClicked: { curveSheet.x1 = 0.0; curveSheet.y1 = 0.0; curveSheet.x2 = 0.58; curveSheet.y2 = 1.0; canvas.requestPaint(); }
            }
            Button {
                text: i18n.t("Ease In-Out")
                onClicked: { curveSheet.x1 = 0.42; curveSheet.y1 = 0.0; curveSheet.x2 = 0.58; curveSheet.y2 = 1.0; canvas.requestPaint(); }
            }
            Button {
                text: i18n.t("Overshoot")
                onClicked: { curveSheet.x1 = 0.34; curveSheet.y1 = 1.56; curveSheet.x2 = 0.64; curveSheet.y2 = 1.0; canvas.requestPaint(); }
            }
        }

        // Interactive Canvas
        Rectangle {
            id: graphArea
            Layout.alignment: Qt.AlignHCenter
            width: 380
            height: 280
            color: "#181818"
            border.color: "#333333"
            border.width: 1
            radius: 4

            Canvas {
                id: canvas
                anchors.fill: parent
                anchors.margins: 25

                onPaint: {
                    var ctx = getContext("2d");
                    ctx.clearRect(0, 0, width, height);

                    // Grid lines
                    ctx.strokeStyle = "#2a2a2a";
                    ctx.lineWidth = 1;
                    for (var i = 0; i <= 4; i++) {
                        var gx = (width * i) / 4;
                        var gy = (height * i) / 4;
                        ctx.beginPath();
                        ctx.moveTo(gx, 0);
                        ctx.lineTo(gx, height);
                        ctx.stroke();

                        ctx.beginPath();
                        ctx.moveTo(0, gy);
                        ctx.lineTo(width, gy);
                        ctx.stroke();
                    }

                    // Diagonal linear reference
                    ctx.strokeStyle = "#444444";
                    ctx.setLineDash([4, 4]);
                    ctx.beginPath();
                    ctx.moveTo(0, height);
                    ctx.lineTo(width, 0);
                    ctx.stroke();
                    ctx.setLineDash([]);

                    // Control handles lines
                    var p0x = 0, p0y = height;
                    var p3x = width, p3y = 0;
                    var c1x = curveSheet.x1 * width;
                    var c1y = (1.0 - curveSheet.y1) * height;
                    var c2x = curveSheet.x2 * width;
                    var c2y = (1.0 - curveSheet.y2) * height;

                    ctx.strokeStyle = "#00bcd4";
                    ctx.lineWidth = 1.5;
                    ctx.beginPath();
                    ctx.moveTo(p0x, p0y);
                    ctx.lineTo(c1x, c1y);
                    ctx.stroke();

                    ctx.strokeStyle = "#ff9800";
                    ctx.beginPath();
                    ctx.moveTo(p3x, p3y);
                    ctx.lineTo(c2x, c2y);
                    ctx.stroke();

                    // Bezier curve
                    ctx.strokeStyle = "#3daee9"; // KDE Plasma breeze blue
                    ctx.lineWidth = 3.0;
                    ctx.beginPath();
                    ctx.moveTo(p0x, p0y);
                    ctx.bezierCurveTo(c1x, c1y, c2x, c2y, p3x, p3y);
                    ctx.stroke();
                }
            }

            // Draggable handle P1
            Rectangle {
                id: handle1
                width: 16
                height: 16
                radius: 8
                color: "#00bcd4"
                border.color: "#ffffff"
                border.width: 2
                x: canvas.x + curveSheet.x1 * canvas.width - width / 2
                y: canvas.y + (1.0 - curveSheet.y1) * canvas.height - height / 2

                MouseArea {
                    anchors.fill: parent
                    drag.target: parent
                    drag.axis: Drag.XAndYAxis
                    onPositionChanged: {
                        if (drag.active) {
                            var relX = (handle1.x + handle1.width / 2 - canvas.x) / canvas.width;
                            var relY = 1.0 - (handle1.y + handle1.height / 2 - canvas.y) / canvas.height;
                            curveSheet.x1 = Math.max(0.0, Math.min(1.0, relX));
                            curveSheet.y1 = Math.max(-0.5, Math.min(2.0, relY));
                            canvas.requestPaint();
                        }
                    }
                }
            }

            // Draggable handle P2
            Rectangle {
                id: handle2
                width: 16
                height: 16
                radius: 8
                color: "#ff9800"
                border.color: "#ffffff"
                border.width: 2
                x: canvas.x + curveSheet.x2 * canvas.width - width / 2
                y: canvas.y + (1.0 - curveSheet.y2) * canvas.height - height / 2

                MouseArea {
                    anchors.fill: parent
                    drag.target: parent
                    drag.axis: Drag.XAndYAxis
                    onPositionChanged: {
                        if (drag.active) {
                            var relX = (handle2.x + handle2.width / 2 - canvas.x) / canvas.width;
                            var relY = 1.0 - (handle2.y + handle2.height / 2 - canvas.y) / canvas.height;
                            curveSheet.x2 = Math.max(0.0, Math.min(1.0, relX));
                            curveSheet.y2 = Math.max(-0.5, Math.min(2.0, relY));
                            canvas.requestPaint();
                        }
                    }
                }
            }
        }

        // Coordinates display
        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: Kirigami.Units.largeSpacing

            RowLayout {
                spacing: 4
                Rectangle { width: 10; height: 10; radius: 5; color: "#00bcd4" }
                Label { text: "P1: (" + curveSheet.x1.toFixed(2) + ", " + curveSheet.y1.toFixed(2) + ")" }
            }

            RowLayout {
                spacing: 4
                Rectangle { width: 10; height: 10; radius: 5; color: "#ff9800" }
                Label { text: "P2: (" + curveSheet.x2.toFixed(2) + ", " + curveSheet.y2.toFixed(2) + ")" }
            }
        }

        // Action buttons
        RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }

            Button {
                text: i18n.t("Cancel")
                onClicked: curveSheet.close()
            }

            Button {
                text: i18n.t("Apply Curve")
                highlighted: true
                icon.name: "dialog-ok"
                onClicked: {
                    curveSheet.curveApplied(curveSheet.x1, curveSheet.y1, curveSheet.x2, curveSheet.y2);
                    curveSheet.close();
                }
            }
        }
    }
}
