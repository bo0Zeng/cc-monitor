//! 控制面判据里引到上层（观测面 · 帧面 · 进后端的口）的那几条：那一层是独立的包（`backend-control`）、够不到上层 ⇒ 挂在主 crate。

// 命令表里的码 == 金样 · 过门的命令登记了门的码 · 路由认进后端的那个词 · 别名预览交这台的账号清单成品。
#[path = "../control/on_upper_tests.rs"]
mod on_upper_tests;
// 起会话挑号的规则表（夹具的账号清单成品照观测面那一份的规则补）。
#[path = "../control/launch_account_pick_tests.rs"]
mod launch_account_pick_tests;
// ccm 的计划（账号清单成品的夹具在观测面）；拼一个号进计划的那两样夹具别的判据也用。
#[path = "../control/ccm/plan_tests.rs"]
pub(crate) mod ccm_plan_tests;
// 经 ccm 用某个号起一次 ⇒ 那个号记到那条会话名下（认便条照观测侧那样；假启动器是 POSIX shell 脚本）。
#[cfg(unix)]
#[path = "../control/ccm/launch_note_e2e_tests.rs"]
mod launch_note_e2e;
// 从历史里恢复一条会话的那一行端到端（判号要帧面宿主交的账号清单成品；假启动器是 POSIX shell 脚本）。
#[cfg(unix)]
#[path = "../control/launch_render/history_resume_e2e_tests.rs"]
mod history_resume_e2e;
// 待答登记表（`needs` 那一份由观测面的认法补出）。
#[path = "../control/asks_tests.rs"]
mod asks_tests;
// 一批会话一次问（判号的账号清单成品夹具在观测面）。
#[path = "../control/session_batch_tests.rs"]
mod session_batch_tests;
// 起会话（替身经观测面认报到 · 账号清单成品）。
#[path = "../control/session_open_new_tests.rs"]
mod session_open_new_tests;
#[path = "../control/session_open_tests.rs"]
mod session_open_tests;
// 终端实时预览（订阅客户端不改观测面读的 tmux 快照）。
#[path = "../control/terminal_follow_tests.rs"]
mod terminal_follow_tests;
