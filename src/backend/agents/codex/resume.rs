//! Codex 的起会话 / resume 事实：默认命令叫什么、resume 那个词怎么写、起它时垫什么参数。
//! 起会话 / resume 那一行由 `ccm` 按适配层这几格拼（`agents::LaunchFace`）：resume 是**子命令形**（`codex resume <sid>`），与 Claude 的 `--resume` flag 不同。
//!
//! 通用层里一个 agent 名字面量都不许有（`agent_locality_guard` 判据②）—— 谁想在别处再开一个 Codex 分支，得先写出 `"codex"`，当场红。

/// 默认启动器。
pub(crate) const DEFAULT_COMMAND: &str = "codex";

/// resume 那个字面量：**子命令形**（不以 `--` 开头）。画像表（`agents::LaunchFace`）读它。
pub(crate) const RESUME_TOKEN: &str = "resume";

/// `ccm` 起它（新起与 resume）一律不连共享后台：共享后台沿用第一个拉起它的进程的环境，
/// 这一趟给的账号与中转地址可能不生效，也认不出哪个窗格是哪个会话。
pub(crate) const LAUNCH_ARGS: &[&str] = &["--no-daemon"];

/// 起之前要清掉的嵌套会话标记：Codex 今天没人考据出有（空 ＝ 考据过、确实没有要清的）。
pub(crate) const NESTED_ENV: &[&str] = &[];
