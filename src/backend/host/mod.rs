//! 宿主口：终端住在哪种宿主里（tmux · 托管），一个口问全部。
//!
//! - [`tmux`]：tmux 那一形；起 `tmux` 进程只在那一份文件里。
//! - [`managed`]：托管那一形（cc-monitor 自己持伪终端）。
//!
//! - [`ccm_line`]：`ccm` 这一行的词与解析（旗标 · 默认值 · 自己叫什么），起会话 · 账号域 · 别名那一族都往下引它。
//!
//! 口按名单里的句柄下手，不收宿主自己的目标串。`control/` 与 `observe/` 都引它。
//!
//! 独立的包 `backend-host`：只引 `agents` · `wire` · `common` · `platform`，引了上层就编不过。上层只够得到这里的 `pub` 项。

// 测试段可以直接问系统钟（量具、夹具的时刻）；生产段只许纯工具那一层的 `time::now`（`clippy.toml`）。
// 测试段收尾时丢结果不写理由；生产段丢一个 `Result` 写 `.discard(Why::…)`。
#![cfg_attr(
    test,
    allow(
        clippy::disallowed_methods,
        clippy::let_underscore_must_use,
        clippy::unused_result_ok
    )
)]

// 包里照旧写 `crate::host::…` · `crate::ccm_line::…` · `crate::agents::…` · `crate::wire::…` · `crate::common::…` · `crate::platform::…`。
extern crate self as host;
use backend_agents as agents;
use backend_common as common;
use backend_platform as platform;
use backend_wire as wire;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本包的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

pub mod ccm_line;
pub mod managed;
pub mod tmux;

/// 名单里的一行：一个终端（tmux：一个挂着 sid 的窗格，或一个一个都没挂的 tmux 会话；托管：一个托管进程）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermRow {
    /// 住在哪种宿主里。
    pub host: crate::wire::TerminalHost,
    /// tmux：会话 ID（`$N`）；托管：句柄本身。
    pub id: String,
    /// 挂着 `sid` 的那个窗格（`%N`）；`None` ⇒ 这个 tmux 会话一个 sid 都没挂，看它当前的窗格。
    pub pane: Option<String>,
    /// 同一个 tmux 会话里还有别的窗格挂着别的 sid（结束这一个只关它的窗格，不关整个会话）。
    pub shared: bool,
    pub name: String,
    pub windows: u32,
    pub activity: u64,
    pub sid: String,
    pub agent: String,
    pub client: String,
    pub program: String,
    pub cwd: String,
    pub title: String,
    /// 输入怎么进：tmux 那一形各端都能打字；托管那一形只有一个持有者。
    pub input: Input,
}

/// 一个终端的输入此刻怎么进。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// tmux：连着的各端都能打字。
    Shared,
    /// 托管：只收持有者的；`None` ＝ 此刻没人持有。
    Held(Option<HeldBy>),
}

/// 托管终端此刻的输入持有者。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeldBy {
    /// 一个真终端里的瘦客户端（用户的终端窗口 · ssh 进来的终端 · 手机的 SSH 终端）。
    Terminal,
    /// 后端替这个前端持有（`desktop` · `mobile`）。
    Front(String),
}

impl TermRow {
    /// 名单发的不透明句柄。
    pub fn handle(&self) -> String {
        match self.host {
            crate::wire::TerminalHost::Tmux => tmux::handle_of(&self.id, self.pane.as_deref()),
            crate::wire::TerminalHost::Managed => self.id.clone(),
        }
    }

    /// tmux 那一形对它下手用的目标：挂着 sid 的窗格，或那个会话（看它当前的窗格）。
    pub fn target(&self) -> &str {
        self.pane.as_deref().unwrap_or(&self.id)
    }
}

/// 两种宿主合起来的名单：tmux 那几行在前、托管的在后；连着的客户端两种宿主合一份（`(行的 id, 接上, 最近动静)`），「报全了没有」照 tmux 那一份。
pub fn list(on: tmux::On<'_>) -> Result<tmux::Listed, crate::common::said::Said> {
    list_with(on, &managed::listed)
}

/// [`list`] 的本体，托管那一臂由调用方交进来（生产 = [`managed::listed`]；判据交一个报一行的假托管）。
pub fn list_with(
    on: tmux::On<'_>,
    managed: &dyn Fn() -> managed::Hosted,
) -> Result<tmux::Listed, crate::common::said::Said> {
    let (mut rows, mut clients, complete) = tmux::rows_on(on)?;
    let (m_rows, m_clients) = managed();
    rows.extend(m_rows);
    clients.extend(m_clients);
    Ok((rows, clients, complete))
}

/// 进程内那一张会话快照（判活 · 铸名避让），探测器是 tmux 那一形的 [`tmux::list_sessions`]。
pub fn snapshot() -> &'static crate::common::session_snapshot::SessionSnapshot {
    static ONE: std::sync::OnceLock<crate::common::session_snapshot::SessionSnapshot> =
        std::sync::OnceLock::new();
    ONE.get_or_init(|| {
        crate::common::session_snapshot::SessionSnapshot::new(Box::new(tmux::list_sessions))
    })
}
