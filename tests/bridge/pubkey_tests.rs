
/// ★★ **推公钥的入口真的净化过公钥吗**〔audit-0805 08-08，Phase G 第 54 件下半〕。
///
/// `sanitize_public_key` 有直接的行为判据，但主语是**净化函数本身**。
/// 08-08 实测：把 `push_public_key` 里那句 `let key = sanitize_public_key(&raw)?;`
/// 换成 `let key = raw.clone();`，**全仓 986 条判据一条不红** ——
/// 而那个 `key` 下一步就被 `build_authorized_keys_cmd` 拼进远端命令并 exec。
///
/// 净化在任何 I/O 之前 ⇒ 本条跑真路：喂一个**空**的公钥文件，
/// 要求**零网络**就被拒（净化第一条就是「公钥为空」）。
/// 净化没接上的话，它会带着空 key 往下走去连一个不存在的主机 —— 两句话分得开。
#[tokio::test]
async fn the_push_entry_point_actually_sanitizes_the_key() {
    let dir = std::env::temp_dir().join(format!(
        "ccm-pubkey-fence-probe-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let empty = dir.join("id_probe.pub");
    std::fs::write(&empty, "").expect("造空公钥");

    let cfg = crate::ssh_source::RemoteConfig {
        host: "这个主机一定不存在-audit0805".into(),
        label: "probe".into(),
        port: 1,
        user: "nobody".into(),
        key_path: None,
        daemon_path: "/tmp/nope".into(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    };
    let r = push_public_key(cfg, Some(empty.to_string_lossy().into_owned())).await;
    let _ = std::fs::remove_dir_all(&dir);

    let err = r
        .err()
        .unwrap_or_else(|| panic!("空公钥竟然一路走通了 —— 净化没接上"));
    assert!(
        err.contains("公钥为空"),
        "拒绝了，但不是净化拒的（错误：{err}）—— \
             说明它带着未净化的 key 越过了这一步，而下一步就是拼进远端命令 exec。"
    );
}
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
