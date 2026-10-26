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
    property real pY1: 0.25
    property real pX2: 0.75
    property real pY2: 0.75

    signal curveChanged(real startVal, real endVal, string easeType, real x1, real y1, real x2, real y2)

    onCurrentEaseChanged: {
        for (var i = 0; i < presetCombo.model.length; i++) {
            if (presetCombo.model[i].key === currentEase) {
                if (presetCombo.currentIndex !== i) {
                    presetCombo.currentIndex = i;
                }
                break;
            }
        }
        cv.requestPaint();
    }
    onPX1Changed: cv.requestPaint()
    onPY1Changed: cv.requestPaint()
    onPX2Changed: cv.requestPaint()
    onPY2Changed: cv.requestPaint()
    onCurrentStartChanged: cv.requestPaint()
    onCurrentEndChanged: cv.requestPaint()

    // Preset selection row
    RowLayout {
        Layout.fillWidth: true
        spacing: 6

        Label {
            text: i18n.t("Curve Preset:")
            font.bold: true
            font.pixelSize: 11
            opacity: 0.85
        }

        ComboBox {
            id: presetCombo
            Layout.fillWidth: true
            model: [
                { key: "linear", label: i18n.t("Linear (Default)") },
                { key: "ease_in_quad", label: i18n.t("Ease In (Quad)") },
                { key: "ease_out_quad", label: i18n.t("Ease Out (Quad)") },
                { key: "ease_in_out_quad", label: i18n.t("Ease In-Out (Quad)") },
                { key: "ease_in_cubic", label: i18n.t("Ease In (Cubic)") },
                { key: "ease_out_cubic", label: i18n.t("Ease Out (Cubic)") },
                { key: "ease_in_out_cubic", label: i18n.t("Ease In-Out (Cubic)") },
                { key: "ease_in_quart", label: i18n.t("Ease In (Quart)") },
                { key: "ease_out_quart", label: i18n.t("Ease Out (Quart)") },
                { key: "ease_in_out_quart", label: i18n.t("Ease In-Out (Quart)") },
                { key: "ease_in_sine", label: i18n.t("Ease In (Sine)") },
                { key: "ease_out_sine", label: i18n.t("Ease Out (Sine)") },
                { key: "ease_in_out_sine", label: i18n.t("Ease In-Out (Sine)") },
                { key: "ease_in_expo", label: i18n.t("Ease In (Expo)") },
                { key: "ease_out_expo", label: i18n.t("Ease Out (Expo)") },
                { key: "ease_in_out_expo", label: i18n.t("Ease In-Out (Expo)") },
                { key: "ease_in_circ", label: i18n.t("Ease In (Circ)") },
                { key: "ease_out_circ", label: i18n.t("Ease Out (Circ)") },
                { key: "ease_in_out_circ", label: i18n.t("Ease In-Out (Circ)") },
                { key: "ease_in_back", label: i18n.t("Ease In (Back/Overshoot)") },
                { key: "ease_out_back", label: i18n.t("Ease Out (Back/Overshoot)") },
                { key: "ease_in_out_back", label: i18n.t("Ease In-Out (Back)") },
                { key: "ease_in_elastic", label: i18n.t("Ease In (Elastic)") },
                { key: "ease_out_elastic", label: i18n.t("Ease Out (Elastic)") },
                { key: "ease_in_out_elastic", label: i18n.t("Ease In-Out (Elastic)") },
                { key: "ease_in_bounce", label: i18n.t("Ease In (Bounce)") },
                { key: "ease_out_bounce", label: i18n.t("Ease Out (Bounce)") },
                { key: "ease_in_out_bounce", label: i18n.t("Ease In-Out (Bounce)") },
                { key: "custom_bezier", label: i18n.t("Custom Editable Bezier") }
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
                text: i18n.t("Curve Amplitude (Range Limits):")
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
                text: i18n.t("Min Limit: %1").replace("%1", curveControl.paramMin.toFixed(2))
                font.pixelSize: 10
                opacity: 0.6
            }
            Item { Layout.fillWidth: true }
            Label {
                text: i18n.t("Max Limit: %1").replace("%1", curveControl.paramMax.toFixed(2))
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
            anchors.margins: 8

            readonly property real padX: 12
            readonly property real padY: 14

            onPaint: {
                var ctx = getContext("2d");
                ctx.clearRect(0, 0, width, height);

                var plotW = width - 2 * padX;
                var plotH = height - 2 * padY;

                // Grid lines (0%, 25%, 50%, 75%, 100%)
                ctx.strokeStyle = "#242424";
                ctx.lineWidth = 1;
                for (var i = 0; i <= 4; i++) {
                    var gx = padX + (plotW * i) / 4.0;
                    var gy = padY + (plotH * i) / 4.0;

                    ctx.beginPath();
                    ctx.moveTo(gx, padY);
                    ctx.lineTo(gx, padY + plotH);
                    ctx.stroke();

                    ctx.beginPath();
                    ctx.moveTo(padX, gy);
                    ctx.lineTo(padX + plotW, gy);
                    ctx.stroke();
                }

                // Reference diagonal line (Linear)
                ctx.strokeStyle = "#383838";
                ctx.lineWidth = 1;
                ctx.setLineDash([3, 3]);
                ctx.beginPath();
                ctx.moveTo(padX, padY + plotH);
                ctx.lineTo(padX + plotW, padY);
                ctx.stroke();
                ctx.setLineDash([]);

                // Control lines for Custom Bezier
                if (curveControl.currentEase === "custom_bezier") {
                    var p0x = padX, p0y = padY + plotH;
                    var p3x = padX + plotW, p3y = padY;
                    var c1x = padX + curveControl.pX1 * plotW;
                    var c1y = padY + (1.0 - curveControl.pY1) * plotH;
                    var c2x = padX + curveControl.pX2 * plotW;
                    var c2y = padY + (1.0 - curveControl.pY2) * plotH;

                    ctx.strokeStyle = "#00bcd4";
                    ctx.lineWidth = 1.0;
                    ctx.beginPath();
                    ctx.moveTo(p0x, p0y);
                    ctx.lineTo(c1x, c1y);
                    ctx.stroke();

                    ctx.strokeStyle = "#ff9800";
                    ctx.beginPath();
                    ctx.moveTo(p3x, p3y);
                    ctx.lineTo(c2x, c2y);
                    ctx.stroke();
                }

                // Main Curve Stroke (sampled across 100 segments for exact mathematical shape)
                var samples = 100;
                ctx.strokeStyle = "#3daee9";
                ctx.lineWidth = 2.5;
                ctx.beginPath();

                for (var s = 0; s <= samples; s++) {
                    var t = s / samples;
                    var factor = evaluateEase(t, curveControl.currentEase);
                    var px = padX + t * plotW;
                    var py = padY + (1.0 - factor) * plotH;
                    if (s === 0) {
                        ctx.moveTo(px, py);
                    } else {
                        ctx.lineTo(px, py);
                    }
                }
                ctx.stroke();
            }
        }

        // Draggable Handle 1 (P1 - Cyan) for Custom Bezier
        Rectangle {
            id: h1
            visible: curveControl.currentEase === "custom_bezier"
            width: 14
            height: 14
            radius: 7
            color: "#00bcd4"
            border.color: "#ffffff"
            border.width: 1.5

            Binding on x {
                when: !h1MouseArea.drag.active
                value: cv.x + cv.padX + curveControl.pX1 * (cv.width - 2 * cv.padX) - h1.width / 2
            }
            Binding on y {
                when: !h1MouseArea.drag.active
                value: cv.y + cv.padY + (1.0 - curveControl.pY1) * (cv.height - 2 * cv.padY) - h1.height / 2
            }

            MouseArea {
                id: h1MouseArea
                anchors.fill: parent
                drag.target: parent
                drag.axis: Drag.XAndYAxis
                onPositionChanged: {
                    if (drag.active) {
                        var plotW = cv.width - 2 * cv.padX;
                        var plotH = cv.height - 2 * cv.padY;
                        var rx = (h1.x + h1.width / 2 - cv.x - cv.padX) / plotW;
                        var ry = 1.0 - (h1.y + h1.height / 2 - cv.y - cv.padY) / plotH;
                        curveControl.pX1 = Math.max(0.0, Math.min(1.0, rx));
                        curveControl.pY1 = Math.max(-0.25, Math.min(1.35, ry));
                        curveControl.currentEase = "custom_bezier";
                        emitChange();
                        cv.requestPaint();
                    }
                }
            }
        }

        // Draggable Handle 2 (P2 - Orange) for Custom Bezier
        Rectangle {
            id: h2
            visible: curveControl.currentEase === "custom_bezier"
            width: 14
            height: 14
            radius: 7
            color: "#ff9800"
            border.color: "#ffffff"
            border.width: 1.5

            Binding on x {
                when: !h2MouseArea.drag.active
                value: cv.x + cv.padX + curveControl.pX2 * (cv.width - 2 * cv.padX) - h2.width / 2
            }
            Binding on y {
                when: !h2MouseArea.drag.active
                value: cv.y + cv.padY + (1.0 - curveControl.pY2) * (cv.height - 2 * cv.padY) - h2.height / 2
            }

            MouseArea {
                id: h2MouseArea
                anchors.fill: parent
                drag.target: parent
                drag.axis: Drag.XAndYAxis
                onPositionChanged: {
                    if (drag.active) {
                        var plotW = cv.width - 2 * cv.padX;
                        var plotH = cv.height - 2 * cv.padY;
                        var rx = (h2.x + h2.width / 2 - cv.x - cv.padX) / plotW;
                        var ry = 1.0 - (h2.y + h2.height / 2 - cv.y - cv.padY) / plotH;
                        curveControl.pX2 = Math.max(0.0, Math.min(1.0, rx));
                        curveControl.pY2 = Math.max(-0.25, Math.min(1.35, ry));
                        curveControl.currentEase = "custom_bezier";
                        emitChange();
                        cv.requestPaint();
                    }
                }
            }
        }

        // Contextual curve info label
        Label {
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.margins: 4
            text: curveControl.currentEase === "custom_bezier" ? "Drag Cyan (P1) & Orange (P2) handles to sculpt curve" : presetCombo.currentText
            font.pixelSize: 9
            opacity: 0.6
        }

        Label {
            anchors.bottom: parent.bottom
            anchors.right: parent.right
            anchors.margins: 4
            text: i18n.t("Time (0% ➔ 100%)")
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

    // Mathematical easing evaluator matching src/tween.rs
    function evaluateEase(t, easeType) {
        var clampedT = Math.max(0.0, Math.min(1.0, t));
        var PI = Math.PI;

        switch (easeType) {
            case "linear":
                return clampedT;
            case "ease_in_quad":
                return clampedT * clampedT;
            case "ease_out_quad":
                return clampedT * (2.0 - clampedT);
            case "ease_in_out_quad":
                if (clampedT < 0.5) return 2.0 * clampedT * clampedT;
                return -1.0 + (4.0 - 2.0 * clampedT) * clampedT;
            case "ease_in_cubic":
                return clampedT * clampedT * clampedT;
            case "ease_out_cubic":
                var f = clampedT - 1.0;
                return f * f * f + 1.0;
            case "ease_in_out_cubic":
                if (clampedT < 0.5) return 4.0 * clampedT * clampedT * clampedT;
                var f = 2.0 * clampedT - 2.0;
                return 0.5 * f * f * f + 1.0;
            case "ease_in_quart":
                return Math.pow(clampedT, 4);
            case "ease_out_quart":
                var f = clampedT - 1.0;
                return 1.0 - Math.pow(f, 4);
            case "ease_in_out_quart":
                if (clampedT < 0.5) return 8.0 * Math.pow(clampedT, 4);
                var f = clampedT - 1.0;
                return 1.0 - 8.0 * Math.pow(f, 4);
            case "ease_in_sine":
                return 1.0 - Math.cos((clampedT * PI) / 2.0);
            case "ease_out_sine":
                return Math.sin((clampedT * PI) / 2.0);
            case "ease_in_out_sine":
                return -0.5 * (Math.cos(PI * clampedT) - 1.0);
            case "ease_in_expo":
                return clampedT === 0.0 ? 0.0 : Math.pow(2.0, 10.0 * (clampedT - 1.0));
            case "ease_out_expo":
                return clampedT === 1.0 ? 1.0 : 1.0 - Math.pow(2.0, -10.0 * clampedT);
            case "ease_in_out_expo":
                if (clampedT === 0.0) return 0.0;
                if (clampedT === 1.0) return 1.0;
                if (clampedT < 0.5) return 0.5 * Math.pow(2.0, 20.0 * clampedT - 10.0);
                return -0.5 * Math.pow(2.0, -20.0 * clampedT + 10.0) + 1.0;
            case "ease_in_circ":
                return 1.0 - Math.sqrt(Math.max(0.0, 1.0 - clampedT * clampedT));
            case "ease_out_circ":
                return Math.sqrt(Math.max(0.0, 1.0 - Math.pow(clampedT - 1.0, 2)));
            case "ease_in_out_circ":
                if (clampedT < 0.5) {
                    return 0.5 * (1.0 - Math.sqrt(Math.max(0.0, 1.0 - 4.0 * clampedT * clampedT)));
                } else {
                    return 0.5 * (Math.sqrt(Math.max(0.0, 1.0 - Math.pow(2.0 * clampedT - 2.0, 2))) + 1.0);
                }
            case "ease_in_back": {
                var c1 = 1.70158;
                var c3 = c1 + 1.0;
                return c3 * Math.pow(clampedT, 3) - c1 * Math.pow(clampedT, 2);
            }
            case "ease_out_back": {
                var c1 = 1.70158;
                var c3 = c1 + 1.0;
                var f = clampedT - 1.0;
                return 1.0 + c3 * Math.pow(f, 3) + c1 * Math.pow(f, 2);
            }
            case "ease_in_out_back": {
                var c1 = 1.70158;
                var c2 = c1 * 1.525;
                if (clampedT < 0.5) {
                    return (Math.pow(2.0 * clampedT, 2) * ((c2 + 1.0) * 2.0 * clampedT - c2)) / 2.0;
                } else {
                    return (Math.pow(2.0 * clampedT - 2.0, 2) * ((c2 + 1.0) * (clampedT * 2.0 - 2.0) + c2) + 2.0) / 2.0;
                }
            }
            case "ease_in_elastic": {
                if (clampedT === 0.0) return 0.0;
                if (clampedT === 1.0) return 1.0;
                var c4 = (2.0 * PI) / 3.0;
                return -Math.pow(2.0, 10.0 * clampedT - 10.0) * Math.sin((clampedT * 10.0 - 10.75) * c4);
            }
            case "ease_out_elastic": {
                if (clampedT === 0.0) return 0.0;
                if (clampedT === 1.0) return 1.0;
                var c4 = (2.0 * PI) / 3.0;
                return Math.pow(2.0, -10.0 * clampedT) * Math.sin((clampedT * 10.0 - 0.75) * c4) + 1.0;
            }
            case "ease_in_out_elastic": {
                if (clampedT === 0.0) return 0.0;
                if (clampedT === 1.0) return 1.0;
                var c5 = (2.0 * PI) / 4.5;
                if (clampedT < 0.5) {
                    return -0.5 * Math.pow(2.0, 20.0 * clampedT - 10.0) * Math.sin((20.0 * clampedT - 11.125) * c5);
                } else {
                    return 0.5 * Math.pow(2.0, -20.0 * clampedT + 10.0) * Math.sin((20.0 * clampedT - 11.125) * c5) + 1.0;
                }
            }
            case "ease_in_bounce":
                return 1.0 - evaluateEaseOutBounce(1.0 - clampedT);
            case "ease_out_bounce":
                return evaluateEaseOutBounce(clampedT);
            case "ease_in_out_bounce":
                if (clampedT < 0.5) {
                    return (1.0 - evaluateEaseOutBounce(1.0 - 2.0 * clampedT)) / 2.0;
                } else {
                    return (1.0 + evaluateEaseOutBounce(2.0 * clampedT - 1.0)) / 2.0;
                }
            case "custom_bezier":
                return evaluateCubicBezier(clampedT, pX1, pY1, pX2, pY2);
            default:
                return clampedT;
        }
    }

    function evaluateEaseOutBounce(t) {
        var n1 = 7.5625;
        var d1 = 2.75;
        if (t < 1.0 / d1) {
            return n1 * t * t;
        } else if (t < 2.0 / d1) {
            var f = t - 1.5 / d1;
            return n1 * f * f + 0.75;
        } else if (t < 2.5 / d1) {
            var f = t - 2.25 / d1;
            return n1 * f * f + 0.9375;
        } else {
            var f = t - 2.625 / d1;
            return n1 * f * f + 0.984375;
        }
    }

    function bezierCoord(t, c1, c2) {
        return 3.0 * (1.0 - t) * (1.0 - t) * t * c1 + 3.0 * (1.0 - t) * t * t * c2 + t * t * t;
    }

    function bezierDerivative(t, c1, c2) {
        return 3.0 * (1.0 - t) * (1.0 - t) * c1 + 6.0 * (1.0 - t) * t * (c2 - c1) + 3.0 * t * t * (1.0 - c2);
    }

    function evaluateCubicBezier(x, x1, y1, x2, y2) {
        if (x <= 0.0) return 0.0;
        if (x >= 1.0) return 1.0;
        var s = x;
        for (var i = 0; i < 12; i++) {
            var cx = bezierCoord(s, x1, x2);
            var err = cx - x;
            if (Math.abs(err) < 1e-6) return bezierCoord(s, y1, y2);
            var slope = bezierDerivative(s, x1, x2);
            if (Math.abs(slope) < 1e-6) break;
            s = Math.max(0.0, Math.min(1.0, s - err / slope));
        }
        var low = 0.0, high = 1.0;
        s = x;
        for (var j = 0; j < 16; j++) {
            var cx = bezierCoord(s, x1, x2);
            if (Math.abs(cx - x) < 1e-5) break;
            if (cx < x) low = s; else high = s;
            s = 0.5 * (low + high);
        }
        return bezierCoord(s, y1, y2);
    }
}
