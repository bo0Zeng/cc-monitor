//! 账号域：两块，互不引用（账号就账号，中转就中转）。
//!
//! | 块 | 是什么 | 谁用它 |
//! |---|---|---|
//! | [`upstream`] | 中转的**上游选择**（apikey 端点改写）：`(agent, 账号)` 路由表 · 凭据文件（中转里只读）· 热重载 ＋ 这台机器上那份凭据文件的帧面读写口 | 常驻后端里中转的装配口 `upstream::host_relay` · `stream/inbound/` 的 `apikey-key-set` / `apikey-read` |
//! | [`manage`] | 这台机器上的**账号库**：建库 · 加号 / 删号 · 核对 · 修复 · 隔离 · 回滚 · 账号别名 | `faces/accounts_face.rs`（`accounts-*` 那几条改账号库的帧命令） |
//!
//! ⚠ **它们不共用一张登记表**：上游选择的那几条判据（`relay::upstream_selection_guard` 的上游选择根、
//! 中转日志白名单 `relay::creds_guard::LOG_ROOTS`、`comm_boundary_registry` 的上游选择前缀）
//! 只圈 `upstream/` 这棵子树；`manage` 一条都不进。两块之间零引用由
//! `upstream_selection_guard::the_two_halves_of_the_account_domain_do_not_reference_each_other` 两向钉着；
//! 建 API 号要把 key 写进上游选择那份表，那一步由帧面宿主接（装配，不是互相认识）。
//!
//! 本文件只声明两块，不放任何代码 —— 放了就成了两块共用的第三处。

pub mod manage;
pub mod oauth;
pub mod quota;
pub mod upstream_select;
