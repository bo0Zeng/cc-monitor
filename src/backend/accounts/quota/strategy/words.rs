//! 字（E13）：把策略回的 `why` 与 `describe` 片段写成字。键以 `@d_` 开头 ⇒ 引擎的描述片段（`beRotation.desc.*`）；
//! 以 `@` 开头 ⇒ 共用小函数的字（`beRotation.lib.*`）；其余照那份策略的 `texts()`。参数：`at` · `*_at` 是时刻（按请求的时区写）·
//! `acct` 是号（换号名）· `then` 是接着的一句（递归写）· `w` 是语义位 · 别的数取整写出。

use super::{Lang, Strategy, Texts};
use crate::accounts::quota::name_words::{account_text, slot_text};
use crate::common::time::Tz;
use copy_core::copy_text;
use serde_json::{Map, Value};

/// 引擎认得的 `why` 键（`lib` 回的）。
const LIB_KEYS: [&str; 8] = [
    "@refused", "@full", "@overage", "@login", "@line", "@off", "@pending", "@going",
];

/// 引擎认得的描述片段键（`lib::describe_common` 回的）。
const DESC_KEYS: [&str; 10] = [
    "@d_n",
    "@d_line",
    "@d_full",
    "@d_acctLine",
    "@d_acctFull",
    "@d_acctSlot",
    "@d_acctOff",
    "@d_delay",
    "@d_stop",
    "@d_go",
];

pub(crate) fn lib_key_known(k: &str) -> bool {
    LIB_KEYS.contains(&k)
}

fn desc_key_known(k: &str) -> bool {
    DESC_KEYS.contains(&k)
}

/// `describe()` 的回值能不能写成字：片段数组，每片的键在 `texts` 里或是引擎认得的 `@d_` 键。
pub(crate) fn check_fragments(v: &Value, texts: &Texts) -> Result<(), String> {
    let arr = v.as_array().ok_or("describe() must return an array")?;
    for (i, f) in arr.iter().enumerate() {
        let k = match f {
            Value::String(k) => k.as_str(),
            Value::Object(m) => m
                .get("k")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("describe()[{i}] needs `k`"))?,
            _ => return Err(format!("describe()[{i}] must be a key or #{{k, …}}")),
        };
        let ok = if k.starts_with("@d_") {
            desc_key_known(k)
        } else {
            !k.starts_with('@') && texts.has(k)
        };
        if !ok {
            return Err(format!("describe()[{i}]: `{k}` has no words"));
        }
    }
    Ok(())
}

/// 写字要的：哪份策略（它的 `texts`）· 语言 · 此刻 · 看的那一台的时区。
pub(crate) struct Speak<'a> {
    pub(crate) texts: &'a Texts,
    pub(crate) lang: Lang,
    pub(crate) now: u64,
    pub(crate) tz: &'a Tz,
}

impl<'a> Speak<'a> {
    pub(crate) fn of(s: &'a Strategy, lang: Lang, now: u64, tz: &'a Tz) -> Self {
        Self {
            texts: &s.texts,
            lang,
            now,
            tz,
        }
    }

    /// 一个参数写成字。
    fn arg(&self, k: &str, v: &Value) -> String {
        match v {
            Value::Null => String::new(),
            Value::String(s) if k == "acct" && s.starts_with('@') => {
                crate::accounts::quota::rotation::doc::row_text(s)
            }
            Value::String(s) if k == "acct" => account_text(s),
            Value::String(s) if k == "w" => slot_text(s),
            Value::String(s) => s.clone(),
            Value::Number(n) if k == "at" || k.ends_with("_at") => {
                let t = n.as_i64().unwrap_or(0);
                crate::common::time::fmt_at(t, i64::try_from(self.now).unwrap_or(i64::MAX), self.tz)
            }
            Value::Number(n) => n
                .as_i64()
                .map(|i| i.to_string())
                .unwrap_or_else(|| format!("{}", n.as_f64().unwrap_or(0.0).round() as i64)),
            Value::Bool(b) => b.to_string(),
            Value::Object(m) if k == "then" => self.why(&Value::Object(m.clone())),
            // 分窗口的线 `{"5h": a, "7d": b}` ⇒ `a/b`（缺的那一位写 —）。
            Value::Object(m) => ["5h", "7d"]
                .iter()
                .map(|w| {
                    m.get(*w)
                        .map_or_else(|| "—".to_string(), |x| self.arg("n", x))
                })
                .collect::<Vec<_>>()
                .join("/"),
            Value::Array(_) => String::new(),
        }
    }

    /// 一个 `{k, …}` 的参数表（`k` 之外的每格）写成字。
    fn args(&self, m: &Map<String, Value>) -> Vec<(String, String)> {
        m.iter()
            .filter(|(k, _)| *k != "k")
            .map(|(k, v)| {
                let s = if k == "then" {
                    self.why(v)
                } else {
                    self.arg(k, v)
                };
                (k.clone(), s)
            })
            .collect()
    }

    /// 策略自己的字：照 `texts[lang][k]` 填 `{参数}`。
    fn own(&self, k: &str, args: &[(String, String)]) -> String {
        let mut s = self.texts.get(self.lang, k).unwrap_or(k).to_string();
        for (n, v) in args {
            s = s.replace(&format!("{{{n}}}"), v);
        }
        s
    }

    /// ★ 一句 `why` 写成字（`()` ⇒ 空串）。
    pub(crate) fn why(&self, why: &Value) -> String {
        let (k, m) = match why {
            Value::String(k) => (k.as_str(), Map::new()),
            Value::Object(m) => match m.get("k").and_then(Value::as_str) {
                Some(k) => (k, m.clone()),
                None => return String::new(),
            },
            _ => return String::new(),
        };
        let args = self.args(&m);
        if k.starts_with("@d_") {
            desc_text(k, &args)
        } else if k.starts_with('@') {
            lib_text(k, &args)
        } else {
            self.own(k, &args)
        }
    }

    /// ★ 描述：`describe()` 的片段写成字，用 ` · ` 连。
    pub(crate) fn describe(&self, frags: &Value) -> String {
        frags
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|f| self.why(f))
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(&copy_text("beRotation.desc.sep", &[]))
            })
            .unwrap_or_default()
    }
}

fn get<'x>(args: &'x [(String, String)], k: &str) -> &'x str {
    args.iter()
        .find(|(n, _)| n == k)
        .map_or("", |(_, v)| v.as_str())
}

/// 共用小函数的字（`beRotation.lib.*`）；时刻说不出的那几句另有不带时刻的一句。
fn lib_text(k: &str, a: &[(String, String)]) -> String {
    let at = get(a, "at");
    let has_at = !at.is_empty();
    match k {
        "@refused" if has_at => copy_text("beRotation.lib.refused", &[("at", at)]),
        "@refused" => copy_text("beRotation.lib.refusedOpen", &[]),
        "@full" if has_at => copy_text("beRotation.lib.full", &[("at", at)]),
        "@full" => copy_text("beRotation.lib.fullOpen", &[]),
        "@overage" => copy_text("beRotation.lib.overage", &[]),
        "@login" => copy_text("beRotation.lib.login", &[]),
        "@line" if has_at => copy_text(
            "beRotation.lib.line",
            &[("w", get(a, "w")), ("n", get(a, "n")), ("at", at)],
        ),
        "@line" => copy_text(
            "beRotation.lib.lineOpen",
            &[("w", get(a, "w")), ("n", get(a, "n"))],
        ),
        "@off" => copy_text("beRotation.lib.off", &[("at", at)]),
        "@pending" => copy_text(
            "beRotation.lib.pending",
            &[("at", at), ("n", get(a, "n")), ("then", get(a, "then"))],
        ),
        "@going" => copy_text("beRotation.lib.going", &[]),
        _ => String::new(),
    }
}

/// 引擎的描述片段（`beRotation.desc.*`）。
fn desc_text(k: &str, a: &[(String, String)]) -> String {
    let (n, acct, at) = (get(a, "n"), get(a, "acct"), get(a, "at"));
    match k {
        "@d_n" => copy_text("beRotation.desc.n", &[("n", n)]),
        "@d_line" => copy_text("beRotation.desc.line", &[("n", n)]),
        "@d_full" => copy_text("beRotation.desc.full", &[]),
        "@d_acctLine" => copy_text("beRotation.desc.acctLine", &[("acct", acct), ("n", n)]),
        "@d_acctFull" => copy_text("beRotation.desc.acctFull", &[("acct", acct)]),
        "@d_acctSlot" => copy_text(
            "beRotation.desc.acctSlot",
            &[("acct", acct), ("at", at), ("n", n)],
        ),
        "@d_acctOff" => copy_text("beRotation.desc.acctOff", &[("acct", acct), ("at", at)]),
        "@d_delay" => copy_text("beRotation.desc.delay", &[("n", n)]),
        "@d_stop" => copy_text("beRotation.desc.stop", &[]),
        "@d_go" => copy_text("beRotation.desc.go", &[]),
        _ => String::new(),
    }
}

/// 策略出错兜底时描述后面接的那一句。
pub(crate) fn broken_text() -> String {
    copy_text("beRotation.desc.broken", &[])
}
