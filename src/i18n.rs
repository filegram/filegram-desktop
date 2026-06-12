//! UI translations: one static string table per language. The effective
//! language defaults to the system locale and can be overridden from the
//! footer language menu; the choice persists via `settings`.

/// A UI language offered by the footer menu. `tag()` / `from_tag` round-trip
/// the BCP-47-style identifiers for persistence; `from_locale` maps an
/// arbitrary system locale onto the closest entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    EnUs,
    ZhCn,
    EsEs,
    HiIn,
    Ar,
    PtPt,
    JaJp,
    RuRu,
    FrFr,
    DeDe,
    Es419,
    Id,
    ItIt,
    Ko,
    PtBr,
    Th,
    Tr,
    Fa,
    Nl,
    Pl,
    Vi,
    Cs,
    El,
    Sv,
    Uk,
    Hu,
    Ro,
    Da,
    Fi,
    No,
    Sk,
    Bg,
    Hr,
    Lt,
    Sr,
    Lv,
    Sl,
    Et,
    He,
    Ms,
    Fil,
}

/// Regions whose Spanish reads as Latin American (plus the UN macro-region
/// code `419` itself); anything else falls back to European Spanish.
const LATIN_AMERICA: [&str; 21] = [
    "419", "ar", "bo", "cl", "co", "cr", "cu", "do", "ec", "gt", "hn", "mx", "ni", "pa", "pe",
    "pr", "py", "sv", "us", "uy", "ve",
];

impl Lang {
    /// The short menu: the most spoken languages, shown until the "…"
    /// entry expands the list to [`Lang::ALL`].
    pub const PRIMARY: [Lang; 17] = [
        Lang::EnUs,
        Lang::ZhCn,
        Lang::EsEs,
        Lang::HiIn,
        Lang::Ar,
        Lang::PtPt,
        Lang::JaJp,
        Lang::RuRu,
        Lang::FrFr,
        Lang::DeDe,
        Lang::Es419,
        Lang::Id,
        Lang::ItIt,
        Lang::Ko,
        Lang::PtBr,
        Lang::Th,
        Lang::Tr,
    ];

    /// Every language, ordered by the share of web content (the regional
    /// variants of Spanish and Portuguese sit together).
    pub const ALL: [Lang; 41] = [
        Lang::EnUs,
        Lang::EsEs,
        Lang::Es419,
        Lang::DeDe,
        Lang::JaJp,
        Lang::RuRu,
        Lang::FrFr,
        Lang::ZhCn,
        Lang::PtPt,
        Lang::PtBr,
        Lang::ItIt,
        Lang::Tr,
        Lang::HiIn,
        Lang::Fa,
        Lang::Nl,
        Lang::Pl,
        Lang::Id,
        Lang::Ar,
        Lang::Ko,
        Lang::Vi,
        Lang::Cs,
        Lang::El,
        Lang::Sv,
        Lang::Th,
        Lang::Uk,
        Lang::Hu,
        Lang::Ro,
        Lang::Da,
        Lang::Fi,
        Lang::No,
        Lang::Sk,
        Lang::Bg,
        Lang::Hr,
        Lang::Lt,
        Lang::Sr,
        Lang::Lv,
        Lang::Sl,
        Lang::Et,
        Lang::He,
        Lang::Ms,
        Lang::Fil,
    ];

    /// The identifier the settings file stores.
    pub fn tag(self) -> &'static str {
        match self {
            Lang::EnUs => "en-US",
            Lang::ZhCn => "zh-CN",
            Lang::EsEs => "es-ES",
            Lang::HiIn => "hi-IN",
            Lang::Ar => "ar",
            Lang::PtPt => "pt-PT",
            Lang::JaJp => "ja-JP",
            Lang::RuRu => "ru-RU",
            Lang::FrFr => "fr-FR",
            Lang::DeDe => "de-DE",
            Lang::Es419 => "es-419",
            Lang::Id => "id",
            Lang::ItIt => "it-IT",
            Lang::Ko => "ko",
            Lang::PtBr => "pt-BR",
            Lang::Th => "th",
            Lang::Tr => "tr",
            Lang::Fa => "fa",
            Lang::Nl => "nl",
            Lang::Pl => "pl",
            Lang::Vi => "vi",
            Lang::Cs => "cs",
            Lang::El => "el",
            Lang::Sv => "sv",
            Lang::Uk => "uk",
            Lang::Hu => "hu",
            Lang::Ro => "ro",
            Lang::Da => "da",
            Lang::Fi => "fi",
            Lang::No => "no",
            Lang::Sk => "sk",
            Lang::Bg => "bg",
            Lang::Hr => "hr",
            Lang::Lt => "lt",
            Lang::Sr => "sr",
            Lang::Lv => "lv",
            Lang::Sl => "sl",
            Lang::Et => "et",
            Lang::He => "he",
            Lang::Ms => "ms",
            Lang::Fil => "fil",
        }
    }

    /// The exact inverse of [`Lang::tag`]; `None` for anything else, so an
    /// edited settings file falls back to the system locale.
    pub fn from_tag(tag: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|lang| lang.tag() == tag)
    }

    /// The menu label: the language in its own writing, with a region where
    /// the menu offers two variants of the same language.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::EnUs => "English (US)",
            Lang::ZhCn => "中文（简体）",
            Lang::EsEs => "Español (España)",
            Lang::HiIn => "हिन्दी",
            Lang::Ar => "العربية",
            Lang::PtPt => "Português (Portugal)",
            Lang::JaJp => "日本語",
            Lang::RuRu => "Русский",
            Lang::FrFr => "Français",
            Lang::DeDe => "Deutsch",
            Lang::Es419 => "Español (Latinoamérica)",
            Lang::Id => "Bahasa Indonesia",
            Lang::ItIt => "Italiano",
            Lang::Ko => "한국어",
            Lang::PtBr => "Português (Brasil)",
            Lang::Th => "ไทย",
            Lang::Tr => "Türkçe",
            Lang::Fa => "فارسی",
            Lang::Nl => "Nederlands",
            Lang::Pl => "Polski",
            Lang::Vi => "Tiếng Việt",
            Lang::Cs => "Čeština",
            Lang::El => "Ελληνικά",
            Lang::Sv => "Svenska",
            Lang::Uk => "Українська",
            Lang::Hu => "Magyar",
            Lang::Ro => "Română",
            Lang::Da => "Dansk",
            Lang::Fi => "Suomi",
            Lang::No => "Norsk",
            Lang::Sk => "Slovenčina",
            Lang::Bg => "Български",
            Lang::Hr => "Hrvatski",
            Lang::Lt => "Lietuvių",
            Lang::Sr => "Српски",
            Lang::Lv => "Latviešu",
            Lang::Sl => "Slovenščina",
            Lang::Et => "Eesti",
            Lang::He => "עברית",
            Lang::Ms => "Bahasa Melayu",
            Lang::Fil => "Filipino",
        }
    }

    /// Best-effort match of a system locale (`en-US`, `de_DE.UTF-8`,
    /// `zh-Hans-CN`) onto a menu entry; unknown languages read as English.
    pub fn from_locale(locale: &str) -> Lang {
        let locale = locale.to_ascii_lowercase();
        let mut parts = locale.split(['-', '_', '.', '@']);
        let language = parts.next().unwrap_or("");
        // The first two-letter (or `419`) subtag is the region; scripts
        // (`hans`) and encodings (`utf`) are longer and skipped.
        let region = parts.find(|part| {
            *part == "419" || (part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
        });
        match language {
            "en" => Lang::EnUs,
            "zh" => Lang::ZhCn,
            "es" => {
                if region.is_some_and(|region| LATIN_AMERICA.contains(&region)) {
                    Lang::Es419
                } else {
                    Lang::EsEs
                }
            }
            "hi" => Lang::HiIn,
            "ar" => Lang::Ar,
            "pt" => {
                if region == Some("br") {
                    Lang::PtBr
                } else {
                    Lang::PtPt
                }
            }
            "ja" => Lang::JaJp,
            "ru" => Lang::RuRu,
            "fr" => Lang::FrFr,
            "de" => Lang::DeDe,
            // `in` is the legacy ISO code Java-era systems report for Indonesian.
            "id" | "in" => Lang::Id,
            "it" => Lang::ItIt,
            "ko" => Lang::Ko,
            "th" => Lang::Th,
            "tr" => Lang::Tr,
            "fa" => Lang::Fa,
            "nl" => Lang::Nl,
            "pl" => Lang::Pl,
            "vi" => Lang::Vi,
            "cs" => Lang::Cs,
            "el" => Lang::El,
            "sv" => Lang::Sv,
            "uk" => Lang::Uk,
            "hu" => Lang::Hu,
            "ro" => Lang::Ro,
            "da" => Lang::Da,
            "fi" => Lang::Fi,
            // Bokmål, Nynorsk and the macrolanguage code all read as Norwegian.
            "no" | "nb" | "nn" => Lang::No,
            "sk" => Lang::Sk,
            "bg" => Lang::Bg,
            "hr" => Lang::Hr,
            "lt" => Lang::Lt,
            "sr" => Lang::Sr,
            "lv" => Lang::Lv,
            "sl" => Lang::Sl,
            "et" => Lang::Et,
            // `iw` is the legacy ISO code Java-era systems report for Hebrew.
            "he" | "iw" => Lang::He,
            "ms" => Lang::Ms,
            // `tl` (Tagalog) is what most systems report for Filipino.
            "fil" | "tl" => Lang::Fil,
            _ => Lang::EnUs,
        }
    }

    /// The language of the OS user interface; English when the OS reports
    /// nothing.
    pub fn system() -> Lang {
        sys_locale::get_locale()
            .as_deref()
            .map(Lang::from_locale)
            .unwrap_or(Lang::EnUs)
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::EnUs => &EN_US,
            Lang::ZhCn => &ZH_CN,
            Lang::EsEs => &ES_ES,
            Lang::HiIn => &HI_IN,
            Lang::Ar => &AR,
            Lang::PtPt => &PT_PT,
            Lang::JaJp => &JA_JP,
            Lang::RuRu => &RU_RU,
            Lang::FrFr => &FR_FR,
            Lang::DeDe => &DE_DE,
            Lang::Es419 => &ES_419,
            Lang::Id => &ID,
            Lang::ItIt => &IT_IT,
            Lang::Ko => &KO,
            Lang::PtBr => &PT_BR,
            Lang::Th => &TH,
            Lang::Tr => &TR,
            Lang::Fa => &FA,
            Lang::Nl => &NL,
            Lang::Pl => &PL,
            Lang::Vi => &VI,
            Lang::Cs => &CS,
            Lang::El => &EL,
            Lang::Sv => &SV,
            Lang::Uk => &UK,
            Lang::Hu => &HU,
            Lang::Ro => &RO,
            Lang::Da => &DA,
            Lang::Fi => &FI,
            Lang::No => &NO,
            Lang::Sk => &SK,
            Lang::Bg => &BG,
            Lang::Hr => &HR,
            Lang::Lt => &LT,
            Lang::Sr => &SR,
            Lang::Lv => &LV,
            Lang::Sl => &SL,
            Lang::Et => &ET,
            Lang::He => &HE,
            Lang::Ms => &MS,
            Lang::Fil => &FIL,
        }
    }
}

/// Every user-visible string of the chrome. The brand name "Filegram" and
/// the version label stay untranslated by design.
pub struct Strings {
    pub app_title: &'static str,
    pub path_placeholder: &'static str,
    pub scan: &'static str,
    pub recent_scans: &'static str,
    pub disks: &'static str,
    pub home: &'static str,
    pub downloads: &'static str,
    pub desktop: &'static str,
    pub documents: &'static str,
    /// The scan progress prefix; the file counter is appended directly,
    /// so each translation includes whatever trailing spacing or
    /// punctuation its script needs before the number.
    pub scanning_files: &'static str,
    pub cancel: &'static str,
    pub new_scan: &'static str,
    pub trash_question: &'static str,
    pub folder: &'static str,
    pub file: &'static str,
    pub trash_button: &'static str,
    pub open_in_file_manager: &'static str,
    pub trash_tip: &'static str,
    pub light_theme: &'static str,
    pub dark_theme: &'static str,
    pub language: &'static str,
    pub hint_select: &'static str,
    pub hint_back: &'static str,
    /// The disk-usage bar connector: `{free} {disk_free} {total}`, e.g.
    /// "182.4 GB free of 494.4 GB".
    pub disk_free: &'static str,
}

static EN_US: Strings = Strings {
    app_title: "Filegram — disk map",
    path_placeholder: "Directory path…",
    scan: "Scan",
    recent_scans: "Recent scans",
    disks: "Disks",
    home: "Home",
    downloads: "Downloads",
    desktop: "Desktop",
    documents: "Documents",
    scanning_files: "Scanning… files: ",
    cancel: "Cancel",
    new_scan: "New scan",
    trash_question: "Move to trash?",
    folder: "Folder",
    file: "File",
    trash_button: "Move to Trash",
    open_in_file_manager: "Open in file manager",
    trash_tip: "Move to trash",
    light_theme: "Light theme",
    dark_theme: "Dark theme",
    language: "Language",
    hint_select: "select",
    hint_back: "back",
    disk_free: "free of",
};

static ZH_CN: Strings = Strings {
    app_title: "Filegram — 磁盘地图",
    path_placeholder: "目录路径…",
    scan: "扫描",
    recent_scans: "最近扫描",
    disks: "磁盘",
    home: "主目录",
    downloads: "下载",
    desktop: "桌面",
    documents: "文档",
    scanning_files: "正在扫描… 文件数：",
    cancel: "取消",
    new_scan: "新建扫描",
    trash_question: "移到回收站？",
    folder: "文件夹",
    file: "文件",
    trash_button: "移到回收站",
    open_in_file_manager: "在文件管理器中显示",
    trash_tip: "移到回收站",
    light_theme: "浅色主题",
    dark_theme: "深色主题",
    language: "语言",
    hint_select: "选择",
    hint_back: "返回",
    disk_free: "可用，共",
};

static ES_ES: Strings = Strings {
    app_title: "Filegram — mapa del disco",
    path_placeholder: "Ruta del directorio…",
    scan: "Escanear",
    recent_scans: "Escaneos recientes",
    disks: "Discos",
    home: "Inicio",
    downloads: "Descargas",
    desktop: "Escritorio",
    documents: "Documentos",
    scanning_files: "Escaneando… archivos: ",
    cancel: "Cancelar",
    new_scan: "Nuevo escaneo",
    trash_question: "¿Mover a la papelera?",
    folder: "Carpeta",
    file: "Archivo",
    trash_button: "Mover a la papelera",
    open_in_file_manager: "Mostrar en el gestor de archivos",
    trash_tip: "Mover a la papelera",
    light_theme: "Tema claro",
    dark_theme: "Tema oscuro",
    language: "Idioma",
    hint_select: "seleccionar",
    hint_back: "atrás",
    disk_free: "libres de",
};

static HI_IN: Strings = Strings {
    app_title: "Filegram — डिस्क मैप",
    path_placeholder: "डायरेक्टरी पथ…",
    scan: "स्कैन करें",
    recent_scans: "हाल के स्कैन",
    disks: "डिस्क",
    home: "होम",
    downloads: "डाउनलोड",
    desktop: "डेस्कटॉप",
    documents: "दस्तावेज़",
    scanning_files: "स्कैन हो रहा है… फ़ाइलें: ",
    cancel: "रद्द करें",
    new_scan: "नया स्कैन",
    trash_question: "ट्रैश में ले जाएँ?",
    folder: "फ़ोल्डर",
    file: "फ़ाइल",
    trash_button: "ट्रैश में ले जाएँ",
    open_in_file_manager: "फ़ाइल मैनेजर में दिखाएँ",
    trash_tip: "ट्रैश में ले जाएँ",
    light_theme: "हल्की थीम",
    dark_theme: "गहरी थीम",
    language: "भाषा",
    hint_select: "चुनें",
    hint_back: "वापस",
    disk_free: "मुक्त /",
};

static AR: Strings = Strings {
    app_title: "Filegram — خريطة القرص",
    path_placeholder: "مسار المجلد…",
    scan: "فحص",
    recent_scans: "عمليات الفحص الأخيرة",
    disks: "الأقراص",
    home: "المنزل",
    downloads: "التنزيلات",
    desktop: "سطح المكتب",
    documents: "المستندات",
    scanning_files: "جارٍ الفحص… الملفات: ",
    cancel: "إلغاء",
    new_scan: "فحص جديد",
    trash_question: "نقل إلى سلة المهملات؟",
    folder: "مجلد",
    file: "ملف",
    trash_button: "نقل إلى سلة المهملات",
    open_in_file_manager: "إظهار في مدير الملفات",
    trash_tip: "نقل إلى سلة المهملات",
    light_theme: "سمة فاتحة",
    dark_theme: "سمة داكنة",
    language: "اللغة",
    hint_select: "تحديد",
    hint_back: "رجوع",
    disk_free: "حر من",
};

static PT_PT: Strings = Strings {
    app_title: "Filegram — mapa do disco",
    path_placeholder: "Caminho do diretório…",
    scan: "Analisar",
    recent_scans: "Análises recentes",
    disks: "Discos",
    home: "Pasta pessoal",
    downloads: "Transferências",
    desktop: "Ambiente de trabalho",
    documents: "Documentos",
    scanning_files: "A analisar… ficheiros: ",
    cancel: "Cancelar",
    new_scan: "Nova análise",
    trash_question: "Mover para o lixo?",
    folder: "Pasta",
    file: "Ficheiro",
    trash_button: "Mover para o lixo",
    open_in_file_manager: "Mostrar no gestor de ficheiros",
    trash_tip: "Mover para o lixo",
    light_theme: "Tema claro",
    dark_theme: "Tema escuro",
    language: "Idioma",
    hint_select: "selecionar",
    hint_back: "voltar",
    disk_free: "livres de",
};

static JA_JP: Strings = Strings {
    app_title: "Filegram — ディスクマップ",
    path_placeholder: "ディレクトリのパス…",
    scan: "スキャン",
    recent_scans: "最近のスキャン",
    disks: "ディスク",
    home: "ホーム",
    downloads: "ダウンロード",
    desktop: "デスクトップ",
    documents: "書類",
    scanning_files: "スキャン中… ファイル数：",
    cancel: "キャンセル",
    new_scan: "新規スキャン",
    trash_question: "ゴミ箱に移動しますか？",
    folder: "フォルダ",
    file: "ファイル",
    trash_button: "ゴミ箱に移動",
    open_in_file_manager: "ファイルマネージャーで表示",
    trash_tip: "ゴミ箱に移動",
    light_theme: "ライトテーマ",
    dark_theme: "ダークテーマ",
    language: "言語",
    hint_select: "選択",
    hint_back: "戻る",
    disk_free: "空き /",
};

static RU_RU: Strings = Strings {
    app_title: "Filegram — карта диска",
    path_placeholder: "Путь к папке…",
    scan: "Сканировать",
    recent_scans: "Недавние сканирования",
    disks: "Диски",
    home: "Домашняя",
    downloads: "Загрузки",
    desktop: "Рабочий стол",
    documents: "Документы",
    scanning_files: "Сканирование… файлов: ",
    cancel: "Отмена",
    new_scan: "Новый скан",
    trash_question: "Переместить в корзину?",
    folder: "Папка",
    file: "Файл",
    trash_button: "В корзину",
    open_in_file_manager: "Показать в файловом менеджере",
    trash_tip: "Переместить в корзину",
    light_theme: "Светлая тема",
    dark_theme: "Тёмная тема",
    language: "Язык",
    hint_select: "выбрать",
    hint_back: "назад",
    disk_free: "свободно из",
};

static FR_FR: Strings = Strings {
    app_title: "Filegram — carte du disque",
    path_placeholder: "Chemin du dossier…",
    scan: "Analyser",
    recent_scans: "Analyses récentes",
    disks: "Disques",
    home: "Dossier personnel",
    downloads: "Téléchargements",
    desktop: "Bureau",
    documents: "Documents",
    scanning_files: "Analyse… fichiers : ",
    cancel: "Annuler",
    new_scan: "Nouvelle analyse",
    trash_question: "Déplacer vers la corbeille ?",
    folder: "Dossier",
    file: "Fichier",
    trash_button: "Déplacer vers la corbeille",
    open_in_file_manager: "Afficher dans le gestionnaire de fichiers",
    trash_tip: "Déplacer vers la corbeille",
    light_theme: "Thème clair",
    dark_theme: "Thème sombre",
    language: "Langue",
    hint_select: "sélectionner",
    hint_back: "retour",
    disk_free: "libres sur",
};

static DE_DE: Strings = Strings {
    app_title: "Filegram — Festplattenkarte",
    path_placeholder: "Verzeichnispfad…",
    scan: "Scannen",
    recent_scans: "Letzte Scans",
    disks: "Laufwerke",
    home: "Persönlicher Ordner",
    downloads: "Downloads",
    desktop: "Desktop",
    documents: "Dokumente",
    scanning_files: "Scanne… Dateien: ",
    cancel: "Abbrechen",
    new_scan: "Neuer Scan",
    trash_question: "In den Papierkorb verschieben?",
    folder: "Ordner",
    file: "Datei",
    trash_button: "In den Papierkorb",
    open_in_file_manager: "Im Dateimanager anzeigen",
    trash_tip: "In den Papierkorb verschieben",
    light_theme: "Helles Design",
    dark_theme: "Dunkles Design",
    language: "Sprache",
    hint_select: "auswählen",
    hint_back: "zurück",
    disk_free: "frei von",
};

static ES_419: Strings = Strings {
    app_title: "Filegram — mapa del disco",
    path_placeholder: "Ruta del directorio…",
    scan: "Escanear",
    recent_scans: "Escaneos recientes",
    disks: "Discos",
    home: "Inicio",
    downloads: "Descargas",
    desktop: "Escritorio",
    documents: "Documentos",
    scanning_files: "Escaneando… archivos: ",
    cancel: "Cancelar",
    new_scan: "Nuevo escaneo",
    trash_question: "¿Mover a la papelera?",
    folder: "Carpeta",
    file: "Archivo",
    trash_button: "Mover a la papelera",
    open_in_file_manager: "Mostrar en el administrador de archivos",
    trash_tip: "Mover a la papelera",
    light_theme: "Tema claro",
    dark_theme: "Tema oscuro",
    language: "Idioma",
    hint_select: "seleccionar",
    hint_back: "atrás",
    disk_free: "libres de",
};

static ID: Strings = Strings {
    app_title: "Filegram — peta disk",
    path_placeholder: "Jalur direktori…",
    scan: "Pindai",
    recent_scans: "Pemindaian terbaru",
    disks: "Disk",
    home: "Beranda",
    downloads: "Unduhan",
    desktop: "Desktop",
    documents: "Dokumen",
    scanning_files: "Memindai… berkas: ",
    cancel: "Batal",
    new_scan: "Pemindaian baru",
    trash_question: "Pindahkan ke tempat sampah?",
    folder: "Folder",
    file: "Berkas",
    trash_button: "Pindahkan ke Tempat Sampah",
    open_in_file_manager: "Tampilkan di pengelola berkas",
    trash_tip: "Pindahkan ke tempat sampah",
    light_theme: "Tema terang",
    dark_theme: "Tema gelap",
    language: "Bahasa",
    hint_select: "pilih",
    hint_back: "kembali",
    disk_free: "tersedia dari",
};

static IT_IT: Strings = Strings {
    app_title: "Filegram — mappa del disco",
    path_placeholder: "Percorso della directory…",
    scan: "Scansiona",
    recent_scans: "Scansioni recenti",
    disks: "Dischi",
    home: "Home",
    downloads: "Scaricati",
    desktop: "Scrivania",
    documents: "Documenti",
    scanning_files: "Scansione… file: ",
    cancel: "Annulla",
    new_scan: "Nuova scansione",
    trash_question: "Spostare nel cestino?",
    folder: "Cartella",
    file: "File",
    trash_button: "Sposta nel cestino",
    open_in_file_manager: "Mostra nel gestore file",
    trash_tip: "Sposta nel cestino",
    light_theme: "Tema chiaro",
    dark_theme: "Tema scuro",
    language: "Lingua",
    hint_select: "seleziona",
    hint_back: "indietro",
    disk_free: "liberi di",
};

static KO: Strings = Strings {
    app_title: "Filegram — 디스크 맵",
    path_placeholder: "디렉터리 경로…",
    scan: "스캔",
    recent_scans: "최근 스캔",
    disks: "디스크",
    home: "홈",
    downloads: "다운로드",
    desktop: "바탕 화면",
    documents: "문서",
    scanning_files: "스캔 중… 파일: ",
    cancel: "취소",
    new_scan: "새 스캔",
    trash_question: "휴지통으로 이동할까요?",
    folder: "폴더",
    file: "파일",
    trash_button: "휴지통으로 이동",
    open_in_file_manager: "파일 관리자에서 보기",
    trash_tip: "휴지통으로 이동",
    light_theme: "라이트 테마",
    dark_theme: "다크 테마",
    language: "언어",
    hint_select: "선택",
    hint_back: "뒤로",
    disk_free: "사용 가능 /",
};

static PT_BR: Strings = Strings {
    app_title: "Filegram — mapa do disco",
    path_placeholder: "Caminho do diretório…",
    scan: "Escanear",
    recent_scans: "Escaneamentos recentes",
    disks: "Discos",
    home: "Início",
    downloads: "Downloads",
    desktop: "Área de trabalho",
    documents: "Documentos",
    scanning_files: "Escaneando… arquivos: ",
    cancel: "Cancelar",
    new_scan: "Novo escaneamento",
    trash_question: "Mover para a lixeira?",
    folder: "Pasta",
    file: "Arquivo",
    trash_button: "Mover para a Lixeira",
    open_in_file_manager: "Mostrar no gerenciador de arquivos",
    trash_tip: "Mover para a lixeira",
    light_theme: "Tema claro",
    dark_theme: "Tema escuro",
    language: "Idioma",
    hint_select: "selecionar",
    hint_back: "voltar",
    disk_free: "livres de",
};

static TH: Strings = Strings {
    app_title: "Filegram — แผนที่ดิสก์",
    path_placeholder: "เส้นทางโฟลเดอร์…",
    scan: "สแกน",
    recent_scans: "การสแกนล่าสุด",
    disks: "ดิสก์",
    home: "โฮม",
    downloads: "ดาวน์โหลด",
    desktop: "เดสก์ท็อป",
    documents: "เอกสาร",
    scanning_files: "กำลังสแกน… ไฟล์: ",
    cancel: "ยกเลิก",
    new_scan: "สแกนใหม่",
    trash_question: "ย้ายไปถังขยะ?",
    folder: "โฟลเดอร์",
    file: "ไฟล์",
    trash_button: "ย้ายไปถังขยะ",
    open_in_file_manager: "แสดงในตัวจัดการไฟล์",
    trash_tip: "ย้ายไปถังขยะ",
    light_theme: "ธีมสว่าง",
    dark_theme: "ธีมมืด",
    language: "ภาษา",
    hint_select: "เลือก",
    hint_back: "กลับ",
    disk_free: "ว่างจาก",
};

static TR: Strings = Strings {
    app_title: "Filegram — disk haritası",
    path_placeholder: "Dizin yolu…",
    scan: "Tara",
    recent_scans: "Son taramalar",
    disks: "Diskler",
    home: "Ev",
    downloads: "İndirilenler",
    desktop: "Masaüstü",
    documents: "Belgeler",
    scanning_files: "Taranıyor… dosya: ",
    cancel: "İptal",
    new_scan: "Yeni tarama",
    trash_question: "Çöp kutusuna taşınsın mı?",
    folder: "Klasör",
    file: "Dosya",
    trash_button: "Çöp Kutusuna Taşı",
    open_in_file_manager: "Dosya yöneticisinde göster",
    trash_tip: "Çöp kutusuna taşı",
    light_theme: "Açık tema",
    dark_theme: "Koyu tema",
    language: "Dil",
    hint_select: "seç",
    hint_back: "geri",
    disk_free: "boş /",
};

static FA: Strings = Strings {
    app_title: "Filegram — نقشه دیسک",
    path_placeholder: "مسیر پوشه…",
    scan: "اسکن",
    recent_scans: "اسکن‌های اخیر",
    disks: "دیسک‌ها",
    home: "خانه",
    downloads: "دانلودها",
    desktop: "دسکتاپ",
    documents: "اسناد",
    scanning_files: "در حال اسکن… فایل‌ها: ",
    cancel: "لغو",
    new_scan: "اسکن جدید",
    trash_question: "به سطل زباله منتقل شود؟",
    folder: "پوشه",
    file: "فایل",
    trash_button: "انتقال به سطل زباله",
    open_in_file_manager: "نمایش در مدیر فایل",
    trash_tip: "انتقال به سطل زباله",
    light_theme: "تم روشن",
    dark_theme: "تم تیره",
    language: "زبان",
    hint_select: "انتخاب",
    hint_back: "بازگشت",
    disk_free: "آزاد از",
};

static NL: Strings = Strings {
    app_title: "Filegram — schijfkaart",
    path_placeholder: "Mappad…",
    scan: "Scannen",
    recent_scans: "Recente scans",
    disks: "Schijven",
    home: "Thuismap",
    downloads: "Downloads",
    desktop: "Bureaublad",
    documents: "Documenten",
    scanning_files: "Scannen… bestanden: ",
    cancel: "Annuleren",
    new_scan: "Nieuwe scan",
    trash_question: "Naar de prullenbak verplaatsen?",
    folder: "Map",
    file: "Bestand",
    trash_button: "Naar prullenbak",
    open_in_file_manager: "Tonen in bestandsbeheer",
    trash_tip: "Naar de prullenbak verplaatsen",
    light_theme: "Licht thema",
    dark_theme: "Donker thema",
    language: "Taal",
    hint_select: "selecteren",
    hint_back: "terug",
    disk_free: "vrij van",
};

static PL: Strings = Strings {
    app_title: "Filegram — mapa dysku",
    path_placeholder: "Ścieżka katalogu…",
    scan: "Skanuj",
    recent_scans: "Ostatnie skanowania",
    disks: "Dyski",
    home: "Katalog domowy",
    downloads: "Pobrane",
    desktop: "Pulpit",
    documents: "Dokumenty",
    scanning_files: "Skanowanie… pliki: ",
    cancel: "Anuluj",
    new_scan: "Nowy skan",
    trash_question: "Przenieść do kosza?",
    folder: "Folder",
    file: "Plik",
    trash_button: "Przenieś do kosza",
    open_in_file_manager: "Pokaż w menedżerze plików",
    trash_tip: "Przenieś do kosza",
    light_theme: "Jasny motyw",
    dark_theme: "Ciemny motyw",
    language: "Język",
    hint_select: "wybierz",
    hint_back: "wstecz",
    disk_free: "wolne z",
};

static VI: Strings = Strings {
    app_title: "Filegram — bản đồ ổ đĩa",
    path_placeholder: "Đường dẫn thư mục…",
    scan: "Quét",
    recent_scans: "Quét gần đây",
    disks: "Ổ đĩa",
    home: "Thư mục cá nhân",
    downloads: "Tải về",
    desktop: "Màn hình nền",
    documents: "Tài liệu",
    scanning_files: "Đang quét… tệp: ",
    cancel: "Hủy",
    new_scan: "Quét mới",
    trash_question: "Chuyển vào thùng rác?",
    folder: "Thư mục",
    file: "Tệp",
    trash_button: "Chuyển vào thùng rác",
    open_in_file_manager: "Hiện trong trình quản lý tệp",
    trash_tip: "Chuyển vào thùng rác",
    light_theme: "Giao diện sáng",
    dark_theme: "Giao diện tối",
    language: "Ngôn ngữ",
    hint_select: "chọn",
    hint_back: "quay lại",
    disk_free: "trống trên",
};

static CS: Strings = Strings {
    app_title: "Filegram — mapa disku",
    path_placeholder: "Cesta k adresáři…",
    scan: "Skenovat",
    recent_scans: "Nedávná skenování",
    disks: "Disky",
    home: "Domů",
    downloads: "Stažené",
    desktop: "Plocha",
    documents: "Dokumenty",
    scanning_files: "Skenování… souborů: ",
    cancel: "Zrušit",
    new_scan: "Nový sken",
    trash_question: "Přesunout do koše?",
    folder: "Složka",
    file: "Soubor",
    trash_button: "Přesunout do koše",
    open_in_file_manager: "Zobrazit ve správci souborů",
    trash_tip: "Přesunout do koše",
    light_theme: "Světlý motiv",
    dark_theme: "Tmavý motiv",
    language: "Jazyk",
    hint_select: "vybrat",
    hint_back: "zpět",
    disk_free: "volných z",
};

static EL: Strings = Strings {
    app_title: "Filegram — χάρτης δίσκου",
    path_placeholder: "Διαδρομή καταλόγου…",
    scan: "Σάρωση",
    recent_scans: "Πρόσφατες σαρώσεις",
    disks: "Δίσκοι",
    home: "Προσωπικός φάκελος",
    downloads: "Λήψεις",
    desktop: "Επιφάνεια εργασίας",
    documents: "Έγγραφα",
    scanning_files: "Σάρωση… αρχεία: ",
    cancel: "Άκυρο",
    new_scan: "Νέα σάρωση",
    trash_question: "Μετακίνηση στα απορρίμματα;",
    folder: "Φάκελος",
    file: "Αρχείο",
    trash_button: "Στα απορρίμματα",
    open_in_file_manager: "Εμφάνιση στον διαχειριστή αρχείων",
    trash_tip: "Μετακίνηση στα απορρίμματα",
    light_theme: "Φωτεινό θέμα",
    dark_theme: "Σκούρο θέμα",
    language: "Γλώσσα",
    hint_select: "επιλογή",
    hint_back: "πίσω",
    disk_free: "ελεύθερα από",
};

static SV: Strings = Strings {
    app_title: "Filegram — diskkarta",
    path_placeholder: "Sökväg till katalog…",
    scan: "Skanna",
    recent_scans: "Senaste skanningar",
    disks: "Diskar",
    home: "Hem",
    downloads: "Hämtningar",
    desktop: "Skrivbord",
    documents: "Dokument",
    scanning_files: "Skannar… filer: ",
    cancel: "Avbryt",
    new_scan: "Ny skanning",
    trash_question: "Flytta till papperskorgen?",
    folder: "Mapp",
    file: "Fil",
    trash_button: "Flytta till papperskorgen",
    open_in_file_manager: "Visa i filhanteraren",
    trash_tip: "Flytta till papperskorgen",
    light_theme: "Ljust tema",
    dark_theme: "Mörkt tema",
    language: "Språk",
    hint_select: "välj",
    hint_back: "tillbaka",
    disk_free: "ledigt av",
};

static UK: Strings = Strings {
    app_title: "Filegram — карта диска",
    path_placeholder: "Шлях до теки…",
    scan: "Сканувати",
    recent_scans: "Останні сканування",
    disks: "Диски",
    home: "Домівка",
    downloads: "Завантаження",
    desktop: "Стільниця",
    documents: "Документи",
    scanning_files: "Сканування… файлів: ",
    cancel: "Скасувати",
    new_scan: "Новий скан",
    trash_question: "Перемістити в смітник?",
    folder: "Тека",
    file: "Файл",
    trash_button: "У смітник",
    open_in_file_manager: "Показати у файловому менеджері",
    trash_tip: "Перемістити в смітник",
    light_theme: "Світла тема",
    dark_theme: "Темна тема",
    language: "Мова",
    hint_select: "вибрати",
    hint_back: "назад",
    disk_free: "вільно з",
};

static HU: Strings = Strings {
    app_title: "Filegram — lemeztérkép",
    path_placeholder: "Könyvtár elérési útja…",
    scan: "Vizsgálat",
    recent_scans: "Legutóbbi vizsgálatok",
    disks: "Lemezek",
    home: "Saját mappa",
    downloads: "Letöltések",
    desktop: "Asztal",
    documents: "Dokumentumok",
    scanning_files: "Vizsgálat… fájlok: ",
    cancel: "Mégse",
    new_scan: "Új vizsgálat",
    trash_question: "Áthelyezi a kukába?",
    folder: "Mappa",
    file: "Fájl",
    trash_button: "Áthelyezés a kukába",
    open_in_file_manager: "Megjelenítés a fájlkezelőben",
    trash_tip: "Áthelyezés a kukába",
    light_theme: "Világos téma",
    dark_theme: "Sötét téma",
    language: "Nyelv",
    hint_select: "kijelölés",
    hint_back: "vissza",
    disk_free: "szabad ebből",
};

static RO: Strings = Strings {
    app_title: "Filegram — harta discului",
    path_placeholder: "Calea directorului…",
    scan: "Scanează",
    recent_scans: "Scanări recente",
    disks: "Discuri",
    home: "Acasă",
    downloads: "Descărcări",
    desktop: "Desktop",
    documents: "Documente",
    scanning_files: "Se scanează… fișiere: ",
    cancel: "Anulează",
    new_scan: "Scanare nouă",
    trash_question: "Mutați la coșul de gunoi?",
    folder: "Dosar",
    file: "Fișier",
    trash_button: "Mută la coș",
    open_in_file_manager: "Afișează în managerul de fișiere",
    trash_tip: "Mută la coșul de gunoi",
    light_theme: "Temă luminoasă",
    dark_theme: "Temă întunecată",
    language: "Limbă",
    hint_select: "selectează",
    hint_back: "înapoi",
    disk_free: "liberi din",
};

static DA: Strings = Strings {
    app_title: "Filegram — diskkort",
    path_placeholder: "Sti til mappe…",
    scan: "Scan",
    recent_scans: "Seneste scanninger",
    disks: "Diske",
    home: "Hjem",
    downloads: "Overførsler",
    desktop: "Skrivebord",
    documents: "Dokumenter",
    scanning_files: "Scanner… filer: ",
    cancel: "Annuller",
    new_scan: "Ny scanning",
    trash_question: "Flyt til papirkurven?",
    folder: "Mappe",
    file: "Fil",
    trash_button: "Flyt til papirkurven",
    open_in_file_manager: "Vis i filhåndtering",
    trash_tip: "Flyt til papirkurven",
    light_theme: "Lyst tema",
    dark_theme: "Mørkt tema",
    language: "Sprog",
    hint_select: "vælg",
    hint_back: "tilbage",
    disk_free: "ledig af",
};

static FI: Strings = Strings {
    app_title: "Filegram — levykartta",
    path_placeholder: "Hakemiston polku…",
    scan: "Skannaa",
    recent_scans: "Viimeisimmät skannaukset",
    disks: "Levyt",
    home: "Koti",
    downloads: "Lataukset",
    desktop: "Työpöytä",
    documents: "Asiakirjat",
    scanning_files: "Skannataan… tiedostoja: ",
    cancel: "Peruuta",
    new_scan: "Uusi skannaus",
    trash_question: "Siirretäänkö roskakoriin?",
    folder: "Kansio",
    file: "Tiedosto",
    trash_button: "Siirrä roskakoriin",
    open_in_file_manager: "Näytä tiedostonhallinnassa",
    trash_tip: "Siirrä roskakoriin",
    light_theme: "Vaalea teema",
    dark_theme: "Tumma teema",
    language: "Kieli",
    hint_select: "valitse",
    hint_back: "takaisin",
    disk_free: "vapaana /",
};

static NO: Strings = Strings {
    app_title: "Filegram — diskkart",
    path_placeholder: "Sti til mappe…",
    scan: "Skann",
    recent_scans: "Nylige skanninger",
    disks: "Disker",
    home: "Hjem",
    downloads: "Nedlastinger",
    desktop: "Skrivebord",
    documents: "Dokumenter",
    scanning_files: "Skanner… filer: ",
    cancel: "Avbryt",
    new_scan: "Ny skanning",
    trash_question: "Flytte til papirkurven?",
    folder: "Mappe",
    file: "Fil",
    trash_button: "Flytt til papirkurven",
    open_in_file_manager: "Vis i filbehandler",
    trash_tip: "Flytt til papirkurven",
    light_theme: "Lyst tema",
    dark_theme: "Mørkt tema",
    language: "Språk",
    hint_select: "velg",
    hint_back: "tilbake",
    disk_free: "ledig av",
};

static SK: Strings = Strings {
    app_title: "Filegram — mapa disku",
    path_placeholder: "Cesta k adresáru…",
    scan: "Skenovať",
    recent_scans: "Nedávne skenovania",
    disks: "Disky",
    home: "Domov",
    downloads: "Stiahnuté",
    desktop: "Plocha",
    documents: "Dokumenty",
    scanning_files: "Skenovanie… súborov: ",
    cancel: "Zrušiť",
    new_scan: "Nový sken",
    trash_question: "Presunúť do koša?",
    folder: "Priečinok",
    file: "Súbor",
    trash_button: "Presunúť do koša",
    open_in_file_manager: "Zobraziť v správcovi súborov",
    trash_tip: "Presunúť do koša",
    light_theme: "Svetlý motív",
    dark_theme: "Tmavý motív",
    language: "Jazyk",
    hint_select: "vybrať",
    hint_back: "späť",
    disk_free: "voľných z",
};

static BG: Strings = Strings {
    app_title: "Filegram — карта на диска",
    path_placeholder: "Път до директория…",
    scan: "Сканиране",
    recent_scans: "Последни сканирания",
    disks: "Дискове",
    home: "Домашна папка",
    downloads: "Изтегляния",
    desktop: "Работен плот",
    documents: "Документи",
    scanning_files: "Сканиране… файлове: ",
    cancel: "Отказ",
    new_scan: "Ново сканиране",
    trash_question: "Преместване в кошчето?",
    folder: "Папка",
    file: "Файл",
    trash_button: "В кошчето",
    open_in_file_manager: "Покажи във файловия мениджър",
    trash_tip: "Преместване в кошчето",
    light_theme: "Светла тема",
    dark_theme: "Тъмна тема",
    language: "Език",
    hint_select: "избор",
    hint_back: "назад",
    disk_free: "свободни от",
};

static HR: Strings = Strings {
    app_title: "Filegram — karta diska",
    path_placeholder: "Putanja do mape…",
    scan: "Skeniraj",
    recent_scans: "Nedavna skeniranja",
    disks: "Diskovi",
    home: "Osobna mapa",
    downloads: "Preuzimanja",
    desktop: "Radna površina",
    documents: "Dokumenti",
    scanning_files: "Skeniranje… datoteke: ",
    cancel: "Odustani",
    new_scan: "Novo skeniranje",
    trash_question: "Premjestiti u smeće?",
    folder: "Mapa",
    file: "Datoteka",
    trash_button: "Premjesti u smeće",
    open_in_file_manager: "Prikaži u upravitelju datoteka",
    trash_tip: "Premjesti u smeće",
    light_theme: "Svijetla tema",
    dark_theme: "Tamna tema",
    language: "Jezik",
    hint_select: "odaberi",
    hint_back: "natrag",
    disk_free: "slobodno od",
};

static LT: Strings = Strings {
    app_title: "Filegram — disko žemėlapis",
    path_placeholder: "Katalogo kelias…",
    scan: "Skenuoti",
    recent_scans: "Paskutiniai skenavimai",
    disks: "Diskai",
    home: "Namai",
    downloads: "Atsisiuntimai",
    desktop: "Darbalaukis",
    documents: "Dokumentai",
    scanning_files: "Skenuojama… failai: ",
    cancel: "Atšaukti",
    new_scan: "Naujas skenavimas",
    trash_question: "Perkelti į šiukšlinę?",
    folder: "Aplankas",
    file: "Failas",
    trash_button: "Į šiukšlinę",
    open_in_file_manager: "Rodyti failų tvarkytuvėje",
    trash_tip: "Perkelti į šiukšlinę",
    light_theme: "Šviesi tema",
    dark_theme: "Tamsi tema",
    language: "Kalba",
    hint_select: "pasirinkti",
    hint_back: "atgal",
    disk_free: "laisva iš",
};

static SR: Strings = Strings {
    app_title: "Filegram — мапа диска",
    path_placeholder: "Путања директоријума…",
    scan: "Скенирај",
    recent_scans: "Недавна скенирања",
    disks: "Дискови",
    home: "Лична фасцикла",
    downloads: "Преузимања",
    desktop: "Радна површина",
    documents: "Документи",
    scanning_files: "Скенирање… датотеке: ",
    cancel: "Откажи",
    new_scan: "Ново скенирање",
    trash_question: "Преместити у смеће?",
    folder: "Фасцикла",
    file: "Датотека",
    trash_button: "Премести у смеће",
    open_in_file_manager: "Прикажи у управљачу датотека",
    trash_tip: "Премести у смеће",
    light_theme: "Светла тема",
    dark_theme: "Тамна тема",
    language: "Језик",
    hint_select: "изабери",
    hint_back: "назад",
    disk_free: "слободно од",
};

static LV: Strings = Strings {
    app_title: "Filegram — diska karte",
    path_placeholder: "Direktorijas ceļš…",
    scan: "Skenēt",
    recent_scans: "Nesenās skenēšanas",
    disks: "Diski",
    home: "Mājas",
    downloads: "Lejupielādes",
    desktop: "Darbvirsma",
    documents: "Dokumenti",
    scanning_files: "Skenē… faili: ",
    cancel: "Atcelt",
    new_scan: "Jauna skenēšana",
    trash_question: "Pārvietot uz atkritni?",
    folder: "Mape",
    file: "Fails",
    trash_button: "Uz atkritni",
    open_in_file_manager: "Rādīt failu pārvaldniekā",
    trash_tip: "Pārvietot uz atkritni",
    light_theme: "Gaišs motīvs",
    dark_theme: "Tumšs motīvs",
    language: "Valoda",
    hint_select: "izvēlēties",
    hint_back: "atpakaļ",
    disk_free: "brīvs no",
};

static SL: Strings = Strings {
    app_title: "Filegram — zemljevid diska",
    path_placeholder: "Pot do mape…",
    scan: "Preglej",
    recent_scans: "Nedavni pregledi",
    disks: "Diski",
    home: "Domov",
    downloads: "Prenosi",
    desktop: "Namizje",
    documents: "Dokumenti",
    scanning_files: "Pregledovanje… datoteke: ",
    cancel: "Prekliči",
    new_scan: "Nov pregled",
    trash_question: "Premakniti v smeti?",
    folder: "Mapa",
    file: "Datoteka",
    trash_button: "Premakni v smeti",
    open_in_file_manager: "Pokaži v upravitelju datotek",
    trash_tip: "Premakni v smeti",
    light_theme: "Svetla tema",
    dark_theme: "Temna tema",
    language: "Jezik",
    hint_select: "izberi",
    hint_back: "nazaj",
    disk_free: "prosto od",
};

static ET: Strings = Strings {
    app_title: "Filegram — kettakaart",
    path_placeholder: "Kataloogi tee…",
    scan: "Skanni",
    recent_scans: "Hiljutised skannimised",
    disks: "Kettad",
    home: "Kodu",
    downloads: "Allalaadimised",
    desktop: "Töölaud",
    documents: "Dokumendid",
    scanning_files: "Skannimine… faile: ",
    cancel: "Loobu",
    new_scan: "Uus skannimine",
    trash_question: "Kas viia prügikasti?",
    folder: "Kaust",
    file: "Fail",
    trash_button: "Vii prügikasti",
    open_in_file_manager: "Näita failihalduris",
    trash_tip: "Vii prügikasti",
    light_theme: "Hele teema",
    dark_theme: "Tume teema",
    language: "Keel",
    hint_select: "vali",
    hint_back: "tagasi",
    disk_free: "vaba /",
};

static HE: Strings = Strings {
    app_title: "Filegram — מפת דיסק",
    path_placeholder: "נתיב תיקייה…",
    scan: "סריקה",
    recent_scans: "סריקות אחרונות",
    disks: "דיסקים",
    home: "בית",
    downloads: "הורדות",
    desktop: "שולחן עבודה",
    documents: "מסמכים",
    scanning_files: "סורק… קבצים: ",
    cancel: "ביטול",
    new_scan: "סריקה חדשה",
    trash_question: "להעביר לאשפה?",
    folder: "תיקייה",
    file: "קובץ",
    trash_button: "העבר לאשפה",
    open_in_file_manager: "הצג במנהל הקבצים",
    trash_tip: "העבר לאשפה",
    light_theme: "ערכת נושא בהירה",
    dark_theme: "ערכת נושא כהה",
    language: "שפה",
    hint_select: "בחירה",
    hint_back: "חזרה",
    disk_free: "פנוי מתוך",
};

static MS: Strings = Strings {
    app_title: "Filegram — peta cakera",
    path_placeholder: "Laluan direktori…",
    scan: "Imbas",
    recent_scans: "Imbasan terkini",
    disks: "Cakera",
    home: "Rumah",
    downloads: "Muat turun",
    desktop: "Desktop",
    documents: "Dokumen",
    scanning_files: "Mengimbas… fail: ",
    cancel: "Batal",
    new_scan: "Imbasan baharu",
    trash_question: "Alih ke tong sampah?",
    folder: "Folder",
    file: "Fail",
    trash_button: "Alih ke Tong Sampah",
    open_in_file_manager: "Tunjuk dalam pengurus fail",
    trash_tip: "Alih ke tong sampah",
    light_theme: "Tema cerah",
    dark_theme: "Tema gelap",
    language: "Bahasa",
    hint_select: "pilih",
    hint_back: "kembali",
    disk_free: "bebas daripada",
};

static FIL: Strings = Strings {
    app_title: "Filegram — mapa ng disk",
    path_placeholder: "Path ng direktoryo…",
    scan: "I-scan",
    recent_scans: "Mga kamakailang scan",
    disks: "Mga disk",
    home: "Home",
    downloads: "Mga download",
    desktop: "Desktop",
    documents: "Mga dokumento",
    scanning_files: "Nag-i-scan… mga file: ",
    cancel: "Kanselahin",
    new_scan: "Bagong scan",
    trash_question: "Ilipat sa basurahan?",
    folder: "Folder",
    file: "File",
    trash_button: "Ilipat sa Basurahan",
    open_in_file_manager: "Ipakita sa file manager",
    trash_tip: "Ilipat sa basurahan",
    light_theme: "Maliwanag na tema",
    dark_theme: "Madilim na tema",
    language: "Wika",
    hint_select: "pumili",
    hint_back: "bumalik",
    disk_free: "libre sa",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_roundtrip_for_every_language() {
        for lang in Lang::ALL {
            assert_eq!(Lang::from_tag(lang.tag()), Some(lang));
        }
    }

    #[test]
    fn tags_are_unique() {
        let mut tags: Vec<&str> = Lang::ALL.iter().map(|lang| lang.tag()).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), Lang::ALL.len());
    }

    #[test]
    fn primary_languages_are_a_subset_of_all() {
        for lang in Lang::PRIMARY {
            assert!(Lang::ALL.contains(&lang), "{lang:?}");
        }
    }

    #[test]
    fn extended_locales_map() {
        assert_eq!(Lang::from_locale("uk-UA"), Lang::Uk);
        assert_eq!(Lang::from_locale("vi-VN"), Lang::Vi);
        assert_eq!(Lang::from_locale("fil-PH"), Lang::Fil);
        // Norwegian variants and legacy codes collapse onto one entry.
        for norwegian in ["no", "nb-NO", "nn-NO"] {
            assert_eq!(Lang::from_locale(norwegian), Lang::No, "{norwegian}");
        }
        assert_eq!(Lang::from_locale("iw-IL"), Lang::He);
        assert_eq!(Lang::from_locale("tl-PH"), Lang::Fil);
    }

    #[test]
    fn unknown_tag_reads_as_none() {
        // An edited settings file must fall back to the system locale.
        assert_eq!(Lang::from_tag("xx"), None);
        assert_eq!(Lang::from_tag(""), None);
        // The persistence match is exact: a loose system tag is not enough.
        assert_eq!(Lang::from_tag("en"), None);
    }

    #[test]
    fn locale_maps_language_subtag() {
        assert_eq!(Lang::from_locale("en-US"), Lang::EnUs);
        assert_eq!(Lang::from_locale("en-GB"), Lang::EnUs);
        assert_eq!(Lang::from_locale("RU-ru"), Lang::RuRu);
        assert_eq!(Lang::from_locale("zh-Hans-CN"), Lang::ZhCn);
        assert_eq!(Lang::from_locale("ja"), Lang::JaJp);
    }

    #[test]
    fn locale_splits_spanish_by_region() {
        assert_eq!(Lang::from_locale("es-ES"), Lang::EsEs);
        assert_eq!(Lang::from_locale("es"), Lang::EsEs);
        for latam in ["es-419", "es-MX", "es-AR", "es-US"] {
            assert_eq!(Lang::from_locale(latam), Lang::Es419, "{latam}");
        }
    }

    #[test]
    fn locale_splits_portuguese_by_region() {
        assert_eq!(Lang::from_locale("pt-BR"), Lang::PtBr);
        assert_eq!(Lang::from_locale("pt-PT"), Lang::PtPt);
        assert_eq!(Lang::from_locale("pt"), Lang::PtPt);
    }

    #[test]
    fn unix_locale_with_encoding_suffix() {
        assert_eq!(Lang::from_locale("de_DE.UTF-8"), Lang::DeDe);
        assert_eq!(Lang::from_locale("es_MX.UTF-8"), Lang::Es419);
        assert_eq!(Lang::from_locale("th_TH.UTF-8@calendar=buddhist"), Lang::Th);
    }

    #[test]
    fn unknown_locale_falls_back_to_english() {
        assert_eq!(Lang::from_locale("tlh-KL"), Lang::EnUs);
        assert_eq!(Lang::from_locale(""), Lang::EnUs);
    }

    #[test]
    fn legacy_indonesian_code_maps() {
        assert_eq!(Lang::from_locale("in-ID"), Lang::Id);
        assert_eq!(Lang::from_locale("id-ID"), Lang::Id);
    }
}
