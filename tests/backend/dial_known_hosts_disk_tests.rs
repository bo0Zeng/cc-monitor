//! cc-monitor 自己那份 known_hosts 的落盘那一半（真文件）：经那只只会记钥匙的手写，落在写句柄的家底下。
//! 纯函数那一半（一行一台 · 主机名写法）住 `dial_known_hosts_tests.rs`。

use super::*;

/// 记下一台：经写句柄落在家目录那一根底下的 `.cc-monitor/known_hosts`（建那一层）；同一把再记一次盘上不动；
/// 换了钥匙换那一行、别的台留着。
#[test]
fn remembering_lands_under_the_handles_home_and_only_rewrites_on_change() {
    let home = std::env::temp_dir().join(format!("ccm-kh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("建夹具家目录");
    let w = Writer(crate::platform::fs::Own::issue_at(&home));
    let file = home.join(".cc-monitor").join("known_hosts");
    w.remember("h", 2222, "ssh-ed25519 AAAA1");
    assert_eq!(
        std::fs::read_to_string(&file).expect("记下了"),
        "[h]:2222 ssh-ed25519 AAAA1\n"
    );
    let first = crate::platform::fs::Ro::stamp(&file);
    w.remember("h", 2222, "ssh-ed25519 AAAA1");
    assert_eq!(crate::platform::fs::Ro::stamp(&file), first, "同一把不写");
    w.remember("other", 22, "ssh-rsa BBBB");
    w.remember("h", 2222, "ssh-ed25519 AAAA2");
    assert_eq!(
        std::fs::read_to_string(&file).expect("读回"),
        "[h]:2222 ssh-ed25519 AAAA2\nother ssh-rsa BBBB\n"
    );
    let _ = std::fs::remove_dir_all(&home);
}
