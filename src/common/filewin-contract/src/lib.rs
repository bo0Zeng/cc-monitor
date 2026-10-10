//! monitor 与文件窗口进程之间两边必须对上的那几样（契约类，两边分属两个 crate，形状只许有这一份）：
//! - [`OpenRequest`] —— 开窗种子：窗口进程 stdin 的第一行（一份 JSON）；之后同一对 stdin / stdout 就是通道（`chan`，没有钥匙：父子管道）；
//! - [`Ready`] —— 窗口进程列完第一屏在 stderr 上说的那一行（带 [`READY_MARK`] 打头；stderr 上别的行是诊断）；
//! - [`TERMINAL_OPEN_OP`] —— 「在此打开终端」：窗口在它那条通道上 `call` 的、由 monitor 自己接下来的那一条（只带意图：那台 ＋ 当前目录）；
//! - [`PLAN_OPEN_OP`] —— 「在计划里看」：同上那一形（只带意图：那台 ＋ 工作区 · 片 · 格），monitor 交给主窗口；
//! - [`BIN_ENV`] —— 指到窗口那份二进制的环境变量。

use copy_core::copy_text;

pub mod theme;
pub use theme::{parse_css_color, parse_shadow, Rgba, Shadow, Theme, THEME_TOKENS};

/// 覆盖「那份二进制在哪」的环境变量。
///
/// 🔴 它存在的理由与 `local_backend::BACKEND_BIN_ENV` / `CCM_DIAL_PROXY` 逐字同形：
/// **判据跑在 `target/<档>/deps/` 里**（`current_exe()` 给的是那条测试二进制），
/// 而 `[[bin]]` 的产物在它的上一级 ⇒ 判据必须说得出「用这一份」。
/// ⚠ 它**不是** fail-open 的开关：给了但那份文件不在，照旧是一条响亮的失败。
pub const BIN_ENV: &str = "CCM_FILEWIN_BIN";

/// 一次开窗的全部输入 —— 它整份过一次进程边界（stdin 的第一行）。之后那一对管子就是通道，帧长上限是 [`Self::frame`]。
/// 字段与窗口那一侧 `shell::FileWindow::seeded` ＋ `set_reveal` 的入参一一对应、不多不少（多一个就是「窗口那侧能有、而开窗这条路给不了」的缝）；
/// 例外两格由窗口进程自己补：`rows`（那一屏，`proc::first_screen` 列）与 `cwd` 缺席时的 home。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OpenRequest {
    /// 这个窗口看哪台机器（寻址用的那个名字，`RemoteConfig::origin_label` 的口径）。开终端由 monitor 接（它自己查配置），窗口只要这个名字。
    pub origin: String,
    /// 开在哪个目录；`None` = 问那台机器的 home（`files-home`，窗口进程自己问）。
    pub cwd: Option<String>,
    /// 开窗就高亮这一行（`None` = 不高亮）。
    pub reveal: Option<String>,
    /// 通道帧头 / 帧体各自的字节上限（两端同一个数；通道就是窗口进程自己的 stdin / stdout）。
    pub frame: usize,
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
    /// 复制详情「本机」那一项的值（`cc-monitor 版本 (构建) · 系统 架构`，monitor 算好）：窗口自己写的那几份详情（通道断了 · 本侧的错）
    /// 照主界面那样带上它。对端拒绝的那份由 monitor 的通道宿主补，不经这里。
    #[serde(default)]
    pub local_line: String,
}

/// 种子 → 字节。**纯函数**（判据两向对拍）。
///
/// # Errors
///
/// 序列化失败（今天各格都是 serde 表达得了的，这一支只是不许 `unwrap`）。
pub fn encode_request(r: &OpenRequest) -> Result<String, String> {
    serde_json::to_string(r)
        .map_err(|e| copy_text("rsFilewinProc.child.noRuntime", &[("e", &e.to_string())]))
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
        .map_err(|e| copy_text("rsFilewinProc.child.noRuntime", &[("e", &e.to_string())]))
}

/// 窗口进程在 stderr 上说的那一行：第一屏列到几行，或列不出来的原话。线上形 `ccm-filewin-ready {"listed":N}` / `… {"failed":"…"}`。
/// 走 stderr 是因为 stdout 已经是通道（列第一屏那几问就在它上面，先于这一行）；打头那个记号把它与 stderr 上的诊断分开。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ready {
    /// 列出来了（行数）—— 它接着就开窗。
    Listed(usize),
    /// 列不出来（原话）—— 它不开窗就退。
    Failed(String),
}

/// 就绪那一行打头的记号（stderr 上带它的那一行才是 [`Ready`]）。
pub const READY_MARK: &str = "ccm-filewin-ready ";

/// [`Ready`] → 那一行（带记号、带换行）。**纯函数**。
pub fn encode_ready(r: &Ready) -> String {
    // `Ready` 只有一个整数或一个字符串，序列化不会失败；万一失败也得是一行、而且说清。
    let json = serde_json::to_string(r).unwrap_or_else(|e| {
        format!(
            "{{\"failed\":{}}}",
            serde_json::Value::String(e.to_string())
        )
    });
    format!("{READY_MARK}{json}\n")
}

/// stderr 上这一行是不是就绪那一行（带 [`READY_MARK`] 打头）。**纯函数**。
pub fn is_ready_line(line: &str) -> bool {
    line.starts_with(READY_MARK)
}

/// 那一行 → [`Ready`]。**纯函数**；没有记号 / 解不出来就是错（不猜）。
///
/// # Errors
///
/// 不是约定的那两种形状。
pub fn decode_ready(line: &str) -> Result<Ready, String> {
    let json = line.strip_prefix(READY_MARK).unwrap_or("");
    serde_json::from_str(json.trim())
        .map_err(|e| copy_text("rsFilewinProc.child.noRuntime", &[("e", &e.to_string())]))
}

/// 「在此打开终端」：窗口在它那条通道上 `call`（寻址 ＝ 那台机器的 `origin`）、
/// **monitor 自己接下来**的那一条（不按 `origin` 转给后端；先例是传输台的 `transfer-upload` / `-download`）。
/// 参数只带意图 `{cwd}`（当前目录的线上形：字符串或 `{"b16": …}`）—— 那一串命令由后端渲（`terminal-ssh`），窗口不拼命令；
/// 机器事实由 monitor 从它自己的机器表取；开窗是 monitor 的事（`launch::open_terminal_window`，与主界面开终端同一条路）。
pub const TERMINAL_OPEN_OP: &str = "terminal-open";

/// 「开另一台的文件窗口」：窗口左栏「其他机器」点一台 ⇒ 在通道上 `call` 这一条（寻址 ＝ 要开的那台机器的名字，参数空），
/// **monitor 自己接**、照开窗入口同一条路起一个新的窗口进程（一窗一机：不是在这个窗口里换机器）。
pub const FILEWIN_OPEN_OP: &str = "filewin-open";

/// 「那台此刻连没连着」那条流的 `kind`：窗口订它（寻址 ＝ 那台），**monitor 自己接**（连接循环的事实，不按 `origin` 转给后端）。
/// 格子：连着 ⇒ `Seen`；刚断、在重连 ⇒ `Unseen{why: Dropped}`；重连一轮没连上 / 从没连上 ⇒ `Unseen{why: Unreachable}`。
pub const LINK_KIND: &str = "link";

/// 「重新连接」：窗口那一条警告条上的按钮 ⇒ 在通道上 `call` 这一条（寻址 ＝ 那台，参数空），
/// **monitor 自己接**：叫醒那台的连接循环，不等退避睡满。回 `{}`；连没连上看 [`LINK_KIND`] 那条流。
pub const LINK_RETRY_OP: &str = "link-retry";

/// 「在计划里看」（计划反查，设计稿 planned-build 06）：窗口在通道上 `call` 这一条（寻址 ＝ 那台），**monitor 自己接**：
/// 把主窗口拉到前面、切到计划页、选中那一格（片按机器区分：寻址就是那台）。参数 `{workspace, slice, id}`（[`plan_open_args`]）。
pub const PLAN_OPEN_OP: &str = "plan-open";

/// 「在计划里看」交给主窗口的那一份（monitor 照它发事件）。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PlanOpen {
    pub workspace: String,
    pub slice: String,
    pub id: String,
}

/// [`PLAN_OPEN_OP`] 的参数。
pub fn plan_open_args(workspace: &str, slice: &str, id: &str) -> serde_json::Value {
    serde_json::json!({ "workspace": workspace, "slice": slice, "id": id })
}

/// [`PLAN_OPEN_OP`] 的参数 → 那一格（缺了 / 形状不对 ⇒ `None`：调用方用错了）。
pub fn plan_open_target(args: &serde_json::Value) -> Option<PlanOpen> {
    serde_json::from_value(args.clone()).ok()
}

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

// ── 远端路径怎么切：开窗入口把「跳到这个文件」切成目录 ＋ 名字交进种子，窗口里之后每一次「上一级」都用同一个切法 ──

/// 上一级目录。只吃一条字符串：远端路径恒用 `/`（SFTP 协议就是这么定的，对面是 Windows 也一样）。
/// 别换成 `std::path`：它在 Windows 上会把 `\` 也当分隔符 ⇒ 名字里含反斜杠的远端目录会被切成两级。
/// 到顶了就返回原值：调用方靠「回来的和给出去的相等」判断已经在顶上了。
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

/// 远端路径的最后一段（basename），与 [`parent_dir`] 是一对（一个给前缀、一个给尾段）。只用 `/`，理由同 [`parent_dir`]。
/// （`corpus.rs` · `shell.rs` 里另有各自的 `rsplit('/')`，这个概念不只一个住址。）
pub fn remote_basename(path: &str) -> &str {
    let t = path.trim_end_matches('/');
    match t.rfind('/') {
        Some(i) => &t[i + 1..],
        None => t,
    }
}
