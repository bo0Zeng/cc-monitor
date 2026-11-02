//! 帧面零件判据里引到上层的那几条：那一层是独立的包（`backend-wire`）、够不到上层 ⇒ 挂在主 crate。

// 会话流金样（夹具经历史 · 识别两层造）· 叫这条会话什么那两格 · 二前端调的子命令 · 会话帧的字与语气。
#[path = "../wire/on_upper_tests.rs"]
mod on_upper_tests;
// 主题表：发端在哪几份 · 重问的是登记过的命令 · 现算的小成品 ＝ 重问那条的回包。
#[path = "../wire/topic_tests.rs"]
mod topic_tests;
// typed 命令的形状表 · 界面命令表（读登记表）· 走登记表跑一条（各族的测试经它问）。
#[path = "../wire/typed_tests.rs"]
pub(crate) mod typed_tests;
