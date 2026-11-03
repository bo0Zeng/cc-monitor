//! 历史清单与注解 ＋ 会话清单那一族（项目 / 会话清单 · 叫会话什么 · 判活；observe 反过来调这里，这里不引 observe）＋ 会话血缘（[`lineage`]）。
//!
//! 独立的包 `backend-history`：只引 `agents` · `dial` · `host` · `wire` · `common` · `platform`，引了上层就编不过。上层只够得到这里的 `pub` 项。

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

// 包里照旧写 `crate::history::…` · `crate::lineage::…` · `crate::agents::…` · `crate::dial::…` · `crate::host::…` · `crate::wire::…` · `crate::common::…` · `crate::platform::…`。
extern crate self as history;
use backend_agents as agents;
use backend_common as common;
use backend_dial as dial;
use backend_host as host;
use backend_platform as platform;
use backend_wire as wire;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本包的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

// 量具（线程级内存 · 金样）：住 dev 侧的量具包，判据照旧写 `crate::alloc_probe::…` · `crate::golden::…`。
#[cfg(test)]
use test_meters::{alloc_probe, golden};

pub mod groups; // 组（一群会话 ＋ 一个名字）：帧面 `groups-here`（这台那一片）· `groups-read`（发问那台问到各台合一次）· `groups-write`（起组 · 进出组写组员那台 · 改名解散发给各片）
pub mod groups_owed; // 组的待补：改名 · 解散没送到的那几台，这一端欠它们的那几句（那台再答 `groups-here` 时补发）
pub mod history_annotations; // 历史注解（星标 / 改名 / 隐藏 / 上次账号 / 在哪几组 · 固定 ＋ 这台那一片组）：帧面 `history-annotate` / `history-forget`（自家状态，经 `Own`；路径由 monitor 交）
pub mod history_browse; // 历史页那一列：帧面 `history-browse`（几台的清单在本机后端并成一列，按看得见的那一窗答）
pub mod history_list; // 历史页的平铺会话清单：帧面 `history-list`（每行带状态与「能做什么」· 按项目的分组 · 搜标题 / 第一句 / 项目名）
pub mod launch_book; // 启动台账（`launch-accounts.json`：每条会话上次用哪个号 · 起会话时那一格 `--tab`）的形状与读
pub mod lineage; // 会话血缘：谁起的谁（中转那一处认，只存 `~/.cc-monitor/lineage.json` 一份）
pub mod list_node; // 会话清单变了的节点：发端（watcher）响、流的 tap 订它推 `changed {history_list}`，答清单那一刻放开
pub mod listing; // 会话清单那一族：项目清单 · 平铺会话清单（连缓存）· 热缓存 · 叫会话什么 · 「这一行能做什么」· 分叉来源 · agent 家
pub mod listing_scan; // 历史清单那一行的逐行扫描（累计的那几格 · 读每一行的窄探针）
pub mod live_table; // 整台一张活会话表：watcher 写、整份发布；冷路判活 · 在干什么 · 在等什么 · 谁在写都问它
pub mod needs_changed; // 需手动变了那一声 · 哪几条会话挂着待答请求（登记表住控制面，开张时把读法交进来）
pub mod remote_listing; // 远端那台的清单：那台怎么答（`history-listing`，带键集合）· 本机后端经 `--resident-call` 取来记着副本
pub mod search_rules; // 搜索与叫会话什么的通用口径（与哪一家 agent 无关的那一半：常量 · snippet 预算 · 最近优先 · 片段 / 截断 · 标题）
pub mod session_hidden; // 隐藏的判定在历史这一层的口（注解里单个那一格 · 叫主会话什么）
pub mod session_queue; // 会话输入队：往一条会话说的每一句排这一条（住会话那一台，主人发序号；帧面 `session-queue-*`，自家状态经 `Own`）
pub mod session_self; // agent 自问：帧面 `session-self`（调用进程往上找这台的哪一条会话 · 它在哪几组；组转给常驻问到各台）

// agent 适配层判据里经历史清单的那几条（适配层是独立的包、够不到这一层）。
#[cfg(test)]
#[path = "../../../tests/backend/agents/on_history_tests.rs"]
mod agents_tests;
