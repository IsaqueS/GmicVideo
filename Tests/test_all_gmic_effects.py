#!/usr/bin/env python3
"""
G'MIC Effects Automated Validator & Test Suite
----------------------------------------------
Applies every G'MIC filter from GmicVideo onto Tests/test_image.jpeg.
Validates the output:
  - Discards and flags any completely black image (mean < 1.0 or max == 0).
  - Saves valid output images in Tests/valid_effects/<filter_id>.png.
  - Logs all results, errors, execution times, and stats into Tests/gmic_test_results.log.
  - Saves machine-readable manifests: Tests/valid_effects.json and Tests/failed_effects.json.
"""

import sys
import os
import json
import time
import subprocess
import argparse
from pathlib import Path
from concurrent.futures import ProcessPoolExecutor, as_completed
from PIL import Image, ImageStat

PROJECT_DIR = Path(__file__).resolve().parent.parent
TESTS_DIR = PROJECT_DIR / "Tests"
TEST_IMAGE = TESTS_DIR / "test_image.jpeg"
VALID_DIR = TESTS_DIR / "valid_effects"
LOG_FILE = TESTS_DIR / "gmic_test_results.log"
VALID_JSON = TESTS_DIR / "valid_effects.json"
FAILED_JSON = TESTS_DIR / "failed_effects.json"
BACKEND_BIN = PROJECT_DIR / "target" / "release" / "GmicVideo"


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


def load_filters_from_backend():
    """Query the Rust backend for all parsed G'MIC filters and default parameters."""
    if not BACKEND_BIN.exists():
        raise FileNotFoundError(f"Backend binary not found at {BACKEND_BIN}. Run `cargo build --release` first.")

    proc = subprocess.Popen(
        [str(BACKEND_BIN), "--server"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True
    )
    req = json.dumps({"action": "get_effects"}) + "\n"
    stdout, _ = proc.communicate(input=req, timeout=30)
    data = json.loads(stdout.strip())
    if data.get("status") != "ok":
        raise RuntimeError(f"Backend error loading effects: {data.get('data')}")
    return data.get("data", {}).get("filters", [])


def build_filter_cli_args(filter_info: dict) -> list[str]:
    """Build the argument tokens for G'MIC CLI based on filter parameters."""
    tokens = []
    for param in filter_info.get("parameters", []):
        ptype = param.get("param_type", {})
        tag = ptype.get("type")
        data = ptype.get("data", {})

        if tag in ("separator", "note"):
            continue
        elif tag == "float":
            val = float(data.get("default", 0.0))
            if abs(val - round(val)) < 1e-5:
                tokens.append(str(int(round(val))))
            else:
                tokens.append(f"{val:.4f}")
        elif tag == "int":
            val = int(data.get("default", 0))
            tokens.append(str(val))
        elif tag == "bool":
            val = 1 if data.get("default", False) else 0
            tokens.append(str(val))
        elif tag == "choice":
            val = int(data.get("default", 0))
            tokens.append(str(val))
        elif tag == "color":
            hex_col = data.get("default_hex", "#ffffff").lstrip("#")
            if len(hex_col) >= 6:
                r = int(hex_col[0:2], 16)
                g = int(hex_col[2:4], 16)
                b = int(hex_col[4:6], 16)
                tokens.append(f"{r},{g},{b}")
            else:
                tokens.append("255,255,255")
        elif tag == "text":
            txt = str(data.get("default", "")).replace('"', '')
            tokens.append(f'"{txt}"')
        elif tag == "button":
            tokens.append("0")
        else:
            tokens.append("0")

    return tokens


def test_single_filter(filter_info: dict, input_image: Path, valid_out_dir: Path, timeout_sec: int = 6) -> dict:
    filter_id = filter_info["id"]
    filter_name = filter_info["name"]
    command = filter_info["command"]
    category = filter_info.get("category", "General")

    arg_tokens = build_filter_cli_args(filter_info)
    if arg_tokens:
        full_gmic_cmd = [command, ",".join(arg_tokens)]
    else:
        full_gmic_cmd = [command]

    temp_out = valid_out_dir / f"tmp_{filter_id}.png"
    final_out = valid_out_dir / f"{filter_id}.png"

    # Remove previous output if exists
    for p in (temp_out, final_out):
        if p.exists():
            try:
                p.unlink()
            except Exception:
                pass

    gmic_args = [
        "gmic",
        "-input", str(input_image),
        *full_gmic_cmd,
        "-keep[0]",
        "-to_rgb",
        "-cut", "0,255",
        "-output", str(temp_out)
    ]

    start_t = time.time()
    try:
        proc = subprocess.run(
            gmic_args,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec
        )
        elapsed = time.time() - start_t

        if proc.returncode != 0:
            err_msg = proc.stderr.strip() or proc.stdout.strip()
            return {
                "id": filter_id,
                "name": filter_name,
                "category": category,
                "status": "error",
                "error": f"G'MIC error (code {proc.returncode}): {err_msg[:160]}",
                "elapsed": elapsed
            }

        if not temp_out.exists() or temp_out.stat().st_size == 0:
            return {
                "id": filter_id,
                "name": filter_name,
                "category": category,
                "status": "error",
                "error": "Output image was not created or empty",
                "elapsed": elapsed
            }

        # Validate image content
        with Image.open(temp_out) as im:
            stat = ImageStat.Stat(im)
            means = stat.mean
            mean_brightness = sum(means[:3]) / min(len(means), 3) if means else 0.0
            max_channel_val = extract_max_val(im.getextrema())

        # Black image check
        if max_channel_val == 0 or mean_brightness < 1.0:
            temp_out.unlink(missing_ok=True)
            return {
                "id": filter_id,
                "name": filter_name,
                "category": category,
                "status": "black_image",
                "error": f"Image is completely black (mean={mean_brightness:.2f}, max={max_channel_val})",
                "elapsed": elapsed,
                "mean": mean_brightness,
                "max": max_channel_val
            }

        # Valid image! Rename temp to final
        temp_out.rename(final_out)
        return {
            "id": filter_id,
            "name": filter_name,
            "category": category,
            "status": "valid",
            "output_path": str(final_out),
            "elapsed": elapsed,
            "mean": mean_brightness,
            "max": max_channel_val,
            "command": " ".join(full_gmic_cmd)
        }

    except subprocess.TimeoutExpired:
        if temp_out.exists():
            temp_out.unlink(missing_ok=True)
        return {
            "id": filter_id,
            "name": filter_name,
            "category": category,
            "status": "timeout",
            "error": f"Execution timed out (> {timeout_sec}s)",
            "elapsed": timeout_sec
        }
    except Exception as e:
        if temp_out.exists():
            temp_out.unlink(missing_ok=True)
        return {
            "id": filter_id,
            "name": filter_name,
            "category": category,
            "status": "error",
            "error": str(e),
            "elapsed": time.time() - start_t
        }


def main():
    parser = argparse.ArgumentParser(description="Test and validate all G'MIC effects on test_image.jpeg")
    parser.add_argument("--limit", type=int, default=None, help="Limit number of filters to test")
    parser.add_argument("--workers", type=int, default=14, help="Number of concurrent workers")
    parser.add_argument("--timeout", type=int, default=5, help="Timeout in seconds per filter")
    args = parser.parse_args()

    if not TEST_IMAGE.exists():
        print(f"[FATAL] Test image not found at {TEST_IMAGE}", file=sys.stderr)
        sys.exit(1)

    VALID_DIR.mkdir(parents=True, exist_ok=True)
    log_fp = open(LOG_FILE, "w", buffering=1, encoding="utf-8")

    log_message("=" * 70, log_fp)
    log_message("Starting G'MIC Effects Full Test & Validation Suite", log_fp)
    log_message(f"Input test image: {TEST_IMAGE}", log_fp)
    log_message(f"Valid effects output dir: {VALID_DIR}", log_fp)
    log_message(f"Log file: {LOG_FILE}", log_fp)

    log_message("Querying filters from Rust backend...", log_fp)
    try:
        filters = load_filters_from_backend()
    except Exception as e:
        log_message(f"[FATAL] Could not load filters from backend: {e}", log_fp)
        log_fp.close()
        sys.exit(1)

    if args.limit:
        filters = filters[:args.limit]

    total_count = len(filters)
    log_message(f"Total filters to test: {total_count} (workers: {args.workers}, timeout: {args.timeout}s)", log_fp)
    log_message("-" * 70, log_fp)

    valid_results = []
    failed_results = []
    black_count = 0

    suite_start = time.time()

    with ProcessPoolExecutor(max_workers=args.workers) as executor:
        future_map = {
            executor.submit(test_single_filter, f, TEST_IMAGE, VALID_DIR, args.timeout): f
            for f in filters
        }

        completed = 0
        for future in as_completed(future_map):
            completed += 1
            res = future.result()
            f_id = res["id"]
            f_name = res["name"]
            status = res["status"]
            elapsed = res.get("elapsed", 0.0)

            if status == "valid":
                valid_results.append(res)
                log_message(
                    f"[{completed:4d}/{total_count}] [VALID] '{f_id}' ({f_name}) -> "
                    f"mean={res['mean']:.1f}, max={res['max']} ({elapsed:.2f}s)",
                    log_fp
                )
            elif status == "black_image":
                black_count += 1
                failed_results.append(res)
                # Print explicit error as requested by user
                err_str = f"[ERROR] Filter '{f_id}' ({f_name}) produced a COMPLETELY BLACK image (mean={res.get('mean', 0.0):.2f}, max={res.get('max', 0)}). Discarded from valid tests!"
                print(err_str, file=sys.stderr, flush=True)
                log_message(f"[{completed:4d}/{total_count}] {err_str}", log_fp)
            else:
                failed_results.append(res)
                log_message(
                    f"[{completed:4d}/{total_count}] [FAIL:{status.upper()}] '{f_id}' ({f_name}): {res.get('error')} ({elapsed:.2f}s)",
                    log_fp
                )

    suite_elapsed = time.time() - suite_start

    log_message("=" * 70, log_fp)
    log_message("TEST SUITE COMPLETED", log_fp)
    log_message(f"Total filters tested:    {total_count}", log_fp)
    log_message(f"Valid filters passed:    {len(valid_results)} ({len(valid_results) / total_count * 100:.1f}%)", log_fp)
    log_message(f"Black images rejected:   {black_count}", log_fp)
    log_message(f"Other errors / timeouts: {len(failed_results) - black_count}", log_fp)
    log_message(f"Total elapsed time:      {suite_elapsed:.1f}s (avg {suite_elapsed / max(total_count, 1):.2f}s per filter)", log_fp)
    log_message("=" * 70, log_fp)

    with open(VALID_JSON, "w", encoding="utf-8") as f:
        json.dump(valid_results, f, indent=2)

    with open(FAILED_JSON, "w", encoding="utf-8") as f:
        json.dump(failed_results, f, indent=2)

    log_message(f"Saved {len(valid_results)} valid filter definitions to: {VALID_JSON}", log_fp)
    log_message(f"Saved {len(failed_results)} failed filter definitions to: {FAILED_JSON}", log_fp)
    log_fp.close()


if __name__ == "__main__":
    main()
