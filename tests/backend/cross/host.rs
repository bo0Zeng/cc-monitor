//! 宿主口与 ccm 行判据里引到上层的那几条：那一层是独立的包（`backend-host`）、够不到上层 ⇒ 挂在主 crate。

// 撞名与分流：`--` 右边只认 ccm 表 · 后端词只紧跟打头的 `--` · `--host-run` 落在 ccm 那一侧（问 control 的分流与入口的判定）。
#[path = "../ccm_line/claude_flags_tests.rs"]
mod claude_flags_tests;
