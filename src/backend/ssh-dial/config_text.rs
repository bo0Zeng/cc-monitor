//! 手机导入的那一段 `~/.ssh/config` 文本 ⇒ 可导入的主机（通配那几块滤掉）。手机上没有 `ssh -G`；桌面的导入照旧调系统那一份
//! （后端 `dial/ssh_config.rs`），这里只服务手机。
//!
//! 认 `Host` · `HostName` · `Port` · `User` · `IdentityFile`（键不分大小写；`Key value` 与 `Key=value` 都行）；忽略注释与空行；
//! 别名里带通配（`*` `?` `!`）的那一块跳过；`Host a b` 多别名各出一条。不处理 `Include` · `ProxyJump` · `Match`。

use serde::Serialize;

/// 一个具体主机。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigHost {
    /// `Host` 别名（作标签）。
    pub alias: String,
    /// `HostName`（缺 ⇒ 别名）。
    pub host_name: String,
    pub port: u16,
    pub user: Option<String>,
    /// `IdentityFile` 原值（手机读不到那个文件，只按文件名去配已导入的钥匙）。
    pub identity_file: Option<String>,
}

/// 解析整段文本。
pub fn parse(text: &str) -> Vec<ConfigHost> {
    let mut out = Vec::new();
    let mut aliases: Vec<String> = Vec::new();
    let mut host_name: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut user: Option<String> = None;
    let mut identity: Option<String> = None;
    let flush = |out: &mut Vec<ConfigHost>,
                 aliases: &[String],
                 host_name: &Option<String>,
                 port: Option<u16>,
                 user: &Option<String>,
                 identity: &Option<String>| {
        for a in aliases.iter().filter(|a| !a.contains(['*', '?', '!'])) {
            out.push(ConfigHost {
                alias: a.clone(),
                host_name: host_name.clone().unwrap_or_else(|| a.clone()),
                port: port.unwrap_or(22),
                user: user.clone(),
                identity_file: identity.clone(),
            });
        }
    };
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = split_key_value(line) else {
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                flush(&mut out, &aliases, &host_name, port, &user, &identity);
                aliases = value.split_whitespace().map(str::to_string).collect();
                host_name = None;
                port = None;
                user = None;
                identity = None;
            }
            "hostname" => host_name = Some(value.to_string()),
            "port" => port = value.parse().ok(),
            "user" => user = Some(value.to_string()),
            "identityfile" => identity = Some(value.to_string()),
            _ => {}
        }
    }
    flush(&mut out, &aliases, &host_name, port, &user, &identity);
    out
}

/// 拆 `Key value` / `Key=value`（先出现的那个分隔符算数）；没有值 ⇒ `None`。
fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let sep = line.find(|c: char| c == '=' || c == ' ' || c == '\t')?;
    let key = line[..sep].trim();
    let value = line[sep + 1..]
        .trim()
        .trim_start_matches('=')
        .trim()
        .trim_matches('"');
    (!key.is_empty() && !value.is_empty()).then_some((key, value))
}

#[cfg(test)]
#[path = "../../../tests/backend/ssh_dial_config_text_tests.rs"]
mod tests;
