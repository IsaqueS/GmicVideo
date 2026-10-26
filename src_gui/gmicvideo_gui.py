#!/usr/bin/env python3
import sys
import os
import json
import subprocess
import threading
import time
from PySide6.QtCore import QObject, Slot, Signal, Property, QUrl, QStandardPaths
from PySide6.QtWidgets import QApplication, QFileDialog
from PySide6.QtGui import QIcon
from PySide6.QtQml import QQmlApplicationEngine
from i18n import I18nManager
from playback import PlaybackBridge, read_frames


IMAGE_EXTS = (".jpg", ".png")


def to_local_path(path):
    """Converts a QML URL (file:///home/me/My%20Video.mp4) or plain path to a local path."""
    if not path:
        return ""
    if path.startswith("file:"):
        return QUrl(path).toLocalFile()
    return path


class BackendClient:
    """JSON-over-stdio client for the Rust backend.

    Every request carries an id; a reader thread routes each response to whoever is
    waiting for it, so a slow request (a heavy preview) never blocks fast ones
    (queue polling, probing) and the GUI thread never waits behind a preview.
    """

    def __init__(self, backend_bin_path, on_event=None, on_frame=None):
        self.backend_bin_path = backend_bin_path
        self.on_event = on_event
        self.on_frame = on_frame
        self.process = None
        self._write_lock = threading.Lock()
        self._state_lock = threading.Lock()
        self._next_id = 1
        self._pending = {}  # id -> (event, result_holder, callback)
        self._start()

    def _start(self):
        # Separate pipe for video frames (keeps the JSON channel responsive).
        frame_r, frame_w = os.pipe()
        try:
            import fcntl
            fcntl.fcntl(frame_w, 1031, 1 << 20)  # F_SETPIPE_SZ: 1 MiB buffer
        except Exception:
            pass
        env = dict(os.environ, GMIC_VIDEO_FRAME_FD=str(frame_w))
        try:
            self.process = subprocess.Popen(
                [self.backend_bin_path, "--server"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=None,  # backend diagnostics go to our stderr
                text=True,
                encoding="utf-8",
                bufsize=1,
                env=env,
                pass_fds=(frame_w,),
            )
        except Exception as e:
            print(f"[Error] Failed to spawn Rust backend binary: {e}", file=sys.stderr)
            self.process = None
            os.close(frame_r)
            os.close(frame_w)
            return
        os.close(frame_w)
        proc = self.process
        threading.Thread(target=self._reader, args=(proc,), daemon=True).start()
        threading.Thread(target=read_frames, args=(frame_r, self._dispatch_frame), daemon=True).start()

    def _dispatch_frame(self, kind, w, h, seq, pts, data):
        if self.on_frame:
            self.on_frame(kind, w, h, seq, pts, data)

    def _reader(self, proc):
        for line in proc.stdout:
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                msg = json.loads(line)
            except ValueError:
                continue
            if "event" in msg:
                if self.on_event:
                    try:
                        self.on_event(msg.get("event"), msg.get("data"))
                    except Exception as e:
                        print(f"[Event Error] {e}", file=sys.stderr)
                continue
            req_id = msg.pop("id", None)
            with self._state_lock:
                if req_id is None and self._pending:
                    # Backend without request ids: answers arrive in order.
                    req_id = min(self._pending)
                entry = self._pending.pop(req_id, None)
            if entry:
                self._complete(entry, msg)
        # Backend exited: fail everything still waiting.
        with self._state_lock:
            stale = [k for k, v in self._pending.items() if v[3] is proc]
            entries = [self._pending.pop(k) for k in stale]
        for entry in entries:
            self._complete(entry, {"status": "error", "data": "Backend process exited"})

    @staticmethod
    def _complete(entry, msg):
        event, holder, callback, _proc = entry
        holder.append(msg)
        event.set()
        if callback:
            try:
                callback(msg)
            except Exception as e:
                print(f"[Callback Error] {e}", file=sys.stderr)

    def _submit(self, action, payload, callback):
        if not self.process or self.process.poll() is not None:
            self._start()
        if not self.process:
            msg = {"status": "error", "data": "Backend is not running"}
            if callback:
                callback(msg)
            return None, [msg]

        event, holder = threading.Event(), []
        with self._state_lock:
            req_id = self._next_id
            self._next_id += 1
            self._pending[req_id] = (event, holder, callback, self.process)
        req = {"id": req_id, "action": action}
        if payload is not None:
            req["payload"] = payload
        try:
            with self._write_lock:
                self.process.stdin.write(json.dumps(req) + "\n")
                self.process.stdin.flush()
        except Exception as e:
            with self._state_lock:
                entry = self._pending.pop(req_id, None)
            if entry:
                self._complete(entry, {"status": "error", "data": str(e)})
        return event, holder

    def request(self, action, payload=None, timeout=None):
        """Blocking request; returns the response dict."""
        event, holder = self._submit(action, payload, None)
        if event is None:
            return holder[0]
        if not event.wait(timeout):
            return {"status": "error", "data": f"Backend did not answer '{action}' in time"}
        return holder[0]

    def request_async(self, action, payload, callback):
        """Non-blocking request; `callback(response)` runs on the reader thread."""
        self._submit(action, payload, callback)


class RustBackendBridge(QObject):
    previewReady = Signal(str, str, float)  # raw_path, effect_path, time_secs
    previewInfo = Signal(str)               # json: width, height, scale, elapsed_ms
    previewFailed = Signal(str)             # error message
    queueUpdated = Signal(str)              # json array of RenderQueueItems
    renderProgressUpdated = Signal(str)     # legacy json progress
    renderCompleted = Signal(str)           # output path
    renderFailed = Signal(str)              # error message
    effectsLoaded = Signal(str)             # json effects
    postRenderActionChanged = Signal(str)

    def __init__(self, backend_bin_path, i18n_manager=None):
        super().__init__()
        self.backend_bin_path = backend_bin_path
        self._i18n = i18n_manager
        self.client = BackendClient(backend_bin_path)
        self._last_queue_json = ""
        self._completed_notified = set()
        self._failed_notified = set()
        self._post_render_action = "none"  # "none" or "shutdown"
        self._had_active_jobs = False

        # Preview coalescing: at most one preview in flight + the latest pending one.
        self._preview_lock = threading.Lock()
        self._preview_inflight = False
        self._preview_pending = None
        self._preview_seq = 0
        self._preview_shown_seq = 0

        self._start_queue_poller()

    def _t(self, text):
        return self._i18n.t(text) if self._i18n else text

    # Kept for tests and simple callers.
    def _send_request(self, action, payload=None):
        return self.client.request(action, payload)

    def _start_queue_poller(self):
        def poller():
            while True:
                time.sleep(0.25)
                res = self.client.request("get_render_queue", timeout=10)
                if res.get("status") != "ok":
                    continue
                items = res.get("data", [])
                raw_json = json.dumps(items)
                if raw_json != self._last_queue_json:
                    self._last_queue_json = raw_json
                    self.queueUpdated.emit(raw_json)

                active_jobs = [item for item in items if item.get("status") in ("rendering", "queued")]
                if self._had_active_jobs and not active_jobs:
                    self._had_active_jobs = False
                    if self._post_render_action == "shutdown":
                        # Safety check: only shut down if at least one render succeeded.
                        completed_count = sum(1 for item in items if item.get("status") == "completed")
                        failed_count = sum(1 for item in items if item.get("status") == "failed")
                        canceled_count = sum(1 for item in items if item.get("status") == "canceled")
                        if completed_count > 0:
                            print(f"[System] All rendering tasks finished ({completed_count} succeeded, {failed_count} failed). Proceeding with shutdown...", flush=True)
                            self._trigger_shutdown()
                        else:
                            print(f"[System] Shutdown ABORTED: no render succeeded ({failed_count} failed, {canceled_count} canceled).", flush=True)
                elif active_jobs:
                    self._had_active_jobs = True

                for item in items:
                    status = item.get("status")
                    j_id = item.get("id")
                    if status == "rendering":
                        self.renderProgressUpdated.emit(json.dumps(item.get("progress", {})))
                    elif status == "completed" and j_id not in self._completed_notified:
                        self._completed_notified.add(j_id)
                        self.renderCompleted.emit(item.get("output_path", ""))
                    elif status == "failed" and j_id not in self._failed_notified:
                        self._failed_notified.add(j_id)
                        self.renderFailed.emit(item.get("error") or item.get("progress", {}).get("message", ""))

        threading.Thread(target=poller, daemon=True).start()

    @staticmethod
    def find_thumbnails_dir():
        candidates = []
        if "GMIC_VIDEO_THUMBNAILS_DIR" in os.environ:
            candidates.append(os.environ["GMIC_VIDEO_THUMBNAILS_DIR"])
        script_dir = os.path.dirname(os.path.abspath(__file__))
        if "APPDIR" in os.environ:
            candidates.append(os.path.join(os.environ["APPDIR"], "usr", "share", "gmicvideo", "assets", "thumbnails"))
        candidates.append(os.path.join(script_dir, "..", "assets", "thumbnails"))
        candidates.append(os.path.join(script_dir, "..", "share", "gmicvideo", "assets", "thumbnails"))
        candidates.append(os.path.join(os.getcwd(), "assets", "thumbnails"))
        candidates.append("/usr/share/gmicvideo/assets/thumbnails")
        # Legacy location (thumbnails generated by the old test script)
        if "APPDIR" in os.environ:
            candidates.append(os.path.join(os.environ["APPDIR"], "usr", "share", "gmicvideo", "Tests", "valid_effects"))
        candidates.append(os.path.join(script_dir, "..", "Tests", "valid_effects"))
        candidates.append(os.path.join(os.getcwd(), "Tests", "valid_effects"))

        for c in candidates:
            norm = os.path.normpath(c)
            if os.path.isdir(norm):
                try:
                    if any(f.lower().endswith(IMAGE_EXTS) for f in os.listdir(norm)):
                        return norm
                except Exception:
                    pass
        return None

    @staticmethod
    def _clean_id(text):
        return "".join(c.lower() if c.isalnum() else "_" for c in text or "")

    @Slot(result=str)
    def getEffects(self):
        res = self.client.request("get_effects")
        if res.get("status") != "ok":
            return "{}"
        data = res.get("data", {})
        tests_dir = self.find_thumbnails_dir()
        if tests_dir and "filters" in data:
            try:
                available_images = {img.lower(): img for img in os.listdir(tests_dir) if img.lower().endswith(IMAGE_EXTS)}

                manifest_map = {}
                manifest_cand = [
                    os.path.join(tests_dir, "thumbnails.json"),
                    os.path.join(os.path.dirname(tests_dir), "valid_effects.json"),
                    os.path.join(tests_dir, "valid_effects.json"),
                ]
                for m_path in manifest_cand:
                    if not os.path.isfile(m_path):
                        continue
                    try:
                        with open(m_path, "r", encoding="utf-8") as mf:
                            items = json.load(mf)
                        for item in items:
                            if item.get("status") != "valid":
                                continue
                            out_p = os.path.basename(item.get("output_path", "")).lower()
                            if out_p not in available_images:
                                continue
                            i_id = item.get("id", "").lower()
                            i_cmd = item.get("command", "").split()[0].lower() if item.get("command") else ""
                            i_name = self._clean_id(item.get("name", ""))
                            for key in (i_id, i_cmd, i_name):
                                if key:
                                    manifest_map.setdefault(key, available_images[out_p])
                        break
                    except Exception:
                        pass

                loaded_count = 0
                for f in data["filters"]:
                    fid = f.get("id", "").lower()
                    cmd = f.get("command", "").lower()
                    name_clean = self._clean_id(f.get("name", ""))
                    cat_clean = self._clean_id(f.get("category", ""))
                    candidates = [manifest_map.get(fid, ""), manifest_map.get(cmd, ""), manifest_map.get(name_clean, "")]
                    for stem in (fid, f"{cat_clean}_{name_clean}", cmd, name_clean):
                        candidates.extend(stem + ext for ext in IMAGE_EXTS)
                    matched_img = next((available_images[c.lower()] for c in candidates if c and c.lower() in available_images), None)
                    if matched_img:
                        f["preview_path"] = QUrl.fromLocalFile(os.path.join(tests_dir, matched_img)).toString()
                        loaded_count += 1
                    else:
                        f["preview_path"] = ""
                print(f"[Thumbnails] Loaded {loaded_count} effect thumbnails from {tests_dir}", flush=True)
            except Exception as e:
                print(f"[Warning] Failed scanning preview images: {e}", file=sys.stderr)

        if "filters" in data:
            def is_non_effect_item(f):
                cat = f.get("category", "").strip().lower()
                name = f.get("name", "").strip().lower()
                cmd = f.get("command", "").strip().lower()
                if cat in ("about", "sobre", "information", "informações"):
                    return True
                if cmd in ("_none_", "none", "", "fx_unknown", "gui_download_all_data", "gui_layer_info"):
                    return True
                if name == "about" or name.startswith("about ") or name == "sobre" or name.startswith("sobre "):
                    return True
                if name in ("privacy notice", "release notes", "send feedback", "tribute to sébastien", "tribute to sebastien", "what's new?", "whats new?", "download external data"):
                    return True
                return False

            data["filters"] = [f for f in data["filters"] if not is_non_effect_item(f)]
            if "categories" in data:
                active_cats = {f.get("category") for f in data["filters"]}
                data["categories"] = [c for c in data["categories"] if c in active_cats and c.lower() not in ("about", "sobre")]

        return json.dumps(data)

    @Slot(str, result=str)
    def probeVideo(self, path):
        res = self.client.request("probe_video", {"path": to_local_path(path)})
        if res.get("status") == "ok":
            return json.dumps(res.get("data", {}))
        return json.dumps({"error": res.get("data", "Probe failed")})

    # ---------------------------------------------------------------- previews

    @Slot(str)
    def requestPreview(self, request_json):
        """Asynchronous still preview. `request_json` holds video_path, time_secs, filter_id,
        base_values, tweens, text_values, cut_start_secs, cut_end_secs and preview_scale.
        Only the newest request is rendered when several arrive while one is running."""
        try:
            req = json.loads(request_json) if request_json else {}
        except ValueError as e:
            self.previewFailed.emit(str(e))
            return
        req["video_path"] = to_local_path(req.get("video_path", ""))
        req["base_values"] = {str(int(k)): float(v) for k, v in (req.get("base_values") or {}).items() if v is not None}
        req["tweens"] = {str(int(k)): v for k, v in (req.get("tweens") or {}).items()}
        req["text_values"] = {str(int(k)): str(v) for k, v in (req.get("text_values") or {}).items()}
        for key in ("cut_start_secs", "cut_end_secs"):
            if req.get(key) is not None and float(req[key]) <= 0.0:
                req[key] = None

        with self._preview_lock:
            self._preview_seq += 1
            req["seq"] = self._preview_seq
            if self._preview_inflight:
                self._preview_pending = req
                return
            self._preview_inflight = True
        self._send_preview(req)

    def _send_preview(self, req):
        def done(res):
            data = res.get("data", {}) if isinstance(res.get("data"), dict) else {}
            with self._preview_lock:
                next_req, self._preview_pending = self._preview_pending, None
                if next_req is None:
                    self._preview_inflight = False
                # Show every finished result that is newer than what is on screen, so
                # dragging a slider keeps updating instead of waiting for the last request.
                show = res.get("status") == "ok" and not data.get("stale") and data.get("seq", 0) > self._preview_shown_seq
                if show:
                    self._preview_shown_seq = data.get("seq", 0)
            if show:
                self.previewReady.emit(data.get("raw_preview", ""), data.get("effect_preview", ""), float(data.get("time_secs", 0.0)))
                self.previewInfo.emit(json.dumps({k: data.get(k) for k in ("width", "height", "scale", "elapsed_ms", "frame_index")}))
            elif res.get("status") != "ok" and next_req is None:
                print(f"[Preview Error] {res.get('data')}", file=sys.stderr)
                self.previewFailed.emit(str(res.get("data", "")))
            if next_req is not None:
                self._send_preview(next_req)

        self.client.request_async("render_preview", req, done)

    @Slot(str, float, float, str, str, str)
    def renderPreview(self, video_path, time_secs, duration_secs, filter_id, base_values_json, tweens_json):
        """Legacy preview slot (no cut range / text values / resolution)."""
        self.requestPreview(json.dumps({
            "video_path": video_path,
            "time_secs": time_secs,
            "duration_secs": duration_secs,
            "filter_id": filter_id,
            "base_values": json.loads(base_values_json) if base_values_json else {},
            "tweens": json.loads(tweens_json) if tweens_json else {},
        }))

    @Slot(str, list, list, int, result=str)
    def evaluateCurve(self, ease_type, bezier_points, curve_points, sample_count=40):
        b_pts = [float(x) for x in bezier_points] if bezier_points and len(bezier_points) == 4 else None
        c_pts = [[float(p[0]), float(p[1])] for p in curve_points] if curve_points else None
        res = self.client.request("evaluate_curve", {
            "ease_type": ease_type,
            "bezier_points": b_pts,
            "curve_points": c_pts,
            "sample_count": sample_count,
        })
        if res.get("status") == "ok":
            return json.dumps(res.get("data", []))
        return "[]"

    # ---------------------------------------------------------------- rendering

    @staticmethod
    def _quality_to_crf(quality):
        q = max(1, min(10, int(quality)))
        return int(round(32 - (q - 1) * 2.0))  # 1..10 -> CRF 32..14

    @staticmethod
    def _speed_to_preset(speed):
        s = max(1, min(10, int(speed)))
        if s <= 2:
            return "veryslow"
        if s <= 4:
            return "slow"
        if s <= 6:
            return "medium"
        if s <= 8:
            return "fast"
        return "ultrafast"

    @Slot(str, result=str)
    def queueRenderJob(self, job_json):
        """Queues a render from a JSON object with video_path, output_path, format, quality (1..10),
        speed (1..10), filter_id, filter_name, base_values, tweens, text_values, cut_start, cut_end."""
        try:
            job = json.loads(job_json)
        except ValueError as e:
            self.renderFailed.emit(str(e))
            return ""
        clean_in = to_local_path(job.get("video_path", ""))
        clean_out = to_local_path(job.get("output_path", ""))
        cut_start = float(job.get("cut_start") or 0.0)
        cut_end = float(job.get("cut_end") or -1.0)
        filter_name = job.get("filter_name", "")
        job_id = f"job_{int(time.time() * 1000)}"
        payload = {
            "id": job_id,
            "name": os.path.basename(clean_out) if clean_out else f"Render_{filter_name}.mp4",
            "video_path": clean_in,
            "output_path": clean_out,
            "format": str(job.get("format", "h264")).lower(),
            "crf": self._quality_to_crf(job.get("quality", 7)),
            "speed_preset": self._speed_to_preset(job.get("speed", 7)),
            "filter_id": job.get("filter_id", ""),
            "filter_name": filter_name,
            "base_values": {str(int(k)): float(v) for k, v in (job.get("base_values") or {}).items() if v is not None},
            "tweens": {str(int(k)): v for k, v in (job.get("tweens") or {}).items()},
            "text_values": {str(int(k)): str(v) for k, v in (job.get("text_values") or {}).items()},
            "cut_start_secs": cut_start if cut_start > 0.001 else None,
            "cut_end_secs": cut_end if cut_end > 0.001 else None,
        }
        res = self.client.request("queue_render", payload)
        if res.get("status") == "ok":
            self._emit_queue()
            return job_id
        self.renderFailed.emit(str(res.get("data", "Failed to queue render")))
        return ""

    @Slot(str, str, str, int, int, str, str, str, str, float, float, result=str)
    @Slot(str, str, str, int, int, str, str, str, str, result=str)
    def queueRender(self, video_path, output_path, format_str, quality, speed, filter_id, filter_name, base_values_json, tweens_json, cut_start=0.0, cut_end=-1.0):
        """Positional variant of queueRenderJob (kept for compatibility)."""
        return self.queueRenderJob(json.dumps({
            "video_path": video_path,
            "output_path": output_path,
            "format": format_str,
            "quality": quality,
            "speed": speed,
            "filter_id": filter_id,
            "filter_name": filter_name,
            "base_values": json.loads(base_values_json) if base_values_json else {},
            "tweens": json.loads(tweens_json) if tweens_json else {},
            "cut_start": cut_start,
            "cut_end": cut_end,
        }))

    def _emit_queue(self):
        q_res = self.client.request("get_render_queue")
        if q_res.get("status") == "ok":
            self.queueUpdated.emit(json.dumps(q_res.get("data", [])))

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
        res = self.client.request("get_render_queue")
        if res.get("status") == "ok":
            return json.dumps(res.get("data", []))
        return "[]"

    @Slot(str)
    def cancelRenderJob(self, job_id):
        self.client.request("cancel_render_job", {"job_id": job_id})
        self._emit_queue()

    @Slot()
    def clearCompletedJobs(self):
        self.client.request("clear_completed_jobs")
        self._emit_queue()

    @Slot(str, str, str, int, str, str, str)
    def startRender(self, video_path, output_path, format_str, crf, filter_id, base_values_json, tweens_json):
        """Legacy single render entry point."""
        self.queueRender(video_path, output_path, format_str, 7, 6, filter_id, filter_id, base_values_json, tweens_json)

    @Slot()
    def cancelRender(self):
        self.client.request("cancel_render")

    # ---------------------------------------------------------------- files

    @staticmethod
    def _movies_dir():
        movies = QStandardPaths.writableLocation(QStandardPaths.StandardLocation.MoviesLocation)
        if movies and os.path.isdir(movies):
            return movies
        return os.path.expanduser("~")

    @Slot(result=str)
    def chooseVideoFile(self):
        file_path, _ = QFileDialog.getOpenFileName(
            None,
            self._t("Select Video to Edit"),
            self._movies_dir(),
            self._t("Video Files") + " (*.mp4 *.webm *.mkv *.mov *.avi *.flv *.ts *.m4v *.mpg *.mpeg *.wmv);;"
            + self._t("All Files") + " (*)",
        )
        return file_path or ""

    @Slot(str, str, result=str)
    def chooseOutputFile(self, default_name, extension="mp4"):
        default_name = to_local_path(default_name)
        default_path = default_name if os.path.isabs(default_name) else os.path.join(self._movies_dir(), default_name)
        if not os.path.isdir(os.path.dirname(default_path)):
            default_path = os.path.join(self._movies_dir(), os.path.basename(default_name))

        file_path, _ = QFileDialog.getSaveFileName(
            None,
            self._t("Choose Render Destination"),
            default_path,
            self._t("Video") + f" (*.{extension});;" + self._t("All Files") + " (*)",
        )
        if file_path and not os.path.splitext(file_path)[1]:
            file_path += f".{extension}"
        return file_path or ""

    @Slot(str)
    def openFolder(self, file_path):
        clean_path = to_local_path(file_path)
        folder = os.path.dirname(clean_path) if os.path.isfile(clean_path) else clean_path
        if os.path.exists(folder):
            subprocess.Popen(["xdg-open", folder])

    @Slot(str, str, str, str, str, str, result=str)
    @Slot(str, str, str, str, str, result=str)
    def savePresetFile(self, filter_id, filter_name, base_values_json, tweens_json, suggested_name="", text_values_json=""):
        """Saves filter settings, values, text values and animation curves into a .geffect JSON file."""
        try:
            preset_data = {
                "format_version": "1.1",
                "generator": "GmicVideo",
                "created_at": time.strftime("%Y-%m-%d %H:%M:%S"),
                "filter_id": filter_id,
                "filter_name": filter_name,
                "base_values": json.loads(base_values_json) if base_values_json else {},
                "tweens": json.loads(tweens_json) if tweens_json else {},
                "text_values": json.loads(text_values_json) if text_values_json else {},
            }

            sugg = suggested_name if suggested_name else f"{filter_id}.geffect"
            if not sugg.endswith(".geffect"):
                sugg += ".geffect"

            file_path, _ = QFileDialog.getSaveFileName(
                None,
                self._t("Save Effect Preset (.geffect)"),
                os.path.join(os.path.expanduser("~"), sugg),
                self._t("G'MIC Video Effect Preset") + " (*.geffect);;" + self._t("JSON Files") + " (*.json);;"
                + self._t("All Files") + " (*)",
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
        """Prompts for a .geffect file, validates it, and returns the JSON payload."""
        try:
            file_path, _ = QFileDialog.getOpenFileName(
                None,
                self._t("Load Effect Preset (.geffect)"),
                os.path.expanduser("~"),
                self._t("G'MIC Video Effect Preset") + " (*.geffect);;" + self._t("JSON Files") + " (*.json);;"
                + self._t("All Files") + " (*)",
            )
            if not file_path:
                return ""
            return json.dumps(self.parse_preset_file(file_path))
        except Exception as e:
            print(f"[Error] Failed to load .geffect: {e}", file=sys.stderr)
            return json.dumps({"error": str(e)})

    @staticmethod
    def parse_preset_file(file_path):
        with open(file_path, "r", encoding="utf-8") as f:
            data = json.load(f)
        if not isinstance(data, dict):
            raise ValueError("Invalid file: Root must be a JSON object")
        if "filter_id" not in data:
            raise ValueError("Missing required field: 'filter_id'")
        if "base_values" not in data or not isinstance(data["base_values"], dict):
            raise ValueError("Missing or invalid 'base_values' object")
        if "tweens" not in data or not isinstance(data["tweens"], dict):
            raise ValueError("Missing or invalid 'tweens' object")
        if not isinstance(data.get("text_values", {}), dict):
            raise ValueError("Invalid 'text_values' object")
        data.setdefault("text_values", {})

        # Clamp curve keyframe times into [0, 1]
        for tw in data["tweens"].values():
            if not isinstance(tw, dict):
                continue
            c_pts = tw.get("curve_points")
            if c_pts and isinstance(c_pts, list):
                for pt in c_pts:
                    if isinstance(pt, list) and len(pt) >= 2:
                        pt[0] = max(0.0, min(1.0, float(pt[0])))

        data["_file_path"] = file_path
        return data

    @Slot(str)
    def openFile(self, file_path):
        clean_path = to_local_path(file_path)
        if os.path.exists(clean_path):
            subprocess.Popen(["xdg-open", clean_path])


def find_backend_binary(base_dir):
    backend_bin = os.environ.get("GMIC_VIDEO_BACKEND_BIN")
    if backend_bin and os.path.exists(backend_bin):
        return backend_bin
    # Prefer the most recently built backend (release or debug).
    builds = [os.path.join(base_dir, "target", kind, "GmicVideo") for kind in ("release", "debug")]
    builds = [b for b in builds if os.path.exists(b)]
    if builds:
        return max(builds, key=os.path.getmtime)
    return "gmicvideo"


def create_app(argv):
    """Builds the application objects (also used by the GUI tests)."""
    app = QApplication.instance() or QApplication(argv)
    app.setApplicationName("GmicVideo")
    app.setOrganizationName("KDE")

    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    for ic_name in ("app_icon.png", "icon.png", "app_icon.svg", "icon.svg"):
        ic_path = os.path.join(base_dir, "assets", "icon", ic_name)
        if os.path.exists(ic_path):
            app.setWindowIcon(QIcon(ic_path))
            break

    i18n_manager = I18nManager()
    bridge = RustBackendBridge(find_backend_binary(base_dir), i18n_manager)
    playback = PlaybackBridge(bridge.client)
    bridge.client.on_event = playback.handle_event
    bridge.client.on_frame = playback.handle_frame

    engine = QQmlApplicationEngine()
    engine.rootContext().setContextProperty("Backend", bridge)
    engine.rootContext().setContextProperty("Playback", playback)
    engine.rootContext().setContextProperty("i18n", i18n_manager)
    engine.load(QUrl.fromLocalFile(os.path.join(base_dir, "ui", "main.qml")))
    return app, engine, bridge, playback, i18n_manager


def main():
    app, engine, bridge, playback, i18n_manager = create_app(sys.argv)

    if not engine.rootObjects():
        print("[Error] Failed to load main.qml", file=sys.stderr)
        sys.exit(1)

    if "--test" in sys.argv:
        print("[Success] GmicVideo GUI initialized and verified successfully!")
        sys.exit(0)

    sys.exit(app.exec())


if __name__ == "__main__":
    main()
