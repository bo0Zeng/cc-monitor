//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md` 的 `bus-list` 节（按序找，找不到说清查过哪儿）
//!
//! 核原文：`bus-list` 节错误码 `not_installed` 逐字「找不到 `cc-list`，消息里带查过哪些位置」。经本模块兑现 —— 本族判候选按序先中、同名目录不算可执行、
//! 找不到逐条列出查过的位置；「`search_path: false` 真不看 `PATH`」那一条没有逐字原文。〔JA1 点址 2026-09-24〕

use super::*;

/// 找不到时那句话要**逐条列出查过的位置**，并把调用方的那句尾巴带上。
#[test]
fn the_not_installed_message_names_every_place_it_looked() {
    let fixed = vec![
        PathBuf::from("/opt/a/tool"),
        PathBuf::from("/home/u/bin/tool"),
    ];
    let msg = not_installed_message("tool", &fixed, 9, "装了吗？（用 TOOL_DIR 指过来）");
    assert!(msg.contains("/opt/a/tool"), "{msg}");
    assert!(msg.contains("/home/u/bin/tool"), "{msg}");
    assert!(msg.contains("9 个目录"), "PATH 那半没说：{msg}");
    assert!(msg.contains("TOOL_DIR"), "调用方的尾巴丢了：{msg}");
}

/// 一个固定位置都没有（比如 HOME 也解不出来）时，不许打出一句空白。
#[test]
fn an_empty_candidate_list_still_says_something_useful() {
    let msg = not_installed_message("tool", &[], 0, "");
    assert!(msg.contains("HOME"), "空候选时那句话什么都没说：{msg}");
}

/// ★ 顺序**就是**优先级：排在前面的先中。
#[test]
fn the_first_candidate_that_exists_wins() {
    let dir = std::env::temp_dir().join(format!("pd-find-{}", std::process::id()));
    let a = dir.join("a");
    let b = dir.join("b");
    std::fs::create_dir_all(&a).expect("mkdir a");
    std::fs::create_dir_all(&b).expect("mkdir b");
    for d in [&a, &b] {
        let f = d.join("tool");
        std::fs::write(&f, b"#!/bin/sh\n").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
    }
    let fixed = vec![b.join("tool"), a.join("tool")];
    let got = find("tool", &fixed, false, "").expect("应当找得到");
    assert_eq!(got, b.join("tool"), "排在前面的那个没有优先");
    std::fs::remove_dir_all(&dir).ok();
}

/// ★ 同名的**目录**不许被当成可执行文件（反推样本逐字记着的那个坑）。
#[test]
fn a_directory_with_the_same_name_is_not_executable() {
    let dir = std::env::temp_dir().join(format!("pd-dir-{}", std::process::id()));
    let trap = dir.join("tool");
    std::fs::create_dir_all(&trap).expect("mkdir trap");
    assert!(
        !is_executable(&trap),
        "同名目录被当成了可执行文件 —— 那正是被劫持的形状"
    );
    let fixed = vec![trap.clone()];
    let err = find("tool", &fixed, false, "尾巴").expect_err("目录不该被当成找到了");
    assert!(err.contains("尾巴"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

/// `search_path: false` 时**真的不看** `PATH` —— 那句话里的目录数必须是 0。
#[test]
fn opting_out_of_path_really_skips_it() {
    let err = find("no-such-tool-anywhere", &[], false, "").expect_err("不该找得到");
    assert!(
        err.contains("PATH 上的 0 个目录"),
        "说没走 PATH，但那句话里的目录数不是 0：{err}"
    );
}

/// ★Windows 那一臂只认 `.exe`（大小写不敏感）：`.bat` / `.cmd` 要经 `cmd.exe` 才起得来，
/// 插件口 argv 直传、不过 shell ⇒ 认它们等于认一个起不来的东西；没有扩展名的（unix 那种）也不认。
/// 纯函数那一格在哪个平台上都测得到（真正的 Windows 臂只在 Windows 上编）。
#[test]
fn on_windows_only_an_exe_counts_as_launchable() {
    use std::path::Path;
    for yes in ["C:/x/tool.exe", "a.EXE", "b.Exe"] {
        assert!(windows_launchable_name(Path::new(yes)), "{yes}");
    }
    for no in [
        "C:/x/tool",
        "run.bat",
        "run.cmd",
        "tool.com",
        "x.exe.txt",
        "exe",
    ] {
        assert!(!windows_launchable_name(Path::new(no)), "{no}");
    }
}
