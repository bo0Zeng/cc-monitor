//! cc-monitor 那份主机钥匙库的落盘那一半（真文件）：经那只手改写、落在写句柄的家底下；终端那份 ssh 配置写在旁边、ssh 真读得懂。
//! 那份配置的全文住 `dial_known_hosts_tests.rs`。

use super::*;

fn sandbox(tag: &str) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!("ccm-kh-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("建夹具家目录");
    home
}

/// 改写：落在家目录那一根底下的 `.cc-monitor/known_hosts`（建那一层）；改写交来的是现有全文；`None` ⇒ 盘上不动。
#[test]
fn rewriting_lands_under_the_handles_home_and_only_writes_on_change() {
    let home = sandbox("rw");
    let w = Writer(crate::platform::fs::Own::issue_at(&home));
    let file = home.join(".cc-monitor").join("known_hosts");
    w.rewrite(&mut |t| Some(format!("{t}a ssh-ed25519 AAAA1\n")))
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "a ssh-ed25519 AAAA1\n"
    );
    let first = crate::platform::fs::Ro::stamp(&file);
    let mut seen = String::new();
    w.rewrite(&mut |t| {
        seen = t.to_string();
        None
    })
    .unwrap();
    assert_eq!(seen, "a ssh-ed25519 AAAA1\n");
    assert_eq!(crate::platform::fs::Ro::stamp(&file), first, "没变不写");
    let _ = std::fs::remove_dir_all(&home);
}

/// 终端那份配置写在库旁边、内容没变不重写；交给真的 `ssh -G -F`：库那一格是我们的，用户那份与系统那份都还读着。
#[cfg(unix)]
#[test]
fn ssh_reads_our_known_hosts_and_still_reads_both_user_and_system_configs() {
    let home = sandbox("cfg");
    let w = Writer(crate::platform::fs::Own::issue_at(&home));
    let cfg = w.terminal_config().unwrap();
    assert_eq!(cfg, home.join(".cc-monitor").join("ssh_config"));
    let stamp = crate::platform::fs::Ro::stamp(&cfg);
    w.terminal_config().unwrap();
    assert_eq!(crate::platform::fs::Ro::stamp(&cfg), stamp, "没变不写");
    // 真那份配置里 `~` 指的是 ssh 自己认的家（不是测试的家）⇒ 用同一个函数另写一份指沙箱里的两份，交给 ssh。
    let user = home.join("user_config");
    let system = home.join("system_config");
    std::fs::write(&user, "Host there\n  Port 2345\n").unwrap();
    std::fs::write(&system, "Host *\n  ServerAliveInterval 77\n").unwrap();
    let known = home.join(".cc-monitor").join("known_hosts");
    let probe = home.join("probe_config");
    std::fs::write(
        &probe,
        config_text(
            &known.to_string_lossy(),
            &user.to_string_lossy(),
            &system.to_string_lossy(),
        ),
    )
    .unwrap();
    let out = std::process::Command::new("ssh")
        .arg("-G")
        .arg("-F")
        .arg(&probe)
        .arg("there")
        .output()
        .expect("这台没有 ssh 客户端：判不了");
    let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
    assert!(out.status.success(), "{text}");
    let has = |line: &str| text.lines().any(|l| l.trim() == line);
    assert!(
        has(&format!("userknownhostsfile {}", known.display()).to_lowercase()),
        "{text}"
    );
    assert!(has("port 2345"), "用户那份没读到：{text}");
    assert!(has("serveraliveinterval 77"), "系统那份没读到：{text}");
    let _ = std::fs::remove_dir_all(&home);
}
