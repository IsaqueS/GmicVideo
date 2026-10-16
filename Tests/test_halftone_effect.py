#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Automated end-to-end tests for the Halftone effects (fx_halftone, fx_generic_halftone, iain_halftone_shapes):
1. Validates that the G'MIC parser correctly parses curly-brace choice{...} and unindexed choice("...") parameters.
2. Tests single-frame preview rendering (render_preview) for Halftone across all 6 Shape modes,
   plus Halftone [Generic] and Halftone Shapes.
3. Tests full streaming video rendering (queueRender) with the Halftone effect.
4. Tests 10-bit video (yuv420p10le) and 16-bit PNG normalization so Halftone never clamps to 255 or turns solid red.
"""

import os
import sys
import json
import time
import tempfile
import subprocess
import unittest
from PySide6.QtGui import QImage

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(REPO_ROOT, "src_gui"))

from gmicvideo_gui import RustBackendBridge


class TestHalftoneEffect(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.repo_root = REPO_ROOT
        bin_cand = [
            os.path.join(cls.repo_root, "target", "debug", "GmicVideo"),
            os.path.join(cls.repo_root, "target", "release", "GmicVideo"),
            os.path.join(cls.repo_root, "target", "debug", "gmicvideo"),
            os.path.join(cls.repo_root, "target", "release", "gmicvideo"),
        ]
        cls.backend_bin = None
        for b in bin_cand:
            if os.path.isfile(b) and os.access(b, os.X_OK):
                cls.backend_bin = b
                break
        assert cls.backend_bin is not None, "Could not find compiled GmicVideo backend binary."
        cls.bridge = RustBackendBridge(cls.backend_bin)
        cls.test_image = os.path.join(cls.repo_root, "Tests", "test_image.jpeg")
        assert os.path.isfile(cls.test_image), f"Missing test image: {cls.test_image}"

    @staticmethod
    def _value_params(filter_obj):
        return [
            p for p in filter_obj.get("parameters", [])
            if p.get("param_type", {}).get("type", "").lower() not in ("separator", "note")
        ]

    def test_01_halftone_parameter_parsing(self):
        """Verify Halftone (fx_halftone) has all 9 value parameters including curly-brace Shape=choice{5,...}."""
        raw_json = self.bridge.getEffects()
        data = json.loads(raw_json)
        filters = data.get("filters", [])

        halftone = next((f for f in filters if f.get("command") == "fx_halftone"), None)
        self.assertIsNotNone(halftone, "fx_halftone must exist in filter database.")
        val_params = self._value_params(halftone)
        self.assertEqual(
            len(val_params),
            9,
            f"fx_halftone must have 9 value parameters (got {len(val_params)}): {[p.get('name') for p in val_params]}"
        )

        # Value parameter index 7 is Shape=choice{5,"Square","Diamond","Circle","Square (Inv.)","Diamond (Inv.)","Circle (Inv.)"}
        shape_param = val_params[7]
        self.assertEqual(shape_param.get("name"), "Shape")
        ptype = shape_param.get("param_type", {})
        self.assertEqual(ptype.get("type", "").lower(), "choice")
        pconfig = ptype.get("data", ptype)
        self.assertEqual(pconfig.get("default"), 5)
        self.assertEqual(
            pconfig.get("options"),
            ["Square", "Diamond", "Circle", "Square (Inv.)", "Diamond (Inv.)", "Circle (Inv.)"]
        )

        # Also check Halftone [Generic] (fx_generic_halftone) and Halftone Shapes (iain_halftone_shapes)
        generic_ht = next((f for f in filters if f.get("command") == "fx_generic_halftone"), None)
        self.assertIsNotNone(generic_ht, "fx_generic_halftone must exist in filter database.")
        self.assertEqual(len(self._value_params(generic_ht)), 12)

        shapes_ht = next((f for f in filters if f.get("command") == "iain_halftone_shapes"), None)
        self.assertIsNotNone(shapes_ht, "iain_halftone_shapes must exist in filter database.")
        self.assertEqual(len(self._value_params(shapes_ht)), 7)

    def test_02_halftone_preview_render_all_shapes(self):
        """Verify render_preview succeeds for fx_halftone across all 6 Shape options."""
        data = json.loads(self.bridge.getEffects())
        halftone = next(f for f in data["filters"] if f.get("command") == "fx_halftone")
        shape_param_id = next(p["id"] for p in halftone["parameters"] if p.get("name") == "Shape")

        for shape_idx in range(6):
            res = self.bridge._send_request(
                "render_preview",
                {
                    "video_path": self.test_image,
                    "time_secs": 0.0,
                    "duration_secs": 1.0,
                    "filter_id": "patterns_halftone",
                    "base_values": {str(shape_param_id): float(shape_idx)},
                    "tweens": {},
                },
            )
            self.assertEqual(
                res.get("status"),
                "ok",
                f"Halftone preview render failed for shape={shape_idx}: {res.get('data')}"
            )
            out_data = res.get("data", {})
            eff_preview = out_data.get("effect_preview", "")
            self.assertTrue(os.path.isfile(eff_preview), f"Output preview missing for shape={shape_idx}")
            self.assertGreater(os.path.getsize(eff_preview), 1000, "Output preview PNG is unexpectedly small")

    def test_03_halftone_generic_and_shapes_preview_render(self):
        """Verify render_preview succeeds for Halftone [Generic] and Halftone Shapes."""
        for filter_id in ["patterns_halftone__generic_", "patterns_halftone_shapes"]:
            res = self.bridge._send_request(
                "render_preview",
                {
                    "video_path": self.test_image,
                    "time_secs": 0.0,
                    "duration_secs": 1.0,
                    "filter_id": filter_id,
                    "base_values": {},
                    "tweens": {},
                },
            )
            self.assertEqual(
                res.get("status"),
                "ok",
                f"Preview render failed for {filter_id}: {res.get('data')}"
            )
            eff_preview = res.get("data", {}).get("effect_preview", "")
            self.assertTrue(os.path.isfile(eff_preview))
            self.assertGreater(os.path.getsize(eff_preview), 1000)

    def test_04_halftone_video_render_pipeline(self):
        """Verify full streaming video render pipeline with Halftone effect succeeds."""
        with tempfile.TemporaryDirectory(prefix="gmic_halftone_vid_") as tmpdir:
            in_video = os.path.join(tmpdir, "input.mp4")
            out_video = os.path.join(tmpdir, "output_halftone.mp4")

            # Generate a short 2-frame 160x120 video from test_image.jpeg
            cmd = [
                "ffmpeg", "-y", "-loop", "1", "-i", self.test_image,
                "-vf", "scale=160:120", "-r", "10", "-frames:v", "2",
                "-c:v", "libx264", "-pix_fmt", "yuv420p", in_video
            ]
            subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            self.assertTrue(os.path.isfile(in_video))

            job_id = self.bridge.queueRender(
                in_video,
                out_video,
                "h264",
                5,
                10,
                "patterns_halftone",
                "Halftone",
                "{}",
                "{}",
                0.0,
                -1.0,
            )
            self.assertTrue(job_id, "queueRender returned empty job_id for Halftone render")

            # Poll render queue until job finishes
            deadline = time.time() + 30.0
            final_item = None
            while time.time() < deadline:
                q_res = self.bridge._send_request("get_render_queue")
                items = q_res.get("data", [])
                item = next((it for it in items if it.get("id") == job_id), None)
                if item and item.get("status") in ("completed", "failed", "canceled"):
                    final_item = item
                    break
                time.sleep(0.25)

            self.assertIsNotNone(final_item, "Halftone video render timed out")
            self.assertEqual(
                final_item.get("status"),
                "completed",
                f"Halftone video render failed: {final_item.get('error')}"
            )
            self.assertTrue(os.path.isfile(out_video), "Rendered Halftone video file does not exist")
            self.assertGreater(os.path.getsize(out_video), 500, "Rendered Halftone video is empty")

    def test_05_halftone_10bit_video_and_color_fidelity(self):
        """Verify 10-bit video (yuv420p10le) does not produce 16-bit clamped red grid in Halftone."""
        with tempfile.TemporaryDirectory(prefix="gmic_halftone_10bit_") as tmpdir:
            in_video_10bit = os.path.join(tmpdir, "input_10bit.mp4")

            # Create a 10-bit yuv420p10le video from test_image.jpeg
            cmd = [
                "ffmpeg", "-y", "-loop", "1", "-i", self.test_image,
                "-vf", "scale=320:240", "-r", "10", "-frames:v", "2",
                "-c:v", "libx264", "-pix_fmt", "yuv420p10le", in_video_10bit
            ]
            subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            self.assertTrue(os.path.isfile(in_video_10bit))

            res = self.bridge._send_request(
                "render_preview",
                {
                    "video_path": in_video_10bit,
                    "time_secs": 0.0,
                    "duration_secs": 1.0,
                    "filter_id": "patterns_halftone",
                    "base_values": {},
                    "tweens": {},
                },
            )
            self.assertEqual(res.get("status"), "ok", f"10-bit preview failed: {res}")
            raw_preview = res.get("data", {}).get("raw_preview", "")
            eff_preview = res.get("data", {}).get("effect_preview", "")

            raw_img = QImage(raw_preview)
            eff_img = QImage(eff_preview)
            self.assertFalse(raw_img.isNull(), "Failed to load raw_preview")
            self.assertFalse(eff_img.isNull(), "Failed to load eff_preview")

            # Compute average R, G, B across the image and verify it matches the source image
            # and does NOT degenerate into the broken red grid (R=255, G=39.7, B=39.7)
            w, h = eff_img.width(), eff_img.height()
            r_sum = g_sum = b_sum = 0
            raw_r = raw_g = raw_b = 0
            count = 0
            for y in range(0, h, 4):
                for x in range(0, w, 4):
                    c = eff_img.pixelColor(x, y)
                    rc = raw_img.pixelColor(x, y)
                    r_sum += c.red()
                    g_sum += c.green()
                    b_sum += c.blue()
                    raw_r += rc.red()
                    raw_g += rc.green()
                    raw_b += rc.blue()
                    count += 1

            r_avg, g_avg, b_avg = r_sum / count, g_sum / count, b_sum / count
            raw_r_avg, raw_g_avg, raw_b_avg = raw_r / count, raw_g / count, raw_b / count

            # Red channel must not be blown out to 255 while G/B collapse
            self.assertLess(r_avg, 240.0, f"Red channel blown out on 10-bit video: R={r_avg}, G={g_avg}, B={b_avg}")
            # Channel averages of Halftone must stay within 35 levels of the original image's channel averages
            self.assertLess(abs(r_avg - raw_r_avg), 35.0, f"R average {r_avg} diverged from raw {raw_r_avg}")
            self.assertLess(abs(g_avg - raw_g_avg), 35.0, f"G average {g_avg} diverged from raw {raw_g_avg}")
            self.assertLess(abs(b_avg - raw_b_avg), 35.0, f"B average {b_avg} diverged from raw {raw_b_avg}")


if __name__ == "__main__":
    unittest.main()
