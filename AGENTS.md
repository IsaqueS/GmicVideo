# Agent Instructions & Guidelines for G'MIC Video

## ⚠️ MANDATORY RULE: ALWAYS TRANSLATE THE GUI TO ALL KDE SUPPORTED LANGUAGES

**Whenever you add, modify, or create ANY user-facing GUI element (buttons, labels, dialogs, window titles, notifications, tooltips, menus, placeholders, card descriptions, or effect names) in QML or Python, you MUST ALWAYS provide translations for ALL KDE supported languages (33 languages):**

1. English (`en`)
2. Portuguese (`pt`)
3. Spanish (`es`)
4. Japanese (`ja`)
5. German (`de`)
6. French (`fr`)
7. Italian (`it`)
8. Russian (`ru`)
9. Simplified Chinese (`zh_CN`)
10. Traditional Chinese (`zh_TW`)
11. Korean (`ko`)
12. Polish (`pl`)
13. Dutch (`nl`)
14. Ukrainian (`uk`)
15. Turkish (`tr`)
16. Swedish (`sv`)
17. Czech (`cs`)
18. Catalan (`ca`)
19. Arabic (`ar`)
20. Hindi (`hi`)
21. Hungarian (`hu`)
22. Danish (`da`)
23. Finnish (`fi`)
24. Norwegian Bokmål (`nb`)
25. Greek (`el`)
26. Indonesian (`id`)
27. Romanian (`ro`)
28. Slovak (`sk`)
29. Slovenian (`sl`)
30. Bulgarian (`bg`)
31. Basque (`eu`)
32. Galician (`gl`)
33. Estonian (`et`)

**Never commit or introduce untranslated raw English text strings into the user interface.**

---

## 🌐 System Language & Internationalization (i18n) Architecture

The application uses a centralized internationalization engine located at:
- [`src_gui/i18n.py`](src_gui/i18n.py)

### Rules for Language Selection and UI Display:
1. **System Language Exclusively (No Dropdown Selector)**:
   The UI must never display a language selection dropdown or manual switcher. The application automatically and exclusively detects the host system's locale using `detect_system_language()` via `LC_ALL`, `LC_MESSAGES`, `LANG`, `LANGUAGE`, `QLocale.system().name()`, and Python's standard `locale`.
2. **Translate Effect Names**:
   Effect names and categories displayed in the UI must be localized using `i18n.t(effect.name)` and `i18n.t(effect.category)`.
3. **Bilingual Search Preservation**:
   In **both** text search (`ui/main.qml`) and visual search (`ui/VisualSearchWindow.qml`), searching using either the **English name** (e.g. `Halftone`, `Warhol`, `Vignette`, `Blur`) or the **localized translated name** (e.g. `Meio-tom`, `Halbton`, `ハーフトーン`, `Demi-teinte`, `Desfoque`, `Vinheta`) MUST ALWAYS work seamlessly.
4. **Visual Search Thumbnails Guarantee**:
   All visual search cards displayed in `ui/VisualSearchWindow.qml` must always have valid preview thumbnails loaded (`f.preview_path`). The "Apenas com miniaturas" / "With Previews Only" button is removed because thumbnail loading is strictly guaranteed and tested.

---

## 🛠️ Step-by-Step: Adding New GUI Elements or Effect Translations

Whenever adding new text or effect names:

1. **Add the string to `src_gui/i18n.py`** under all KDE language sections in `TRANSLATIONS`.
2. **In QML, wrap the text with `i18n.t(...)`**:
   ```qml
   Button {
       text: i18n.t("My New Feature")
   }

   Label {
       text: i18n.t(modelData.name)
   }
   ```
3. **In search filter models**, always match against both translated and original English names:
   ```javascript
   var transName = (i18n.t(f.name) || "").toLowerCase();
   var origName = (f.name || "").toLowerCase();
   var nameMatch = transName.indexOf(query) !== -1 || origName.indexOf(query) !== -1;
   ```

---

## 🧪 Verification & Automated Test Checklist

Before completing any task touching GUI files, translations, thumbnails, or backend logic:
1. **Run Thumbnail & i18n Unit Tests**:
   ```bash
   python3 Tests/test_visual_search_thumbnails.py
   ```
2. **Verify Multi-Locale GUI Initialization**:
   ```bash
   LANG=pt_BR.UTF-8 python3 src_gui/gmicvideo_gui.py --test
   LANG=de_DE.UTF-8 python3 src_gui/gmicvideo_gui.py --test
   LANG=ja_JP.UTF-8 python3 src_gui/gmicvideo_gui.py --test
   LANG=fr_FR.UTF-8 python3 src_gui/gmicvideo_gui.py --test
   LANG=zh_CN.UTF-8 python3 src_gui/gmicvideo_gui.py --test
   ```
3. **Run Backend Rust Tests**:
   ```bash
   cargo test
   ```
4. **Verify AppImage Packaging**:
   ```bash
   ./packaging/build_appimage.sh
   ```
