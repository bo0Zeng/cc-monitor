//! v1.7.0：PowerShell 主动注册 (PS_PID → hwnd) 映射的握手 watcher。
//!
//! ## 信息流
//!
//! ```text
//! [PowerShell cc function]
//!   1. 写 ps-await/<PID>.json {ps_pid, marker, proc_start}
//!   2. 设 WindowTitle = marker
//!   3. 轮询等 ps-await/<PID>.json 被删（800ms 超时）
//!
//! [本模块 BindRegistry watcher]
//!   4. notify 监听 ps-await 目录
//!   5. 新文件 → 读 marker
//!   6. EnumWindows + GetWindowTextW 找 title == marker 的窗口
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
//! 每 10s 扫一遍内存中的 ps-registry，对每个 PS_PID 调 `is_process_alive`，
//! 死 PS 的条目从内存 + 磁盘移除。避免长期累积。

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// cc function 写入 ps-await/<PID>.json 的内容。
#[derive(Debug, Deserialize, Clone)]
pub struct AwaitRequest {
    pub ps_pid: u32,
    pub marker: String,
    /// .NET DateTime.ToFileTime() 字符串（同 SessionInfo.proc_start 语义）
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
        let await_dir = monitor_data_dir.join("ps-await");
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

    /// 当前所有注册条目的浅快照（UI 列表展示用）
    pub fn snapshot(&self) -> Vec<HwndEntry> {
        self.by_ps_pid.read().values().cloned().collect()
    }

    fn await_dir(&self) -> PathBuf {
        self.monitor_data_dir.join("ps-await")
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

/// 启动时扫已有 ps-registry/*.json（应对 monitor 重启）。
fn scan_registry_dir(dir: &Path) -> HashMap<u32, HwndEntry> {
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().map_or(false, |e| e == "json") {
            if let Ok(s) = std::fs::read_to_string(&p) {
                if let Ok(e) = serde_json::from_str::<HwndEntry>(&s) {
                    out.insert(e.ps_pid, e);
                }
            }
        }
    }
    out
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
    let req: AwaitRequest = match serde_json::from_str(&raw) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("bind: parse {} failed: {e}", await_file.display());
            // 解析失败也要删掉，避免反复触发
            let _ = std::fs::remove_file(await_file);
            return;
        }
    };

    let entry = match find_window_for_marker(&req) {
        Some(e) => e,
        None => {
            tracing::warn!(
                "bind: no window found with marker={:?} ps_pid={}",
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
    if let Err(e) = atomic_write_json(&registry_file, &entry) {
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

#[cfg(windows)]
fn find_window_for_marker(req: &AwaitRequest) -> Option<HwndEntry> {
    use std::cell::RefCell;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    struct Match {
        hwnd: isize,
        owner_pid: u32,
        title: String,
    }

    thread_local! {
        static FOUND: RefCell<Option<Match>> = const { RefCell::new(None) };
        static MARKER: RefCell<String> = const { RefCell::new(String::new()) };
    }

    MARKER.with(|m| *m.borrow_mut() = req.marker.clone());
    FOUND.with(|f| *f.borrow_mut() = None);

    // v1.7.5 修：不再过滤 `GetWindow(hwnd, GW_OWNER) != 0` 的窗口。
    //
    // 原本继承自 v1.6.x 4-tier 算法的"只看 top-level 无 owner 窗口"过滤，
    // 在 v1.7 cc 注入式绑定下导致 bug：WindowsTerminal 的 XAML 子窗口（Microsoft.UI.Xaml.*）
    // owner != 0（owner = WT 主窗口），会被过滤掉。PowerShell 的
    // `$Host.UI.RawUI.WindowTitle` 可能同步到这些 XAML 子窗口而非 WT 主窗口
    // （取决于 WT/conhost 版本）。
    //
    // marker 字符串 = "ccm-bind-{PID}-{8 char UUID}" 极独特，不会撞别的窗口
    // title，不需要 owner=0 这个保险。
    unsafe extern "system" fn cb(hwnd: HWND, _lp: LPARAM) -> BOOL {
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }
        // v1.7.7：不再用 GetWindowTextLengthW 预查询长度。
        // 对 Microsoft.UI.Xaml.Controls / WinUI 控件（Windows Terminal 用的），
        // GetWindowTextLengthW 经常返回 0（WinRT 控件兼容 Win32 API 的 quirk），
        // 但 GetWindowTextW 直接给 buffer 调用能拿到实际 title。
        // 固定 512 buffer 跟用户端诊断脚本一致；marker 长 ≤ 50 字符肯定够。
        let title = unsafe {
            let mut buf = vec![0u16; 512];
            let n = GetWindowTextW(hwnd, &mut buf);
            if n > 0 {
                String::from_utf16_lossy(&buf[..n as usize])
            } else {
                String::new()
            }
        };
        let marker_match = MARKER.with(|m| {
            let m = m.borrow();
            !m.is_empty() && title.contains(m.as_str())
        });
        if !marker_match {
            return BOOL(1);
        }
        let mut owner_pid: u32 = 0;
        let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_pid)) };
        FOUND.with(|f| {
            *f.borrow_mut() = Some(Match {
                hwnd: hwnd.0,
                owner_pid,
                title,
            });
        });
        BOOL(0) // 找到了，停止枚举
    }

    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(0));
    }

    let m = FOUND.with(|f| f.borrow_mut().take())?;
    let owner_proc_start = process_creation_filetime(m.owner_pid).unwrap_or(0);

    Some(HwndEntry {
        ps_pid: req.ps_pid,
        hwnd: m.hwnd,
        owner_pid: m.owner_pid,
        owner_proc_start,
        ps_proc_start: req.proc_start.clone(),
        title_at_bind: m.title,
        registered_at: now_ms(),
    })
}

#[cfg(not(windows))]
fn find_window_for_marker(_req: &AwaitRequest) -> Option<HwndEntry> {
    None
}

/// 拿指定 PID 的 GetProcessTimes creation FILETIME（u64）。失败返 None。
#[cfg(windows)]
fn process_creation_filetime(pid: u32) -> Option<u64> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    if pid == 0 {
        return None;
    }
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) if !h.is_invalid() => h,
            _ => return None,
        };
        let mut creation = FILETIME::default();
        let mut exit_t = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let ok =
            GetProcessTimes(handle, &mut creation, &mut exit_t, &mut kernel, &mut user).is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return None;
        }
        Some(((creation.dwHighDateTime as u64) << 32) | (creation.dwLowDateTime as u64))
    }
}

#[cfg(not(windows))]
fn process_creation_filetime(_pid: u32) -> Option<u64> {
    None
}

/// 用 ToolHelp 拿指定 PID 的 parent_pid。失败返 None。
#[cfg(windows)]
pub fn get_parent_pid(pid: u32) -> Option<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        if snap.is_invalid() {
            return None;
        }
        let mut entry = PROCESSENTRY32 {
            dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
            ..Default::default()
        };
        let mut result = None;
        if Process32First(snap, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == pid {
                    result = Some(entry.th32ParentProcessID);
                    break;
                }
                if Process32Next(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        result
    }
}

#[cfg(not(windows))]
pub fn get_parent_pid(_pid: u32) -> Option<u32> {
    None
}

/// 验证 hwnd 仍合法 + 当前 owner_pid 跟绑定时一致 + 该进程 procStart 一致。
/// 返回 Ok(()) 表示通过，Err(reason) 描述失败原因（给 toast 用）。
#[cfg(windows)]
pub fn verify_binding(binding: &SidHwndBinding) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};
    unsafe {
        let hwnd = HWND(binding.hwnd);
        if !IsWindow(hwnd).as_bool() {
            return Err("窗口已不存在（被关闭或 HWND 被回收）".to_string());
        }
        let mut cur_owner: u32 = 0;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut cur_owner));
        if cur_owner != binding.owner_pid {
            return Err(format!(
                "HWND 复用：当前属主 PID {} ≠ 绑定时 PID {}",
                cur_owner, binding.owner_pid
            ));
        }
        if binding.owner_proc_start != 0 {
            let cur_proc_start = process_creation_filetime(cur_owner).unwrap_or(0);
            if cur_proc_start != 0 && cur_proc_start != binding.owner_proc_start {
                return Err("属主进程 PID 复用（procStart 不一致）".to_string());
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn verify_binding(_binding: &SidHwndBinding) -> Result<(), String> {
    Err("only supported on Windows".into())
}

/// 把窗口拉到前台。失败时返 Err（不致命，OS 会让窗口在任务栏闪烁）。
#[cfg(windows)]
pub fn activate(hwnd: isize) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    unsafe {
        let h = HWND(hwnd);
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        if SetForegroundWindow(h).as_bool() {
            Ok(())
        } else {
            Err(
                "SetForegroundWindow 被拒绝（窗口可能在任务栏闪烁；用户需要先点 monitor 窗口）"
                    .into(),
            )
        }
    }
}

#[cfg(not(windows))]
pub fn activate(_hwnd: isize) -> Result<(), String> {
    Err("only supported on Windows".into())
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
        let parent_pid = get_parent_pid(claude_pid)?;
        let entry = bind.lookup_hwnd_for_ps(parent_pid)?;
        let binding = SidHwndBinding {
            hwnd: entry.hwnd,
            owner_pid: entry.owner_pid,
            owner_proc_start: entry.owner_proc_start,
            ps_pid: entry.ps_pid,
            ps_proc_start: entry.ps_proc_start,
            title_at_bind: entry.title_at_bind,
            registered_at: now_ms(),
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

    fn persist(&self) {
        let snapshot = self.by_sid.read().clone();
        if let Ok(s) = serde_json::to_string_pretty(&snapshot) {
            // 复用 atomic_write_json 的语义但简化（不带 serde 因为我们手动 stringify 了）
            let tmp = self.file.with_extension("json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::remove_file(&self.file);
                let _ = std::fs::rename(&tmp, &self.file);
            }
        }
    }
}

#[cfg(windows)]
fn is_pid_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    const STILL_ACTIVE: u32 = 259;
    if pid == 0 {
        return false;
    }
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) if !h.is_invalid() => h,
            _ => return false,
        };
        let mut code: u32 = 0;
        let alive = GetExitCodeProcess(handle, &mut code).is_ok() && code == STILL_ACTIVE;
        let _ = CloseHandle(handle);
        alive
    }
}

#[cfg(not(windows))]
fn is_pid_alive(_pid: u32) -> bool {
    false
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
        .filter(|(pid, _)| !is_pid_alive(*pid))
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

/// 原子写：先写到 .tmp，再 rename。Windows 用 std::fs::rename 在目标存在时会失败，
/// 这里直接 remove 旧文件后再 rename（不严格原子，但失败概率低，能容忍）。
fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    let s = serde_json::to_string_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(&tmp, s)?;
    // Windows: rename 目标存在会 fail，先 remove
    let _ = std::fs::remove_file(path);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_await_request() {
        let raw = r#"{"ps_pid":9692,"marker":"ccm-bind-9692-abc12345","proc_start":"639150434950992340"}"#;
        let req: AwaitRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.ps_pid, 9692);
        assert_eq!(req.marker, "ccm-bind-9692-abc12345");
        assert_eq!(req.proc_start, "639150434950992340");
    }

    #[test]
    fn hwnd_entry_roundtrip() {
        let e = HwndEntry {
            ps_pid: 9692,
            hwnd: 0x12345,
            owner_pid: 37684,
            owner_proc_start: 132456789012345678,
            ps_proc_start: "639150434950992340".to_string(),
            title_at_bind: "✳ Claude Code".to_string(),
            registered_at: 1716393600000,
        };
        let s = serde_json::to_string(&e).unwrap();
        let parsed: HwndEntry = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed.ps_pid, e.ps_pid);
        assert_eq!(parsed.hwnd, e.hwnd);
        assert_eq!(parsed.title_at_bind, e.title_at_bind);
    }
}
