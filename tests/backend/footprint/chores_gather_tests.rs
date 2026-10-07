//! 收事实那几样里能离线判的：PATH 上的 ccm 是不是我们那一份 · 启动文件里确证失效的行。

use super::*;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "st8-gather-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn path_上先找到的是我们那一份_不报_别的那一份_报() {
    let h = tmp("ccm");
    let ours = h.join(".cc-monitor/bin");
    let other = h.join("other");
    std::fs::create_dir_all(&ours).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(ours.join("ccm"), "x").unwrap();
    std::fs::write(other.join("ccm"), "x").unwrap();
    let p = std::env::join_paths([&ours, &other]).unwrap();
    assert!(stale_ccm(&h, Some(&p), false).is_none());
    let p = std::env::join_paths([&other, &ours]).unwrap();
    let s = stale_ccm(&h, Some(&p), false).unwrap();
    assert_eq!(s.path, other.join("ccm").display().to_string());
    assert!(!s.needs_root, "家目录底下的不用 sudo");
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn 失效行_只报_source_的文件不在的_围栏里与注释与说不清的不报() {
    let h = tmp("dead");
    std::fs::write(h.join("here.sh"), "").unwrap();
    let text = "source ~/here.sh\nsource ~/gone.sh\n. $HOME/gone2\n# source ~/gone3\nsource $FOO/x\n# === cc-monitor BEGIN ===\nsource ~/gone4\n# === cc-monitor END ===\nsource \"/nope/abs\"\n";
    let got = crate::platform::shell::posix::dead_source_lines(text, Some(&h));
    let nums: Vec<usize> = got.iter().map(|(n, _)| *n).collect();
    assert_eq!(nums, vec![2, 3, 9]);
    let _ = std::fs::remove_dir_all(&h);
}
