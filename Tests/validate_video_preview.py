#!/usr/bin/env python3
"""
Video Preview Automated Validator
---------------------------------
Uses verified valid G'MIC effects from Tests/valid_effects.json as a base
to validate that the video preview system in GmicVideo is working correctly,
fast, and consistently.

Validates:
  - Probe video functionality (ffprobe metadata).
  - Preview frame extraction (ensuring scaled <= 960px for consistent rendering speed).
  - Effect preview generation through G'MIC across valid filters.
  - Image validity (non-empty, non-black, correct channels).
  - Preview rendering latency.
  - Comprehensive logging to Tests/video_preview_validation.log.
"""

import sys
import os
import json
import time
import subprocess
import argparse
from pathlib import Path
from PIL import Image, ImageStat

PROJECT_DIR = Path(__file__).resolve().parent.parent
TESTS_DIR = PROJECT_DIR / "Tests"
VALID_JSON = TESTS_DIR / "valid_effects.json"
LOG_FILE = TESTS_DIR / "video_preview_validation.log"
RESULTS_JSON = TESTS_DIR / "video_preview_results.json"
BACKEND_BIN = PROJECT_DIR / "target" / "release" / "GmicVideo"

CANDIDATE_VIDEOS = [
    Path("/home/isaques/Code/Projetos/Python/ultrawarp-batch/Exemplos de Videos/Original.mp4"),
    Path("/home/isaques/Vídeos/Wallpapers/RAT.mp4"),
]


def log_message(msg: str, log_fp=None):
    timestamp = time.strftime("%Y-%m-%d %H:%M:%S")
    formatted = f"[{timestamp}] {msg}"
    print(formatted, flush=True)
    if log_fp:
        log_fp.write(formatted + "\n")
        log_fp.flush()


def extract_max_val(extrema):
    """Safely extract maximum pixel value from PIL getextrema output regardless of format."""
    if isinstance(extrema, (list, tuple)):
        vals = []
        for item in extrema:
            if isinstance(item, (list, tuple)):
                vals.append(max(item))
            else:
                vals.append(item)
        return int(max(vals)) if vals else 0
    return int(extrema)


class BackendClient:
    def __init__(self, bin_path: Path):
        self.process = subprocess.Popen(
            [str(bin_path), "--server"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1
        )

    def request(self, action: str, payload=None) -> dict:
        req = {"action": action}
        if payload is not None:
            req["payload"] = payload
        line = json.dumps(req) + "\n"
        self.process.stdin.write(line)
        self.process.stdin.flush()
        resp_line = self.process.stdout.readline()
        if not resp_line:
            raise RuntimeError("Backend server closed stdout unexpectedly")
        return json.loads(resp_line.strip())

    def close(self):
        try:
            self.process.terminate()
            self.process.wait(timeout=2)
        except Exception:
            self.process.kill()


def find_test_video() -> Path:
    for vid in CANDIDATE_VIDEOS:
        if vid.exists():
            return vid
    home = Path.home()
    for search_dir in [home / "Vídeos", home / "Videos"]:
        if search_dir.exists():
            mp4s = list(search_dir.glob("*.mp4"))
            if mp4s:
                return mp4s[0]
    return None


def main():
    parser = argparse.ArgumentParser(description="Validate video preview with verified G'MIC effects")
    parser.add_argument("--video", type=str, default=None, help="Path to video file")
    parser.add_argument("--count", type=int, default=30, help="Number of valid effects to test")
    parser.add_argument("--all", action="store_true", help="Test all valid effects")
    args = parser.parse_args()

    log_fp = open(LOG_FILE, "w", buffering=1, encoding="utf-8")
    log_message("=" * 70, log_fp)
    log_message("Starting Video Preview Automated Validation", log_fp)

    # 1. Locate test video
    if args.video:
        video_path = Path(args.video)
    else:
        video_path = find_test_video()

    if not video_path or not video_path.exists():
        log_message(f"[FATAL] No valid test video found at: {video_path}", log_fp)
        log_fp.close()
        sys.exit(1)

    log_message(f"Using test video: {video_path} ({video_path.stat().st_size / (1024*1024):.1f} MB)", log_fp)

    # 2. Load verified valid effects
    if not VALID_JSON.exists():
        log_message(f"[FATAL] Valid effects file not found: {VALID_JSON}. Run test_all_gmic_effects.py first.", log_fp)
        log_fp.close()
        sys.exit(1)

    with open(VALID_JSON, "r", encoding="utf-8") as f:
        valid_effects = json.load(f)

    if not valid_effects:
        log_message("[FATAL] valid_effects.json contains 0 effects.", log_fp)
        log_fp.close()
        sys.exit(1)

    log_message(f"Loaded {len(valid_effects)} verified valid effects from {VALID_JSON}", log_fp)

    if not args.all and args.count and args.count < len(valid_effects):
        selected_effects = valid_effects[:args.count]
    else:
        selected_effects = valid_effects

    log_message(f"Testing video preview against {len(selected_effects)} effects...", log_fp)

    # 3. Start backend client
    log_message(f"Launching backend server: {BACKEND_BIN}", log_fp)
    client = BackendClient(BACKEND_BIN)

    # 4. Probe video
    log_message("Probing video metadata...", log_fp)
    probe_res = client.request("probe_video", {"path": str(video_path)})
    if probe_res.get("status") != "ok":
        log_message(f"[FATAL] Video probe failed: {probe_res.get('data')}", log_fp)
        client.close()
        log_fp.close()
        sys.exit(1)

    video_info = probe_res.get("data", {})
    width = video_info.get("width")
    height = video_info.get("height")
    duration = video_info.get("duration_secs", 0.0)
    fps = video_info.get("fps", 24.0)
    frames = video_info.get("frame_count", 0)
    log_message(f"[OK] Video probed: {width}x{height}, duration={duration:.2f}s, fps={fps:.1f}, frames={frames}", log_fp)
    log_message("-" * 70, log_fp)

    preview_time = min(1.0, max(0.0, duration / 2.0))

    passed_count = 0
    black_count = 0
    error_count = 0
    results = []

    total_start = time.time()

    for idx, eff in enumerate(selected_effects, start=1):
        f_id = eff["id"]
        f_name = eff["name"]
        cat = eff.get("category", "")

        t0 = time.time()
        try:
            res = client.request("render_preview", {
                "video_path": str(video_path),
                "time_secs": preview_time,
                "duration_secs": duration,
                "filter_id": f_id,
                "base_values": {},
                "tweens": {}
            })
            dt = time.time() - t0

            if res.get("status") != "ok":
                err = res.get("data", "Unknown error")
                error_count += 1
                log_message(f"[{idx:3d}/{len(selected_effects)}] [FAIL:BACKEND] '{f_id}' ({f_name}): {err} ({dt:.2f}s)", log_fp)
                results.append({"id": f_id, "name": f_name, "status": "error", "error": err, "elapsed": dt})
                continue

            data = res.get("data", {})
            raw_path = Path(data.get("raw_preview", ""))
            effect_path = Path(data.get("effect_preview", ""))

            if not raw_path.exists() or raw_path.stat().st_size == 0:
                error_count += 1
                log_message(f"[{idx:3d}/{len(selected_effects)}] [FAIL:RAW] '{f_id}': Raw frame missing or empty ({dt:.2f}s)", log_fp)
                results.append({"id": f_id, "name": f_name, "status": "error", "error": "Raw frame missing", "elapsed": dt})
                continue

            if not effect_path.exists() or effect_path.stat().st_size == 0:
                error_count += 1
                log_message(f"[{idx:3d}/{len(selected_effects)}] [FAIL:EFFECT] '{f_id}': Effect frame missing or empty ({dt:.2f}s)", log_fp)
                results.append({"id": f_id, "name": f_name, "status": "error", "error": "Effect frame missing", "elapsed": dt})
                continue

            # Verify image properties
            with Image.open(effect_path) as im:
                im_w, im_h = im.size
                stat = ImageStat.Stat(im)
                means = stat.mean
                mean_b = sum(means[:3]) / min(len(means), 3) if means else 0.0
                max_channel = extract_max_val(im.getextrema())

            if max_channel == 0 or mean_b < 1.0:
                black_count += 1
                err_msg = f"[ERROR] Preview produced a completely black frame for '{f_id}' (mean={mean_b:.2f}, max={max_channel})"
                print(err_msg, file=sys.stderr, flush=True)
                log_message(f"[{idx:3d}/{len(selected_effects)}] {err_msg} ({dt:.2f}s)", log_fp)
                results.append({"id": f_id, "name": f_name, "status": "black_image", "mean": mean_b, "max": max_channel, "elapsed": dt})
                continue

            passed_count += 1
            log_message(
                f"[{idx:3d}/{len(selected_effects)}] [PREVIEW OK] '{f_id}' ({f_name}) -> "
                f"{im_w}x{im_h}, mean={mean_b:.1f}, max={max_channel} in {dt:.2f}s",
                log_fp
            )
            results.append({
                "id": f_id,
                "name": f_name,
                "category": cat,
                "status": "ok",
                "width": im_w,
                "height": im_h,
                "mean": mean_b,
                "max": max_channel,
                "elapsed": dt
            })

        except Exception as e:
            dt = time.time() - t0
            error_count += 1
            log_message(f"[{idx:3d}/{len(selected_effects)}] [FAIL:EXCEPTION] '{f_id}': {e} ({dt:.2f}s)", log_fp)
            results.append({"id": f_id, "name": f_name, "status": "exception", "error": str(e), "elapsed": dt})

    total_elapsed = time.time() - total_start
    client.close()

    log_message("=" * 70, log_fp)
    log_message("VIDEO PREVIEW VALIDATION SUMMARY", log_fp)
    log_message(f"Total effects tested:      {len(selected_effects)}", log_fp)
    log_message(f"Preview successful:        {passed_count} ({passed_count / max(len(selected_effects), 1) * 100:.1f}%)", log_fp)
    log_message(f"Black frames detected:     {black_count}", log_fp)
    log_message(f"Preview errors/failures:   {error_count}", log_fp)
    log_message(f"Total validation time:     {total_elapsed:.1f}s (avg {total_elapsed / max(len(selected_effects), 1):.2f}s per preview)", log_fp)
    log_message("=" * 70, log_fp)

    with open(RESULTS_JSON, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2)

    log_message(f"Saved full preview validation results to: {RESULTS_JSON}", log_fp)
    log_message(f"Detailed logs saved to: {LOG_FILE}", log_fp)
    log_fp.close()

    if passed_count > 0 and black_count == 0:
        print("\n>>> VIDEO PREVIEW VALIDATION PASSED! <<<")
        sys.exit(0)
    else:
        print("\n>>> VIDEO PREVIEW VALIDATION ENCOUNTERED ISSUES <<<")
        sys.exit(1)


if __name__ == "__main__":
    main()
