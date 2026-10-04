//! 登录令牌（OAuth 一类）在那一家凭据文件里的读写 ＋ 续期那一发的请求体。
//!
//! 字段名都由调用方给（哪一家的格式住那一家的适配层）；本模块只管「秘密怎么进出类型」：
//! 读出来就包成 [`SecretKey`]；写回经 [`crate::store::merge_secret`]（落盘出口）；
//! 续期请求体经 [`SecretKey::expose_for_token_request`]（第三个出口，只在 [`refresh_body`]）。

use crate::store::merge_secret;
use crate::SecretKey;
use serde_json::{Map, Value};

/// `doc[section][field]` 那一格字符串，包成秘密。没有、不是串、空串 ⇒ `None`。
pub fn secret_in(doc: &Map<String, Value>, section: &str, field: &str) -> Option<SecretKey> {
    doc.get(section)?
        .get(field)?
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(SecretKey::new)
}

/// 续期那一发的请求体：一个 JSON 对象，`plain` 里是明文格，`secret` 那一格是刷新令牌。
pub fn refresh_body(plain: &[(&str, &str)], secret: (&str, &SecretKey)) -> String {
    let mut m = Map::new();
    for (k, v) in plain {
        m.insert((*k).to_string(), Value::String((*v).to_string()));
    }
    m.insert(
        secret.0.to_string(),
        Value::String(secret.1.expose_for_token_request().to_string()),
    );
    Value::Object(m).to_string()
}

/// 把续好的令牌并进**刚从盘上读回来的**文档：只换 `section` 里这几格，其余键（同一节里别的键、别的节）一个不动。
pub fn merge_tokens(
    current: &Map<String, Value>,
    section: &str,
    secrets: &[(&str, &SecretKey)],
    plain: &[(&str, Value)],
) -> Map<String, Value> {
    let mut sec = current
        .get(section)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (k, s) in secrets {
        sec = merge_secret(&sec, k, s);
    }
    for (k, v) in plain {
        sec.insert((*k).to_string(), v.clone());
    }
    let mut out = current.clone();
    out.insert(section.to_string(), Value::Object(sec));
    out
}

#[cfg(test)]
#[path = "../../../../tests/common/creds-core/token_tests.rs"]
mod tests;
