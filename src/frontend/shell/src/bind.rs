//! ↗ 认终端窗口：此刻显示这个会话的，是这台电脑上哪个窗口。
//!
//! ## 一条规则，本机远端同一条
//!
//! 点 ↗ 那一刻现拿一串进程（本机会话：claude 往上的进程链；远端会话：本机后端按连接对到的、开着那条连接的进程往上），
//! [`pick_chain_window`] 沿链从下往上，每一级先问「它显示在哪个窗口」（[`shell_window`]），再看它名下有没有可见顶层窗口；
//! 碰到有窗口的那一级（终端本身）就停，那一级恰好一个窗口 ⇒ 它，好几个 ⇒ 照实说分不清（不挑）。
//! 找到的窗口交 [`bring_found_window`] 三重指纹校验 ＋ 拉到前台。不缓存：每次点击现读。
//!
//! 「它显示在哪个窗口」两种读法，哪台有哪种：
//! - **Windows**：问那个进程的控制台（`platform::console::console_window`）—— Windows Terminal 里每个标签的伪控制台窗口的属主就是
//!   承载它的那个终端窗口，经典控制台就是控制台窗口自己。不要接入块、不要登记、不看是谁起的它（cc-monitor 从标签栏起的、
//!   用户自己开的一样认得准）。只认到窗口，认不到窗口里的哪个标签（Windows Terminal 没有从外部选中别人标签的接口）。
//! - **Linux（X11）**：bash / zsh 接入块（`src/shared/ccm-aliases.sh`）在本机桌面上开的 shell 里留一份 `ps-await/<进程号>.tty`
//!   （进程号 · 起始时刻 · 终端设备），不起后台、不等。认窗口归这里：monitor 起来那一刻扫一遍、之后每落一份就认一份
//!   （[`process_tty_file`]）—— 往那个终端写改标题的控制序列挂记号、按标题找窗口（EWMH）、写进握手表（`ps-registry/`），标题出栈还原。
//!
//! 远端那台还会回显窗口标签（`LC_CCM_WINDOW`，`<shell 进程号>-<起始时刻>`，接入块设的）：对得上 ⇒ 直接问那个 shell（[`labeled_window`]）。
//!
//! ## 心跳清理
//!
//! 每 10s 扫一遍内存中的握手表，对每个 shell 进程号调 `is_process_alive`〔散文墓碑〕，死了的从内存 + 磁盘移除。

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// 握手表的一条（Linux bash / zsh 那一份认出来的），写入 ps-registry/<进程号>.json；同时缓存到 BindRegistry.by_ps_pid 内存。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HwndEntry {
    pub ps_pid: u32,
    pub hwnd: isize,
    /// GetWindowThreadProcessId 拿到的窗口属主进程 PID（WT 多窗口下 = WT 主进程 PID）。
    /// 拉前时校验"窗口当前还属于这个进程"防 HWND 复用到无关进程。
    pub owner_pid: u32,
    /// owner_pid 进程的 procStart（防 PID 复用）。0 = 拿不到（不致命，留宽容）
    pub owner_proc_start: u64,
    /// PS 进程自己的 procStart（同 ps-await 里的 proc_start）。防 ps_pid 复用
    pub ps_proc_start: String,
    /// 注册时窗口的 title。后续校验时仅做 sanity check（title 会被 claude 写改）
    pub title_at_bind: String,
    /// Unix 毫秒
    pub registered_at: i64,
}
/// 全局 shell 进程号 → 窗口的握手表（Linux bash / zsh 那一份）。
pub struct BindRegistry {
    monitor_data_dir: PathBuf,
    by_ps_pid: Arc<RwLock<HashMap<u32, HwndEntry>>>,
    /// 这一趟 monitor 里已经认过一回、没认上的 bash / zsh 记录（`ps-await/*.tty`）：不再每来一个文件事件就重认一遍
    /// （那个标签页不在前面时每认一回都要在它标题上闪一下），留到下一个 monitor 起来再认。
    tty_tried: parking_lot::Mutex<std::collections::HashSet<PathBuf>>,
}

impl BindRegistry {
    /// 启动 watcher 线程 + 心跳线程。返回 Arc 给外部持有引用。
    pub fn spawn(monitor_data_dir: PathBuf) -> Arc<Self> {
        let await_dir = monitor_data_dir.join(AWAIT_SUBDIR);
        let registry_dir = monitor_data_dir.join("ps-registry");

        for d in [&await_dir, &registry_dir] {
            if let Err(e) = std::fs::create_dir_all(d) {
                tracing::warn!("create {} failed: {e}", d.display());
            }
        }

        // 启动时把磁盘上的 ps-registry/*.json 加载到内存（应对 monitor 重启后绑定丢失）
        let initial = scan_registry_dir(&registry_dir);
        tracing::info!(
            "bind: loaded {} ps-registry entries from disk",
            initial.len()
        );

        let me = Arc::new(Self {
            monitor_data_dir,
            by_ps_pid: Arc::new(RwLock::new(initial)),
            tty_tried: parking_lot::Mutex::new(std::collections::HashSet::new()),
        });

        Self::spawn_await_watcher(me.clone(), await_dir);
        Self::spawn_heartbeat(me.clone());
        me
    }

    /// 查 shell 进程号对应的那一条登记。
    pub fn lookup_hwnd_for_ps(&self, ps_pid: u32) -> Option<HwndEntry> {
        self.by_ps_pid.read().get(&ps_pid).cloned()
    }

    fn registry_dir(&self) -> PathBuf {
        self.monitor_data_dir.join("ps-registry")
    }

    fn spawn_await_watcher(this: Arc<Self>, await_dir: PathBuf) {
        if let Err(e) = std::thread::Builder::new()
            .name("bind-await-watcher".into())
            .spawn(move || {
                run_await_watcher(this, await_dir);
            })
        {
            tracing::error!("spawn bind-await-watcher failed: {e}");
        }
    }

    fn spawn_heartbeat(this: Arc<Self>) {
        if let Err(e) = std::thread::Builder::new()
            .name("bind-heartbeat".into())
            .spawn(move || {
                run_heartbeat(this);
            })
        {
            tracing::error!("spawn bind-heartbeat failed: {e}");
        }
    }
}

use shell_quote_core::AWAIT_SUBDIR;

/// 启动时扫已有 ps-registry/*.json（应对 monitor 重启）。
/// P3 归并：走 utils::scan_dir_jsons。
fn scan_registry_dir(dir: &Path) -> HashMap<u32, HwndEntry> {
    crate::utils::scan_dir_jsons(dir, |e: &HwndEntry| e.ps_pid)
}

fn run_await_watcher(this: Arc<BindRegistry>, await_dir: PathBuf) {
    let (tx, rx) = std::sync::mpsc::channel::<DebounceEventResult>();
    let mut debouncer = match new_debouncer(Duration::from_millis(50), tx) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("bind debouncer init failed: {e}");
            return;
        }
    };
    if let Err(e) = debouncer
        .watcher()
        .watch(&await_dir, RecursiveMode::NonRecursive)
    {
        tracing::error!("bind watch failed for {}: {e}", await_dir.display());
        return;
    }

    // 启动时也扫一遍现有的（monitor 起来之前开的 shell 留下的那几份）
    drain_await_dir(&this, &await_dir);

    while let Ok(_evt) = rx.recv() {
        drain_await_dir(&this, &await_dir);
    }
}

/// 处理 await_dir 下所有 *.tty（bash / zsh：见 [`process_tty_file`]）
fn drain_await_dir(this: &BindRegistry, await_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(await_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        match p.extension().and_then(|e| e.to_str()) {
            Some(TTY_RECORD_EXT) => process_tty_file(this, &p),
            _ => {}
        }
    }
}

/// 本机 bash / zsh 接入块留的那一份的后缀（`ps-await/<进程号>.tty`；写的一侧是 `src/shared/ccm-aliases.sh`）。
const TTY_RECORD_EXT: &str = "tty";

/// 本机 bash / zsh 接入块（`src/shared/ccm-aliases.sh`）在 `ps-await/<进程号>.tty` 留下的那一份：那个 shell 的进程号 ·
/// 起始时刻（`/proc/<pid>/stat` 第 22 格，与它设的 `LC_CCM_WINDOW` 同一个键）· 它的终端设备。
#[derive(Debug, Clone, Deserialize)]
pub struct TtyRecord {
    pub shell_pid: u32,
    pub proc_start: String,
    pub tty: String,
}

/// 认一份 bash / zsh 记录的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TtyClaim {
    /// 找到了那个终端窗口 ⇒ 这一条登记（写进握手表）。
    Registered(HwndEntry),
    /// 这一回没找到（那个标签页不在前面 · 这台此刻找不了窗口）⇒ 留着，下一个 monitor 起来再认。
    Keep,
    /// shell 没了 / 进程号被复用 / 已经登记过 / 记录认不出 ⇒ 扔掉，不碰那个终端。
    Drop,
}

/// 像不像一个终端设备（只认 `/dev/pts/*` · `/dev/tty*`，不许 `..`）：往别的文件里写控制序列不行。
fn looks_like_a_terminal(tty: &str) -> bool {
    (tty.starts_with("/dev/pts/") || tty.starts_with("/dev/tty")) && !tty.contains("..")
}

/// 认一份 bash / zsh 记录：那个 shell 还是它（`start_now` 与记下的起始时刻对得上）、还没登记（`holding`）⇒
/// 交 `probe` 在它的终端上挂记号标题找窗口；属主的起始时刻由 `start_of` 读。读法都是参数（判据喂替身）。
pub(crate) fn claim_tty(
    rec: &TtyRecord,
    start_now: Option<u64>,
    holding: bool,
    probe: impl FnOnce(&str, &str) -> Option<MarkerHit>,
    start_of: impl Fn(u32) -> u64,
) -> TtyClaim {
    let Ok(start) = rec.proc_start.trim().parse::<u64>() else {
        return TtyClaim::Drop;
    };
    if start_now != Some(start) || holding || !looks_like_a_terminal(&rec.tty) {
        return TtyClaim::Drop;
    }
    let marker = format!(
        "ccm-bind-{}-{}",
        rec.shell_pid,
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    match probe(&rec.tty, &marker) {
        Some(hit) => TtyClaim::Registered(HwndEntry {
            ps_pid: rec.shell_pid,
            hwnd: hit.hwnd,
            owner_pid: hit.owner_pid,
            owner_proc_start: start_of(hit.owner_pid),
            ps_proc_start: start.to_string(),
            title_at_bind: hit.title,
            registered_at: crate::utils::now_ms(),
        }),
        None => TtyClaim::Keep,
    }
}

/// 生产那一份探针：这台此刻找得了窗口才去碰终端 —— 标题入栈，挂记号、等一步、按标题找，最多 12 步（≤600ms，同握手那条）；
/// 每一步都重挂一次（shell 刚起来时提示符会把标题改回去），找到就停，最后出栈还原。
fn probe_tty_title(tty: &str, marker: &str) -> Option<MarkerHit> {
    use crate::platform::console::{tty_title, TtyTitle};
    if !crate::platform::hwnd::supported() || !tty_title(tty, TtyTitle::Push) {
        return None;
    }
    let mut hit = None;
    for _ in 0..12 {
        if !tty_title(tty, TtyTitle::Set(marker)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        hit = find_window_by_marker_substr(marker);
        if hit.is_some() {
            break;
        }
    }
    tty_title(tty, TtyTitle::Pop);
    hit
}

fn process_tty_file(this: &BindRegistry, file: &Path) {
    if this.tty_tried.lock().contains(file) {
        return;
    }
    let rec = std::fs::read_to_string(file)
        .ok()
        .and_then(|raw| serde_json::from_str::<TtyRecord>(raw.trim()).ok());
    let Some(rec) = rec else {
        tracing::warn!("bind: 认不出 {}，扔掉", file.display());
        let _ = std::fs::remove_file(file);
        return;
    };
    let claim = claim_tty(
        &rec,
        crate::platform::pid::start_stamp(rec.shell_pid),
        holding_registration(this, rec.shell_pid).is_some(),
        probe_tty_title,
        |pid| crate::platform::pid::start_stamp(pid).unwrap_or(0),
    );
    match claim {
        TtyClaim::Registered(entry) => {
            let registry_file = this.registry_dir().join(format!("{}.json", entry.ps_pid));
            if let Err(e) = host_core::atomic_write_json(&registry_file, &entry) {
                tracing::warn!(
                    "bind: write registry {} failed: {e}",
                    registry_file.display()
                );
                this.tty_tried.lock().insert(file.to_path_buf());
                return;
            }
            tracing::info!(
                "bind: registered shell_pid={} window={:#x} owner_pid={} (tty {})",
                entry.ps_pid,
                entry.hwnd,
                entry.owner_pid,
                rec.tty
            );
            this.by_ps_pid.write().insert(entry.ps_pid, entry);
            let _ = std::fs::remove_file(file);
        }
        TtyClaim::Keep => {
            this.tty_tried.lock().insert(file.to_path_buf());
        }
        TtyClaim::Drop => {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// `find_window_by_marker_substr` 命中的窗口快照（住 `platform::hwnd`，这里再导出）。
pub use crate::platform::hwnd::MarkerHit;

/// 「这个窗口是不是我们要的那个」：title **子串包含** `marker`（不是相等：WT 会往标题里塞别的东西）；空 marker 不认任何窗口。
fn title_carries_marker(title: &str, marker: &str) -> bool {
    !marker.is_empty() && title.contains(marker)
}

/// 扫一遍所有可见窗口，返回 **title 子串包含 `marker`** 的第一个窗口。
///
/// EnumWindows 那一跳（读法）住 `platform::hwnd::first_visible_window`（别处恒 `None`）；
/// 「哪个标题算命中」这条规则是上面那个 [`title_carries_marker`]，留在这里。
fn find_window_by_marker_substr(marker: &str) -> Option<MarkerHit> {
    crate::platform::hwnd::first_visible_window(marker, title_carries_marker)
}

/// ↗ 一次的结局（闭集；界面按 `kind` 排版，句子在文案表）。分不清是哪个窗口时**不挑一个切**：照实说拉不了、带候选个数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FrontOutcome {
    /// 窗口到前面了。
    Switched,
    /// 链上哪一级都认不出显示在哪个窗口，终端程序又开着几个窗口 ⇒ 分不清；`count` 是候选窗口个数。
    Several { program: String, count: usize },
    /// 本机会话的进程链上哪一级都认不出显示在哪个窗口（Linux：shell 开在接上终端之前 · 会话在 tmux 里，界面再按窗口标签找一次）。
    Unbound,
    /// 认得的那个窗口已经不在了。
    WindowGone,
    /// 句柄或进程号已被别的进程复用 ⇒ 认不准（细节进日志）。
    Unclear,
    /// 找到了，系统不许抢前台（它在任务栏闪）。
    Refused,
    /// 整条链上哪一级都没有窗口：真在后台。
    NoWindow { program: String },
    /// Wayland 会话：别的程序的窗口 cc-monitor 看不见、也切不过去（`desktop` 是桌面名，可能空）。界面给［在 cc-monitor 里打开］。
    DesktopWontSwitch { desktop: String },
    /// 这台系统没有「按句柄找 / 验 / 拉前窗口」这一族。
    Unsupported,
}

/// 这一种桌面会话上 ↗ 走不走得通：Windows · X11 ⇒ 走（`None`）；Wayland ⇒ 照实说切不了；别的 ⇒ 不支持。
pub(crate) fn refusal_of(session: &crate::platform::hwnd::DisplaySession) -> Option<FrontOutcome> {
    use crate::platform::hwnd::DisplaySession as S;
    match session {
        S::Win32 | S::X11 => None,
        S::Wayland { desktop } => Some(FrontOutcome::DesktopWontSwitch {
            desktop: desktop.clone(),
        }),
        S::Unsupported => Some(FrontOutcome::Unsupported),
    }
}

/// 此刻这台的那一句（走得通 ⇒ `None`）。
pub fn front_refusal() -> Option<FrontOutcome> {
    refusal_of(&crate::platform::hwnd::display_session())
}

/// 校验不过的两种：窗口没了 · 句柄 / 进程号被别人复用（细节只进日志）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum VerifyMiss {
    Gone,
    Reused(String),
}

impl VerifyMiss {
    fn outcome(self) -> FrontOutcome {
        match self {
            VerifyMiss::Gone => FrontOutcome::WindowGone,
            VerifyMiss::Reused(why) => {
                tracing::info!("bind: 认不准那个窗口：{why}");
                FrontOutcome::Unclear
            }
        }
    }
}

/// 找到的那个窗口还是不是它：句柄还在 · 属主 pid 没换 · 属主起始时刻没换（三样事实由 `platform::{hwnd, pid}` 读，三格比对留在这里）。
fn verify_window(hwnd_v: isize, owner_pid: u32, owner_proc_start: u64) -> Result<(), VerifyMiss> {
    use crate::platform::hwnd;
    if !hwnd::exists(hwnd_v) {
        return Err(VerifyMiss::Gone);
    }
    let cur_owner = hwnd::owner_pid(hwnd_v);
    if cur_owner != owner_pid {
        return Err(VerifyMiss::Reused(format!(
            "window handle reused: owner now {cur_owner}, bound {owner_pid}"
        )));
    }
    if owner_proc_start != 0 {
        // 两边都是这台的起始时刻戳（同一个口径，`platform::pid::start_stamp`）→ 直接比
        let cur_proc_start = crate::platform::pid::start_stamp(cur_owner).unwrap_or(0);
        if cur_proc_start != 0 && cur_proc_start != owner_proc_start {
            return Err(VerifyMiss::Reused(
                "owner pid reused: start time differs".into(),
            ));
        }
    }
    Ok(())
}

/// 把窗口拉到前台：系统答应 ⇒ `Switched`；不答应（OS 会让窗口在任务栏闪烁）⇒ `Refused`。
/// 拉法住 `platform::hwnd::bring_to_front`。
pub fn activate(hwnd: isize) -> FrontOutcome {
    if let Some(o) = front_refusal() {
        return o;
    }
    if crate::platform::hwnd::bring_to_front(hwnd) {
        FrontOutcome::Switched
    } else {
        FrontOutcome::Refused
    }
}

/// 沿进程链找到的那个窗口（属主与它的起始时刻由本进程现读）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoundWindow {
    pub hwnd: isize,
    pub owner_pid: u32,
    /// 属主进程的起始时刻（FILETIME）；0 = 拿不到（校验那一格跳过）。
    pub owner_proc_start: u64,
}

/// ↗ 远端那一格的最后一跳：找到的窗口 ⇒ 三重指纹校验（与本机那条同一段）⇒ 拉到前台。
pub fn bring_found_window(w: &FoundWindow) -> FrontOutcome {
    match verify_window(w.hwnd, w.owner_pid, w.owner_proc_start) {
        Ok(()) => activate(w.hwnd),
        Err(m) => m.outcome(),
    }
}

/// 本机后端回的进程链的一级（开着那条连接的进程在前）。`start` 那一格本进程不用（属主起始时刻自己现读）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChainLink {
    pub pid: u32,
    pub name: String,
}

/// 握手表里这一条还作不作数：登记的那个 shell 就是此刻这个进程（起始时刻 `start_now` 与登记时对得上；读不到也不算）·
/// 它登记的窗口还在且属主没换（`window_ok`）。读法是参数（判据喂替身）。
pub(crate) fn registration_holds(
    entry: &HwndEntry,
    start_now: Option<u64>,
    window_ok: impl Fn(&FoundWindow) -> bool,
) -> bool {
    let same_shell = matches!(
        (entry.ps_proc_start.trim().parse::<u64>().ok(), start_now),
        (Some(a), Some(b)) if a == b
    );
    same_shell && window_ok(&FoundWindow::of(entry))
}

impl FoundWindow {
    /// 握手表里那一条登记的窗口。
    fn of(entry: &HwndEntry) -> Self {
        FoundWindow {
            hwnd: entry.hwnd,
            owner_pid: entry.owner_pid,
            owner_proc_start: entry.owner_proc_start,
        }
    }
}

/// 沿进程链从下往上走到终端窗口那一级为止的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChainHit<'a> {
    /// 走到终端窗口之前，有一级认得出它显示在哪个窗口（`known`）⇒ 那个窗口。
    Known(FoundWindow),
    /// 先走到一个有可见顶层窗口的进程（终端本身）⇒ 它和它的窗口们（一个或好几个）。
    Owner(&'a ChainLink, Vec<isize>),
    /// 整条链都没有。
    Nothing,
}

/// 沿进程链从下往上（开着连接 / 起会话的那个进程在前）：每一级先问它显示在哪个窗口（`known`，生产那一份是 [`shell_window`]），
/// 再看它名下有没有可见顶层窗口；碰到有窗口的那一级就停 —— 终端窗口的属主以上的进程不在这个窗口里，
/// 它们显示在别的窗口（比如从某个 PowerShell 里打开的 Windows Terminal，它上面那个 PowerShell）。读法是参数（判据喂替身）。
pub(crate) fn walk_chain<'a>(
    chain: &'a [ChainLink],
    known: impl Fn(u32) -> Option<FoundWindow>,
    windows_of: impl Fn(u32) -> Vec<isize>,
) -> ChainHit<'a> {
    for l in chain {
        if let Some(w) = known(l.pid) {
            return ChainHit::Known(w);
        }
        let wins = windows_of(l.pid);
        if !wins.is_empty() {
            return ChainHit::Owner(l, wins);
        }
    }
    ChainHit::Nothing
}

/// 进程链 ⇒ 要拉的那个窗口：链上有一级认得出 ⇒ 那个窗口（精确到窗口）；否则终端窗口的属主恰好一个窗口 ⇒ 它；
/// 好几个 ⇒ 分不清（不挑，交出候选个数）；整条链都没有 ⇒ 没有窗口（说开着连接的那个程序）。`start_of` 读属主的起始时刻。
pub(crate) fn pick_chain_window(
    chain: &[ChainLink],
    known: impl Fn(u32) -> Option<FoundWindow>,
    windows_of: impl Fn(u32) -> Vec<isize>,
    start_of: impl Fn(u32) -> u64,
) -> Result<FoundWindow, FrontOutcome> {
    match walk_chain(chain, known, windows_of) {
        ChainHit::Known(w) => Ok(w),
        ChainHit::Owner(l, wins) => match wins.as_slice() {
            [h] => Ok(FoundWindow {
                hwnd: *h,
                owner_pid: l.pid,
                owner_proc_start: start_of(l.pid),
            }),
            _ => Err(FrontOutcome::Several {
                program: l.name.clone(),
                count: wins.len(),
            }),
        },
        ChainHit::Nothing => Err(FrontOutcome::NoWindow {
            program: chain.first().map(|l| l.name.clone()).unwrap_or_default(),
        }),
    }
}

/// 本机会话的那一条：同 [`pick_chain_window`]，只是整条链都没有窗口时说「没登记」（界面接着按窗口标签找：Linux 上会话在 tmux 里，
/// claude 的进程链到不了终端窗口）。
pub(crate) fn pick_local_window(
    chain: &[ChainLink],
    known: impl Fn(u32) -> Option<FoundWindow>,
    windows_of: impl Fn(u32) -> Vec<isize>,
    start_of: impl Fn(u32) -> u64,
) -> Result<FoundWindow, FrontOutcome> {
    pick_chain_window(chain, known, windows_of, start_of).map_err(|o| match o {
        FrontOutcome::NoWindow { .. } => FrontOutcome::Unbound,
        o => o,
    })
}

/// 那台回显的窗口标签（`<shell 进程号>-<起始时刻>`，接入块设的）⇒ 那个 shell 显示在哪个窗口：此刻那个进程号的起始时刻
/// （`start_of`）与标签里的对得上才问（进程号被复用不认）。几个终端按交来的顺序（最近动静在前），第一个认得出的就是它。
pub(crate) fn labeled_window(
    terminals: &[serde_json::Value],
    start_of: impl Fn(u32) -> Option<u64>,
    known: impl Fn(u32) -> Option<FoundWindow>,
) -> Option<FoundWindow> {
    terminals
        .iter()
        .filter_map(|t| t.get("window")?.as_str()?.split_once('-'))
        .filter_map(|(p, s)| Some((p.parse::<u32>().ok()?, s.parse::<u64>().ok()?)))
        .filter(|&(pid, start)| start_of(pid) == Some(start))
        .find_map(|(pid, _)| known(pid))
}

/// ↗ 远端那一格先按窗口标签找：对上了 ⇒ 校验、拉前，回那一次的结局；没有标签 / 对不上 ⇒ `None`（接着按连接对）。
pub fn bring_labeled_window(
    terminals: &[serde_json::Value],
    bind: &BindRegistry,
) -> Option<FrontOutcome> {
    if !crate::platform::hwnd::supported() {
        return None;
    }
    labeled_window(terminals, crate::platform::pid::start_stamp, |pid| {
        shell_window(bind, pid)
    })
    .map(|w| bring_found_window(&w))
}

/// 生产那一份「握手表里作数的那一条」：查表 ＋ 此刻的起始时刻 ＋ 窗口三重校验。
fn holding_registration(bind: &BindRegistry, pid: u32) -> Option<HwndEntry> {
    let entry = bind.lookup_hwnd_for_ps(pid)?;
    let start_now = crate::platform::pid::start_stamp(pid);
    registration_holds(&entry, start_now, |w| {
        verify_window(w.hwnd, w.owner_pid, w.owner_proc_start).is_ok()
    })
    .then_some(entry)
}

/// 生产那一份「这一级显示在哪个窗口」：握手表里作数的登记（Linux bash / zsh）；没有 ⇒ 它的控制台显示在哪个窗口（Windows）。
/// 属主与它的起始时刻现读。
fn shell_window(bind: &BindRegistry, pid: u32) -> Option<FoundWindow> {
    if let Some(e) = holding_registration(bind, pid) {
        return Some(FoundWindow::of(&e));
    }
    let hwnd = crate::platform::console::console_window(pid)?;
    let owner_pid = crate::platform::hwnd::owner_pid(hwnd);
    Some(FoundWindow {
        hwnd,
        owner_pid,
        owner_proc_start: crate::platform::pid::start_stamp(owner_pid).unwrap_or(0),
    })
}

/// 本机后端回的那串进程按本机现读 ⇒ 窗口（认得出 ⇒ 那个；否则属主那一级的窗口）。
fn pick_live(
    chain: &[ChainLink],
    bind: &BindRegistry,
    local: bool,
) -> Result<FoundWindow, FrontOutcome> {
    let known = |pid| shell_window(bind, pid);
    let windows_of = crate::platform::hwnd::visible_top_windows_of;
    let start_of = |pid| crate::platform::pid::start_stamp(pid).unwrap_or(0);
    if local {
        pick_local_window(chain, known, windows_of, start_of)
    } else {
        pick_chain_window(chain, known, windows_of, start_of)
    }
}

/// ↗ 远端那一格：本机后端交来的进程链 ⇒ 窗口 ⇒ 校验 ＋ 拉前。
pub fn bring_chain_window(chain: &[ChainLink], bind: &BindRegistry) -> FrontOutcome {
    if let Some(o) = front_refusal() {
        return o;
    }
    match pick_live(chain, bind, false) {
        Ok(w) => bring_found_window(&w),
        Err(o) => o,
    }
}

/// ↗ 本机那一格：点那一刻现走 claude（`claude_pid`）往上的进程链 ⇒ 窗口 ⇒ 校验 ＋ 拉前。
/// claude 往往不是 shell 的直接子进程（敲 cc 时中间隔着 ccm；npm 装的 claude 中间隔着 cmd）。
pub fn bring_local_window(claude_pid: u32, bind: &BindRegistry) -> FrontOutcome {
    if let Some(o) = front_refusal() {
        return o;
    }
    // 从它自己问起（它和它的 shell 挂在同一个控制台上；被终端直接起的那一形，父进程就是终端本身、借不到控制台）。
    // 它自己那一级的名字不进任何一句（分不清说的是终端那一级、没有窗口在本机说「没登记」）⇒ 空。
    let chain: Vec<ChainLink> = std::iter::once((claude_pid, String::new()))
        .chain(crate::platform::pid::ancestors(claude_pid))
        .map(|(pid, name)| ChainLink { pid, name })
        .collect();
    match pick_live(&chain, bind, true) {
        Ok(w) => bring_found_window(&w),
        Err(o) => o,
    }
}

fn run_heartbeat(this: Arc<BindRegistry>) {
    loop {
        std::thread::sleep(Duration::from_secs(10));
        cleanup_dead(&this);
    }
}

fn cleanup_dead(this: &BindRegistry) {
    let snapshot: Vec<(u32, HwndEntry)> = this
        .by_ps_pid
        .read()
        .iter()
        .map(|(k, v)| (*k, v.clone()))
        .collect();

    let dead: Vec<u32> = snapshot
        .into_iter()
        .filter(|(pid, _)| !crate::platform::pid::is_alive(*pid))
        .map(|(pid, _)| pid)
        .collect();

    if dead.is_empty() {
        return;
    }

    {
        let mut w = this.by_ps_pid.write();
        for pid in &dead {
            w.remove(pid);
        }
    }

    for pid in &dead {
        let p = this.registry_dir().join(format!("{}.json", pid));
        let _ = std::fs::remove_file(&p);
    }

    tracing::info!(
        "bind heartbeat: removed {} dead PS registration(s): {:?}",
        dead.len(),
        dead
    );
}

// P3 归并：本地 atomic_write_json / now_ms 已删，全部走 crate::utils。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/bind_tests.rs"]
mod tests;
