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

/// 被「文件在才读」守着的 source（`[ -f X ]` · `[ -r X ]` · `test -f X`；同一行 `&&`、或包在 `if … then … fi` 里）——
/// 文件不在是正常的，不是失效；守的是别的文件、或裸 source 一个不在的文件 ⇒ 照报。
#[test]
fn 失效行_文件在才读的不报_守着别的文件的与裸的照报() {
    let h = tmp("guarded");
    let text = [
        "if [ -f ~/.extra_aliases ]; then",                   // 1
        "    . ~/.extra_aliases",                             // 2 守着
        "fi",                                                 // 3
        "[ -f ~/gone-a ] && . ~/gone-a",                      // 4 守着
        "[ -r \"$HOME/gone-b\" ] && source \"$HOME/gone-b\"", // 5 守着
        "test -f ~/gone-c && . ~/gone-c",                     // 6 守着
        "if test -r ~/gone-d; then source ~/gone-d; fi",      // 7 守着
        "if [ -f ~/gone-e ]",                                 // 8
        "then",                                               // 9
        "  echo x",                                           // 10
        "  . $HOME/gone-e",                                   // 11 守着（同一个文件的另一种写法）
        "fi",                                                 // 12
        ". ~/gone-bare",                                      // 13 裸的 ⇒ 报
        "if [ -f ~/other ]; then",                            // 14
        "  . ~/gone-f",                                       // 15 守的是别的文件 ⇒ 报
        "fi",                                                 // 16
        "[ -f ~/other ] && . ~/gone-g",                       // 17 同上 ⇒ 报
        ". ~/gone-h",                                         // 18 if 已经关了 ⇒ 报
    ]
    .join("\n");
    let got = crate::platform::shell::posix::dead_source_lines(&text, Some(&h));
    let nums: Vec<usize> = got.iter().map(|(n, _)| *n).collect();
    assert_eq!(nums, vec![13, 15, 17, 18]);
    let _ = std::fs::remove_dir_all(&h);
}
