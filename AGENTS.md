# Agent Instructions & Guidelines for G'MIC Video

## ⚠️ MANDATORY RULE: ALWAYS TRANSLATE THE GUI

**Whenever you add, modify, or create ANY user-facing GUI element (buttons, labels, dialogs, window titles, notifications, tooltips, menus, placeholders, or card descriptions) in QML or Python, you MUST ALWAYS provide translations for all 4 supported languages:**

1. **English (`en`)**
2. **Portuguese (`pt`)**
3. **Spanish (`es`)**
4. **Japanese (`ja`)**

**Never commit or introduce untranslated raw English text strings into the user interface.**

---

## 🌐 Internationalization (i18n) Architecture

The application uses a centralized internationalization engine located at:
- [`src_gui/i18n.py`](src_gui/i18n.py)

### How It Works

1. **Translations Catalog (`src_gui/i18n.py`)**:
   Contains the `TRANSLATIONS` dictionary organized by language code (`en`, `pt`, `es`, `ja`).
2. **`I18nManager` (`QObject`)**:
   Exposed globally to QML as the context property `i18n`.
   - `i18n.currentLanguage`: Active language code (`en`, `pt`, `es`, `ja`).
   - `i18n.t(key)`: Slot returning the localized string. If a key is missing in the active language, it falls back to English, then to the key itself.
   - `i18n.setLanguage(code)`: Switches language dynamically and emits `languageChanged(code)`.
   - `i18n.availableLanguages()`: Returns list of available language options for UI selectors.
3. **Automatic System Language Detection**:
   On application startup, `detect_system_language()` checks:
   - Environment variables: `LC_ALL`, `LC_MESSAGES`, `LANG` (e.g. `pt_BR.UTF-8` -> `pt`, `es_ES.UTF-8` -> `es`, `ja_JP.UTF-8` -> `ja`).
   - `QLocale.system().name()`.
   - Standard Python `locale.getdefaultlocale()`.
   - Defaults to English (`en`) if the host system locale is not one of the supported 4.

---

## 🛠️ Step-by-Step: Adding New GUI Elements

Whenever adding new text to any QML file (`ui/main.qml`, `ui/RenderDialog.qml`, `ui/VisualSearchWindow.qml`, `ui/InlineCurveEditor.qml`, `ui/BezierCurveEditor.qml`):

1. **Add the string to `src_gui/i18n.py`** under all 4 sections (`en`, `pt`, `es`, `ja`):
   ```python
   # In src_gui/i18n.py:
   "en": {
       "My New Feature": "My New Feature",
       "Items: %1": "Items: %1",
   },
   "pt": {
       "My New Feature": "Meu Novo Recurso",
       "Items: %1": "Itens: %1",
   },
   "es": {
       "My New Feature": "Mi Nueva Función",
       "Items: %1": "Elementos: %1",
   },
   "ja": {
       "My New Feature": "新しい機能",
       "Items: %1": "項目: %1",
   }
   ```

2. **In QML, wrap the text with `i18n.t(...)`**:
   ```qml
   Button {
       text: i18n.t("My New Feature")
   }

   Label {
       text: i18n.t("Items: %1").replace("%1", count)
   }
   ```

---

## 🧪 Verification & Self-Test Checklist

Before completing any task touching GUI files or backend logic:
1. Verify GUI initialization and syntax:
   ```bash
   python3 src_gui/gmicvideo_gui.py --test
   ```
2. Verify all locale detections:
   ```bash
   LANG=pt_BR.UTF-8 python3 src_gui/gmicvideo_gui.py --test
   LANG=es_ES.UTF-8 python3 src_gui/gmicvideo_gui.py --test
   LANG=ja_JP.UTF-8 python3 src_gui/gmicvideo_gui.py --test
   ```
3. Run backend unit tests:
   ```bash
   cargo test
   ```
4. Verify AppImage packaging if packaging was modified:
   ```bash
   ./packaging/build_appimage.sh
   ```
