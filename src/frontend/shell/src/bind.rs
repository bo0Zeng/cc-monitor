//! v1.7.0：PowerShell 主动注册 (PS_PID → hwnd) 映射的握手 watcher。
//!
//! ## 信息流
//!
//! ```text
//! [PowerShell cc function]
//!   1. 设 WindowTitle = marker  ← **先**（v2 竞态修复，顺序不可换）
//!   2. 写 ps-await/<PID>.json {ps_pid, marker, proc_start}  ← **后**
//!   3. 轮询（30ms 步，deadline 3s）：await 被删 **或** registry 落地且指纹匹配 ⇒ 返回
//!
//! [本模块 BindRegistry watcher]
//!   4. notify 监听 ps-await 目录
//!   5. 新文件 → 读 marker
//!   6. EnumWindows + GetWindowTextW 找 `title.contains(marker)` 的窗口（**子串**，不是相等：
//!      WT 会往标题里塞别的东西）；找不到先重试 ≤600ms（12 × 50ms）兜旧模板
//!   7. 写 ps-registry/<PID>.json {ps_pid, hwnd, owner_pid, owner_proc_start, proc_start}
//!   8. 删 ps-await/<PID>.json → PS 解除阻塞
//!
//! [SessionMap added 新 session]
//!   9. ToolHelp 拿 claude_pid 往上的进程链（中间可以隔着 ccm / cmd）
//!   10. 沿链走到终端窗口的属主为止，第一个登记过且作数的 PowerShell（[`walk_chain`]）→ HwndEntry
//!   11. 写 sid-hwnd-cache.json
//! ```
//!
//! ## 心跳清理
//!
//! 每 10s 扫一遍内存中的 ps-registry，对每个 PS_PID 调 `is_process_alive`〔散文墓碑〕，
//! 死 PS 的条目从内存 + 磁盘移除。避免长期累积。
//!
//! ## ↗ 远端那一格
//!
//! 点 ↗ 时现查（那台答「此刻谁在显示它」、本机后端按连接对到这台电脑上的进程链），本模块做最后两跳 ——
//! [`bring_chain_window`]：沿链从下往上走到终端窗口的属主为止，哪一级 PowerShell 在这张表里登记过、且作数 ⇒ 用它登记的窗口；
//! 没有就看属主那一级的窗口（好几个就照实说分不清，不挑）；交 [`bring_found_window`] 三重指纹校验 ＋ 拉到前台。

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// cc function 写入 ps-await/<PID>.json 的内容。
#[derive(Debug, Deserialize, Clone)]
pub struct AwaitRequest {
    pub ps_pid: u32,
    pub marker: String,
    /// Win32 FILETIME 字符串（PS 端 `[Process].StartTime.ToFileTime()` 输出）。
    /// **跟 Claude Code 写的 `procStart`（.NET 本地 ticks）不同单位**——前者自 1601-01-01 UTC，
    /// 后者自 0001-01-01 Local。详 `utils::FileTime`。
    pub proc_start: String,
}

/// 写入 ps-registry/<PID>.json 的内容；同时缓存到 BindRegistry.by_ps_pid 内存。
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
/// session_id → 拉前所需信息（持久化到 sid-hwnd-cache.json）。
/// 跟 HwndEntry 几乎一样，但带 session 维度的快照（hwnd 复用 / PID 复用校验靠这些字段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidHwndBinding {
    pub hwnd: isize,
    pub owner_pid: u32,
    pub owner_proc_start: u64,
    pub ps_pid: u32,
    pub ps_proc_start: String,
    pub title_at_bind: String,
    pub registered_at: i64,
}

/// 全局 ps-pid → hwnd 注册表。Arc<Self> 给 SessionMap 持有用。
pub struct BindRegistry {
    monitor_data_dir: PathBuf,
    by_ps_pid: Arc<RwLock<HashMap<u32, HwndEntry>>>,
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
        });

        Self::spawn_await_watcher(me.clone(), await_dir);
        Self::spawn_heartbeat(me.clone());
        // 挂上「monitor 起来了 / 还活着」那两样：PowerShell 接入块后台那一份等到它才去认领窗口（不定时醒、不动窗口标题），
        // 在 monitor 起来之前就开着的 PowerShell 也由此在它起来时补上登记。
        crate::platform::proc::hold_monitor_marks(
            shell_quote_core::MONITOR_ALIVE_NAME,
            shell_quote_core::MONITOR_UP_NAME,
        );
        me
    }

    /// 正常退出时调：把「monitor 起来了」复位，之后新开的 PowerShell 不会去找一个已经不在的 monitor。
    pub fn going_away() {
        crate::platform::proc::lower_monitor_up_mark();
    }

    /// 查 ps_pid 对应的 hwnd entry。SessionMap 在新 session 加入时调。
    pub fn lookup_hwnd_for_ps(&self, ps_pid: u32) -> Option<HwndEntry> {
        self.by_ps_pid.read().get(&ps_pid).cloned()
    }

    /// 当前注册的 PS 数量（UI 状态显示用）
    pub fn registration_count(&self) -> usize {
        self.by_ps_pid.read().len()
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

/// 把「扫到的那个窗口」＋「await 请求」组装成一条绑定。
/// 平台无关（Win32 那一跳全在 [`find_window_by_marker_substr`] 里）。
fn entry_from_marker_hit(req: &AwaitRequest, hit: MarkerHit, owner_proc_start: u64) -> HwndEntry {
    HwndEntry {
        ps_pid: req.ps_pid,
        hwnd: hit.hwnd,
        owner_pid: hit.owner_pid,
        owner_proc_start,
        ps_proc_start: req.proc_start.clone(),
        title_at_bind: hit.title,
        registered_at: crate::utils::now_ms(),
    }
}

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

    // 启动时也扫一遍现有的（应对 monitor 启动前 PS 已写了 await 文件）
    drain_await_dir(&this, &await_dir);

    while let Ok(_evt) = rx.recv() {
        drain_await_dir(&this, &await_dir);
    }
}

/// 处理 await_dir 下所有 *.json：读 marker → 找窗口 → 写 registry → 删 await
fn drain_await_dir(this: &BindRegistry, await_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(await_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.extension().map_or(false, |e| e == "json") {
            continue;
        }
        process_await_file(this, &p);
    }
}

fn process_await_file(this: &BindRegistry, await_file: &Path) {
    let raw = match std::fs::read_to_string(await_file) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("bind: read {} failed: {e}", await_file.display());
            return;
        }
    };
    // v1.7.8：PS 5.1 `Out-File -Encoding utf8` 写 UTF-8 BOM（前 3 字节 EF BB BF），
    // serde_json 不剥 BOM 直接解析失败 → process_await_file 早早 return，
    // find_window_for_marker / ps-registry 全没机会跑。这是 v1.7.0-1.7.7 整个
    // cc 集成"装上没用"的真凶。防御性剥 BOM 兜任何 UTF-8 输入。
    let raw = raw.trim_start_matches('\u{feff}');
    let req: AwaitRequest = match serde_json::from_str(raw) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("bind: parse {} failed: {e}", await_file.display());
            // 解析失败也要删掉，避免反复触发
            let _ = std::fs::remove_file(await_file);
            return;
        }
    };

    // v2.22 竞态修复:老模板(profile 块 v1)是**先写 await 文件、后设窗口标题**——
    // notify 在文件落地瞬间触发本函数,首次 EnumWindows 时 marker 大概率还没设上。
    // 立刻删 await 走失败路径会让 PS 端绑定成败全凭时序运气(实测:每个新 shell
    // 首次 cc 固定烧满超时)。找不到先短暂重试(≤600ms,50ms 步),给 PS 设标题的
    // 窗口;新模板(v2)已反转顺序,首次即中,重试是对旧模板/慢标题传播的兜底。
    let mut found = find_window_for_marker(&req);
    if found.is_none() {
        for _ in 0..12 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            found = find_window_for_marker(&req);
            if found.is_some() {
                break;
            }
        }
    }
    let entry = match found {
        Some(e) => e,
        None => {
            tracing::warn!(
                "bind: no window found with marker={:?} ps_pid={} (retried 600ms)",
                req.marker,
                req.ps_pid
            );
            // 找不到窗口也要清 await，让 PS 解除阻塞超时
            let _ = std::fs::remove_file(await_file);
            return;
        }
    };

    // 写到 ps-registry/<PID>.json
    let registry_file = this.registry_dir().join(format!("{}.json", req.ps_pid));
    if let Err(e) = host_core::atomic_write_json(&registry_file, &entry) {
        tracing::warn!(
            "bind: write registry {} failed: {e}",
            registry_file.display()
        );
        let _ = std::fs::remove_file(await_file);
        return;
    }

    // 更新内存缓存
    this.by_ps_pid.write().insert(req.ps_pid, entry.clone());

    tracing::info!(
        "bind: registered ps_pid={} hwnd={:#x} owner_pid={} title={:?}",
        req.ps_pid,
        entry.hwnd,
        entry.owner_pid,
        entry.title_at_bind
    );

    // 最后删 await 文件，解除 PS 阻塞
    if let Err(e) = std::fs::remove_file(await_file) {
        tracing::warn!("bind: remove await {} failed: {e}", await_file.display());
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

fn find_window_for_marker(req: &AwaitRequest) -> Option<HwndEntry> {
    let m = find_window_by_marker_substr(&req.marker)?;
    // FileTime → u64（HwndEntry.owner_proc_start 仍 wire u64 保兼容；0 表示拿不到）
    let owner_proc_start = crate::platform::pid::creation_filetime(m.owner_pid)
        .map(|ft| ft.0)
        .unwrap_or(0);

    // 组装那一步是**平台无关**的（见 `entry_from_marker_hit` 头注：写在这里的话
    // 那一格在 Linux 门禁上一条判据都够不到）。
    Some(entry_from_marker_hit(req, m, owner_proc_start))
}

/// ↗ 一次的结局（闭集；界面按 `kind` 排版，句子在文案表）。分不清是哪个窗口时**不挑一个切**：照实说拉不了、带候选个数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FrontOutcome {
    /// 窗口到前面了。
    Switched,
    /// 同一个终端程序开着几个窗口、这个终端没登记 ⇒ 分不清；`count` 是候选窗口个数。
    Several { program: String, count: usize },
    /// 本机会话的终端没登记（在接上终端之前开的）。
    Unbound,
    /// 认得的那个窗口已经不在了。
    WindowGone,
    /// 句柄或进程号已被别的进程复用 ⇒ 认不准（细节进日志）。
    Unclear,
    /// 找到了，系统不许抢前台（它在任务栏闪）。
    Refused,
    /// 链上有控制台 shell 却没有窗口：窗口归了链外的程序（默认终端交接给了 Windows Terminal）。`program` 是开着那条连接的程序。
    HostedByWt { program: String },
    /// 整条链连个 shell 都没有：真在后台。
    NoWindow { program: String },
    /// 这台系统没有「按句柄找 / 验 / 拉前窗口」这一族。
    Unsupported,
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

/// 验证 hwnd 仍合法 + 当前 owner_pid 跟绑定时一致 + 该进程 procStart 一致。
///
/// 三样事实（窗口还在 · 属主 · 属主起始时刻）由 `platform::{hwnd, pid}` 读，三格比对留在这里；
/// 这台没有桌面窗口那一族 ⇒ `Unsupported`。
pub fn verify_binding(binding: &SidHwndBinding) -> Result<(), FrontOutcome> {
    if !crate::platform::hwnd::SUPPORTED {
        return Err(FrontOutcome::Unsupported);
    }
    verify_window(binding.hwnd, binding.owner_pid, binding.owner_proc_start)
        .map_err(VerifyMiss::outcome)
}

/// [`verify_binding`] 的本体：只看那三样（句柄 · 属主 pid · 属主起始时刻），绑定从哪来不管。
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
        // 两边都是 FileTime UTC（u64 同零点）→ 直接比 .0 即可
        let cur_proc_start = crate::platform::pid::creation_filetime(cur_owner)
            .map(|ft| ft.0)
            .unwrap_or(0);
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
    if !crate::platform::hwnd::SUPPORTED {
        return FrontOutcome::Unsupported;
    }
    if crate::platform::hwnd::bring_to_front(hwnd) {
        FrontOutcome::Switched
    } else {
        FrontOutcome::Refused
    }
}

/// 持久化的 sid → 拉前信息缓存。SessionMap 在新 session 时 record；
/// bring_terminal_to_front 时 lookup + verify_binding + activate。
pub struct SidHwndCache {
    file: PathBuf,
    by_sid: Arc<RwLock<HashMap<String, SidHwndBinding>>>,
}

impl SidHwndCache {
    pub fn load(file: PathBuf) -> Arc<Self> {
        let mut initial = HashMap::new();
        if let Ok(s) = std::fs::read_to_string(&file) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, SidHwndBinding>>(&s) {
                initial = map;
            }
        }
        tracing::info!("sid-hwnd-cache: loaded {} entries", initial.len());
        Arc::new(Self {
            file,
            by_sid: Arc::new(RwLock::new(initial)),
        })
    }

    pub fn lookup(&self, sid: &str) -> Option<SidHwndBinding> {
        self.by_sid.read().get(sid).cloned()
    }

    /// claude 新 session 出现时调：从 claude 往上沿进程链（同远端那一格的规则，[`walk_chain`]）找登记过、且作数的 PowerShell → 写绑定。
    /// claude 往往不是 PowerShell 的直接子进程（敲 cc 时中间隔着 ccm；npm 装的 claude 中间隔着 cmd）。
    /// 返回 Some 表示绑定成功，None 表示没找到（那个 PowerShell 没登记 / 还没登记完）。
    pub fn record(
        &self,
        sid: &str,
        claude_pid: u32,
        bind: &BindRegistry,
    ) -> Option<SidHwndBinding> {
        let chain: Vec<ChainLink> = crate::platform::pid::ancestors(claude_pid)
            .into_iter()
            .map(|(pid, name)| ChainLink { pid, name })
            .collect();
        let ChainHit::Registered(entry) = walk_chain(
            &chain,
            |pid| holding_registration(bind, pid),
            crate::platform::hwnd::visible_top_windows_of,
        ) else {
            return None;
        };
        let binding = SidHwndBinding {
            hwnd: entry.hwnd,
            owner_pid: entry.owner_pid,
            owner_proc_start: entry.owner_proc_start,
            ps_pid: entry.ps_pid,
            ps_proc_start: entry.ps_proc_start,
            title_at_bind: entry.title_at_bind,
            registered_at: crate::utils::now_ms(),
        };
        self.by_sid.write().insert(sid.to_string(), binding.clone());
        self.persist();
        tracing::info!(
            "sid-hwnd: bound sid={} → hwnd={:#x} (ps_pid={} owner_pid={})",
            sid,
            binding.hwnd,
            binding.ps_pid,
            binding.owner_pid
        );
        Some(binding)
    }

    pub fn forget(&self, sid: &str) {
        if self.by_sid.write().remove(sid).is_some() {
            self.persist();
            tracing::debug!("sid-hwnd: forgot sid={}", sid);
        }
    }

    /// K-W1C：**本机**一个 sid 离开活跃集这个**事实**到达时，这份缓存该变成什么样。
    ///
    /// # 为什么这一步必须住在这里
    ///
    /// 原先它整条住在 Tauri `setup` 闭包里那条 `session-changes-emitter` 线程上
    /// （拿着 `AppHandle`）—— **测不动**。于是「后端算出会话没了」到「那条绑定真的
    /// 被忘了」这一段线，本仓一条判据都没有（当时唯一碰 `forget` 的单测直接调原语，一条推送边都不经过）。
    /// 抽成方法之后，判据钉的是**行为**（事实进来、缓存变成什么样），
    /// 不是那段闭包的行号 —— 这条链哪天搬家，判据整块跟着走。
    ///
    /// # 今天的行为，逐字一句
    ///
    /// **离开活跃集的一律忘**（可重连 · 已结束 · 说不清三种去向都走这里，由 `lib.rs::session_side_effects` 调）：
    /// 本机 ↗ 只在 Windows 上有，而 Windows 没有 tmux ⇒ 本机走不到「可重连」；被顶替的旧 sid 连 attach 都接不上。
    pub fn apply_local_removal(&self, sid: &str) {
        self.forget(sid);
    }

    fn persist(&self) {
        let snapshot = self.by_sid.read().clone();
        if let Err(e) = host_core::atomic_write_json(&self.file, &snapshot) {
            tracing::warn!("sid-hwnd persist failed: {e}");
        }
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

/// 握手表里这一条还作不作数：登记的那个 PowerShell 就是此刻这个进程（起始时刻 `start_now` 与登记时对得上；读不到也不算）·
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
    /// 走到终端窗口之前，有一级在握手表里登记过、且作数（`registered` 已校验）⇒ 它登记的那一条。
    Registered(HwndEntry),
    /// 先走到一个有可见顶层窗口的进程（终端本身）⇒ 它和它的窗口们（一个或好几个）。
    Owner(&'a ChainLink, Vec<isize>),
    /// 整条链都没有。
    Nothing,
}

/// 沿进程链从下往上（开着连接 / 起会话的那个进程在前）：每一级先看握手表（`registered` 只交作数的那一条），
/// 再看它名下有没有可见顶层窗口；碰到有窗口的那一级就停 —— 终端窗口的属主以上的进程不在这个窗口里，
/// 它们登记的是别的窗口（比如从某个 PowerShell 里打开的 Windows Terminal，它上面那个 PowerShell）。读法是参数（判据喂替身）。
pub(crate) fn walk_chain<'a>(
    chain: &'a [ChainLink],
    registered: impl Fn(u32) -> Option<HwndEntry>,
    windows_of: impl Fn(u32) -> Vec<isize>,
) -> ChainHit<'a> {
    for l in chain {
        if let Some(e) = registered(l.pid) {
            return ChainHit::Registered(e);
        }
        let wins = windows_of(l.pid);
        if !wins.is_empty() {
            return ChainHit::Owner(l, wins);
        }
    }
    ChainHit::Nothing
}

/// 进程链 ⇒ 要拉的那个窗口：链上登记过的 PowerShell ⇒ 它登记的窗口（精确到窗口）；否则终端窗口的属主恰好一个窗口 ⇒ 它；
/// 好几个 ⇒ 分不清（不挑，交出候选）；整条链都没有 ⇒ 没有窗口。`start_of` 读属主的起始时刻。
pub(crate) fn pick_chain_window(
    chain: &[ChainLink],
    registered: impl Fn(u32) -> Option<HwndEntry>,
    windows_of: impl Fn(u32) -> Vec<isize>,
    start_of: impl Fn(u32) -> u64,
) -> Result<FoundWindow, FrontOutcome> {
    match walk_chain(chain, registered, windows_of) {
        ChainHit::Registered(e) => Ok(FoundWindow::of(&e)),
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
        // 链上有一个控制台 shell 却没有窗口 ⇒ 它的终端窗口归了链外的程序（Windows 默认终端把它交给了 Windows Terminal），
        // 不是在后台跑；整条链连个 shell 都没有 ⇒ 真在后台。
        ChainHit::Nothing => {
            let program = chain.first().map(|l| l.name.clone()).unwrap_or_default();
            if chain.iter().any(|l| is_console_shell(&l.name)) {
                Err(FrontOutcome::HostedByWt { program })
            } else {
                Err(FrontOutcome::NoWindow { program })
            }
        }
    }
}

/// 交互终端里的 shell（有它 ⇒ 这一串进程是开在一个终端窗口里的）。
fn is_console_shell(name: &str) -> bool {
    ["powershell.exe", "pwsh.exe", "cmd.exe"]
        .iter()
        .any(|s| name.eq_ignore_ascii_case(s))
}

/// 生产那一份「握手表里作数的那一条」：查表 ＋ 此刻的起始时刻 ＋ 窗口三重校验。
fn holding_registration(bind: &BindRegistry, pid: u32) -> Option<HwndEntry> {
    let entry = bind.lookup_hwnd_for_ps(pid)?;
    let start_now = crate::platform::pid::creation_filetime(pid).map(|ft| ft.0);
    registration_holds(&entry, start_now, |w| {
        verify_window(w.hwnd, w.owner_pid, w.owner_proc_start).is_ok()
    })
    .then_some(entry)
}

/// ↗ 远端那一格：进程链 ⇒ 握手表里登记的窗口（或终端窗口的属主那一个）⇒ 校验 ＋ 拉前。
pub fn bring_chain_window(chain: &[ChainLink], bind: &BindRegistry) -> FrontOutcome {
    if !crate::platform::hwnd::SUPPORTED {
        return FrontOutcome::Unsupported;
    }
    let picked = pick_chain_window(
        chain,
        |pid| holding_registration(bind, pid),
        crate::platform::hwnd::visible_top_windows_of,
        |pid| {
            crate::platform::pid::creation_filetime(pid)
                .map(|ft| ft.0)
                .unwrap_or(0)
        },
    );
    match picked {
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
