//! 插件通用调用口：宿主怎么把一个外部可执行文件当插件使唤，以及使唤之前怎么问清楚「你会不会我要的那几样」。
//!
//! 这一层装什么，由两个真实实现反推（`control/cc_bus.rs`：Rust，宿主调 cc-bus；`src/shared/cc-bus/scripts/cc-spawn`：shell，宿主调 ccm）：
//!
//! | 段 | 两处的形状 | 落在哪 |
//! |---|---|---|
//! | ① 找它 | 都有，但候选顺序与级数不同，且中间那一级语义相反 | [`discover`]，候选列表是参数 |
//! | ② 问它会什么 | 只有一侧有 —— 这一段是新造的 | [`probe`] |
//! | ③ 传 argv 起它 | 都有 | [`invoke`]，唯一一处起进程 |
//! | ④ 翻退出码 | 都有，但语义互斥（`3` 在一侧是「路由拒绝」、在另一侧是「撞名该重试」） | 只抽骨架 |
//!
//! ④ 只抽骨架（拿到码 · 认出被信号打断 · 超时交成的那个码 · 摘一行诊断）；码 → 语义的映射表每插件一份，住在各自的适配代码里。
//!
//! # 三条硬形状
//!
//! ① 候选列表、`PATH` 兜底与否、找不到时那句话的尾巴全是参数：两个实现的查找顺序不同；本层进了 `agent_boundary_guard::CORE_FILES`，
//!    cc-bus 的候选路径里带着 agent 的名字 ⇒ 写死在这里当场会红。
//! ② 期限归起子进程原语（`platform::child` 那一次有界等待；到点杀整组、交成 [`invoke::TIMED_OUT_CODE`]），本层一个 `Duration` 都不出现。
//! ③ 本层不许认识任何一个具体插件 —— [`layer_guard`] 里三条，认的东西不同（三条都有阴性对照）：
//!
//! | 判据 | 它认的是 | ⚠ 它认不出的 |
//! |---|---|---|
//! | `the_generic_port_names_no_concrete_plugin` | 词汇：[`layer_guard::concrete_plugin_words`] 那张表 | 表外的另一族词 |
//! | `the_generic_port_does_not_translate_exit_codes` | 形状：`Some(<整数字面量>)` / 裸整数 `match` 臂 | 具名常量写的表 · 非 `i32` 的码 · 运行期从数据里读的表 · `mod.rs` 本身 |
//! | `the_only_exit_code_constants_here_are_the_registered_generic_ones` | 登记：本层的 `i32` 常量恰好是登记的那几个（今天只有 `TIMED_OUT_CODE`） | 非 `i32` 类型的码常量 · `mod.rs` 本身 |
//!
//! # 方向（`layering_guard` 里有对应的判据）
//!
//! - `plugin → control` · `plugin → observe`：一条都不许（通用调用口一旦认识它们，就成了 control 的私有助手）。
//! - `control → plugin`：逐条登记 + 条数钉死（形状照 `ALLOWED_OBSERVE_TO_CONTROL`），让每一条边都被人看见一次。
//!
//! # 边界
//!
//! - [`probe`] 零生产调用方：本仓用这一口的插件（cc-bus）没有 probe 口；唯一的读者是走通全流程那份夹具（`plugin_walk_fixture`）。
//! - 本层管不到被起的那个进程自己在干什么：它 `while true` 每秒跑一圈，本 crate 的零定时器护栏看不见（主语是 backend 自己的源码）。

pub mod discover;
pub(crate) mod invoke;

// 零生产调用方（见头注「边界」第一条）⇒ 非测试构建压掉死代码提示。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) mod probe;

/// 本层自己的边界判据：通用调用口不许认识任何一个具体插件。
///
/// `cc_bus_boundary_guard` 守的是「backend 不许绕到 cc-bus 背后读它的数据文件」（针是 cc-bus 专有的）；本层的病是把某一个插件的语义写进通用口。
/// 两条判据主语不同，谁也替不了谁。本模块里是两族：
///
/// | 族 | 判据 | 它认的是 | 换一个插件还认得出吗 |
/// |---|---|---|---|
/// | 词汇 | [`concrete_plugin_words`] 那一条 | 某一个插件写下的名字 | 认不出（针是那一族的专有词） |
/// | 形状 | 退出码那两条 | 「把退出码翻成语义」这件事本身的形状 | 认得出（不挑插件），但另有它认不出的形（各自头注里写着） |
///
/// 形状那一族非有不可：把一张码表原样抬进本层、只擦掉插件的名字，词汇那一族一条不红。
#[cfg(test)]
#[path = "../../../tests/backend/plugin_layer_guard.rs"]
mod layer_guard;
