import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: curveControl
    spacing: 6
    Layout.fillWidth: true

    property real paramMin: 0.0
    property real paramMax: 100.0
    property real currentStart: 0.0
    property real currentEnd: 100.0
    property string currentEase: "linear"
    property real pX1: 0.25
    property real pY1: 0.1
    property real pX2: 0.25
    property real pY2: 1.0

    signal curveChanged(real startVal, real endVal, string easeType, real x1, real y1, real x2, real y2)

    // Preset selection row
    RowLayout {
        Layout.fillWidth: true
        spacing: 6

        Label {
            text: "Curve Preset:"
            font.bold: true
            font.pixelSize: 11
            opacity: 0.85
        }

        ComboBox {
            id: presetCombo
            Layout.fillWidth: true
            model: [
                { key: "linear", label: "Linear (Default)" },
                { key: "ease_in_quad", label: "Ease In (Quad)" },
                { key: "ease_out_quad", label: "Ease Out (Quad)" },
                { key: "ease_in_out_quad", label: "Ease In-Out (Quad)" },
                { key: "ease_in_cubic", label: "Ease In (Cubic)" },
                { key: "ease_out_cubic", label: "Ease Out (Cubic)" },
                { key: "ease_in_out_cubic", label: "Ease In-Out (Cubic)" },
                { key: "ease_in_quart", label: "Ease In (Quart)" },
                { key: "ease_out_quart", label: "Ease Out (Quart)" },
                { key: "ease_in_out_quart", label: "Ease In-Out (Quart)" },
                { key: "ease_in_sine", label: "Ease In (Sine)" },
                { key: "ease_out_sine", label: "Ease Out (Sine)" },
                { key: "ease_in_out_sine", label: "Ease In-Out (Sine)" },
                { key: "ease_in_expo", label: "Ease In (Expo)" },
                { key: "ease_out_expo", label: "Ease Out (Expo)" },
                { key: "ease_in_out_expo", label: "Ease In-Out (Expo)" },
                { key: "ease_in_circ", label: "Ease In (Circ)" },
                { key: "ease_out_circ", label: "Ease Out (Circ)" },
                { key: "ease_in_out_circ", label: "Ease In-Out (Circ)" },
                { key: "ease_in_back", label: "Ease In (Back/Overshoot)" },
                { key: "ease_out_back", label: "Ease Out (Back/Overshoot)" },
                { key: "ease_in_out_back", label: "Ease In-Out (Back)" },
                { key: "ease_in_elastic", label: "Ease In (Elastic)" },
                { key: "ease_out_elastic", label: "Ease Out (Elastic)" },
                { key: "ease_in_out_elastic", label: "Ease In-Out (Elastic)" },
                { key: "ease_in_bounce", label: "Ease In (Bounce)" },
                { key: "ease_out_bounce", label: "Ease Out (Bounce)" },
                { key: "ease_in_out_bounce", label: "Ease In-Out (Bounce)" },
                { key: "custom_bezier", label: "Custom Editable Bezier" }
            ]
            textRole: "label"
            currentIndex: {
                for (var i = 0; i < model.length; i++) {
                    if (model[i].key === curveControl.currentEase) return i;
                }
                return 0;
            }
            onActivated: {
                var chosen = model[currentIndex].key;
                curveControl.currentEase = chosen;
                if (chosen === "ease_in_quad" || chosen === "ease_in_cubic") {
                    curveControl.pX1 = 0.42; curveControl.pY1 = 0.0; curveControl.pX2 = 1.0; curveControl.pY2 = 1.0;
                } else if (chosen === "ease_out_quad" || chosen === "ease_out_cubic") {
                    curveControl.pX1 = 0.0; curveControl.pY1 = 0.0; curveControl.pX2 = 0.58; curveControl.pY2 = 1.0;
                } else if (chosen === "ease_in_out_quad" || chosen === "ease_in_out_cubic") {
                    curveControl.pX1 = 0.42; curveControl.pY1 = 0.0; curveControl.pX2 = 0.58; curveControl.pY2 = 1.0;
                } else if (chosen === "ease_out_back") {
                    curveControl.pX1 = 0.34; curveControl.pY1 = 1.56; curveControl.pX2 = 0.64; curveControl.pY2 = 1.0;
                } else if (chosen === "linear") {
                    curveControl.pX1 = 0.25; curveControl.pY1 = 0.25; curveControl.pX2 = 0.75; curveControl.pY2 = 0.75;
                }
                emitChange();
                cv.requestPaint();
            }
        }
    }

    // Amplitude Range Row (Min & Max Amplitude Limits)
    ColumnLayout {
        Layout.fillWidth: true
        spacing: 2

        RowLayout {
            Layout.fillWidth: true
            Label {
                text: "Curve Amplitude (Range Limits):"
                font.bold: true
                font.pixelSize: 11
                opacity: 0.85
            }
            Item { Layout.fillWidth: true }
            Label {
                text: curveControl.currentStart.toFixed(2) + "  ➔  " + curveControl.currentEnd.toFixed(2)
                font.pixelSize: 11
                font.bold: true
                color: Kirigami.Theme.highlightColor
            }
        }

        RangeSlider {
            id: ampRangeSlider
            Layout.fillWidth: true
            from: curveControl.paramMin
            to: curveControl.paramMax
            first.value: curveControl.currentStart
            second.value: curveControl.currentEnd
            stepSize: (curveControl.paramMax - curveControl.paramMin) > 50 ? 1.0 : 0.01

            first.onMoved: {
                curveControl.currentStart = first.value;
                emitChange();
                cv.requestPaint();
            }
            second.onMoved: {
                curveControl.currentEnd = second.value;
                emitChange();
                cv.requestPaint();
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Label {
                text: "Min Limit: " + curveControl.paramMin.toFixed(2)
                font.pixelSize: 10
                opacity: 0.6
            }
            Item { Layout.fillWidth: true }
            Label {
                text: "Max Limit: " + curveControl.paramMax.toFixed(2)
                font.pixelSize: 10
                opacity: 0.6
            }
        }
    }

    // Interactive Embedded Curve Canvas
    Rectangle {
        id: canvasBox
        Layout.fillWidth: true
        height: 140
        color: "#141414"
        border.color: Kirigami.Theme.separatorColor
        border.width: 1
        radius: 4
        clip: true

        Canvas {
            id: cv
            anchors.fill: parent
            anchors.margins: 16

            onPaint: {
                var ctx = getContext("2d");
                ctx.clearRect(0, 0, width, height);

                // Grid lines (normalized 0% to 100%)
                ctx.strokeStyle = "#222222";
                ctx.lineWidth = 1;
                for (var i = 0; i <= 4; i++) {
                    var gx = (width * i) / 4.0;
                    var gy = (height * i) / 4.0;
                    ctx.beginPath();
                    ctx.moveTo(gx, 0); ctx.lineTo(gx, height); ctx.stroke();
                    ctx.beginPath();
                    ctx.moveTo(0, gy); ctx.lineTo(width, gy); ctx.stroke();
                }

                // Linear reference diagonal
                ctx.strokeStyle = "#383838";
                ctx.setLineDash([3, 3]);
                ctx.beginPath();
                ctx.moveTo(0, height);
                ctx.lineTo(width, 0);
                ctx.stroke();
                ctx.setLineDash([]);

                // Control lines for Bezier
                var p0x = 0, p0y = height;
                var p3x = width, p3y = 0;
                var c1x = curveControl.pX1 * width;
                var c1y = (1.0 - curveControl.pY1) * height;
                var c2x = curveControl.pX2 * width;
                var c2y = (1.0 - curveControl.pY2) * height;

                ctx.strokeStyle = "#00bcd4";
                ctx.lineWidth = 1.0;
                ctx.beginPath();
                ctx.moveTo(p0x, p0y); ctx.lineTo(c1x, c1y); ctx.stroke();

                ctx.strokeStyle = "#ff9800";
                ctx.beginPath();
                ctx.moveTo(p3x, p3y); ctx.lineTo(c2x, c2y); ctx.stroke();

                // Main Curve Stroke
                ctx.strokeStyle = "#3daee9";
                ctx.lineWidth = 2.5;
                ctx.beginPath();
                ctx.moveTo(p0x, p0y);
                ctx.bezierCurveTo(c1x, c1y, c2x, c2y, p3x, p3y);
                ctx.stroke();
            }
        }

        // Draggable Handle 1 (P1 - Cyan)
        Rectangle {
            id: h1
            width: 14
            height: 14
            radius: 7
            color: "#00bcd4"
            border.color: "#ffffff"
            border.width: 1.5
            x: cv.x + curveControl.pX1 * cv.width - width / 2
            y: cv.y + (1.0 - curveControl.pY1) * cv.height - height / 2

            MouseArea {
                anchors.fill: parent
                drag.target: parent
                drag.axis: Drag.XAndYAxis
                onPositionChanged: {
                    if (drag.active) {
                        var rx = (h1.x + h1.width / 2 - cv.x) / cv.width;
                        var ry = 1.0 - (h1.y + h1.height / 2 - cv.y) / cv.height;
                        curveControl.pX1 = Math.max(0.0, Math.min(1.0, rx));
                        curveControl.pY1 = Math.max(-0.2, Math.min(1.4, ry));
                        curveControl.currentEase = "custom_bezier";
                        emitChange();
                        cv.requestPaint();
                    }
                }
            }
        }

        // Draggable Handle 2 (P2 - Orange)
        Rectangle {
            id: h2
            width: 14
            height: 14
            radius: 7
            color: "#ff9800"
            border.color: "#ffffff"
            border.width: 1.5
            x: cv.x + curveControl.pX2 * cv.width - width / 2
            y: cv.y + (1.0 - curveControl.pY2) * cv.height - height / 2

            MouseArea {
                anchors.fill: parent
                drag.target: parent
                drag.axis: Drag.XAndYAxis
                onPositionChanged: {
                    if (drag.active) {
                        var rx = (h2.x + h2.width / 2 - cv.x) / cv.width;
                        var ry = 1.0 - (h2.y + h2.height / 2 - cv.y) / cv.height;
                        curveControl.pX2 = Math.max(0.0, Math.min(1.0, rx));
                        curveControl.pY2 = Math.max(-0.2, Math.min(1.4, ry));
                        curveControl.currentEase = "custom_bezier";
                        emitChange();
                        cv.requestPaint();
                    }
                }
            }
        }

        Label {
            anchors.bottom: parent.bottom
            anchors.right: parent.right
            anchors.margins: 4
            text: "Time (0% → 100%)"
            font.pixelSize: 9
            opacity: 0.5
        }
    }

    function emitChange() {
        curveChanged(
            currentStart,
            currentEnd,
            currentEase,
            pX1,
            pY1,
            pX2,
            pY2
        );
    }
}
