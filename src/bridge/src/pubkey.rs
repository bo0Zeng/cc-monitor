//! F50 公钥一键推送 authorized_keys(aterm N2)。
//!
//! 把本地公钥追加到远端 `~/.ssh/authorized_keys`,推完免密。命令注入防护(aterm 契约,
//! 整条命令直译):`printf '%s\n'` 非 echo、`grep -qxF` 精确去重、mkdir/chmod 700/600、
//! 输出 `ADDED`/`ALREADY` 标记、只接受**单一非空行**(防第二行注入)。走
//! `connect_and_exec_cmd` 一次性 exec(账本:exec 通道只消费不改形),key 经 `shell_quote`
//! 单引号嵌入(与 remote_history 查询同一转义模式)。
//!
//! 认证前提:当前 `connect_session` 只做 publickey/agent 鉴权(密码鉴权未实现,F61 已取消)。故 v1 =
//! 已有 key/agent 访问权时追加/轮换新公钥;纯密码冷 onboarding 不支持(F61 已取消)。

use crate::copy_table::copy_text;
use crate::ssh_source::{self, shell_quote, RemoteConfig};
use serde::Serialize;
use tokio::io::{AsyncReadExt, BufReader};

/// 推送结果:key 是新加的还是本就存在。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Added,
    Already,
}

impl PushOutcome {
    fn as_str(self) -> &'static str {
        match self {
            PushOutcome::Added => "added",
            PushOutcome::Already => "already",
        }
    }
}

/// 前端回传:结果标记 + 实际推送的 .pub 路径(供 toast 显示)。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PushResult {
    pub outcome: String,
    pub pub_path: String,
}

/// 已知公钥类型前缀(粗校验形态,拒明显非公钥内容如误选私钥/任意文本)。
fn is_known_key_type(t: &str) -> bool {
    matches!(
        t,
        "ssh-ed25519" | "ssh-rsa" | "ssh-dss" | "ssh-ed25519-cert-v01@openssh.com"
    ) || t.starts_with("ecdsa-sha2-")
        || t.starts_with("sk-ssh-")
        || t.starts_with("sk-ecdsa-")
}

/// 校验 + 规范化公钥行。取**唯一**非空行(多于一行非空 → Err,防第二行注入);去首尾空白;
/// 拒空 / 含控制字符;粗校验形态(已知类型前缀 + 至少一个 base64 主体)。返回规范化单行 key。
pub fn sanitize_public_key(raw: &str) -> Result<String, String> {
    let non_empty: Vec<&str> = raw
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    match non_empty.len() {
        0 => return Err(copy_text("rsPubkey.sanitizePublicKey.empty", &[])),
        1 => {}
        _ => return Err(copy_text("rsPubkey.sanitizePublicKey.multiline", &[])),
    }
    let key = non_empty[0];
    if key.chars().any(|c| c.is_control()) {
        return Err(copy_text("rsPubkey.sanitizePublicKey.controlChars", &[]));
    }
    let mut parts = key.split_whitespace();
    let ktype = parts.next().unwrap_or("");
    let blob = parts.next().unwrap_or("");
    if !is_known_key_type(ktype) {
        return Err(copy_text(
            "rsPubkey.sanitizePublicKey.unknownType",
            &[("ktype", &ktype.to_string())],
        ));
    }
    if blob.is_empty() {
        return Err(copy_text("rsPubkey.sanitizePublicKey.noBody", &[]));
    }
    // base64 字符集轻校验(粗校验收紧):挡 `ssh-ed25519 !!!` 这类形态。
    if !blob
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
    {
        return Err(copy_text("rsPubkey.sanitizePublicKey.badBody", &[]));
    }
    Ok(key.to_string())
}

/// 构造远端 authorized_keys 追加命令(aterm 契约)。key 经 `shell_quote` 单引号嵌入;
/// `grep -qxF -- "$k"` 命中打 `ALREADY`,否则追加打 `ADDED`。命令走远端登录 shell `-c`
/// (单层,connect_and_exec_cmd 不再包装),故单引号转义是唯一需要的逃逸。
///
/// **换行边界(D-重要1)**:追加前若文件非空且末字节非换行,先补一个 `\n`——否则末行 key
/// 无尾换行时 `>>` 会把既有 key 与新 key 粘成一行(既坏旧 key 又让新 key 非法却假报 ADDED)。
/// `grep`/`printf` 加 `--`(纵深防御,sanitize 已保证 key 不以 `-` 打头)。
pub fn build_authorized_keys_cmd(key: &str) -> String {
    let k = shell_quote(key);
    // 只用 format! 拼装赋值;命令体用 raw 字符串,避开 `{}` 与 `\n` 的双重转义。
    format!("k={k}; ")
        + r#"d="$HOME/.ssh"; f="$d/authorized_keys"; mkdir -p "$d" && chmod 700 "$d" && touch "$f" && chmod 600 "$f" && { grep -qxF -- "$k" "$f" && printf 'ALREADY\n' || { { [ -s "$f" ] && [ -n "$(tail -c1 "$f")" ] && printf '\n' >> "$f"; }; printf '%s\n' "$k" >> "$f" && printf 'ADDED\n'; }; }"#
}

/// 解析远端 stdout 取结果标记。ADDED/ALREADY 互斥(命令末尾只跑一个 printf);都无 → Err
/// (mkdir/chmod 中途失败等,命令链断在标记前)。
pub fn parse_push_outcome(output: &str) -> Result<PushOutcome, String> {
    if output.contains("ADDED") {
        Ok(PushOutcome::Added)
    } else if output.contains("ALREADY") {
        Ok(PushOutcome::Already)
    } else {
        Err(copy_text(
            "rsPubkey.parsePushOutcome.noMarker",
            &[("output", &(output.trim()).to_string())],
        ))
    }
}

/// 一键推送本地公钥到远端 authorized_keys。`pub_key_path` 显式指定 .pub;为空时回退
/// `{key_path}.pub`(私钥同名公钥);再无则报错让前端选文件。
#[tauri::command]
pub async fn push_public_key(
    cfg: RemoteConfig,
    pub_key_path: Option<String>,
) -> Result<PushResult, String> {
    // 1) 定位本地公钥 .pub。
    let path = pub_key_path
        .filter(|p| !p.trim().is_empty())
        .or_else(|| {
            cfg.key_path
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .map(|kp| format!("{kp}.pub"))
        })
        .ok_or_else(|| copy_text("rsPubkey.pushPublicKey.noKey", &[]))?;

    // 2) 读本地公钥 + 校验(注入防护红线)。
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        copy_text(
            "rsPubkey.pushPublicKey.readFailed",
            &[("path", &path.to_string()), ("e", &e.to_string())],
        )
    })?;
    let key = sanitize_public_key(&raw)?;

    // 3) 构造 + 一次性 exec,读 stdout 取标记。
    let cmd = build_authorized_keys_cmd(&key);
    let stream = ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
    let mut reader = BufReader::new(stream);
    let mut out = String::new();
    reader.read_to_string(&mut out).await.map_err(|e| {
        copy_text(
            "rsPubkey.pushPublicKey.readResultFailed",
            &[("e", &e.to_string())],
        )
    })?;
    let outcome = parse_push_outcome(&out)?;

    Ok(PushResult {
        outcome: outcome.as_str().to_string(),
        pub_path: path,
    })
}

#[cfg(test)]
#[path = "../../../tests/bridge/pubkey_tests.rs"]
mod tests;
