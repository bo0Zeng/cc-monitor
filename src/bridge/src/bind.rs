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
//! 每 10s 扫一遍内存中的 ps-registry，对每个 PS_PID 调 `is_process_alive`，
//! 死 PS 的条目从内存 + 磁盘移除。避免长期累积。
//!
//! 〔第二波 T4〕它同时是 ↗ 令牌路的「死绑定周期清」（`设计/80 §0.1` ④）：
//! [`resolve_remote_front`] 按令牌查的就是这张表，死掉的窗口 10s 内出表 ⇒ ↗ 退到标题路。
//! 节拍只有这一个（归 [`BindRegistry`]）；令牌账本 [`RbindTokenBook`] 是事件驱动的，不另起节拍器。
//!
//! ## 🔴 第二种 marker 来源：**启动期令牌**（`设计/80 §8.7` 步 3，2026-09-23）
//!
//! 上面那条链一个字都没改。这一节只说**多出来的那一种 marker 来源**。
//!
//! `设计/80 §8.1` 的判断逐字：「**tmux 不是在做发现身份，是在做把身份广播到本地**」——
//! 而 `↗ 拉前终端` 需要的全部东西只是一个映射 `(sid) → (本地 HWND)`。今天那个映射靠
//! tmux 会话级 option `@ccm_sid` ＋ `set-titles-string` 合成的窗口标题，**跳五次、无回执**，
//! 而且 `container:"none"`（直连、没有 tmux）那一档**根本没有**这个映射。
//!
//! 方案 E 造的那个「本地已知、可以 join 的键」就是**启动期令牌**：monitor 起会话时
//! 铸一个 32 位小写十六进制的随机串，一路注进远端进程的 environ（`CCM_RBIND_TOKEN`，
//! 步 1 ＋ 步 2 已落地），**同一个串**同时交给本地那个终端进程当 marker。
//! ⇒ 本模块要多记的只有一件事：**`令牌 → HWND`**。
//!
//! ### 落法：**Era 2 那套一行没动，只多一个可空字段 ＋ 一个查法**
//!
//! - marker 长这样：`ccm-rbind-token-<32 hex>`（[`RBIND_TOKEN_MARKER_PREFIX`]）。
//!   握手文件 [`AwaitRequest`] 的**形状一个字节都没变** —— 令牌不是新字段，
//!   它**就是 marker 本身**（`§8.2` 逐字「marker = token」）。这样一来
//!   「窗口标题里含 marker」与「窗口标题里含令牌」是同一件事，
//!   `§8.2` 那条退路（本地 shell 在 ssh 之前自设标题）**不需要第二套解析**。
//! - [`HwndEntry`] 多一个 `rbind_token: Option<String>`（`skip_serializing_if`）
//!   ⇒ 今天写出去的 `ps-registry/<PID>.json` **逐字节等于从前**，老文件照样读得进来。
//! - 查法 [`BindRegistry::lookup_hwnd_for_token`] 是**在同一张表上扫**，
//!   **不是第二份索引**：三重指纹 · 心跳清理 · 磁盘持久化 · monitor 重启后重载
//!   —— 四件全部原样继承，不需要各写一遍失效逻辑（第二份索引最典型的病就是
//!   「主表清了、索引没清」，这里在构造上不可能发生）。
//!   表里是「这台机上还活着的 PowerShell 窗口」，个位数量级 ⇒ 线性扫不值得换索引。
//!
//! ### ⚠ 本模块**买不到**什么（别把这一段读大）
//!
//! - 〔第二波 T4 订正〕步 3 落地那天这里写的是「今天没有任何生产代码往 `ps-await` 里写一个
//!   带令牌的 marker ⇒ 本模块买到的是『接得住』，不是『已经在收』」—— **那句话到此作废**：
//!   写入方接上了，是 monitor 拉起窗口时注入的那段令牌握手前奏
//!   （`launch.rs::with_rbind_bind_prelude` ＋ `scripts/rbind-token-bind.ps1.tpl`，
//!   与 `__ccm_bind` 同一条握手、只差 marker 的形状）。
//!   ⚠ 但「那段 PowerShell 在真 Windows 上真的跑通、表里真的多了一条」**本机一格都买不到**
//!   （没有 `pwsh`、没有 Windows）；判据只钉得住交给 PowerShell 的那段**文字**。
//! - 🔴 **「↗ 真的把那个窗口拉到前台了」这一维本仓的 Linux 门禁一格都买不到**：
//!   没有图形会话、没有 Windows，`find_window_by_marker_substr` 在非 Windows 上
//!   是个恒 `None` 的桩。判据能验的是**平台无关**的那两段（marker 解令牌 · 表里查得到），
//!   Win32 那一跳仍只有 `remote_bind_finds_real_ccm_rbind_window` 那条手动 smoke。

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
    /// `设计/80 §8.7` 步 3：这次绑定的**启动期令牌**（`[0-9a-f]{32}`），
    /// 由 [`rbind_token_from_marker`] 从 marker 里解出来。
    ///
    /// **`None` 是绝大多数条目的正常取值** —— Era 2 那条链（PowerShell profile 的
    /// `__ccm_bind`）用的 marker 是 `ccm-bind-<PID>-<8hex>`，它不带令牌，
    /// 这些条目今天和从前一样只能按 `ps_pid` 查。
    ///
    /// `serde(default)` ＋ `skip_serializing_if`：**磁盘上的老 `ps-registry/*.json`
    /// 照样读得进来，新写出去的也逐字节等于从前**（additive，与 wire 上那一族同纪律）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rbind_token: Option<String>,
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

    /// `设计/80 §8.7` 步 3：**按启动期令牌查那个窗口** —— 方案 E 要的 `token → HWND`。
    ///
    /// # 为什么是「在同一张表上扫」而不是第二份索引
    ///
    /// 这张表里是**这台机上还活着的 PowerShell 窗口**（个位数量级；心跳每 10s
    /// 把死掉的清出去）。多一份 `HashMap<String, u32>` 换来的是 O(1)，代价是
    /// **多一条要各写一遍的失效路径** —— 而本模块的失效路径有四条
    /// （心跳清理 · `cleanup_dead` 的磁盘删除 · 启动时 `scan_registry_dir` 重载 ·
    /// `process_await_file` 的覆盖写）。第二份索引最典型的病就是「主表清了、索引没清」，
    /// 那个 bug 在这里**在构造上不可能发生**：只有一张表。
    ///
    /// # 形状：**入表时**就过了闸，查询这一侧刻意不再过一遍
    ///
    /// 表里的 `rbind_token` 只可能来自 [`rbind_token_from_marker`]（fail closed：
    /// 不 `trim`、不认大写、必须恰好 32 位）⇒ 表里每个键形状都确定对，
    /// 而这里是**逐字节相等**比较 ⇒ 形状不对的查询串**在构造上**命中不了任何一条。
    /// 〔死值验 09-24〕初版这里还多一道查询侧形状闸；把它删掉之后 1792 条判据
    /// **全绿** —— 它不可观测、没有判据能钉它，于是删了，而不是留一行没人守的代码。
    /// 「形状不对的查询不命中」这件事本身仍有判据
    /// （`the_launch_token_finds_its_window_handle_in_the_same_era2_table` 的 ④）。
    pub fn lookup_hwnd_for_token(&self, token: &str) -> Option<HwndEntry> {
        self.by_ps_pid
            .read()
            .values()
            .find(|e| e.rbind_token.as_deref() == Some(token))
            .cloned()
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

/// `设计/80 §8.7` 步 3：带令牌的那一种 marker 长什么样。
///
/// **前缀刻意与 Era 2 的 `ccm-bind-<PID>-<8hex>` 以及远端标题路的
/// `ccm-rbind-<sid>` 三者互不为前缀**，所以三种 marker 在同一个 `title.contains`
/// 的世界里不会互相误命中：
///
/// | 来源 | 形状 | 键 |
/// |---|---|---|
/// | Era 2 · PowerShell profile `__ccm_bind` | `ccm-bind-<PID>-<8hex>` | `ps_pid` |
/// | Era 3 · 远端 tmux 标题（`RemoteHwndCache`） | `ccm-rbind-<sid>` | `sid` |
/// | **方案 E · 启动期令牌（本节）** | `ccm-rbind-token-<32hex>` | **令牌** |
///
/// ⚠ `ccm-rbind-token-` **是** `ccm-rbind-` 的扩展，看起来像会撞 —— 不会：
/// 那一路拼的是 `ccm-rbind-<sid>`，而 sid 是 uuid（含 `-`、有大写、长 36），
/// 与 `token-<32hex>` 无论如何对不上；反向也一样（本函数要求前缀后**恰好** 32 个
/// 小写十六进制字符、后面一个字节都不许有）。两条判据各钉一头，见 `bind_tests.rs`。
pub const RBIND_TOKEN_MARKER_PREFIX: &str = "ccm-rbind-token-";

/// 握手目录 `ps-await/` 的名字（相对 monitor 数据目录）。
///
/// 〔`设计/80 §8.7` 步 3 收尾，第二波 T4〕**有两个写入方、一个读方**，三处必须同一个名字：
/// 读方 = [`BindRegistry::spawn`] 监听的目录；写入方 ① = PowerShell profile 里的 `__ccm_bind`
/// （`scripts/cc.ps1.tpl`，它在用户机器上自己拼 `ps-await`，改不动已装的那份 ⇒ 本常量**不许改值**）；
/// 写入方 ② = `launch.rs` 在拉起窗口时注入的那段令牌握手前奏（取的就是本常量）。
pub const AWAIT_SUBDIR: &str = "ps-await";

/// 带令牌的 marker：`ccm-rbind-token-<32hex>`。形状不对 ⇒ `None`（不产一个解不回来的 marker）。
///
/// 与 [`rbind_token_from_marker`] 互为逆：`rbind_token_from_marker(&rbind_token_marker(t)?) == Some(t)`。
/// **写入方只许用它拼**（`launch.rs` 的令牌握手前奏）—— 手拼一份前缀，哪天前缀改了，
/// 本地表会静默收不到任何带令牌的条目（「拉不到窗口」与「没有令牌」同形）。
pub fn rbind_token_marker(token: &str) -> Option<String> {
    rbind_token_shape_ok(token).then(|| format!("{RBIND_TOKEN_MARKER_PREFIX}{token}"))
}

/// 令牌的字符数 —— **32**。
///
/// 🔴 **这是一个跨三处的双写点**，三处必须同一个数：本常量 ·
/// 载荷侧 `backend::control::payload::RBIND_TOKEN_LEN` ·
/// 后端读侧 `control::identity_tag`。那三处各自有判据，**别在这里再抄一个 32 出去**。
const RBIND_TOKEN_LEN: usize = 32;

/// 令牌形状：恰好 [`RBIND_TOKEN_LEN`] 个**小写**十六进制字符。
///
/// 与载荷侧 `payload::rbind_token_shape_ok` 是**同一条形状**（那边是渲染前的闸，
/// 这边是绑定时的闸）。不收大写、不 `trim`、不认 `0x` 前缀 ——
/// 只有一种写法，本地这张 `token → HWND` 表与从远端 `environ` 读回来的串
/// 才能**直接相等比较**，中间不留归一化步骤（归一化是「两侧各写一遍、
/// 各写错一遍」的经典落点，`launch-dimensions.ts::isValidRbindToken` 的头注同话）。
///
/// 🔴 **本 crate 里只许有这一份**：`ssh_source::parse_frame` 读 wire 上那个
/// `rbind_token` 字段时过的也是这一条（`设计/80 §8.7` 步 3/步 4 同拍）。
/// 「本地表的键」与「wire 上读回来的串」形状一旦不同源，
/// join 就会在某些取值上静默失配 —— 而失配的表现是「拉不到窗口」，与「没有令牌」同形。
pub(crate) fn rbind_token_shape_ok(token: &str) -> bool {
    token.len() == RBIND_TOKEN_LEN
        && token
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// 从一个 marker 里解出启动期令牌。**不是**这一种 marker ⇒ `None`（Era 2 的
/// `ccm-bind-…` 走的就是这条，行为与从前一字不差）。
///
/// fail closed 到底：前缀对了但后面形状不对（少一位 / 多一位 / 有大写 / 尾巴上
/// 还挂着东西）**一律当没有**，而不是当「大概是它」。把一个形状可疑的串记进表里，
/// 会让「拉错窗口」（有键、键错了）伪装成「拉不到窗口」（没键）—— 那正是
/// `§6.2` 记的「失败归因把人引向一个不存在的问题」。
pub fn rbind_token_from_marker(marker: &str) -> Option<&str> {
    let rest = marker.strip_prefix(RBIND_TOKEN_MARKER_PREFIX)?;
    rbind_token_shape_ok(rest).then_some(rest)
}

/// 把一段**可能含启动期令牌**的文本（marker / 窗口标题）变成可以进日志的样子。
///
/// 🔴 令牌是敏感数据（`§8.6 ③`：它会进远端 `/proc/<pid>/environ`、`cmdline` 与
/// shell 历史）。本仓后端那一侧对同一条性质有一条专门的判据
/// （`identity_tag::tests::the_token_value_never_reaches_a_log_macro`），
/// **而本地这一侧此前没有** —— 因为此前本地 marker 里没有敏感值。
/// 步 3 把令牌变成 marker 之后，本函数与 `bind_tests.rs` 里那条同名判据是这一格的闸。
///
/// 只抹**值**、保留**形状**：排障要能看出「这是一个带令牌的 marker」，
/// 那一位信息不敏感，敏感的是那 32 个字符。
fn redact_marker(text: &str) -> String {
    match rbind_token_from_marker(text) {
        Some(_) => format!("{RBIND_TOKEN_MARKER_PREFIX}<32hex 已隐去>"),
        // 窗口标题是**子串**匹配（WT 会往标题里塞别的东西）⇒ 令牌可能夹在中间，
        // 上面那条 `strip_prefix` 够不着。这一支按前缀切一刀，前缀之后全抹掉。
        None => match text.find(RBIND_TOKEN_MARKER_PREFIX) {
            Some(i) => format!("{}{RBIND_TOKEN_MARKER_PREFIX}<已隐去>", &text[..i]),
            None => text.to_string(),
        },
    }
}

/// 把「扫到的那个窗口」＋「await 请求」组装成一条绑定。
///
/// **刻意是平台无关的**（Win32 那一跳全在 [`find_window_by_marker_substr`] 里）：
/// `设计/80 §8.7` 步 3 新增的那一格 —— marker 里的令牌要跟着进表 —— 如果写在
/// `#[cfg(windows)]` 的函数体里，本仓 Linux 门禁**一条判据都够不到它**
/// （`cfg(not(windows))` 那支是恒 `None` 的桩）。抽出来之后那一格在任何机器上都验得了。
fn entry_from_marker_hit(req: &AwaitRequest, hit: MarkerHit, owner_proc_start: u64) -> HwndEntry {
    HwndEntry {
        ps_pid: req.ps_pid,
        hwnd: hit.hwnd,
        owner_pid: hit.owner_pid,
        owner_proc_start,
        ps_proc_start: req.proc_start.clone(),
        title_at_bind: hit.title,
        registered_at: crate::utils::now_ms(),
        // `设计/80 §8.7` 步 3：令牌**就是 marker 本身**（`§8.2` 逐字「marker = token」）。
        // 不是这一种 marker ⇒ `None` ⇒ 这条绑定只能按 `ps_pid` 查（= 今天的行为）。
        rbind_token: rbind_token_from_marker(&req.marker).map(str::to_string),
    }
}

/// 启动时扫已有 ps-registry/*.json（应对 monitor 重启）。
/// P3 归并：走 utils::scan_dir_jsons。
///
/// ⚠ `设计/80 §8.7` 步 3 之后这一句**同时**把 `token → HWND` 那张表恢复了 ——
/// 因为根本没有第二张表（见 [`BindRegistry::lookup_hwnd_for_token`] 的头注）。
/// 「持久化」这一维是白拿的，不是又实现了一遍。
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
            // 🔴 `设计/80 §8.7` 步 3：marker 今天**可能就是令牌本身**
            //    ⇒ 原样打出去等于把敏感值写进滚动日志（`§8.6 ③`）。过一道脱敏。
            tracing::warn!(
                "bind: no window found with marker={:?} ps_pid={} (retried 600ms)",
                redact_marker(&req.marker),
                req.ps_pid
            );
            // 找不到窗口也要清 await，让 PS 解除阻塞超时
            let _ = std::fs::remove_file(await_file);
            return;
        }
    };

    // 写到 ps-registry/<PID>.json
    let registry_file = this.registry_dir().join(format!("{}.json", req.ps_pid));
    if let Err(e) = crate::utils::atomic_write_json(&registry_file, &entry) {
        tracing::warn!(
            "bind: write registry {} failed: {e}",
            registry_file.display()
        );
        let _ = std::fs::remove_file(await_file);
        return;
    }

    // 更新内存缓存
    this.by_ps_pid.write().insert(req.ps_pid, entry.clone());

    // 🔴 同上：`title_at_bind` 是**包含 marker 的那个窗口标题** ⇒ 带令牌的那一档里
    //    它含着令牌。这一行还多报一位「这条绑定有没有令牌」——那是排障时真正要知道的，
    //    而它**不泄露值**（`§8.5 ②` 要的就是这个布尔，不是那个串）。
    tracing::info!(
        "bind: registered ps_pid={} hwnd={:#x} owner_pid={} title={:?} has_rbind_token={}",
        req.ps_pid,
        entry.hwnd,
        entry.owner_pid,
        redact_marker(&entry.title_at_bind),
        entry.rbind_token.is_some()
    );

    // 最后删 await 文件，解除 PS 阻塞
    if let Err(e) = std::fs::remove_file(await_file) {
        tracing::warn!("bind: remove await {} failed: {e}", await_file.display());
    }
}

/// `find_window_by_marker_substr` 命中的窗口快照（leaf primitive 输出）。
/// `find_window_for_marker`（本地 ps-bind）和 `RemoteHwndCache::try_bind`（远端
/// sid-bind）共用这同一个 EnumWindows 扫描，只是后续组的 struct 不同。
#[cfg(windows)]
pub struct MarkerHit {
    pub hwnd: isize,
    pub owner_pid: u32,
    pub title: String,
}

/// EnumWindows 扫一遍所有可见窗口，返回 **title 子串包含 `marker`** 的第一个窗口。
///
/// 这是从 `find_window_for_marker` 抽出的纯 leaf primitive（code-motion，行为
/// byte-identical）：同样的 thread_local FOUND/MARKER、同样的 512-u16 buffer、
/// 同样的 `title.contains(marker)` 子串匹配、同样**不过滤 owner=0**（见下方注释）。
#[cfg(windows)]
fn find_window_by_marker_substr(marker: &str) -> Option<MarkerHit> {
    use std::cell::RefCell;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    thread_local! {
        static FOUND: RefCell<Option<MarkerHit>> = const { RefCell::new(None) };
        static MARKER: RefCell<String> = const { RefCell::new(String::new()) };
    }

    MARKER.with(|m| *m.borrow_mut() = marker.to_string());
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
            *f.borrow_mut() = Some(MarkerHit {
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

    FOUND.with(|f| f.borrow_mut().take())
}

#[cfg(not(windows))]
fn find_window_by_marker_substr(_marker: &str) -> Option<MarkerHit> {
    None
}

#[cfg(not(windows))]
pub struct MarkerHit {
    pub hwnd: isize,
    pub owner_pid: u32,
    pub title: String,
}

#[cfg(windows)]
fn find_window_for_marker(req: &AwaitRequest) -> Option<HwndEntry> {
    let m = find_window_by_marker_substr(&req.marker)?;
    // FileTime → u64（HwndEntry.owner_proc_start 仍 wire u64 保兼容；0 表示拿不到）
    let owner_proc_start = process_creation_filetime(m.owner_pid)
        .map(|ft| ft.0)
        .unwrap_or(0);

    // 组装那一步是**平台无关**的（见 `entry_from_marker_hit` 头注：写在这里的话
    // `设计/80 §8.7` 步 3 那一格在 Linux 门禁上一条判据都够不到）。
    Some(entry_from_marker_hit(req, m, owner_proc_start))
}

#[cfg(not(windows))]
fn find_window_for_marker(_req: &AwaitRequest) -> Option<HwndEntry> {
    None
}

/// 拿指定 PID 的 GetProcessTimes creation FILETIME。失败返 None。
/// 返回 `crate::utils::FileTime` 强类型（避免跟 NetTicks 混用）。
#[cfg(windows)]
fn process_creation_filetime(pid: u32) -> Option<crate::utils::FileTime> {
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
        Some(crate::utils::FileTime::from_win32(&creation))
    }
}

#[cfg(not(windows))]
fn process_creation_filetime(_pid: u32) -> Option<crate::utils::FileTime> {
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
            return Err(copy_text("rsBind.verify.windowGone", &[]));
        }
        let mut cur_owner: u32 = 0;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut cur_owner));
        if cur_owner != binding.owner_pid {
            return Err(copy_text(
                "rsBind.verify.windowReused",
                &[
                    ("curOwner", &cur_owner.to_string()),
                    ("ownerPid", &binding.owner_pid.to_string()),
                ],
            ));
        }
        if binding.owner_proc_start != 0 {
            // 两边都是 FileTime UTC（u64 同零点）→ 直接比 .0 即可
            let cur_proc_start = process_creation_filetime(cur_owner)
                .map(|ft| ft.0)
                .unwrap_or(0);
            if cur_proc_start != 0 && cur_proc_start != binding.owner_proc_start {
                return Err(copy_text("rsBind.verify.pidReused", &[]));
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
            Err(copy_text("rsBind.activate.refused", &[]).into())
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
    /// 被忘了」这一段线，本仓一条判据都没有：唯一碰 `forget` 的单测是直接调原语的
    /// `remote_hwnd_cache_insert_lookup_forget`，**一条推送边都不经过**。
    /// 抽成方法之后，判据钉的是**行为**（事实进来、缓存变成什么样），
    /// 不是那段闭包的行号 —— 这条链哪天搬家，判据整块跟着走。
    ///
    /// # 今天的行为，逐字一句
    ///
    /// **两种 cause 一视同仁，都忘。** `Gone` 是真死；`Superseded` 是同一个 pidfile
    /// 原地换了 sid（`/branch` `/clear`），旧 sid 连 attach 都 attach 不上
    /// ⇒ 那条绑定对它已经没有任何意义。
    ///
    /// ⚠ **在这里长出 `match cause` 是一次行为改动，不是重构。** 判据两条各钉一格
    /// （`Gone` / `Superseded`），谁在这里加分支，那一格会出声。
    pub fn apply_local_removal(&self, removed: &crate::session_map::RemovedSid) {
        self.forget(&removed.sid);
    }

    fn persist(&self) {
        let snapshot = self.by_sid.read().clone();
        if let Err(e) = crate::utils::atomic_write_json(&self.file, &snapshot) {
            tracing::warn!("sid-hwnd persist failed: {e}");
        }
    }
}

/// Feature ②（远端 Tab ↗ 拉前）：sid → 拉前所需信息的**纯内存**缓存。
///
/// 跟本地 [`SidHwndCache`] 是两套独立机制：本地走 PS 主动握手（ps-await/ps-registry
/// 加持久化加心跳）；远端走 wrapper 设的窗口标题 `ccm-rbind-<sid>`，monitor 在
/// session_added 时扫本地窗口找该标题并直接绑 sid。
///
/// **无持久化、无 record/get_parent_pid**：远端绑定是瞬时的（窗口标题在远端 shell
/// 存活期间一直在），monitor 重启后 session_added 会重扫重绑；`verify_binding` 是
/// 运行时安全网（HWND 复用 / 进程换人都会被它拦下）。
pub struct RemoteHwndCache {
    by_sid: Arc<RwLock<HashMap<String, SidHwndBinding>>>,
}

impl RemoteHwndCache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            by_sid: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn lookup(&self, sid: &str) -> Option<SidHwndBinding> {
        self.by_sid.read().get(sid).cloned()
    }

    pub fn forget(&self, sid: &str) {
        self.by_sid.write().remove(sid);
    }

    /// K-W1C：**远端**那条 removed 的分流结果到达时，这份缓存该变成什么样。
    ///
    /// 入参是 `classify_removed` 的裁决，不是原始 cause —— 「该归档还是该判灰」
    /// 那个判断只有一个住址（那个纯函数），本方法**只答缓存忘不忘**，不许在这里
    /// 重算一遍分流（那会长出第二份语义）。
    ///
    /// # 今天的行为，两句
    ///
    /// - `Archive`（claude 死了、tmux 那一格也没了）⇒ **忘**。
    /// - `Idle`（claude 退了、tmux 会话还在，灰灯那一格）⇒ **不忘**：
    ///   本地那个 ssh 窗口可能还开着，那条绑定仍然拉得前。
    /// - 〔GP1 · 第四波〕`Unseen`（到那台的连接断了，说不清）⇒ **不忘**，理由同 `Idle`：monitor 那条后端连接断了
    ///   不等于用户那个 ssh 窗口没了；重连之后会话若还活着，↗ 照旧指得到它。代价如实记：重连后它若不在清单里
    ///   （前端落已结束），这条绑定没有人来忘 —— 一个 sid 一格，量级 = 会话数。
    ///
    /// ⚠ 「`Idle` 到底该不该忘」这一问**还没裁**（件计划 D5 在问它，理由是那条绑定
    /// 此刻指的窗口未必还是那个会话的窗口）。本方法只把**今天是这样**钉住，
    /// 好让哪天有人改它的时候有一条判据出声，而不是靠读注释。
    pub fn apply_remote_disposition(
        &self,
        sid: &str,
        disposition: &crate::ssh_source::RemovedDisposition,
    ) {
        if matches!(disposition, crate::ssh_source::RemovedDisposition::Archive) {
            self.forget(sid);
            // 〔`设计/80 §8.7` 步 4，第二波 T4〕令牌账本跟着忘，口径与上面那条绑定**同一条**：
            // `Archive` 忘、`Idle` 不忘（本地那个 ssh 窗口可能还开着，令牌指的正是它）。
            remote_rbind_tokens().forget(sid);
        }
    }

    /// 扫本地窗口找标题含 `ccm-rbind-<sid>` 的窗口并绑定。成功返 true。
    ///
    /// 复用本地机制的 leaf primitive（[`find_window_by_marker_substr`]）。组出的
    /// `SidHwndBinding` 里 `ps_pid`/`ps_proc_start` 留空（0 / ""）——远端无 PS 握手，
    /// 而 `verify_binding` 只读 hwnd/owner_pid/owner_proc_start，不读这两个字段。
    #[cfg(windows)]
    pub fn try_bind(&self, sid: &str) -> bool {
        let marker = format!("ccm-rbind-{sid}");
        let Some(hit) = find_window_by_marker_substr(&marker) else {
            return false;
        };
        let owner_proc_start = process_creation_filetime(hit.owner_pid)
            .map(|ft| ft.0)
            .unwrap_or(0);
        let binding = SidHwndBinding {
            hwnd: hit.hwnd,
            owner_pid: hit.owner_pid,
            owner_proc_start,
            ps_pid: 0,
            ps_proc_start: String::new(),
            title_at_bind: hit.title,
            registered_at: crate::utils::now_ms(),
        };
        self.by_sid.write().insert(sid.to_string(), binding);
        true
    }

    #[cfg(not(windows))]
    pub fn try_bind(&self, _sid: &str) -> bool {
        false
    }

    /// F75（#41 远端拉前不及时）：**带重试**的现扫绑定，供 on-demand（↗ 点击）路径用。
    ///
    /// 单次 [`try_bind`] 对 on-demand 不够：远端标题传播链是「远端 shell → SSH → tmux → 本地
    /// 终端」**四跳**，且 tmux 默认截标题（wrapper 每 ~0.3s 重刷 `ccm-rbind-<sid>`）——用户在标题
    /// 传播完成前点 ↗、或 `/resume` 切 sid 后 marker 刚重刷时，单次扫描常错过 → ↗ 报「未绑定」。
    /// 短暂重试给标题传播/重刷的时间（本地 `handle_await_files` 同款思路 `bind.rs:225`；但远端四跳
    /// 需更长窗口——本地 600ms 大概率不够）。命中即停（`try_bind`/EnumWindows 廉价）。调用方在
    /// `spawn_blocking` 里，sleep 不阻塞主线程。
    ///
    /// ⚠️ **窗口长度待真机实测调**（四跳 + tmux 截断 + 用户点击后的可接受等待，`ON_DEMAND_BIND_*`）。
    #[cfg(windows)]
    pub fn try_bind_with_retry(&self, sid: &str, attempts: u32, step_ms: u64) -> bool {
        if self.try_bind(sid) {
            return true;
        }
        for _ in 0..attempts {
            std::thread::sleep(std::time::Duration::from_millis(step_ms));
            if self.try_bind(sid) {
                return true;
            }
        }
        false
    }

    #[cfg(not(windows))]
    pub fn try_bind_with_retry(&self, _sid: &str, _attempts: u32, _step_ms: u64) -> bool {
        false
    }
}

// ═══════ 🔴 `设计/80 §8.7` 步 4 / 步 5：**↗ 远端那一格的唯一分派点** ═══════════════════
//
// 步 4 逐字：「↗ 改走 join；四套『有没有终端』的判断收敛成一句」。那四套是
// `attachable` 布尔 · `findClaudeTmuxMatches` · 后端 HWND 校验 · E73 那次远端 RPC（`§8.5 ②`）。
// 收成的那一句就是：**这个 sid 有没有启动令牌**。
//
// 步 5 逐字：「旧标题路降级成**退路**（不删 —— 它覆盖『用户自己在 tmux 里跑 ccm』那一档）」。
// ⇒ 分派只有一种顺序：**先令牌、后标题**；失败时说的话**只由那一个布尔决定**。
//
// ## 为什么标题路在「有令牌」时也要试一次
//
// 令牌登记的是**拉起那一刻**的那个窗口。那个窗口关掉、用户再用 attach 开一个新的 ——
// 新窗口不做令牌握手（`attach` 不铸币），但 tmux 容器那一格的外层命令设了
// `set-titles-string ccm-rbind-#{@ccm_sid}`（`payload.rs::render_tmux_outer`）⇒ 标题路接得住。
// 不试的话，这一档从「今天能拉」退成「拉不了」—— 那是回归，不是收敛。
//
// ## ⚠ 买不到什么
//
// `verify_binding` / `activate` / `find_window_by_marker_substr` 在非 Windows 上都是桩 ⇒
// 本仓 Linux 门禁买得到的是**分派本身**（哪条路先、什么时候退、失败说哪句话），
// 「窗口真的到了前台」一格都买不到。

/// 远端会话 `sid → 启动期令牌`（wire 上 `SessionAdded.rbind_token` 读回来的那个）。
///
/// **不落盘，刻意的**：真相源在远端那个进程的 `environ` 里（后端每次重连 / 重新宣告都会
/// 再报一遍）。在本地再存一份只会多一个会陈旧的副本 —— monitor 重启之后，
/// `令牌 → HWND` 那一半由 `ps-registry/*.json` 重载（持久化），`sid → 令牌` 这一半由
/// 重连后的 `SessionAdded` 重新喂进来，join 自然恢复。判据见 `bind_tests.rs` 的重启那一条。
pub struct RbindTokenBook {
    by_sid: RwLock<HashMap<String, String>>,
}

impl RbindTokenBook {
    pub fn new() -> Self {
        Self {
            by_sid: RwLock::new(HashMap::new()),
        }
    }

    /// wire 上读到一条 `SessionAdded` 时调。`None` ⇒ **删掉**旧值：同一个 sid 被重新宣告成
    /// 「没令牌」（比如换了一个老后端、或那个进程换了人）时，不许让上一次的令牌粘着 ——
    /// 粘着的令牌会把 ↗ 拉到一个已经不属于它的窗口上。
    ///
    /// ⚠ 形状**不在这里再判一遍**：进来的值只可能来自 `ssh_source::parse_frame`，
    /// 那里已经过了 [`rbind_token_shape_ok`]（同一条函数）。
    pub fn note(&self, sid: &str, token: Option<&str>) {
        let mut w = self.by_sid.write();
        match token {
            Some(t) => {
                w.insert(sid.to_string(), t.to_string());
            }
            None => {
                w.remove(sid);
            }
        }
    }

    pub fn token_of(&self, sid: &str) -> Option<String> {
        self.by_sid.read().get(sid).cloned()
    }

    pub fn forget(&self, sid: &str) {
        self.by_sid.write().remove(sid);
    }
}

impl Default for RbindTokenBook {
    fn default() -> Self {
        Self::new()
    }
}

/// 进程里唯一那本令牌账本。写者 = `ssh_source` 收 `SessionAdded` 的那一处；
/// 读者 = [`bring_remote_front`]；清者 = [`RemoteHwndCache::apply_remote_disposition`]（`Archive`）。
pub fn remote_rbind_tokens() -> &'static RbindTokenBook {
    static BOOK: std::sync::OnceLock<RbindTokenBook> = std::sync::OnceLock::new();
    BOOK.get_or_init(RbindTokenBook::new)
}

/// 本地表里那一条 → ↗ 要的那份绑定。字段一一对应（`verify_binding` 读 hwnd / owner_pid /
/// owner_proc_start 三样）。
fn binding_from_entry(e: HwndEntry) -> SidHwndBinding {
    SidHwndBinding {
        hwnd: e.hwnd,
        owner_pid: e.owner_pid,
        owner_proc_start: e.owner_proc_start,
        ps_pid: e.ps_pid,
        ps_proc_start: e.ps_proc_start,
        title_at_bind: e.title_at_bind,
        registered_at: e.registered_at,
    }
}

/// 有令牌、却切不到窗口时说的话（`§8.5 ②`：失败归因只由「有没有令牌」一个布尔决定）。
///
/// 🔴 两句话的**开头**是分派的对外面：前端不再猜（E73 那次远端 RPC 已删），用户读到的就是这里。
/// ⚠ 措辞过 `设计/91` 的术语表：不说「令牌」「拉前」「拉起」（前两个是内部词，后一个是禁档），
///   说用户看得见的那件事 —— 「是不是 cc-monitor 启动的」。
pub(crate) static FRONT_FAIL_WITH_TOKEN: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsBind.front.failWithToken", &[]));
/// 没有令牌时说的话。
pub(crate) static FRONT_FAIL_WITHOUT_TOKEN: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsBind.front.failWithoutToken", &[]));

/// ↗ 远端那一格的**唯一分派点**。平台相关的两跳（校验窗口、现扫标题）由调用方注入，
/// 好让分派本身在任何机器上都验得了（`entry_from_marker_hit` 同一个理由）。
///
/// - `token`：这个 sid 的启动期令牌（[`RbindTokenBook::token_of`]）。**这是唯一的分派变量。**
/// - `verify`：生产是 [`verify_binding`]（IsWindow ＋ 属主 PID ＋ procStart）。
/// - `rescan`：生产是 [`RemoteHwndCache::try_bind_with_retry`]（标题路的点击时现扫）。
pub fn resolve_remote_front(
    sid: &str,
    token: Option<&str>,
    registry: &BindRegistry,
    title_path: &RemoteHwndCache,
    verify: &dyn Fn(&SidHwndBinding) -> Result<(), String>,
    rescan: &dyn Fn(&str) -> bool,
) -> Result<SidHwndBinding, String> {
    // ① 令牌路（步 4）：`sid → token → HWND`，查的是 Era 2 那张表（持久化 ＋ 心跳白拿）。
    let mut token_window_gone: Option<String> = None;
    if let Some(tok) = token {
        if let Some(entry) = registry.lookup_hwnd_for_token(tok) {
            let b = binding_from_entry(entry);
            match verify(&b) {
                Ok(()) => return Ok(b),
                Err(why) => token_window_gone = Some(why),
            }
        }
    }
    // ② 标题路（步 5：退路，不删）。与改之前 `lib.rs` 那一段逐步相同：
    //    没缓存 ⇒ 现扫；缓存校验不过 ⇒ 忘掉、再现扫、再校验。
    let title = (|| -> Result<SidHwndBinding, String> {
        let b = match title_path.lookup(sid) {
            Some(b) => b,
            None => {
                rescan(sid);
                title_path.lookup(sid).ok_or_else(String::new)?
            }
        };
        if verify(&b).is_ok() {
            return Ok(b);
        }
        title_path.forget(sid);
        rescan(sid);
        let b = title_path.lookup(sid).ok_or_else(String::new)?;
        verify(&b)?;
        Ok(b)
    })();
    title.map_err(|title_err| {
        // ③ 归因：**只看 `token.is_some()`**。
        let head = match (token.is_some(), &token_window_gone) {
            (true, Some(why)) => format!("{}：{why}。", FRONT_FAIL_WITH_TOKEN.as_str()),
            (true, None) => copy_text(
                "rsBind.remote.mayBeClosed",
                &[("fail", &FRONT_FAIL_WITH_TOKEN.to_string())],
            ),
            (false, _) => format!("{}。", FRONT_FAIL_WITHOUT_TOKEN.as_str()),
        };
        let tail = if title_err.is_empty() {
            copy_text("rsBind.remote.titleNotFound", &[])
        } else {
            copy_text(
                "rsBind.remote.titleStale",
                &[("titleErr", &title_err.to_string())],
            )
        };
        format!("{head}{tail}")
    })
}

/// 生产那一趟：查账本 → 分派 → 拉前。`lib.rs::bring_remote_terminal_to_front` 只调这一个。
pub fn bring_remote_front(
    sid: &str,
    registry: &BindRegistry,
    title_path: &RemoteHwndCache,
) -> Result<(), String> {
    let token = remote_rbind_tokens().token_of(sid);
    let b = resolve_remote_front(
        sid,
        token.as_deref(),
        registry,
        title_path,
        &verify_binding,
        &|s| title_path.try_bind_with_retry(s, ON_DEMAND_BIND_ATTEMPTS, ON_DEMAND_BIND_STEP_MS),
    )?;
    activate(b.hwnd)
}

/// F75：on-demand（↗ 点击）现扫绑定的重试窗口——`ON_DEMAND_BIND_ATTEMPTS × ON_DEMAND_BIND_STEP_MS`。
/// #41 真机实证:1.5s **确认不足**——用户 attach 后首点 ↗ 仍弹「未绑定窗口」、几秒后才成(四跳 +
/// tmux 截标题 + rbind 每秒轮询 的传播 > 1.5s)。故 15×100ms → **40×100ms = 4s**。仅在**失败**时才
/// 等满窗口(成功即返回),且跑在 `spawn_blocking`(不阻塞主线程);前端 ↗ 超时(`tabs.ts` 8s)已抬到
/// > 本窗口,不撞车。**仍属 carry-forward 真机微调**(4s 若仍偶发不足,据真机再抬)。
pub const ON_DEMAND_BIND_ATTEMPTS: u32 = 40;
pub const ON_DEMAND_BIND_STEP_MS: u64 = 100;

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

// P3 归并：本地 atomic_write_json / now_ms 已删，全部走 crate::utils。

#[cfg(test)]
#[path = "../../../tests/bridge/bind_tests.rs"]
mod tests;
