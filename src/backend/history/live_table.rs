//! **整台一张活会话表**：这台此刻每个会话进程一行（pidfile 经适配层翻成 [`ProcFact`]，带判过的活不活 · 启动时刻核过没 · 冻住），
//! 外加待答登记表挂着的会话与可重连的那几条。会话活不活 · 在干什么 · 在等什么 · 谁在写，冷路一律问它（[`now`]）。
//!
//! 只有 watcher 写：它每次看见进程状态文件 · 冻住 · 可重连那几样变了就整份重造、发布（[`publish`]）；常驻进程读发布的那份。
//! 没人发布（一次性子命令 · 判据夹具 · 问的不是这台盯着的那个家）⇒ 用同一个构造函数现扫（[`LiveTable::scan`]）。
//! 挂着的请求问常驻进程那一份待答登记表（`control::asks`，读的那一刻现取）；会话账本交来可重连的那几条（[`set_reconnectable`]）。

use crate::agents::{ProcFact, SessionActivity};
use crate::platform::fs::Ro;
use relay_route_core::{Cap, Over};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

/// 单份进程状态文件（`sessions/<PID>.json`）读取上限（正常几百字节）：超了跳过那一份、读那一层 `warn!` 点名。
/// 盯这些文件的那一路（`observe::watcher`）读它们也用这一个。
pub(crate) const SESSION_FILE_CAP: Cap = Cap {
    bytes: 1024 * 1024,
    over: Over::SkipAndSay,
};
/// pidfile 目录扫描上限，防病态目录拖垮一次性查询。
const MAX_SESSION_FILES: usize = 500;

/// 表里的一行：一个会话进程。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveRow {
    pub(crate) pid: u32,
    /// 那份 pidfile 说的（哪一家 · 会话 id · 工作目录 · 在干什么 · 在等什么 · 何时起）。
    pub(crate) fact: ProcFact,
    /// 进程此刻还是写它的那一个（`platform::proc::session_alive`：存在性 ＋ 有启动时刻时对拍）。
    pub(crate) alive: bool,
    /// 启动时刻核过：pidfile 记了 `procStart`、与此刻那个 pid 的启动时刻逐值相等（[`start_matches`]）。
    /// 账号归属只认这一格（错标签比缺标签危害大：缺 `procStart` 的老 pidfile 不认）。
    pub(crate) start_checked: bool,
    /// 说在跑、可过了时限没再听见它（watcher 判的心跳那一维）⇒ 不说在跑。现扫的那份恒为假。
    pub(crate) frozen: bool,
}

impl LiveRow {
    /// 这一行说的会话 id（空串当没有）。
    pub(crate) fn sid(&self) -> Option<&str> {
        self.fact.sid.as_deref().filter(|s| !s.is_empty())
    }

    /// 活着的交互进程（后台任务不算）。
    fn interactive(&self) -> bool {
        self.alive && !self.fact.background
    }

    /// 这一行此刻交出去的那一态：说在跑、冻住了 ⇒ 说不清。
    fn heard(&self) -> Option<SessionActivity> {
        heard(self.fact.activity, self.frozen)
    }
}

/// 说在跑、冻住了 ⇒ 说不清；别的态照原样。
pub(crate) fn heard(activity: Option<SessionActivity>, frozen: bool) -> Option<SessionActivity> {
    if frozen && activity == Some(SessionActivity::Working) {
        None
    } else {
        activity
    }
}

/// 同一会话几个进程持着：取最要紧的那一个（等人 ＞ 在跑 ＞ 后台 ＞ 闲着 ＞ 说不清）。
fn rank(a: Option<SessionActivity>) -> u8 {
    match a {
        Some(SessionActivity::NeedsYou) => 4,
        Some(SessionActivity::Working) => 3,
        Some(SessionActivity::BackgroundWork) => 2,
        Some(SessionActivity::Idle) => 1,
        None => 0,
    }
}

/// **一条活会话此刻在干什么的唯一判法**：有待答请求挂着 ⇒ 等人（挡着终端的那种框不改 pidfile 的状态）；
/// 否则各进程交出去的那一态（[`heard`]）取最要紧的。watcher 宣告 · 改态与冷路都经它。
pub(crate) fn doing_of(
    heard: impl IntoIterator<Item = Option<SessionActivity>>,
    asked: bool,
) -> Option<SessionActivity> {
    if asked {
        return Some(SessionActivity::NeedsYou);
    }
    heard.into_iter().max_by_key(|a| rank(*a)).flatten()
}

/// pidfile 记的启动时刻与此刻那个 pid 的逐值相等（启动时刻的唯一读法；watcher 加会话那一道闸也问它）。
/// 没记 · 读不到（进程已死）⇒ 假。
pub(crate) fn start_matches(pid: u32, recorded: Option<u64>) -> bool {
    match (recorded, crate::platform::proc::proc_starttime(pid)) {
        (Some(r), Some(c)) => r == c,
        _ => false,
    }
}

/// 那台 pidfile 说「在等」的那一格（[`LiveTable::wait_of`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PidWait {
    /// 在等什么框（适配层翻好的 [`crate::agents::WaitOn`]）；没说 / 说不清 ⇒ `None`。
    pub(crate) waiting_for: Option<crate::agents::WaitOn>,
    pub(crate) since_ms: Option<u64>,
    /// 读那份 pidfile 的时刻（epoch ms，**同一台的钟**）：「已等多久」＝ 它减 `since_ms`。
    pub(crate) read_at_ms: u64,
}

/// 整台的那一张。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LiveTable {
    /// 造它的那个家（发布的那份只答问同一个家的）。
    pub(crate) home: PathBuf,
    pub(crate) rows: Vec<LiveRow>,
    /// 此刻有待答请求挂着的会话（常驻进程那一份登记表）。
    pub(crate) asked: BTreeSet<String>,
    /// 此刻可重连的那几条（claude 退了、tmux 里还挂着它；会话账本交来）。
    pub(crate) reconnectable: BTreeSet<String>,
}

impl LiveTable {
    /// **唯一的构造函数**：读 `agent_home` 的 pidfile 目录（有上限、跳过要说清），每份一行、判好活不活与启动时刻；
    /// 挂着的请求与可重连的那几条取此刻交来的那一份。`frozen` 由 watcher 发布前填（[`LiveTable::freeze`]）。
    pub(crate) fn scan(agent_home: &Path) -> Self {
        LiveTable {
            home: agent_home.to_path_buf(),
            ..Self::scan_dir(&crate::agents::pidfile_dir(agent_home))
        }
    }

    /// 同 [`LiveTable::scan`]，读的是点名的那个 pidfile 目录（答题 hook 盯的是发问那一家自己的目录）。
    pub(crate) fn scan_dir(dir: &Path) -> Self {
        let rows = pidfiles(dir)
            .into_iter()
            .map(|(pid, fact)| LiveRow {
                pid,
                alive: crate::platform::proc::session_alive(pid, fact.proc_start),
                start_checked: start_matches(pid, fact.proc_start),
                frozen: false,
                fact,
            })
            .collect();
        LiveTable {
            home: PathBuf::new(),
            rows,
            asked: super::needs_changed::asked_sids(),
            reconnectable: lock(&RECONNECTABLE).clone(),
        }
    }

    /// watcher 判的冻住那一格（`pid` 冻住的那几个进程）。
    pub(crate) fn freeze(&mut self, frozen: &BTreeSet<u32>) {
        for r in &mut self.rows {
            r.frozen = frozen.contains(&r.pid);
        }
    }

    /// 全部的行（连已死的：账号归属那几行要说它死了）。
    pub(crate) fn rows(&self) -> &[LiveRow] {
        &self.rows
    }

    fn live_rows<'a>(&'a self, sid: &'a str) -> impl Iterator<Item = &'a LiveRow> + 'a {
        self.rows
            .iter()
            .filter(move |r| r.alive && r.sid() == Some(sid))
    }

    /// **这台此刻活着的每个会话在干什么**（[`doing_of`]；后台任务也算）。不在里面 ＝ 不活。
    pub(crate) fn doing(&self) -> BTreeMap<String, Option<SessionActivity>> {
        let mut out: BTreeMap<String, Option<SessionActivity>> = BTreeMap::new();
        for r in self.rows.iter().filter(|r| r.alive) {
            let Some(sid) = r.sid() else { continue };
            let cell = out.entry(sid.to_string()).or_default();
            *cell = doing_of([*cell, r.heard()], false);
        }
        for (sid, cell) in &mut out {
            if self.asked.contains(sid) {
                *cell = doing_of([*cell], true);
            }
        }
        out
    }

    /// 这一条此刻在干什么：活着 ⇒ `Some(那一态)`；不活 ⇒ `None`。
    pub(crate) fn doing_one(&self, sid: &str) -> Option<Option<SessionActivity>> {
        let heard: Vec<Option<SessionActivity>> = self.live_rows(sid).map(LiveRow::heard).collect();
        if heard.is_empty() {
            return None;
        }
        Some(doing_of(heard, self.asked.contains(sid)))
    }

    /// 这一条此刻那一态从何时起（epoch ms，那台的钟）：几个进程都说了 ⇒ 最晚的那个；不活 · 都没说 ⇒ `None`。
    /// 会话输入队的投递判「送出之后那一轮起来过没有」读它。
    pub(crate) fn since_of(&self, sid: &str) -> Option<u64> {
        self.live_rows(sid)
            .filter_map(|r| r.fact.status_since_ms)
            .max()
    }

    /// **这台上需手动的会话**：活着、在等人的（那台说在等 ∪ 有请求挂着），按会话 id 排。
    pub(crate) fn needs(&self) -> Vec<String> {
        self.doing()
            .into_iter()
            .filter(|(_, a)| *a == Some(SessionActivity::NeedsYou))
            .map(|(sid, _)| sid)
            .collect()
    }

    /// 那台 pidfile 自己说这条会话在等人（后台任务也算；只挂着请求不算）。
    pub(crate) fn says_waiting(&self, sid: &str) -> bool {
        self.live_rows(sid)
            .any(|r| r.fact.activity == Some(SessionActivity::NeedsYou))
    }

    /// 这条会话此刻在不在等人（同 [`LiveTable::needs`] 那一判）。
    pub(crate) fn waiting(&self, sid: &str) -> bool {
        self.doing_one(sid) == Some(Some(SessionActivity::NeedsYou))
    }

    /// 那台 pidfile 说这条会话此刻在等人的那一格（几个进程都在等 ⇒ 等得最早的那个）；不在等 · 不活 ⇒ `None`。
    /// 只挂着请求、pidfile 没说在等 ⇒ 也是 `None`（等的是什么由请求那一格说）。
    pub(crate) fn wait_of(&self, sid: &str) -> Option<PidWait> {
        let read_at_ms = crate::common::time::now_ms();
        self.live_rows(sid)
            .filter(|r| !r.fact.background)
            .filter(|r| r.fact.activity == Some(SessionActivity::NeedsYou))
            .map(|r| PidWait {
                waiting_for: r.fact.wait,
                since_ms: r.fact.status_since_ms,
                read_at_ms,
            })
            .min_by_key(|w| w.since_ms.unwrap_or(u64::MAX))
    }

    /// 那台 pidfile 说这条会话一轮停了、后台命令还在跑 ⇒ `Some(那个进程何时起的；没写 ⇒ None)`（几个进程 ⇒ 起得最早的）。
    pub(crate) fn background_since(&self, sid: &str) -> Option<Option<u64>> {
        self.live_rows(sid)
            .filter(|r| !r.fact.background)
            .filter(|r| r.fact.activity == Some(SessionActivity::BackgroundWork))
            .map(|r| r.fact.started_ms)
            .min_by_key(|b| b.unwrap_or(u64::MAX))
    }

    /// 写这份 pidfile 的那一家（找不到这条会话的记录时认工具用）。
    pub(crate) fn kind_of(&self, sid: &str) -> Option<&'static str> {
        self.rows
            .iter()
            .find(|r| r.sid() == Some(sid))
            .map(|r| r.fact.kind)
    }

    /// 这台判的这一条活不活：活着照 [`LiveTable::doing_one`]，否则可重连 · 已结束。
    pub(crate) fn life_of(&self, sid: &str) -> crate::wire::WireLife {
        use crate::wire::WireLife;
        match self.doing_one(sid) {
            Some(activity) => WireLife::Live { activity },
            None if self.reconnectable.contains(sid) => WireLife::Reconnectable,
            None => WireLife::Ended { by: None },
        }
    }

    /// 此刻持着 `sid` 的活交互进程 pid（后台任务不算），升序。不止一个 ⇒ 几个进程在同时写。
    pub(crate) fn writers(&self, sid: &str) -> Vec<u32> {
        let mut pids: Vec<u32> = self
            .rows
            .iter()
            .filter(|r| r.interactive() && r.fact.sid.as_deref() == Some(sid))
            .map(|r| r.pid)
            .collect();
        pids.sort_unstable();
        pids
    }

    /// 这台此刻活着的会话（sid 集合）。
    pub(crate) fn sids(&self) -> BTreeSet<String> {
        self.rows
            .iter()
            .filter(|r| r.alive)
            .filter_map(|r| r.sid().map(str::to_string))
            .collect()
    }

    /// 这台此刻活着的交互会话的工作目录（后台任务不算），去重、排好序。
    pub(crate) fn cwds(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .rows
            .iter()
            .filter(|r| r.interactive())
            .filter_map(|r| r.fact.cwd.clone().filter(|c| !c.is_empty()))
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// 这台此刻活着的分身会话（pidfile 那一家说是后台的）。
    pub(crate) fn background(&self) -> BTreeSet<String> {
        self.rows
            .iter()
            .filter(|r| r.alive && r.fact.background)
            .filter_map(|r| r.sid().map(str::to_string))
            .collect()
    }

    /// 这台此刻活着的交互会话那几行（后台任务不算）：读 MCP 那一格之类逐会话要再读记录的。
    pub(crate) fn interactive(&self) -> impl Iterator<Item = &LiveRow> {
        self.rows.iter().filter(|r| r.interactive())
    }

    /// 持着这条会话、启动时刻核过的那个进程（读它的环境认号用）。
    pub(crate) fn checked_pid(&self, sid: &str) -> Option<u32> {
        self.rows
            .iter()
            .find(|r| r.fact.sid.as_deref() == Some(sid) && r.start_checked)
            .map(|r| r.pid)
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// watcher 发布的那一份（整份换）。
static PUBLISHED: Mutex<Option<Arc<LiveTable>>> = Mutex::new(None);
/// 会话账本交来的那一份。
static RECONNECTABLE: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// watcher 交来整份（只它写）。可重连照此刻交来的那一份盖上。
/// 跟上一份（同一个家）比：进出需手动的 · 离开「活」的（计划那一侧的要人看可能多一条）各敲一声需手动的铃（[`super::needs_changed::moved`]）。
pub(crate) fn publish(mut table: LiveTable) {
    table.reconnectable = lock(&RECONNECTABLE).clone();
    let next = Arc::new(table);
    let prev = lock(&PUBLISHED).replace(Arc::clone(&next));
    let prev = prev.filter(|p| p.home == next.home).unwrap_or_default();
    for sid in needs_moved(&prev, &next) {
        super::needs_changed::moved(&sid);
    }
    if let Some(ring) = LISTENER.get() {
        ring();
    }
}

/// 每发布一份就叫一声的那一个（一个进程一个；表不认是谁在听）。
static LISTENER: std::sync::OnceLock<fn()> = std::sync::OnceLock::new();

/// 登记「每发布一份叫我一声」（会话输入队的投递线程起来时登记，`faces::session_queue_deliver::start`）。
/// 已有一个 ⇒ 不换，回 `false`。
pub(crate) fn on_publish(ring: fn()) -> bool {
    LISTENER.set(ring).is_ok()
}

/// 两份之间需手动那一判动了的会话：在等人的进出 ∪ 离开「活」的。
fn needs_moved(prev: &LiveTable, next: &LiveTable) -> BTreeSet<String> {
    let was: BTreeSet<String> = prev.needs().into_iter().collect();
    let now: BTreeSet<String> = next.needs().into_iter().collect();
    let mut out: BTreeSet<String> = was.symmetric_difference(&now).cloned().collect();
    out.extend(prev.sids().difference(&next.sids()).cloned());
    out
}

/// **问这张表的唯一入口**：发布过、问的是同一个家 ⇒ 那一份（挂着的请求换成此刻的）；否则现扫（[`LiveTable::scan`]）。
pub(crate) fn now(agent_home: &Path) -> Arc<LiveTable> {
    let published = lock(&PUBLISHED)
        .as_ref()
        .filter(|t| t.home == agent_home)
        .cloned();
    let Some(t) = published else {
        return Arc::new(LiveTable::scan(agent_home));
    };
    let asked = super::needs_changed::asked_sids();
    if t.asked == asked {
        return t;
    }
    let mut fresh = (*t).clone();
    fresh.asked = asked;
    Arc::new(fresh)
}

/// 这台此刻活着的会话（家取这台那一家的家）：轮换数「另 N 会话也在用」· 判「会话走了」都问它。
pub(crate) fn live_here() -> BTreeSet<String> {
    now(&super::listing::agent_home()).sids()
}

/// 点名的 pidfile 目录里持着这条会话的每个活进程此刻的（那一态, 这一态从何时起）：答题 hook 判「终端那边答了」用
/// （hook 盯的是发问那一家自己的目录；`main` 把它交给 `control::asks::hook::run`）。
pub fn states_in(dir: &Path, sid: &str) -> Vec<(Option<SessionActivity>, Option<u64>)> {
    LiveTable::scan_dir(dir)
        .live_rows(sid)
        .map(|r| (r.fact.activity, r.fact.status_since_ms))
        .collect()
}

/// 会话账本交来此刻可重连的那几条（整份换）；发布过的那份跟着换。
pub(crate) fn set_reconnectable(now: &BTreeSet<String>) {
    let mut g = lock(&RECONNECTABLE);
    if *g == *now {
        return;
    }
    g.clone_from(now);
    drop(g);
    if let Some(t) = lock(&PUBLISHED).as_mut() {
        Arc::make_mut(t).reconnectable = now.clone();
    }
}

/// pidfile 目录里每一份读得出来的 `(pid, 它说的事实)`（上限、跳过要说清）：目录与读法问注册表（`agents::pidfile_dir` · `agents::proc_fact`）。
fn pidfiles(dir: &Path) -> Vec<(u32, ProcFact)> {
    let mut out = Vec::new();
    let Ok(rd) = Ro::read_dir(dir) else {
        return Vec::new(); // 没有 pidfile 目录 → 零行
    };
    let mut seen = 0usize;
    for ent in rd.flatten() {
        if seen >= MAX_SESSION_FILES {
            tracing::warn!("pidfile 目录条目超过 {MAX_SESSION_FILES}，其余跳过");
            break;
        }
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(pid) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        seen += 1;
        let bytes = match Ro::read_capped(&path, SESSION_FILE_CAP) {
            Ok(b) => b,
            Err(e) => {
                // 跳过要说清是谁：一个超过上限的 pidfile 被静默丢掉，那个会话就永远不归属到任何账号、也没人说得出是哪一个。
                tracing::warn!(
                    "会话文件 {} 读不了，跳过（不归属该会话）：{e}",
                    path.display()
                );
                continue;
            }
        };
        let Some(fact) = crate::agents::proc_fact(&bytes) else {
            continue;
        };
        out.push((pid, fact));
    }
    out
}

#[cfg(test)]
#[path = "../../../tests/backend/history/live_table_tests.rs"]
mod tests;
