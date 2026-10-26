"""Real-time playback bridge between the Rust playback engine and QML.

Frames arrive on a dedicated pipe (see src/frame_sink.rs) and are pushed into QML
`VideoOutput` items through their `QVideoSink`. Audio is played by `QMediaPlayer`;
the engine starts its clock when the first frame is ready, the audio starts at that
moment, and the audio position is sent back every second to correct drift.
"""

import json
import os
import struct
import sys
import threading

from PySide6.QtCore import QObject, Property, QSize, QTimer, QUrl, Signal, Slot
from PySide6.QtMultimedia import (
    QAudioOutput,
    QMediaPlayer,
    QVideoFrame,
    QVideoFrameFormat,
    QVideoSink,
)

FRAME_MAGIC = 0x52465647
HEADER = struct.Struct("<IIIIQd")  # magic, kind, width, height, seq, pts
KIND_EFFECT = 0
KIND_ORIGINAL = 1


def _read_exact(f, view):
    got = 0
    total = len(view)
    while got < total:
        n = f.readinto(view[got:])
        if not n:
            return False
        got += n
    return True


def read_frames(fd, on_frame):
    """Reads frame messages from `fd` until EOF, calling on_frame(kind, w, h, seq, pts, data)."""
    with os.fdopen(fd, "rb", buffering=0) as f:
        header = bytearray(HEADER.size)
        while _read_exact(f, memoryview(header)):
            magic, kind, w, h, seq, pts = HEADER.unpack(header)
            if magic != FRAME_MAGIC or w <= 0 or h <= 0 or w * h > 64_000_000:
                print("[Playback] Corrupt frame stream, stopping reader", file=sys.stderr)
                return
            data = bytearray(w * h * 4)
            if not _read_exact(f, memoryview(data)):
                return
            try:
                on_frame(kind, w, h, seq, pts, data)
            except Exception as e:
                print(f"[Playback] Frame handler error: {e}", file=sys.stderr)


def make_video_frame(w, h, data):
    fmt = QVideoFrameFormat(QSize(w, h), QVideoFrameFormat.PixelFormat.Format_RGBX8888)
    frame = QVideoFrame(fmt)
    if not frame.map(QVideoFrame.MapMode.WriteOnly):
        return None
    try:
        bits = frame.bits(0)
        stride = frame.bytesPerLine(0)
        row = w * 4
        if stride == row:
            bits[: row * h] = data
        else:
            src = memoryview(data)
            for y in range(h):
                bits[y * stride : y * stride + row] = src[y * row : (y + 1) * row]
    finally:
        frame.unmap()
    return frame


class PlaybackBridge(QObject):
    playingChanged = Signal()
    positionChanged = Signal()
    statusChanged = Signal()
    mutedChanged = Signal()
    stillReady = Signal(str)      # json info about a rendered still frame
    errorOccurred = Signal(str)
    _frameReady = Signal(int)     # internal: hop from reader thread to GUI thread
    _eventReceived = Signal(str, str)

    def __init__(self, client=None, parent=None):
        super().__init__(parent)
        self.client = client
        self._sinks = {KIND_EFFECT: None, KIND_ORIGINAL: None}
        self._latest = {}
        self._pending = {KIND_EFFECT: False, KIND_ORIGINAL: False}
        self._lock = threading.Lock()
        self._playing = False
        self._position = 0.0
        self._status = {}
        self._loop = True
        self._cut = (None, None)
        self._video_path = ""
        self.frames_delivered = {KIND_EFFECT: 0, KIND_ORIGINAL: 0}  # for tests/diagnostics

        self._audio_out = QAudioOutput(self)
        self._player = QMediaPlayer(self)
        self._player.setAudioOutput(self._audio_out)
        self._player.mediaStatusChanged.connect(self._on_media_status)

        self._sync_timer = QTimer(self)
        self._sync_timer.setInterval(1000)
        self._sync_timer.timeout.connect(self._sync_audio)

        self._frameReady.connect(self._deliver_frame)
        self._eventReceived.connect(self._handle_event)

    # ------------------------------------------------------------ wiring

    def handle_frame(self, kind, w, h, seq, pts, data):
        """Called on the frame reader thread."""
        frame = make_video_frame(w, h, data)
        if frame is None:
            return
        with self._lock:
            self._latest[kind] = (frame, pts)
            if self._pending.get(kind):
                return  # the GUI thread will pick up the newest frame
            self._pending[kind] = True
        self._frameReady.emit(kind)

    def handle_event(self, name, data):
        """Called on the backend reader thread."""
        if name and name.startswith("playback_"):
            self._eventReceived.emit(name, json.dumps(data or {}))

    @Slot(int)
    def _deliver_frame(self, kind):
        with self._lock:
            item = self._latest.pop(kind, None)
            self._pending[kind] = False
        if item is None:
            return
        frame, pts = item
        sink = self._sinks.get(kind)
        if sink is not None:
            sink.setVideoFrame(frame)
            self.frames_delivered[kind] = self.frames_delivered.get(kind, 0) + 1
        if kind == KIND_EFFECT and abs(pts - self._position) > 1e-6:
            self._position = pts
            self.positionChanged.emit()

    @Slot(str, str)
    def _handle_event(self, name, data_json):
        try:
            data = json.loads(data_json)
        except ValueError:
            data = {}
        if name == "playback_state":
            playing = bool(data.get("playing"))
            self._status.update({k: data.get(k) for k in ("scale", "auto", "width", "height", "mode")})
            if not playing and self._playing:
                self._set_playing(False)
                self._player.pause()
            self.statusChanged.emit()
        elif name == "playback_stats":
            self._status.update(data)
            self.statusChanged.emit()
        elif name == "playback_still":
            self._status.update({k: data.get(k) for k in ("width", "height", "scale", "elapsed_ms", "mode")})
            self._status["fps"] = None
            self.statusChanged.emit()
            self.stillReady.emit(data_json)
        elif name == "playback_clock_started":
            if self._playing:
                self._player.setPosition(int(float(data.get("position", 0.0)) * 1000))
                self._player.play()
        elif name == "playback_ended":
            self._player.pause()
            self._set_playing(False)
        elif name == "playback_looped":
            self._player.pause()  # resumes in sync on playback_clock_started
        elif name == "playback_error":
            self.errorOccurred.emit(str(data.get("message", "")))

    def _send(self, action, payload=None):
        if self.client:
            self.client.request_async(action, payload, lambda res: None)

    def _set_playing(self, value):
        if self._playing != value:
            self._playing = value
            if value:
                self._sync_timer.start()
            else:
                self._sync_timer.stop()
            self.playingChanged.emit()

    def _on_media_status(self, status):
        if status == QMediaPlayer.MediaStatus.LoadedMedia:
            # Audio only: the engine draws the video.
            try:
                self._player.setActiveVideoTrack(-1)
            except Exception:
                pass

    def _sync_audio(self):
        if self._playing and self._player.playbackState() == QMediaPlayer.PlaybackState.PlayingState and self._player.hasAudio():
            self._send("playback_sync", {"secs": self._player.position() / 1000.0})

    # ------------------------------------------------------------ QML API

    @Slot(QObject)
    def setEffectSink(self, sink):
        self._sinks[KIND_EFFECT] = sink if isinstance(sink, QVideoSink) else None

    @Slot(QObject)
    def setOriginalSink(self, sink):
        self._sinks[KIND_ORIGINAL] = sink if isinstance(sink, QVideoSink) else None

    @Slot(str)
    def open(self, path):
        if path.startswith("file:"):
            path = QUrl(path).toLocalFile()
        self._video_path = path
        self._set_playing(False)
        self._player.stop()
        self._player.setSource(QUrl.fromLocalFile(path))
        self._position = 0.0
        self.positionChanged.emit()
        self._send("playback_open", {"path": path})
        self._send("playback_set_loop", {"enabled": self._loop})

    @Slot(str)
    def configure(self, config_json):
        """config: {effect: {filter_id, base_values, tweens, text_values} | null,
        cut_start_secs, cut_end_secs, scale (0 = auto), show_original}"""
        try:
            cfg = json.loads(config_json) if config_json else {}
        except ValueError as e:
            self.errorOccurred.emit(str(e))
            return
        eff = cfg.get("effect")
        if eff:
            eff["base_values"] = {str(int(k)): float(v) for k, v in (eff.get("base_values") or {}).items() if v is not None}
            eff["tweens"] = {str(int(k)): v for k, v in (eff.get("tweens") or {}).items()}
            eff["text_values"] = {str(int(k)): str(v) for k, v in (eff.get("text_values") or {}).items()}
        for key in ("cut_start_secs", "cut_end_secs"):
            v = cfg.get(key)
            cfg[key] = float(v) if v is not None and float(v) > 0 else None
        self._cut = (cfg.get("cut_start_secs"), cfg.get("cut_end_secs"))
        cfg["scale"] = float(cfg.get("scale") or 0.0)
        cfg["show_original"] = bool(cfg.get("show_original"))
        self._send("playback_configure", cfg)

    @Slot()
    def play(self):
        if self._playing or not self._video_path:
            return
        self._set_playing(True)
        self._send("playback_play", {"from_secs": self._position})

    @Slot()
    def pause(self):
        if not self._playing:
            return
        self._set_playing(False)
        self._player.pause()
        self._send("playback_pause")

    @Slot()
    def togglePlay(self):
        if self._playing:
            self.pause()
        else:
            self.play()

    @Slot(float)
    def seek(self, secs):
        self._position = max(0.0, float(secs))
        self.positionChanged.emit()
        if self._playing:
            self._player.pause()  # resumes when the engine shows the first new frame
        self._send("playback_seek", {"secs": self._position})

    @Property(bool, notify=playingChanged)
    def playing(self):
        return self._playing

    @Property(float, notify=positionChanged)
    def position(self):
        return self._position

    @Property(bool, notify=mutedChanged)
    def muted(self):
        return self._audio_out.isMuted()

    @muted.setter
    def muted(self, value):
        if self._audio_out.isMuted() != bool(value):
            self._audio_out.setMuted(bool(value))
            self.mutedChanged.emit()

    @Property(bool, notify=statusChanged)
    def looping(self):
        return self._loop

    @Slot(bool)
    def setLooping(self, value):
        self._loop = bool(value)
        self._send("playback_set_loop", {"enabled": self._loop})
        self.statusChanged.emit()

    @Property(str, notify=statusChanged)
    def statusJson(self):
        return json.dumps(self._status)
