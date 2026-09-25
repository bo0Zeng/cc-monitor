//! # 要求住址：`INVARIANTS §41.1`（多会话里杀掉其中一个 ⇒ tmux hook 通知后端，零轮询）
//!
//! 核原文：`INVARIANTS §41.1` 四路事件表逐字「tmux `session-created/closed/renamed[50]` hook → `--tmux-notify` → SIGUSR1」——
//! 三个事件缺一不可与 hook 参数形状判的是它；starttime 校验对 `§41.2`（starttime 不符即静默 no-op）；hook 里不许出现会话 sid 选项
//! 对 `§41.5` 逐字「在 hook 上下文会解析到**别的会话**」；零写盘对 `§41.6`（真保障是 `readonly_guard.rs` 的白名单，本族那张黑名单只是纵深）。〔JA1 点址 2026-09-24〕

use super::*;
use std::path::PathBuf;

/// 判据一律 `format!` 运行时拼、绝不把它当字面量写进源码 —— 否则扫源码的断言会被
/// **自己那行字面量**命中（P4a 里同一个自指陷阱连踩五次，其中一次让守卫成了安慰剂）。
fn prod_src() -> &'static str {
    let me = include_str!("../../../src/backend/control/tmux_hook.rs");
    let marker = "\n#[cfg(test)]\nmod tests";
    match me.find(marker) {
        Some(i) => &me[..i],
        None => me,
    }
}

/// 生产段**再剥掉注释**。本模块的头注**逐字解释**了那两个坑（`#{@ccm_sid}` 陷阱、
/// 被 `readonly_guard` 拦下的 `fs::create_dir`）—— 不剥的话，两条守卫会被
/// **解释它们自己的那段散文**命中而恒红（实测：两条一起红）。
/// 与 `tests/paste-block-guard.vitest.ts` 同一处置（那边也是「把注释当代码」栽过）。
fn prod_code() -> String {
    prod_src()
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn hook_args_shape() {
    let a = hook_set_args(
        "session-closed",
        &PathBuf::from("/opt/ccm/backend"),
        42,
        999,
    );
    assert_eq!(a[0], "set-hook");
    assert_eq!(a[1], "-g");
    assert_eq!(a[2], "session-closed[50]");
    assert_eq!(
        a[3],
        "run-shell -b ''\\''/opt/ccm/backend'\\'' --tmux-notify 42 999'"
    );
}

/// exe 路径含单引号也不能破坏命令串（我们自己产生的路径，但用户可以把 backend
/// 部署到任意目录）。
#[test]
fn exe_path_with_quote_is_escaped() {
    let a = hook_set_args("session-closed", &PathBuf::from("/o'p/d"), 1, 2);
    assert!(a[3].contains(r"'\''"), "单引号必须转义：{}", a[3]);
}

#[test]
fn unset_args_shape() {
    assert_eq!(
        hook_unset_args("session-renamed"),
        vec!["set-hook", "-gu", "session-renamed[50]"]
    );
}

/// ★ 三个事件缺一不可 —— `session-renamed` 最容易被当成可选：名字是 monitor 侧查表的
/// 键，改名不通知会让下一次差分把它误判成「一个消失 + 一个新出现」。
#[test]
fn all_three_events_are_hooked() {
    for e in ["session-created", "session-closed", "session-renamed"] {
        assert!(HOOK_EVENTS.contains(&e), "缺 hook 事件 {e}");
    }
    assert_eq!(HOOK_EVENTS.len(), 3);
}

/// ★ P0 那条陷阱：hook 里绝不许出现 `#{@ccm_sid}`（会拿到空 ⇒ 活会话被判灰）。
/// 本设计连会话名都不传，这条是**防将来**有人「顺便把名字带上」。
#[test]
fn never_uses_ccm_sid_format_in_hooks() {
    let forbidden = format!("#{}@ccm_sid{}", "{", "}");
    let code = prod_code();
    assert!(
        !code.contains(&forbidden),
        "hook 里不许用 {forbidden}（P0 实测：它在 hook 上下文取到空，会把活会话判灰）"
    );
    // 反向自检：真剥出了代码（不是把整份都过滤没了 ⇒ 断言空转）。
    assert!(
        code.contains("fn hook_set_args"),
        "剥注释后代码为空，断言在空转"
    );
}

/// ★ 本模块**零文件系统写**（红线 I7；P4 原设计就是栽在这）。
/// `readonly_guard` 已经全局扫一遍，这里再钉一次是因为**本文件是最可能复发的地方**。
#[test]
fn no_filesystem_writes_in_this_module() {
    // ★ **前提触发器**〔audit-0805 08-07〕：下面那张表是**黑名单**（列出已知的写 API），
    // 它天然漏掉没列的那些 —— 实测把 `std::fs::remove_file` 放进本模块生产段，
    // 本条**不红**（删除/改名/复制都不在表里）。
    //
    // 真正的保障是 `readonly_guard::every_fs_call_in_backend_production_is_read_only`：
    // 那条是**白名单**（默认拒绝），上面那一刀正是它逮住的。
    // 两层失效模式相反（黑名单漏新写法 / 白名单误伤新读法）⇒ 留着这层是纵深，不是重复；
    // 但**别把这张黑名单读成保障**。它一旦成了唯一的一层，本模块就没人守了。
    // ⚠⚠ **08-08 订正：光查名字挡不住「掏空」**。实测把 `readonly_guard` 那条白名单的
    // 函数体清空、名字原样留着 ⇒ 本触发器**照样绿**，而本模块就只剩下面那张黑名单了。
    // 「符号在 ≠ 它还在做那件事」——本区 08-08 在两条前提触发器上连撞两次。
    // ⇒ 三条腿：名字在 · 它还在堵**导入逃生口**（`is_hatch`）· 它还有**默认拒绝**那半。
    let guard = include_str!("../readonly_guard.rs");
    for (needle, why) in [
        (
            "fn every_fs_call_in_backend_production_is_read_only",
            "那条白名单整个不见了",
        ),
        (
            "fn is_hatch",
            "白名单还在，但**堵逃生口那一段没了** —— `use std::fs::{…}` 之后调用点不带前缀，\
                 白名单会整个瞎掉（08-06 实测：那样一次真写盘，六条判据全绿）",
        ),
        (
            "bad.is_empty()",
            "白名单还在，但**不再对违规集合下断言** —— 大概率被掏空了",
        ),
    ] {
        assert!(
            guard.contains(needle),
            "`readonly_guard` 那条白名单：{why}（找 `{needle}`）。\n\
                 ★ 本模块只剩下面那张**黑名单**，而它挡不住没列进去的写 API\n\
                 （08-07 实测：`fs::remove_file` 在本条下是绿的）。\n\
                 要么把那条白名单找回来/补全，要么本条改成默认拒绝，别让它绿着。"
        );
    }
    let src = prod_code();
    for pat in [
        format!("fs::{}", "write"),
        format!("fs::{}", "create_dir"),
        format!("File::{}", "create"),
        format!("{}::new", "OpenOptions"),
    ] {
        assert!(!src.contains(&pat), "本模块不许有文件系统写：{pat}");
    }
    assert!(src.contains("fn notify"), "剥注释后代码为空，断言在空转");
}

/// starttime 对不上 ⇒ 什么都不做且退 0（不误伤被复用了 pid 的无关进程）。
#[test]
fn notify_rejects_mismatched_starttime() {
    let me = std::process::id();
    let args = vec![
        "--tmux-notify".to_string(),
        me.to_string(),
        "999999999".to_string(), // 几乎不可能等于真实 starttime
    ];
    assert_eq!(notify(&args), 0);
}

#[test]
fn notify_rejects_bad_args() {
    assert_eq!(notify(&["--tmux-notify".into()]), 2);
    assert_eq!(notify(&["--tmux-notify".into(), "x".into(), "1".into()]), 2);
}
