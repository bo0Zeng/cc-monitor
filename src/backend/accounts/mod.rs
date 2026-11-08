//! 账号域：两块，互不引用（账号就账号，中转就中转）。
//!
//! | 块 | 是什么 | 谁用它 |
//! |---|---|---|
//! | [`upstream_select`] | 中转的**上游选择**（apikey 端点改写）：`(agent, 账号)` 路由表 · 凭据文件（中转里只读）· 热重载 ＋ 这台机器上那份凭据文件的帧面读写口 | 常驻后端里中转的装配口 `upstream_select::host_relay` · `stream/inbound/` 的 `apikey-key-set` / `apikey-read` |
//! | [`manage`] | 这台机器上的**账号库**：建库 · 加号 / 删号 · 核对 · 修复 · 隔离 · 回滚 · 账号别名 | `faces/accounts_face.rs`（`accounts-*` 那几条改账号库的帧命令） |
//! | [`sign_in`] | 在 app 里**登录一个号**：起那一家的登录子命令、转链接、递码（令牌那一家自己写） | `faces/login_face.rs`（`login-*`） |
//!
//! ⚠ **它们不共用一张登记表**：上游选择的那几条判据（`relay_upstream_selection_guard` 的上游选择根、
//! 中转日志白名单 `relay_creds_guard::LOG_ROOTS`、`comm_boundary_registry` 的上游选择前缀）
//! 只圈 `upstream_select/` 这棵子树；`manage` 一条都不进。两块之间零引用由
//! `upstream_selection_guard::the_two_halves_of_the_account_domain_do_not_reference_each_other` 两向钉着；
//! 建 API 号要把 key 写进上游选择那份表，那一步由帧面宿主接（装配，不是互相认识）。
//!
//! 本文件只声明两块，不放任何代码 —— 放了就成了两块共用的第三处。
//! 底下挂的是中转宿主（独立的包 `backend-relay`）那几条引到账号域的判据：真中转 ＋ 生产段的上游选择。
//!
//! 独立的包 `backend-accounts`：只引 `history`（含 `lineage`）· `agents` · `relay` · `wire`（含 `ccm_line`）· `common` · `platform`，
//! 引了上层就编不过。上层只够得到这里的 `pub` 项。

// 测试段可以直接问系统钟（量具、夹具的时刻）；生产段只许纯工具那一层的 `time::now`（`clippy.toml`）。
// 测试段收尾时丢结果不写理由；生产段丢一个 `Result` 写 `.discard(Why::…)`。
#![cfg_attr(
    test,
    allow(
        clippy::allow_attributes,
        clippy::allow_attributes_without_reason,
        clippy::disallowed_methods,
        clippy::let_underscore_must_use,
        clippy::unused_result_ok,
        reason = "测试段放开禁用名单与吞结果那两条：夹具自己定期限、读写盘、起进程、收尾时丢结果；用不用得到随判据增减，写成 `expect` 会在用不到时报 ⇒ 整包唯一许写 `allow` 的一行"
    )
)]

// 包里照旧写 `crate::accounts::…` · `crate::history::…` · `crate::lineage::…` · `crate::agents::…` · `crate::ccm_line::…` ·
// `crate::relay::…` · `crate::wire::…` · `crate::common::…` · `crate::platform::…`。
extern crate self as accounts;
use backend_agents as agents;
use backend_common as common;
use backend_history as history;
use backend_history::lineage;
use backend_platform as platform;
use backend_relay as relay;
use backend_wire as wire;
use backend_wire::ccm_line;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本包的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

// 量具（线程级内存 · 金样）：住 dev 侧的量具包，判据照旧写 `crate::alloc_probe::…` · `crate::golden::…`。
#[cfg(test)]
use test_meters::{alloc_probe, golden};
// 拨了必被拒、又不会被别人捡走的本机口（续令牌一律发到它；住拨号核心，经它的 `test-support`）。
#[cfg(test)]
use ssh_dial::refusing_port;

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
