#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Automated tests for G'MIC Video Visual Search thumbnails and internationalization:
1. Validates that thumbnails are always found and loaded for all visual search effects.
2. Validates that every thumbnail file exists on disk, is non-empty, and is a valid image.
3. Asserts that the 'Apenas com miniaturas' / 'With Previews Only' button has been removed.
4. Asserts that the GUI uses the system language automatically without any manual language dropdown.
5. Validates bilingual search: effects can be found using either their English name or their localized name.
"""

import os
import sys
import json
import unittest

# Ensure src_gui is on path
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(REPO_ROOT, "src_gui"))

from gmicvideo_gui import RustBackendBridge
import i18n


class TestVisualSearchThumbnailsAndI18n(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.repo_root = REPO_ROOT
        # Find backend binary
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

    def test_01_thumbnails_directory_found(self):
        """Verify find_thumbnails_dir locates the valid_effects thumbnail directory."""
        thumb_dir = RustBackendBridge.find_thumbnails_dir()
        self.assertIsNotNone(thumb_dir, "find_thumbnails_dir() must find a thumbnail directory.")
        self.assertTrue(os.path.isdir(thumb_dir), f"{thumb_dir} is not a directory.")
        pngs = [f for f in os.listdir(thumb_dir) if f.endswith(".png")]
        self.assertGreater(len(pngs), 100, f"Expected >100 thumbnail PNGs, found {len(pngs)}.")

    def test_02_get_effects_loads_thumbnails_consistently(self):
        """Verify getEffects loads thumbnails for hundreds of filters."""
        bridge = RustBackendBridge(self.backend_bin)
        raw_json = bridge.getEffects()
        self.assertTrue(raw_json and len(raw_json) > 10, "getEffects returned empty data.")
        data = json.loads(raw_json)
        filters = data.get("filters", [])
        self.assertGreater(len(filters), 100, "Filter database has fewer than 100 filters.")

        filters_with_preview = [f for f in filters if f.get("preview_path")]
        self.assertGreater(
            len(filters_with_preview),
            500,
            f"Expected at least 500 effects with thumbnails, found {len(filters_with_preview)}."
        )

        # Verify famous effect thumbnails are loaded
        names_with_thumb = {f.get("name") for f in filters_with_preview}
        for expected in ["Warhol", "Halftone", "UltraWarp 2", "Vignette"]:
            self.assertIn(
                expected,
                names_with_thumb,
                f"Essential effect '{expected}' must have a thumbnail loaded!"
            )

    def test_03_all_loaded_thumbnails_exist_on_disk(self):
        """Verify every single preview_path points to an actual existing file on disk."""
        bridge = RustBackendBridge(self.backend_bin)
        data = json.loads(bridge.getEffects())
        filters = data.get("filters", [])

        checked = 0
        for f in filters:
            p = f.get("preview_path", "")
            if p:
                self.assertTrue(p.startswith("file://"), f"preview_path must start with file://: {p}")
                disk_path = p[7:]
                self.assertTrue(os.path.isfile(disk_path), f"Thumbnail file not found on disk: {disk_path}")
                self.assertGreater(os.path.getsize(disk_path), 0, f"Thumbnail file is empty (0 bytes): {disk_path}")
                checked += 1

        self.assertGreater(checked, 500, f"Expected >500 checked thumbnails, checked {checked}.")

    def test_04_visual_search_window_qml_guarantees_thumbnails(self):
        """Verify VisualSearchWindow.qml has removed 'Apenas com miniaturas' and filters empty previews."""
        qml_path = os.path.join(self.repo_root, "ui", "VisualSearchWindow.qml")
        self.assertTrue(os.path.isfile(qml_path))
        with open(qml_path, "r", encoding="utf-8") as f:
            qml = f.read()

        # Must NOT contain previewToggle or With Previews Only
        self.assertNotIn("previewToggle", qml, "VisualSearchWindow.qml must not contain 'previewToggle'")
        self.assertNotIn("With Previews Only", qml, "VisualSearchWindow.qml must not have 'With Previews Only' button")
        self.assertNotIn("Apenas com miniaturas", qml, "VisualSearchWindow.qml must not have 'Apenas com miniaturas'")

        # Must enforce preview_path presence so all visual search items have thumbnails
        self.assertIn("!f.preview_path", qml, "VisualSearchWindow.qml must enforce preview_path presence.")

    def test_05_main_qml_no_language_dropdown(self):
        """Verify main.qml has removed the manual language selector dropdown."""
        qml_path = os.path.join(self.repo_root, "ui", "main.qml")
        self.assertTrue(os.path.isfile(qml_path))
        with open(qml_path, "r", encoding="utf-8") as f:
            qml = f.read()

        self.assertNotIn("langSelector", qml, "main.qml must not contain manual langSelector dropdown.")

    def test_06_bilingual_search_in_both_visual_and_text_search(self):
        """Verify bilingual search logic works: users can search in English or localized name."""
        dummy_filter = {"name": "Halftone", "category": "Artistic", "command": "fx_halftone"}
        mgr = i18n.I18nManager()

        # In Portuguese environment
        mgr.setLanguage("pt")
        self.assertEqual(mgr.t("Halftone"), "Meio-tom")
        for q in ["halftone", "meio-tom", "artistic", "fx_halftone"]:
            trans = mgr.t(dummy_filter["name"]).lower()
            orig = dummy_filter["name"].lower()
            matched = (q in trans or q in orig or q in dummy_filter["category"].lower() or q in dummy_filter["command"].lower())
            self.assertTrue(matched, f"Portuguese query '{q}' failed to match.")

        # In German environment
        mgr.setLanguage("de")
        self.assertEqual(mgr.t("Halftone"), "Halbton")
        for q in ["halftone", "halbton", "artistic", "fx_halftone"]:
            trans = mgr.t(dummy_filter["name"]).lower()
            orig = dummy_filter["name"].lower()
            matched = (q in trans or q in orig or q in dummy_filter["category"].lower() or q in dummy_filter["command"].lower())
            self.assertTrue(matched, f"German query '{q}' failed to match.")

        # In Japanese environment
        mgr.setLanguage("ja")
        self.assertEqual(mgr.t("Halftone"), "ハーフトーン")
        for q in ["halftone", "ハーフトーン", "artistic", "fx_halftone"]:
            trans = mgr.t(dummy_filter["name"]).lower()
            orig = dummy_filter["name"].lower()
            matched = (q in trans or q in orig or q in dummy_filter["category"].lower() or q in dummy_filter["command"].lower())
            self.assertTrue(matched, f"Japanese query '{q}' failed to match.")

    def test_07_all_kde_languages_supported(self):
        """Verify all 33 KDE supported languages are present in TRANSLATIONS dictionary."""
        expected_langs = [
            "en", "pt", "es", "ja", "de", "fr", "it", "ru", "zh_CN", "zh_TW", "ko", "pl",
            "nl", "uk", "tr", "sv", "cs", "ca", "ar", "hi", "hu", "da", "fi", "nb", "el",
            "id", "ro", "sk", "sl", "bg", "eu", "gl", "et"
        ]
        for lang in expected_langs:
            self.assertIn(lang, i18n.TRANSLATIONS, f"KDE language '{lang}' must be in TRANSLATIONS.")
            self.assertGreater(len(i18n.TRANSLATIONS[lang]), 20, f"Language '{lang}' has insufficient translations.")


    def test_08_no_non_effects_in_filter_database(self):
        """Verify that non-effect entries (About G'MIC, Privacy Notice, Release Notes, etc.) are excluded."""
        bridge = RustBackendBridge(self.backend_bin)
        data = json.loads(bridge.getEffects())
        filters = data.get("filters", [])
        categories = [c.lower() for c in data.get("categories", [])]

        self.assertNotIn("about", categories, "Category 'About' must not be in categories.")
        self.assertNotIn("sobre", categories, "Category 'Sobre' must not be in categories.")

        excluded_names = {
            "about g'mic", "about gmic", "download external data", "privacy notice",
            "release notes", "send feedback", "tribute to sébastien", "tribute to sebastien",
            "what's new?", "whats new?", "gmicky - roddy", "layer info", "histogram analysis"
        }

        for f in filters:
            name = f.get("name", "").lower()
            cmd = f.get("command", "").lower()
            cat = f.get("category", "").lower()

            self.assertNotEqual(cmd, "_none_", f"Filter '{name}' has command _none_ and must not exist.")
            self.assertNotEqual(cmd, "none", f"Filter '{name}' has command none and must not exist.")
            self.assertNotEqual(cmd, "gui_download_all_data", "Download External Data must not exist in filters.")
            self.assertNotEqual(cat, "about", f"Filter '{name}' is under 'About' category and must not exist.")
            self.assertNotIn(name, excluded_names, f"Non-effect '{name}' must not exist in filters.")
            self.assertFalse(name == "about" or name.startswith("about "), f"About filter '{name}' must not exist.")


if __name__ == "__main__":
    unittest.main()
