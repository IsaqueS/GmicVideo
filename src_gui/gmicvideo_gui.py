#!/usr/bin/env python3
import sys
import os
import json
import subprocess
import threading
import time
from PySide6.QtCore import QObject, Slot, Signal, Property, QUrl
from PySide6.QtWidgets import QApplication, QFileDialog
from PySide6.QtGui import QIcon
from PySide6.QtQml import QQmlApplicationEngine
from i18n import I18nManager

class RustBackendBridge(QObject):
    previewReady = Signal(str, str, float) # raw_path, effect_path, time_secs
    queueUpdated = Signal(str)             # json array of RenderQueueItems
    renderProgressUpdated = Signal(str)    # legacy json progress
    renderCompleted = Signal(str)          # output path
    renderFailed = Signal(str)             # error message
    effectsLoaded = Signal(str)            # json effects
    postRenderActionChanged = Signal(str)

    def __init__(self, backend_bin_path):
        super().__init__()
        self.backend_bin_path = backend_bin_path
        self.process = None
        self.lock = threading.Lock()
        self._last_queue_json = ""
        self._completed_notified = set()
        self._post_render_action = "none" # "none" or "shutdown"
        self._had_active_jobs = False
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
                    active_jobs = [item for item in items if item.get("status") in ("rendering", "queued")]
                    if self._had_active_jobs and not active_jobs:
                        self._had_active_jobs = False
                        if self._post_render_action == "shutdown":
                            # Safety check: if 100% of renders failed (or no job succeeded), do NOT shutdown!
                            completed_count = sum(1 for item in items if item.get("status") == "completed")
                            failed_count = sum(1 for item in items if item.get("status") == "failed")
                            canceled_count = sum(1 for item in items if item.get("status") == "canceled")
                            total_finished = completed_count + failed_count + canceled_count
                            
                            if total_finished > 0 and completed_count == 0:
                                print(f"[System] Shutdown ABORTED: 100% of render tasks failed or were canceled ({failed_count} failed, {canceled_count} canceled, 0 succeeded).", flush=True)
                            elif completed_count > 0:
                                print(f"[System] All rendering tasks completed successfully ({completed_count} succeeded, {failed_count} failed). Proceeding with shutdown...", flush=True)
                                self._trigger_shutdown()
                            else:
                                print("[System] Shutdown skipped: no successful render jobs found in queue.", flush=True)
                    elif active_jobs:
                        self._had_active_jobs = True

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

    @staticmethod
    def find_thumbnails_dir():
        candidates = []
        if "GMIC_VIDEO_THUMBNAILS_DIR" in os.environ:
            candidates.append(os.environ["GMIC_VIDEO_THUMBNAILS_DIR"])
        if "APPDIR" in os.environ:
            candidates.append(os.path.join(os.environ["APPDIR"], "usr", "share", "gmicvideo", "Tests", "valid_effects"))
            candidates.append(os.path.join(os.environ["APPDIR"], "Tests", "valid_effects"))
        script_dir = os.path.dirname(os.path.abspath(__file__))
        candidates.append(os.path.join(script_dir, "..", "Tests", "valid_effects"))
        candidates.append(os.path.join(script_dir, "..", "share", "gmicvideo", "Tests", "valid_effects"))
        candidates.append(os.path.join(script_dir, "Tests", "valid_effects"))
        cwd = os.getcwd()
        candidates.append(os.path.join(cwd, "Tests", "valid_effects"))
        candidates.append(os.path.join(cwd, "packaging", "AppDir", "usr", "share", "gmicvideo", "Tests", "valid_effects"))
        candidates.append("/usr/share/gmicvideo/Tests/valid_effects")

        for c in candidates:
            norm = os.path.normpath(c)
            if os.path.isdir(norm) and os.path.exists(norm):
                try:
                    if any(f.endswith(".png") for f in os.listdir(norm)):
                        return norm
                except Exception:
                    pass
        return None

    @Slot(result=str)
    def getEffects(self):
        res = self._send_request("get_effects")
        if res.get("status") == "ok":
            data = res.get("data", {})
            tests_dir = self.find_thumbnails_dir()
            if tests_dir and "filters" in data:
                try:
                    available_images = {img.lower(): img for img in os.listdir(tests_dir) if img.endswith(".png")}

                    manifest_map = {}
                    manifest_cand = [
                        os.path.join(os.path.dirname(tests_dir), "valid_effects.json"),
                        os.path.join(tests_dir, "valid_effects.json"),
                        os.path.join(tests_dir, "..", "valid_effects.json"),
                    ]
                    for m_path in manifest_cand:
                        if os.path.isfile(m_path):
                            try:
                                with open(m_path, "r", encoding="utf-8") as mf:
                                    items = json.load(mf)
                                    for item in items:
                                        if item.get("status") == "valid":
                                            i_id = item.get("id", "").lower()
                                            i_cmd = item.get("command", "").split()[0].lower() if item.get("command") else ""
                                            i_name = "".join(c.lower() if c.isalnum() else "_" for c in item.get("name", ""))
                                            out_p = os.path.basename(item.get("output_path", ""))
                                            if out_p in available_images:
                                                if i_id: manifest_map[i_id] = available_images[out_p]
                                                if i_cmd: manifest_map[i_cmd] = available_images[out_p]
                                                if i_name: manifest_map[i_name] = available_images[out_p]
                                break
                            except Exception:
                                pass

                    loaded_count = 0
                    for f in data["filters"]:
                        fid = f.get("id", "").lower()
                        cmd = f.get("command", "").lower()
                        name_clean = "".join(c.lower() if c.isalnum() else "_" for c in f.get("name", ""))
                        cat_clean = "".join(c.lower() if c.isalnum() else "_" for c in f.get("category", ""))

                        candidates = [
                            f"{fid}.png",
                            manifest_map.get(fid, ""),
                            manifest_map.get(cmd, ""),
                            manifest_map.get(name_clean, ""),
                            f"{cat_clean}_{name_clean}.png",
                            f"{cmd}.png",
                            f"{name_clean}.png",
                        ]
                        matched_img = None
                        for cand in candidates:
                            if not cand:
                                continue
                            cand_low = cand.lower()
                            if cand_low in available_images:
                                matched_img = available_images[cand_low]
                                break
                        if matched_img:
                            full_path = os.path.join(tests_dir, matched_img)
                            f["preview_path"] = f"file://{full_path}"
                            loaded_count += 1
                        else:
                            f["preview_path"] = ""
                    print(f"[Thumbnails] Loaded {loaded_count} effect thumbnails from {tests_dir}", flush=True)
                except Exception as e:
                    print(f"[Warning] Failed scanning preview images: {e}", file=sys.stderr)
            return json.dumps(data)
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

    @Property(str, notify=postRenderActionChanged)
    def postRenderAction(self):
        return self._post_render_action

    @Slot(str)
    def setPostRenderAction(self, action):
        if action in ("none", "shutdown") and action != self._post_render_action:
            self._post_render_action = action
            self.postRenderActionChanged.emit(action)

    def _trigger_shutdown(self):
        print("[System] All rendering tasks completed! Triggering system shutdown...", flush=True)
        for cmd in (["systemctl", "poweroff"], ["shutdown", "-h", "now"], ["poweroff"]):
            try:
                res = subprocess.run(cmd, capture_output=True, timeout=5)
                if res.returncode == 0:
                    print(f"[System] Shutdown executed successfully: {' '.join(cmd)}", flush=True)
                    break
            except Exception as e:
                print(f"[System] Failed shutdown attempt ({' '.join(cmd)}): {e}", flush=True)

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

    @Slot(str, str, str, str, str, result=str)
    def savePresetFile(self, filter_id, filter_name, base_values_json, tweens_json, suggested_name=""):
        """
        Saves current filter settings, values, and animation curves into a .geffect JSON file.
        """
        try:
            base_values = json.loads(base_values_json) if base_values_json else {}
            tweens = json.loads(tweens_json) if tweens_json else {}

            preset_data = {
                "format_version": "1.0",
                "generator": "GmicVideo",
                "created_at": time.strftime("%Y-%m-%d %H:%M:%S"),
                "filter_id": filter_id,
                "filter_name": filter_name,
                "base_values": base_values,
                "tweens": tweens
            }

            sugg = suggested_name if suggested_name else f"{filter_id}.geffect"
            if not sugg.endswith(".geffect"):
                sugg += ".geffect"

            default_dir = os.path.expanduser("~")
            file_path, _ = QFileDialog.getSaveFileName(
                None,
                "Save Effect Preset (.geffect)",
                os.path.join(default_dir, sugg),
                "G'MIC Video Effect Preset (*.geffect);;JSON Files (*.json);;All Files (*.*)"
            )

            if not file_path:
                return ""

            if not file_path.endswith(".geffect") and not file_path.endswith(".json"):
                file_path += ".geffect"

            with open(file_path, "w", encoding="utf-8") as f:
                json.dump(preset_data, f, indent=2, ensure_ascii=False)

            return file_path
        except Exception as e:
            print(f"[Error] Failed to save .geffect: {e}", file=sys.stderr)
            return ""

    @Slot(result=str)
    def loadPresetFile(self):
        """
        Prompts user to select a .geffect file, performs thorough validation, and returns the JSON payload.
        """
        try:
            file_path, _ = QFileDialog.getOpenFileName(
                None,
                "Load Effect Preset (.geffect)",
                os.path.expanduser("~"),
                "G'MIC Video Effect Preset (*.geffect);;JSON Files (*.json);;All Files (*.*)"
            )

            if not file_path:
                return ""

            with open(file_path, "r", encoding="utf-8") as f:
                data = json.load(f)

            # Validation steps
            if not isinstance(data, dict):
                raise ValueError("Invalid file: Root must be a JSON object")

            if "filter_id" not in data:
                raise ValueError("Missing required field: 'filter_id'")

            if "base_values" not in data or not isinstance(data["base_values"], dict):
                raise ValueError("Missing or invalid 'base_values' object")

            if "tweens" not in data or not isinstance(data["tweens"], dict):
                raise ValueError("Missing or invalid 'tweens' object")

            # Validate curve points and normalized percentages if present
            for pid, tw in data["tweens"].items():
                if not isinstance(tw, dict):
                    continue
                c_pts = tw.get("curve_points")
                if c_pts and isinstance(c_pts, list):
                    for pt in c_pts:
                        if isinstance(pt, (list, tuple)) and len(pt) >= 2:
                            t_pct, v_pct = pt[0], pt[1]
                            # clamp percentage in [0.0, 1.0]
                            pt[0] = max(0.0, min(1.0, float(t_pct)))

            data["_file_path"] = file_path
            return json.dumps(data)
        except Exception as e:
            print(f"[Error] Failed to load .geffect: {e}", file=sys.stderr)
            return json.dumps({"error": str(e)})

    @Slot(str)
    def openFile(self, file_path):
        clean_path = file_path.replace("file://", "")
        if os.path.exists(clean_path):
            subprocess.Popen(["xdg-open", clean_path])

def main():
    app = QApplication(sys.argv)
    app.setApplicationName("GmicVideo")
    app.setOrganizationName("KDE")

    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    # Load custom application icon
    icon_candidates = [
        os.path.join(base_dir, "assets", "icon", "app_icon.png"),
        os.path.join(base_dir, "assets", "icon", "icon.png"),
        os.path.join(base_dir, "assets", "icon", "app_icon.svg"),
        os.path.join(base_dir, "assets", "icon", "icon.svg"),
        os.path.join(base_dir, "assets", "icons", "app_icon.png"),
        os.path.join(base_dir, "assets", "icons", "icon.png"),
    ]
    for ic_path in icon_candidates:
        if os.path.exists(ic_path):
            app.setWindowIcon(QIcon(ic_path))
            break

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
    i18n_manager = I18nManager()
    engine.rootContext().setContextProperty("Backend", bridge)
    engine.rootContext().setContextProperty("i18n", i18n_manager)

    qml_file = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "ui", "main.qml")
    engine.load(QUrl.fromLocalFile(qml_file))

    if not engine.rootObjects():
        print("[Error] Failed to load main.qml", file=sys.stderr)
        sys.exit(1)

    if "--test" in sys.argv:
        print("[Success] GmicVideo GUI initialized and verified successfully!")
        sys.exit(0)

    sys.exit(app.exec())

if __name__ == "__main__":
    main()
