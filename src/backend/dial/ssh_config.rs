//! **`~/.ssh/config` 的解读住这里** —— 与拨号同一个家（本目录）。
//!
//! 三条帧命令（界面经 `chan.call(<local>, …)` 问本机常驻后端；远端后端同一份代码，答的是它自己那台的 config）：
//!
//! | 命令 | 成品 |
//! |---|---|
//! | `ssh-config-aliases` | `{aliases:[…]}`：`Host` 行里非通配的别名（去重保序） |
//! | `ssh-config-resolve {alias}` | 一台的有效连接参数（系统 `ssh -G` 解析，Include / Match / 通配都由它处理） |
//! | `ssh-config-import` | 全部别名逐个解析后按「同一台机器的多个地址」聚合成的预览组 |
//!
//! 从 monitor `stream_source/` 原样搬来（规则一个字没改）；monitor 从此一处 `.ssh` 都不读、不起 `ssh`（「monitor 零 SSH」字面成立）。
//! 只读 `~/.ssh/config`，**不碰任何密钥文件**（`identityfile` 只问「在不在」，不读内容）。

use crate::platform::child::{Child, Deadline};
use copy_core::copy_text;
use serde::Serialize;

/// 命令级错误：`(code, message)`。
type CmdErr = (&'static str, String);

/// 一个别名解析出的有效连接参数（`ssh-config-resolve` 的成品）。线上 camelCase（界面 `ssh-config-reads.ts` 严格收）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolvedHost {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) user: String,
    /// 第一个**存在**的 IdentityFile（`~` 已展开）。都不存在 ⇒ `null`（用户可改走 agent）。
    pub(crate) key_path: Option<String>,
    /// `ssh -G` 的 `proxyjump`（`none` ⇒ `null`）。
    pub(crate) proxy_jump: Option<String>,
}

/// 预览组里的一个来源别名（界面「拆分」时据此还原成独立机器）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportMember {
    pub(crate) alias: String,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) proxy_jump: Option<String>,
}

/// 批量导入预览的一组：聚合后的一台建议机器（含多地址与来源成员）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportGroup {
    pub(crate) label: String,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) user: String,
    pub(crate) key_path: Option<String>,
    /// 组内除 `host` 外的其余地址（去重；端口与组首不同则 `host:port`）。单机组为空。
    pub(crate) addresses: Vec<String>,
    /// 组内首个非空 proxyjump（别名）。
    pub(crate) jump: Option<String>,
    pub(crate) members: Vec<ImportMember>,
}

/// 从 `~/.ssh/config` 文本里抽出非通配的 host 别名（纯函数）。
///
/// 逐行认 `Host`（大小写不敏感，关键字与别名间空白或 `=`）；排除含 `*` / `?` 的 pattern 与 `!` 否定；去重保序。
/// 不展开 `Include`、不解析 `Match`（真正的参数解析交给 `ssh -G`）。**只看 `Host` 行** —— 别的指令的值一个都不许流出去。
pub(crate) fn parse_host_aliases(content: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut aliases = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.splitn(2, |c: char| c.is_whitespace() || c == '=');
        let keyword = parts.next().unwrap_or("");
        if !keyword.eq_ignore_ascii_case("host") {
            continue;
        }
        let rest = parts.next().unwrap_or("").trim();
        for tok in rest.split_whitespace() {
            if tok.contains('*') || tok.contains('?') || tok.starts_with('!') {
                continue;
            }
            if seen.insert(tok.to_string()) {
                aliases.push(tok.to_string());
            }
        }
    }
    aliases
}

/// 别名 allowlist（`^[A-Za-z0-9._@:-]+$`）：挡住 ssh 选项 / 参数注入。`-` 开头另挡（[`resolve`]）。
pub(crate) fn is_safe_alias(alias: &str) -> bool {
    !alias.is_empty()
        && alias
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '@' | ':' | '-'))
}

/// 以 `~` 开头的路径展开成绝对路径（`~` / `~/...`）。其余原样。
pub(crate) fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
            if let Some(home) = crate::platform::paths::home_dir() {
                let rest = rest.trim_start_matches(['/', '\\']);
                return if rest.is_empty() {
                    home
                } else {
                    home.join(rest)
                };
            }
        }
    }
    std::path::PathBuf::from(path)
}

/// `ssh -G <alias>` 的 stdout ⇒ [`ResolvedHost`]（纯逻辑）。`hostname` 缺省回退别名；`identityfile` 取第一个展开后存在的。
pub(crate) fn parse_ssh_g_output(stdout: &str, alias: &str) -> ResolvedHost {
    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut user: Option<String> = None;
    let mut key_path: Option<String> = None;
    let mut proxy_jump: Option<String> = None;
    for line in stdout.lines() {
        let mut it = line.splitn(2, char::is_whitespace);
        let key = it.next().unwrap_or("").trim().to_ascii_lowercase();
        let val = it.next().unwrap_or("").trim();
        if val.is_empty() {
            continue;
        }
        match key.as_str() {
            "hostname" => host = Some(val.to_string()),
            "port" => port = val.parse::<u16>().ok(),
            "user" => user = Some(val.to_string()),
            "identityfile" if key_path.is_none() => {
                let expanded = expand_tilde(val);
                if expanded.exists() {
                    key_path = Some(expanded.to_string_lossy().to_string());
                }
            }
            "proxyjump" if !val.eq_ignore_ascii_case("none") => {
                proxy_jump = Some(val.to_string());
            }
            _ => {}
        }
    }
    ResolvedHost {
        host: host.unwrap_or_else(|| alias.to_string()),
        port: port.unwrap_or(22),
        user: user.unwrap_or_default(),
        key_path,
        proxy_jump,
    }
}

/// 别名基名前缀：首个 `-` / `_` / `.` 前的段（`devbox-lan` → `devbox`）。同机聚合判据之一。
pub(crate) fn alias_base(alias: &str) -> String {
    alias
        .split(['-', '_', '.'])
        .next()
        .unwrap_or(alias)
        .to_string()
}

/// 智能聚合：同 `(keyPath, user, 基名前缀)` 的多个别名判为**同一台机器的多个地址**，聚成一组；否则各自一组。保序。
/// 单成员组用完整别名当 `label`（否则 `prod-web` / `prod-db` 异 key 拆成两组却同名，落卡时会丢一台）。
pub(crate) fn aggregate_ssh_hosts(hosts: Vec<(String, ResolvedHost)>) -> Vec<ImportGroup> {
    type GKey = (Option<String>, String, String);
    let mut groups: Vec<(GKey, ImportGroup)> = Vec::new();
    for (alias, r) in hosts {
        let gkey: GKey = (r.key_path.clone(), r.user.clone(), alias_base(&alias));
        let member = ImportMember {
            alias,
            host: r.host.clone(),
            port: r.port,
            proxy_jump: r.proxy_jump.clone(),
        };
        if let Some((_, g)) = groups.iter_mut().find(|(k, _)| *k == gkey) {
            let addr = if r.port == g.port {
                r.host.clone()
            } else {
                format!("{}:{}", r.host, r.port)
            };
            if r.host != g.host && !g.addresses.contains(&addr) {
                g.addresses.push(addr);
            }
            g.members.push(member);
            if g.jump.is_none() {
                g.jump = r.proxy_jump.clone();
            }
        } else {
            groups.push((
                gkey,
                ImportGroup {
                    label: alias_base(&member.alias),
                    host: r.host.clone(),
                    port: r.port,
                    user: r.user.clone(),
                    key_path: r.key_path.clone(),
                    addresses: Vec::new(),
                    jump: r.proxy_jump.clone(),
                    members: vec![member],
                },
            ));
        }
    }
    let mut out: Vec<ImportGroup> = groups.into_iter().map(|(_, g)| g).collect();
    for g in &mut out {
        if g.members.len() == 1 {
            g.label = g.members[0].alias.clone();
        }
    }
    out
}

/// 读 `~/.ssh/config` 列别名。文件不在 / 读不了 ⇒ 空（没有 config 是正常的）。
pub(crate) fn list_aliases() -> Vec<String> {
    let Some(path) = crate::platform::paths::home_dir().map(|h| h.join(".ssh").join("config"))
    else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .map(|c| parse_host_aliases(&c))
        .unwrap_or_default()
}

/// `ssh -G` 一发的期限：界面等 `ssh-config-resolve` 的预算是 10 s，收一档到 5 s（只读配置、不建连接，正常毫秒级）。
const SSH_G_WITHIN: Deadline = Deadline::secs(5);

/// 用系统 `ssh -G <alias>` 解析一个别名。别名先过 allowlist、`-` 开头另挡；argv 直传、不过 shell。
/// `ssh -G` 只读配置、不建连接。
pub(crate) fn resolve(alias: &str) -> Result<ResolvedHost, CmdErr> {
    let alias = alias.trim();
    if !is_safe_alias(alias) {
        return Err((
            "bad_alias",
            copy_text("beSshConfig.host.badAlias", &[("alias", alias)]),
        ));
    }
    if alias.starts_with('-') {
        return Err(("bad_alias", copy_text("beSshConfig.host.dashAlias", &[])));
    }
    let out = Child::new("ssh")
        .arg("-G")
        .arg(alias)
        .run(SSH_G_WITHIN)
        .map_err(|e| {
            e.into_cmd_err("failed", |e| {
                copy_text("beSshConfig.host.sshGFailed", &[("e", &e.to_string())])
            })
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err((
            "failed",
            copy_text(
                "beSshConfig.host.sshGExit",
                &[("alias", alias), ("detail", stderr.trim())],
            ),
        ));
    }
    Ok(parse_ssh_g_output(
        &String::from_utf8_lossy(&out.stdout),
        alias,
    ))
}

/// 批量导入：列全部别名、逐个 `ssh -G`（保序）、聚合成预览组。解析失败的别名跳过（best-effort）。
pub(crate) fn import() -> Vec<ImportGroup> {
    let resolved: Vec<(String, ResolvedHost)> = list_aliases()
        .into_iter()
        .filter_map(|a| resolve(&a).ok().map(|r| (a, r)))
        .collect();
    aggregate_ssh_hosts(resolved)
}

fn to_value<T: Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).unwrap_or(serde_json::Value::Null)
}

/// `ssh-config-aliases` 的成品。
pub(crate) fn answer_aliases() -> serde_json::Value {
    serde_json::json!({ "aliases": list_aliases() })
}

/// `ssh-config-resolve {alias}` 的成品。
pub(crate) fn answer_resolve(args: &serde_json::Value) -> Result<serde_json::Value, CmdErr> {
    let alias = args.get("alias").and_then(|v| v.as_str()).ok_or((
        "invalid_args",
        crate::common::contract::malformed("missing `alias`"),
    ))?;
    resolve(alias).map(|r| to_value(&r))
}

/// `ssh-config-import` 整条命令总期限的上限（每个别名一发 `ssh -G`，整批共用；用完了剩下的别名各自超时、照「解析失败」跳过）。
pub(crate) const SSH_IMPORT_CAP: Deadline = Deadline::secs(28);

/// `ssh-config-import` 的成品。
pub(crate) fn answer_import() -> serde_json::Value {
    serde_json::json!({ "groups": to_value(&import()) })
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_ssh_config_tests.rs"]
mod tests;
