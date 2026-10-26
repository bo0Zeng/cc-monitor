//! 这台 sshd 的配置关没关端口转发：照 sshd 的读法（先读到的算数 · Include 就地展开 · Match 段是条件段），说不清就不报。

use super::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MAIN: &str = "/etc/ssh/sshd_config";

/// 假的那台：路径 ⇒ 内容（不在表里 = 读不到）。
fn judge(files: &[(&str, &str)]) -> Option<ForwardingOff> {
    let map: BTreeMap<PathBuf, String> = files
        .iter()
        .map(|(p, t)| (PathBuf::from(p), t.to_string()))
        .collect();
    let read = |p: &Path| map.get(p).cloned();
    let list = |dir: &Path| {
        Some(
            map.keys()
                .filter(|k| k.parent() == Some(dir))
                .cloned()
                .collect::<Vec<_>>(),
        )
    };
    forwarding_off(Path::new(MAIN), &read, &list)
}

#[test]
fn 关了转发_报那一行与改好的那一行() {
    let got = judge(&[(
        MAIN,
        "Port 22\n# AllowTcpForwarding yes\n  allowtcpforwarding   no\n",
    )])
    .expect("该报");
    assert_eq!(got.path, MAIN);
    assert_eq!(got.line, 3);
    assert_eq!(got.text, "  allowtcpforwarding   no");
    assert_eq!(got.fix, "AllowTcpForwarding yes");
}

#[test]
fn 只许反向_也算关了本地转发() {
    let got = judge(&[(MAIN, "AllowTcpForwarding=remote\n")]).expect("该报");
    assert_eq!(got.fix, "AllowTcpForwarding yes");
}

#[test]
fn disable_forwarding_一并关掉() {
    let got = judge(&[(MAIN, "DisableForwarding yes\n")]).expect("该报");
    assert_eq!(got.fix, "DisableForwarding no");
}

#[test]
fn 先读到的算数_前面的_yes_压过后面的_no() {
    assert!(judge(&[(MAIN, "AllowTcpForwarding yes\nAllowTcpForwarding no\n")]).is_none());
    assert!(judge(&[(MAIN, "AllowTcpForwarding local\n")]).is_none());
    assert!(judge(&[(MAIN, "Port 22\n")]).is_none(), "缺省就是许");
}

#[test]
fn include_就地展开_相对_etc_ssh_带星号按字典序() {
    let files = [
        (
            MAIN,
            "Include /etc/ssh/sshd_config.d/*.conf\nAllowTcpForwarding yes\n",
        ),
        (
            "/etc/ssh/sshd_config.d/50-cloud.conf",
            "AllowTcpForwarding no\n",
        ),
        ("/etc/ssh/sshd_config.d/10-a.conf", "Port 22\n"),
        ("/etc/ssh/sshd_config.d/x.txt", "AllowTcpForwarding yes\n"),
    ];
    let got = judge(&files).expect("Include 里先读到 no");
    assert_eq!(got.path, "/etc/ssh/sshd_config.d/50-cloud.conf");
    assert_eq!(got.line, 1);
    let rel = [
        (MAIN, "Include sshd_config.d/*.conf\n"),
        (
            "/etc/ssh/sshd_config.d/50-cloud.conf",
            "AllowTcpForwarding no\n",
        ),
    ];
    assert!(judge(&rel).is_some(), "相对路径按 /etc/ssh 算");
}

#[test]
fn 说不清就不报_match_里也写了_include_读不到_主文件读不到() {
    assert!(judge(&[(
        MAIN,
        "AllowTcpForwarding no\nMatch User alice\n  AllowTcpForwarding yes\n"
    )])
    .is_none());
    assert!(judge(&[(MAIN, "Include /etc/ssh/gone.conf\nAllowTcpForwarding no\n")]).is_none());
    assert!(judge(&[]).is_none());
    // Match 段里没写这两个关键字 ⇒ 条件段外的那一句照算。
    assert!(judge(&[(
        MAIN,
        "AllowTcpForwarding no\nMatch User alice\n  X11Forwarding no\n"
    )])
    .is_some());
}
