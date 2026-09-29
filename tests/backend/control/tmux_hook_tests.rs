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
/// 与 `tests/frontend/ui/paste-block-guard.vitest.ts` 同一处置（那边也是「把注释当代码」栽过）。
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
        63,
        &PathBuf::from("/opt/ccm/backend"),
        42,
        999,
    );
    assert_eq!(a[0], "set-hook");
    assert_eq!(a[1], "-g");
    assert_eq!(a[2], "session-closed[63]");
    assert_eq!(
        a[3],
        "run-shell -b ''\\''/opt/ccm/backend'\\'' -- --tmux-notify 42 999'"
    );
}

/// exe 路径含单引号也不能破坏命令串（我们自己产生的路径，但用户可以把 backend
/// 部署到任意目录）。
#[test]
fn exe_path_with_quote_is_escaped() {
    let a = hook_set_args("session-closed", 50, &PathBuf::from("/o'p/d"), 1, 2);
    assert!(a[3].contains(r"'\''"), "单引号必须转义：{}", a[3]);
}

#[test]
fn unset_args_shape() {
    assert_eq!(
        hook_unset_args("session-renamed", 77),
        vec!["set-hook", "-gu", "session-renamed[77]"]
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

// ═══ 〔HX2 · 主会话 D-b〕hook 槽位按实例一格、起时清死槽 ═══════════════════════════════════
//
// 要求住址：主会话 4D 裁 D-b 逐字「tmux hook 槽位按实例区分、起时清死 pid 的槽」；`INVARIANTS §41.1`（hook → `--tmux-notify` → SIGUSR1）
// 与 `§41.2`（starttime 不符即静默 no-op —— 本族「死槽」的判准是同一条身份校验）。审计 `E-compat.md` §E5。
// 不碰真 tmux server（`C7i`）：执行器是一张内存 hook 表（逐条解释 `set-hook -g` / `-gu` / `show-hooks -g`）。

/// 内存里的 tmux：`(事件, 槽) → 命令`。`hijack` 在第 N 次「装」的调用之后把那一格整格改成别人（撞格那一形：
/// 另一个后端的那一串三条在我们那一串之后跑完）。
struct FakeTmux {
    hooks: std::collections::BTreeMap<(String, u32), String>,
    sets: usize,
    hijack: Option<(usize, String)>,
}

impl FakeTmux {
    fn new() -> Self {
        Self {
            hooks: Default::default(),
            sets: 0,
            hijack: None,
        }
    }
    fn put(&mut self, event: &str, slot: u32, cmd: &str) {
        self.hooks
            .insert((event.to_string(), slot), cmd.to_string());
    }
    fn key(a: &str) -> (String, u32) {
        let (e, rest) = a.split_once('[').expect("没有下标");
        (
            e.to_string(),
            rest.trim_end_matches(']').parse().expect("下标不是数"),
        )
    }
    /// 一次调用可以是 `;` 分开的一串命令（同 tmux：一串在它自己的队列里一口气跑完）；`hijack` 数的是**调用**次数。
    fn run(&mut self, args: &[String]) -> Result<(bool, String), String> {
        let mut out = String::new();
        let mut set_slot = None;
        for cmd in args.split(|a| a == ";") {
            let a: Vec<&str> = cmd.iter().map(String::as_str).collect();
            match a.as_slice() {
                ["show-hooks", "-g", event] => {
                    out.push_str(&format!("{event}\n"));
                    for ((e, slot), c) in &self.hooks {
                        if e == event {
                            // 同 tmux 3.6 的回话形状：载荷被它重新加一层双引号。
                            out.push_str(&format!(
                                "{e}[{slot}] {}\n",
                                c.replacen("run-shell -b '", "run-shell -b \"", 1)
                            ));
                        }
                    }
                }
                ["set-hook", "-g", k, c] => {
                    let key = Self::key(k);
                    set_slot = Some(key.1);
                    self.hooks.insert(key, c.to_string());
                }
                ["set-hook", "-gu", k] => {
                    self.hooks.remove(&Self::key(k));
                }
                other => return Err(format!("假 tmux 不认：{other:?}")),
            }
        }
        if let Some(slot) = set_slot {
            self.sets += 1;
            if let Some((n, other)) = &self.hijack {
                if self.sets == *n {
                    for ev in HOOK_EVENTS {
                        self.hooks.insert((ev.to_string(), slot), other.clone());
                    }
                }
            }
        }
        Ok((true, out))
    }
}

fn payload(pid: u32, start: u64) -> String {
    format!("run-shell -b ''\\''/opt/ccm/backend'\\'' --tmux-notify {pid} {start}'")
}

/// H1：回话解析 == 手写（带引号的路径 · 用户自己的 hook · 空槽那一行 · 坏行 · 别的事件的行）。
#[test]
fn hx2_show_hooks_output_is_read_into_slots_and_owners() {
    let out = "session-closed\n\
               session-closed[50] run-shell -b \"'/p a/exe' --tmux-notify 123 456\"\n\
               session-closed[51] run-shell -b \"echo hi\"\n\
               session-closed[7] display-message x\n\
               session-closed[x] run-shell -b \"y --tmux-notify 1 2\"\n\
               session-closed[52 garbage\n\
               session-created[53] run-shell -b \"z --tmux-notify 9 9\"\n\
               session-closed[60] run-shell -b \"z --tmux-notify 9\"\n";
    let got = parse_show_hooks("session-closed", out);
    let want: std::collections::BTreeMap<u32, Occupant> = [
        (50, Occupant::Backend(123, 456)),
        (51, Occupant::Foreign),
        (7, Occupant::Foreign),
        (60, Occupant::Foreign),
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
}

/// H2：选格纯函数 —— 空段 ⇒ 首选格；首选格被活的别人占 ⇒ 下一格；自己已在某格 ⇒ 复用；满 ⇒ `None`；
/// 死槽摘、用户的 hook 与段外的一格不碰；自己落在别的格的旧条目摘。
#[test]
fn hx2_the_slot_plan_matches_the_hand_written_cases() {
    let me = (1234u32, 77u64);
    let first = preferred_slot(me.0);
    assert_eq!(first, 50 + 1234 % 50);
    let alive_all = |_: u32, _: u64| true;
    let full = |slot: u32, occ: Occupant| -> Board { (0..3).map(|ev| ((ev, slot), occ)).collect() };

    assert_eq!(
        plan(&Board::new(), me, &alive_all),
        Plan {
            unset: vec![],
            slot: Some(first)
        }
    );

    let other = Occupant::Backend(99, 1);
    let p = plan(&full(first, other), me, &alive_all);
    assert_eq!(
        p,
        Plan {
            unset: vec![],
            slot: Some(first + 1)
        },
        "首选格被活的别人占 ⇒ 下一格"
    );

    let mut b = full(first + 5, Occupant::Backend(me.0, me.1));
    b.insert((1, first), Occupant::Backend(me.0, me.1));
    let p = plan(&b, me, &alive_all);
    assert_eq!(p.slot, Some(first + 5), "三个事件都是自己的那一格 ⇒ 复用");
    assert_eq!(p.unset, vec![(1, first)], "自己落在别的格的旧条目 ⇒ 摘");

    let dead = |p: u32, _: u64| p != 99;
    let mut b = full(first, Occupant::Backend(99, 1));
    b.insert((0, 30), Occupant::Backend(99, 1)); // 段外 ⇒ 不看
    b.insert((2, 70), Occupant::Foreign); // 用户的 ⇒ 不碰
    let p = plan(&b, me, &dead);
    assert_eq!(
        p.unset,
        vec![(0, first), (1, first), (2, first)],
        "死槽 ⇒ 摘（段外、用户的不碰）"
    );
    assert_eq!(p.slot, Some(first), "摘掉死槽之后首选格就空了");

    let mut b = Board::new();
    for k in 0..HOOK_SLOT_COUNT {
        b.insert((0, HOOK_SLOT_BASE + k), Occupant::Foreign);
    }
    assert_eq!(plan(&b, me, &alive_all).slot, None, "满 ⇒ 不装");
}

/// 🔴 H3：真装一趟（内存 tmux × **真 `/proc` 身份**）：预置一个死槽（本进程 pid ＋ 错的 starttime）、一个活的别人
/// （本进程 pid ＋ 真 starttime）、一条用户 hook、一条段外的 ccm 死条目 ⇒ 死槽没了、活的别人 / 用户 hook / 段外那条逐字不动、
/// 自己三条在同一格。
#[cfg(target_os = "linux")] // 〔HX2〕读真 `/proc` 的 starttime（非 Linux 上 `proc_starttime` 恒 `None`）
#[test]
fn hx2_install_clears_dead_slots_and_takes_one_slot_without_touching_the_living() {
    let pid = std::process::id();
    let real = crate::platform::proc::proc_starttime(pid)
        .expect("本进程的 starttime 读不到（非 Linux？）");
    let me = (4_000_000u32, 5u64);
    let mut t = FakeTmux::new();
    let dead_slot = preferred_slot(me.0);
    let live_slot = HOOK_SLOT_BASE + (dead_slot - HOOK_SLOT_BASE + 1) % HOOK_SLOT_COUNT;
    for ev in HOOK_EVENTS {
        t.put(ev, dead_slot, &payload(pid, real + 1));
        t.put(ev, live_slot, &payload(pid, real));
    }
    t.put("session-closed", 99, "run-shell -b 'echo user'");
    t.put("session-closed", 20, &payload(pid, real + 1));
    let before_live: Vec<String> = HOOK_EVENTS
        .iter()
        .map(|e| t.hooks[&(e.to_string(), live_slot)].clone())
        .collect();
    let n = install_hooks_with(
        &mut |a: &[String]| t.run(a),
        &PathBuf::from("/opt/ccm/backend"),
        me.0,
        me.1,
        &backend_alive,
    );
    assert_eq!(n, 3);
    for (i, ev) in HOOK_EVENTS.iter().enumerate() {
        assert_eq!(
            t.hooks[&(ev.to_string(), live_slot)],
            before_live[i],
            "活的别人被动了"
        );
        assert_eq!(
            t.hooks
                .get(&(ev.to_string(), dead_slot))
                .map(|c| occupant_of_test(c)),
            Some(Occupant::Backend(me.0, me.1)),
            "死槽没被清掉、或自己没装进那一格"
        );
    }
    assert_eq!(
        t.hooks[&("session-closed".to_string(), 99)],
        "run-shell -b 'echo user'"
    );
    assert!(
        t.hooks.contains_key(&("session-closed".to_string(), 20)),
        "段外那一条被动了"
    );
    let mine = t
        .hooks
        .values()
        .filter(|c| c.contains(&format!("--tmux-notify {} {}", me.0, me.1)))
        .count();
    assert_eq!(mine, 3, "自己恰好三条");
}

fn occupant_of_test(cmd: &str) -> Occupant {
    *parse_show_hooks("e", &format!("e[1] {cmd}\n"))
        .get(&1)
        .expect("解不出")
}

/// 🔴 H4：撞格 —— 装完那一串之后那一格被另一个（活的）后端整格占了 ⇒ 装完那一读看出来、换下一格重装；别人那一格不动。
/// 三条是**一次**调用（`;` 分开）⇒ 不会出现半格是我半格是它。
#[test]
fn hx2_a_slot_lost_to_a_concurrent_backend_is_retried_in_the_next_one() {
    let me = (4_000_001u32, 5u64);
    let first = preferred_slot(me.0);
    let other = payload(4_000_002, 9);
    let mut t = FakeTmux::new();
    t.hijack = Some((1, other.clone()));
    let alive = |p: u32, _: u64| p == 4_000_002;
    let n = install_hooks_with(
        &mut |a: &[String]| t.run(a),
        &PathBuf::from("/x"),
        me.0,
        me.1,
        &alive,
    );
    assert_eq!(n, 3);
    let next = HOOK_SLOT_BASE + (first - HOOK_SLOT_BASE + 1) % HOOK_SLOT_COUNT;
    for ev in HOOK_EVENTS {
        assert_eq!(
            t.hooks[&(ev.to_string(), first)],
            other,
            "别人抢到的那一格被动了"
        );
        assert!(
            t.hooks[&(ev.to_string(), next)].contains(&format!("--tmux-notify {} {}", me.0, me.1)),
            "没换到下一格"
        );
    }
}

/// 读数（`#[ignore]`，手动跑）：**真 tmux**、私有 socket（`-L`，不碰用户那台 server，`C7i`）上走一趟：
/// 预置一个死槽（本进程 pid ＋ 错的 starttime，放在首选格）与一条用户 hook ⇒ 装完死槽换成自己、用户 hook 原样、
/// `show-hooks` 回话里自己恰好三条同一格。跑法：`cargo test --lib hx2_real_tmux -- --ignored`。
#[cfg(target_os = "linux")] // 〔HX2〕读真 `/proc` 的 starttime（非 Linux 上 `proc_starttime` 恒 `None`）
#[test]
#[ignore]
fn hx2_real_tmux_reading_on_a_private_socket() {
    let sock = format!("ccm-hx2-{}", std::process::id());
    let tmux = |args: &[&str]| {
        std::process::Command::new("tmux")
            .args(["-L", sock.as_str(), "-u"])
            .args(args)
            .output()
            .expect("起不来 tmux")
    };
    assert!(tmux(&["new-session", "-d", "-s", "hx2", "sleep 30"])
        .status
        .success());
    let pid = std::process::id();
    let real = crate::platform::proc::proc_starttime(pid).unwrap();
    let me = (4_000_003u32, 1u64);
    let first = preferred_slot(me.0);
    let dead = format!("run-shell -b '/x --tmux-notify {pid} {}'", real + 1);
    for ev in HOOK_EVENTS {
        assert!(tmux(&["set-hook", "-g", &format!("{ev}[{first}]"), &dead])
            .status
            .success());
    }
    assert!(tmux(&[
        "set-hook",
        "-g",
        "session-closed[70]",
        "run-shell -b 'echo user'"
    ])
    .status
    .success());
    let mut run = |a: &[String]| -> Result<(bool, String), String> {
        let mut v: Vec<&str> = Vec::new();
        v.extend(a.iter().map(String::as_str));
        let o = tmux(&v);
        Ok((
            o.status.success(),
            String::from_utf8_lossy(&o.stdout).into_owned(),
        ))
    };
    let n = install_hooks_with(
        &mut run,
        &PathBuf::from("/opt/ccm/backend"),
        me.0,
        me.1,
        &backend_alive,
    );
    let after: Vec<String> = HOOK_EVENTS
        .iter()
        .map(|ev| String::from_utf8_lossy(&tmux(&["show-hooks", "-g", ev]).stdout).into_owned())
        .collect();
    let _ = tmux(&["kill-server"]);
    assert_eq!(n, 3, "{after:?}");
    for (i, ev) in HOOK_EVENTS.iter().enumerate() {
        let got = parse_show_hooks(ev, &after[i]);
        assert_eq!(
            got.get(&first),
            Some(&Occupant::Backend(me.0, me.1)),
            "{ev}: {}",
            after[i]
        );
    }
    assert_eq!(
        parse_show_hooks("session-closed", &after[1]).get(&70),
        Some(&Occupant::Foreign)
    );
    eprintln!("真 tmux 读数：{after:#?}");
}
