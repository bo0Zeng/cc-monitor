//! `ccm` 这一行的**词与解析**：旗标 · 解析 · 默认值（[`argv`]）· 自己叫什么 · 用配置那一段的记号。
//!
//! 起会话（`control::ccm`）· 账号域拼 ccm 那一行（`accounts::manage::wire`）· 别名与配置那一族（`assets::aliases`）都认这一套词，
//! 所以它住在它们底下：只引 `agents`（认得哪几家）· `common` · `platform`，不往上引；后端有哪些子命令它不知道（`control::ccm::route` 收入口交下来的那一问）。
//! 做事那一半（算计划 · 渲染 · 起会话）住 `control::ccm`。

pub(crate) mod argv;

/// 这个二进制在终端里的名字（`~/.cc-monitor/bin/ccm`）。预览语境里「叫的是 `ccm`」就是它（`control::ccm::plan::Env::for_preview`）。
pub(crate) const SUBCOMMAND_WORD: &str = "ccm";

/// 新会话的 tmux 名从哪来：不给名（从工作目录派生）· `--ccm-tmux=<名>` · `--tmux-base <名>`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TmuxNaming {
    Auto,
    Fixed,
    Base,
}

/// 撞名时退不退让：只有显式名不让。起会话取名（`control::ccm::plan::build`）与别名页那句说明都照这一处。
pub(crate) const fn tmux_steps_aside(n: TmuxNaming) -> bool {
    !matches!(n, TmuxNaming::Fixed)
}

/// 用配置文件里那一段的写法：`@<名>` 打头（[`PROFILE_SIGIL`] ＋ 段名）。
pub(crate) const PROFILE_SIGIL: char = '@';

/// 这个名字是不是 ccm 自己（`ccm` · 后端本名 `cc-monitor-backend` 及旧版释放的 `cc-monitor-backend-<build>`）。
/// 不是 ⇒ 被叫成这个名字等同 `@<名>`（`control::ccm::route`）；配置文件也不收这几个名字当段名。
pub(crate) fn is_own_name(name: &str) -> bool {
    name == SUBCOMMAND_WORD || name.starts_with(crate::common::id::BIN_NAME)
}
