//! Shared i18n catalog for chaos user-visible strings.
//!
//! Catalogs live at monorepo `locales/{en,zh-CN}.json` and are embedded at compile time.

#[cfg(not(unix))]
compile_error!(
    "chaos-i18n is Unix-only: chaos is a Linux dae control plane. Windows support \
     was removed in 0.1.27."
);

use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;

const EN_JSON: &str = include_str!("../../../locales/en.json");
const ZH_CN_JSON: &str = include_str!("../../../locales/zh-CN.json");

static EN: Lazy<HashMap<String, String>> = Lazy::new(|| load_flat(EN_JSON));
static ZH_CN: Lazy<HashMap<String, String>> = Lazy::new(|| load_flat(ZH_CN_JSON));

/// Supported UI / API locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Locale {
    #[default]
    En,
    ZhCn,
}

impl Locale {
    pub fn as_str(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::ZhCn => "zh-CN",
        }
    }

    /// Normalize BCP-47-ish tags used by browsers / Accept-Language.
    pub fn parse(tag: &str) -> Option<Self> {
        let primary = tag
            .split(',')
            .next()
            .unwrap_or(tag)
            .split(';')
            .next()
            .unwrap_or(tag)
            .trim()
            .to_ascii_lowercase()
            .replace('_', "-");

        if primary.is_empty() {
            return None;
        }

        if primary == "zh"
            || primary.starts_with("zh-hans")
            || primary.starts_with("zh-cn")
            || primary.starts_with("zh-sg")
        {
            return Some(Locale::ZhCn);
        }
        // Other zh variants (TW/HK) fall back to en until we ship them.
        if primary == "en" || primary.starts_with("en-") {
            return Some(Locale::En);
        }
        None
    }

    /// First recognized language from an `Accept-Language` header value.
    pub fn from_accept_language(header: Option<&str>) -> Self {
        let Some(raw) = header else {
            return Locale::En;
        };
        for part in raw.split(',') {
            let tag = part.split(';').next().unwrap_or(part).trim();
            if let Some(loc) = Self::parse(tag) {
                return loc;
            }
        }
        Locale::En
    }
}

fn load_flat(json: &str) -> HashMap<String, String> {
    let value: Value = serde_json::from_str(json).expect("locale JSON must parse");
    let mut out = HashMap::new();
    flatten("", &value, &mut out);
    out
}

fn flatten(prefix: &str, value: &Value, out: &mut HashMap<String, String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&key, v, out);
            }
        }
        Value::String(s) => {
            out.insert(prefix.to_string(), s.clone());
        }
        _ => {}
    }
}

fn catalog(locale: Locale) -> &'static HashMap<String, String> {
    match locale {
        Locale::En => &EN,
        Locale::ZhCn => &ZH_CN,
    }
}

/// Look up `key` for `locale`, falling back to English then the key itself.
pub fn t(locale: Locale, key: &str) -> String {
    if let Some(s) = catalog(locale).get(key) {
        return s.clone();
    }
    if locale != Locale::En {
        if let Some(s) = EN.get(key) {
            return s.clone();
        }
    }
    key.to_string()
}

/// `t` with simple `{name}` placeholders.
pub fn t_params(locale: Locale, key: &str, params: &[(&str, &str)]) -> String {
    let mut s = t(locale, key);
    for (name, value) in params {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    s
}

/// Localized message for a stable API `error.code`.
pub fn error_message(locale: Locale, code: &str) -> String {
    let key = format!("error.{code}");
    let msg = t(locale, &key);
    if msg == key {
        t(locale, "error.unknown")
    } else {
        msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_zh_variants() {
        assert_eq!(Locale::parse("zh-CN"), Some(Locale::ZhCn));
        assert_eq!(Locale::parse("zh"), Some(Locale::ZhCn));
        assert_eq!(Locale::parse("zh-Hans-CN"), Some(Locale::ZhCn));
        assert_eq!(Locale::parse("en-US"), Some(Locale::En));
        assert_eq!(Locale::parse("fr"), None);
    }

    #[test]
    fn accept_language_picks_first_known() {
        assert_eq!(
            Locale::from_accept_language(Some("fr-FR,zh-CN;q=0.9,en;q=0.8")),
            Locale::ZhCn
        );
        assert_eq!(Locale::from_accept_language(Some("de")), Locale::En);
        assert_eq!(Locale::from_accept_language(None), Locale::En);
    }

    #[test]
    fn english_dashboard_title() {
        assert_eq!(t(Locale::En, "dashboard.title"), "Dashboard");
    }

    #[test]
    fn chinese_error_unauthorized() {
        assert_eq!(error_message(Locale::ZhCn, "unauthorized"), "未认证");
    }

    #[test]
    fn missing_key_falls_back_to_en_then_key() {
        assert_eq!(t(Locale::ZhCn, "dashboard.title"), "仪表盘");
        // key only in neither → raw key
        assert_eq!(t(Locale::En, "no.such.key"), "no.such.key");
    }

    #[test]
    fn interpolation() {
        let s = t_params(
            Locale::En,
            "dashboard.latencyFinished",
            &[("alive", "3"), ("total", "5")],
        );
        assert_eq!(s, "Latency test complete: 3/5 nodes available");
    }

    #[test]
    fn catalogs_share_error_keys() {
        for key in EN.keys() {
            if key.starts_with("error.") {
                assert!(ZH_CN.contains_key(key), "zh-CN missing {key}");
            }
        }
    }
}
