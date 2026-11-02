//! ↗ 认终端窗口：此刻显示这个会话的，是这台电脑上哪个窗口。
//!
//! ## 一条规则，本机远端同一条
//!
//! 点 ↗ 那一刻本机后端答一次这条会话在这台电脑上的哪儿（`session-window`：窗口标签 · 进程链；本机会话的链从显示它的 tmux 客户端 · 它自己起，
//! 远端会话的链是开着那条连接的进程往上），[`bring_session_window`] 先按标签、再沿链：
//! [`pick_chain_window`] 沿链从下往上，每一级先问「它显示在哪个窗口」（[`look_of`]），再看它名下有没有可见顶层窗口；
//! 碰到有窗口的那一级（终端本身）就停，那一级恰好一个窗口 ⇒ 它，好几个 ⇒ 照实说分不清（不挑）。
//! 找到的窗口交 [`bring_found_window`] 三重指纹校验 ＋ 拉到前台。不缓存：每次点击现读。
//!
//! 「它显示在哪个窗口」两种读法，哪台有哪种：
//! - **Windows**：问那个进程的控制台（本二进制的一次性小助手借着问，`platform::console::console_looks`）—— Windows Terminal 里每个标签的伪控制台窗口的属主就是
//!   承载它的那个终端窗口，经典控制台就是控制台窗口自己。不要接入块、不要登记、不看是谁起的它（cc-monitor 从标签栏起的、
//!   用户自己开的一样认得准）。Windows Terminal 的窗口里再切到那个标签页（[`pick_tab`]）：借那个会话的控制台挂一个记号标题，
//!   UI Automation 在那个窗口的标签页里找名字带记号的、选中，原标题写回；认不出 ⇒ 只切窗口、照实说为什么。
//! - **Linux（X11）**：bash / zsh 接入块（`src/shared/ccm-aliases.sh`）在本机桌面上开的 shell 里留一份 `ps-await/<进程号>.tty`
//!   （进程号 · 起始时刻 · 终端设备），不起后台、不等。认窗口归这里：monitor 起来那一刻扫一遍、之后每落一份就认一份
//!   （[`process_tty_file`]）—— 往那个终端写改标题的控制序列挂记号、按标题找窗口（EWMH）、写进握手表（`ps-registry/`），标题出栈还原。
//!
//! 窗口标签（`LC_CCM_WINDOW`，`<shell 进程号>-<起始时刻>`，接入块设的，终端带着它）：对得上 ⇒ 直接问那个 shell（[`labeled_window`]）。
//!
//! ## shell 退了，登记就没了
//!
//! 握手表里每一条（monitor 起来时从盘上读回来的、之后认上的）都挂一个退出看守（`pidwatch_core`：内核在那个 shell 退出的那一刻叫醒，
//! 不扫、不轮询）：一醒就从内存与盘上删那一条（[`forget`]）。挂的那一刻那个 shell 已经不在、或进程号已被别人复用（起始时刻对不上）⇒ 当场删。

use crate::platform::own_fs::OwnState;
use copy_core::{Discard, Why};
use notify::RecursiveMode;
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
    /// 数据目录的句柄：握手表与等待记录都在它底下，写与删只经它。空表（[`BindRegistry::empty`]）没有它，一个字节不碰盘。
    own: Option<OwnState>,
    by_ps_pid: Arc<RwLock<HashMap<u32, HwndEntry>>>,
    /// 这一趟 monitor 里已经认过一回、没认上的 bash / zsh 记录（`ps-await/*.tty`）：不再每来一个文件事件就重认一遍
    /// （那个标签页不在前面时每认一回都要在它标题上闪一下），留到下一个 monitor 起来再认。
    tty_tried: parking_lot::Mutex<std::collections::HashSet<PathBuf>>,
}

impl BindRegistry {
    /// 启动 watcher 线程，读回来的每一条挂上退出看守。返回 Arc 给外部持有引用。
    /// 这台没有 bash / zsh 终端记录那一套（[`crate::platform::console::TTY_RECORDS`]：只有 Linux 有）⇒ 一张空表，不建目录、不读盘、不起线程
    /// （Windows 按控制台认窗口，这张表在那里永远是空的）。
    pub fn spawn(own: OwnState) -> Arc<Self> {
        if !crate::platform::console::TTY_RECORDS {
            return Self::empty();
        }
        let await_dir = own.root().join(AWAIT_SUBDIR);
        let registry_dir = own.root().join(REGISTRY_SUBDIR);

        for d in [AWAIT_SUBDIR, REGISTRY_SUBDIR] {
            if let Err(e) = own.dir(d) {
                tracing::warn!("bind: {e}");
            }
        }

        // 启动时把磁盘上的 ps-registry/*.json 加载到内存（应对 monitor 重启后绑定丢失）
        let initial = scan_registry_dir(&registry_dir);
        tracing::info!(
            "bind: loaded {} ps-registry entries from disk",
            initial.len()
        );

        let me = Arc::new(Self {
            own: Some(own),
            by_ps_pid: Arc::new(RwLock::new(HashMap::new())),
            tty_tried: parking_lot::Mutex::new(std::collections::HashSet::new()),
        });
        for entry in initial.into_values() {
            keep(&me, entry);
        }

        Self::spawn_await_watcher(me.clone(), await_dir);
        me
    }

    /// 一张空表：不建目录、不读盘、不起线程（没有终端记录那一套的平台 · 诊断子命令那一趟用）。
    pub fn empty() -> Arc<Self> {
        Arc::new(Self {
            own: None,
            by_ps_pid: Arc::new(RwLock::new(HashMap::new())),
            tty_tried: parking_lot::Mutex::new(std::collections::HashSet::new()),
        })
    }

    /// 查 shell 进程号对应的那一条登记。
    pub fn lookup_hwnd_for_ps(&self, ps_pid: u32) -> Option<HwndEntry> {
        self.by_ps_pid.read().get(&ps_pid).cloned()
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
}

/// 把一条登记放进内存、给它那个 shell 挂上退出看守（读回来的、新认上的都走这一处）。
/// 看守在挂的那一刻就可能叫醒（shell 已不在 · 起始时刻对不上）⇒ 先放进去再挂，醒了删的就是它。
fn keep(this: &Arc<BindRegistry>, entry: HwndEntry) {
    let pid = entry.ps_pid;
    let recorded = entry.ps_proc_start.clone();
    this.by_ps_pid.write().insert(pid, entry);
    let weak = Arc::downgrade(this);
    let mine = recorded.clone();
    pidwatch_core::watch_pid_until_exit(
        pid,
        // 开到之后才读（挡「记下 → 开」之间的复用）。
        move || same_shell(&mine, crate::platform::pid::start_stamp(pid)),
        move || {
            if let Some(this) = weak.upgrade() {
                forget(&this, pid, &recorded);
            }
        },
    );
}

/// 那个 shell 退了：删内存与盘上那一条 —— 只删还是**它**的那一条（同一个进程号后来又登记了一个新 shell ⇒ 不碰）。
fn forget(this: &BindRegistry, pid: u32, recorded: &str) {
    {
        let mut w = this.by_ps_pid.write();
        if w.get(&pid).map(|e| e.ps_proc_start.as_str()) != Some(recorded) {
            return;
        }
        w.remove(&pid);
    }
    // 死进程留下的登记：删不掉下次重扫再撤。
    if let Some(own) = &this.own {
        own.remove(&format!("{REGISTRY_SUBDIR}/{pid}.json"))
            .discard(Why::CleanupAfterFailure);
    }
    tracing::info!("bind: shell {pid} 退了，删掉它的窗口登记");
}

/// 记下的起始时刻与此刻读到的对得上 = 还是那个 shell（进程号没被复用）。读不到 ⇒ 不是。
fn same_shell(recorded: &str, start_now: Option<u64>) -> bool {
    matches!(
        (recorded.trim().parse::<u64>().ok(), start_now),
        (Some(a), Some(b)) if a == b
    )
}

use shell_quote_core::AWAIT_SUBDIR;

/// 握手表那一层（数据目录底下）。
const REGISTRY_SUBDIR: &str = "ps-registry";

/// 启动时扫已有 ps-registry/*.json（应对 monitor 重启）。
/// P3 归并：走 utils::scan_dir_jsons。
fn scan_registry_dir(dir: &Path) -> HashMap<u32, HwndEntry> {
    crate::utils::scan_dir_jsons(dir, |e: &HwndEntry| e.ps_pid)
}

type AwaitEars = (
    notify::RecommendedWatcher,
    std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
);

/// await 目录那道耳朵（notify 原生监听）：每一声原样进 channel，叫不叫醒由 [`wakes`] 判。
fn await_ears(await_dir: &Path) -> notify::Result<AwaitEars> {
    use notify::Watcher as _;
    let (tx, rx) = std::sync::mpsc::channel();
    let mut ears = notify::recommended_watcher(tx)?;
    ears.watch(await_dir, RecursiveMode::NonRecursive)?;
    Ok((ears, rx))
}

/// 这一声要不要再扫一遍 await 目录：「打开了」不算。notify 7 起 inotify 也报打开，不滤的话 [`drain_await_dir`]
/// 一读目录 · 读记录就又叫醒自己，空转不停。
fn wakes(res: &notify::Result<notify::Event>) -> bool {
    !res.as_ref().is_ok_and(|ev| {
        matches!(
            ev.kind,
            notify::EventKind::Access(notify::event::AccessKind::Open(_))
        )
    })
}

fn run_await_watcher(this: Arc<BindRegistry>, await_dir: PathBuf) {
    let (_ears, rx) = match await_ears(&await_dir) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("bind watch failed for {}: {e}", await_dir.display());
            return;
        }
    };

    // 启动时也扫一遍现有的（monitor 起来之前开的 shell 留下的那几份）
    drain_await_dir(&this, &await_dir);

    // 记录由接入块先写 `.part` 再改名落成（不会读到半份）⇒ 不去抖，来一声扫一遍；扫是幂等的。
    for res in rx.iter() {
        if wakes(&res) {
            drain_await_dir(&this, &await_dir);
        }
    }
}

/// 处理 await_dir 下所有 *.tty（bash / zsh：见 [`process_tty_file`]）
fn drain_await_dir(this: &Arc<BindRegistry>, await_dir: &Path) {
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
        #[expect(
            clippy::disallowed_methods,
            reason = "本机 shell 那一份记录挂上记号标题之后等它传到窗口标题上：50ms 一步、至多 12 步（≤600ms），找到就停；只在 monitor 起来与新记录落地那一下跑。终端标题变更没有给外部订的事件，有了就去掉"
        )]
        std::thread::sleep(Duration::from_millis(50));
        hit = find_window_by_marker_substr(marker);
        if hit.is_some() {
            break;
        }
    }
    tty_title(tty, TtyTitle::Pop);
    hit
}

fn process_tty_file(this: &Arc<BindRegistry>, file: &Path) {
    if this.tty_tried.lock().contains(file) {
        return;
    }
    let rec = std::fs::read_to_string(file)
        .ok()
        .and_then(|raw| serde_json::from_str::<TtyRecord>(raw.trim()).ok());
    let Some(rec) = rec else {
        tracing::warn!("bind: 认不出 {}，扔掉", file.display());
        // 用完 / 认不出 / 作废就删；删不掉下一个 monitor 起来按身份再核、再删（下同）。
        drop_record(this, file);
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
            let Some(own) = &this.own else {
                return;
            };
            let registry_file = format!("{REGISTRY_SUBDIR}/{}.json", entry.ps_pid);
            let written = serde_json::to_vec_pretty(&entry)
                .map_err(std::io::Error::other)
                .and_then(|body| own.write(&registry_file, &body));
            if let Err(e) = written {
                tracing::warn!("bind: write registry failed: {e}");
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
            keep(this, entry);
            drop_record(this, file);
        }
        TtyClaim::Keep => {
            this.tty_tried.lock().insert(file.to_path_buf());
        }
        TtyClaim::Drop => {
            drop_record(this, file);
        }
    }
}

/// 删掉等待目录里的那一份记录（经数据目录的句柄）；删不掉下一个 monitor 起来按身份再核、再删。
fn drop_record(this: &BindRegistry, file: &Path) {
    let (Some(own), Some(name)) = (&this.own, file.file_name().and_then(|n| n.to_str())) else {
        return;
    };
    own.remove(&format!("{AWAIT_SUBDIR}/{name}"))
        .discard(Why::CleanupAfterFailure);
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

// ↗ 一次的结局（`FrontOutcome`）与它的两种原因住 `face_core::front`：浮层那一张也在那里写（[`FrontOutcome::told`]），壳只判窗口的事。
// 分不清的原因由 [`several_why`] 判，标签页没切成的原因由 [`pick_tab`] · [`tab_step`] 判。
use face_core::front::{FrontOutcome, SeveralWhy, TabMiss};

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

/// 拉前之后算不算切过去了：系统答应了（`asked`）、而且此刻前台确实是它（`front_now`，答应之后才读）⇒ `Switched`；
/// 否则 `Refused`（OS 会让窗口在任务栏闪烁）。真机上见过 `SetForegroundWindow` 回成功、前台却没换。
pub(crate) fn landed(
    asked: bool,
    front_now: impl FnOnce() -> Option<isize>,
    hwnd: isize,
) -> FrontOutcome {
    if asked && front_now() == Some(hwnd) {
        FrontOutcome::Switched
    } else {
        FrontOutcome::Refused
    }
}

/// 把窗口拉到前台、核过前台（[`landed`]）。拉法与读前台住 `platform::hwnd`。
pub fn activate(hwnd: isize) -> FrontOutcome {
    if let Some(o) = front_refusal() {
        return o;
    }
    let asked = crate::platform::hwnd::bring_to_front(hwnd);
    landed(
        asked,
        || crate::platform::hwnd::foreground_settled(hwnd),
        hwnd,
    )
}

/// 切标签页那一步的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabPick {
    /// 选中了带记号的那个标签页。
    Selected,
    /// 窗口里只有一个标签（或没有标签栏）：不用切。
    Single,
    Miss(TabMiss),
}

/// 切标签页的读法（生产那一份住 `platform/console.rs` 的小助手里：UI Automation 读标签页 ＋ 借着那个会话的控制台改标题）。
pub(crate) trait TabReader {
    /// 此刻那个窗口里各标签页的名字（从左到右）；读不到 ⇒ `None`。
    fn names(&mut self) -> Option<Vec<String>>;
    /// 把那个会话的控制台标题换成 `title`，回原来的；换不了 ⇒ `None`。
    fn mark(&mut self, title: &str) -> Option<String>;
    /// 写回原标题。
    fn restore(&mut self, old: &str);
    /// 选中上一次 [`Self::names`] 读到的第 `i` 个；选不了 ⇒ `false`。
    fn select(&mut self, i: usize) -> bool;
    /// 两次读之间等一步（生产 50 ms）。
    fn wait(&mut self);
    /// 这一趟的预算还有没有剩（生产：小助手起来之后 2 s，`TAB_BUDGET`）。
    fn time_left(&self) -> bool;
}

/// 现铸一个记号标题（`ccm-front-` ＋ 8 位）。
pub fn front_marker() -> String {
    format!(
        "ccm-front-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    )
}

/// 名字带记号的那个标签页是第几个（第一个带的；空记号不认）。
pub(crate) fn marked_tab(names: &[String], marker: &str) -> Option<usize> {
    names.iter().position(|n| title_carries_marker(n, marker))
}

/// 切到挂着记号的那个标签页：标签页读不到 ⇒ 读不到标签页；至多一个 ⇒ 不用切（不挂记号）；挂上记号（挂不上 ⇒ 读不到控制台），
/// 再读至多 `tries` 次（标签名跟上控制台标题要几十到两百多毫秒），带记号的那个 ⇒ 选中（选不了 ⇒ 读不到标签页）；
/// 读满了也没有 ⇒ 标签名不随程序标题变化；时候到了（读得太慢：真机上刚开的窗口头几次读要几秒）⇒ 读不到标签页，
/// 挂记号之前时候就到了 ⇒ 不挂。挂上了就一定写回原标题（不论找没找到）。
pub(crate) fn pick_tab(r: &mut impl TabReader, marker: &str, tries: usize) -> TabPick {
    match r.names() {
        None => return TabPick::Miss(TabMiss::NoUia),
        Some(n) if n.len() <= 1 => return TabPick::Single,
        Some(_) if !r.time_left() => return TabPick::Miss(TabMiss::NoUia),
        Some(_) => {}
    }
    let Some(old) = r.mark(marker) else {
        return TabPick::Miss(TabMiss::NoConsole);
    };
    let mut hit = None;
    let mut out_of_time = false;
    for i in 0..tries {
        if i > 0 {
            r.wait();
            if !r.time_left() {
                out_of_time = true;
                break;
            }
        }
        if let Some(at) = r.names().and_then(|n| marked_tab(&n, marker)) {
            hit = Some(at);
            break;
        }
    }
    let picked = match hit {
        Some(at) if r.select(at) => TabPick::Selected,
        Some(_) => TabPick::Miss(TabMiss::NoUia),
        None if out_of_time => TabPick::Miss(TabMiss::NoUia),
        None => TabPick::Miss(TabMiss::TitleIgnored),
    };
    r.restore(&old);
    picked
}

/// 拉前之前那一步：要不要试切标签页。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabStep {
    /// 不试（不是 Windows Terminal：经典控制台没有标签，别的终端程序不试 · 终端那一级认出的窗口只开着一个标签：不用切）。
    Skip,
    /// 借这个进程的控制台挂记号试。
    Try(u32),
    /// 试不了，原因在此。
    Miss(TabMiss),
}

/// 只在 Windows Terminal 的窗口（`is_wt`）、而且窗口是借某一级的控制台认出来的才试（记号挂在那个控制台上）；
/// 终端那一级认出的窗口 ⇒ 试不了，但它只开着一个标签（`tabs` 照进程树数，只在这一支才数）⇒ 前面显示的就是它、不用切；
/// 否则说为什么：提权 ⇒ 无权；不提权 ⇒ 读不到控制台。
pub(crate) fn tab_step(via: Via, is_wt: bool, tabs: impl Fn() -> usize) -> TabStep {
    match (is_wt, via) {
        (false, _) => TabStep::Skip,
        (true, Via::Shown(pid)) => TabStep::Try(pid),
        (true, Via::Owner { .. }) if tabs() == 1 => TabStep::Skip,
        (true, Via::Owner { elevated: true }) => TabStep::Miss(TabMiss::Elevated),
        (true, Via::Owner { elevated: false }) => TabStep::Miss(TabMiss::NoConsole),
    }
}

/// 窗口那一次的结局 ＋ 标签页没切成的原因 ⇒ 最后那一句：窗口切过去了、标签页没切成 ⇒ 已切到窗口 · 标签页未切换；别的照窗口那一句。
pub(crate) fn with_tab(front: FrontOutcome, tab: Option<TabMiss>) -> FrontOutcome {
    match (front, tab) {
        (FrontOutcome::Switched, Some(why)) => FrontOutcome::SwitchedWindow { why },
        (o, _) => o,
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

/// 沿进程链认出的窗口 ⇒ 校验 ⇒ 切标签页（[`tab_step`]：Windows Terminal 里借那一级的控制台挂记号，小助手现做）⇒ 拉前 ⇒ 最后那一句（[`with_tab`]）。
fn bring_pick(p: &Pick) -> FrontOutcome {
    let w = &p.window;
    if let Err(m) = verify_window(w.hwnd, w.owner_pid, w.owner_proc_start) {
        return m.outcome();
    }
    let is_wt = crate::platform::wt_tabs::is_terminal_window(w.hwnd);
    let tab = match tab_step(p.via, is_wt, || {
        crate::platform::wt_tabs::tab_count(w.owner_pid)
    }) {
        TabStep::Skip => None,
        TabStep::Miss(why) => Some(why),
        TabStep::Try(pid) => match crate::platform::console::pick_tab(pid, w.hwnd) {
            TabPick::Miss(why) => Some(why),
            TabPick::Selected | TabPick::Single => None,
        },
    };
    with_tab(activate(w.hwnd), tab)
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
    same_shell(&entry.ps_proc_start, start_now) && window_ok(&FoundWindow::of(entry))
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

/// 链上一级「它显示在哪个窗口」读到的样子（生产那一份是 [`look_of`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Look {
    /// 认得出：就是这个窗口。
    Window(FoundWindow),
    /// Linux：握手表里没有它（shell 没登记）。
    Unregistered,
    /// Windows：借到了它的控制台，那个控制台却不显示在任何可见窗口里。
    Hidden,
    /// Windows：借它的控制台被系统拒了。
    Denied,
    /// 它没有控制台 / 进程已经没了。
    NoConsole,
}

/// 找到的窗口是怎么认出来的：链上某一级显示在它里面（记下那一级，切标签页要借它的控制台）· 或终端那一级恰好一个窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Via {
    Shown(u32),
    /// `elevated`：终端那一级是提权的。
    Owner {
        elevated: bool,
    },
}

/// 进程链 ⇒ 要拉的那个窗口和它是怎么认出来的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pick {
    pub window: FoundWindow,
    pub via: Via,
}

/// 沿进程链从下往上走到终端窗口那一级为止的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
enum ChainHit<'a> {
    /// 走到终端窗口之前，有一级（`pid`）认得出它显示在哪个窗口 ⇒ 那个窗口。
    Known(u32, FoundWindow),
    /// 先走到一个有可见顶层窗口的进程（终端本身）⇒ 它和它的窗口们（一个或好几个）；`looks` 是它下面那几级读到的样子。
    Owner(&'a ChainLink, Vec<isize>, Vec<Look>),
    /// 整条链都没有。
    Nothing,
}

/// 沿进程链从下往上（开着连接 / 起会话的那个进程在前）：每一级先问它显示在哪个窗口（`look`），
/// 再看它名下有没有可见顶层窗口；碰到有窗口的那一级就停 —— 终端窗口的属主以上的进程不在这个窗口里，
/// 它们显示在别的窗口（比如从某个 PowerShell 里打开的 Windows Terminal，它上面那个 PowerShell）。读法是参数（判据喂替身）。
fn walk_chain<'a>(
    chain: &'a [ChainLink],
    look: impl Fn(u32) -> Look,
    windows_of: impl Fn(u32) -> Vec<isize>,
) -> ChainHit<'a> {
    let mut looks = Vec::new();
    for l in chain {
        match look(l.pid) {
            Look::Window(w) => return ChainHit::Known(l.pid, w),
            other => looks.push(other),
        }
        let wins = windows_of(l.pid);
        if !wins.is_empty() {
            return ChainHit::Owner(l, wins, looks);
        }
    }
    ChainHit::Nothing
}

/// 分不清时的原因，按终端那一级下面每一级读到的样子判：有一级没登记 ⇒ 未登记（Linux）；终端那一级是提权的 ⇒ 提权；
/// 有一级借控制台被拒 ⇒ 读不到控制台；都借到了却不在可见窗口里 / 都没有控制台 ⇒ 这个程序不提供控制台。
pub(crate) fn several_why(looks: &[Look], owner_elevated: bool) -> SeveralWhy {
    if looks.contains(&Look::Unregistered) {
        SeveralWhy::Unregistered
    } else if owner_elevated {
        SeveralWhy::Elevated
    } else if looks.contains(&Look::Denied) {
        SeveralWhy::ConsoleUnreachable
    } else {
        SeveralWhy::Foreign
    }
}

/// 进程链 ⇒ 要拉的那个窗口：链上有一级认得出 ⇒ 那个窗口（精确到窗口）；否则终端窗口的属主恰好一个窗口 ⇒ 它；
/// 好几个 ⇒ 分不清（不挑，交出候选个数与原因）；整条链都没有 ⇒ 没有窗口（说开着连接的那个程序）。
/// `start_of` 读属主的起始时刻，`elevated_of` 读终端那一级是不是提权的（只问那一级）。
pub(crate) fn pick_chain_window(
    chain: &[ChainLink],
    look: impl Fn(u32) -> Look,
    windows_of: impl Fn(u32) -> Vec<isize>,
    start_of: impl Fn(u32) -> u64,
    elevated_of: impl Fn(u32) -> bool,
) -> Result<Pick, FrontOutcome> {
    match walk_chain(chain, look, windows_of) {
        ChainHit::Known(pid, w) => Ok(Pick {
            window: w,
            via: Via::Shown(pid),
        }),
        ChainHit::Owner(l, wins, looks) => {
            let elevated = elevated_of(l.pid);
            match wins.as_slice() {
                [h] => Ok(Pick {
                    window: FoundWindow {
                        hwnd: *h,
                        owner_pid: l.pid,
                        owner_proc_start: start_of(l.pid),
                    },
                    via: Via::Owner { elevated },
                }),
                _ => Err(FrontOutcome::Several {
                    program: l.name.clone(),
                    count: wins.len(),
                    why: several_why(&looks, elevated),
                }),
            }
        }
        ChainHit::Nothing => Err(FrontOutcome::NoWindow {
            program: chain.first().map(|l| l.name.clone()).unwrap_or_default(),
        }),
    }
}

/// 终端带的窗口标签（`<shell 进程号>-<起始时刻>`，接入块设的）⇒ 那个 shell 显示在哪个窗口：此刻那个进程号的起始时刻
/// （`start_of`）与标签里的对得上才问（进程号被复用不认）。几个标签按交来的顺序（最近动静在前），第一个认得出的就是它。
pub(crate) fn labeled_window(
    labels: &[String],
    start_of: impl Fn(u32) -> Option<u64>,
    known: impl Fn(u32) -> Option<FoundWindow>,
) -> Option<FoundWindow> {
    labels
        .iter()
        .filter_map(|l| l.split_once('-'))
        .filter_map(|(p, s)| Some((p.parse::<u32>().ok()?, s.parse::<u64>().ok()?)))
        .filter(|&(pid, start)| start_of(pid) == Some(start))
        .find_map(|(pid, _)| known(pid))
}

/// ↗ 一条命令的先后（读法是参数，判据喂替身）：什么都没得找 ⇒ `None`（界面摆本机后端那条原因）；这台切不了 ⇒ 照实说（`refusal`）；
/// 先按窗口标签找（`by_label` 对上 ⇒ 那一次的结局）；没对上 ⇒ 沿链（`by_chain`）；链也空 ⇒ `None`。
pub(crate) fn front_by(
    labels: &[String],
    chain: &[ChainLink],
    refusal: Option<FrontOutcome>,
    by_label: impl FnOnce(&[String]) -> Option<FrontOutcome>,
    by_chain: impl FnOnce(&[ChainLink]) -> FrontOutcome,
) -> Option<FrontOutcome> {
    if labels.is_empty() && chain.is_empty() {
        return None;
    }
    if refusal.is_some() {
        return refusal;
    }
    if let Some(o) = by_label(labels) {
        return Some(o);
    }
    (!chain.is_empty()).then(|| by_chain(chain))
}

/// ↗：本机后端答的「这条会话在这台电脑上的哪儿」（`session-window` 的窗口标签 · 进程链）⇒ 找窗口、校验、拉前（本机会话与远端会话同一条）。
/// 标签没对上、链又空 ⇒ `None`。
pub fn bring_session_window(
    labels: &[String],
    chain: &[ChainLink],
    bind: &BindRegistry,
) -> Option<FrontOutcome> {
    front_by(
        labels,
        chain,
        front_refusal(),
        |labels| {
            labeled_window(labels, crate::platform::pid::start_stamp, |pid| {
                holding_registration(bind, pid).map(|e| FoundWindow::of(&e))
            })
            .map(|w| bring_found_window(&w))
        },
        |chain| match pick_live(chain, bind) {
            Ok(p) => bring_pick(&p),
            Err(o) => o,
        },
    )
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

/// 生产那一份「这一级显示在哪个窗口」：握手表里作数的登记 ⇒ 那个窗口；这台有登记这一套（Linux）却没有 ⇒ 没登记；
/// 否则 ⇒ 小助手借它的控制台读到的样子（`consoles`，Windows；整条链一趟问完）。属主与它的起始时刻现读。
fn look_of(bind: &BindRegistry, consoles: &ConsoleLooks, pid: u32) -> Look {
    use crate::platform::console::ConsoleLook;
    if let Some(e) = holding_registration(bind, pid) {
        return Look::Window(FoundWindow::of(&e));
    }
    if crate::platform::console::TTY_RECORDS {
        return Look::Unregistered;
    }
    match consoles.of(pid) {
        ConsoleLook::Shown(hwnd) => {
            let owner_pid = crate::platform::hwnd::owner_pid(hwnd);
            Look::Window(FoundWindow {
                hwnd,
                owner_pid,
                owner_proc_start: crate::platform::pid::start_stamp(owner_pid).unwrap_or(0),
            })
        }
        ConsoleLook::Hidden => Look::Hidden,
        ConsoleLook::Denied => Look::Denied,
        ConsoleLook::None => Look::NoConsole,
    }
}

use crate::platform::console::ConsoleLooks;

/// 本机后端回的那串进程按本机现读 ⇒ 窗口（认得出 ⇒ 那个；否则属主那一级的窗口）。整条链的控制台起一趟小助手问完
/// （有登记这一套的那台不问：它不靠控制台）。
fn pick_live(chain: &[ChainLink], bind: &BindRegistry) -> Result<Pick, FrontOutcome> {
    let pids: Vec<u32> = if crate::platform::console::TTY_RECORDS {
        Vec::new()
    } else {
        chain.iter().map(|l| l.pid).collect()
    };
    let consoles = crate::platform::console::console_looks(&pids);
    pick_chain_window(
        chain,
        |pid| look_of(bind, &consoles, pid),
        crate::platform::hwnd::visible_top_windows_of,
        |pid| crate::platform::pid::start_stamp(pid).unwrap_or(0),
        crate::platform::pid::above_us,
    )
}

/// 真机诊断子命令 `--ccm-pick <pid>` 的命令行解析（与 `platform::console::helper_request` 同族，纯函数、判据喂）：
/// 紧跟程序名的是 `--ccm-pick`、后面恰好一个数 ⇒ 那个 claude 进程号；别的形（没参数 · 别的旗 · 没进程号 · 不是数 · 多了参数）⇒ `None`。
pub fn pick_request(args: &[String]) -> Option<u32> {
    let (flag, rest) = args.get(1..)?.split_first()?;
    if flag != "--ccm-pick" {
        return None;
    }
    match rest {
        [pid] => pid.parse::<u32>().ok(),
        _ => None,
    }
}

/// 被当成 `--ccm-pick <pid>` 诊断子命令起的那一趟：从那个进程往上现走进程链，照 ↗ 的真路子（[`bring_session_window`]：
/// 认窗口 → 终端那一级提权了没有 → 拉前 · 核前台 · 切标签页）算一次结局，印成一行 JSON（`kind` 与界面同一套），做完就退、不起 monitor。
/// 真机上核「为什么切不了 / 是不是因为提权」用：判据只能喂替身的读法，提权终端的真令牌要真机才有。
pub fn pick_main() -> Option<i32> {
    let pid = pick_request(&std::env::args().collect::<Vec<_>>())?;
    let bind = BindRegistry::empty();
    let chain: Vec<ChainLink> = std::iter::once((pid, String::new()))
        .chain(crate::platform::pid::ancestors(pid))
        .map(|(pid, name)| ChainLink { pid, name })
        .collect();
    let outcome = bring_session_window(&[], &chain, &bind).unwrap_or(FrontOutcome::NoWindow {
        program: String::new(),
    });
    println!(
        "{}",
        serde_json::to_string(&outcome).unwrap_or_else(|_| format!("{outcome:?}"))
    );
    Some(0)
}

// P3 归并：本地 atomic_write_json / now_ms 已删，全部走 crate::utils。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/bind_tests.rs"]
mod tests;
