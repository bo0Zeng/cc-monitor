//! **后端命令的参数契约面** —— monitor 这一侧「一条命令的 `args` 长什么样」的唯一住址。
//!
//! # 它为什么单独一个模块〔`设计/05 §8.1` 步 3.5，2026-09-21〕
//!
//! 这三样本来住在 `inbound_client.rs` 里，而那一份是**传输面**（`设计/05 §8` 步 3
//! 点名要圈进通信层的那一族之一）。`设计/05 §2` 的铁律逐字：
//! 「**通信层不知道什么是会话、账号、skill、agent。** 它只知道：地址 · 操作名 ·
//! 载荷 · 流的订阅与分发」。
//!
//! 而「`launch` 这条命令要带 `ccm_sid`、`agent`、`width`、`height`」**恰恰**是
//! 会话与 agent 的知识 —— 它是**载荷的内容**，不是载荷的搬运。
//! ⇒ `C1`（零业务语义）在 `inbound_client.rs` 上咬到 `sid` 与 `agent` 两个词，
//! 两处都在这三样身上。**剥出来**，让传输面只管把不透明的 `Value` 发出去。
//!
//! ⚠ **这不是改名躲判据。** 搬走的是**语义**：搬完之后 `inbound_client.rs`
//! 生产段里一个业务词都没有，而这个模块**明着**是业务契约的家 —— 它不在
//! 通信层的边界登记表里，`C1` 不管它，也不该管它。
//!
//! # 它的单元判据为什么没跟着搬
//!
//! 那三条（`the_e2e_send_into_line_is_exactly_what_the_encoder_produces` ·
//! `the_tmux_primitive_arg_builder_matches_the_backend_parser` ·
//! `launch_args_field_names_match_the_backend_parser`）仍住
//! `tests/bridge/backend/control/inbound_client_tests.rs`，只把函数路径改了。
//! 理由是**现打的**：那几条带**跨半边的 `include_str!`**
//! （`src/backend/control/launch.rs` · `tests/e2e/inbound-backend-frames.sh`），
//! 而 `cross_half_edge_registry` 的那张表按**读者文件路径**逐条登记这些边 ——
//! 搬文件就得改那张登记表，而它**不在本步的写区**。
//! ⇒ 停在这里、写清为什么，别顺手改别人的登记表。
//! 🔴 这是一笔**记了账的欠账**，不是「就这样挺好」：判据与被判对象分居两个模块，
//! 下一个读 `inbound_client_tests.rs` 的人会以为那三条判的是传输面。

use serde_json::Value;

/// U8a-2b：`launch` 命令的**参数构造器**（monitor 这一侧的契约面）。
///
/// # 它今天有没有生产调用方 —— 没有，如实说
///
/// 生产路径还没切过来：tauri 命令 `launch_remote_terminal(origin, remote_cmd)` 收到的
/// 已经是一条**渲染好的 shell 串**，拆不回结构化计划。切换要等前端改成发结构化请求
/// （U8c 的两个 TS 渲染器 + IR 退役），登记为 **U8a-2c**。
///
/// 那为什么现在就写：**它是契约**。字段名一旦与后端的解析器漂开，症状是
/// 「命令发出去了、backend 回 `bad_request` 说缺字段」，而两边各自看都「对」。
/// `launch_args_field_names_match_the_backend_parser` 把这件事变成编译期就会红的对拍。
///
/// `mode` 只有两种取值 —— **没有 `attach-only`**：attach 是平面 ③，backend 在远端，
/// 开不了你面前的窗（见 backend `control/launch.rs` 头注）。
// U8a-2c-1：**它有生产调用方了** —— `backend::control::backend_launch::backend_send_into`。
// 在那之前这里挂着 `#[allow(dead_code)]`（编码器早写好、零调用方，正是复盘点名的「方向偏移」形状）。
pub fn launch_args(
    mode: &str,
    name: &str,
    payload: &str,
    cwd: Option<&str>,
    ccm_sid: Option<&str>,
    extras: LaunchExtras<'_>,
) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("mode".into(), Value::String(mode.to_string()));
    m.insert("name".into(), Value::String(name.to_string()));
    m.insert("payload".into(), Value::String(payload.to_string()));
    if let Some(c) = cwd {
        m.insert("cwd".into(), Value::String(c.to_string()));
    }
    if let Some(s) = ccm_sid {
        m.insert("ccm_sid".into(), Value::String(s.to_string()));
    }
    if let Some(a) = extras.agent {
        m.insert("agent".into(), Value::String(a.to_string()));
    }
    if let (Some(w), Some(h)) = (extras.width, extras.height) {
        m.insert("width".into(), Value::String(w.to_string()));
        m.insert("height".into(), Value::String(h.to_string()));
    }
    Value::Object(m)
}

/// `K-R104`：`capture-pane` 命令的**参数构造器**（monitor 这一侧的契约面）。
///
/// 与 [`launch_args`] 同一条理由：字段名一旦与后端的解析器漂开，症状是
/// 「命令发出去了、backend 回 `invalid_args` 说缺字段」，而两边各自看都「对」。
/// 由 [`tests::the_tmux_primitive_arg_builder_matches_the_backend_parser`] 对拍。
pub fn capture_pane_args(name: &str) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("name".into(), Value::String(name.to_string()));
    Value::Object(m)
}

/// `create-or-attach` **专有**的那三个可选字段〔`K-P2` `D3` 09-03〕。
///
/// # 为什么是一个结构体，不是再挂三个位置参数
///
/// 挂上去就是**连着五个 `Option<&str>`** —— `width` 与 `height` 同型同类，
/// 调换两个实参编译器一个字都不会说，而症状是「窗口尺寸反了」这种没人会怀疑到调用点的事。
/// 具名字段让那类错**在源码上就看得见**。
///
/// ⚠ `send-into` / `send-keys-raw` 那两个 mode 用 [`LaunchExtras::default`]：
/// 这三个字段**只对新建会话有意义**（backend 侧也只在 `CreateOrAttach` 那条臂上读它们）。
#[derive(Debug, Clone, Copy, Default)]
pub struct LaunchExtras<'a> {
    /// 哪个 AI —— 落成 tmux 的 `@ccm_agent` 标记。
    pub agent: Option<&'a str>,
    /// 新建窗口宽（十进制串）。**与 `height` 成对**：只给一半时这里直接两个都不发，
    /// 让「半个尺寸」在**发出去之前**就不存在，而不是等后端回 `invalid_args`。
    pub width: Option<&'a str>,
    /// 新建窗口高（十进制串）。见 `width`。
    pub height: Option<&'a str>,
}
