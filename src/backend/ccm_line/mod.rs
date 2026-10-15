//! `ccm` 这一行的**词与解析**：旗标 · 解析 · 默认值（[`argv`]）· 自己叫什么 · 用配置那一段的记号。
//!
//! 起会话（`control::ccm`）· 账号域拼 ccm 那一行（`accounts::manage::wire`）· 别名与配置那一族（`assets::aliases`）都认这一套词，
//! 所以它住在它们底下：只引 `agents`（认得哪几家）· `common` · `platform` 与根上的 argv 表，不往上引。
//! 做事那一半（算计划 · 渲染 · 起会话）住 `control::ccm`。

pub(crate) mod argv;

/// 这个二进制在终端里的名字（`~/.cc-monitor/bin/ccm`）。预览语境里「叫的是 `ccm`」就是它（`control::ccm::plan::Env::for_preview`）。
pub(crate) const SUBCOMMAND_WORD: &str = "ccm";

/// 用配置文件里那一段的写法：`@<名>` 打头（[`PROFILE_SIGIL`] ＋ 段名）。
pub(crate) const PROFILE_SIGIL: char = '@';

/// 这个名字是不是 ccm 自己（`ccm` · 后端本名 `cc-monitor-backend` 及旧版释放的 `cc-monitor-backend-<build>`）。
/// 不是 ⇒ 被叫成这个名字等同 `@<名>`（`control::ccm::route`）；配置文件也不收这几个名字当段名。
pub(crate) fn is_own_name(name: &str) -> bool {
    name == SUBCOMMAND_WORD || name.starts_with(env!("CARGO_PKG_NAME"))
}

/// 后端认得的第一个词：一次性子命令 ∪ 流模式旗标。只许紧跟**打头的** `--` 出现（`ccm -- --stream …`）。
pub(crate) fn is_backend_word(w: &str) -> bool {
    crate::SUBCOMMANDS.contains(&w) || crate::STREAM_FLAGS.contains(&w)
}
