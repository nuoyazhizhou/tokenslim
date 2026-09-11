use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;

const EN_MESSAGES: &str = include_str!("../../resources/messages.en.json");
const ZH_CN_MESSAGES: &str = include_str!("../../resources/messages.zh-CN.json");
const AR_MESSAGES: &str = include_str!("../../resources/messages.ar.json");
const DE_MESSAGES: &str = include_str!("../../resources/messages.de.json");
const ES_MESSAGES: &str = include_str!("../../resources/messages.es.json");
const FR_MESSAGES: &str = include_str!("../../resources/messages.fr.json");
const JA_MESSAGES: &str = include_str!("../../resources/messages.ja.json");
const KO_MESSAGES: &str = include_str!("../../resources/messages.ko.json");
const RU_MESSAGES: &str = include_str!("../../resources/messages.ru.json");
const ZH_TW_MESSAGES: &str = include_str!("../../resources/messages.zh-TW.json");

static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
static ZH_CN: OnceLock<HashMap<String, String>> = OnceLock::new();
static AR: OnceLock<HashMap<String, String>> = OnceLock::new();
static DE: OnceLock<HashMap<String, String>> = OnceLock::new();
static ES: OnceLock<HashMap<String, String>> = OnceLock::new();
static FR: OnceLock<HashMap<String, String>> = OnceLock::new();
static JA: OnceLock<HashMap<String, String>> = OnceLock::new();
static KO: OnceLock<HashMap<String, String>> = OnceLock::new();
static RU: OnceLock<HashMap<String, String>> = OnceLock::new();
static ZH_TW: OnceLock<HashMap<String, String>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiLang {
    EnUs,
    ZhCn,
    Ar,
    De,
    Es,
    Fr,
    Ja,
    Ko,
    Ru,
    ZhTw,
}

/// 探测当前 UI 语言：依次读取 LOCALE/LANGUAGE/LANG 等环境变量，回退到系统区域设置，再按前缀映射到 `UiLang`（默认英文）。
pub fn detect_ui_lang() -> UiLang {
    let mut raw = [
        "AI_CONTROL_PLANE_LOCALE",
        "LANGUAGE",
        "LC_ALL",
        "LC_MESSAGES",
        "LANG",
    ]
    .iter()
    .find_map(|key| std::env::var(key).ok())
    .unwrap_or_default()
    .to_ascii_lowercase()
    .replace('_', "-");

    if raw.is_empty() {
        if let Some(locale) = sys_locale::get_locale() {
            raw = locale.to_ascii_lowercase().replace('_', "-");
        }
    }

    if raw.starts_with("zh-tw")
        || raw.starts_with("zh-hant")
        || raw.starts_with("zh-hk")
        || raw.starts_with("zh-mo")
    {
        UiLang::ZhTw
    } else if raw.starts_with("zh") {
        UiLang::ZhCn
    } else if raw.starts_with("ar") {
        UiLang::Ar
    } else if raw.starts_with("de") {
        UiLang::De
    } else if raw.starts_with("es") {
        UiLang::Es
    } else if raw.starts_with("fr") {
        UiLang::Fr
    } else if raw.starts_with("ja") {
        UiLang::Ja
    } else if raw.starts_with("ko") {
        UiLang::Ko
    } else if raw.starts_with("ru") {
        UiLang::Ru
    } else {
        UiLang::EnUs
    }
}

/// P3-184（P3-127 同族）：UI 语言探测缓存。`detect_ui_lang` 每次调用读 5 个环境变量
/// 并可能触发 `sys_locale` 系统调用；`t()` 在高频循环日志场景每次调用重复探测。
/// 语言在进程内不变，用 `OnceLock` 首次探测后缓存，消除重复系统调用。
static UI_LANG_CACHE: OnceLock<UiLang> = OnceLock::new();
fn ui_lang_cached() -> UiLang {
    *UI_LANG_CACHE.get_or_init(detect_ui_lang)
}

/// 将 JSON 文本解析为「键→消息」映射，解析失败则返回空映射。
fn load_messages(text: &str) -> HashMap<String, String> {
    serde_json::from_str::<HashMap<String, String>>(text).unwrap_or_default()
}

/// 按指定语言从对应静态字典（首次访问时懒加载）查键，命中返回消息字符串，否则返回 `None`。
fn t_for_lang(lang: UiLang, key: &str) -> Option<&'static str> {
    match lang {
        UiLang::ZhCn => ZH_CN
            .get_or_init(|| load_messages(ZH_CN_MESSAGES))
            .get(key)
            .map(|s| s.as_str()),
        UiLang::EnUs => EN
            .get_or_init(|| load_messages(EN_MESSAGES))
            .get(key)
            .map(|s| s.as_str()),
        UiLang::Ar => AR
            .get_or_init(|| load_messages(AR_MESSAGES))
            .get(key)
            .map(|s| s.as_str()),
        UiLang::De => DE
            .get_or_init(|| load_messages(DE_MESSAGES))
            .get(key)
            .map(|s| s.as_str()),
        UiLang::Es => ES
            .get_or_init(|| load_messages(ES_MESSAGES))
            .get(key)
            .map(|s| s.as_str()),
        UiLang::Fr => FR
            .get_or_init(|| load_messages(FR_MESSAGES))
            .get(key)
            .map(|s| s.as_str()),
        UiLang::Ja => JA
            .get_or_init(|| load_messages(JA_MESSAGES))
            .get(key)
            .map(|s| s.as_str()),
        UiLang::Ko => KO
            .get_or_init(|| load_messages(KO_MESSAGES))
            .get(key)
            .map(|s| s.as_str()),
        UiLang::Ru => RU
            .get_or_init(|| load_messages(RU_MESSAGES))
            .get(key)
            .map(|s| s.as_str()),
        UiLang::ZhTw => ZH_TW
            .get_or_init(|| load_messages(ZH_TW_MESSAGES))
            .get(key)
            .map(|s| s.as_str()),
    }
}

/// 按运行时探测到的语言查询消息键，未命中返回 `None`。
pub fn t_dynamic(key: &str) -> Option<&'static str> {
    let lang = ui_lang_cached();
    t_for_lang(lang, key)
}

/// 按当前语言查询消息键；未命中时回退英文，仍无则原样返回键名。
pub fn t(key: &'static str) -> &'static str {
    let lang = ui_lang_cached();
    if let Some(val) = t_for_lang(lang, key) {
        return val;
    }
    if let Some(val) = t_for_lang(UiLang::EnUs, key) {
        return val;
    }
    key
}

/// 强制以英文查询消息键，未命中时回退键名。
pub fn t_en(key: &'static str) -> &'static str {
    t_for_lang(UiLang::EnUs, key).unwrap_or(key)
}

/// 强制以简体中文查询消息键，未命中时回退键名。
pub fn t_zh(key: &'static str) -> &'static str {
    t_for_lang(UiLang::ZhCn, key).unwrap_or(key)
}

/// 查询消息键并将首个 `{}` 占位符替换为参数，返回格式化字符串。
pub fn t1(key: &'static str, arg1: impl Display) -> String {
    t(key).replacen("{}", &arg1.to_string(), 1)
}

/// 查询消息键并依次替换前两个 `{}` 占位符为对应参数。
pub fn t2(key: &'static str, arg1: impl Display, arg2: impl Display) -> String {
    t(key)
        .replacen("{}", &arg1.to_string(), 1)
        .replacen("{}", &arg2.to_string(), 1)
}

/// 查询消息键并依次替换前三个 `{}` 占位符为对应参数。
pub fn t3(key: &'static str, arg1: impl Display, arg2: impl Display, arg3: impl Display) -> String {
    t(key)
        .replacen("{}", &arg1.to_string(), 1)
        .replacen("{}", &arg2.to_string(), 1)
        .replacen("{}", &arg3.to_string(), 1)
}

#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::utils::i18n::t($key).to_string()
    };
    ($key:expr, $arg1:expr) => {
        $crate::utils::i18n::t($key).replace("{}", &format!("{}", $arg1))
    };
    ($key:expr, $arg1:expr, $arg2:expr) => {
        $crate::utils::i18n::t($key)
            .replacen("{}", &format!("{}", $arg1), 1)
            .replacen("{}", &format!("{}", $arg2), 1)
    };
}

#[derive(Debug, Clone)]
pub struct UserFacingMessage {
    pub code: &'static str,
    pub message_zh: String,
    pub message_en: String,
    pub hint_zh: Option<String>,
    pub hint_en: Option<String>,
}

impl UserFacingMessage {
    /// 将面向用户的消息按指定语言渲染为终端文本（含提示）：中文语境附 EN、英文语境附 ZH。
    pub fn render_terminal(&self, lang: UiLang) -> String {
        match lang {
            UiLang::ZhCn | UiLang::ZhTw => {
                let mut out = String::new();
                out.push_str(&format!(
                    "[{}] {}
",
                    self.code, self.message_zh
                ));
                if let Some(h) = self.hint_zh.as_deref() {
                    out.push_str(&format!(
                        "建议: {}
",
                        h
                    ));
                }
                out.push_str(&format!(
                    "EN: {}
",
                    self.message_en
                ));
                if let Some(h) = self.hint_en.as_deref() {
                    out.push_str(&format!(
                        "Hint: {}
",
                        h
                    ));
                }
                out.trim_end().to_string()
            }
            _ => {
                let mut out = String::new();
                out.push_str(&format!(
                    "[{}] {}
",
                    self.code, self.message_en
                ));
                if let Some(h) = self.hint_en.as_deref() {
                    out.push_str(&format!(
                        "Hint: {}
",
                        h
                    ));
                }
                out.push_str(&format!(
                    "ZH: {}
",
                    self.message_zh
                ));
                if let Some(h) = self.hint_zh.as_deref() {
                    out.push_str(&format!(
                        "建议: {}
",
                        h
                    ));
                }
                out.trim_end().to_string()
            }
        }
    }
}

/// 按运行时探测到的语言渲染面向用户消息为终端文本。
pub fn render_user_facing_terminal_message(msg: UserFacingMessage) -> String {
    msg.render_terminal(ui_lang_cached())
}
