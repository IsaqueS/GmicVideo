#!/usr/bin/env python3
import sys
import os
import json
import subprocess
import threading
import time
from PySide6.QtCore import QObject, Slot, Signal, Property, QUrl
from PySide6.QtWidgets import QApplication, QFileDialog
from PySide6.QtQml import QQmlApplicationEngine

class RustBackendBridge(QObject):
    previewReady = Signal(str, str, float) # raw_path, effect_path, time_secs
    queueUpdated = Signal(str)             # json array of RenderQueueItems
    renderProgressUpdated = Signal(str)    # legacy json progress
    renderCompleted = Signal(str)          # output path
    renderFailed = Signal(str)             # error message
    effectsLoaded = Signal(str)            # json effects

    def __init__(self, backend_bin_path):
        super().__init__()
        self.backend_bin_path = backend_bin_path
        self.process = None
        self.lock = threading.Lock()
        self._last_queue_json = ""
        self._completed_notified = set()
        self._start_backend()
        self._start_queue_poller()

    def _start_backend(self):
        try:
            self.process = subprocess.Popen(
                [self.backend_bin_path, "--server"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                text=True,
                bufsize=1
            )
        except Exception as e:
            print(f"[Error] Failed to spawn Rust backend binary: {e}", file=sys.stderr)

    def _send_request(self, action, payload=None):
        if not self.process or self.process.poll() is not None:
            self._start_backend()

        req = {"action": action}
        if payload is not None:
            req["payload"] = payload

        req_line = json.dumps(req) + "\n"
        with self.lock:
            try:
                self.process.stdin.write(req_line)
                self.process.stdin.flush()
                while True:
                    resp_line = self.process.stdout.readline()
                    if not resp_line:
                        return {"status": "error", "data": "Empty response from backend"}
                    resp_line = resp_line.strip()
                    if resp_line.startswith("{"):
                        return json.loads(resp_line)
            except Exception as e:
                return {"status": "error", "data": str(e)}

    def _start_queue_poller(self):
        def poller():
            while True:
                time.sleep(0.25)
                res = self._send_request("get_render_queue")
                if res.get("status") == "ok":
                    items = res.get("data", [])
                    raw_json = json.dumps(items)
                    if raw_json != self._last_queue_json:
                        self._last_queue_json = raw_json
                        self.queueUpdated.emit(raw_json)

                    # Also update legacy progress for active item
                    for item in items:
                        if item.get("status") == "rendering":
                            prog = item.get("progress", {})
                            self.renderProgressUpdated.emit(json.dumps(prog))
                        elif item.get("status") == "completed":
                            j_id = item.get("id")
                            if j_id not in self._completed_notified:
                                self._completed_notified.add(j_id)
                                self.renderCompleted.emit(item.get("output_path", ""))

        t = threading.Thread(target=poller, daemon=True)
        t.start()

    @Slot(result=str)
    def getEffects(self):
        res = self._send_request("get_effects")
        if res.get("status") == "ok":
            return json.dumps(res.get("data", {}))
        return "{}"

    @Slot(str, result=str)
    def probeVideo(self, path):
        clean_path = path.replace("file://", "")
        res = self._send_request("probe_video", {"path": clean_path})
        if res.get("status") == "ok":
            return json.dumps(res.get("data", {}))
        else:
            return json.dumps({"error": res.get("data", "Probe failed")})

    @Slot(str, float, float, str, str, str)
    def renderPreview(self, video_path, time_secs, duration_secs, filter_id, base_values_json, tweens_json):
        clean_path = video_path.replace("file://", "")
        def worker():
            try:
                base_values = json.loads(base_values_json) if base_values_json else {}
                base_values_parsed = {int(k): float(v) for k, v in base_values.items()}
                tweens = json.loads(tweens_json) if tweens_json else {}
                tweens_parsed = {int(k): v for k, v in tweens.items()}

                payload = {
                    "video_path": clean_path,
                    "time_secs": float(time_secs),
                    "duration_secs": float(duration_secs),
                    "filter_id": filter_id,
                    "base_values": base_values_parsed,
                    "tweens": tweens_parsed
                }
                res = self._send_request("render_preview", payload)
                if res.get("status") == "ok":
                    data = res.get("data", {})
                    raw_p = data.get("raw_preview", "")
                    eff_p = data.get("effect_preview", "")
                    t_val = data.get("time_secs", time_secs)
                    self.previewReady.emit(raw_p, eff_p, float(t_val))
                else:
                    print(f"[Preview Error] {res.get('data')}", file=sys.stderr)
            except Exception as e:
                print(f"[Preview Exception] {e}", file=sys.stderr)

        threading.Thread(target=worker, daemon=True).start()

    @Slot(str, list, list, int, result=str)
    def evaluateCurve(self, ease_type, bezier_points, curve_points, sample_count=40):
        b_pts = [float(x) for x in bezier_points] if bezier_points and len(bezier_points) == 4 else None
        c_pts = [[float(p[0]), float(p[1])] for p in curve_points] if curve_points else None
        payload = {
            "ease_type": ease_type,
            "bezier_points": b_pts,
            "curve_points": c_pts,
            "sample_count": sample_count
        }
        res = self._send_request("evaluate_curve", payload)
        if res.get("status") == "ok":
            return json.dumps(res.get("data", []))
        return "[]"

    @Slot(str, str, str, int, int, str, str, str, str, float, float, result=str)
    @Slot(str, str, str, int, int, str, str, str, str, result=str)
    def queueRender(self, video_path, output_path, format_str, quality, speed, filter_id, filter_name, base_values_json, tweens_json, cut_start=0.0, cut_end=-1.0):
        """
        Simplified queue render slot:
          - quality: integer 1..10 (1 = draft, 10 = maximum)
          - speed: integer 1..10 (1 = max compression, 10 = ultrafast)
        """
        clean_in = video_path.replace("file://", "")
        clean_out = output_path.replace("file://", "")
        base_values = json.loads(base_values_json) if base_values_json else {}
        base_values_parsed = {int(k): float(v) for k, v in base_values.items()}
        tweens = json.loads(tweens_json) if tweens_json else {}
        tweens_parsed = {int(k): v for k, v in tweens.items()}

        # Map Quality 1..10 -> CRF 32..14 (lower CRF is higher quality)
        q = max(1, min(10, int(quality)))
        crf = int(round(32 - (q - 1) * 2.0))

        # Map Speed 1..10 -> ffmpeg preset
        s = max(1, min(10, int(speed)))
        if s <= 2:
            preset = "veryslow"
        elif s <= 4:
            preset = "slow"
        elif s <= 6:
            preset = "medium"
        elif s <= 8:
            preset = "fast"
        else:
            preset = "ultrafast"

        job_id = f"job_{int(time.time() * 1000)}"
        job_name = os.path.basename(clean_out) if clean_out else f"Render_{filter_name}.mp4"

        payload = {
            "id": job_id,
            "name": job_name,
            "video_path": clean_in,
            "output_path": clean_out,
            "format": format_str.lower(),
            "crf": crf,
            "speed_preset": preset,
            "filter_id": filter_id,
            "filter_name": filter_name,
            "base_values": base_values_parsed,
            "tweens": tweens_parsed,
            "cut_start_secs": float(cut_start) if cut_start > 0.001 else None,
            "cut_end_secs": float(cut_end) if cut_end > 0.001 else None
        }

        res = self._send_request("queue_render", payload)
        if res.get("status") == "ok":
            # Force immediate queue update
            q_res = self._send_request("get_render_queue")
            if q_res.get("status") == "ok":
                self.queueUpdated.emit(json.dumps(q_res.get("data", [])))
            return job_id
        else:
            err = str(res.get("data", "Failed to queue render"))
            self.renderFailed.emit(err)
            return ""

    @Slot(result=str)
    def getRenderQueue(self):
        res = self._send_request("get_render_queue")
        if res.get("status") == "ok":
            return json.dumps(res.get("data", []))
        return "[]"

    @Slot(str)
    def cancelRenderJob(self, job_id):
        self._send_request("cancel_render_job", {"job_id": job_id})
        q_res = self._send_request("get_render_queue")
        if q_res.get("status") == "ok":
            self.queueUpdated.emit(json.dumps(q_res.get("data", [])))

    @Slot()
    def clearCompletedJobs(self):
        self._send_request("clear_completed_jobs")
        q_res = self._send_request("get_render_queue")
        if q_res.get("status") == "ok":
            self.queueUpdated.emit(json.dumps(q_res.get("data", [])))

    # Legacy startRender for compatibility
    @Slot(str, str, str, int, str, str, str)
    def startRender(self, video_path, output_path, format_str, crf, filter_id, base_values_json, tweens_json):
        self.queueRender(
            video_path, output_path, format_str, 7, 6,
            filter_id, filter_id, base_values_json, tweens_json
        )

    @Slot()
    def cancelRender(self):
        self._send_request("cancel_render")

    @Slot(result=str)
    def chooseVideoFile(self):
        file_path, _ = QFileDialog.getOpenFileName(
            None,
            "Select Video to Edit",
            os.path.expanduser("~"),
            "Video Files (*.mp4 *.webm *.mkv *.mov *.avi *.flv *.ts);;All Files (*.*)"
        )
        return file_path or ""

    @Slot(str, str, result=str)
    def chooseOutputFile(self, default_name, extension="mp4"):
        default_path = os.path.join(os.path.expanduser("~/Vídeos"), default_name)
        if not os.path.exists(os.path.dirname(default_path)):
            default_path = os.path.join(os.path.expanduser("~"), default_name)

        file_path, _ = QFileDialog.getSaveFileName(
            None,
            "Choose Render Destination",
            default_path,
            f"Video (*.{extension});;All Files (*.*)"
        )
        return file_path or ""

    @Slot(str)
    def openFolder(self, file_path):
        clean_path = file_path.replace("file://", "")
        folder = os.path.dirname(clean_path) if os.path.isfile(clean_path) else clean_path
        if os.path.exists(folder):
            subprocess.Popen(["xdg-open", folder])

    @Slot(str)
    def openFile(self, file_path):
        clean_path = file_path.replace("file://", "")
        if os.path.exists(clean_path):
            subprocess.Popen(["xdg-open", clean_path])

def main():
    app = QApplication(sys.argv)
    app.setApplicationName("GmicVideo")
    app.setOrganizationName("KDE")

    backend_bin = os.environ.get("GMIC_VIDEO_BACKEND_BIN")
    if not backend_bin or not os.path.exists(backend_bin):
        base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        rel_bin = os.path.join(base_dir, "target", "release", "GmicVideo")
        dbg_bin = os.path.join(base_dir, "target", "debug", "GmicVideo")
        if os.path.exists(rel_bin):
            backend_bin = rel_bin
        elif os.path.exists(dbg_bin):
            backend_bin = dbg_bin
        else:
            backend_bin = "gmicvideo"

    bridge = RustBackendBridge(backend_bin)

    engine = QQmlApplicationEngine()
    engine.rootContext().setContextProperty("Backend", bridge)

    qml_file = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "ui", "main.qml")
    engine.load(QUrl.fromLocalFile(qml_file))

    if not engine.rootObjects():
        print("[Error] Failed to load main.qml", file=sys.stderr)
        sys.exit(1)

    sys.exit(app.exec())

if __name__ == "__main__":
    main()
