//! **整台一张活会话表**：这台此刻每个会话进程一行（pidfile 经适配层翻成 [`ProcFact`]，带判过的活不活 · 启动时刻核过没 · 冻住），
//! 外加待答登记表挂着的会话与可重连的那几条。会话活不活 · 在干什么 · 在等什么 · 谁在写，冷路一律问它（[`now`]）。
//!
//! 只有 watcher 写：它每次看见进程状态文件 · 冻住 · 可重连那几样变了就整份重造、发布（[`publish`]）；常驻进程读发布的那份。
//! 没人发布（一次性子命令 · 判据夹具 · 问的不是这台盯着的那个家）⇒ 用同一个构造函数现扫（[`LiveTable::scan`]）。
//! 挂着的请求问常驻进程那一份待答登记表（`control::asks`，读的那一刻现取）；会话账本交来可重连的那几条（[`set_reconnectable`]）；
//! 那一家的钩子交来的统一事件由常驻收 hook 那一道交进来（[`hear`]），在干什么先照它、pidfile 只在更新时补（[`doing_heard`]）。

use crate::agents::{AgentHome, ProcFact, SessionActivity};
use crate::platform::fs::Ro;
use cap_core::{Cap, Over};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

/// 单份进程状态文件（`sessions/<PID>.json`）读取上限（正常几百字节）：超了跳过那一份、[`read_pidfile`] 里 `warn!` 点名。
const SESSION_FILE_CAP: Cap = Cap {
    bytes: 1024 * 1024,
    over: Over::SkipAndSay,
};
/// pidfile 目录扫描上限，防病态目录拖垮一次性查询。
const MAX_SESSION_FILES: usize = 500;

/// 表里的一行：一个会话进程。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRow {
    pub pid: u32,
    /// 那份 pidfile 说的（哪一家 · 会话 id · 工作目录 · 在干什么 · 在等什么 · 何时起）。
    pub fact: ProcFact,
    /// 进程此刻活着、且是写它的那一条会话（[`author_check`]：bg-spare 不算 · 身份核上 · 否则看时刻与 cmdline）。
    pub alive: bool,
    /// 启动时刻核过：pidfile 记了 `procStart`、与此刻那个 pid 的启动时刻逐值相等（[`start_matches`]）。
    /// 账号归属只认这一格（错标签比缺标签危害大：缺 `procStart` 的老 pidfile 不认）。
    pub start_checked: bool,
    /// 说在跑、可过了时限没再听见它（watcher 判的心跳那一维）⇒ 不说在跑。现扫的那份恒为假。
    pub(crate) frozen: bool,
}

impl LiveRow {
    /// 这一行说的会话 id（空串当没有）。
    pub fn sid(&self) -> Option<&str> {
        self.fact.sid.as_deref().filter(|s| !s.is_empty())
    }

    /// 活着的交互进程（后台任务不算）。
    fn interactive(&self) -> bool {
        self.alive && !self.fact.background
    }

    /// 这一行喂进判法的那一份（[`Feed`]）。
    fn feed(&self) -> Feed {
        Feed {
            activity: self.fact.activity,
            since: self.fact.status_since_ms,
            frozen: self.frozen,
            alive: self.alive,
        }
    }
}

/// **喂进判法的一份**（活会话表的一行 · watcher 账上的一份都只经它交 [`doing_heard`]），三样各只一种定法：
/// - `frozen` 按进程：持着它的那个进程冻住了（watcher 判，[`LiveTable::freeze`] 收的就是进程号）；
/// - `alive` 只算活着的：死了的那一份不交（[`Feed::said`] 回 `None`）；
/// - `since` 取 pidfile 上的 `statusUpdatedAt`（没有 pidfile 的那一份才由读它的那一侧记）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feed {
    pub activity: Option<SessionActivity>,
    pub since: Option<u64>,
    pub frozen: bool,
    pub alive: bool,
}

impl Feed {
    /// 交 [`doing_heard`] 的那一格：那一态（说在跑、冻住了 ⇒ 说不清）与它从何时起；不活 ⇒ 不交。
    pub fn said(&self) -> Option<(Option<SessionActivity>, Option<u64>)> {
        self.alive
            .then(|| (heard(self.activity, self.frozen), self.since))
    }
}

/// 说在跑、冻住了 ⇒ 说不清；别的态照原样。
pub fn heard(activity: Option<SessionActivity>, frozen: bool) -> Option<SessionActivity> {
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
/// 有那一家的钩子交来的事件（[`Heard`]）⇒ 照事件，pidfile 那几态里只有 `statusUpdatedAt` 比事件收到的时刻新的才算（Esc 打断不发事件 ·
/// 后台 shell · 钩子不管的框）；没写时刻的当它旧。没事件 ⇒ 照 pidfile 那几态。
/// `said` ＝ pidfile 那几态与各自的 `statusUpdatedAt`（epoch ms，这台的钟）。
pub fn doing_heard(
    said: impl IntoIterator<Item = (Option<SessionActivity>, Option<u64>)>,
    asked: bool,
    heard: Option<Heard>,
) -> Option<SessionActivity> {
    if asked {
        return Some(SessionActivity::NeedsYou);
    }
    let most = |it: Vec<Option<SessionActivity>>| it.into_iter().max_by_key(|a| rank(*a)).flatten();
    let Some(h) = heard else {
        return most(said.into_iter().map(|(a, _)| a).collect());
    };
    let said: Vec<(Option<SessionActivity>, Option<u64>)> = said.into_iter().collect();
    let newer: Vec<Option<SessionActivity>> = said
        .iter()
        .filter(|(_, at)| at.is_some_and(|t| t > h.at_ms))
        .map(|(a, _)| *a)
        .collect();
    if !newer.is_empty() {
        return most(newer);
    }
    // 事件说在跑、持着它的那几个进程都说不清（冻住了：说在跑、过了时限没再听见）⇒ 也说不清，不画在跑。
    let frozen = !said.is_empty() && said.iter().all(|(a, _)| a.is_none());
    if frozen && h.event.activity() == SessionActivity::Working {
        return None;
    }
    Some(h.event.activity())
}

/// 那一家的钩子交来的最后一件统一事件，与这台收到它的时刻（epoch ms）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heard {
    pub event: crate::agents::AgentEvent,
    pub at_ms: u64,
}

/// pidfile 记的启动时刻与此刻那个 pid 的逐值相等（启动时刻的唯一读法）。没记 · 读不到（进程已死）⇒ 假。
fn start_matches(recorded: Option<u64>, current: Option<u64>) -> bool {
    matches!((recorded, current), (Some(r), Some(c)) if r == c)
}

/// 这个 pid 此刻的占用者是不是写那份 pidfile 的那一条会话（[`add_time_verdict`] 的判决）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddTimeVerdict {
    Alive,
    Imposter(&'static str),
}

/// 进程起得比 pidfile 最后一次写晚出这么多秒 ⇒ 不可能是它写的。60 秒吸收钟的毛刺（mtime 粒度 · btime 取整 · NTP 微调）；
/// 真的 PID 复用隔的是几小时到几周，这个余量是安全的。
const ADD_TIME_TOLERANCE_SECS: u64 = 60;

/// **判活的唯一判法**（纯函数，各平台都测）：这个 pid 此刻的占用者是不是写那份 pidfile 的那一条会话。
///
/// - **cmdline 说它不是一条会话**（那一家的进程、可不是会话：守护池停泊的备用进程，适配层 [`crate::agents::CmdlineSays::NotSession`]）⇒
///   不活 —— 在身份证据之前判：它是真进程、写合规 pidfile、启动时刻自洽，身份一核就会判它活。
/// - **身份证据（主）**：`start_checked` —— pidfile 的 `procStart` 与此刻占用者的启动时刻逐值相等 ⇒ 就是它写的，不再看别的。
///   **对不上不当场判死**：要么是 PID 复用，要么是那一家换了 `procStart` 的记法（Windows 上 pidfile 记 .NET 本地 ticks、
///   这台读的是 FILETIME，恒不等）—— 往下走，下面的启发式两种情形都接得住。
/// - **时刻证据**：进程起得比 pidfile 最后一次写晚出余量 ⇒ 冒名（那一家每次换态都重写 pidfile，mtime 是「真进程此刻还活着」的下界；
///   重启之后也是这一形）。两边都是同一台钟的墙上秒，不涉时区。
/// - **cmdline 证据**：读得出、明显不像那一家（[`crate::agents::CmdlineSays::Unlike`]）⇒ 冒名。
/// - 缺数据（没记 `procStart` · 读不到启动时刻 / mtime / cmdline）⇒ 那一条跳过、放行。
///
/// 生产注册表上的这一形只给判据逐条喂值；生产各处照交下来的注册表走 [`add_time_verdict_among`]。
#[cfg(test)]
pub(crate) fn add_time_verdict(
    kind: &str,
    start_checked: bool,
    proc_start_epoch: Option<u64>,
    file_mtime_epoch: Option<u64>,
    cmdline: Option<&str>,
) -> AddTimeVerdict {
    add_time_verdict_among(
        crate::agents::REGISTRY,
        kind,
        start_checked,
        proc_start_epoch,
        file_mtime_epoch,
        cmdline,
    )
}

/// [`add_time_verdict`] 照给定注册表认那一家的 cmdline（活会话表照交下来的那一份扫时经它）。
fn add_time_verdict_among(
    registry: &[crate::agents::Adapter],
    kind: &str,
    start_checked: bool,
    proc_start_epoch: Option<u64>,
    file_mtime_epoch: Option<u64>,
    cmdline: Option<&str>,
) -> AddTimeVerdict {
    use crate::agents::CmdlineSays;
    // 写这份 pidfile 的那一家（`kind`）认它的 cmdline。
    let says = cmdline.and_then(|cmd| {
        let lower = cmd.to_lowercase();
        crate::agents::local_face_among(registry, kind).map(|f| (f.cmdline_may_be_agent)(&lower))
    });
    if says == Some(CmdlineSays::NotSession) {
        return AddTimeVerdict::Imposter("not-a-session");
    }
    if start_checked {
        return AddTimeVerdict::Alive;
    }
    if let (Some(start), Some(mtime)) = (proc_start_epoch, file_mtime_epoch) {
        if start > mtime + ADD_TIME_TOLERANCE_SECS {
            return AddTimeVerdict::Imposter("started-after-pidfile");
        }
    }
    if says == Some(CmdlineSays::Unlike) {
        return AddTimeVerdict::Imposter("cmdline");
    }
    AddTimeVerdict::Alive
}

/// 判活那一趟 /proc 读：（启动时刻核上没有, 活着且是写它的那一条 ⇒ `Ok(此刻的启动时刻)`，否则 `Err(原因)`）。
/// 进程不在 ⇒ `Err`。`recorded` ＝ 那份 pidfile 说的启动时刻（[`ProcFact::proc_start`]）。
fn author_of(
    registry: &[crate::agents::Adapter],
    kind: &str,
    pid: u32,
    recorded: Option<u64>,
    path: &Path,
) -> (bool, Result<Option<u64>, &'static str>) {
    use crate::platform::proc::{pid_alive, proc_cmdline, proc_starttime, start_epoch_from_ticks};
    if !pid_alive(pid) {
        return (false, Err("gone"));
    }
    let ticks = proc_starttime(pid);
    let checked = start_matches(recorded, ticks);
    let verdict = match add_time_verdict_among(
        registry,
        kind,
        checked,
        start_epoch_from_ticks(ticks),
        file_mtime_epoch(path),
        proc_cmdline(pid).as_deref(),
    ) {
        AddTimeVerdict::Imposter(reason) => Err(reason),
        AddTimeVerdict::Alive => Ok(ticks),
    };
    (checked, verdict)
}

/// **判活的入口**（活会话表每一行 · watcher 加会话那一道闸都经它）：`path` 那份 pidfile 说的 `pid` 此刻活着、
/// 且是写它的那一条会话 ⇒ `Ok(此刻的启动时刻)`（watcher 拿它当 pid 看守的基线），否则 `Err(原因)`。
pub fn author_check(
    kind: &str,
    pid: u32,
    recorded: Option<u64>,
    path: &Path,
) -> Result<Option<u64>, &'static str> {
    author_of(crate::agents::REGISTRY, kind, pid, recorded, path).1
}

/// pidfile 的修改时刻（epoch 秒；读不到 ⇒ `None`，那一条证据跳过）。
fn file_mtime_epoch(path: &Path) -> Option<u64> {
    let mtime = Ro::metadata(path).ok()?.modified().ok()?;
    mtime
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// 那台 pidfile 说「在等」的那一格（[`LiveTable::wait_of`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PidWait {
    /// 在等什么框（适配层翻好的 [`crate::agents::WaitOn`]）；没说 / 说不清 ⇒ `None`。
    pub waiting_for: Option<crate::agents::WaitOn>,
    pub since_ms: Option<u64>,
    /// 读那份 pidfile 的时刻（epoch ms，**同一台的钟**）：「已等多久」＝ 它减 `since_ms`。
    pub read_at_ms: u64,
}

/// 整台的那一张。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveTable {
    /// 造它的那几家的家（[`homes_key`]：发布的那份只答问同一串家的）。
    pub(crate) home: PathBuf,
    pub(crate) rows: Vec<LiveRow>,
    /// 此刻有待答请求挂着的会话（常驻进程那一份登记表）。
    pub(crate) asked: BTreeSet<String>,
    /// 此刻可重连的那几条（claude 退了、tmux 里还挂着它；会话账本交来）。
    pub(crate) reconnectable: BTreeSet<String>,
    /// 钩子交来的最后一件事件（会话 id ⇒ 那一件；[`hear`]）。
    pub(crate) heard: BTreeMap<String, Heard>,
}

impl LiveTable {
    /// **唯一的构造函数**：`homes` 里每一家各读它家的 pidfile 目录（有上限、跳过要说清），每份一行、判好活不活与启动时刻；
    /// 挂着的请求与可重连的那几条取此刻交来的那一份。`frozen` 由 watcher 发布前填（[`LiveTable::freeze`]）。
    pub fn scan(homes: impl crate::agents::IntoHomes) -> Self {
        Self::scan_among(crate::agents::REGISTRY, homes)
    }

    /// [`Self::scan`] 照给定注册表：各家的 pidfile 目录 · 翻法 · 判活认的 cmdline 都照它问（交注册表下来的那几处照它扫）。
    pub fn scan_among(
        registry: &[crate::agents::Adapter],
        homes: impl crate::agents::IntoHomes,
    ) -> Self {
        let homes_v = homes.into_homes_among(registry);
        let homes: &[crate::agents::AgentHome] = &homes_v;
        let rows = homes
            .iter()
            .flat_map(|h| {
                pidfiles(registry, h)
                    .into_iter()
                    .map(move |(path, pid, fact)| {
                        let (start_checked, verdict) =
                            author_of(registry, fact.kind, pid, fact.proc_start, &path);
                        LiveRow {
                            pid,
                            alive: verdict.is_ok(),
                            start_checked,
                            frozen: false,
                            fact,
                        }
                    })
            })
            .collect();
        LiveTable {
            home: homes_key(homes),
            rows,
            asked: super::needs_changed::asked_sids(),
            reconnectable: lock(&RECONNECTABLE).clone(),
            heard: lock(&HEARD).0.clone(),
        }
    }

    /// watcher 判的冻住那一格（冻住按进程：`frozen` 是冻住的那几个进程号，watcher 自己喂判法也照同一份）。
    pub fn freeze(&mut self, frozen: &BTreeSet<u32>) {
        for r in &mut self.rows {
            r.frozen = frozen.contains(&r.pid);
        }
    }

    /// 全部的行（连已死的：账号归属那几行要说它死了）。
    pub fn rows(&self) -> &[LiveRow] {
        &self.rows
    }

    fn live_rows<'a>(&'a self, sid: &'a str) -> impl Iterator<Item = &'a LiveRow> + 'a {
        self.rows
            .iter()
            .filter(move |r| r.alive && r.sid() == Some(sid))
    }

    /// **这台此刻活着的每个会话在干什么**（[`doing_heard`]；后台任务也算）。不在里面 ＝ 不活。
    pub fn doing(&self) -> BTreeMap<String, Option<SessionActivity>> {
        let sids: BTreeSet<&str> = self
            .rows
            .iter()
            .filter(|r| r.alive)
            .filter_map(LiveRow::sid)
            .collect();
        sids.into_iter()
            .filter_map(|sid| Some((sid.to_string(), self.doing_one(sid)?)))
            .collect()
    }

    /// 这一条此刻在干什么：活着 ⇒ `Some(那一态)`；不活 ⇒ `None`。
    pub fn doing_one(&self, sid: &str) -> Option<Option<SessionActivity>> {
        let said: Vec<_> = self
            .rows
            .iter()
            .filter(|r| r.sid() == Some(sid))
            .filter_map(|r| r.feed().said())
            .collect();
        if said.is_empty() {
            return None;
        }
        Some(doing_heard(
            said,
            self.asked.contains(sid),
            self.heard.get(sid).copied(),
        ))
    }

    /// 这一条此刻那一态从何时起（epoch ms，那台的钟）：几个进程都说了 ⇒ 最晚的那个；不活 · 都没说 ⇒ `None`。
    /// 会话输入队的投递判「送出之后那一轮起来过没有」读它。
    pub fn since_of(&self, sid: &str) -> Option<u64> {
        self.live_rows(sid)
            .filter_map(|r| r.fact.status_since_ms)
            .max()
    }

    /// **这台上需手动的会话**：活着、在等人的（那台说在等 ∪ 有请求挂着），按会话 id 排。
    pub fn needs(&self) -> Vec<String> {
        self.doing()
            .into_iter()
            .filter(|(_, a)| *a == Some(SessionActivity::NeedsYou))
            .map(|(sid, _)| sid)
            .collect()
    }

    /// 这条会话此刻在不在等人（同 [`LiveTable::needs`] 那一判）。
    pub fn waiting(&self, sid: &str) -> bool {
        self.doing_one(sid) == Some(Some(SessionActivity::NeedsYou))
    }

    /// **在等什么的 pidfile 那一半**（唯一一处）：照合起来那一态（[`LiveTable::doing_one`]，挂着请求也算在里面）：
    /// 是在等人 ⇒ 说在等的进程里等得最早的那一份（前台后台一样：后台子 agent 挡着终端的批准也要人答）；不是 · 不活 · 没有哪一份说在等 ⇒ `None`。
    /// 「要不要人」的 pidfile 那一半只判这一处（需手动列表 · 会话事实都调它）。
    /// 挂着的请求由调用方经 `dress_ask_with` 配上、压过这一格（只挂着请求 ⇒ 整份照请求造；pidfile 也在等 ⇒ 种类与抬头照请求、起点照 pidfile）。
    pub fn wait_of(&self, sid: &str) -> Option<PidWait> {
        if self.doing_one(sid) != Some(Some(SessionActivity::NeedsYou)) {
            return None;
        }
        let read_at_ms = crate::common::time::now_ms();
        // 认新旧同 [`doing_heard`]：有钩子事件 ⇒ 只算比它新的那几份（被事件盖过的旧 pidfile 不说等的是什么）。
        let heard = self.heard.get(sid).map(|h| h.at_ms);
        self.live_rows(sid)
            .filter(|r| r.fact.activity == Some(SessionActivity::NeedsYou))
            .filter(|r| heard.is_none_or(|at| r.fact.status_since_ms.is_some_and(|t| t > at)))
            .map(|r| PidWait {
                waiting_for: r.fact.wait,
                since_ms: r.fact.status_since_ms,
                read_at_ms,
            })
            .min_by_key(|w| w.since_ms.unwrap_or(u64::MAX))
    }

    /// 那台 pidfile 说这条会话一轮停了、后台命令还在跑 ⇒ `Some(那个进程何时起的；没写 ⇒ None)`（几个进程 ⇒ 起得最早的）。
    pub fn background_since(&self, sid: &str) -> Option<Option<u64>> {
        self.live_rows(sid)
            .filter(|r| !r.fact.background)
            .filter(|r| r.fact.activity == Some(SessionActivity::BackgroundWork))
            .map(|r| r.fact.started_ms)
            .min_by_key(|b| b.unwrap_or(u64::MAX))
    }

    /// 写这份 pidfile 的那一家（找不到这条会话的记录时认工具用）。
    pub fn kind_of(&self, sid: &str) -> Option<&'static str> {
        self.rows
            .iter()
            .find(|r| r.sid() == Some(sid))
            .map(|r| r.fact.kind)
    }

    /// 这台判的这一条活不活：活着照 [`LiveTable::doing_one`]，否则可重连 · 已结束。
    pub fn life_of(&self, sid: &str) -> crate::wire::WireLife {
        use crate::wire::WireLife;
        match self.doing_one(sid) {
            Some(activity) => WireLife::Live { activity },
            None if self.reconnectable.contains(sid) => WireLife::Reconnectable,
            None => WireLife::Ended { by: None },
        }
    }

    /// 此刻持着 `sid` 的活交互进程 pid（后台任务不算），升序。不止一个 ⇒ 几个进程在同时写。
    pub fn writers(&self, sid: &str) -> Vec<u32> {
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
    pub fn cwds(&self) -> Vec<String> {
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
    pub fn background(&self) -> BTreeSet<String> {
        self.rows
            .iter()
            .filter(|r| r.alive && r.fact.background)
            .filter_map(|r| r.sid().map(str::to_string))
            .collect()
    }

    /// 这台此刻活着的交互会话那几行（后台任务不算）：读 MCP 那一格之类逐会话要再读记录的。
    pub fn interactive(&self) -> impl Iterator<Item = &LiveRow> {
        self.rows.iter().filter(|r| r.interactive())
    }

    /// 持着这条会话、启动时刻核过的那个进程（读它的环境认号用）。
    pub fn checked_pid(&self, sid: &str) -> Option<u32> {
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
/// 钩子交来的那几件（常驻进程里一份；[`hear`] 写）。
static HEARD: Mutex<HeardBook> = Mutex::new(HeardBook(BTreeMap::new()));

/// 事件只留最近这么多条会话的（更早的会话多半已经走了；挤掉的那条退回照 pidfile 判）。
pub(crate) const HEARD_KEEP: usize = 256;

/// 每条会话最后一件事件（[`HEARD_KEEP`] 条封顶，挤掉收得最早的那条）。
#[derive(Debug, Default)]
pub(crate) struct HeardBook(pub(crate) BTreeMap<String, Heard>);

impl HeardBook {
    pub(crate) fn put(&mut self, sid: &str, event: crate::agents::AgentEvent, at_ms: u64) {
        self.0.insert(sid.to_string(), Heard { event, at_ms });
        while self.0.len() > HEARD_KEEP {
            let Some(oldest) = self
                .0
                .iter()
                .min_by_key(|(_, h)| h.at_ms)
                .map(|(s, _)| s.clone())
            else {
                break;
            };
            self.0.remove(&oldest);
        }
    }
}

/// 那一家的钩子交来一件统一事件（常驻收 hook 那一道调）：记成这条会话的最后一件、时刻取此刻，
/// 发布过的那份跟着换，敲这条会话需手动的铃（事件可能把它移进移出「等人」；watcher 听这一声补推那条会话的脸）。
pub fn hear(sid: &str, event: crate::agents::AgentEvent) {
    let at_ms = crate::common::time::now_ms();
    let mut g = lock(&HEARD);
    g.put(sid, event, at_ms);
    let now = g.0.clone();
    drop(g);
    if let Some(t) = lock(&PUBLISHED).as_mut() {
        Arc::make_mut(t).heard = now;
    }
    super::needs_changed::moved(sid);
}

/// 这条会话最后一件事件（没有 ⇒ `None`：不是 ccm 起的 · 还没听见 · 挤掉了；watcher 造脸那一处读）。
pub fn heard_of(sid: &str) -> Option<Heard> {
    lock(&HEARD).0.get(sid).copied()
}

/// watcher 交来整份（只它写）。可重连照此刻交来的那一份盖上。
/// 跟上一份（同一个家）比：进出需手动的 · 离开「活」的（计划那一侧的要人看可能多一条）各敲一声需手动的铃（[`super::needs_changed::moved`]）。
pub fn publish(mut table: LiveTable) {
    table.reconnectable = lock(&RECONNECTABLE).clone();
    table.heard = lock(&HEARD).0.clone();
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
pub fn on_publish(ring: fn()) -> bool {
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

/// 一串家记成表上那一格（[`LiveTable::home`]）：各家的家目录逐行接起来（只有一家 ⇒ 就是它的家目录）。
fn homes_key(homes: &[AgentHome]) -> PathBuf {
    let mut key = std::ffi::OsString::new();
    for (i, h) in homes.iter().enumerate() {
        if i > 0 {
            key.push("\n");
        }
        key.push(h.home.as_os_str());
    }
    PathBuf::from(key)
}

/// **问这张表的唯一入口**：发布过、问的是同一串家 ⇒ 那一份（挂着的请求换成此刻的）；否则现扫（[`LiveTable::scan`]）。
pub fn now(homes: impl crate::agents::IntoHomes) -> Arc<LiveTable> {
    now_among(crate::agents::REGISTRY, homes)
}

/// [`now`] 照给定注册表（足迹那一趟交下来的那一份）：没发布过同一串家 ⇒ 照它现扫（[`LiveTable::scan_among`]）；
/// 发布过同一串家 ⇒ 那一份（watcher 照生产注册表扫的，只认家目录那一串、不看传进来的注册表）。
pub fn now_among(
    registry: &[crate::agents::Adapter],
    homes: impl crate::agents::IntoHomes,
) -> Arc<LiveTable> {
    let homes = homes.into_homes_among(registry);
    let published = lock(&PUBLISHED)
        .as_ref()
        .filter(|t| t.home == homes_key(&homes))
        .cloned();
    let Some(t) = published else {
        return Arc::new(LiveTable::scan_among(registry, &homes));
    };
    let asked = super::needs_changed::asked_sids();
    if t.asked == asked {
        return t;
    }
    let mut fresh = (*t).clone();
    fresh.asked = asked;
    Arc::new(fresh)
}

/// 这台此刻活着的会话（有本机布局的每一家各问它的家）：轮换数「另 N 会话也在用」· 判「会话走了」都问它。
pub fn live_here() -> BTreeSet<String> {
    now(&crate::agents::local_homes()).sids()
}

/// `home` 那一家的 pidfile 目录里持着这条会话的每个活进程此刻的（那一态, 这一态从何时起）：判「终端那边答了」用
/// （家是这条会话自己那一家的：hook 那一侧照它自己的环境，常驻那一侧照 `control::asks::hook::status_home`；
/// 一次性子命令那一处 `stream/oneshot.rs` 把它交给 `control::asks::hook::run`）。
pub fn states_in(home: &AgentHome, sid: &str) -> Vec<(Option<SessionActivity>, Option<u64>)> {
    LiveTable::scan(home)
        .live_rows(sid)
        .map(|r| (r.fact.activity, r.fact.status_since_ms))
        .collect()
}

/// 会话账本交来此刻可重连的那几条（整份换）；发布过的那份跟着换。
pub fn set_reconnectable(now: &BTreeSet<String>) {
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

/// 那一家 pidfile 目录里每一份读得出来的 `(路径, pid, 它说的事实)`（上限、跳过要说清）：目录问那一家（[`AgentHome::pidfile_dir`]），每份照 [`read_pidfile`]。
fn pidfiles(
    registry: &[crate::agents::Adapter],
    home: &AgentHome,
) -> Vec<(PathBuf, u32, ProcFact)> {
    let mut out = Vec::new();
    let Some(dir) = home.pidfile_dir_among(registry) else {
        return Vec::new(); // 那一家没有本机布局 → 零行
    };
    let Ok(rd) = Ro::read_dir(&dir) else {
        return Vec::new(); // 没有 pidfile 目录 → 零行
    };
    let mut seen = 0usize;
    for ent in rd.flatten() {
        if seen >= MAX_SESSION_FILES {
            tracing::warn!("pidfile 目录条目超过 {MAX_SESSION_FILES}，其余跳过");
            break;
        }
        let path = ent.path();
        if pid_of(&path).is_none() {
            continue;
        }
        seen += 1;
        if let Some((pid, fact)) = read_pidfile_among(registry, home, &path) {
            out.push((path, pid, fact));
        }
    }
    out
}

/// `sessions/<PID>.json` 那个文件名说的 pid（不是这一形 ⇒ `None`）。
fn pid_of(path: &Path) -> Option<u32> {
    if path.extension().and_then(|e| e.to_str()) != Some("json") {
        return None;
    }
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.parse::<u32>().ok())
}

/// **pidfile 的唯一读法**：`home` 那一家的一份 `<PID>.json` ⇒ `(pid, 它说的事实)`（有上限；读不了要点名；翻法问那一家 [`AgentHome::proc_fact`]）。
/// 活会话表每一份 · watcher 看见一份变了都经它。文件名不是 `<PID>.json` · 读不了 · 翻不出 ⇒ `None`。
pub fn read_pidfile(home: &AgentHome, path: &Path) -> Option<(u32, ProcFact)> {
    read_pidfile_among(crate::agents::REGISTRY, home, path)
}

/// [`read_pidfile`] 照给定注册表翻（[`AgentHome::proc_fact_among`]）。
fn read_pidfile_among(
    registry: &[crate::agents::Adapter],
    home: &AgentHome,
    path: &Path,
) -> Option<(u32, ProcFact)> {
    let pid = pid_of(path)?;
    let bytes = match Ro::read_capped(path, SESSION_FILE_CAP) {
        Ok(b) => b,
        Err(e) => {
            // 跳过要说清是谁：一个超过上限的 pidfile 被静默丢掉，那个会话就永远不归属到任何账号、也没人说得出是哪一个。
            tracing::warn!(
                "会话文件 {} 读不了，跳过（不归属该会话）：{e}",
                path.display()
            );
            return None;
        }
    };
    home.proc_fact_among(registry, &bytes)
        .map(|fact| (pid, fact))
}

#[cfg(test)]
#[path = "../../../tests/backend/history/live_table_tests.rs"]
mod tests;
