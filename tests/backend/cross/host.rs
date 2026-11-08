//! ccm 行判据里引到上层的那几条：`ccm_line` 住独立的包（`backend-wire`）、够不到上层 ⇒ 挂在主 crate。

// 撞名与分流：`--` 右边只认 ccm 表 · 后端词只紧跟打头的 `--` · `--host-run` 落在 ccm 那一侧（问 control 的分流与入口的判定）。
#[path = "../ccm_line/claude_flags_tests.rs"]
mod claude_flags_tests;
