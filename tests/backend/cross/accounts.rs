//! 账号域判据里引到上层的那几条：那一层是独立的包（`backend-accounts`）、够不到上层 ⇒ 挂在主 crate。

// 账号库同步 MCP 的那一整套（经帧面 `accounts_face` 真跑改账号库那几条命令、文件经 `assets::aliases::tests::HomeDoor`）。
#[cfg(unix)]
#[path = "../accounts/manage/mcp_share_exec_tests.rs"]
mod mcp_share_exec_tests;
// 账号库之间同步信任过的目录（同上，经帧面真建库 · 加号）。
#[cfg(unix)]
#[path = "../accounts/manage/trust_share_exec_tests.rs"]
mod trust_share_exec_tests;
// 在 app 里登录一个号的 MCP 那一路（经帧面建库 · 加号，再起登录）。
#[cfg(unix)]
#[path = "../accounts/sign_in_mcp_tests.rs"]
mod sign_in_mcp_tests;
// 盯登录的那一份交接 · 额度显示态读成帧面那一行（`faces::rotation_face::account_dirs` · `faces::quota_read`）。
#[path = "../accounts/on_upper_tests.rs"]
mod on_upper_tests;
