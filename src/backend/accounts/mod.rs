//! 账号**域**：两块，**互不引用**（用户逐字「账号就账号, 中转就中转」）。
//!
//! | 块 | 是什么 | 谁用它 |
//! |---|---|---|
//! | [`apikey`] | `--relay` 的**层 2**（apikey 端点改写）：`(agent, 账号)` 路由表 · 凭据文件（只读）· 热重载 | 中转进程的装配口 `apikey::run_relay` |
//! | [`iso`] | 这台机器上的 `cc-acct-iso`：装没装 · `shellinit` 片段（两条一次性查询） | `main.rs` 的 `--acct-iso-*` 两条臂 |
//!
//! ⚠ **它们不共用一张登记表**：层 2 的那几条判据（`relay::account_layer_guard` 的层 2 根、
//! 中转日志白名单 `relay::creds_guard::LOG_ROOTS`、`comm_boundary_registry` 的层 2 前缀）
//! 只圈 `apikey/` 这棵子树；`iso` 一条都不进。两块之间零引用由
//! `account_layer_guard::the_two_halves_of_the_account_domain_do_not_reference_each_other` 两向钉着。
//!
//! 本文件只声明两块，不放任何代码 —— 放了就成了两块共用的第三处。

pub mod apikey;
pub mod iso;
