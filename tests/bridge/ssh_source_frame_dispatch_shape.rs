/// ★ **daemon 帧的消费分派不许有兜底臂**〔audit-0805 08-06〕。
///
/// # 它钉的是「谁是被偶然守住的」那一类（第三例）
///
/// `stream_loop` 里那条 `match frame` 今天用**九条具名臂**盖住 `InboundFrame`
/// 的全部 10 个变体（`Reply` 与 `Cancelled` 合用一条）＋ 一条 `None`，
/// **没有兜底臂** ⇒ daemon 新加一种帧、monitor 忘了处理时**编译失败**。
///
/// ⚠ 那是个没人盯的前提：谁加一条兜底臂，穷尽性当场消失，
/// 新帧从此被**静默丢弃** —— 而 monitor 侧既有判据一条都不会因此变红
/// （它们各测各的帧）。后果不是报错，是**功能默默不生效**：
/// 用户看到的是「daemon 明明发了，界面没反应」。
/// ★ 赌注比前两例高：这条流**同时被仓外的 aterm 消费**（承接 D6：
/// 暴露给第三方 = 契约冻结成本），而 monitor 是它的参考实现。
///
/// 与 `watcher.rs`（daemon 七路信号）、`config_surface.rs`（审计页解析形态）同型。
#[test]
fn the_daemon_frame_dispatch_has_no_catch_all_arm() {
    // ⚠ **不能按首个 cfg-test 切**：本文件有十几个测试模块，第一个在 915 行，
    //   而要守的那条分派在 3552 行 —— 第一版就是这么写的，
    //   抽取器自检当场报「只扫到 0 条臂」。用共享原语剥全部测试段。
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    let lines: Vec<&str> = prod.lines().collect();
    let ind = |l: &str| l.len() - l.trim_start().len();
    let mut arms = 0usize;
    let mut offenders: Vec<usize> = Vec::new();
    for (n, l) in lines.iter().enumerate() {
        let s = l.trim_start();
        if s.starts_with("Some(InboundFrame::") || s.starts_with("Some(f @ (InboundFrame::") {
            arms += 1;
            continue;
        }
        if !s.starts_with("_ =>") {
            continue;
        }
        let d = ind(l);
        for k in (0..n).rev() {
            let prev = lines[k];
            if prev.trim().is_empty() {
                continue;
            }
            if ind(prev) < d {
                break;
            }
            if ind(prev) == d && prev.trim_start().starts_with("Some(InboundFrame::") {
                offenders.push(n + 1);
                break;
            }
        }
    }
    assert!(
        arms >= 8,
        "只扫到 {arms} 条 `Some(InboundFrame::` 臂（08-06 实测 9）—— 抽取坏了，本条此刻是空转的"
    );
    // 豁免登记（**默认拒绝**：不在这张表里的兜底臂一律红）。按**函数名**登记，不按行号。
    const ALLOWED: &[(&str, &str)] = &[(
        "route_inbound_frame",
        "它上面紧挨着另一条 match，已用具名臂 `other =>` 把非入方向帧拦下并 `warn!` \
             报了身份（E4）。到这条 match 时只可能是 Reply/Cancelled，兜底臂不可达，不吞新帧。",
    )];
    let fn_of = |line_no: usize| -> String {
        lines[..line_no.min(lines.len())]
            .iter()
            .rev()
            .find_map(|l| {
                let s = l.trim_start();
                s.strip_prefix("fn ")
                    .or_else(|| s.strip_prefix("pub fn "))
                    .or_else(|| s.strip_prefix("async fn "))
                    .or_else(|| s.strip_prefix("pub async fn "))
                    .map(|r| {
                        r.chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect::<String>()
                    })
            })
            .unwrap_or_default()
    };
    let offenders: Vec<usize> = offenders
        .into_iter()
        .filter(|n| !ALLOWED.iter().any(|(f, _)| *f == fn_of(*n)))
        .collect();
    for (f, _) in ALLOWED {
        assert!(
            prod.contains(&format!("fn {f}(")),
            "豁免表里的 `{f}` 已经不在生产段里 —— 登记该删了"
        );
    }
    assert!(
        offenders.is_empty(),
        "daemon 帧的消费分派里出现了兜底臂（生产段第 {offenders:?} 行）。\n\
             ⚠ 后果不是报错，是**新帧被静默丢弃** —— 用户看到的是\n\
             「daemon 明明发了，界面没反应」，而既有判据一条都不会红。\n\
             这条流同时被仓外 aterm 消费（D6：契约冻结成本）。\n\
             新增帧请写成具名臂；确实不处理也请显式写出来并加一句为什么。"
    );
}
