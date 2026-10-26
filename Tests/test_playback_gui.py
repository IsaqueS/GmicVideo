#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
End-to-end GUI playback test (runs the real QML UI offscreen):
1. Loading a video shows a still frame in the VideoOutput.
2. Play delivers frames continuously, in real time, and the timeline advances.
3. Changing an effect parameter while playing keeps playback running.
4. Pause stops frame delivery.
"""

import os
import subprocess
import sys
import tempfile
import time
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(REPO_ROOT, "src_gui"))

from PySide6.QtCore import QCoreApplication, QMetaObject, Q_ARG  # noqa: E402
from gmicvideo_gui import create_app  # noqa: E402


def pump(seconds, until=None):
    end = time.time() + seconds
    while time.time() < end:
        QCoreApplication.processEvents()
        if until and until():
            return True
        time.sleep(0.01)
    return until() if until else True


class TestPlaybackGui(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(prefix="gmic_playback_")
        cls.clip = os.path.join(cls.tmp.name, "clip.mp4")
        subprocess.run(
            ["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=1280x720:r=30:d=6",
             "-f", "lavfi", "-i", "sine=f=440:d=6", "-shortest", "-pix_fmt", "yuv420p", "-c:a", "aac", cls.clip],
            check=True,
        )
        cls.app, cls.engine, cls.bridge, cls.playback, _ = create_app([sys.argv[0]])
        cls.root = cls.engine.rootObjects()[0]

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def call(self, name, *args):
        QMetaObject.invokeMethod(self.root, name, *[Q_ARG("QVariant", a) for a in args])

    def test_01_load_shows_still(self):
        self.call("loadVideo", self.clip)
        ok = pump(20, lambda: self.playback.frames_delivered[0] >= 1)
        self.assertTrue(ok, "a still frame must be shown after loading")
        self.assertFalse(self.root.property("isPreviewLoading"))

    def test_02_play_is_real_time(self):
        before = self.playback.frames_delivered[0]
        self.playback.play()
        pump(2.5)  # warm-up: first frames + automatic resolution settling
        t0, f0, p0 = time.time(), self.playback.frames_delivered[0], self.playback.position
        pump(3.0)
        dt = time.time() - t0
        fps = (self.playback.frames_delivered[0] - f0) / dt
        advanced = self.playback.position - p0
        print(f"\n[playback] {fps:.1f} fps delivered, position advanced {advanced:.2f}s in {dt:.2f}s", flush=True)
        print(f"[playback] engine status: {self.playback.statusJson} filter={self.root.property('selectedFilter').toVariant().get('name') if hasattr(self.root.property('selectedFilter'), 'toVariant') else ''}", flush=True)
        self.assertTrue(self.playback.playing)
        self.assertGreater(self.playback.frames_delivered[0], before + 10)
        self.assertGreater(advanced, dt * 0.6, "timeline must advance in (near) real time")

    def test_03_change_parameter_while_playing(self):
        self.assertTrue(self.playback.playing)
        vals = dict(self.root.property("paramValues").toVariant() if hasattr(self.root.property("paramValues"), "toVariant") else self.root.property("paramValues"))
        if vals:
            k = next(iter(vals))
            vals[k] = float(vals[k]) + 1.0
            self.root.setProperty("paramValues", vals)
            self.call("requestPreviewUpdate")
        f0 = self.playback.frames_delivered[0]
        pump(1.5)
        self.assertTrue(self.playback.playing)
        self.assertGreater(self.playback.frames_delivered[0], f0)

    def test_04_pause_stops_frames(self):
        self.playback.pause()
        pump(0.5)
        f0 = self.playback.frames_delivered[0]
        pump(1.0)
        self.assertFalse(self.playback.playing)
        self.assertLessEqual(self.playback.frames_delivered[0] - f0, 1)


if __name__ == "__main__":
    unittest.main()
