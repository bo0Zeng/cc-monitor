//! F74c（#60-A）：tmux 存活对账——「带外（`Ctrl-b &` / `tmux kill-session` 等）杀掉某会话的
//! tmux 后端 → 对应 tab 在有界时间内变灰」。
//!
//! **背景**：`remote_active`（`lib.rs`）只由 daemon 的 pidfile 事件（SessionAdded/Removed）驱动，
//! daemon 从不看 tmux。带外杀 tmux 时 claude 进程可能仍被守护托管而不死 → daemon 一直判活 →
//! tab 恒 live，直到断连 flush 才批量清（issue #60-A）。本模块补一条**独立的 tmux 存活信号**。
//!
//! **架构定死（设计 agent 2026-07-17）**：cc-monitor 侧低频 poller（非 daemon 侧、非前端驱动）——
//! 零 daemon/wire 改动、可 CI 单测、F90 可整段 lift。
//! - **§24 单写者不破**：poller 绝不碰 `remote_active`、绝不让前端直接 archive；只把 retire 的 sid
//!   当 `SessionChange{removed}` 送进 `remote_tx` 的一个 clone，由唯一写者 `remote-session-emitter`
//!   照常处理（forget + emit SESSION_ENDED → 前端 archiveTab）。断连 flush 早已是这样的第二生产者。
//! - **source-agnostic**：`reconcile_step` 的「某后端自报正在跑的 sid 集」是裸 `HashSet`（**不叫 tmux_***），
//!   F90 lift 时把来源从 `parse_tmux_ls` 的 `@ccm_sid` 换成 daemon `session.status()` RPC，函数不变
//!   （守 INVARIANT §30「别固化成只有 tmux 能这样」）。
//!
//! **防误判**（假阳性）：① `ever_bound` 门——只对「曾在后端自报里出现过」的 sid 判 retire，
//! never-bound（bg / 无 wrapper / 直起 claude）永不误判；② debounce——连续缺失够 `RETIRE_MISS_THRESHOLD`
//! 轮才 retire（挡 wrapper 刚起 `@ccm_sid` 未写的窗口）；③ `/branch` 漂移——daemon 先退旧 sid 宣告新
//! sid，旧 sid 离开 `announced_live` 即被剔除追踪（不会误 retire 一个还活着只是换了 sid 的会话）；
//! ④ 观测无效门——ssh 抖动（`Err`）/ 无 tmux（`Ok(None)`）跳过本轮不累计缺失（在 poller 里，不进纯函数）。
//!
//! **真机累积项**（Linux/CI 测不了 daemon 活性时序）：带外杀端到端变灰、60-A 触发点确认、
//! `POLL_INTERVAL`/`RETIRE_MISS_THRESHOLD` 标定（现为占位常量）。

use std::collections::{HashMap, HashSet};

/// 连续缺失多少轮（帧）才 retire（debounce，真机标定项）。灰延迟 ≈ daemon 推帧间隔 × 本值。
pub const RETIRE_MISS_THRESHOLD: u32 = 2;
/// ★ 承重下限：threshold **必须 ≥ 2**——`/branch` 漂移有 ~1s 竞态窗（daemon 退旧 sid A 晚一拍：
/// 某轮 A 仍在 `announced_live` 但 backend 已是新 sid B）。threshold=1 会在这单轮把还活着、只是
/// 换了 sid 的会话 A 误 retire。真机标定绝不能调到 1——编译期兜死（改小于 2 直接编译失败）。
const _: () = assert!(RETIRE_MISS_THRESHOLD >= 2);
// B2 起：对账读 daemon 帧推来的 in-memory tmux 状态（`snapshot_tmux_by_origin`），不再 SSH——
// 故去掉原 `LIST_TIMEOUT`（SSH 超时保护已无对象）。

/// 单个 sid 的对账追踪。
#[derive(Default, Debug, Clone, PartialEq)]
struct SidTrack {
    /// 曾在某后端自报里出现过（= wrapper-in-tmux 会话）。只对这类做 retire 判定。
    ever_bound: bool,
    /// 连续缺失轮数。
    miss: u32,
    /// 已 retire（幂等：不重复 emit removed）。
    retired: bool,
}

/// 每 origin 一份对账状态（跨轮累计缺失计数）。
#[derive(Default)]
pub struct ReconcileState {
    per_sid: HashMap<String, SidTrack>,
}

/// 纯决策：给定「daemon 仍宣告活跃的 sid 集」+「某后端自报正在跑的 sid 集」+ debounce 阈值，
/// 返回本轮应 retire（当 removed 冲进 emitter）的 sid。**source-agnostic、可 CI 单测、F90 可 lift。**
///
/// 只对「曾自报过（`ever_bound`）、现从所有后端消失、连续够 `retire_threshold` 轮」的 sid retire。
/// never-bound 永不判（bg/无 wrapper 免疫）；漂移靠「sid 离开 `announced_live` 即剔除追踪」兜；
/// ssh 抖动/无 tmux 由调用方跳过本轮、不进本函数（观测无效不累计缺失）。
///
/// `pre_bound`：带外已证明绑过后端的 sid（idle-tmux——其 `@ccm_sid` 在帧里=铁证绑过 tmux），直接
/// 播种 `ever_bound`，免跨线程竞态漏置（详见函数体注释）。非 idle 场景传空集即退化为原语义。
pub fn reconcile_step(
    state: &mut ReconcileState,
    announced_live: &HashSet<String>,
    backend_reported_sids: &HashSet<String>,
    pre_bound: &HashSet<String>,
    retire_threshold: u32,
) -> Vec<String> {
    // 1. 剔除已不在 announced_live 的追踪（会话经 daemon 正常结束/漂移，清陈旧账）。
    state.per_sid.retain(|sid, _| announced_live.contains(sid));
    let mut retire = Vec::new();
    // 2. 逐个宣告活跃 sid 更新追踪。
    for sid in announced_live {
        let t = state.per_sid.entry(sid.clone()).or_default();
        // audit-fixes F03.2（D 审计②修）：`pre_bound` = 带外已证明绑过后端的 sid（idle-tmux：其
        // `@ccm_sid` 仍在帧里=铁证绑过 tmux）→ 直接播种 `ever_bound`。否则有个窄竞态卡灰：daemon
        // `SessionRemoved(S)` 先把 S 从 `announced` 删掉，与 emitter 随后 `mark_idle(S)` 之间有跨线程
        // 缝；若恰有一帧落在缝里，该帧 `tracked=announced∪idle` 两边都无 S → `ever_bound` 漏置；此后
        // S 只经 idle 进 tracked、backend 再不含它（tmux 已死）→ 永走 never-bound 分支、不累计缺失 →
        // 该连接内永久卡灰关不掉。idle sid 按定义绑过，播种即消除此缝（断连 flush 只是兜底、非首选）。
        if pre_bound.contains(sid) {
            t.ever_bound = true;
        }
        if backend_reported_sids.contains(sid) {
            // 后端自报在跑 → 绑定/复绑，清缺失、清 retired（复活重新计）。
            t.ever_bound = true;
            t.miss = 0;
            t.retired = false;
        } else if t.ever_bound && !t.retired {
            // 曾绑定、现缺失 → 累计缺失；够阈值则 retire（幂等：置 retired 不再重发）。
            t.miss += 1;
            if t.miss >= retire_threshold {
                t.retired = true;
                retire.push(sid.clone());
            }
        }
        // else：从未自报过（never_bound）→ 不判（bg/无 wrapper/直起 claude 免疫误 retire）。
    }
    retire
}

// audit-fixes F03.2：原 `run_tmux_reconcile_poller`（8s 轮询）已删——收割逻辑改为**收帧驱动**，
// 调用点落在 `ssh_source::stream_loop` 的 `TmuxSessions` 帧臂（daemon 每 ~8s 推帧即算，cc-monitor
// 侧零轮询）。空 backend / `NO_TMUX` 守卫、per-连接 state、tracked=announced∪idle 均在该调用点。
// 本模块只保留**纯决策** `reconcile_step` + `ReconcileState`（source-agnostic、可 CI 单测、F90 可 lift）。

#[cfg(test)]
mod tests {
    use super::*;

    fn hs(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn never_bound_sid_never_retires() {
        // 从未在后端自报里出现（bg/无 wrapper）→ 缺失多少轮都不 retire。
        let mut st = ReconcileState::default();
        let announced = hs(&["bg1"]);
        let backend = hs(&[]); // 后端一直不报它
        for _ in 0..10 {
            assert!(reconcile_step(&mut st, &announced, &backend, &hs(&[]), 2).is_empty());
        }
    }

    #[test]
    fn bound_then_missing_retires_at_threshold_once() {
        let mut st = ReconcileState::default();
        let announced = hs(&["s"]);
        // 第 1 轮：后端自报在跑 → ever_bound。
        assert!(reconcile_step(&mut st, &announced, &hs(&["s"]), &hs(&[]), 2).is_empty());
        // 后端消失：第 1 次缺失（miss=1 < 2）→ 不 retire。
        assert!(reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 2).is_empty());
        // 第 2 次缺失（miss=2 >= 2）→ retire 一次。
        assert_eq!(
            reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 2),
            vec!["s".to_string()]
        );
    }

    #[test]
    fn retire_is_idempotent() {
        let mut st = ReconcileState::default();
        let announced = hs(&["s"]);
        reconcile_step(&mut st, &announced, &hs(&["s"]), &hs(&[]), 1); // bound
        assert_eq!(
            reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 1),
            vec!["s".to_string()]
        );
        // 之后继续缺失也不重复 emit（retired 已置）。
        assert!(reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 1).is_empty());
        assert!(reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 1).is_empty());
    }

    #[test]
    fn rebind_resets_miss() {
        let mut st = ReconcileState::default();
        let announced = hs(&["s"]);
        reconcile_step(&mut st, &announced, &hs(&["s"]), &hs(&[]), 3); // bound
        reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 3); // miss=1
        reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 3); // miss=2
        reconcile_step(&mut st, &announced, &hs(&["s"]), &hs(&[]), 3); // 复绑 → miss=0
                                                                       // 再缺 2 轮仍不到阈值 3。
        assert!(reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 3).is_empty());
        assert!(reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 3).is_empty());
    }

    #[test]
    fn sid_leaving_announced_drops_tracking() {
        let mut st = ReconcileState::default();
        reconcile_step(&mut st, &hs(&["s"]), &hs(&["s"]), &hs(&[]), 1); // bound
        reconcile_step(&mut st, &hs(&["s"]), &hs(&[]), &hs(&[]), 1); // retire s（miss>=1）
                                                                     // s 经 daemon 正常结束 → 离开 announced_live → 追踪被剔除。
        reconcile_step(&mut st, &hs(&[]), &hs(&[]), &hs(&[]), 1);
        // s 又出现且后端在跑（复用同名）→ 重新 ever_bound、不带旧 retired 状态。
        assert!(reconcile_step(&mut st, &hs(&["s"]), &hs(&["s"]), &hs(&[]), 1).is_empty());
    }

    #[test]
    fn branch_drift_does_not_retire_old_sid() {
        // /branch：daemon 先退旧 A 宣告新 B。对账里 A 离开 announced_live → 不 retire；B 新绑。
        let mut st = ReconcileState::default();
        reconcile_step(&mut st, &hs(&["A"]), &hs(&["A"]), &hs(&[]), 2); // A bound
                                                                        // 漂移后一轮：announced_live 只剩 B（daemon 已退 A），后端自报 B。
        let retire = reconcile_step(&mut st, &hs(&["B"]), &hs(&["B"]), &hs(&[]), 2);
        assert!(
            retire.is_empty(),
            "A 不该被 retire（它是正常漂移离开、非带外杀）"
        );
    }

    #[test]
    fn branch_drift_lag_round_does_not_retire_with_threshold_2() {
        // 审计发现的滞后竞态：daemon 退旧 sid A 晚一拍——某轮 A 仍 announced、backend 已是 B（A 缺失）。
        // threshold=2 下这单轮 miss=1 < 2 → 不 retire；下一轮 daemon 退 A、A 离开 announced → 剔除。
        // （这正是 threshold ≥ 2 编译期兜死要保的性质。）
        let mut st = ReconcileState::default();
        reconcile_step(&mut st, &hs(&["A"]), &hs(&["A"]), &hs(&[]), 2); // A bound
                                                                        // 滞后 1 轮：A 仍 announced、backend=B → A miss=1 < 2 → 不 retire。
        assert!(reconcile_step(&mut st, &hs(&["A", "B"]), &hs(&["B"]), &hs(&[]), 2).is_empty());
        // 下一轮 daemon 退 A → A 离开 announced_live → 剔除追踪、不 retire。
        assert!(reconcile_step(&mut st, &hs(&["B"]), &hs(&["B"]), &hs(&[]), 2).is_empty());
    }

    #[test]
    fn empty_backend_increments_ever_bound_sid() {
        // 注：纯函数对空 backend 会累计缺失（source-agnostic、F90 期或有更好的空 vs 错区分）；
        // **poller 侧对空 backend 保守跳过**（见 run_tmux_reconcile_poller）——本测只锁纯函数语义。
        let mut st = ReconcileState::default();
        let announced = hs(&["s"]);
        reconcile_step(&mut st, &announced, &hs(&["s"]), &hs(&[]), 5); // ever_bound
                                                                       // 后端空集（tmux 全没了但会话还宣告活）→ 累计缺失。
        for i in 1..5 {
            assert!(
                reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 5).is_empty(),
                "miss={i} 未到阈值 5"
            );
        }
        assert_eq!(
            reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 5),
            vec!["s".to_string()]
        );
    }

    #[test]
    fn threshold_one_retires_immediately_on_first_miss() {
        let mut st = ReconcileState::default();
        let announced = hs(&["s"]);
        reconcile_step(&mut st, &announced, &hs(&["s"]), &hs(&[]), 1); // bound
        assert_eq!(
            reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 1),
            vec!["s".to_string()]
        );
    }

    #[test]
    fn multiple_sids_mixed() {
        // a: bound→missing 到阈值 retire；b: 一直在跑不动；c: never-bound 不判。
        let mut st = ReconcileState::default();
        let announced = hs(&["a", "b", "c"]);
        reconcile_step(&mut st, &announced, &hs(&["a", "b"]), &hs(&[]), 2); // a,b bound; c never
        let retire = reconcile_step(&mut st, &announced, &hs(&["b"]), &hs(&[]), 2); // a miss=1
        assert!(retire.is_empty());
        let mut retire = reconcile_step(&mut st, &announced, &hs(&["b"]), &hs(&[]), 2); // a miss=2 → retire
        retire.sort();
        assert_eq!(retire, vec!["a".to_string()]);
    }

    #[test]
    fn pre_bound_idle_sid_retires_even_if_never_in_backend() {
        // audit-fixes F03.2（D 审计②修 + 覆盖缺口）：复现「卡灰」竞态——idle sid 因跨线程缝从未在
        // 「tracked ∩ backend」的帧里被置 ever_bound（这里 backend 全程不含 s，模拟那帧已过）。
        // pre_bound（idle 集）播种 ever_bound 后，s 仍应累计缺失并在阈值 retire，不会永久卡灰。
        // **变异锚点**：删掉函数体里 `if pre_bound.contains(sid) { t.ever_bound = true; }` 则本测红
        //（s 恒走 never-bound 分支、miss 永不累计、retire 永空）。
        let mut st = ReconcileState::default();
        let announced = hs(&["s"]);
        let idle = hs(&["s"]);
        // backend 从不含 s；s 是 idle（pre_bound）→ 第 1 轮 miss=1 < 2。
        assert!(reconcile_step(&mut st, &announced, &hs(&[]), &idle, 2).is_empty());
        // 第 2 轮 miss=2 >= 2 → retire（tmux 真没了 → 归档，灰关得掉）。
        assert_eq!(
            reconcile_step(&mut st, &announced, &hs(&[]), &idle, 2),
            vec!["s".to_string()]
        );
    }

    #[test]
    fn pre_bound_idle_still_in_backend_does_not_retire() {
        // idle sid 但 tmux 仍在（backend 含它）→ pre_bound 播种 ever_bound、backend 分支清 miss →
        // 不 retire（灰灯继续，正确：claude 死但 tmux 活=可复用）。
        let mut st = ReconcileState::default();
        let announced = hs(&["s"]);
        let idle = hs(&["s"]);
        for _ in 0..5 {
            assert!(reconcile_step(&mut st, &announced, &hs(&["s"]), &idle, 2).is_empty());
        }
    }
}

/// U7b：**把「对账走推送、不许回到轮询」钉住。**
///
/// # 判定：两条 tmux 读路**刻意并存**，不是重复
///
/// U7-5 起初把它归成「旧轮询 vs 新推送，该退役旧的」。**复核后判定不同**：
///
/// | 路径 | 谁在用 | 为什么不能没有 |
/// |---|---|---|
/// | **推送账本**（`tmux_sessions` 帧 → `ssh_source::REMOTE_TMUX_RAW`） | 本模块的**后台对账**（判 idle / 归档） | B2 加它正是为了**替掉每 8s 新建 SSH 的轮询**（治远端 sshd 日志刷屏） |
/// | **按需 exec**（`tmux::list_remote_tmux`） | `tabs.ts` 的 6 个**决策点**（接/起/回退） | 账本**只在该 origin 有 daemon 流连着时才有数据**；未连、daemonless、刚启动都为空。而且决策点要的是**当下这一刻**的权威读，不是推送节奏上的快照 |
///
/// 所以它们不是同一件事的两份实现，是**背景对账**与**决策点取值**两种用途。
/// 硬合只能二选一：要么对账退回轮询（撤销 B2），要么决策点在没有 daemon 时失去数据。
///
/// # 那这条护栏防什么
///
/// 防**轮询回潮** —— 「对账拿不到数据时顺手 exec 一下」是最自然的补丁，
/// 而它会把 B2 治好的那件事原样带回来（每 8s 一条新 SSH，远端 sshd 日志刷屏）。
/// 判据是结构性的：**本模块的生产段里不许出现 `list_remote_tmux`。**
///
/// 已知的混用坑另有守卫：`ssh_source` 的
/// `superseded_always_archives_even_when_tmux_snapshot_still_shows_the_sid`
/// —— `/branch` 原地换 sid 时，那份快照对这个场景**恒错**。
#[cfg(test)]
mod source_of_truth_guard {
    /// ★ 对账路径不许自己去开 SSH 拉 tmux。
    #[test]
    fn the_reconcile_path_never_execs_its_own_tmux_listing() {
        let me = include_str!("tmux_reconcile.rs");
        assert!(
            me.len() > 3000,
            "只读到 {} 字节 —— include_str! 没读到，本断言在空转",
            me.len()
        );
        // 只看生产段：本护栏自己的文档里就写着那个名字。
        let marker = "\n#[cfg(test)]";
        let prod = me.split(marker).next().unwrap_or(me);
        assert!(
            prod.len() > 1000 && prod.len() < me.len(),
            "剥完生产段只剩 {} 字节（原文 {}）—— 剥法坏了",
            prod.len(),
            me.len()
        );
        assert!(
            !prod.contains("list_remote_tmux"),
            "对账路径开始自己 exec `tmux ls` 了 —— 那是**轮询回潮**。\n\
             B2 加推送帧正是为了替掉每 8s 新建 SSH 的轮询（治远端 sshd 日志刷屏）。\n\
             对账拿不到数据时应当**等下一帧**，不是顺手 exec 一条。\n\
             决策点（`tabs.ts`）用 `list_remote_tmux` 是另一回事，那条路要保留。"
        );
    }
}
