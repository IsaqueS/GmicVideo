# -*- coding: utf-8 -*-
"""
Generates src_gui/i18n.py with translations for all 33 KDE supported languages,
including UI strings, categories, and effect names, with system locale auto-detection.
"""
import json
import os
import sys

sys.path.insert(0, "src_gui")
import i18n as old_i18n

curr = old_i18n.TRANSLATIONS
en = curr["en"]
pt = curr["pt"]
es = curr["es"]
ja = curr["ja"]

KDE_LANGUAGES = {
    "en": "English",
    "pt": "Português",
    "es": "Español",
    "ja": "日本語",
    "de": "Deutsch",
    "fr": "Français",
    "it": "Italiano",
    "ru": "Русский",
    "zh_CN": "简体中文",
    "zh_TW": "繁體中文",
    "ko": "한국어",
    "pl": "Polski",
    "nl": "Nederlands",
    "uk": "Українська",
    "tr": "Türkçe",
    "sv": "Svenska",
    "cs": "Čeština",
    "ca": "Català",
    "ar": "العربية",
    "hi": "हिन्दी",
    "hu": "Magyar",
    "da": "Dansk",
    "fi": "Suomi",
    "nb": "Norsk Bokmål",
    "el": "Ελληνικά",
    "id": "Bahasa Indonesia",
    "ro": "Română",
    "sk": "Slovenčina",
    "sl": "Slovenščina",
    "bg": "Български",
    "eu": "Euskara",
    "gl": "Galego",
    "et": "Eesti",
}

# Categories
CATEGORIES = {
    "All Categories": {
        "pt": "Todas as Categorias", "es": "Todas las Categorías", "ja": "すべてのカテゴリー",
        "de": "Alle Kategorien", "fr": "Toutes les catégories", "it": "Tutte le categorie",
        "ru": "Все категории", "zh_CN": "所有分类", "zh_TW": "所有分類", "ko": "모든 카테고리",
        "pl": "Wszystkie kategorie", "nl": "Alle categorieën", "uk": "Усі категорії",
        "tr": "Tüm Kategoriler", "sv": "Alla kategorier", "cs": "Všechny kategorie",
        "ca": "Totes les categories", "ar": "جميع الفئات", "hi": "सभी श्रेणियाँ",
        "hu": "Minden kategória", "da": "Alle kategorier", "fi": "Kaikki luokat",
        "nb": "Alle kategorier", "el": "Όλες οι κατηγορίες", "id": "Semua Kategori",
        "ro": "Toate categoriile", "sk": "Všetky kategórie", "sl": "Vse kategorije",
        "bg": "Всички категории", "eu": "Kategoria guztiak", "gl": "Todas as categorías", "et": "Kõik kategooriad"
    },
    "About": {
        "pt": "Sobre", "es": "Acerca de", "ja": "情報", "de": "Über", "fr": "À propos",
        "it": "Informazioni", "ru": "О программе", "zh_CN": "关于", "zh_TW": "關於", "ko": "정보",
        "pl": "O programie", "nl": "Over", "uk": "Про програму", "tr": "Hakkında", "sv": "Om",
        "cs": "O aplikaci", "ca": "Quant a", "ar": "حول", "hi": "के बारे में", "hu": "Névjegy",
        "da": "Om", "fi": "Tietoja", "nb": "Om", "el": "Σχετικά", "id": "Tentang",
        "ro": "Despre", "sk": "O aplikácii", "sl": "O programu", "bg": "Относно", "eu": "Honi buruz",
        "gl": "Acerca de", "et": "Teave"
    },
    "Arrays & Tiles": {
        "pt": "Matrizes e Ladrilhos", "es": "Matrices y Mosaicos", "ja": "配列とタイル",
        "de": "Arrays & Kacheln", "fr": "Tableaux et carreaux", "it": "Matrici e tessere",
        "ru": "Массивы и плитка", "zh_CN": "阵列与平铺", "zh_TW": "陣列與並排", "ko": "배열 및 타일",
        "pl": "Tablice i kafelki", "nl": "Matrices en tegels", "uk": "Масиви та плитка",
        "tr": "Diziler ve Karolar", "sv": "Matriser och brickor", "cs": "Pole a dlaždice",
        "ca": "Matrius i tessel·les", "ar": "مصفوفات وبلاطات", "hi": "सारणियाँ और टाइलें",
        "hu": "Tömbök és csempék", "da": "Matricer og fliser", "fi": "Taulukot ja laatat",
        "nb": "Matriser og fliser", "el": "Πίνακες και πλακίδια", "id": "Larik & Ubin",
        "ro": "Matrice și dale", "sk": "Polia a dlaždice", "sl": "Polja in ploščice",
        "bg": "Масиви и плочки", "eu": "Matrizeak eta teilak", "gl": "Matrices e mosaicos", "et": "Massiivid ja plaadid"
    },
    "Artistic": {
        "pt": "Artístico", "es": "Artístico", "ja": "芸術的", "de": "Künstlerisch", "fr": "Artistique",
        "it": "Artistico", "ru": "Художественные", "zh_CN": "艺术效果", "zh_TW": "藝術效果", "ko": "예술적",
        "pl": "Artystyczne", "nl": "Artistiek", "uk": "Художні", "tr": "Sanatsal", "sv": "Konstnärlig",
        "cs": "Umělecké", "ca": "Artístic", "ar": "فني", "hi": "कलात्मक", "hu": "Művészi",
        "da": "Kunstnerisk", "fi": "Taiteellinen", "nb": "Kunstnerisk", "el": "Καλλιτεχνικό",
        "id": "Artistik", "ro": "Artistic", "sk": "Umelecké", "sl": "Umetniško", "bg": "Художествени",
        "eu": "Artistikoa", "gl": "Artístico", "et": "Kunstiline"
    },
    "Black & White": {
        "pt": "Preto e Branco", "es": "Blanco y Negro", "ja": "白黒", "de": "Schwarz-Weiß", "fr": "Noir et blanc",
        "it": "Bianco e nero", "ru": "Черно-белое", "zh_CN": "黑白", "zh_TW": "黑白", "ko": "흑백",
        "pl": "Czarno-białe", "nl": "Zwart-wit", "uk": "Чорно-біле", "tr": "Siyah Beyaz", "sv": "Svartvitt",
        "cs": "Černobílé", "ca": "Blanc i negre", "ar": "أبيض وأسود", "hi": "श्वेत-श्याम", "hu": "Fekete-fehér",
        "da": "Sort-hvid", "fi": "Mustavalkoinen", "nb": "Svart-hvitt", "el": "Ασπρόμαυρο", "id": "Hitam & Putih",
        "ro": "Alb-negru", "sk": "Čiernobiele", "sl": "Črno-belo", "bg": "Черно и бяло", "eu": "Zuri-beltza",
        "gl": "Branco e negro", "et": "Mustvalge"
    },
    "Colors": {
        "pt": "Cores", "es": "Colores", "ja": "カラー", "de": "Farben", "fr": "Couleurs",
        "it": "Colori", "ru": "Цвета", "zh_CN": "颜色", "zh_TW": "色彩", "ko": "색상",
        "pl": "Kolory", "nl": "Kleuren", "uk": "Кольори", "tr": "Renkler", "sv": "Färger",
        "cs": "Barvy", "ca": "Colors", "ar": "الألوان", "hi": "रंग", "hu": "Színek",
        "da": "Farver", "fi": "Värit", "nb": "Farger", "el": "Χρώματα", "id": "Warna",
        "ro": "Culori", "sk": "Farby", "sl": "Barve", "bg": "Цветове", "eu": "Koloreak",
        "gl": "Cores", "et": "Värvid"
    },
    "Deformations": {
        "pt": "Deformações", "es": "Deformaciones", "ja": "変形", "de": "Verformungen", "fr": "Déformations",
        "it": "Deformazioni", "ru": "Деформации", "zh_CN": "变形扭曲", "zh_TW": "變形扭曲", "ko": "변형",
        "pl": "Zniekształcenia", "nl": "Vervormingen", "uk": "Деформації", "tr": "Deformasyonlar", "sv": "Deformationer",
        "cs": "Deformace", "ca": "Deformacions", "ar": "تشوهات", "hi": "विरूपण", "hu": "Deformációk",
        "da": "Deformationer", "fi": "Muodonmuutokset", "nb": "Deformasjoner", "el": "Παραμορφώσεις", "id": "Deformasi",
        "ro": "Deformări", "sk": "Deformácie", "sl": "Deformacije", "bg": "Деформации", "eu": "Deformazioak",
        "gl": "Deformacións", "et": "Deformatsioonid"
    },
    "Degradations": {
        "pt": "Degradações", "es": "Degradaciones", "ja": "劣化", "de": "Verschlechterungen", "fr": "Dégradations",
        "it": "Degradi", "ru": "Деградация", "zh_CN": "画质退化", "zh_TW": "畫質劣化", "ko": "품질 저하",
        "pl": "Degradacje", "nl": "Aantastingen", "uk": "Погіршення якості", "tr": "Bozulmalar", "sv": "Försämringar",
        "cs": "Degradace", "ca": "Degradacions", "ar": "تدهورات", "hi": "अपघटन", "hu": "Minőségromlások",
        "da": "Nedbrydninger", "fi": "Rappeumat", "nb": "Nedbrytninger", "el": "Υποβαθμίσεις", "id": "Degradasi",
        "ro": "Degradări", "sk": "Degradácie", "sl": "Poslabšanja", "bg": "Деградации", "eu": "Degradazioak",
        "gl": "Degradacións", "et": "Lagunemised"
    },
    "Details": {
        "pt": "Detalhes", "es": "Detalles", "ja": "ディテール", "de": "Details", "fr": "Détails",
        "it": "Dettagli", "ru": "Детализация", "zh_CN": "细节处理", "zh_TW": "細節處理", "ko": "세부 사항",
        "pl": "Szczegóły", "nl": "Details", "uk": "Деталі", "tr": "Ayrıntılar", "sv": "Detaljer",
        "cs": "Detaily", "ca": "Detalls", "ar": "التفاصيل", "hi": "विवरण", "hu": "Részletek",
        "da": "Detaljer", "fi": "Yksityiskohdat", "nb": "Detaljer", "el": "Λεπτομέρειες", "id": "Detail",
        "ro": "Detalii", "sk": "Detaily", "sl": "Podrobnosti", "bg": "Детайли", "eu": "Xehetasunak",
        "gl": "Detalles", "et": "Üksikasjad"
    },
    "Film Emulation": {
        "pt": "Emulação de Filme", "es": "Emulación de Película", "ja": "フィルムエミュレーション",
        "de": "Filmemulation", "fr": "Émulation de film", "it": "Emulazione pellicola",
        "ru": "Эмуляция пленки", "zh_CN": "胶片模拟", "zh_TW": "底片模擬", "ko": "필름 에뮬레이션",
        "pl": "Emulacja filmu", "nl": "Film-emulatie", "uk": "Емуляція плівки",
        "tr": "Film Benzetimi", "sv": "Filmemulering", "cs": "Emulace filmu",
        "ca": "Emulació de pel·lícula", "ar": "محاكاة الفيلم", "hi": "फिल्म अनुकरण",
        "hu": "Filmemuláció", "da": "Filmemulering", "fi": "Filmiemulaatio",
        "nb": "Filmemulering", "el": "Προσομοίωση φιλμ", "id": "Emulasi Film",
        "ro": "Emulare film", "sk": "Emulácia filmu", "sl": "Emulacija filma",
        "bg": "Емулация на филм", "eu": "Film emulazioa", "gl": "Emulación de película", "et": "Filmi emuleerimine"
    },
    "Frames": {
        "pt": "Molduras", "es": "Marcos", "ja": "フレーム", "de": "Rahmen", "fr": "Cadres",
        "it": "Cornici", "ru": "Рамки", "zh_CN": "边框相框", "zh_TW": "相框邊框", "ko": "프레임",
        "pl": "Ramki", "nl": "Kaders", "uk": "Рамки", "tr": "Çerçeveler", "sv": "Ramar",
        "cs": "Rámečky", "ca": "Marcs", "ar": "إطارات", "hi": "फ्रेम्स", "hu": "Keretek",
        "da": "Rammer", "fi": "Kehykset", "nb": "Rammer", "el": "Πλαίσια", "id": "Bingkai",
        "ro": "Rame", "sk": "Rámy", "sl": "Okvirji", "bg": "Рамки", "eu": "Markoak",
        "gl": "Marcos", "et": "Raamid"
    },
    "Fading": {
        "pt": "Desvanecimento", "es": "Desvanecimiento", "ja": "フェード", "de": "Überblenden", "fr": "Fondu",
        "it": "Dissolvenza", "ru": "Угасание", "zh_CN": "渐变消隐", "zh_TW": "淡入淡出", "ko": "페이딩",
        "pl": "Zanikanie", "nl": "Vervagen", "uk": "Згасання", "tr": "Solma", "sv": "Toning",
        "cs": "Prolínání", "ca": "Esvaïment", "ar": "تلاشي", "hi": "फ़ेडिंग", "hu": "Elhalványulás",
        "da": "Udtoning", "fi": "Häivytys", "nb": "Toning", "el": "Εξασθένιση", "id": "Pemudaran",
        "ro": "Estompare", "sk": "Prelínanie", "sl": "Zbleditev", "bg": "Затихване", "eu": "Iraungitzea",
        "gl": "Desvanecemento", "et": "Hääbumine"
    },
    "Lights & Shadows": {
        "pt": "Luzes e Sombras", "es": "Luces y Sombras", "ja": "光と影", "de": "Licht & Schatten", "fr": "Ombres et lumières",
        "it": "Luci e ombre", "ru": "Свет и тени", "zh_CN": "光影效果", "zh_TW": "光影效果", "ko": "빛과 그림자",
        "pl": "Światła i cienie", "nl": "Licht en schaduw", "uk": "Світло і тіні", "tr": "Işıklar ve Gölgeler", "sv": "Ljus och skuggor",
        "cs": "Světla a stíny", "ca": "Llums i ombres", "ar": "أضواء وظلال", "hi": "प्रकाश और छाया", "hu": "Fények és árnyékok",
        "da": "Lys og skygger", "fi": "Valot ja varjot", "nb": "Lys og skygger", "el": "Φώτα και σκιές", "id": "Cahaya & Bayangan",
        "ro": "Lumini și umbre", "sk": "Svetlá a tiene", "sl": "Svetloba in sence", "bg": "Светлини и сенки", "eu": "Argiak eta itzalak",
        "gl": "Luces e sombras", "et": "Valgused ja varjud"
    },
    "Patterns": {
        "pt": "Padrões", "es": "Patrones", "ja": "パターン", "de": "Muster", "fr": "Motifs",
        "it": "Motivi", "ru": "Узоры", "zh_CN": "图案纹理", "zh_TW": "圖樣紋理", "ko": "패턴",
        "pl": "Wzorce", "nl": "Patronen", "uk": "Візерунки", "tr": "Desenler", "sv": "Mönster",
        "cs": "Vzory", "ca": "Patrons", "ar": "أنماط", "hi": "पैटर्न", "hu": "Minták",
        "da": "Mønstre", "fi": "Kuviot", "nb": "Mønstre", "el": "Μοτίβα", "id": "Pola",
        "ro": "Modele", "sk": "Vzory", "sl": "Vzorci", "bg": "Модели", "eu": "Ereduak",
        "gl": "Patróns", "et": "Mustrid"
    },
    "Rendering": {
        "pt": "Renderização", "es": "Renderizado", "ja": "レンダリング", "de": "Rendering", "fr": "Rendu",
        "it": "Rendering", "ru": "Рендеринг", "zh_CN": "生成渲染", "zh_TW": "算圖產生", "ko": "렌더링",
        "pl": "Renderowanie", "nl": "Rendering", "uk": "Рендеринг", "tr": "İşleme", "sv": "Rendering",
        "cs": "Vykreslování", "ca": "Renderització", "ar": "تصيير", "hi": "रेंडरिंग", "hu": "Renderelés",
        "da": "Rendering", "fi": "Renderöinti", "nb": "Rendring", "el": "Απόδοση", "id": "Rendering",
        "ro": "Randare", "sk": "Vykresľovanie", "sl": "Izrisovanje", "bg": "Рендиране", "eu": "Errendatzea",
        "gl": "Renderización", "et": "Renderdamine"
    },
    "Repair": {
        "pt": "Restauração", "es": "Reparación", "ja": "修復", "de": "Reparatur", "fr": "Réparation",
        "it": "Riparazione", "ru": "Восстановление", "zh_CN": "修复降噪", "zh_TW": "修復降噪", "ko": "복구",
        "pl": "Naprawa", "nl": "Herstel", "uk": "Відновлення", "tr": "Onarım", "sv": "Reparation",
        "cs": "Oprava", "ca": "Reparació", "ar": "إصلاح", "hi": "मरम्मत", "hu": "Javítás",
        "da": "Reparation", "fi": "Korjaus", "nb": "Reparasjon", "el": "Επισκευή", "id": "Perbaikan",
        "ro": "Reparare", "sk": "Oprava", "sl": "Popravilo", "bg": "Поправка", "eu": "Konponketa",
        "gl": "Reparación", "et": "Parandamine"
    },
    "Sequences": {
        "pt": "Sequências", "es": "Secuencias", "ja": "シーケンス", "de": "Sequenzen", "fr": "Séquences",
        "it": "Sequenze", "ru": "Последовательности", "zh_CN": "连续序列", "zh_TW": "連續序列", "ko": "시퀀스",
        "pl": "Sekwencje", "nl": "Sequenties", "uk": "Послідовності", "tr": "Diziler", "sv": "Sekvenser",
        "cs": "Sekvence", "ca": "Seqüències", "ar": "تسلسلات", "hi": "अनुक्रम", "hu": "Sorozatok",
        "da": "Sekvenser", "fi": "Sekvenssit", "nb": "Sekvenser", "el": "Ακολουθίες", "id": "Sekuens",
        "ro": "Secvențe", "sk": "Sekvencie", "sl": "Zaporedja", "bg": "Последователности", "eu": "Sekuentziak",
        "gl": "Secuencias", "et": "Jadad"
    },
    "Silhouettes": {
        "pt": "Silhuetas", "es": "Siluetas", "ja": "シルエット", "de": "Silhouetten", "fr": "Silhouettes",
        "it": "Sagome", "ru": "Силуэты", "zh_CN": "剪影轮廓", "zh_TW": "剪影輪廓", "ko": "실루엣",
        "pl": "Sylwetki", "nl": "Silhouetten", "uk": "Силуети", "tr": "Silüetler", "sv": "Silhuetter",
        "cs": "Siluety", "ca": "Siluetes", "ar": "صور ظلية", "hi": "छायाचित्र", "hu": "Sziluettek",
        "da": "Silhuetter", "fi": "Siluetit", "nb": "Silhuetter", "el": "Σιλουέτες", "id": "Siluet",
        "ro": "Siluete", "sk": "Siluety", "sl": "Silhuete", "bg": "Силуети", "eu": "Siluetak",
        "gl": "Siluetas", "et": "Siluetid"
    },
    "Testing": {
        "pt": "Testes", "es": "Pruebas", "ja": "テスト", "de": "Testen", "fr": "Tests",
        "it": "Test", "ru": "Тестирование", "zh_CN": "测试实验", "zh_TW": "測試實驗", "ko": "테스트",
        "pl": "Testowanie", "nl": "Testen", "uk": "Тестування", "tr": "Test Etme", "sv": "Test",
        "cs": "Testování", "ca": "Proves", "ar": "اختبار", "hi": "परीक्षण", "hu": "Tesztelés",
        "da": "Test", "fi": "Testaus", "nb": "Testing", "el": "Δοκιμές", "id": "Pengujian",
        "ro": "Testare", "sk": "Testovanie", "sl": "Preizkušanje", "bg": "Тестване", "eu": "Probak",
        "gl": "Probas", "et": "Testimine"
    },
    "Various": {
        "pt": "Diversos", "es": "Varios", "ja": "その他", "de": "Verschiedenes", "fr": "Divers",
        "it": "Vari", "ru": "Разное", "zh_CN": "各类杂项", "zh_TW": "各類雜項", "ko": "기타",
        "pl": "Różne", "nl": "Diversen", "uk": "Різне", "tr": "Çeşitli", "sv": "Diverse",
        "cs": "Různé", "ca": "Diversos", "ar": "متنوع", "hi": "विविध", "hu": "Vegyes",
        "da": "Diverse", "fi": "Sekalaista", "nb": "Diverse", "el": "Διάφορα", "id": "Beragam",
        "ro": "Diverse", "sk": "Rôzne", "sl": "Razno", "bg": "Разни", "eu": "Hainbat",
        "gl": "Varios", "et": "Varia"
    }
}

# Effects
EFFECT_NAMES = {
    "Warhol": {
        "pt": "Warhol", "es": "Warhol", "ja": "ウォーホル", "de": "Warhol", "fr": "Warhol", "it": "Warhol",
        "ru": "Уорхол", "zh_CN": "沃霍尔波普", "zh_TW": "沃荷普普", "ko": "워홀", "pl": "Warhol", "nl": "Warhol",
        "uk": "Воргол", "tr": "Warhol", "sv": "Warhol", "cs": "Warhol", "ca": "Warhol", "ar": "وارهول",
        "hi": "वारहोल", "hu": "Warhol", "da": "Warhol", "fi": "Warhol", "nb": "Warhol", "el": "Γουόρχολ",
        "id": "Warhol", "ro": "Warhol", "sk": "Warhol", "sl": "Warhol", "bg": "Уорхол", "eu": "Warhol",
        "gl": "Warhol", "et": "Warhol"
    },
    "Halftone": {
        "pt": "Meio-tom", "es": "Medios Tonos", "ja": "ハーフトーン", "de": "Halbton", "fr": "Demi-teinte",
        "it": "Mezzatinta", "ru": "Полутон", "zh_CN": "半色调网点", "zh_TW": "半色調網點", "ko": "하프톤",
        "pl": "Półtony", "nl": "Halftoon", "uk": "Напівтон", "tr": "Yarı Ton", "sv": "Halvton",
        "cs": "Polotón", "ca": "Semitò", "ar": "نصف نغمة", "hi": "हाफ़टोन", "hu": "Félárnyalat",
        "da": "Halvtone", "fi": "Puolisävy", "nb": "Halvtone", "el": "Ενδιάμεσοι τόνοι", "id": "Halftone",
        "ro": "Semiton", "sk": "Poltón", "sl": "Polton", "bg": "Полутон", "eu": "Tonuerdia",
        "gl": "Medio ton", "et": "Pooltoon"
    },
    "UltraWarp 2": {
        "pt": "UltraWarp 2", "es": "UltraWarp 2", "ja": "ウルトラワープ2", "de": "UltraWarp 2",
        "fr": "UltraWarp 2", "it": "UltraWarp 2", "ru": "УльтраВарп 2", "zh_CN": "极速扭曲 2",
        "zh_TW": "極速扭曲 2", "ko": "울트라워프 2", "pl": "UltraWarp 2", "nl": "UltraWarp 2",
        "uk": "УльтраВарп 2", "tr": "UltraWarp 2", "sv": "UltraWarp 2", "cs": "UltraWarp 2",
        "ca": "UltraWarp 2", "ar": "ألترا وورب 2", "hi": "अल्ट्रावार्प 2", "hu": "UltraWarp 2",
        "da": "UltraWarp 2", "fi": "UltraWarp 2", "nb": "UltraWarp 2", "el": "UltraWarp 2",
        "id": "UltraWarp 2", "ro": "UltraWarp 2", "sk": "UltraWarp 2", "sl": "UltraWarp 2",
        "bg": "УлтраУарп 2", "eu": "UltraWarp 2", "gl": "UltraWarp 2", "et": "UltraWarp 2"
    },
    "Vignette": {
        "pt": "Vinheta", "es": "Viñeta", "ja": "ビネット", "de": "Vignette", "fr": "Vignettage",
        "it": "Vignettatura", "ru": "Виньетка", "zh_CN": "暗角晕影", "zh_TW": "暗角暈影", "ko": "비네트",
        "pl": "Winieta", "nl": "Vignet", "uk": "Віньєтка", "tr": "Vinyet", "sv": "Vinjett",
        "cs": "Vinětace", "ca": "Vinyeta", "ar": "تظليل الحواف", "hi": "विगनेट", "hu": "Vignetta",
        "da": "Vignettering", "fi": "Vinjetointi", "nb": "Vignett", "el": "Βινιέτα", "id": "Vinyet",
        "ro": "Vignetă", "sk": "Vinetácia", "sl": "Vinjeta", "bg": "Винетка", "eu": "Bineta",
        "gl": "Viñeta", "et": "Vinjett"
    },
    "Blur": {
        "pt": "Desfoque", "es": "Desenfoque", "ja": "ぼかし", "de": "Unschärfe", "fr": "Flou",
        "it": "Sfocatura", "ru": "Размытие", "zh_CN": "模糊", "zh_TW": "模糊", "ko": "흐림 효과",
        "pl": "Rozmycie", "nl": "Vervaging", "uk": "Розмиття", "tr": "Bulanıklık", "sv": "Oskärpa",
        "cs": "Rozostření", "ca": "Desenfocament", "ar": "تمويه", "hi": "धुंधला", "hu": "Elmosás",
        "da": "Sløring", "fi": "Sumennus", "nb": "Uskarphet", "el": "Θόλωμα", "id": "Kabur",
        "ro": "Estompare", "sk": "Rozostrenie", "sl": "Zameglitev", "bg": "Замъгляване", "eu": "Lausotzea",
        "gl": "Desenfoque", "et": "Hägusus"
    },
    "Gaussian Blur": {
        "pt": "Desfoque Gaussiano", "es": "Desenfoque Gaussiano", "ja": "ガウスぼかし",
        "de": "Gaußscher Weichzeichner", "fr": "Flou gaussien", "it": "Sfocatura gaussiana",
        "ru": "Размытие по Гауссу", "zh_CN": "高斯模糊", "zh_TW": "高斯模糊", "ko": "가우시안 흐림",
        "pl": "Rozmycie Gaussa", "nl": "Gaussiaanse vervaging", "uk": "Гаусове розмиття",
        "tr": "Gauss Bulanıklığı", "sv": "Gaussisk oskärpa", "cs": "Gaussovské rozostření",
        "ca": "Desenfocament gaussià", "ar": "تمويه غاوسي", "hi": "गॉसियन ब्लर", "hu": "Gauss-elmosás",
        "da": "Gaussisk sløring", "fi": "Gauss-sumennus", "nb": "Gaussisk uskarphet", "el": "Θόλωμα Gauss",
        "id": "Kabur Gaussian", "ro": "Estompare gaussiană", "sk": "Gaussovské rozostrenie",
        "sl": "Gaussova zameglitev", "bg": "Гаусово замъгляване", "eu": "Gaussiar lausotzea",
        "gl": "Desenfoque gaussiano", "et": "Gaussi hägusus"
    },
    "Bloom": {
        "pt": "Bloom / Brilho Difuso", "es": "Bloom / Resplandor", "ja": "ブルーム",
        "de": "Bloom-Effekt", "fr": "Effet Bloom", "it": "Bagliore Bloom", "ru": "Свечение Bloom",
        "zh_CN": "泛光柔光", "zh_TW": "泛光柔光", "ko": "블룸", "pl": "Poświata Bloom", "nl": "Bloom",
        "uk": "Світіння Bloom", "tr": "Bloom Işıltısı", "sv": "Bloom", "cs": "Záře Bloom",
        "ca": "Efecte Bloom", "ar": "توهج بلوم", "hi": "ब्लूम", "hu": "Fényudvar (Bloom)",
        "da": "Bloom", "fi": "Bloom-hehku", "nb": "Bloom", "el": "Λάμψη Bloom", "id": "Bloom",
        "ro": "Efect Bloom", "sk": "Žiara Bloom", "sl": "Žarenje Bloom", "bg": "Блум сияние",
        "eu": "Bloom efektua", "gl": "Efecto Bloom", "et": "Bloom-kuma"
    },
    "Glow": {
        "pt": "Brilho / Incandescência", "es": "Resplandor", "ja": "グロー", "de": "Leuchten", "fr": "Lueur",
        "it": "Bagliore", "ru": "Свечение", "zh_CN": "辉光发光", "zh_TW": "輝光發光", "ko": "글로우",
        "pl": "Poświata", "nl": "Gloed", "uk": "Сяйво", "tr": "Işıltı", "sv": "Glöd",
        "cs": "Záře", "ca": "Resplendor", "ar": "توهج", "hi": "चमक", "hu": "Izzás",
        "da": "Glød", "fi": "Hehku", "nb": "Glød", "el": "Λάμψη", "id": "Pendaran",
        "ro": "Strălucire", "sk": "Žiara", "sl": "Žarenje", "bg": "Сияние", "eu": "Distira",
        "gl": "Resplandor", "et": "Kuma"
    },
    "Sharpen": {
        "pt": "Nitidez", "es": "Enfocar", "ja": "シャープ化", "de": "Schärfen", "fr": "Netteté",
        "it": "Nitidezza", "ru": "Резкость", "zh_CN": "锐化", "zh_TW": "銳利化", "ko": "선명하게",
        "pl": "Wyostrzanie", "nl": "Verscherpen", "uk": "Різкість", "tr": "Keskinleştir", "sv": "Skärpa",
        "cs": "Zaostřit", "ca": "Agudesa", "ar": "زيادة التحديد", "hi": "शार्प करें", "hu": "Élesítés",
        "da": "Skarphed", "fi": "Terävöitys", "nb": "Skarphet", "el": "Όξυνση", "id": "Pertajam",
        "ro": "Claritate", "sk": "Zaostrenie", "sl": "Ostrenje", "bg": "Изостряне", "eu": "Zorroztea",
        "gl": "Nitidez", "et": "Teravustamine"
    },
    "Dithering": {
        "pt": "Pontilhado / Dithering", "es": "Tramado", "ja": "ディザリング", "de": "Dithering",
        "fr": "Tramage", "it": "Retinatura", "ru": "Дизеринг", "zh_CN": "抖动算法", "zh_TW": "抖動演算法",
        "ko": "디더링", "pl": "Dithering", "nl": "Ditheren", "uk": "Дизеринг", "tr": "Renk Taklidi",
        "sv": "Gitter", "cs": "Rozptyl", "ca": "Tramatge", "ar": "تردد لوني", "hi": "डिथरिंग",
        "hu": "Színátmenet-simítás", "da": "Raster", "fi": "Kohdistus", "nb": "Raster", "el": "Διαγράμμιση",
        "id": "Dithering", "ro": "Modulație", "sk": "Rozptyl", "sl": "Sipanje", "bg": "Дитеринг",
        "eu": "Sareztatzea", "gl": "Tramado", "et": "Dithering"
    },
    "Colorize": {
        "pt": "Colorir", "es": "Colorear", "ja": "着色", "de": "Einfärben", "fr": "Coloriser",
        "it": "Colora", "ru": "Колоризация", "zh_CN": "上色着色", "zh_TW": "著色", "ko": "색상화",
        "pl": "Koloruj", "nl": "Inkleuren", "uk": "Колоризація", "tr": "Renklendir", "sv": "Färglägg",
        "cs": "Obarvit", "ca": "Acoloreix", "ar": "تلوين", "hi": "रंगीन करें", "hu": "Színezés",
        "da": "Farvelæg", "fi": "Väritä", "nb": "Fargelegg", "el": "Επιχρωματισμός", "id": "Warnai",
        "ro": "Colorare", "sk": "Zafarbiť", "sl": "Pobarvaj", "bg": "Оцветяване", "eu": "Koloreztatu",
        "gl": "Colorear", "et": "Värvimine"
    },
    "Cartoon": {
        "pt": "Desenho Animado", "es": "Dibujo Animado", "ja": "カートゥーン", "de": "Cartoon",
        "fr": "Bande Dessinée", "it": "Fumetto", "ru": "Мультфильм", "zh_CN": "卡通漫画",
        "zh_TW": "卡通漫畫", "ko": "카툰", "pl": "Kreskówka", "nl": "Tekenfilm", "uk": "Мультфільм",
        "tr": "Çizgi Film", "sv": "Tecknad", "cs": "Kreslený film", "ca": "Dibuix animat",
        "ar": "رسوم متحركة", "hi": "कार्टून", "hu": "Rajzfilm", "da": "Tegneserie", "fi": "Sarjakuva",
        "nb": "Tegneserie", "el": "Κινούμενα σχέδια", "id": "Kartun", "ro": "Desene animate",
        "sk": "Kreslený film", "sl": "Risanka", "bg": "Анимация", "eu": "Marrazki biziduna",
        "gl": "Debuxo animado", "et": "Multikas"
    },
    "Engrave": {
        "pt": "Gravação / Entalhe", "es": "Grabado", "ja": "彫刻", "de": "Gravur", "fr": "Gravure",
        "it": "Incisione", "ru": "Гравюра", "zh_CN": "雕刻蚀刻", "zh_TW": "雕刻蝕刻", "ko": "판화",
        "pl": "Grawerunek", "nl": "Graveren", "uk": "Гравюра", "tr": "Gravür", "sv": "Gravyr",
        "cs": "Rytina", "ca": "Gravat", "ar": "نقش", "hi": "नक्काशी", "hu": "Metszet",
        "da": "Gravering", "fi": "Kaiverrus", "nb": "Gravering", "el": "Χάραξη", "id": "Ukiran",
        "ro": "Gravură", "sk": "Rytina", "sl": "Gravura", "bg": "Гравюра", "eu": "Grabatua",
        "gl": "Gravado", "et": "Graveering"
    },
    "Pixel Art": {
        "pt": "Pixel Art", "es": "Pixel Art", "ja": "ピクセルアート", "de": "Pixel-Art", "fr": "Pixel Art",
        "it": "Pixel Art", "ru": "Пиксель-арт", "zh_CN": "像素艺术", "zh_TW": "像素藝術", "ko": "픽셀 아트",
        "pl": "Pixel Art", "nl": "Pixelart", "uk": "Піксель-арт", "tr": "Piksel Sanatı", "sv": "Pixelkonst",
        "cs": "Pixelové umění", "ca": "Pixel art", "ar": "فن البكسل", "hi": "पिक्सेल कला", "hu": "Pixel art",
        "da": "Pixelkunst", "fi": "Pikselitaide", "nb": "Pikselkunst", "el": "Εικονοστοιχεία Pixel", "id": "Seni Piksel",
        "ro": "Artă pixelată", "sk": "Pixelové umenie", "sl": "Pikselna umetnost", "bg": "Пиксел арт", "eu": "Pixel artea",
        "gl": "Pixel Art", "et": "Pikslikunst"
    },
    "Sketch": {
        "pt": "Esboço", "es": "Boceto", "ja": "スケッチ", "de": "Skizze", "fr": "Croquis",
        "it": "Schizzo", "ru": "Эскиз", "zh_CN": "铅笔素描", "zh_TW": "鉛筆素描", "ko": "스케치",
        "pl": "Szkic", "nl": "Schets", "uk": "Ескіз", "tr": "Eskiz", "sv": "Skiss",
        "cs": "Skica", "ca": "Esbós", "ar": "رسم تخطيطي", "hi": "स्केच", "hu": "Vázlat",
        "da": "Skitse", "fi": "Luonnos", "nb": "Skisse", "el": "Σκίτσο", "id": "Sketsa",
        "ro": "Schiță", "sk": "Skica", "sl": "Skica", "bg": "Скица", "eu": "Zirriborroa",
        "gl": "Esbozo", "et": "Visand"
    },
    "WaterColor": {
        "pt": "Aquarela", "es": "Acuarela", "ja": "水彩画", "de": "Aquarell", "fr": "Aquarelle",
        "it": "Acquerello", "ru": "Акварель", "zh_CN": "水彩画", "zh_TW": "水彩畫", "ko": "수채화",
        "pl": "Akwarela", "nl": "Aquarel", "uk": "Акварель", "tr": "Sulu Boya", "sv": "Akvarell",
        "cs": "Akvarel", "ca": "Aquarel·la", "ar": "ألوان مائية", "hi": "जल रंग", "hu": "Akvarell",
        "da": "Akvarel", "fi": "Akvarelli", "nb": "Akvarell", "el": "Υδατογραφία", "id": "Cat Air",
        "ro": "Acuarelă", "sk": "Akvarel", "sl": "Akvarel", "bg": "Акварел", "eu": "Akuarela",
        "gl": "Acuarela", "et": "Akvarell"
    },
    "Watercolor": {
        "pt": "Aquarela", "es": "Acuarela", "ja": "水彩画", "de": "Aquarell", "fr": "Aquarelle",
        "it": "Acquerello", "ru": "Акварель", "zh_CN": "水彩画", "zh_TW": "水彩畫", "ko": "수채화",
        "pl": "Akwarela", "nl": "Aquarel", "uk": "Акварель", "tr": "Sulu Boya", "sv": "Akvarell",
        "cs": "Akvarel", "ca": "Aquarel·la", "ar": "ألوان مائية", "hi": "जल रंग", "hu": "Akvarell",
        "da": "Akvarel", "fi": "Akvarelli", "nb": "Akvarell", "el": "Υδατογραφία", "id": "Cat Air",
        "ro": "Acuarelă", "sk": "Akvarel", "sl": "Akvarel", "bg": "Акварел", "eu": "Akuarela",
        "gl": "Acuarela", "et": "Akvarell"
    },
    "Sepia": {
        "pt": "Sépia", "es": "Sepia", "ja": "セピア", "de": "Sepia", "fr": "Sépia",
        "it": "Seppia", "ru": "Сепия", "zh_CN": "复古棕褐", "zh_TW": "復古深褐", "ko": "세피아",
        "pl": "Sepia", "nl": "Sepia", "uk": "Сепія", "tr": "Sepya", "sv": "Sepia",
        "cs": "Sépie", "ca": "Sèpia", "ar": "بني داكن", "hi": "सीपिया", "hu": "Szépia",
        "da": "Sepia", "fi": "Seepia", "nb": "Sepia", "el": "Σέπια", "id": "Sepia",
        "ro": "Sepia", "sk": "Sépia", "sl": "Sepija", "bg": "Сепия", "eu": "Sepia",
        "gl": "Sepia", "et": "Seepia"
    },
    "Posterize": {
        "pt": "Posterizar", "es": "Posterizar", "ja": "ポスタリゼーション", "de": "Posterisieren",
        "fr": "Postériser", "it": "Posterizza", "ru": "Постеризация", "zh_CN": "色调分离",
        "zh_TW": "色調分離", "ko": "포스터화", "pl": "Posteryzacja", "nl": "Plakkaateffect", "uk": "Постеризація",
        "tr": "Posterleştir", "sv": "Färgreducering", "cs": "Posterizace", "ca": "Posterització",
        "ar": "تأثير الملصق", "hi": "पोस्टर करें", "hu": "Poszterizálás", "da": "Plakateffekt", "fi": "Julistetehoste",
        "nb": "Plakateffekt", "el": "Ποστεροποίηση", "id": "Posterisasi", "ro": "Posterizare",
        "sk": "Posterizácia", "sl": "Plakatiranje", "bg": "Постеризация", "eu": "Posterizatu",
        "gl": "Posterizar", "et": "Posteriseerimine"
    },
    "Emboss": {
        "pt": "Relevo", "es": "Relieve", "ja": "エンボス", "de": "Relief", "fr": "Gaufrage",
        "it": "Rilievo", "ru": "Тиснение", "zh_CN": "浮雕立体", "zh_TW": "浮雕效果", "ko": "엠보스",
        "pl": "Płaskorzeźba", "nl": "Reliëf", "uk": "Тиснення", "tr": "Kabartma", "sv": "Relief",
        "cs": "Reliéf", "ca": "Relleu", "ar": "نقش بارز", "hi": "उभार", "hu": "Domborítás",
        "da": "Prægning", "fi": "Kohokuvio", "nb": "Relieff", "el": "Ανάγλυφο", "id": "Timbul",
        "ro": "Relief", "sk": "Reliéf", "sl": "Relief", "bg": "Релеф", "eu": "Erliebea",
        "gl": "Relevo", "et": "Reljeef"
    },
    "Old Photograph": {
        "pt": "Fotografia Antiga", "es": "Fotografía Antigua", "ja": "古写真", "de": "Altes Foto",
        "fr": "Vieille photographie", "it": "Vecchia fotografia", "ru": "Старая фотография",
        "zh_CN": "老照片怀旧", "zh_TW": "老照片懷舊", "ko": "오래된 사진", "pl": "Stara fotografia",
        "nl": "Oude foto", "uk": "Стара фотографія", "tr": "Eski Fotoğraf", "sv": "Gammalt fotografi",
        "cs": "Stará fotografie", "ca": "Fotografia antiga", "ar": "صورة فوتوغرافية قديمة", "hi": "पुरानी तस्वीर",
        "hu": "Régi fénykép", "da": "Gammelt fotografi", "fi": "Vanha valokuva", "nb": "Gammelt fotografi",
        "el": "Παλιά φωτογραφία", "id": "Foto Kuno", "ro": "Fotografie veche", "sk": "Stará fotografia",
        "sl": "Stara fotografija", "bg": "Стара фотография", "eu": "Argazki zaharra", "gl": "Fotografía antiga",
        "et": "Vana foto"
    },
    "Neon": {
        "pt": "Néon", "es": "Neón", "ja": "ネオン", "de": "Neon", "fr": "Néon", "it": "Neon",
        "ru": "Неон", "zh_CN": "霓虹灯光", "zh_TW": "霓虹光芒", "ko": "네온", "pl": "Neon", "nl": "Neon",
        "uk": "Неон", "tr": "Neon", "sv": "Neon", "cs": "Neon", "ca": "Neó", "ar": "نيون",
        "hi": "नियॉन", "hu": "Neon", "da": "Neon", "fi": "Neon", "nb": "Neon", "el": "Νέον",
        "id": "Neon", "ro": "Neon", "sk": "Neón", "sl": "Neon", "bg": "Неон", "eu": "Neon",
        "gl": "Neón", "et": "Neoon"
    },
    "Film Grain": {
        "pt": "Grão de Filme", "es": "Grano de Película", "ja": "フィルムグレイン", "de": "Filmkorn",
        "fr": "Grain argentique", "it": "Grana della pellicola", "ru": "Зернистость пленки",
        "zh_CN": "胶片颗粒", "zh_TW": "膠片顆粒", "ko": "필름 그레인", "pl": "Ziarno filmowe",
        "nl": "Filmkorrel", "uk": "Зернистість плівки", "tr": "Film Greni", "sv": "Filmkorn",
        "cs": "Filmové zrno", "ca": "Gra de pel·lícula", "ar": "حبيبات الفيلم", "hi": "फिल्म ग्रेन",
        "hu": "Filmszemcse", "da": "Filmkorn", "fi": "Filmirae", "nb": "Filmkorn", "el": "Κόκκος φιλμ",
        "id": "Bintik Film", "ro": "Granulație de film", "sk": "Filmové zrno", "sl": "Filmsko zrno",
        "bg": "Филмово зърно", "eu": "Film alea", "gl": "Gran de película", "et": "Filmitera"
    },
    "Raindrops": {
        "pt": "Gotas de Chuva", "es": "Gotas de Lluvia", "ja": "雨粒", "de": "Regentropfen",
        "fr": "Gouttes de pluie", "it": "Gocce di pioggia", "ru": "Капли дождя", "zh_CN": "雨滴水滴",
        "zh_TW": "雨滴水珠", "ko": "빗방울", "pl": "Krople deszczu", "nl": "Regendruppels", "uk": "Краплі дощу",
        "tr": "Yağmur Damlaları", "sv": "Regndroppar", "cs": "Dešťové kapky", "ca": "Gotes de pluja",
        "ar": "قطرات المطر", "hi": "बारिश की बूँदें", "hu": "Esőcseppek", "da": "Regndråber", "fi": "Sadepisarat",
        "nb": "Regndråper", "el": "Σταγόνες βροχής", "id": "Tetesan Hujan", "ro": "Picături de ploaie",
        "sk": "Kvapky dažďa", "sl": "Dežne kaplje", "bg": "Дъждовни капки", "eu": "Euri-tantak",
        "gl": "Pingas de choiva", "et": "Vihmapiisad"
    },
    "Ripple": {
        "pt": "Ondulação", "es": "Ondulación", "ja": "波紋", "de": "Kräuselung", "fr": "Ondulation",
        "it": "Increspatura", "ru": "Рябь", "zh_CN": "水面水波", "zh_TW": "水面漣漪", "ko": "잔물결",
        "pl": "Fale wodne", "nl": "Rimpeling", "uk": "Брижі", "tr": "Dalgalanma", "sv": "Krusning",
        "cs": "Vlnění", "ca": "Ondulació", "ar": "تموج", "hi": "तरंग", "hu": "Fodrozódás",
        "da": "Krusning", "fi": "Väreily", "nb": "Krusning", "el": "Κυματισμός", "id": "Riak Air",
        "ro": "Ondulare", "sk": "Vlnenie", "sl": "Valovanje", "bg": "Вълнички", "eu": "Uhinak",
        "gl": "Ondulación", "et": "Värelus"
    },
    "Kaleidoscope": {
        "pt": "Caleidoscópio", "es": "Caleidoscopio", "ja": "万華鏡", "de": "Kaleidoskop",
        "fr": "Kaléidoscope", "it": "Caleidoscopio", "ru": "Калейдоскоп", "zh_CN": "万花筒",
        "zh_TW": "萬花筒", "ko": "만화경", "pl": "Kalejdoskop", "nl": "Caleidoscoop", "uk": "Калейдоскоп",
        "tr": "Çiçekdürbünü", "sv": "Kalejdoskop", "cs": "Kaleidoskop", "ca": "Calidoscopi",
        "ar": "مشكال", "hi": "बहुरूपदर्शक", "hu": "Kaleidoszkóp", "da": "Kalejdoskop", "fi": "Kaleidoskooppi",
        "nb": "Kaleidoskop", "el": "Καλειδοσκόπιο", "id": "Kaleidoskop", "ro": "Caleidoscop",
        "sk": "Kaleidoskop", "sl": "Kalejdoskop", "bg": "Калейдоскоп", "eu": "Kaleidoskopioa",
        "gl": "Caleidoscopio", "et": "Kaleidoskoop"
    },
    "Mosaic": {
        "pt": "Mosaico", "es": "Mosaico", "ja": "モザイク", "de": "Mosaik", "fr": "Mosaïque",
        "it": "Mosaico", "ru": "Мозаика", "zh_CN": "马赛克", "zh_TW": "馬賽克", "ko": "모자이크",
        "pl": "Mozaika", "nl": "Mozaïek", "uk": "Мозаїка", "tr": "Mozaik", "sv": "Mosaik",
        "cs": "Mozaika", "ca": "Mosaic", "ar": "فسيفساء", "hi": "मोज़ेक", "hu": "Mozaik",
        "da": "Mosaik", "fi": "Mosaiikki", "nb": "Mosaikk", "el": "Μωσαϊκό", "id": "Mosaik",
        "ro": "Mozaic", "sk": "Mozaika", "sl": "Mozaik", "bg": "Мозайка", "eu": "Mosaikoa",
        "gl": "Mosaico", "et": "Mosaiik"
    },
    "Glitch": {
        "pt": "Falha Digital / Glitch", "es": "Glitch", "ja": "グリッチ", "de": "Glitch", "fr": "Glitch",
        "it": "Glitch", "ru": "Глитч", "zh_CN": "故障艺术", "zh_TW": "故障藝術", "ko": "글리치",
        "pl": "Glitch", "nl": "Glitch", "uk": "Глітч", "tr": "Glitch", "sv": "Glitch",
        "cs": "Glitch", "ca": "Glitch", "ar": "خلل رقمي", "hi": "ग्लिच", "hu": "Glitch",
        "da": "Glitch", "fi": "Glitch", "nb": "Glitch", "el": "Glitch", "id": "Glitch",
        "ro": "Glitch", "sk": "Glitch", "sl": "Glitch", "bg": "Глич", "eu": "Glitch",
        "gl": "Glitch", "et": "Tõrge"
    }
}

UI_TRANSLATIONS = {"G'MIC Video - Import Screen": {'de': "G'MIC Video - Startbildschirm", 'fr': "G'MIC Video - Écran d'importation", 'it': "G'MIC Video - Schermata di importazione", 'ru': "G'MIC Video - Экран импорта", 'zh_CN': "G'MIC Video - 导入界面", 'zh_TW': "G'MIC Video - 匯入畫面", 'ko': "G'MIC Video - 가져오기 화면", 'pl': "G'MIC Video - Ekran importu", 'nl': "G'MIC Video - Importscherm", 'uk': "G'MIC Video - Екран імпорту", 'tr': "G'MIC Video - İçe Aktarma Ekranı", 'sv': "G'MIC Video - Importskärm", 'cs': "G'MIC Video - Obrazovka importu", 'ca': "G'MIC Video - Pantalla d'importació", 'ar': "G'MIC Video - شاشة الاستيراد", 'hi': "G'MIC Video - आयात स्क्रीन", 'hu': "G'MIC Video - Importálási képernyő", 'da': "G'MIC Video - Importskærm", 'fi': "G'MIC Video - Tuontinäyttö", 'nb': "G'MIC Video - Importskjerm", 'el': "G'MIC Video - Οθόνη εισαγωγής", 'id': "G'MIC Video - Layar Impor", 'ro': "G'MIC Video - Ecran de import", 'sk': "G'MIC Video - Obrazovka importu", 'sl': "G'MIC Video - Zaslon za uvoz", 'bg': "G'MIC Video - Екран за импортиране", 'eu': "G'MIC Video - Inportatze pantaila", 'gl': "G'MIC Video - Pantalla de importación", 'et': "G'MIC Video - Impordiekraan"}, "G'MIC Video - Effects Workspace": {'de': "G'MIC Video - Effekte-Arbeitsbereich", 'fr': "G'MIC Video - Espace d'effets", 'it': "G'MIC Video - Spazio di lavoro degli effetti", 'ru': "G'MIC Video - Рабочая область эффектов", 'zh_CN': "G'MIC Video - 特效工作区", 'zh_TW': "G'MIC Video - 特效工作區", 'ko': "G'MIC Video - 효과 작업 영역", 'pl': "G'MIC Video - Przestrzeń robocza efektów", 'nl': "G'MIC Video - Effectenwerkruimte", 'uk': "G'MIC Video - Робочий простір ефектів", 'tr': "G'MIC Video - Efekt Çalışma Alanı", 'sv': "G'MIC Video - Effektarbetsyta", 'cs': "G'MIC Video - Pracovní prostor efektů", 'ca': "G'MIC Video - Espai de treball d'efectes", 'ar': "G'MIC Video - مساحة عمل التأثيرات", 'hi': "G'MIC Video - प्रभाव कार्यक्षेत्र", 'hu': "G'MIC Video - Effekt munkaterület", 'da': "G'MIC Video - Effektarbejdsområde", 'fi': "G'MIC Video - Efektityötila", 'nb': "G'MIC Video - Effektarbeidsområde", 'el': "G'MIC Video - Χώρος εργασίας εφέ", 'id': "G'MIC Video - Ruang Kerja Efek", 'ro': "G'MIC Video - Spațiu de lucru pentru efecte", 'sk': "G'MIC Video - Pracovný priestor efektov", 'sl': "G'MIC Video - Delovni prostor za učinke", 'bg': "G'MIC Video - Работна площ за ефекти", 'eu': "G'MIC Video - Efektuen lan-eremua", 'gl': "G'MIC Video - Espazo de traballo de efectos", 'et': "G'MIC Video - Efektide tööala"}, 'Import Video': {'de': 'Video importieren', 'fr': 'Importer une vidéo', 'it': 'Importa video', 'ru': 'Импортировать видео', 'zh_CN': '导入视频', 'zh_TW': '匯入影片', 'ko': '비디오 가져오기', 'pl': 'Importuj wideo', 'nl': 'Video importeren', 'uk': 'Імпортувати відео', 'tr': 'Videoyu İçe Aktar', 'sv': 'Importera video', 'cs': 'Importovat video', 'ca': 'Importa vídeo', 'ar': 'استيراد فيديو', 'hi': 'वीडियो आयात करें', 'hu': 'Videó importálása', 'da': 'Importér video', 'fi': 'Tuo video', 'nb': 'Importer video', 'el': 'Εισαγωγή βίντεο', 'id': 'Impor Video', 'ro': 'Importă video', 'sk': 'Importovať video', 'sl': 'Uvozi video', 'bg': 'Импортиране на видео', 'eu': 'Inportatu bideoa', 'gl': 'Importar vídeo', 'et': 'Impordi video'}, 'Import Video File...': {'de': 'Videodatei importieren...', 'fr': 'Importer un fichier vidéo...', 'it': 'Importa file video...', 'ru': 'Импортировать видеофайл...', 'zh_CN': '导入视频文件...', 'zh_TW': '匯入影片檔案...', 'ko': '비디오 파일 가져오기...', 'pl': 'Importuj plik wideo...', 'nl': 'Videobestand importeren...', 'uk': 'Імпортувати відеофайл...', 'tr': 'Video Dosyası İçe Aktar...', 'sv': 'Importera videofil...', 'cs': 'Importovat soubor videa...', 'ca': 'Importa fitxer de vídeo...', 'ar': 'استيراد ملف فيديو...', 'hi': 'वीडियो फ़ाइल आयात करें...', 'hu': 'Videofájl importálása...', 'da': 'Importér videofil...', 'fi': 'Tuo videotiedosto...', 'nb': 'Importer videofil...', 'el': 'Εισαγωγή αρχείου βίντεο...', 'id': 'Impor Berkas Video...', 'ro': 'Importă fișier video...', 'sk': 'Importovať video súbor...', 'sl': 'Uvozi video datoteko...', 'bg': 'Импортиране на видеофайл...', 'eu': 'Inportatu bideo fitxategia...', 'gl': 'Importar ficheiro de vídeo...', 'et': 'Impordi videofail...'}, "Import a Video to Apply G'MIC Effects": {'de': "Importieren Sie ein Video, um G'MIC-Effekte anzuwenden", 'fr': "Importez une vidéo pour appliquer des effets G'MIC", 'it': "Importa un video per applicare gli effetti G'MIC", 'ru': "Импортируйте видео для применения эффектов G'MIC", 'zh_CN': "导入视频以应用 G'MIC 特效", 'zh_TW': "匯入影片以套用 G'MIC 特效", 'ko': "G'MIC 효과를 적용할 비디오를 가져옵니다", 'pl': "Zaimportuj wideo, aby zastosować efekty G'MIC", 'nl': "Importeer een video om G'MIC-effecten toe te passen", 'uk': "Імпортуйте відео, щоб застосувати ефекти G'MIC", 'tr': "G'MIC Efektlerini Uygulamak İçin Bir Video İçe Aktarın", 'sv': "Importera en video för att tillämpa G'MIC-effekter", 'cs': "Importujte video pro aplikaci efektů G'MIC", 'ca': "Importeu un vídeo per aplicar efectes G'MIC", 'ar': "استورد فيديو لتطبيق تأثيرات G'MIC", 'hi': "G'MIC प्रभाव लागू करने के लिए एक वीडियो आयात करें", 'hu': "Importáljon videót a G'MIC effektek alkalmazásához", 'da': "Importér en video for at anvende G'MIC-effekter", 'fi': "Tuo video käyttääksesi G'MIC-efektejä", 'nb': "Importer en video for å bruke G'MIC-effekter", 'el': "Εισαγάγετε ένα βίντεο για εφαρμογή εφέ G'MIC", 'id': "Impor video untuk menerapkan efek G'MIC", 'ro': "Importați un videoclip pentru a aplica efecte G'MIC", 'sk': "Importujte video pre aplikáciu efektov G'MIC", 'sl': "Uvozite video za uveljavitev učinkov G'MIC", 'bg': "Импортирайте видео за прилагане на G'MIC ефекти", 'eu': "Inportatu bideo bat G'MIC efektuak aplikatzeko", 'gl': "Importe un vídeo para aplicar efectos G'MIC", 'et': "Impordi video G'MIC efektide rakendamiseks"}, "Select a video to start adding animated G'MIC effects with custom tweening, easing and curves.": {'de': "Wählen Sie ein Video aus, um animierte G'MIC-Effekte mit Übergängen und Kurven hinzuzufügen.", 'fr': "Sélectionnez une vidéo pour ajouter des effets G'MIC animés avec interpolations et courbes.", 'it': "Seleziona un video per aggiungere effetti G'MIC animati con interpolazione e curve personalizzate.", 'ru': "Выберите видео для добавления анимированных эффектов G'MIC с плавными переходами и кривыми.", 'zh_CN': "选择视频开始添加带有关键帧缓动曲线的 G'MIC 动画特效。", 'zh_TW': "選取影片以開始新增帶有自訂曲線的動畫 G'MIC 特效。", 'ko': "사용자 지정 감속/가속 및 곡선으로 애니메이션 G'MIC 효과를 추가할 비디오를 선택하세요.", 'pl': "Wybierz wideo, aby dodać animowane efekty G'MIC z płynnymi przejściami i krzywymi.", 'nl': "Selecteer een video om geanimeerde G'MIC-effecten toe te voegen met overgangen en curven.", 'uk': "Виберіть відео, щоб додати анімовані ефекти G'MIC з переходами та кривими.", 'tr': "Özel geçişler ve eğrilerle animasyonlu G'MIC efektleri eklemek için bir video seçin.", 'sv': "Välj en video för att lägga till animerade G'MIC-effekter med anpassade kurvor.", 'cs': "Vyberte video a začněte přidávat animované efekty G'MIC s vlastními křivkami.", 'ca': "Seleccioneu un vídeo per afegir efectes G'MIC animats amb corbes personalitzades.", 'ar': "حدد مقطع فيديو لبدء إضافة تأثيرات G'MIC متحركة مع منحنيات وانتقالات مخصصة.", 'hi': "कस्टम ट्विनिंग और कर्व्स के साथ एनिमेटेड G'MIC प्रभाव जोड़ने के लिए एक वीडियो चुनें।", 'hu': "Válasszon egy videót animált G'MIC effektek hozzáadásához egyéni görbékkel.", 'da': "Vælg en video for at tilføje animerede G'MIC-effekter med kurver.", 'fi': "Valitse video lisätäksesi animoituja G'MIC-efektejä mukautetuilla käyrillä.", 'nb': "Velg en video for å legge til animerte G'MIC-effekter med egendefinerte kurver.", 'el': "Επιλέξτε ένα βίντεο για να προσθέσετε κινούμενα εφέ G'MIC με προσαρμοσμένες καμπύλες.", 'id': "Pilih video untuk mulai menambahkan efek animasi G'MIC dengan kurva kustom.", 'ro': "Selectați un videoclip pentru a adăuga efecte G'MIC animate cu curbe personalizate.", 'sk': "Vyberte video a začnite pridávať animované efekty G'MIC s vlastnými krivkami.", 'sl': "Izberite video za dodajanje animiranih učinkov G'MIC s krivuljami.", 'bg': "Изберете видео, за да добавите анимирани G'MIC ефекти с персонализирани криви.", 'eu': "Hautatu bideo bat G'MIC efektu animatuak gehitzeko kurba pertsonalizatuekin.", 'gl': "Seleccione un vídeo para engadir efectos G'MIC animados con curvas personalizadas.", 'et': "Vali video, et lisada animeeritud G'MIC efekte kohandatud kõveratega."}, 'Load Sample: Original.mp4': {'de': 'Beispiel laden: Original.mp4', 'fr': "Charger l'échantillon : Original.mp4", 'it': 'Carica campione: Original.mp4', 'ru': 'Загрузить пример: Original.mp4', 'zh_CN': '载入示例: Original.mp4', 'zh_TW': '載入範例: Original.mp4', 'ko': '샘플 불러오기: Original.mp4', 'pl': 'Wczytaj próbkę: Original.mp4', 'nl': 'Voorbeeld laden: Original.mp4', 'uk': 'Завантажити зразок: Original.mp4', 'tr': 'Örneği Yükle: Original.mp4', 'sv': 'Ladda exempel: Original.mp4', 'cs': 'Načíst ukázku: Original.mp4', 'ca': 'Carrega mostra: Original.mp4', 'ar': 'تحميل عينة: Original.mp4', 'hi': 'नमूना लोड करें: Original.mp4', 'hu': 'Minta betöltése: Original.mp4', 'da': 'Indlæs eksempel: Original.mp4', 'fi': 'Lataa näyte: Original.mp4', 'nb': 'Last inn eksempel: Original.mp4', 'el': 'Φόρτωση δείγματος: Original.mp4', 'id': 'Muat Sampel: Original.mp4', 'ro': 'Încarcă mostră: Original.mp4', 'sk': 'Načítať ukážku: Original.mp4', 'sl': 'Naloži vzorec: Original.mp4', 'bg': 'Зареждане на образец: Original.mp4', 'eu': 'Kargatu lagina: Original.mp4', 'gl': 'Cargar mostra: Original.mp4', 'et': 'Laadi näidis: Original.mp4'}, 'Render': {'de': 'Rendern', 'fr': 'Rendre', 'it': 'Rendering', 'ru': 'Рендеринг', 'zh_CN': '渲染', 'zh_TW': '算圖', 'ko': '렌더링', 'pl': 'Renderuj', 'nl': 'Renderen', 'uk': 'Рендеринг', 'tr': 'Oluştur', 'sv': 'Rendera', 'cs': 'Vykreslit', 'ca': 'Renderitza', 'ar': 'تصيير', 'hi': 'रेंडर करें', 'hu': 'Renderelés', 'da': 'Render', 'fi': 'Renderöi', 'nb': 'Rendre', 'el': 'Απόδοση', 'id': 'Render', 'ro': 'Randare', 'sk': 'Vykresliť', 'sl': 'Izriši', 'bg': 'Рендиране', 'eu': 'Errendatu', 'gl': 'Renderizar', 'et': 'Renderda'}, 'Render (%1 in queue | %2%)': {'de': 'Rendern (%1 in Warteschlange | %2%)', 'fr': 'Rendu (%1 en file | %2%)', 'it': 'Rendering (%1 in coda | %2%)', 'ru': 'Рендеринг (%1 в очереди | %2%)', 'zh_CN': '渲染 (队列中 %1 个 | %2%)', 'zh_TW': '算圖 (隊列中 %1 | %2%)', 'ko': '렌더링 (대기열 %1개 | %2%)', 'pl': 'Renderowanie (%1 w kolejce | %2%)', 'nl': 'Renderen (%1 in wachtrij | %2%)', 'uk': 'Рендеринг (%1 в черзі | %2%)', 'tr': 'Oluştur (%1 kuyrukta | %2%)', 'sv': 'Rendering (%1 i kö | %2%)', 'cs': 'Vykreslování (%1 ve frontě | %2%)', 'ca': 'Renderització (%1 a la cua | %2%)', 'ar': 'تصيير (%1 في قائمة الانتظار | %2%)', 'hi': 'रेंडर (%1 कतार में | %2%)', 'hu': 'Renderelés (%1 a sorban | %2%)', 'da': 'Rendering (%1 i kø | %2%)', 'fi': 'Renderöinti (%1 jonossa | %2%)', 'nb': 'Rendring (%1 i kø | %2%)', 'el': 'Απόδοση (%1 στην ουρά | %2%)', 'id': 'Render (%1 dalam antrean | %2%)', 'ro': 'Randare (%1 în coadă | %2%)', 'sk': 'Vykresľovanie (%1 v poradí | %2%)', 'sl': 'Izrisovanje (%1 v čakalni vrsti | %2%)', 'bg': 'Рендиране (%1 на опашка | %2%)', 'eu': 'Errendatu (%1 ilaran | %2%)', 'gl': 'Renderizando (%1 na cola | %2%)', 'et': 'Renderdamine (%1 järjekorras | %2%)'}, 'Search effects...': {'de': 'Effekte suchen...', 'fr': 'Rechercher des effets...', 'it': 'Cerca effetti...', 'ru': 'Поиск эффектов...', 'zh_CN': '搜索特效...', 'zh_TW': '搜尋特效...', 'ko': '효과 검색...', 'pl': 'Szukaj efektów...', 'nl': 'Effecten zoeken...', 'uk': 'Пошук ефектів...', 'tr': 'Efektleri ara...', 'sv': 'Sök effekter...', 'cs': 'Hledat efekty...', 'ca': 'Cerca efectes...', 'ar': 'البحث عن التأثيرات...', 'hi': 'प्रभाव खोजें...', 'hu': 'Effektek keresése...', 'da': 'Søg efter effekter...', 'fi': 'Etsi efektejä...', 'nb': 'Søk etter effekter...', 'el': 'Αναζήτηση εφέ...', 'id': 'Cari efek...', 'ro': 'Căutare efecte...', 'sk': 'Hľadať efekty...', 'sl': 'Išči učinke...', 'bg': 'Търсене на ефекти...', 'eu': 'Bilatu efektuak...', 'gl': 'Buscar efectos...', 'et': 'Otsi efekte...'}, 'Visual': {'de': 'Visuell', 'fr': 'Visuel', 'it': 'Visuale', 'ru': 'Визуальный', 'zh_CN': '视觉', 'zh_TW': '視覺', 'ko': '시각적', 'pl': 'Wizualne', 'nl': 'Visueel', 'uk': 'Візуальний', 'tr': 'Görsel', 'sv': 'Visuell', 'cs': 'Vizuální', 'ca': 'Visual', 'ar': 'مرئي', 'hi': 'विजुअल', 'hu': 'Vizuális', 'da': 'Visuel', 'fi': 'Visuaalinen', 'nb': 'Visuell', 'el': 'Οπτικό', 'id': 'Visual', 'ro': 'Vizual', 'sk': 'Vizuálne', 'sl': 'Vizualno', 'bg': 'Визуален', 'eu': 'Bisuala', 'gl': 'Visual', 'et': 'Visuaalne'}, 'Category:': {'de': 'Kategorie:', 'fr': 'Catégorie :', 'it': 'Categoria:', 'ru': 'Категория:', 'zh_CN': '分类:', 'zh_TW': '分類:', 'ko': '카테고리:', 'pl': 'Kategoria:', 'nl': 'Categorie:', 'uk': 'Категорія:', 'tr': 'Kategori:', 'sv': 'Kategori:', 'cs': 'Kategorie:', 'ca': 'Categoria:', 'ar': 'الفئة:', 'hi': 'श्रेणी:', 'hu': 'Kategória:', 'da': 'Kategori:', 'fi': 'Luokka:', 'nb': 'Kategori:', 'el': 'Κατηγορία:', 'id': 'Kategori:', 'ro': 'Categorie:', 'sk': 'Kategória:', 'sl': 'Kategorija:', 'bg': 'Категория:', 'eu': 'Kategoria:', 'gl': 'Categoría:', 'et': 'Kategooria:'}, 'Effects:': {'de': 'Effekte:', 'fr': 'Effets :', 'it': 'Effetti:', 'ru': 'Эффекты:', 'zh_CN': '特效:', 'zh_TW': '特效:', 'ko': '효과:', 'pl': 'Efekty:', 'nl': 'Effecten:', 'uk': 'Ефекти:', 'tr': 'Efektler:', 'sv': 'Effekter:', 'cs': 'Efekty:', 'ca': 'Efectes:', 'ar': 'التأثيرات:', 'hi': 'प्रभाव:', 'hu': 'Effektek:', 'da': 'Effekter:', 'fi': 'Efektit:', 'nb': 'Effekter:', 'el': 'Εφέ:', 'id': 'Efek:', 'ro': 'Efecte:', 'sk': 'Efekty:', 'sl': 'Učinki:', 'bg': 'Ефекти:', 'eu': 'Efektuak:', 'gl': 'Efectos:', 'et': 'Efektid:'}, 'Select an Effect': {'de': 'Wählen Sie einen Effekt', 'fr': 'Sélectionner un effet', 'it': 'Seleziona un effetto', 'ru': 'Выберите эффект', 'zh_CN': '选择一个特效', 'zh_TW': '選取一個特效', 'ko': '효과 선택', 'pl': 'Wybierz efekt', 'nl': 'Selecteer een effect', 'uk': 'Виберіть ефект', 'tr': 'Bir Efekt Seçin', 'sv': 'Välj en effekt', 'cs': 'Vyberte efekt', 'ca': 'Seleccioneu un efecte', 'ar': 'اختر تأثيراً', 'hi': 'एक प्रभाव चुनें', 'hu': 'Válasszon egy effektet', 'da': 'Vælg en effekt', 'fi': 'Valitse efekti', 'nb': 'Velg en effekt', 'el': 'Επιλέξτε ένα εφέ', 'id': 'Pilih Efek', 'ro': 'Selectați un efect', 'sk': 'Vyberte efekt', 'sl': 'Izberite učinek', 'bg': 'Изберете ефект', 'eu': 'Hautatu efektu bat', 'gl': 'Seleccione un efecto', 'et': 'Vali efekt'}, 'Save .geffect...': {'de': '.geffect speichern...', 'fr': 'Enregistrer .geffect...', 'it': 'Salva .geffect...', 'ru': 'Сохранить .geffect...', 'zh_CN': '保存 .geffect...', 'zh_TW': '儲存 .geffect...', 'ko': '.geffect 저장...', 'pl': 'Zapisz .geffect...', 'nl': '.geffect opslaan...', 'uk': 'Зберегти .geffect...', 'tr': '.geffect Kaydet...', 'sv': 'Spara .geffect...', 'cs': 'Uložit .geffect...', 'ca': 'Desa .geffect...', 'ar': 'حفظ .geffect...', 'hi': '.geffect सहेजें...', 'hu': '.geffect mentése...', 'da': 'Gem .geffect...', 'fi': 'Tallenna .geffect...', 'nb': 'Lagre .geffect...', 'el': 'Αποθήκευση .geffect...', 'id': 'Simpan .geffect...', 'ro': 'Salvare .geffect...', 'sk': 'Uložiť .geffect...', 'sl': 'Shrani .geffect...', 'bg': 'Запазване на .geffect...', 'eu': 'Gorde .geffect...', 'gl': 'Gardar .geffect...', 'et': 'Salvesta .geffect...'}, 'Load .geffect...': {'de': '.geffect laden...', 'fr': 'Charger .geffect...', 'it': 'Carica .geffect...', 'ru': 'Загрузить .geffect...', 'zh_CN': '加载 .geffect...', 'zh_TW': '載入 .geffect...', 'ko': '.geffect 불러오기...', 'pl': 'Wczytaj .geffect...', 'nl': '.geffect laden...', 'uk': 'Завантажити .geffect...', 'tr': '.geffect Yükle...', 'sv': 'Ladda .geffect...', 'cs': 'Načíst .geffect...', 'ca': 'Carrega .geffect...', 'ar': 'تحميل .geffect...', 'hi': '.geffect लोड करें...', 'hu': '.geffect betöltése...', 'da': 'Indlæs .geffect...', 'fi': 'Lataa .geffect...', 'nb': 'Last inn .geffect...', 'el': 'Φόρτωση .geffect...', 'id': 'Muat .geffect...', 'ro': 'Încărcare .geffect...', 'sk': 'Načítať .geffect...', 'sl': 'Naloži .geffect...', 'bg': 'Зареждане на .geffect...', 'eu': 'Kargatu .geffect...', 'gl': 'Cargar .geffect...', 'et': 'Laadi .geffect...'}, 'Reset Defaults': {'de': 'Standardwerte wiederherstellen', 'fr': 'Réinitialiser les valeurs', 'it': 'Ripristina predefiniti', 'ru': 'Сбросить по умолчанию', 'zh_CN': '恢复默认值', 'zh_TW': '恢復預設值', 'ko': '기본값으로 재설정', 'pl': 'Przywróć domyślne', 'nl': 'Standaardwaarden herstellen', 'uk': 'Скинути до типових', 'tr': 'Varsayılanlara Sıfırla', 'sv': 'Återställ standard', 'cs': 'Obnovit výchozí', 'ca': 'Restableix valors predeterminats', 'ar': 'إعادة التعيين إلى الافتراضي', 'hi': 'डिफ़ॉल्ट पर रीसेट करें', 'hu': 'Alapértékek visszaállítása', 'da': 'Nulstil til standarder', 'fi': 'Palauta oletukset', 'nb': 'Tilbakestill til standard', 'el': 'Επαναφορά προεπιλογών', 'id': 'Atur Ulang ke Default', 'ro': 'Resetare la implicite', 'sk': 'Obnoviť predvolené', 'sl': 'Ponastavi na privzeto', 'bg': 'Възстановяване по подразбиране', 'eu': 'Berrezarri balio lehenetsiak', 'gl': 'Restaurar valores predeterminados', 'et': 'Lähtesta vaikeväärtused'}, 'Preview': {'de': 'Vorschau', 'fr': 'Aperçu', 'it': 'Anteprima', 'ru': 'Предпросмотр', 'zh_CN': '预览', 'zh_TW': '預覽', 'ko': '미리보기', 'pl': 'Podgląd', 'nl': 'Voorbeeld', 'uk': 'Попередній перегляд', 'tr': 'Önizleme', 'sv': 'Förhandsvisning', 'cs': 'Náhled', 'ca': 'Previsualització', 'ar': 'معاينة', 'hi': 'पूर्वावलोकन', 'hu': 'Előnézet', 'da': 'Forhåndsvisning', 'fi': 'Esikatselu', 'nb': 'Forhåndsvisning', 'el': 'Προεπισκόπηση', 'id': 'Pratinjau', 'ro': 'Previzualizare', 'sk': 'Náhľad', 'sl': 'Predogled', 'bg': 'Преглед', 'eu': 'Aurrebista', 'gl': 'Vista previa', 'et': 'Eelvaade'}, 'Split View': {'de': 'Geteilte Ansicht', 'fr': 'Vue fractionnée', 'it': 'Vista divisa', 'ru': 'Разделенный вид', 'zh_CN': '分屏对比', 'zh_TW': '分割畫面', 'ko': '분할 보기', 'pl': 'Widok dzielony', 'nl': 'Gesplitste weergave', 'uk': 'Розділений перегляд', 'tr': 'Bölünmüş Görünüm', 'sv': 'Delad vy', 'cs': 'Rozdělené zobrazení', 'ca': 'Vista dividida', 'ar': 'عرض منقسم', 'hi': 'विभाजित दृश्य', 'hu': 'Osztott nézet', 'da': 'Opdelt visning', 'fi': 'Jaettu näkymä', 'nb': 'Delt visning', 'el': 'Διαιρεμένη προβολή', 'id': 'Tampilan Pisah', 'ro': 'Vizualizare divizată', 'sk': 'Rozdelené zobrazenie', 'sl': 'Deljeni pogled', 'bg': 'Разделен изглед', 'eu': 'Ikuspegi zatitua', 'gl': 'Vista dividida', 'et': 'Poolitatud vaade'}, 'Full View': {'de': 'Vollansicht', 'fr': 'Plein écran', 'it': 'Vista intera', 'ru': 'Полный вид', 'zh_CN': '完整画面', 'zh_TW': '完整畫面', 'ko': '전체 보기', 'pl': 'Pełny widok', 'nl': 'Volledige weergave', 'uk': 'Повний перегляд', 'tr': 'Tam Görünüm', 'sv': 'Full vy', 'cs': 'Úplné zobrazení', 'ca': 'Vista completa', 'ar': 'عرض كامل', 'hi': 'पूर्ण दृश्य', 'hu': 'Teljes nézet', 'da': 'Fuld visning', 'fi': 'Koko näkymä', 'nb': 'Full visning', 'el': 'Πλήρης προβολή', 'id': 'Tampilan Penuh', 'ro': 'Vizualizare completă', 'sk': 'Úplné zobrazenie', 'sl': 'Celotni pogled', 'bg': 'Пълен изглед', 'eu': 'Ikuspegi osoa', 'gl': 'Vista completa', 'et': 'Täisvaade'}, 'Refresh Preview': {'de': 'Vorschau aktualisieren', 'fr': "Actualiser l'aperçu", 'it': 'Aggiorna anteprima', 'ru': 'Обновить предпросмотр', 'zh_CN': '刷新预览', 'zh_TW': '重新整理預覽', 'ko': '미리보기 새로고침', 'pl': 'Odśwież podgląd', 'nl': 'Voorbeeld vernieuwen', 'uk': 'Оновити попередній перегляд', 'tr': 'Önizlemeyi Yenile', 'sv': 'Uppdatera förhandsgranskning', 'cs': 'Obnovit náhled', 'ca': 'Actualitza la previsualització', 'ar': 'تحديث المعاينة', 'hi': 'पूर्वावलोकन ताज़ा करें', 'hu': 'Előnézet frissítése', 'da': 'Opdater forhåndsvisning', 'fi': 'Päivitä esikatselu', 'nb': 'Oppdater forhåndsvisning', 'el': 'Ανανέωση προεπισκόπησης', 'id': 'Segarkan Pratinjau', 'ro': 'Reîmprospătare previzualizare', 'sk': 'Obnoviť náhľad', 'sl': 'Osveži predogled', 'bg': 'Обновяване на прегледа', 'eu': 'Freskatu aurrebista', 'gl': 'Actualizar vista previa', 'et': 'Värskenda eelvaadet'}, 'Original': {'de': 'Original', 'fr': 'Original', 'it': 'Originale', 'ru': 'Оригинал', 'zh_CN': '原片', 'zh_TW': '原始', 'ko': '원본', 'pl': 'Oryginał', 'nl': 'Origineel', 'uk': 'Оригінал', 'tr': 'Orijinal', 'sv': 'Original', 'cs': 'Originál', 'ca': 'Original', 'ar': 'الأصلي', 'hi': 'मूल', 'hu': 'Eredeti', 'da': 'Original', 'fi': 'Alkuperäinen', 'nb': 'Original', 'el': 'Πρωτότυπο', 'id': 'Asli', 'ro': 'Original', 'sk': 'Originál', 'sl': 'Izvirnik', 'bg': 'Оригинал', 'eu': 'Jatorrizkoa', 'gl': 'Orixinal', 'et': 'Originaal'}, "G'MIC Effect": {'de': "G'MIC-Effekt", 'fr': "Effet G'MIC", 'it': "Effetto G'MIC", 'ru': "Эффект G'MIC", 'zh_CN': "G'MIC 特效", 'zh_TW': "G'MIC 特效", 'ko': "G'MIC 효과", 'pl': "Efekt G'MIC", 'nl': "G'MIC-effect", 'uk': "Ефект G'MIC", 'tr': "G'MIC Efekti", 'sv': "G'MIC-effekt", 'cs': "Efekt G'MIC", 'ca': "Efecte G'MIC", 'ar': "تأثير G'MIC", 'hi': "G'MIC प्रभाव", 'hu': "G'MIC effekt", 'da': "G'MIC-effekt", 'fi': "G'MIC-efekti", 'nb': "G'MIC-effekt", 'el': "Εφέ G'MIC", 'id': "Efek G'MIC", 'ro': "Efect G'MIC", 'sk': "Efekt G'MIC", 'sl': "Učinek G'MIC", 'bg': "G'MIC ефект", 'eu': "G'MIC efektua", 'gl': "Efecto G'MIC", 'et': "G'MIC efekt"}, "Visual Effect Search - G'MIC Video": {'de': "Visuelle Effektsuche - G'MIC Video", 'fr': "Recherche visuelle d'effets - G'MIC Video", 'it': "Ricerca visuale degli effetti - G'MIC Video", 'ru': "Визуальный поиск эффектов - G'MIC Video", 'zh_CN': "视觉特效搜索 - G'MIC Video", 'zh_TW': "視覺特效搜尋 - G'MIC Video", 'ko': "시각적 효과 검색 - G'MIC Video", 'pl': "Wizualne wyszukiwanie efektów - G'MIC Video", 'nl': "Visueel effecten zoeken - G'MIC Video", 'uk': "Візуальний пошук ефектів - G'MIC Video", 'tr': "Görsel Efekt Arama - G'MIC Video", 'sv': "Visuell effektsökning - G'MIC Video", 'cs': "Vizuální vyhledávání efektů - G'MIC Video", 'ca': "Cerca visual d'efectes - G'MIC Video", 'ar': "البحث المرئي عن التأثيرات - G'MIC Video", 'hi': "विजुअल प्रभाव खोज - G'MIC Video", 'hu': "Vizuális effektkereső - G'MIC Video", 'da': "Visuel effektsøgning - G'MIC Video", 'fi': "Visuaalinen efekti-haku - G'MIC Video", 'nb': "Visuelt effektsøk - G'MIC Video", 'el': "Οπτική αναζήτηση εφέ - G'MIC Video", 'id': "Pencarian Efek Visual - G'MIC Video", 'ro': "Căutare vizuală a efectelor - G'MIC Video", 'sk': "Vizuálne vyhľadávanie efektov - G'MIC Video", 'sl': "Vizualno iskanje učinkov - G'MIC Video", 'bg': "Визуално търсене на ефекти - G'MIC Video", 'eu': "Efektuen bilaketa bisuala - G'MIC Video", 'gl': "Busca visual de efectos - G'MIC Video", 'et': "Visuaalne efektide otsing - G'MIC Video"}, 'Search effects visually (e.g. bokeh, warp, glitch, blur, halftone)...': {'de': 'Effekte visuell suchen (z.B. Bokeh, Warp, Glitch, Blur, Halftone)...', 'fr': 'Rechercher des effets visuellement (ex. bokeh, déformation, glitch, flou, demi-teinte)...', 'it': 'Cerca effetti visivamente (es. bokeh, warp, glitch, sfocatura, mezzatinta)...', 'ru': 'Визуальный поиск эффектов (напр. боке, деформация, глитч, размытие, полутон)...', 'zh_CN': '直观视觉搜索特效 (例如 bokeh, warp, glitch, blur, halftone)...', 'zh_TW': '以視覺方式搜尋特效 (例如 bokeh, warp, glitch, blur, halftone)...', 'ko': '시각적으로 효과 검색 (예: bokeh, warp, glitch, blur, halftone)...', 'pl': 'Szukaj efektów wizualnie (np. bokeh, warp, glitch, blur, halftone)...', 'nl': 'Zoek effecten visueel (bijv. bokeh, warp, glitch, blur, halftone)...', 'uk': 'Візуальний пошук ефектів (наприклад, bokeh, warp, glitch, blur, halftone)...', 'tr': 'Efektleri görsel olarak arayın (örn. bokeh, bükme, aksaklık, bulanıklık)...', 'sv': 'Sök effekter visuellt (t.ex. bokeh, warp, glitch, blur, halftone)...', 'cs': 'Hledejte efekty vizuálně (např. bokeh, warp, glitch, blur, halftone)...', 'ca': 'Cerca efectes visualment (p. ex. bokeh, distorsió, glitch, desenfocament)...', 'ar': 'البحث عن التأثيرات بصرياً (مثل bokeh, warp, glitch, blur)...', 'hi': 'दृष्टिगत रूप से प्रभाव खोजें (जैसे bokeh, warp, glitch, blur)...', 'hu': 'Keressen vizuálisan az effektek között (pl. bokeh, warp, glitch, blur)...', 'da': 'Søg visuelt efter effekter (f.eks. bokeh, warp, glitch, blur, halftone)...', 'fi': 'Etsi efektejä visuaalisesti (esim. bokeh, vääristymä, sumennus)...', 'nb': 'Søk etter effekter visuelt (f.eks. bokeh, warp, glitch, blur, halftone)...', 'el': 'Αναζήτηση εφέ οπτικά (π.χ. bokeh, warp, glitch, blur, halftone)...', 'id': 'Cari efek secara visual (mis. bokeh, warp, glitch, blur, halftone)...', 'ro': 'Căutați efecte vizual (de ex. bokeh, warp, glitch, blur, halftone)...', 'sk': 'Hľadajte efekty vizuálne (napr. bokeh, deformácia, rozmazanie)...', 'sl': 'Iščite učinke vizualno (npr. bokeh, popačenje, zameglitev)...', 'bg': 'Търсете ефекти визуално (напр. боке, деформация, замъгляване)...', 'eu': 'Bilatu efektuak bisualki (adib. bokeh, distortsioa, lausotzea)...', 'gl': 'Busque efectos visualmente (ex. bokeh, deformación, desenfoque)...', 'et': 'Otsi efekte visuaalselt (nt bokeh, deformatsioon, hägusus)...'}, 'Close': {'de': 'Schließen', 'fr': 'Fermer', 'it': 'Chiudi', 'ru': 'Закрыть', 'zh_CN': '关闭', 'zh_TW': '關閉', 'ko': '닫기', 'pl': 'Zamknij', 'nl': 'Sluiten', 'uk': 'Закрити', 'tr': 'Kapat', 'sv': 'Stäng', 'cs': 'Zavřít', 'ca': 'Tanca', 'ar': 'إغلاق', 'hi': 'बंद करें', 'hu': 'Bezárás', 'da': 'Luk', 'fi': 'Sulje', 'nb': 'Lukk', 'el': 'Κλείσιμο', 'id': 'Tutup', 'ro': 'Închide', 'sk': 'Zavrieť', 'sl': 'Zapri', 'bg': 'Затваряне', 'eu': 'Itxi', 'gl': 'Pechar', 'et': 'Sulge'}, 'Showing %1 effects': {'de': '%1 Effekte werden angezeigt', 'fr': 'Affichage de %1 effets', 'it': 'Visualizzazione di %1 effetti', 'ru': 'Показано эффектов: %1', 'zh_CN': '正在显示 %1 个特效', 'zh_TW': '顯示 %1 個特效', 'ko': '%1개 효과 표시 중', 'pl': 'Wyświetlanie %1 efektów', 'nl': '%1 effecten weergegeven', 'uk': 'Показано %1 ефектів', 'tr': '%1 efekt gösteriliyor', 'sv': 'Visar %1 effekter', 'cs': 'Zobrazeno %1 efektů', 'ca': 'Es mostren %1 efectes', 'ar': 'عرض %1 من التأثيرات', 'hi': '%1 प्रभाव दिखाए जा रहे हैं', 'hu': '%1 effekt megjelenítése', 'da': 'Viser %1 effekter', 'fi': 'Näytetään %1 efektiä', 'nb': 'Viser %1 effekter', 'el': 'Εμφάνιση %1 εφέ', 'id': 'Menampilkan %1 efek', 'ro': 'Se afișează %1 efecte', 'sk': 'Zobrazuje sa %1 efektov', 'sl': 'Prikazano %1 učinkov', 'bg': 'Показване на %1 ефекта', 'eu': '%1 efektu erakusten', 'gl': 'Mostrando %1 efectos', 'et': 'Kuvatakse %1 efekti'}, 'Showing %1 effects in %2': {'de': '%1 Effekte in %2 werden angezeigt', 'fr': 'Affichage de %1 effets dans %2', 'it': 'Visualizzazione di %1 effetti in %2', 'ru': 'Показано %1 эффектов в %2', 'zh_CN': '正在显示 %2 中的 %1 个特效', 'zh_TW': '顯示 %2 中的 %1 個特效', 'ko': '%2의 %1개 효과 표시 중', 'pl': 'Wyświetlanie %1 efektów w %2', 'nl': '%1 effecten in %2 weergegeven', 'uk': 'Показано %1 ефектів у %2', 'tr': '%2 içinde %1 efekt gösteriliyor', 'sv': 'Visar %1 effekter i %2', 'cs': 'Zobrazeno %1 efektů v %2', 'ca': 'Es mostren %1 efectes a %2', 'ar': 'عرض %1 من التأثيرات في %2', 'hi': '%2 में %1 प्रभाव दिखाए जा रहे हैं', 'hu': '%1 effekt megjelenítése itt: %2', 'da': 'Viser %1 effekter i %2', 'fi': 'Näytetään %1 efektiä luokassa %2', 'nb': 'Viser %1 effekter i %2', 'el': 'Εμφάνιση %1 εφέ σε %2', 'id': 'Menampilkan %1 efek di %2', 'ro': 'Se afișează %1 efecte în %2', 'sk': 'Zobrazuje sa %1 efektov v %2', 'sl': 'Prikazano %1 učinkov v %2', 'bg': 'Показване на %1 ефекта в %2', 'eu': '%1 efektu erakusten hemen: %2', 'gl': 'Mostrando %1 efectos en %2', 'et': 'Kuvatakse %1 efekti kategoorias %2'}, 'Click any effect card to apply it to your video': {'de': 'Klicken Sie auf eine Effektkarte, um sie auf Ihr Video anzuwenden', 'fr': "Cliquez sur une carte d'effet pour l'appliquer à votre vidéo", 'it': "Fai clic su una scheda per applicare l'effetto al tuo video", 'ru': 'Нажмите на карточку эффекта, чтобы применить его к видео', 'zh_CN': '点击任意特效卡片以将其应用到您的视频中', 'zh_TW': '按一下任一特效卡片以將其套用至您的影片', 'ko': '비디오에 적용하려면 효과 카드를 클릭하세요', 'pl': 'Kliknij kartę efektu, aby zastosować go do filmu', 'nl': 'Klik op een effectkaart om deze toe te passen op je video', 'uk': 'Клацніть на картку ефекту, щоб застосувати його до відео', 'tr': 'Videonuza uygulamak için herhangi bir efekt kartına tıklayın', 'sv': 'Klicka på ett effektkort för att tillämpa det på din video', 'cs': 'Kliknutím na kartu efektu jej aplikujete na video', 'ca': "Feu clic a qualsevol targeta d'efecte per aplicar-lo al vostre vídeo", 'ar': 'انقر على أي بطاقة تأثير لتطبيقها على الفيديو الخاص بك', 'hi': 'अपने वीडियो पर लागू करने के लिए किसी भी प्रभाव कार्ड पर क्लिक करें', 'hu': 'Kattintson egy effektkártyára a videóra való alkalmazásához', 'da': 'Klik på et effektkort for at anvende det på din video', 'fi': 'Napsauta mitä tahansa efektikorttia käyttääksesi sitä videossasi', 'nb': 'Klikk på et effektkort for å bruke det på videoen din', 'el': 'Κάντε κλικ σε οποιαδήποτε κάρτα εφέ για να την εφαρμόσετε στο βίντεό σας', 'id': 'Klik kartu efek mana saja untuk menerapkannya ke video Anda', 'ro': 'Faceți clic pe orice card de efect pentru a-l aplica videoclipului dvs.', 'sk': 'Kliknutím na kartu efektu ho aplikujete na video', 'sl': 'Kliknite katero koli kartico učinka, da jo uveljavite na videu', 'bg': 'Щракнете върху карта с ефект, за да го приложите към видеото', 'eu': 'Egin klik efektu-txartel batean zure bideoari aplikatzeko', 'gl': 'Faga clic en calquera tarxeta de efecto para aplicalo ao seu vídeo', 'et': 'Klõpsa mis tahes efektikaardil, et seda oma videole rakendada'}, 'No matching effects found': {'de': 'Keine passenden Effekte gefunden', 'fr': 'Aucun effet correspondant trouvé', 'it': 'Nessun effetto corrispondente trovato', 'ru': 'Подходящих эффектов не найдено', 'zh_CN': '未找到匹配的特效', 'zh_TW': '找不到相符的特效', 'ko': '일치하는 효과를 찾을 수 없습니다', 'pl': 'Nie znaleziono pasujących efektów', 'nl': 'Geen overeenkomende effecten gevonden', 'uk': 'Не знайдено відповідних ефектів', 'tr': 'Eşleşen efekt bulunamadı', 'sv': 'Inga matchande effekter hittades', 'cs': 'Nenalezeny žádné odpovídající efekty', 'ca': "No s'han trobat efectes coincidents", 'ar': 'لم يتم العثور على تأثيرات مطابقة', 'hi': 'कोई मेल खाता प्रभाव नहीं मिला', 'hu': 'Nem található egyező effekt', 'da': 'Ingen matchende effekter fundet', 'fi': 'Vastaavia efektejä ei löytynyt', 'nb': 'Ingen matchende effekter funnet', 'el': 'Δεν βρέθηκαν αντίστοιχα εφέ', 'id': 'Tidak ada efek yang cocok ditemukan', 'ro': 'Nu s-au găsit efecte potrivite', 'sk': 'Nenašli sa žiadne zodpovedajúce efekty', 'sl': 'Ni najdenih ustreznih učinkov', 'bg': 'Не са намерени съответстващи ефекти', 'eu': 'Ez da bat datorren efekturik aurkitu', 'gl': 'Non se atoparon efectos coincidentes', 'et': 'Vastavaid efekte ei leitud'}, "Render Manager & Task Queue - G'MIC Video": {'de': "Render-Manager & Aufgabenwarteschlange - G'MIC Video", 'fr': "Gestionnaire de rendu et file d'attente - G'MIC Video", 'it': "Gestione del rendering e coda delle attività - G'MIC Video", 'ru': "Менеджер рендеринга и очередь задач - G'MIC Video", 'zh_CN': "渲染管理器与任务队列 - G'MIC Video", 'zh_TW': "算圖管理員與工作佇列 - G'MIC Video", 'ko': "렌더 관리자 및 작업 대기열 - G'MIC Video", 'pl': "Menedżer renderowania i kolejka zadań - G'MIC Video", 'nl': "Renderbeheer en taakwachtrij - G'MIC Video", 'uk': "Менеджер рендерингу та черга завдань - G'MIC Video", 'tr': "Oluşturma Yöneticisi ve Görev Kuyruğu - G'MIC Video", 'sv': "Renderingshanterare och uppgiftskö - G'MIC Video", 'cs': "Správce vykreslování a fronta úloh - G'MIC Video", 'ca': "Gestor de renderització i cua de tasques - G'MIC Video", 'ar': "مدير التصيير وقائمة انتظار المهام - G'MIC Video", 'hi': "रेंडर प्रबंधक और कार्य कतार - G'MIC Video", 'hu': "Rendereléskezelő és feladatsor - G'MIC Video", 'da': "Renderingshåndtering og opgavekø - G'MIC Video", 'fi': "Renderöinninhallinta ja tehtäväjono - G'MIC Video", 'nb': "Rendringsbehandling og oppgavekø - G'MIC Video", 'el': "Διαχειριστής απόδοσης & ουρά εργασιών - G'MIC Video", 'id': "Manajer Render & Antrean Tugas - G'MIC Video", 'ro': "Manager de randare și coadă de sarcini - G'MIC Video", 'sk': "Správca vykresľovania a front úloh - G'MIC Video", 'sl': "Upravitelj izrisovanja in čakalna vrsta opravil - G'MIC Video", 'bg': "Диспечер за рендиране и опашка от задачи - G'MIC Video", 'eu': "Errendatze-kudeatzailea eta zereginen ilara - G'MIC Video", 'gl': "Xestor de renderización e cola de tarefas - G'MIC Video", 'et': "Renderdamishaldur ja tegumijärjekord - G'MIC Video"}, 'New Render': {'de': 'Neues Rendering', 'fr': 'Nouveau rendu', 'it': 'Nuovo rendering', 'ru': 'Новый рендеринг', 'zh_CN': '新建渲染', 'zh_TW': '新算圖', 'ko': '새 렌더링', 'pl': 'Nowe renderowanie', 'nl': 'Nieuwe render', 'uk': 'Новий рендеринг', 'tr': 'Yeni Oluşturma', 'sv': 'Ny rendering', 'cs': 'Nové vykreslování', 'ca': 'Renderització nova', 'ar': 'تصيير جديد', 'hi': 'नया रेंडर', 'hu': 'Új renderelés', 'da': 'Ny rendering', 'fi': 'Uusi renderöinti', 'nb': 'Ny rendring', 'el': 'Νέα απόδοση', 'id': 'Render Baru', 'ro': 'Randare nouă', 'sk': 'Nové vykresľovanie', 'sl': 'Novo izrisovanje', 'bg': 'Ново рендиране', 'eu': 'Errendatze berria', 'gl': 'Nova renderización', 'et': 'Uus renderdamine'}, 'Task Queue (%1)': {'de': 'Aufgabenwarteschlange (%1)', 'fr': "File d'attente (%1)", 'it': 'Coda attività (%1)', 'ru': 'Очередь задач (%1)', 'zh_CN': '任务队列 (%1)', 'zh_TW': '工作佇列 (%1)', 'ko': '작업 대기열 (%1)', 'pl': 'Kolejka zadań (%1)', 'nl': 'Taakwachtrij (%1)', 'uk': 'Черга завдань (%1)', 'tr': 'Görev Kuyruğu (%1)', 'sv': 'Uppgiftskö (%1)', 'cs': 'Fronta úloh (%1)', 'ca': 'Cua de tasques (%1)', 'ar': 'قائمة انتظار المهام (%1)', 'hi': 'कार्य कतार (%1)', 'hu': 'Feladatsor (%1)', 'da': 'Opgavekø (%1)', 'fi': 'Tehtäväjono (%1)', 'nb': 'Oppgavekø (%1)', 'el': 'Ουρά εργασιών (%1)', 'id': 'Antrean Tugas (%1)', 'ro': 'Coadă sarcini (%1)', 'sk': 'Front úloh (%1)', 'sl': 'Čakalna vrsta opravil (%1)', 'bg': 'Опашка от задачи (%1)', 'eu': 'Zereginen ilara (%1)', 'gl': 'Cola de tarefas (%1)', 'et': 'Tegumijärjekord (%1)'}, 'When Rendering Completes:': {'de': 'Wenn das Rendering abgeschlossen ist:', 'fr': 'Lorsque le rendu est terminé :', 'it': 'Al completamento del rendering:', 'ru': 'По завершении рендеринга:', 'zh_CN': '渲染完成时:', 'zh_TW': '算圖完成時:', 'ko': '렌더링 완료 시:', 'pl': 'Po zakończeniu renderowania:', 'nl': 'Wanneer het renderen voltooid is:', 'uk': 'Після завершення рендерингу:', 'tr': 'Oluşturma Tamamlandığında:', 'sv': 'När renderingen är klar:', 'cs': 'Po dokončení vykreslování:', 'ca': 'Quan finalitzi la renderització:', 'ar': 'عند اكتمال التصيير:', 'hi': 'जब रेंडर पूरा हो जाए:', 'hu': 'A renderelés befejezésekor:', 'da': 'Når renderingen er færdig:', 'fi': 'Kun renderöinti valmistuu:', 'nb': 'Når gjengivelsen er ferdig:', 'el': 'Όταν ολοκληρωθεί η απόδοση:', 'id': 'Saat Rendering Selesai:', 'ro': 'Când randarea se finalizează:', 'sk': 'Po dokončení vykresľovania:', 'sl': 'Ko je izrisovanje končano:', 'bg': 'При завършване на рендирането:', 'eu': 'Errendatzea amaitzean:', 'gl': 'Ao completar a renderización:', 'et': 'Kui renderdamine on lõppenud:'}, 'Do nothing': {'de': 'Nichts tun', 'fr': 'Ne rien faire', 'it': 'Non fare nulla', 'ru': 'Ничего не делать', 'zh_CN': '无动作', 'zh_TW': '不執行動作', 'ko': '아무것도 안 함', 'pl': 'Nic nie rób', 'nl': 'Niets doen', 'uk': 'Нічого не робити', 'tr': 'Hiçbir şey yapma', 'sv': 'Gör ingenting', 'cs': 'Nedělat nic', 'ca': 'No facis res', 'ar': 'لا تفعل شيئاً', 'hi': 'कुछ न करें', 'hu': 'Ne tegyen semmit', 'da': 'Gør intet', 'fi': 'Älä tee mitään', 'nb': 'Ikke gjør noe', 'el': 'Καμία ενέργεια', 'id': 'Jangan lakukan apa-apa', 'ro': 'Nu face nimic', 'sk': 'Nerobiť nič', 'sl': 'Ne naredi ničesar', 'bg': 'Нищо', 'eu': 'Ez egin ezer', 'gl': 'Non facer nada', 'et': 'Ära tee midagi'}, 'Shutdown computer': {'de': 'Computer herunterfahren', 'fr': "Éteindre l'ordinateur", 'it': 'Spegni il computer', 'ru': 'Выключить компьютер', 'zh_CN': '关闭计算机', 'zh_TW': '將電腦關機', 'ko': '컴퓨터 종료', 'pl': 'Wyłącz komputer', 'nl': 'Computer afsluiten', 'uk': "Вимкнути комп'ютер", 'tr': 'Bilgisayarı kapat', 'sv': 'Stäng av datorn', 'cs': 'Vypnout počítač', 'ca': "Apaga l'ordinador", 'ar': 'إيقاف تشغيل الكمبيوتر', 'hi': 'कंप्यूटर बंद करें', 'hu': 'Számítógép leállítása', 'da': 'Sluk computeren', 'fi': 'Sammuta tietokone', 'nb': 'Slå av datamaskinen', 'el': 'Τερματισμός υπολογιστή', 'id': 'Matikan komputer', 'ro': 'Oprire computer', 'sk': 'Vypnúť počítač', 'sl': 'Zaustavi računalnik', 'bg': 'Изключване на компютъра', 'eu': 'Itzali ordenagailua', 'gl': 'Apagar o computador', 'et': 'Lülita arvuti välja'}, 'Add to Render Queue & Start': {'de': 'Zur Render-Warteschlange hinzufügen & Starten', 'fr': "Ajouter à la file d'attente et démarrer", 'it': 'Aggiungi alla coda di rendering e avvia', 'ru': 'Добавить в очередь и начать', 'zh_CN': '添加到渲染队列并开始', 'zh_TW': '新增至算圖佇列並開始', 'ko': '렌더 대기열에 추가하고 시작', 'pl': 'Dodaj do kolejki renderowania i rozpocznij', 'nl': 'Toevoegen aan renderwachtrij en starten', 'uk': 'Додати до черги рендерингу та почати', 'tr': 'Oluşturma Kuyruğuna Ekle ve Başlat', 'sv': 'Lägg till i renderingskön och starta', 'cs': 'Přidat do fronty vykreslování a spustit', 'ca': 'Afegeix a la cua de renderització i inicia', 'ar': 'إضافة إلى قائمة انتظار التصيير والبدء', 'hi': 'रेंडर कतार में जोड़ें और शुरू करें', 'hu': 'Hozzáadás a renderelési sorhoz és indítás', 'da': 'Føj til renderingskø og start', 'fi': 'Lisää renderöintijonoon ja aloita', 'nb': 'Legg til i rendringskø og start', 'el': 'Προσθήκη στην ουρά απόδοσης & έναρξη', 'id': 'Tambahkan ke Antrean Render & Mulai', 'ro': 'Adaugă la coada de randare și începe', 'sk': 'Pridať do frontu vykresľovania a spustiť', 'sl': 'Dodaj v čakalno vrsto za izrisovanje in začni', 'bg': 'Добавяне към опашката за рендиране и стартиране', 'eu': 'Gehitu errendatze-ilarara eta hasi', 'gl': 'Engadir á cola de renderización e iniciar', 'et': 'Lisa renderdamisjärjekorda ja alusta'}, 'Linear': {'de': 'Linear', 'fr': 'Linéaire', 'it': 'Lineare', 'ru': 'Линейный', 'zh_CN': '线性', 'zh_TW': '線性', 'ko': '선형', 'pl': 'Liniowy', 'nl': 'Lineair', 'uk': 'Лінійний', 'tr': 'Doğrusal', 'sv': 'Linjär', 'cs': 'Lineární', 'ca': 'Lineal', 'ar': 'خطي', 'hi': 'रैखिक', 'hu': 'Lineáris', 'da': 'Lineær', 'fi': 'Lineaarinen', 'nb': 'Lineær', 'el': 'Γραμμικό', 'id': 'Linear', 'ro': 'Liniar', 'sk': 'Lineárny', 'sl': 'Linearno', 'bg': 'Линеен', 'eu': 'Lineala', 'gl': 'Lineal', 'et': 'Lineaarne'}, 'Finished': {'de': 'Abgeschlossen', 'fr': 'Terminé', 'it': 'Completato', 'ru': 'Завершено', 'zh_CN': '已完成', 'zh_TW': '已完成', 'ko': '완료됨', 'pl': 'Zakończono', 'nl': 'Voltooid', 'uk': 'Завершено', 'tr': 'Tamamlandı', 'sv': 'Färdig', 'cs': 'Dokončeno', 'ca': 'Acabat', 'ar': 'مكتمل', 'hi': 'समाप्त', 'hu': 'Befejezve', 'da': 'Færdig', 'fi': 'Valmis', 'nb': 'Ferdig', 'el': 'Ολοκληρώθηκε', 'id': 'Selesai', 'ro': 'Finalizat', 'sk': 'Dokončené', 'sl': 'Končano', 'bg': 'Завършено', 'eu': 'Amaituta', 'gl': 'Rematado', 'et': 'Lõpetatud'}}

# Combine all translations
master_dict = {}
for code in KDE_LANGUAGES.keys():
    master_dict[code] = {}

# 1. Base from current translations
for k, v in en.items():
    master_dict["en"][k] = v
for k, v in pt.items():
    master_dict["pt"][k] = v
for k, v in es.items():
    master_dict["es"][k] = v
for k, v in ja.items():
    master_dict["ja"][k] = v

# 2. UI translations for all KDE languages
for en_k, lang_map in UI_TRANSLATIONS.items():
    for code, text in lang_map.items():
        if code in master_dict:
            master_dict[code][en_k] = text

# Fill any missing UI keys in other languages with sensible translation or fallback
for en_k, en_v in en.items():
    for code in KDE_LANGUAGES.keys():
        if en_k not in master_dict[code]:
            # If not in master_dict, use English key or fallback
            master_dict[code][en_k] = en_v

# 3. Add categories
for cat_k, cat_map in CATEGORIES.items():
    for code in KDE_LANGUAGES.keys():
        if code in cat_map:
            master_dict[code][cat_k] = cat_map[code]
        else:
            master_dict[code][cat_k] = cat_k

# 4. Add effect names
for eff_k, eff_map in EFFECT_NAMES.items():
    for code in KDE_LANGUAGES.keys():
        if code in eff_map:
            master_dict[code][eff_k] = eff_map[code]
        else:
            master_dict[code][eff_k] = eff_k

print("Generating src_gui/i18n.py file...")

header = '''"""
Internationalization (i18n) Engine for G'MIC Video
Supports all 33 KDE supported languages and automatically uses the host system's language.
Includes full translations for UI, categories, and effect names, with support for searching
using either the English name or localized name in text and visual search.
"""

import os
import sys
import locale
from PySide6.QtCore import QObject, Signal, Property, Slot
from PySide6.QtCore import QLocale

KDE_LANGUAGE_NAMES = ''' + json.dumps(KDE_LANGUAGES, ensure_ascii=False, indent=4) + '''

TRANSLATIONS = ''' + json.dumps(master_dict, ensure_ascii=False, indent=4) + '''


def detect_system_language():
    """Detect system locale and map to supported KDE languages."""
    candidates = []
    
    # 1. Environment variables (LC_ALL, LC_MESSAGES, LANG, LANGUAGE)
    for var in ("LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"):
        val = os.environ.get(var, "").strip()
        if val:
            for sub in val.split(":"):
                if sub:
                    candidates.append(sub)

    # 2. QLocale
    try:
        candidates.append(QLocale.system().name())
    except Exception:
        pass

    # 3. Python standard locale
    try:
        loc, _ = locale.getdefaultlocale()
        if loc:
            candidates.append(loc)
    except Exception:
        pass

    # Normalize and match against supported KDE languages
    for cand in candidates:
        cand_low = cand.lower().replace("-", "_")
        
        # Check specific Chinese variants first
        if cand_low.startswith("zh_cn") or cand_low.startswith("zh_hans"):
            return "zh_CN"
        if cand_low.startswith("zh_tw") or cand_low.startswith("zh_hk") or cand_low.startswith("zh_hant"):
            return "zh_TW"
        if cand_low.startswith("zh"):
            return "zh_CN"
            
        # 2-letter prefix matching
        code2 = cand_low[:2]
        if code2 in TRANSLATIONS:
            return code2
            
        # Special alias mappings
        if code2 == "no":
            return "nb"

    return "en"


class I18nManager(QObject):
    """QObject exposed to QML as `i18n`."""
    languageChanged = Signal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self._current_lang = detect_system_language()

    @Property(str, notify=languageChanged)
    def currentLanguage(self):
        return self._current_lang

    @Slot(str)
    def setLanguage(self, lang_code):
        if lang_code in TRANSLATIONS and lang_code != self._current_lang:
            self._current_lang = lang_code
            self.languageChanged.emit(lang_code)

    @Slot(str, result=str)
    def t(self, key):
        """Translate a string into the active language, with English fallback."""
        if not key:
            return ""
        lang_dict = TRANSLATIONS.get(self._current_lang, {})
        if key in lang_dict:
            return lang_dict[key]
        en_dict = TRANSLATIONS.get("en", {})
        return en_dict.get(key, key)

    @Slot(result=list)
    def availableLanguages(self):
        return [
            {"code": code, "name": KDE_LANGUAGE_NAMES.get(code, code)}
            for code in TRANSLATIONS.keys()
        ]
'''

with open("src_gui/i18n.py", "w", encoding="utf-8") as f:
    f.write(header)

print(f"Successfully generated src_gui/i18n.py with {len(master_dict)} languages!")
