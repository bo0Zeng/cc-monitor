//! ★〔audit-0805 F05 下半，报告 §4.3〕**冷启动最贵的三段必须各有一条 `[perf]`**。
//!
//! 实测（08-06）：`[perf]` 在前端 13 处、monitor Rust 14 处，而**本文件 0 处** ——
//! 偏偏它是「点开应用 → 看见远端会话」之间唯一的那条链。
//! 后果不是「不知道快慢」：报告 §4.3 里那些 50-200ms 的数字**是外部常识值不是本仓证据**，
//! 那条诚实边界就挂在这上面。
//!
//! ⚠ 本条**只钉「埋点在不在、在不在对的函数里」**，钉不了「量出来的数对不对」——
//! 那要真 SSH（红线禁）。**别把它读成性能判据。**

/// `(阶段名, 必须出现在哪个函数体里, **完整** needle)`。
///
/// ⚠ needle **存完整串、不用前缀拼**：第一版是 `format!("… {phase}")`，
/// 而「起流」是「起流程」的**前缀** —— 把埋点改名成「起流程」时判据**照样绿**（变异实测）。
/// 前缀匹配在中文短词上尤其危险。
const PHASES: &[(&str, &str, &str)] = &[
    (
        "部署预检",
        "async fn preflight",
        "[perf] stream_source [{host_label}] 部署预检 {}ms（",
    ),
    (
        "起流",
        // 重连循环每接一次都经 `DeskLink::stream` 开那条链路（SSH 登录 ＋ exec 小中继），开完报一行。
        "fn perf_dialed",
        "[perf] stream_source [{host_label}] 起流 {}ms（",
    ),
    (
        "首个 hello",
        "fn on_hello",
        "[perf] stream_source [{host_label}] 首个 hello T+{}ms（",
    ),
    // 握手那一段搬进了拨号代理，量它的埋点跟着搬到宿主 `dial_host.rs::open`
    //   （原住 `stream_source/` 里的 `connect_session`；SFTP 那一份也随 `inproc_dial.rs` 整份删了，握手今天只在本机后端 `ssh-dial/connect.rs`）。
    //   ⚠ 量的东西多了一次**起代理进程** —— 那是认下来的代价，埋点让它看得见。
    (
        "SSH 握手+鉴权",
        "async fn open",
        "[perf] stream_source [{origin}] SSH 握手+鉴权 {}ms（",
    ),
];

/// 埋点住两份文件：流循环在 `stream_source/`，握手在拨号代理的宿主 `dial_host.rs`。
/// 阶段表每一行按「它在哪个函数里」去对应的那一份里找。
fn source_of(sig: &str) -> String {
    if sig == "async fn open" {
        include_str!("../../../../src/frontend/shell/src/dial_host.rs").to_string()
    } else {
        guard_core::addr::stream_source_raw()
    }
}

/// 抠出某个函数的函数体。
///
/// ⚠ **锚点必须带换行与左括号**（`\n{sig}(`）：不带的话 `src.find(sig)` 会先命中
/// **本模块 `PHASES` 表里的那个字符串字面量**，于是抽出来的是守卫自己的函数体 ——
/// 实测第一次跑就栽在这里，诊断说「流循环里没有埋点」而埋点明明在。
/// ★ 这是本区**第四次**「判据匹配到自己」（F12 注释 · F13 `RULE` 常量 ·
/// F18 自己的登记表 · 本条自己的阶段表）。**判据要读的东西和判据本身写在同一片文本里时，
/// 锚点必须能把两者分开。**
fn body_of(src: &str, sig: &str) -> String {
    let anchored = format!("\n{sig}(");
    let at = src.find(&anchored).unwrap_or_else(|| {
        panic!("找不到函数定义 `{anchored:?}` —— 它被改写或搬走了，本条会零命中地绿")
    });
    let rest = &src[at..];
    // 到下一个顶层 `\n}\n` 为止：本仓 rustfmt 风格下这就是函数结尾。
    let end = rest.find("\n}\n").unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn the_three_most_expensive_cold_start_phases_each_emit_a_perf_line() {
    // 抽取器坏了 ⇒ 下面每一相的函数都找不到、`body_of` 当场红（不靠字节数地板）。
    let src = guard_core::addr::stream_source_raw();
    for (phase, sig, needle) in PHASES {
        let body = body_of(&source_of(sig), sig);
        assert!(
            body.contains(needle),
            "冷启动阶段「{phase}」在 `{sig}` 里没有 `[perf]` 埋点（找的是 `{needle}`）。\n\
                 ★ 报告 §4.3 说的就是这件事：**最贵的三段恰好一个埋点都没有**，\n\
                 而前端与 Rust 别处都有完整分段埋点（08-06 实测 13 + 14 处）。\n\
                 没有它，「冷启动三连接合并省了多少」这句话永远只能靠推 ——\n\
那条诚实边界（那些 50-200ms 是外部常识值）就挂在这上面。"
        );
    }
}

/// 本文件的 `[perf]` 条数不许降（**递减棘轮**：埋点只许多不许少）。
/// 「本文件」= `stream_source/` ＋ 握手那一段搬去的 `dial_host.rs`（两份合起来数）。
#[test]
fn the_perf_probes_in_this_file_only_grow() {
    let src = [
        guard_core::addr::stream_source_raw(),
        include_str!("../../../../src/frontend/shell/src/dial_host.rs").to_string(),
    ]
    .concat();
    // ⚠ 只数**发射点所在的那一行**（trim 后以那个字面量开头）——
    // 本模块自己的源码里也有这串（`needle` 的 `format!`、以及这行 `starts_with` 的参数），
    // 直接 `src.matches(...)` 会**数到自己**：实测数到 6 而真实发射点只有 4。
    // ★ 本区**第五次**「判据匹配到自己」，同一轮里第二次
    //（前一次是 `body_of` 的锚点命中了 `PHASES` 表里的字符串）。
    let n = src
        .lines()
        .filter(|l| l.trim_start().starts_with("\"[perf] stream_source [{"))
        .count();
    assert!(
        n >= 4,
        "本文件只剩 {n} 条 `[perf]` 发射点（08-06 埋下 4 条：部署预检 / 起流 / 首个 hello / \
             SSH 握手+鉴权）。删埋点前先证明它没用 —— 定框 **E11** 那条「删判据前先证明它恒绿」\
             同样适用：**删之前先说清「这一段现在靠什么知道快慢」**。"
    );
}

/// 界面那一头的启动管线读数：「首批行到前端」那一句（`[perf] first stream batch received`）与整条 timeline（经 `frontend_perf_log`
/// 落进 monitor 日志、带建卡数 `recordsRendered`）要打在**今天冷起真走的那一处**。
///
/// 壳冷起不再整份重放（会话流里没有那一段批）⇒ 原来挂在批边界上的这一句冷起时一次都不打，timeline 也不落盘（合测读不到）。
/// 冷起首个标签页的正文走的是「会话刚进在看名单 ⇒ 按行号把上流点之下那一截取回来」（`TabStreamView.catchUpTo`）：
/// 喂行那一处（`feedHistoryRows`）记首批、那一趟落地处落 timeline。行为（只打一次 · 建卡数照读）住 `startup-perf.vitest.ts` 与
/// `tabs.vitest.ts`「冷起那一形」；本条只钉「那一句只有一处发」「冷起那条路调到了它」。
#[test]
fn the_ui_first_batch_line_is_emitted_on_todays_cold_start_path() {
    let perf = include_str!("../../../../src/frontend/ui/startup-perf.ts");
    let view = include_str!("../../../../src/frontend/ui/tab-stream-view.ts");
    let events = include_str!("../../../../src/frontend/ui/events.ts");
    let needle = "console.info(`[perf] first stream batch received @ ";
    assert_eq!(
        perf.matches(needle).count(),
        1,
        "「首批行到前端」那一句该恰好一处发（`startup-perf.ts::noteFirstBatch`）"
    );
    assert!(
        !events.contains(needle) && !view.contains(needle),
        "那一句又在别处另打了一份 —— 两条来路（重放批 · 冷起补读）都经 `noteFirstBatch`"
    );
    let method = |sig: &str| -> String {
        let at = view.find(sig).unwrap_or_else(|| {
            panic!("`tab-stream-view.ts` 里找不到 `{sig}` —— 被改写或搬走了，本条会零命中地绿")
        });
        let rest = &view[at..];
        rest[..rest.find("\n  }\n").unwrap_or(rest.len())].to_string()
    };
    assert!(
        method("  private feedHistoryRows(").contains("noteHistoryArrived();"),
        "按行号取回的行喂进 tab 那一处（`feedHistoryRows`）没记首批"
    );
    assert!(
        method("  catchUpTo(").contains("noteColdFilled();"),
        "冷起那一趟（`catchUpTo` 整份取尾巴那一形）落地时没把 timeline 落盘"
    );
    assert!(
        events.contains("noteFirstBatch();"),
        "重放批那一条来路（会话流里的 `batch` 边界）不再记首批"
    );
}
