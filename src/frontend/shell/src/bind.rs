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
//!   9. ToolHelp 拿 claude_pid 的 parent → PS_PID
//!   10. BindRegistry::lookup_hwnd_for_ps(PS_PID) → HwndEntry
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
//! 远端会话的窗口不在这张表里：点 ↗ 时现查（那台答「此刻谁在显示它」、本机后端按连接对到这台电脑上的进程链），
//! 本模块做最后两跳 —— [`bring_chain_window`]：沿进程链找属主的窗口，交 [`bring_found_window`] 三重指纹校验 ＋ 拉到前台。

use crate::copy_table::copy_text;
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
    /// **跟 `SessionInfo.proc_start` (NetTicks) 不同单位**——前者自 1601-01-01 UTC，
    /// 后者自 0001-01-01 Local。详 `utils::FileTime` / `utils::NetTicks`。
    pub proc_start: String,
}

/// 写入 ps-registry/<PID>.json 的内容；同时缓存到 BindRegistry.by_ps_pid 内存。
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        me
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

/// 验证 hwnd 仍合法 + 当前 owner_pid 跟绑定时一致 + 该进程 procStart 一致。
/// 返回 Ok(()) 表示通过，Err(reason) 描述失败原因（给 toast 用）。
///
/// 三样事实（窗口还在 · 属主 · 属主起始时刻）由 `platform::{hwnd, pid}` 读，三格比对留在这里；
/// 这台没有桌面窗口那一族 ⇒ 原先非 Windows 那一句。
pub fn verify_binding(binding: &SidHwndBinding) -> Result<(), String> {
    verify_window(binding.hwnd, binding.owner_pid, binding.owner_proc_start)
}

/// [`verify_binding`] 的本体：只看那三样（句柄 · 属主 pid · 属主起始时刻），绑定从哪来不管。
fn verify_window(hwnd_v: isize, owner_pid: u32, owner_proc_start: u64) -> Result<(), String> {
    use crate::platform::hwnd;
    if !hwnd::SUPPORTED {
        return Err("only supported on Windows".into());
    }
    if !hwnd::exists(hwnd_v) {
        return Err(copy_text("rsBind.verify.windowGone", &[]));
    }
    let cur_owner = hwnd::owner_pid(hwnd_v);
    if cur_owner != owner_pid {
        return Err(copy_text(
            "rsBind.verify.windowReused",
            &[
                ("curOwner", &cur_owner.to_string()),
                ("ownerPid", &owner_pid.to_string()),
            ],
        ));
    }
    if owner_proc_start != 0 {
        // 两边都是 FileTime UTC（u64 同零点）→ 直接比 .0 即可
        let cur_proc_start = crate::platform::pid::creation_filetime(cur_owner)
            .map(|ft| ft.0)
            .unwrap_or(0);
        if cur_proc_start != 0 && cur_proc_start != owner_proc_start {
            return Err(copy_text("rsBind.verify.pidReused", &[]));
        }
    }
    Ok(())
}

/// 把窗口拉到前台。失败时返 Err（不致命，OS 会让窗口在任务栏闪烁）。
/// 拉法住 `platform::hwnd::bring_to_front`，这里只答「拉不动说哪句」。
pub fn activate(hwnd: isize) -> Result<(), String> {
    if !crate::platform::hwnd::SUPPORTED {
        return Err("only supported on Windows".into());
    }
    if crate::platform::hwnd::bring_to_front(hwnd) {
        Ok(())
    } else {
        Err(copy_text("rsBind.activate.refused", &[]).into())
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

    /// claude 新 session 出现时调：拿 parent_pid → 查 BindRegistry → 写绑定。
    /// 返回 Some 表示绑定成功，None 表示没找到（该 PS 未跑过 cc / cc 还没握手完）。
    pub fn record(
        &self,
        sid: &str,
        claude_pid: u32,
        bind: &BindRegistry,
    ) -> Option<SidHwndBinding> {
        let parent_pid = crate::platform::pid::parent_pid(claude_pid)?;
        let entry = bind.lookup_hwnd_for_ps(parent_pid)?;
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
            parent_pid,
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
pub fn bring_found_window(w: &FoundWindow) -> Result<(), String> {
    verify_window(w.hwnd, w.owner_pid, w.owner_proc_start)?;
    activate(w.hwnd)
}

/// 本机后端回的进程链的一级（开着那条连接的进程在前）。`start` 那一格本进程不用（属主起始时刻自己现读）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChainLink {
    pub pid: u32,
    pub name: String,
}

/// 沿进程链从下往上，第一个有可见顶层窗口的进程：恰好一个 ⇒ (窗口, 属主)；好几个 ⇒ 分不清；整条链都没有 ⇒ 没有窗口。
/// 窗口那一问是参数（判据喂替身）。
pub(crate) fn pick_chain_window(
    chain: &[ChainLink],
    windows_of: impl Fn(u32) -> Vec<isize>,
) -> Result<(isize, u32), String> {
    for l in chain {
        match windows_of(l.pid).as_slice() {
            [] => continue,
            [h] => return Ok((*h, l.pid)),
            _ => {
                return Err(copy_text(
                    "rsBind.front.severalWindows",
                    &[("name", &l.name)],
                ))
            }
        }
    }
    let name = chain.first().map(|l| l.name.as_str()).unwrap_or_default();
    Err(copy_text(
        "rsBind.front.noWindow",
        &[("name", &name.to_string())],
    ))
}

/// ↗ 远端那一格：进程链 ⇒ 属主的那个窗口 ⇒ 校验 ＋ 拉前。
pub fn bring_chain_window(chain: &[ChainLink]) -> Result<(), String> {
    if !crate::platform::hwnd::SUPPORTED {
        return Err("only supported on Windows".into());
    }
    let (hwnd, owner_pid) =
        pick_chain_window(chain, crate::platform::hwnd::visible_top_windows_of)?;
    let owner_proc_start = crate::platform::pid::creation_filetime(owner_pid)
        .map(|ft| ft.0)
        .unwrap_or(0);
    bring_found_window(&FoundWindow {
        hwnd,
        owner_pid,
        owner_proc_start,
    })
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
