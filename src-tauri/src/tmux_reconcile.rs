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

use crate::session_map::SessionChange;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;
use std::time::Duration;

/// 对账轮询间隔（真机标定项）。灰延迟 ≈ `POLL_INTERVAL × RETIRE_MISS_THRESHOLD`。
pub const POLL_INTERVAL: Duration = Duration::from_secs(8);
/// 连续缺失多少轮才 retire（debounce，真机标定项）。
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
pub fn reconcile_step(
    state: &mut ReconcileState,
    announced_live: &HashSet<String>,
    backend_reported_sids: &HashSet<String>,
    retire_threshold: u32,
) -> Vec<String> {
    // 1. 剔除已不在 announced_live 的追踪（会话经 daemon 正常结束/漂移，清陈旧账）。
    state.per_sid.retain(|sid, _| announced_live.contains(sid));
    let mut retire = Vec::new();
    // 2. 逐个宣告活跃 sid 更新追踪。
    for sid in announced_live {
        let t = state.per_sid.entry(sid.clone()).or_default();
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

/// 低频 poller：读 `announced_registry`（origin→sids）、逐 origin 查 daemon 经 `TmuxSessions` 帧推来的
/// `@ccm_sid` 集、跑 `reconcile_step`、把 retire 的 sid 当 `SessionChange{removed}` 送进 `remote_tx`
/// （§24 由唯一写者 emitter 兜底）。仅在有远端配置时由 lib.rs 起。
///
/// **B2 能力前提（审计知情）**：本对账信号依赖 daemon 发 `TmuxSessions` 帧（EMITS `"tmux_sessions"`，
/// p1p 起）。对**不发该帧**的 origin——① daemonless 模式（无 daemon），② 陈旧手动部署、未升到 p1p 的旧
/// daemon——`tmux_by_origin` 恒无该 origin → 每轮跳过 → 「带外杀 tmux → 有界变灰」这条加速信号对其**静默
/// 失效**（非 panic、非误灰，退化为无此信号）。可接受，因：daemonless 会话仍由 mtime 窗口
/// （`DAEMONLESS_ACTIVE_WINDOW_MINUTES`，最多 ~30min）兜底变灰；陈旧 daemon 本就由 `EXPECTED_DAEMON_BUILD_ID`
/// 版本协商的 `StaleBuild` 警告提示用户升级（tmux-reconcile 退化是「daemon 过旧」的子集）。正常路径 daemon
/// 连上即自动部署到 p1p → 恒发该帧，此退化不触及。**未加 tmux 专属 warn**（正确的 warn 需把 daemon `emits`
/// 也解进 `InboundFrame::Hello` + 每-origin 能力登记；启发式「已宣告但无 tmux 帧」会在 daemonless / 首连窗口
/// 误报）——留作可选后续。
pub async fn run_tmux_reconcile_poller(remote_tx: Sender<SessionChange>) {
    let mut states: HashMap<String, ReconcileState> = HashMap::new();
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        let by_origin = crate::ssh_source::snapshot_announced_by_origin();
        // B2：读 daemon 经 `TmuxSessions` 帧推来的每 origin 最新 tmux ls 原文（**零 SSH**——替掉每 8s
        // 新建 SSH 跑 tmux ls 的刷屏轮询；daemon 在远端本地跑 tmux ls 经常驻流上报）。
        let tmux_by_origin = crate::ssh_source::snapshot_tmux_by_origin();
        for (origin, announced_live) in &by_origin {
            // 该 origin 尚无 tmux 状态（daemon 未发 / 连接刚起 / 断连已清）→ 跳过本轮（观测无效不累计缺失）。
            let raw = match tmux_by_origin.get(origin) {
                Some(r) => r,
                None => continue,
            };
            // 哨兵 `NO_TMUX`（远端无 tmux）→ 跳过（同原 `Ok(None)`），观测无效不累计缺失。
            if raw.trim() == "NO_TMUX" {
                continue;
            }
            let sessions = crate::tmux::parse_tmux_ls(raw);
            let backend: HashSet<String> = sessions.iter().filter_map(|s| s.sid.clone()).collect();
            // ★ 空 backend 保守跳过：`tmux ls` 的 `|| true` 把「tmux ls 瞬时错误」也吞成空，
            // 无法与「tmux 真全没了/整服务被杀」区分——两者都空。若对空 backend 累计缺失，一次抖动
            // 就会把该 origin 所有会话批量误灰、且 poller 只发 removed 不发 added → **永久卡灰**
            // （不像断连窗口靠重连自愈）。主场景「杀单个会话」backend 非空、目标缺失照常 retire；
            // 「整服务被杀」的变灰交给断连 flush 兜（那本就会断连）。
            if backend.is_empty() {
                continue;
            }
            let st = states.entry(origin.clone()).or_default();
            let retire = reconcile_step(st, announced_live, &backend, RETIRE_MISS_THRESHOLD);
            if !retire.is_empty() {
                tracing::info!(
                    "tmux-reconcile: [{origin}] retire {} sid(s)（tmux 后端已不在）",
                    retire.len()
                );
                // 送进唯一写者通道；发送失败（emitter 线程没起）忽略——退化为旧行为。
                let _ = remote_tx.send(SessionChange {
                    added: Vec::new(),
                    removed: retire,
                    status_changed: Vec::new(),
                });
            }
        }
        // 清掉已无任何宣告的 origin 的 state，避免无界增长。
        states.retain(|o, _| by_origin.contains_key(o));
    }
}

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
            assert!(reconcile_step(&mut st, &announced, &backend, 2).is_empty());
        }
    }

    #[test]
    fn bound_then_missing_retires_at_threshold_once() {
        let mut st = ReconcileState::default();
        let announced = hs(&["s"]);
        // 第 1 轮：后端自报在跑 → ever_bound。
        assert!(reconcile_step(&mut st, &announced, &hs(&["s"]), 2).is_empty());
        // 后端消失：第 1 次缺失（miss=1 < 2）→ 不 retire。
        assert!(reconcile_step(&mut st, &announced, &hs(&[]), 2).is_empty());
        // 第 2 次缺失（miss=2 >= 2）→ retire 一次。
        assert_eq!(
            reconcile_step(&mut st, &announced, &hs(&[]), 2),
            vec!["s".to_string()]
        );
    }

    #[test]
    fn retire_is_idempotent() {
        let mut st = ReconcileState::default();
        let announced = hs(&["s"]);
        reconcile_step(&mut st, &announced, &hs(&["s"]), 1); // bound
        assert_eq!(
            reconcile_step(&mut st, &announced, &hs(&[]), 1),
            vec!["s".to_string()]
        );
        // 之后继续缺失也不重复 emit（retired 已置）。
        assert!(reconcile_step(&mut st, &announced, &hs(&[]), 1).is_empty());
        assert!(reconcile_step(&mut st, &announced, &hs(&[]), 1).is_empty());
    }

    #[test]
    fn rebind_resets_miss() {
        let mut st = ReconcileState::default();
        let announced = hs(&["s"]);
        reconcile_step(&mut st, &announced, &hs(&["s"]), 3); // bound
        reconcile_step(&mut st, &announced, &hs(&[]), 3); // miss=1
        reconcile_step(&mut st, &announced, &hs(&[]), 3); // miss=2
        reconcile_step(&mut st, &announced, &hs(&["s"]), 3); // 复绑 → miss=0
                                                             // 再缺 2 轮仍不到阈值 3。
        assert!(reconcile_step(&mut st, &announced, &hs(&[]), 3).is_empty());
        assert!(reconcile_step(&mut st, &announced, &hs(&[]), 3).is_empty());
    }

    #[test]
    fn sid_leaving_announced_drops_tracking() {
        let mut st = ReconcileState::default();
        reconcile_step(&mut st, &hs(&["s"]), &hs(&["s"]), 1); // bound
        reconcile_step(&mut st, &hs(&["s"]), &hs(&[]), 1); // retire s（miss>=1）
                                                           // s 经 daemon 正常结束 → 离开 announced_live → 追踪被剔除。
        reconcile_step(&mut st, &hs(&[]), &hs(&[]), 1);
        // s 又出现且后端在跑（复用同名）→ 重新 ever_bound、不带旧 retired 状态。
        assert!(reconcile_step(&mut st, &hs(&["s"]), &hs(&["s"]), 1).is_empty());
    }

    #[test]
    fn branch_drift_does_not_retire_old_sid() {
        // /branch：daemon 先退旧 A 宣告新 B。对账里 A 离开 announced_live → 不 retire；B 新绑。
        let mut st = ReconcileState::default();
        reconcile_step(&mut st, &hs(&["A"]), &hs(&["A"]), 2); // A bound
                                                              // 漂移后一轮：announced_live 只剩 B（daemon 已退 A），后端自报 B。
        let retire = reconcile_step(&mut st, &hs(&["B"]), &hs(&["B"]), 2);
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
        reconcile_step(&mut st, &hs(&["A"]), &hs(&["A"]), 2); // A bound
                                                              // 滞后 1 轮：A 仍 announced、backend=B → A miss=1 < 2 → 不 retire。
        assert!(reconcile_step(&mut st, &hs(&["A", "B"]), &hs(&["B"]), 2).is_empty());
        // 下一轮 daemon 退 A → A 离开 announced_live → 剔除追踪、不 retire。
        assert!(reconcile_step(&mut st, &hs(&["B"]), &hs(&["B"]), 2).is_empty());
    }

    #[test]
    fn empty_backend_increments_ever_bound_sid() {
        // 注：纯函数对空 backend 会累计缺失（source-agnostic、F90 期或有更好的空 vs 错区分）；
        // **poller 侧对空 backend 保守跳过**（见 run_tmux_reconcile_poller）——本测只锁纯函数语义。
        let mut st = ReconcileState::default();
        let announced = hs(&["s"]);
        reconcile_step(&mut st, &announced, &hs(&["s"]), 5); // ever_bound
                                                             // 后端空集（tmux 全没了但会话还宣告活）→ 累计缺失。
        for i in 1..5 {
            assert!(
                reconcile_step(&mut st, &announced, &hs(&[]), 5).is_empty(),
                "miss={i} 未到阈值 5"
            );
        }
        assert_eq!(
            reconcile_step(&mut st, &announced, &hs(&[]), 5),
            vec!["s".to_string()]
        );
    }

    #[test]
    fn threshold_one_retires_immediately_on_first_miss() {
        let mut st = ReconcileState::default();
        let announced = hs(&["s"]);
        reconcile_step(&mut st, &announced, &hs(&["s"]), 1); // bound
        assert_eq!(
            reconcile_step(&mut st, &announced, &hs(&[]), 1),
            vec!["s".to_string()]
        );
    }

    #[test]
    fn multiple_sids_mixed() {
        // a: bound→missing 到阈值 retire；b: 一直在跑不动；c: never-bound 不判。
        let mut st = ReconcileState::default();
        let announced = hs(&["a", "b", "c"]);
        reconcile_step(&mut st, &announced, &hs(&["a", "b"]), 2); // a,b bound; c never
        let retire = reconcile_step(&mut st, &announced, &hs(&["b"]), 2); // a miss=1
        assert!(retire.is_empty());
        let mut retire = reconcile_step(&mut st, &announced, &hs(&["b"]), 2); // a miss=2 → retire
        retire.sort();
        assert_eq!(retire, vec!["a".to_string()]);
    }
}
