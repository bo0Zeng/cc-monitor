use super::*;

/// P2s-Y1（acceptor: 机检）：**三个口各只有一条命令，且都吃 `origin`**。
///
/// 防的是「本机一套名字、远端另一套名字」—— 那样两侧就长出两套语义，
/// 正是 `C1` 排除掉的做法（也是 #58 门⑤ 要防的形态）。
#[test]
fn the_three_ports_are_one_command_each_and_all_take_origin() {
    let src = guard_core::production_code(include_str!("../../src/bridge/src/daemon_control.rs"));
    const PORTS: &[&str] = &["daemon_status", "daemon_start", "daemon_stop"];
    for p in PORTS {
        let sig = format!("pub fn {p}(origin: String)");
        let at = guard_core::find_pinned(&src, &sig).unwrap_or_else(|e| {
            panic!(
                "`{p}` 不是「恰好一处、且第一个参数是 origin」的形状（{e}）。\n\
                     ★ 三个口必须**各只有一条命令**且都按 origin 分派 —— 本机一套、远端一套\n\
                     就是两套语义（`C1` 明说排除这条路）。"
            )
        });
        // ★★ **逐口切体，不数全局**。
        //
        // 第一版写的是「`is_local(&origin)` 全局出现 >= 2 处」。变异实测：
        // 把 `daemon_stop` 里那一支整个摘掉，**判据照样绿** —— 因为另外两口还各有一处，
        // 计数仍然 >= 2。⇒ 计数型自检在「人群有多个成员」时天然逮不到「某一个成员塌了」。
        let body: String = src[at..]
            .lines()
            .skip(1)
            // ⚠ 收尾行**不写字面量右花括号** —— 本仓有判据用「花括号配平」剥测试段
            // （`ssh_source::strip_cfg_test`），源码里多一个孤立的右花括号会让它**提前闭合**（`b'…'` 的字符字面量也算，我第一次「修」时就还带着一个），
            // 测试段整段泄漏进「生产段」⇒ 别的判据当场误报（08-11 实测：单写者守卫红了）。
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            body.len() > 30,
            "`{p}` 切出来的函数体只有 {} 字节 —— 切错了，本条在空转",
            body.len()
        );
        guard_core::find_pinned(&body, "is_local(&origin)").unwrap_or_else(|e| {
            panic!(
                "`{p}` 里没有恰好一处 `is_local(&origin)` 分派（{e}）——\n\
                     ⇒ 这一口没接上本机那侧（或接了两次），`C1`「本地要和远端一样」在这口上落空。"
            )
        });
    }
}

/// ★★ **两条「说成功其实没成」的形态**〔D 阶段补审 08-11 新增，A2 + A6〕。
///
/// | # | 原形态 | 后果 |
/// |---|---|---|
/// | A2 | 远端「起」的判据是 `slot.handle.is_some()` | `JoinHandle` 完成后**不会变 `None`** ⇒ 流早就结束了，每次「起」仍恒回「已经在跑」；用户只能先「停」再「起」 |
/// | A6 | `daemon_start` 把 `Resolved::Missing{reason}` 当 `Ok(reason)` | 「没内嵌 daemon」「释放失败」两种**真失败**在前端只走 `console.info`，**一个 toast 都不弹** |
///
/// 本条钉：① 远端那支必须问 `is_finished()`（不是 `is_some()`）；
/// ② 本机那支必须有 `Failed` ⇒ `Err` 那一格（三态各有去处，不靠 `reason` 字符串猜）。
#[test]
fn starting_reports_failure_as_failure_and_finished_streams_as_not_running() {
    let src = guard_core::production_code(include_str!("../../src/bridge/src/daemon_control.rs"));
    let at = guard_core::find_pinned(&src, "pub fn daemon_start(origin: String)")
        .expect("起口不在了");
    let body: String = src[at..]
        .lines()
        .skip(1)
        .take_while(|l| *l != "\u{7d}")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        body.len() > 200,
        "切出来的体只有 {} 字节 —— 切错了",
        body.len()
    );

    guard_core::find_pinned(&body, "is_finished()").unwrap_or_else(|e| {
        panic!(
            "远端「起」不再问 `is_finished()`（{e}）。\n\
                 若换回 `handle.is_some()`：`JoinHandle` 完成后不会变 `None`\n\
                 ⇒ 流早就结束了，「起」仍恒回「已经在跑」，用户只能先「停」再「起」。"
        )
    });
    guard_core::find_pinned(&body, "StartOutcome::Failed").unwrap_or_else(|e| {
        panic!(
            "本机「起」没有把 `Failed` 单独接出来（{e}）。\n\
                 三种结局（起了 / 已经在跑 / 起不来）必须各有去处 ——\n\
                 全塞进 `Ok(reason)` 的话，真失败在前端只走 `console.info`，一个 toast 都不弹。"
        )
    });
    assert!(
        body.contains("Err(format!"),
        "本机「起」的失败那一格不回 `Err` —— 那就是把失败说成了成功。"
    );
}

/// ★ **接线钉（远端那半）**：`lib.rs` 起每台远端时**真的**把把手注册进来。
///
/// 不注册的后果**不是编译错，是运行时一句「没有把手」** —— 而本模块的单测全都照样绿
/// （它们不需要真把手）。这正是 F03 那个坑：「模块存在 ≠ 模块被调用」。
///
/// ⚠ 还要钉「把手不是被丢掉的」：原来那处逐字是 `tauri::async_runtime::spawn(async move {…});`
/// —— **JoinHandle 直接丢**，于是远端流起了就再也停不下来。
#[test]
fn the_startup_path_really_registers_remote_handles() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/lib.rs"));
    guard_core::find_pinned(&prod, "daemon_control::register_remote(").unwrap_or_else(|e| {
        panic!(
            "`lib.rs` 的生产段里没有恰好一处 `register_remote(`（{e}）。\n\
                 ⇒ 远端那侧的起/停在运行时只会回一句「没有这台机的把手」，\n\
                 而本模块的单测**全都照样绿**（它们不需要真把手）。"
        )
    });
}

#[test]
fn an_empty_origin_is_refused_by_every_port() {
    for r in [
        daemon_status("  ".into()).map(|_| ()),
        daemon_start(" ".into()).map(|_| ()),
        daemon_stop("".into()).map(|_| ()),
    ] {
        assert!(
            r.is_err(),
            "空 origin 被放过了 —— 那会造出一档谁都读不到的「全局」，设了没反应且不报错"
        );
    }
}

#[test]
fn an_unknown_remote_origin_says_so_instead_of_pretending() {
    let e = daemon_stop("从没注册过的机器".into()).unwrap_err();
    assert!(
        e.contains("没有这台机的把手"),
        "对不认识的 origin 应当明说没有把手，而不是返回一句像成功的话：{e}"
    );
}
