//! 要求：「种子与钥匙走 stdin，不走 argv / env」· `§2.3`「窗口进程拨回通道之后……在 stdout 上说一行「列到 N 行」或「列不出来：原话」」
//!
//! monitor 与文件窗口进程之间**两边必须对上**的那几样（契约类）：
//! - [`OpenRequest`] —— 开窗种子，整份走窗口进程的 stdin（一份 JSON，写完关掉 = EOF = 给完了）；
//! - [`Ready`] —— 窗口进程列完第一屏在 stdout 上说的那一行；
//! - [`TERMINAL_OPEN_OP`] —— 「在此打开终端」：窗口在它那条通道上 `call` 的、由 monitor 自己接下来的那一条（只带意图：那台 ＋ 当前目录）；
//! - [`BIN_ENV`] —— 指到窗口那份二进制的环境变量（判据与「这个程序由 cc-monitor 打开」那句话都说它）。
//!
//! 从前这些住壳里 `filewin/proc.rs`：monitor 与窗口进程编自同一个 crate。窗口独立成包（`src/frontend/filewin/`）之后
//! 两边分属两个 crate，形状只许有这一份。

use copy_core::copy_text;

pub mod theme;
pub use theme::{parse_css_color, Rgba, Theme, THEME_TOKENS};

/// 覆盖「那份二进制在哪」的环境变量。
///
/// 🔴 它存在的理由与 `local_backend::BACKEND_BIN_ENV` / `CCM_DIAL_PROXY` 逐字同形：
/// **判据跑在 `target/<档>/deps/` 里**（`current_exe()` 给的是那条测试二进制），
/// 而 `[[bin]]` 的产物在它的上一级 ⇒ 判据必须说得出「用这一份」。
/// ⚠ 它**不是** fail-open 的开关：给了但那份文件不在，照旧是一条响亮的失败。
pub const BIN_ENV: &str = "CCM_FILEWIN_BIN";

/// 一次开窗的**全部**输入 —— 它整份过一次进程边界（走 stdin，见模块头注 §三）。
///
/// 🔴多了 [`Self::handoff`]：通道的交接件（回环地址 ＋ **钥匙** ＋ 帧长）。
/// 它**只走 stdin** —— 不走 argv（`/proc/<pid>/cmdline` 世界可读）、不走环境变量
/// （`/proc/<pid>/environ` 同用户可读、且会被孙进程继承），理由与 `chan/host.rs` 头注
/// 「钥匙怎么交接」第 3 步逐字同一条。⚠ 本类型的 `Debug` 会打到 `Handoff` 那一格，
/// 而 `Handoff` / `Key` 的 `Debug` 都手写成不打印钥匙 —— 钥匙不进日志。
///
/// 🔴 字段与窗口那一侧 `shell::FileWindow::seeded` ＋ `set_reveal` 的入参**一一对应**，
/// 刻意不多不少：多一个字段就是一处「窗口那侧能有、而开窗这条路给不了」的缝。
/// 〔09-28 裁 3〕例外恰好两格、都由窗口进程自己补：`rows`（那一屏，窗口那一侧 `proc::first_screen` 列）与
/// `cwd` 缺席时的 home（同一处问）。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OpenRequest {
    /// 这个窗口看哪台机器（寻址用的那个名字，`RemoteConfig::origin_label` 的口径）。
    /// 从前这一格是那台机器的整份配置（`RemoteConfig`）：窗口只拿它取名字、再交给「在此打开终端」算机器事实 ——
    ///   开终端改由 monitor 接（它自己查配置），窗口只剩这个名字。
    pub origin: String,
    /// 开在哪个目录；`None` = 问那台机器的 home（`files-home`，窗口进程自己问）。
    /// 〔09-28 裁 3〕「已经列好的那一屏」（`rows`）这一格删了：第一屏由窗口进程自己列。
    pub cwd: Option<String>,
    /// 开窗就高亮这一行（`None` = 不高亮）。
    pub reveal: Option<String>,
    /// 🔴窗口进程拿它拨回 monitor 那个通道口（`chan::dial::dial`）。
    pub handoff: chan_core::chan::handoff::Handoff,
    /// 书签文件的全路径（monitor 算好：它住 monitor 的数据目录）。
    /// `None` ＝ 数据目录解不出来 ⇒ 窗口的书签栏上出声，不静默不画。
    pub bookmarks: Option<std::path::PathBuf>,
    /// 视图文件的全路径（记整窗缩放；同书签那份，monitor 算好）。`None` ＝ 数据目录解不出来 ⇒ 照样能缩放，只是不记。
    pub view: Option<std::path::PathBuf>,
    /// 「复制到另一台」那一问的下拉：本机 ＋ 已配的远端（monitor 从已有的配置读口算好；名字与 `origin` 同一个口径）。
    #[serde(default)]
    pub machines: Vec<String>,
    /// 主窗所在显示器的工作区（monitor 问 Tauri 得来）：窗口开出来第一拍夹进它。`None` ＝ 问不到，不夹。
    #[serde(default)]
    pub work_area: Option<host_core::WorkArea>,
    /// 窗口的样子：开窗那一刻主界面解析出来的那一套（含用户改过的）。窗口只照它画，不另有一份。
    pub theme: Theme,
}

/// 种子 → 字节。**纯函数**（判据两向对拍）。
///
/// # Errors
///
/// 序列化失败（今天各格都是 serde 表达得了的，这一支只是不许 `unwrap`）。
pub fn encode_request(r: &OpenRequest) -> Result<String, String> {
    serde_json::to_string(r)
        .map_err(|e| copy_text("rsFilewinProc.seed.encodeFailed", &[("e", &e.to_string())]))
}

/// 字节 → 种子。**纯函数**。
///
/// ⚠ 它**不许**对残缺的种子补默认值：补一个默认 cwd 出来，用户会看到一个
/// 开在别处的窗口而且没有一句话。解不出来就是错。
///
/// # Errors
///
/// 不是合法 JSON / 字段形状不对 / 空输入。
pub fn decode_request(raw: &str) -> Result<OpenRequest, String> {
    if raw.trim().is_empty() {
        return Err(copy_text(
            "rsFilewinProc.seed.empty",
            &[("binEnv", &BIN_ENV.to_string())],
        ));
    }
    serde_json::from_str(raw)
        .map_err(|e| copy_text("rsFilewinProc.seed.unreadable", &[("e", &e.to_string())]))
}

/// 窗口进程在 stdout 上说的**那一行**（〔09-28 裁 3〕）：第一屏列到几行，或列不出来的原话。
/// 线上形 `{"listed":N}` / `{"failed":"…"}`，一行一个 JSON。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ready {
    /// 列出来了（行数）—— 它接着就开窗。
    Listed(usize),
    /// 列不出来（原话）—— 它不开窗就退。
    Failed(String),
}

/// [`Ready`] → 那一行（带换行）。**纯函数**。
pub fn encode_ready(r: &Ready) -> String {
    // `Ready` 只有一个整数或一个字符串，序列化不会失败；万一失败也得是一行、而且说清。
    let mut s = serde_json::to_string(r).unwrap_or_else(|e| {
        format!(
            "{{\"failed\":{}}}",
            serde_json::Value::String(e.to_string())
        )
    });
    s.push('\n');
    s
}

/// 那一行 → [`Ready`]。**纯函数**；解不出来就是错（不猜）。
///
/// # Errors
///
/// 不是约定的那两种形状。
pub fn decode_ready(line: &str) -> Result<Ready, String> {
    serde_json::from_str(line.trim())
        .map_err(|e| copy_text("rsFilewinProc.ready.unreadable", &[("e", &e.to_string())]))
}

/// 「在此打开终端」：窗口在它那条通道上 `call`（寻址 ＝ 那台机器的 `origin`）、
/// **monitor 自己接下来**的那一条（不按 `origin` 转给后端；先例是传输台的 `transfer-upload` / `-download`）。
/// 参数只带意图 `{cwd}`（当前目录的线上形：字符串或 `{"b16": …}`）—— 那一串命令由后端渲（`terminal-ssh`），窗口不拼命令；
/// 机器事实由 monitor 从它自己的机器表取；开窗是 monitor 的事（`launch::open_terminal_window`，与主界面开终端同一条路）。
pub const TERMINAL_OPEN_OP: &str = "terminal-open";

/// 「开另一台的文件窗口」：窗口左栏「其他机器」点一台 ⇒ 在通道上 `call` 这一条（寻址 ＝ 要开的那台机器的名字，参数空），
/// **monitor 自己接**、照开窗入口同一条路起一个新的窗口进程（一窗一机：不是在这个窗口里换机器）。
pub const FILEWIN_OPEN_OP: &str = "filewin-open";

/// [`FILEWIN_OPEN_OP`] 的参数：发起那扇窗正在用的样子（新窗口照它画）。
pub fn filewin_open_args(theme: &Theme) -> serde_json::Value {
    serde_json::json!({ "theme": theme })
}

/// [`FILEWIN_OPEN_OP`] 的参数 → 样子（缺了 / 形状不对 ⇒ `None`：调用方用错了）。
pub fn filewin_open_theme(args: &serde_json::Value) -> Option<Theme> {
    serde_json::from_value(args.get("theme")?.clone()).ok()
}

/// [`TERMINAL_OPEN_OP`] 的参数。`cwd` 是当前目录的线上形（窗口那一侧 `source::RemotePath::wire`），原样交给后端。
pub fn terminal_open_args(cwd: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "cwd": cwd })
}

/// [`TERMINAL_OPEN_OP`] 的参数 → 当前目录的线上形（缺了就是 `None`：调用方用错了）。
pub fn terminal_open_cwd(args: &serde_json::Value) -> Option<&serde_json::Value> {
    args.get("cwd")
}

// ── 远端路径怎么切（原住窗口 `source.rs`，逐字搬来：开窗入口把「跳到这个文件」切成目录 ＋ 名字交进种子，窗口里之后每一次「上一级」
//    都用同一个切法；两边分属两个 crate 之后切法只许住这一份）──

/// 上一级目录。
///
/// 🔴 **它只吃一条字符串，不吃 [`Source`]** —— 而那是本机那一侧退役买到的东西之一：
/// 远端路径**恒用 `/`**（SFTP 协议就是这么定的，对面是 Windows 也一样）
/// ⇒ 只剩一个算法。⚠ 别为了「看起来通用」把 `std::path` 换回来：
/// 它在 Windows 上会把 `\` 也当分隔符 ⇒ 远端一个名字里含反斜杠的目录会被切成两级。
///
/// ⚠ 到顶了就**返回原值**（不是空串、不是 `None`）—— 调用方靠「回来的和给出去的相等」
/// 判断「已经在顶上了」，这样「到顶」这件事不需要第二个返回通道。
pub fn parent_dir(cwd: &str) -> String {
    let trimmed = cwd.trim_end_matches('/');
    if trimmed.is_empty() {
        // `/` 或空串：都已经在根上。
        return "/".to_string();
    }
    match trimmed.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => trimmed[..i].to_string(),
    }
}

/// 远端路径的**最后一段**（basename）。
///
/// 🔴 抽成具名函数是因为盘上已经有**三处** `rsplit('/')` 各写了一份
/// （`corpus.rs` · `shell.rs` 那两处），而这一刀要的是第四处。
/// ⇒ 不再加第四份。它与 [`parent_dir`] 是**一对**（一个给前缀、一个给尾段），
/// 所以住同一处。
///
/// ⚠ **只用 `/`**，理由与 [`parent_dir`] 逐字相同：SFTP 协议恒用 `/`，
/// 拿 `std::path` 去切远端路径在 Windows 上会把 `\` 也当分隔符。
/// ⚠ 那三处旧写法**本刀不动**（它们各在自己的语境里，改它们是另一件活）——
/// 如实登记在这儿，别以为这个概念只有一个住址。
pub fn remote_basename(path: &str) -> &str {
    let t = path.trim_end_matches('/');
    match t.rfind('/') {
        Some(i) => &t[i + 1..],
        None => t,
    }
}
