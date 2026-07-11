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
        0 => return Err("公钥为空".to_string()),
        1 => {}
        _ => return Err("公钥文件含多行内容,拒绝(防注入);一次只推一把公钥".to_string()),
    }
    let key = non_empty[0];
    if key.chars().any(|c| c.is_control()) {
        return Err("公钥含控制字符,拒绝".to_string());
    }
    let mut parts = key.split_whitespace();
    let ktype = parts.next().unwrap_or("");
    let blob = parts.next().unwrap_or("");
    if !is_known_key_type(ktype) {
        return Err(format!("无法识别的公钥类型 `{ktype}`(是否误选了私钥?)"));
    }
    if blob.is_empty() {
        return Err("公钥缺少 base64 主体".to_string());
    }
    // base64 字符集轻校验(粗校验收紧):挡 `ssh-ed25519 !!!` 这类形态。
    if !blob
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
    {
        return Err("公钥 base64 主体含非法字符".to_string());
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
        Err(format!(
            "推送未返回预期标记,可能失败(权限/路径)。远端输出:{}",
            output.trim()
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
        .ok_or_else(|| {
            "未指定公钥:请在配置里填私钥路径(取同名 .pub),或选一个 .pub 文件".to_string()
        })?;

    // 2) 读本地公钥 + 校验(注入防护红线)。
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读公钥 {path} 失败: {e}"))?;
    let key = sanitize_public_key(&raw)?;

    // 3) 构造 + 一次性 exec,读 stdout 取标记。
    let cmd = build_authorized_keys_cmd(&key);
    let stream = ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
    let mut reader = BufReader::new(stream);
    let mut out = String::new();
    reader
        .read_to_string(&mut out)
        .await
        .map_err(|e| format!("读推送结果失败: {e}"))?;
    let outcome = parse_push_outcome(&out)?;

    Ok(PushResult {
        outcome: outcome.as_str().to_string(),
        pub_path: path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_accepts_valid_keys() {
        assert_eq!(
            sanitize_public_key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 user@host\n").unwrap(),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 user@host"
        );
        assert!(sanitize_public_key("ssh-rsa AAAAB3NzaC1yc2E comment").is_ok());
        assert!(sanitize_public_key("ecdsa-sha2-nistp256 AAAAE2VjZHNh x").is_ok());
        assert!(sanitize_public_key("sk-ssh-ed25519@openssh.com AAAAG x").is_ok());
        // 前后空行/空白仍取那一行并 trim
        assert_eq!(
            sanitize_public_key("\n  ssh-ed25519 AAAA key-comment  \n\n").unwrap(),
            "ssh-ed25519 AAAA key-comment"
        );
    }

    #[test]
    fn sanitize_rejects_bad_input() {
        assert!(sanitize_public_key("").is_err()); // 空
        assert!(sanitize_public_key("   \n  \n").is_err()); // 全空白
                                                            // 双非空行 → 防第二行注入
        assert!(sanitize_public_key("ssh-ed25519 AAAA a\nssh-rsa BBBB b").is_err());
        // 控制字符(含 NUL)
        assert!(sanitize_public_key("ssh-ed25519 AA\0AA x").is_err());
        // 未知类型(误选私钥 / 任意文本 / 注入尝试)
        assert!(sanitize_public_key("rm -rf /").is_err());
        assert!(sanitize_public_key("-----BEGIN OPENSSH PRIVATE KEY-----").is_err());
        // 缺 base64 主体
        assert!(sanitize_public_key("ssh-ed25519").is_err());
        // base64 主体含非法字符
        assert!(sanitize_public_key("ssh-ed25519 !!! comment").is_err());
    }

    #[test]
    fn cmd_follows_aterm_contract() {
        let c = build_authorized_keys_cmd("ssh-ed25519 AAAA user@host");
        assert!(c.contains("printf '%s\\n'"), "须 printf %s 而非 echo");
        assert!(!c.contains("echo "), "不得用 echo");
        assert!(c.contains("grep -qxF"), "须 grep -qxF 精确去重");
        assert!(c.contains("mkdir -p"));
        assert!(c.contains("chmod 700"));
        assert!(c.contains("chmod 600"));
        assert!(c.contains("ADDED"));
        assert!(c.contains("ALREADY"));
        // key 经 shell_quote 单引号包裹
        assert!(c.contains("'ssh-ed25519 AAAA user@host'"));
    }

    #[test]
    fn cmd_escapes_single_quote_injection() {
        // 公钥正常不含单引号;异常输入下 shell_quote 须逃逸,不破坏命令结构。
        let c = build_authorized_keys_cmd("ssh-ed25519 AAAA a'b");
        assert!(c.contains(r"'\''"), "单引号须被 shell_quote 逃逸为 '\\''");
    }

    #[test]
    fn parse_outcome_markers() {
        assert_eq!(parse_push_outcome("ADDED\n").unwrap(), PushOutcome::Added);
        assert_eq!(
            parse_push_outcome("ALREADY\n").unwrap(),
            PushOutcome::Already
        );
        assert!(parse_push_outcome("").is_err());
        assert!(parse_push_outcome("mkdir: cannot create: permission denied\n").is_err());
    }

    /// D-重要1 回归:在真实 `sh` 上跑生成的命令,验证「既有 key 无尾换行时追加不粘行」+
    /// 幂等(同 key 再推 → ALREADY)。Windows CI 跳过(cfg unix)。
    #[cfg(unix)]
    #[test]
    fn append_respects_newline_boundary_and_idempotent() {
        use std::process::Command;
        let dir = std::env::temp_dir().join(format!("ccm-f50-{}", std::process::id()));
        let ssh = dir.join(".ssh");
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&ssh).unwrap();
        let ak = ssh.join("authorized_keys");
        // 既有 key,故意**不带尾换行**——触发粘行 bug 的条件。
        std::fs::write(&ak, "ssh-ed25519 AAAAEXISTING existing@host").unwrap();

        let key = "ssh-ed25519 AAAANEWKEY new@host";
        let run = |k: &str| {
            Command::new("sh")
                .arg("-c")
                .arg(build_authorized_keys_cmd(k))
                .env("HOME", &dir)
                .output()
                .unwrap()
        };

        let out = run(key);
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("ADDED"),
            "首推应 ADDED"
        );
        let content = std::fs::read_to_string(&ak).unwrap();
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(
            lines.iter().any(|l| l.contains("AAAAEXISTING")),
            "既有 key 应保留独占一行: {content:?}"
        );
        assert!(
            lines.iter().any(|l| *l == key),
            "新 key 应独占一行: {content:?}"
        );
        assert!(
            !content.contains("AAAAEXISTINGssh-ed25519"),
            "两把 key 不得粘成一行: {content:?}"
        );

        // 同 key 再推 → 幂等 ALREADY,不重复追加。
        let out2 = run(key);
        assert!(
            String::from_utf8_lossy(&out2.stdout).contains("ALREADY"),
            "重复推同 key 应 ALREADY"
        );
        let n = std::fs::read_to_string(&ak)
            .unwrap()
            .matches("AAAANEWKEY")
            .count();
        assert_eq!(n, 1, "新 key 不应被追加两次");

        std::fs::remove_dir_all(&dir).ok();
    }
}
