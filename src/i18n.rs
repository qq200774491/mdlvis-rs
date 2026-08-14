use std::borrow::Cow;
use std::collections::HashMap;
#[cfg(test)]
use std::collections::HashSet;
use std::sync::RwLock;

use fluent_templates::fluent_bundle::FluentValue;
use fluent_templates::static_loader;
use unic_langid::{LanguageIdentifier, langid};

type FluentMap = HashMap<Cow<'static, str>, FluentValue<'static>>;

pub const DEFAULT_LOCALE: &str = "zh-CN";
pub const FALLBACK_LANG: LanguageIdentifier = langid!("zh-CN");
#[cfg(test)]
const SUPPORTED: [&str; 2] = ["zh-CN", "en-US"];

static_loader! {
    static LOCALES = {
        locales: "./locales",
        fallback_language: "zh-CN",
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

static CURRENT: RwLock<LanguageIdentifier> = RwLock::new(FALLBACK_LANG);

pub fn normalize_locale(tag: &str) -> &'static str {
    let compact: String = tag
        .trim()
        .chars()
        .filter(|c| *c != '_' && *c != '-')
        .flat_map(|c| c.to_lowercase())
        .collect();
    match compact.as_str() {
        "en" | "enus" => "en-US",
        "zh" | "zhcn" | "zhhans" => "zh-CN",
        _ => DEFAULT_LOCALE,
    }
}

pub fn parse_locale(tag: &str) -> LanguageIdentifier {
    normalize_locale(tag)
        .parse()
        .unwrap_or_else(|_| FALLBACK_LANG.clone())
}

pub fn set_locale(tag: &str) {
    let lang = parse_locale(tag);
    if let Ok(mut current) = CURRENT.write() {
        *current = lang;
    }
}

pub fn current_locale() -> LanguageIdentifier {
    CURRENT
        .read()
        .map(|lang| lang.clone())
        .unwrap_or_else(|_| FALLBACK_LANG.clone())
}

#[cfg(test)]
fn current_locale_tag() -> String {
    current_locale().to_string()
}

pub fn t(id: &str) -> String {
    lookup(id, &fluent_id(id), None)
}

pub fn t_args<'a>(id: &str, args: impl IntoIterator<Item = (&'a str, FluentValue<'a>)>) -> String {
    let mut map = FluentMap::new();
    for (key, value) in args {
        map.insert(Cow::Owned(key.to_string()), owned_fluent_value(value));
    }
    lookup(id, &fluent_id(id), Some(&map))
}

fn fluent_id(id: &str) -> String {
    id.replace('.', "-")
}

fn owned_fluent_value(value: FluentValue<'_>) -> FluentValue<'static> {
    match value {
        FluentValue::String(text) => FluentValue::String(Cow::Owned(text.into_owned())),
        FluentValue::Number(number) => FluentValue::Number(number),
        _ => FluentValue::None,
    }
}

fn lookup(original_id: &str, fluent_id: &str, args: Option<&FluentMap>) -> String {
    let current = current_locale();
    if let Some(value) = try_lookup(&current, fluent_id, args) {
        return value;
    }
    if current != FALLBACK_LANG {
        if let Some(value) = try_lookup(&FALLBACK_LANG, fluent_id, args) {
            return value;
        }
    }
    original_id.to_string()
}

fn try_lookup(lang: &LanguageIdentifier, id: &str, args: Option<&FluentMap>) -> Option<String> {
    let value = LOCALES.lookup_single_language(lang, id, args).ok()?;
    if value.is_empty() || value == id {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
fn collect_ftl_ids(lang: &str, ids: &mut HashSet<String>) {
    let source = match lang {
        "zh-CN" => include_str!("../locales/zh-CN/app.ftl"),
        "en-US" => include_str!("../locales/en-US/app.ftl"),
        _ => return,
    };
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('.') {
            continue;
        }
        if let Some((id, rest)) = line.split_once('=') {
            if rest.trim_start().starts_with('|') || id.chars().any(|c| c.is_whitespace()) {
                continue;
            }
            ids.insert(id.trim().replace('.', "-"));
        }
    }
}

#[cfg(test)]
fn locale_message_ids(lang: &str) -> HashSet<String> {
    let mut ids = HashSet::new();
    collect_ftl_ids(lang, &mut ids);
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn with_locale<T>(tag: &str, f: impl FnOnce() -> T) -> T {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        set_locale(tag);
        let result = f();
        set_locale(DEFAULT_LOCALE);
        result
    }

    #[test]
    fn catalogs_have_the_same_ids() {
        assert_eq!(SUPPORTED, ["zh-CN", "en-US"]);
        let zh = locale_message_ids(SUPPORTED[0]);
        let en = locale_message_ids(SUPPORTED[1]);
        let missing_en: Vec<_> = zh.difference(&en).collect();
        let missing_zh: Vec<_> = en.difference(&zh).collect();
        assert!(
            missing_en.is_empty() && missing_zh.is_empty(),
            "id mismatch\nmissing in en-US: {missing_en:?}\nmissing in zh-CN: {missing_zh:?}"
        );
    }

    #[test]
    fn default_locale_is_zh_cn() {
        with_locale("not-a-locale", || {
            assert_eq!(current_locale_tag(), "zh-CN");
            assert_eq!(t("menu.open-model"), "打开模型");
        });
    }

    #[test]
    fn missing_key_returns_id() {
        with_locale("zh-CN", || {
            assert_eq!(t("this.key.does-not-exist"), "this.key.does-not-exist");
        });
    }

    #[test]
    fn switches_to_en_us() {
        with_locale("en-US", || {
            assert_eq!(t("menu.open-model"), "Open Model");
            assert_eq!(
                t_args("info.name", [("name", "Arthas".into())]),
                "Name: Arthas"
            );
        });
    }
}
