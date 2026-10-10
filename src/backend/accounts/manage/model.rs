//! 账号清单（`~/.cc-monitor/accounts/accounts.json`，schema v1）的**模型与读写** —— 纯：文本进、文本出，不碰盘。
//! 账号库在哪由家决定，清单里不记它；每个号的 `configDir` 照旧写绝对路径。
//!
//! 盘上那一份是用户的活数据，原样接着用：认得的格改，认不得的格（顶层与每个号上的）原样留着写回去；
//! 形状不合规矩的号（名字或配置目录过不了）整条原样留着、不去动它。
//! 账号 0（「不设 `CLAUDE_CONFIG_DIR`」这个状态）不是注册项：读时丢掉、写时在数组末尾合成一条（没有 `configDir` 键）。

use serde_json::{Map, Value};

/// 账号 0 的名字（保留名：不许注册同名的号）。
pub(crate) const ACCOUNT_ZERO: &str = "0";

/// 一个具名的号。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Account {
    pub name: String,
    pub email: String,
    pub config_dir: String,
    pub is_default: bool,
    /// 盘上 `authKind` 那一格原样（缺席 = 旧清单 = 订阅号）。
    pub auth_kind: Option<String>,
    /// 这个号上本模块不认得的键（原样写回）。
    pub extra: Map<String, Value>,
}

impl Account {
    pub(crate) fn is_api_key(&self) -> bool {
        acct_core::auth_kind_from_manifest(self.auth_kind.as_deref())
            == acct_core::AUTH_KIND_API_KEY
    }
}

/// 清单里的一条。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Entry {
    Managed(Account),
    /// 形状不合规矩的那条：原样留着、原样写回，不参与任何改动。
    Kept(Value),
}

/// 整份清单。
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Manifest {
    pub shared_store: Option<String>,
    pub entries: Vec<Entry>,
    /// 顶层本模块不认得的键（如 `claudeVersionPinned`），原样写回。
    pub extra: Map<String, Value>,
}

/// 顶层本模块认得（每次写都重算或原样处置）的键。
const TOP_KEYS: &[&str] = &["version", "updatedAt", "sharedStore", "accounts"];
/// 每个号上本模块认得的键。`mode` 每次写都按配置目录重算。
const ACCOUNT_KEYS: &[&str] = &[
    "name",
    "email",
    "configDir",
    "isDefault",
    "mode",
    "authKind",
];

/// 一个名字能不能当账号名（与 `ccm … --account <名>` 同一条规则）。
pub(crate) fn name_ok(name: &str) -> bool {
    shell_quote_core::account_name_ok(name)
}

/// 账号目录能不能交给下游拼命令（POSIX 形全表：绝对 · 无 `..` · 无元字符 / 控制符 / 视觉欺骗字符）。
pub(crate) fn config_dir_ok(dir: &str) -> bool {
    acct_core::config_dir_posix_ok(dir)
}

impl Manifest {
    /// 解析盘上那份。版本不是 1 / 不是 JSON 对象 ⇒ `Err`（这份清单不归本模块改）。
    pub(crate) fn parse(text: &str) -> Result<Manifest, String> {
        let body = text.strip_prefix('\u{feff}').unwrap_or(text);
        let root: Value = serde_json::from_str(body).map_err(|e| {
            crate::common::said::IntoNote::into_note(crate::common::said::Said::with_raw(
                copy_core::copy_text("beAccountsQuery.loadManifest.badJson", &[]),
                e,
            ))
        })?;
        let Value::Object(obj) = root else {
            return Err(copy_core::copy_text("beAcctModel.parse.notObject", &[]));
        };
        match obj.get("version").and_then(Value::as_u64) {
            Some(v) if v == acct_core::SUPPORTED_SCHEMA => {}
            other => {
                let shown = other.map_or_else(|| "-".to_string(), |v| v.to_string());
                return Err(copy_core::copy_text(
                    "beAccountsQuery.loadManifest.badVersion",
                    &[("v", &shown)],
                ));
            }
        }
        let mut m = Manifest {
            shared_store: obj
                .get("sharedStore")
                .and_then(Value::as_str)
                .map(str::to_string),
            ..Manifest::default()
        };
        for (k, v) in &obj {
            if !TOP_KEYS.contains(&k.as_str()) {
                m.extra.insert(k.clone(), v.clone());
            }
        }
        if let Some(arr) = obj.get("accounts").and_then(Value::as_array) {
            for item in arr {
                if is_zero_entry(item) {
                    continue;
                }
                m.entries.push(match account_of(item) {
                    Some(a) => Entry::Managed(a),
                    None => Entry::Kept(item.clone()),
                });
            }
        }
        Ok(m)
    }

    /// 写回去的全文。`zero_email` = 账号 0 此刻的登录邮箱（`$HOME/.claude.json` 里读的；没有就空串）。
    pub(crate) fn render(&self, shared_store: &str, zero_email: &str, updated_at: &str) -> String {
        let mut top: Vec<(String, Value)> = vec![
            ("version".into(), Value::from(acct_core::SUPPORTED_SCHEMA)),
            ("updatedAt".into(), Value::from(updated_at)),
            ("sharedStore".into(), Value::from(shared_store)),
        ];
        top.extend(self.extra.iter().map(|(k, v)| (k.clone(), v.clone())));
        let mut rows: Vec<String> = self
            .entries
            .iter()
            .map(|e| match e {
                Entry::Kept(v) => v.to_string(),
                Entry::Managed(a) => {
                    let mut kv: Vec<(String, Value)> = vec![
                        ("name".into(), Value::from(a.name.as_str())),
                        ("email".into(), Value::from(a.email.as_str())),
                        ("configDir".into(), Value::from(a.config_dir.as_str())),
                        ("isDefault".into(), Value::from(a.is_default)),
                        (
                            "mode".into(),
                            Value::from(if a.config_dir == shared_store {
                                "in-place"
                            } else {
                                "isolated"
                            }),
                        ),
                    ];
                    if let Some(k) = &a.auth_kind {
                        kv.push(("authKind".into(), Value::from(k.as_str())));
                    }
                    kv.extend(a.extra.iter().map(|(k, v)| (k.clone(), v.clone())));
                    inline_object(&kv)
                }
            })
            .collect();
        rows.push(inline_object(&[
            ("name".into(), Value::from(ACCOUNT_ZERO)),
            ("email".into(), Value::from(zero_email)),
            ("isDefault".into(), Value::from(false)),
            ("mode".into(), Value::from("bare")),
        ]));
        let mut out = String::from("{\n");
        for (k, v) in &top {
            out.push_str(&format!("  {}: {},\n", Value::from(k.as_str()), v));
        }
        out.push_str("  \"accounts\": [\n");
        out.push_str(
            &rows
                .iter()
                .map(|r| format!("    {r}"))
                .collect::<Vec<_>>()
                .join(",\n"),
        );
        out.push_str("\n  ]\n}\n");
        out
    }

    pub(crate) fn managed(&self) -> impl Iterator<Item = &Account> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Managed(a) => Some(a),
            Entry::Kept(_) => None,
        })
    }

    pub(crate) fn find(&self, name: &str) -> Option<&Account> {
        self.managed().find(|a| a.name == name)
    }

    /// 这个名字在清单里有没有人占着（含原样留着的那几条）。
    pub(crate) fn name_taken(&self, name: &str) -> bool {
        self.entries.iter().any(|e| match e {
            Entry::Managed(a) => a.name == name,
            Entry::Kept(v) => v.get("name").and_then(Value::as_str) == Some(name),
        })
    }

    /// 加一个号（追加在末尾：按下标取账号的地方下标都不变）。
    pub(crate) fn with_added(&self, a: Account) -> Manifest {
        let mut m = self.clone();
        m.entries.push(Entry::Managed(a));
        m
    }

    /// 只留一个默认号。
    pub(crate) fn with_default(&self, name: &str) -> Manifest {
        let mut m = self.clone();
        for e in &mut m.entries {
            if let Entry::Managed(a) = e {
                a.is_default = a.name == name;
            }
        }
        m
    }

    /// 删一个号；删掉的是默认号而还有别的号 ⇒ 第一个号接着当默认（`ccm` 裸起要有个默认号）。
    /// 回 `(新清单, 新默认号)`。
    pub(crate) fn without(&self, name: &str) -> (Manifest, Option<String>) {
        let was_default = self.find(name).is_some_and(|a| a.is_default);
        let mut m = self.clone();
        m.entries
            .retain(|e| !matches!(e, Entry::Managed(a) if a.name == name));
        if was_default {
            if let Some(first) = m.managed().next().map(|a| a.name.clone()) {
                return (m.with_default(&first), Some(first));
            }
        }
        (m, None)
    }

    /// 改一个号的邮箱。
    pub(crate) fn with_email(&self, name: &str, email: &str) -> Manifest {
        let mut m = self.clone();
        for e in &mut m.entries {
            if let Entry::Managed(a) = e {
                if a.name == name {
                    a.email = email.to_string();
                }
            }
        }
        m
    }
}

/// 账号 0 那一条：名字是 `0`、没有 `configDir` 键（写时合成的，读回时丢掉）。
fn is_zero_entry(v: &Value) -> bool {
    v.get("name").and_then(Value::as_str) == Some(ACCOUNT_ZERO) && v.get("configDir").is_none()
}

/// 一条认得出来的号；名字 / 配置目录过不了规矩 ⇒ `None`（整条原样留着）。
fn account_of(v: &Value) -> Option<Account> {
    let o = v.as_object()?;
    let name = o.get("name")?.as_str()?;
    let dir = o.get("configDir")?.as_str()?;
    if !name_ok(name) || !config_dir_ok(dir) {
        return None;
    }
    let email = match o.get("email") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return None,
    };
    let is_default = match o.get("isDefault") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return None,
    };
    let auth_kind = match o.get("authKind") {
        None => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return None,
    };
    let extra = o
        .iter()
        .filter(|(k, _)| !ACCOUNT_KEYS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Some(Account {
        name: name.to_string(),
        email,
        config_dir: dir.trim_end_matches('/').to_string(),
        is_default,
        auth_kind,
        extra,
    })
}

/// 一个 JSON 对象写成一行，键按给的顺序。
fn inline_object(kv: &[(String, Value)]) -> String {
    let body: Vec<String> = kv
        .iter()
        .map(|(k, v)| format!("{}: {}", Value::from(k.as_str()), v))
        .collect();
    format!("{{ {} }}", body.join(", "))
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/model_tests.rs"]
mod tests;
