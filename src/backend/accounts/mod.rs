//! 账号域：两块，互不引用（账号就账号，中转就中转）。
//!
//! | 块 | 是什么 | 谁用它 |
//! |---|---|---|
//! | [`upstream`] | 中转的**上游选择**（apikey 端点改写）：`(agent, 账号)` 路由表 · 凭据文件（中转里只读）· 热重载 ＋ 这台机器上那份凭据文件的帧面读写口 | 常驻后端里中转的装配口 `upstream::host_relay` · `stream/inbound/` 的 `apikey-key-set` / `apikey-read` |
//! | [`manage`] | 这台机器上的**账号库**：建库 · 加号 / 删号 · 核对 · 修复 · 隔离 · 回滚 · 账号别名 | `faces/accounts_face.rs`（`accounts-*` 那几条改账号库的帧命令） |
//! | [`sign_in`] | 在 app 里**登录一个号**：起那一家的登录子命令、转链接、递码（令牌那一家自己写） | `faces/login_face.rs`（`login-*`） |
//!
//! ⚠ **它们不共用一张登记表**：上游选择的那几条判据（`relay_upstream_selection_guard` 的上游选择根、
//! 中转日志白名单 `relay_creds_guard::LOG_ROOTS`、`comm_boundary_registry` 的上游选择前缀）
//! 只圈 `upstream/` 这棵子树；`manage` 一条都不进。两块之间零引用由
//! `upstream_selection_guard::the_two_halves_of_the_account_domain_do_not_reference_each_other` 两向钉着；
//! 建 API 号要把 key 写进上游选择那份表，那一步由帧面宿主接（装配，不是互相认识）。
//!
//! 本文件只声明两块，不放任何代码 —— 放了就成了两块共用的第三处。
//! 底下挂的是中转宿主（独立的包 `backend-relay`）那几条引到账号域的判据：真中转 ＋ 生产段的上游选择。

pub mod manage;
pub mod oauth;
pub mod quota;
pub mod sign_in;
pub mod upstream_select;

// ── 中转宿主的判据里引到账号域的那几条（那一层是独立的包、够不到账号域）──
// 记日志走白名单 · 写半边只从一处声明开 · 换头前先丢的头名
#[cfg(test)]
#[path = "../../../tests/backend/relay/creds_guard.rs"]
mod relay_creds_guard;
// 中转去上游经出网口（这台的描述 · 那条会话的描述）
#[cfg(test)]
#[path = "../../../tests/backend/relay/egress_tests.rs"]
mod relay_egress_tests;
// 中转成员的单测里要借后端的那几格（内存探针 · 上游选择）
#[cfg(test)]
#[path = "../../../tests/backend/relay/member_tests.rs"]
mod relay_member_tests;
#[cfg(test)]
#[path = "../../../tests/backend/relay/upstream_selection_guard.rs"]
mod relay_upstream_selection_guard;
// 「零行为变化」的字节金标准（三条线各一份手写期望）
#[cfg(test)]
#[path = "../../../tests/backend/relay/wire_golden.rs"]
mod relay_wire_golden;
