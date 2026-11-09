//! 「这台机器上现在有哪些 tmux 会话」的那一张快照 —— 全 crate 只有一份（watcher 要差分「这一轮谁没了」，Gate 要判活 ＋ 铸名避让）。
//!
//! 两条纪律都在 API 形状上：
//! 1. 问它一次 = 它更新一次。[`SessionSnapshot::query`] 没有「只读缓存」这条路 —— 先探一次、再把探回来的那份存下来并返回，拿到的值恒是这一刻的。
//!    watcher 用 [`SessionSnapshot::publish`] 把它焐热，但焐热的值不会被 `query` 交出去。退化成读缓存 = 拿陈值判活 = 把刚死的会话报成活的
//!    （`bus-list` 会把敲门文字打进陌生占用者的屏幕）。
//! 2. 铸名避让只能问它：[`TakenNames`] 的字段是模块私有的 ⇒ 本模块之外造不出一份「已占用的名字」。
//!
//! 住 `common/`：control（Gate 判活与铸名）与 observe（watcher 发布）都要，而 `control/` 不许引用 `observe/`；
//! 无域知识（只认「会话名 + `@ccm_sid`」两个字段）。探测器（起 tmux 那一发）住 `host::tmux::list_sessions`，
//! 进程内那一份由 `host::snapshot` 造（`common/` 只引 `platform/`）。
//!
//! # 不承诺的
//!
//! - 它不是门那一探的替代品：`control/gate.rs::probe_with` 取的是 `#{session_id}` 句柄（关 TOCTOU 窗口）；本模块交出来的是名字，
//!   名字会被重新绑定 ⇒ 不许拿它去下破坏性命令。
//! - 不带 `#{session_id}`：两个消费者（`cc_bus::join_identity` 与 `ccm` 的铸名避让）都不问那一列；格式串里有它（会话账本要），快照这一行不收。切行住探测器那一侧（`host::tmux::session_rows`）。

use std::sync::Mutex;

/// 命令级错误：`(code, message)`。与 `control::gate` 同型。
use crate::common::said::Said;

/// 快照里的一行：**只有名字与 `@ccm_sid`**（为什么不带 `session_id` 见模块头注）。
///
/// `@ccm_sid` 打在**窗格**上（一个会话里可以跑几个 claude）⇒ 一行 = 一个会话 × 它里面挂着的一个 sid：
/// 挂着几个就几行（活动窗格那个在前），一个都没挂 ⇒ 一行空 sid（会话本身还在）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    /// `#{session_name}`。
    pub name: String,
    /// 这个会话某个窗格上 `@ccm_sid` 的值。一个都没挂 ⇒ 空串（= 不是 `ccm` 起的，或还没绑 sid）。
    pub ccm_sid: String,
}

/// 一个会话里挂着的 sid：活动窗格那个（`active` ＝ 会话上下文里的 `#{@ccm_sid}`）在前，其余按窗口 / 窗格序
/// （`all` ＝ `#{W:#{P:#{@ccm_sid} }}` 的展开：逐窗口、逐窗格，空格分隔，没挂的窗格是空段；窗格上没有自己的值时
/// tmux 往上取会话那一级的，那也算挂着）；去重；只认会话 id（`cmd_wire::Sid`；极老 tmux 不展开、原样留 `#{…}` 字面量 ⇒ 不当 sid）。
pub fn pane_sids(active: &str, all: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in std::iter::once(active).chain(all.split(' ')).map(str::trim) {
        if s.parse::<cmd_wire::Sid>().is_ok() && !out.iter().any(|o| o == s) {
            out.push(s.to_string());
        }
    }
    out
}

/// 一个会话 ⇒ 快照行（见 [`SessionRow`]）。
pub fn rows_of(name: &str, sids: Vec<String>) -> Vec<SessionRow> {
    if sids.is_empty() {
        return vec![SessionRow {
            name: name.to_string(),
            ccm_sid: String::new(),
        }];
    }
    sids.into_iter()
        .map(|ccm_sid| SessionRow {
            name: name.to_string(),
            ccm_sid,
        })
        .collect()
}

/// 已被占用的会话名。字段刻意是模块私有的：铸名避让（`ccm::plan::build`）只收这个类型，「另起一份名字集合」编译不过。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TakenNames(Vec<String>, Vec<SessionRow>, Vec<SessionRow>);

impl TakenNames {
    /// 并上这台活着的托管终端（名字 ＋ 挂着的 sid；`ccm` 那一侧凭盘现读交进来）：
    /// 托管名照同一份避让（两种宿主的名字不撞），「在跑」也认托管那几行（[`Self::running_hosted`]）。
    pub fn with_hosted(mut self, rows: Vec<SessionRow>) -> Self {
        for r in &rows {
            if !self.0.contains(&r.name) {
                self.0.push(r.name.clone());
            }
        }
        self.2 = rows;
        self
    }

    /// 只有托管那一臂（tmux 那一份问不到：没装 · 列不成）：避让照托管那几个名字。
    pub fn hosted_only(rows: Vec<SessionRow>) -> Self {
        TakenNames(Vec::new(), Vec::new(), Vec::new()).with_hosted(rows)
    }

    /// claude 会话 `sid` 正在哪个托管终端里跑（那一个的事实挂着它）；没有 ⇒ `None`。
    pub fn running_hosted(&self, sid: &str) -> Option<&str> {
        self.2
            .iter()
            .find(|r| !sid.is_empty() && r.ccm_sid == sid)
            .map(|r| r.name.as_str())
    }

    /// 给避让算法用的那一份。
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    /// claude 会话 `sid` 正在哪个 tmux 会话里跑（某个窗格的 `@ccm_sid` 对上）；没有 ⇒ `None`。
    /// 与避让同一份快照（同一次探测）⇒ `ccm --ccm-print` 与真跑对「在跑」也同答。
    pub fn running(&self, sid: &str) -> Option<&str> {
        self.1
            .iter()
            .find(|r| !sid.is_empty() && r.ccm_sid == sid)
            .map(|r| r.name.as_str())
    }
}

/// 探回来一份的那个动作。测试用假的替掉，生产用 [`probe_tmux`]。
pub(crate) type Prober = Box<dyn Fn() -> Result<Vec<SessionRow>, Said> + Send + Sync>;

/// 那一张快照。
pub struct SessionSnapshot {
    rows: Mutex<Vec<SessionRow>>,
    probe: Prober,
}

impl SessionSnapshot {
    /// 拿一个探测器造一份（进程内那一份在 `crate::host::snapshot`）。
    pub fn new(probe: Prober) -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
            probe,
        }
    }

    /// 向快照发一次询问 ⇒ 快照更新一次。没有「只读」这条路：交出去的恒是刚探回来的那一份。改成「先看缓存、有就直接回」是本模块唯一会静默失效的改法。
    pub(crate) fn query(&self) -> Result<Vec<SessionRow>, Said> {
        let fresh = (self.probe)()?;
        // 存一份供 `publish` 的同居者读（并让「刚探到的」与「焐着的」始终是同一份）。
        if let Ok(mut g) = self.rows.lock() {
            g.clone_from(&fresh);
        }
        Ok(fresh)
    }

    /// 铸名避让要的那份「已占用的名字」。**同样会触发一次更新**（它就是 `query` 的投影）。
    pub fn taken_names(&self) -> Result<TakenNames, Said> {
        let rows = self.query()?;
        let mut names: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
        names.dedup();
        Ok(TakenNames(names, rows, Vec::new()))
    }

    /// 观测方把刚看到的一份**焐进来**。
    ///
    /// ⚠ **焐热不等于「问过了」**：`query` 一定重探，绝不把这里存的值交出去。
    /// 它买的是「同一张表」这件事本身 —— 观测与判活从此看的是一个数据结构，
    /// 而不是两条各自去起 tmux 的路。
    pub fn publish(&self, rows: Vec<SessionRow>) {
        if let Ok(mut g) = self.rows.lock() {
            *g = rows;
        }
    }
}

// 判据用的两个口子（脚本化的探测器 · 陈值）：本包与上层的测试档（经 `test-support`）共用。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../tests/backend/common/session_snapshot_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "../../../tests/backend/common/session_snapshot_tests.rs"]
mod tests;
