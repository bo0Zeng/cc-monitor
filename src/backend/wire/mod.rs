//! Phase-0 backend→client wire types.
//!
//! Wire contract: exactly one UTF-8 JSON object per line, terminated by `\n`,
//! with no bare `\n`/`\r` inside the object. `serde_json` compact output
//! escapes any inner newline as `\n` (two chars), so the only literal newline
//! on the wire is the trailing terminator appended by [`to_line`].
//!
//! 这一份是**帧面的协议零件**那一层的根（最底下，只引 `platform` · `common` · `agents` · `relay`；`layering_guard` 钉着）：
//! 帧本身住这里，其余零件各一份子模块。命令表与接线（分派 · 门 · 期限 · 出口声明 · tap）住上面的 `crate::stream`。

pub mod cli_words; // CLI 面的词：流模式显式词 · 子命令后面的选项词（与请求信封同名的那几格）· 帧命令名到子命令的拼法
pub(crate) mod conn; // 连接本身那几条帧命令的类型（`ping` · `cancel` · `stream-watch`）
pub(crate) mod detail; // 失败应答的「复制详情」那一格（时刻 · 机器 · 命令 · 码 · 原话）· 失败投到 CLI 面
pub mod drain; // 流模式收场：退出之前先排空开跑之后停不下来的那一档（闸 · 票 · 期限）
pub mod listen; // K-P1：常驻监听口的纯判定（判 attach · 拒绝那一行 · 分档 · 模式 · 常量；接受循环住 main.rs）
pub(crate) mod spec; // 一条入方向命令的登记形状（各族命令表只 use 这一份）· 按档装处理器 `prepare`
pub mod topic; // 「X 变了 ⇒ 重读」那一种推送（`changed`）的主题表（只此一处：名字 · 可丢性 · 小成品上限）
pub mod topic_hook; // 帧里的小成品怎么现算：进程里装的那一个函数（发端经它调；`main` 装 `stream::topic_body`）
pub(crate) mod typed; // typed 命令：入参 · 回包 · 错误码各是一个 Rust 类型

use crate::agents::record::RecordExt as _;
use crate::common::cells::{Tone, Words};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
pub use topic::TopicExt;

/// 帧的线上形住 `cmd-wire`（[`cmd_wire::frame`]，客户端按同一份解）；这里是后端在它上面做的事：写字 · 判定 · 丢帧规矩 · 分页 · 钟面
/// （扩展特征：孤儿规则不许在那边的类型上加固有方法）。
pub use cmd_wire::frame::{decode, Frame, Hello, Line, SessionAdded, SessionPrimed, SessionStatus};
pub use cmd_wire::frame::{ActivityFace, CanSwitch, NameFace, SessionActivity, Switch};
pub use cmd_wire::frame::{AgentHome, LostFrame, RemovalCause, SessionFate, Topic, Unavailable};
pub use cmd_wire::frame::{Cancelled, Changed, Overflow, Reply, SessionRemoved, SessionState};
pub use cmd_wire::frame::{FilesBatch, LinkData, LinkEnd, Probe, SessionsReplayed, Transfer};
pub use cmd_wire::frame::{FollowEnd, RereadWhy, SessionContainer, TerminalHost, TransferEnd};
pub use cmd_wire::frame::{RunDid, RunEnded, RunInfo, RunState, RunWhy};
pub use cmd_wire::frame::{SessionBranch, SessionRuns, TerminalFollowEnd, TerminalScreen};
pub use cmd_wire::frame::{SessionFileGone, SessionFileReread, TurnEnd};

/// 写好的字与语气（短名 · 悬停那一句 · 语气）：`session_state` 帧上带着，出口照抄。
pub(crate) trait SessionFateExt {
    fn cells(self) -> (Words, Words, Tone);
}

impl SessionFateExt for SessionFate {
    /// 写好的字与语气（短名 · 悬停那一句 · 语气）：`session_state` 帧上带着，出口照抄。
    fn cells(self) -> (Words, Words, Tone) {
        match self {
            SessionFate::Reconnectable => (
                Words(copy_core::copy_text("sessionState.reconnectable.name", &[])),
                Words(copy_core::copy_text(
                    "sessionState.reconnectable.tooltip",
                    &[],
                )),
                Tone::Plain,
            ),
            SessionFate::Ended => (
                Words(copy_core::copy_text("sessionState.ended.name", &[])),
                Words(copy_core::copy_text("sessionState.ended.tooltip", &[])),
                Tone::Plain,
            ),
        }
    }
}

/// **活会话此刻在干什么的那几格的唯一构造口**（[`ActivityFace`]，`session_added` · `session_status` 都带它，平铺在帧上）：
/// 帧上这几格只经这里来，不许哪一处另拼（格是公开的、客户端要解；判据 `the_written_faces_are_built_in_one_place_only` 守「只在这里造」）。
///
/// 在跑 ⇒ 运行中 · `now`；在等人 ⇒ 需手动 · `need`；闲着 ⇒ 空闲 · `plain`；一轮停了、后台命令还在跑 ⇒ 后台任务运行中 · `busy`；
/// 活着、那一家没说在干什么（`None`）⇒ 说不清 · `plain`（出口照画，不自己补一种默认）。
/// 序：等人 0 · 在干活 1 · 后台任务运行中 2 · 闲着 / 说不清 3（要人操作的先看；序跟着语气走）。
pub(crate) trait ActivityFaceExt: Sized {
    fn of(a: Option<SessionActivity>) -> Self;
    fn reading() -> Self;
}

impl ActivityFaceExt for ActivityFace {
    /// 那一态 ⇒ 这几格。
    fn of(a: Option<SessionActivity>) -> Self {
        let (text, tone, order) = match a {
            Some(SessionActivity::NeedsYou) => (
                copy_core::copy_text("beSession.activity.needsYou", &[]),
                Tone::Need,
                0,
            ),
            Some(SessionActivity::Working) => (
                copy_core::copy_text("beSession.activity.working", &[]),
                Tone::Now,
                1,
            ),
            Some(SessionActivity::BackgroundWork) => (
                copy_core::copy_text("beSession.activity.backgroundWork", &[]),
                Tone::Busy,
                2,
            ),
            Some(SessionActivity::Idle) => (
                copy_core::copy_text("beSession.activity.idle", &[]),
                Tone::Plain,
                3,
            ),
            // 判不出就照实说「状态不明」：不说一句没判出来的「运行中」。语气 plain、序跟着语气走（与闲着同一档）。
            None => (
                copy_core::copy_text("beSession.activity.unclear", &[]),
                Tone::Plain,
                3,
            ),
        };
        ActivityFace {
            activity: a,
            activity_text: Words(text),
            activity_tone: tone,
            activity_order: order,
            switch: Switch::live(a),
        }
    }

    /// 宣告时判不出、要等记录读完才判得出的那一刻（这一家没有进程状态文件、此刻在干什么只有记录说）：
    /// 与「说不清」同一格（`activity` 缺席、语气与序同那一格，界面照它画），字照实说「读记录中」—— 不先说一句读完才会知道真假的。
    fn reading() -> Self {
        ActivityFace {
            activity_text: Words(copy_core::copy_text("beSession.activity.reading", &[])),
            ..Self::of(None)
        }
    }
}

/// **此刻能不能换模型 · 权限档的唯一判定处**（[`Switch`]）。界面只照 `can` 灰不灰、照 `why` 写 chip 上方那一行，不自己判。
/// 正在换（转圈）是发起的那一端自己知道的，不在这里。
pub(crate) trait SwitchExt: Sized {
    fn live(a: Option<SessionActivity>) -> Self;
    fn gone() -> Self;
}

impl SwitchExt for Switch {
    /// 活会话此刻那一态 ⇒ 能不能换：
    /// 在等人 ⇒ 都不能（按键 · 送的那一行会落进那个框里）；一轮在跑 ⇒ 模型不能（送的那一行排成下一条消息、不立刻生效）、权限档能（跑着也收切档键）；
    /// 闲着 · 后台命令在跑 · 说不清 ⇒ 都能（说不清时由那两条命令自己判、照实回）。
    fn live(a: Option<SessionActivity>) -> Self {
        match a {
            Some(SessionActivity::NeedsYou) => {
                let waiting = || CanSwitch::No {
                    why: Words(copy_core::copy_text("beSessionSwitch.why.waiting", &[])),
                };
                Switch {
                    model: waiting(),
                    permission: waiting(),
                }
            }
            Some(SessionActivity::Working) => Switch {
                model: CanSwitch::No {
                    why: Words(copy_core::copy_text("beSessionSwitch.why.running", &[])),
                },
                permission: CanSwitch::Yes,
            },
            Some(SessionActivity::Idle | SessionActivity::BackgroundWork) | None => Switch {
                model: CanSwitch::Yes,
                permission: CanSwitch::Yes,
            },
        }
    }

    /// 离开「活」（进程退了：可重连 · 已结束）⇒ 都不能：两条命令都要往那一家的进程里送。
    fn gone() -> Self {
        let ended = || CanSwitch::No {
            why: Words(copy_core::copy_text("beSessionSwitch.why.ended", &[])),
        };
        Switch {
            model: ended(),
            permission: ended(),
        }
    }
}

/// **这条会话叫什么那两格的唯一构造口**（[`NameFace`]）：`label` ＝ [`crate::history::search_rules::session_label`]（与历史清单 · 全文搜索同一个函数）；
/// `label_project` ＝ 项目目录最后一截。
pub(crate) trait NameFaceExt: Sized {
    fn of_parts(label: String, label_project: Option<String>) -> Self;
    fn label(&self) -> &str;
}

impl NameFaceExt for NameFace {
    /// 已经算好的那两格（只由 `history::search_rules::name_face` 调）。
    fn of_parts(label: String, label_project: Option<String>) -> Self {
        NameFace {
            label: Words(label),
            label_project: label_project.map(Words::from),
        }
    }

    /// 名字那一格。
    fn label(&self) -> &str {
        &self.label.0
    }
}

/// 分页的那几条帧命令（`history-index` · `-user-inputs` · `-turns` · `-branch`）一条应答至多多少字节，`session_branch` 一帧同此。
///
/// 一条应答就是那台流上的一整行，写它的时候那台别的会话的实时帧都在后面等（写出方向写完一页就先让实时帧走，`main.rs::writer_task`）
/// ⇒ 页越大，实时行被挡得越久；页越小，往返越多。取 1 MiB：同 `history-read` 一页、同远端链路的窗口；
/// 一份 394 MB 的合成记录冷开一趟分页那四条共 19 页（256 KiB 要 71 页，远端每页一个往返）。
pub const REPLY_PAGE_BYTES: usize = 1 << 20;

/// hello 回显哪几格宿主交来的环境（[`Frame::Hello`] 的 `host_env`）。**名单只有这两格** —— 中转端口 ·
/// 家（`CCM_DATA_DIR`，只有隔离跑才交），都是本机 monitor 起常驻后端时交的（monitor 那一侧 `local_backend_host::HANDED_ENVS`，
/// 两向对拍）。监听口的钥匙文件路径不在这里。
pub const HOST_ECHO_ENVS: [&str; 2] = [crate::relay::ENV_PORT, creds_core::store::DATA_DIR_ENV];

/// 按 [`HOST_ECHO_ENVS`] 从环境里取回显的那几格（没被交 / 空串的那一格不回显）。**纯函数**：环境由调用方给。
pub fn host_env_from(
    get: impl Fn(&str) -> Option<String>,
) -> std::collections::BTreeMap<String, String> {
    HOST_ECHO_ENVS
        .iter()
        .filter_map(|name| {
            let v = get(name).filter(|v| !v.is_empty())?;
            Some((name.to_string(), v))
        })
        .collect()
}

/// 活卡那几样（[`Frame::LiveCard`] 的体）：线上形住 `cmd-wire::live`。
pub use cmd_wire::live::{LiveBlock, LiveCard, LiveOp, LivePhase, LiveWhy};

/// 一段上游流怎么收场的（流归位交给活卡的那一格）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapEnd {
    /// 上游把响应说完了（转发正常收尾）。
    Done,
    /// 转发以错误收尾：下游走了（claude 被 Esc 打断）· 上游断了 · 写不动。
    Broken,
}

/// 传输收场那一形（跑起来之后才停下的那一形写复制详情）。
pub(crate) trait TransferEndExt {
    fn failed(why: String, code: Option<String>, cmd: &str, raw: Option<&str>) -> TransferEnd;
}

impl TransferEndExt for TransferEnd {
    /// 跑起来之后才停下的那一形：那一句 ＋ 复制详情（命令 `cmd` · 下层原话，后端这一端写好）。
    fn failed(why: String, code: Option<String>, cmd: &str, raw: Option<&str>) -> TransferEnd {
        TransferEnd::Failed {
            why,
            code,
            detail: Some(crate::wire::detail::of_run(cmd, raw)),
        }
    }
}

/// 造帧的那几处（写字的那几种帧只经这里造）。
pub(crate) trait FrameMake {
    /// 一轮结束那一帧：通知的两行在这里一处写。叫这条会话什么用标签页那一格（[`NameFace`]，同一个 `session_label`）。
    fn turn_end(session_id: String, uuid: String, name: &NameFace) -> Frame;

    /// 成功、不带数据的那一种应答。
    fn ok(id: &str) -> Frame;

    /// `changed` 帧（只此一处造）：`body` 序列化后超了主题表那一格的上限 ⇒ 不带（客户端重问）；主题不带 `body` 的给了也不带。
    fn changed(
        topic: Topic,
        key: Option<String>,
        rev: Option<String>,
        body: Option<serde_json::Value>,
    ) -> Frame;

    /// `session_state` 帧：去向 ＋ 它写好的字与语气（只此一处造这一帧）。
    fn session_state(sid: String, state: SessionFate) -> Frame;

    /// 失败应答：那一份失败（[`crate::wire::detail::Failed`]，CLI 面的信封出自同一份）装进 `reply`。
    fn failed(id: String, f: crate::wire::detail::Failed) -> Frame;

    /// 硬臂那几条命令（`Run::Builtin`：链路 · 传输 · 终端订阅）就地被拒：详情里带命令名（同处理器回的失败）。
    fn refused(id: &str, cmd: &str, code: &str, message: &str) -> Frame;

    /// 协议级失败（还没落到哪条命令上：认不出 · 读不懂 · 收场中）：详情里没有命令那一项。
    fn err(id: &str, code: &str, message: &str) -> Frame;
}

impl FrameMake for Frame {
    fn turn_end(session_id: String, uuid: String, name: &NameFace) -> Frame {
        Frame::TurnEnd(TurnEnd {
            title: Words(copy_core::copy_text(
                "turnNotify.finish.title",
                &[("tab", name.label())],
            )),
            body: Words(copy_core::copy_text("turnNotify.finish.body", &[])),
            session_id,
            uuid,
        })
    }

    fn ok(id: &str) -> Frame {
        Frame::Reply(Reply {
            id: id.to_string(),
            ok: true,
            code: None,
            message: None,
            detail: None,
            data: None,
        })
    }

    fn changed(
        topic: Topic,
        key: Option<String>,
        rev: Option<String>,
        body: Option<serde_json::Value>,
    ) -> Frame {
        let cap = topic.spec().body_cap;
        let body = body.filter(|b| serde_json::to_vec(b).is_ok_and(|v| v.len() <= cap));
        Frame::Changed(Changed {
            topic,
            key,
            rev,
            body,
        })
    }

    fn session_state(sid: String, state: SessionFate) -> Frame {
        let (state_text, state_hint, state_tone) = state.cells();
        Frame::SessionState(SessionState {
            sid,
            state,
            state_text,
            state_hint,
            state_tone,
            switch: Switch::gone(),
        })
    }

    fn failed(id: String, f: crate::wire::detail::Failed) -> Frame {
        Frame::Reply(Reply {
            id,
            ok: false,
            code: Some(f.code),
            message: Some(f.message),
            detail: Some(f.detail),
            data: f.data,
        })
    }

    fn refused(id: &str, cmd: &str, code: &str, message: &str) -> Frame {
        let f = crate::wire::detail::Failed::new(Some(cmd), code, message.to_string(), None, None);
        Frame::failed(id.to_string(), f)
    }

    fn err(id: &str, code: &str, message: &str) -> Frame {
        let f = crate::wire::detail::Failed::new(None, code, message.to_string(), None, None);
        Frame::failed(id.to_string(), f)
    }
}

/// 帧上的规矩：丢了能不能恢复 · 是不是某个会话的整行内容 · 丢帧身份 · 主线外清单分页 · 推出去那一下写钟面。
pub trait FrameRules {
    /// **丢了还能不能恢复**：出方向那条大通道按帧种分「丢了别处还在」与「丢了就没了」（应答走独立通道、阻塞背压，不在此列）。
    /// 拿不准的一律按不可恢复算（错判成可恢复 ＝ 静默丢数据；错判成不可恢复 ＝ `overflow` 多报一条身份）。
    /// ⚠ **穷尽 `match`，不许 `_ =>`** —— 新增帧种时编译期就被逼着表态。
    fn loss_is_recoverable(&self) -> bool;

    /// 这一帧是不是**某个会话的整行内容**（回那个会话的 id）：流只发要看的会话（`stream-watch`），
    /// 不在这条流的名单里的会话，这几种帧不上流；别的（宣告 · 状态 · 轮次边沿 · 去向 · 文件重读 / 不在了 · 推送 …）照发。
    ///
    /// 穷尽 `match`，同 [`Self::loss_is_recoverable`]：新增帧种时编译期就得表态。
    fn content_of(&self) -> Option<&str>;

    /// 丢帧时给客户端用来**重同步**的身份：帧种 + 主体（sid / tmux 会话名）。
    ///
    /// ⚠ 与 [`Self::loss_is_recoverable`] 同样是穷尽 `match`：新增帧种时两处一起被逼着表态。
    fn loss_identity(&self) -> LostFrame;

    /// 一份主线外清单「前 `at` 条照旧，其后是 `off[at..]`」切成几帧 `session_branch`（每帧至多 `REPLY_PAGE_BYTES` 字节，一条单独超了 ⇒ 那一帧只有它）。
    /// 后面没有了（`at` 就是全长）也发一帧空的：它说「清单到第 `at` 条为止」。
    fn branch_pages(sid: &str, path: &str, off: &[String], at: usize) -> Vec<Frame>;

    /// **推出去那一下**按这条流看的那一台的时区写钟面（`main.rs::writer_task` 对出方向每一帧调它；应答在答的那一下已按请求的 `tz` 写好）：
    /// `line.record.timeText` · `session_runs.runs[].startedText`。产这些帧的那几层不写钟面（同一份状态可能推给几个时区不同的看的人）。
    fn stamp(&mut self, tz: &crate::common::time::Tz);
}

impl FrameRules for Frame {
    fn loss_is_recoverable(&self) -> bool {
        match self {
            // 内容帧：行确实还在远端 jsonl 里，重开会话/重读就补上。
            Frame::Line(_) => true,
            // 派生自某一行 jsonl（轮次判词只看那条记录）⇒ 与 `Line` 同命。
            Frame::TurnEnd(_) => true,

            // ↓ 以下都是「丢了别处没有」或「拿不准」，一律按不可恢复算。
            //
            // 一次差分的结果，别处不存在 —— 这三个正是 B-3 的正题。
            Frame::SessionAdded(_) => false,
            // 一次性的「读完了、从第几行起」：没有「下一次必然重发」⇒ 丢了客户端不知道从哪接、也不拉快照。
            Frame::SessionPrimed(_) => false,
            Frame::SessionRemoved(_) => false,
            // 账本的成品：一次裁决的结果，别处没有 ⇒ 不可恢复。
            Frame::SessionState(_) => false,
            // 状态变迁；没有「下一次必然重发」的保证 ⇒ 保守。
            Frame::SessionStatus(_) => false,
            // 握手帧丢了这条连接就没有身份了。
            Frame::Hello(_) => false,
            // 它自己就是「丢了东西」的信号，丢了它等于连丢失都没人知道。
            Frame::Overflow(_) => false,
            // ⚠ 这两个**根本不走出方向那条通道**（应答走 `REPLY_CHANNEL_CAPACITY` 那条独立小通道，
            //   且是阻塞 `send().await`）。列在这里**只为穷尽** —— 真走到这条路上说明接线错了，
            //   按不可恢复算是保守的那一侧。
            Frame::Reply(_) => false,
            Frame::Cancelled(_) => false,
            // 「X 变了」：按主题表答。可丢的那几样在盘上（重问随时拿得到）、走 tap 那条；
            //   不可丢的（账号清单 · 配置文件 · 任务清单）是一次变化的通知，没有「下一次必然重发」⇒ 丢了按身份报，客户端重问。
            Frame::Changed(Changed { topic, .. }) => topic.spec().lossy,
            // 一次性的标记，没有「下一次必然重发」⇒ 丢了客户端就一直停在「说不清」
            //   （保守的那一侧：不会把一条说不清的会话说成已结束）。按不可恢复报身份，客户端才知道要重连。
            Frame::SessionsReplayed(_) => false,
            // 一次性的出声，没有「下一次必然重发」⇒ 丢了那个 tab 就不说那句话（内容本身照旧对：游标已按它处置）。
            Frame::SessionFileGone(_) => false,
            Frame::SessionFileReread(_) => false,
            // 链路字节：丢一块 = 那条链路上的数据坏了，别处没有第二份。
            // 与上面两个同理，它们**不走**会丢帧的那条通道（走应答通道、阻塞发送）。
            Frame::LinkData(_) => false,
            Frame::LinkEnd(_) => false,
            // 传输的进度 / 终局：丢了终局那一帧，看的人永远等下去；也走应答通道。
            Frame::Transfer(_) => false,
            // 测试连接的进度 / 结局：同上（走应答通道）。
            Frame::Probe(_) => false,
            // 一组的进度：只是快照（结局在应答里），但走应答通道、不丢。
            Frame::FilesBatch(_) => false,
            // SSE 只保快：它说的事 jsonl 那一侧都有（落盘保对），丢了由位置号 `n` 原位说出来。
            // ⚠ 它**不走**出方向那条通道（走 tap 自己那条），列在这里只为穷尽。
            Frame::LiveCard(_) => true,
            // 整份快照：下一次表一变就整份重发；丢了那个会话的子运行行停在旧的那一份，直到下一次变（带身份，客户端知道）。
            Frame::SessionRuns(_) => false,
            // 只发变了的那一截：丢了那一帧，那条连接手上的清单就对不上 —— 发帧那一层记下（`FrameSink`），下一次给它整份。
            Frame::SessionBranch(_) => false,
            // 终端实时预览：丢了在途那一帧，客户端等不到它就不回执，订阅停住；也走应答通道。
            Frame::TerminalScreen(_) => false,
            Frame::TerminalFollowEnd(_) => false,
        }
    }

    fn content_of(&self) -> Option<&str> {
        match self {
            Frame::Line(Line { session_id, .. }) => Some(session_id),
            Frame::SessionRuns(SessionRuns { sid, .. }) => Some(sid),
            Frame::SessionBranch(SessionBranch { sid, .. }) => Some(sid),
            Frame::LiveCard(l) => Some(&l.sid),
            Frame::Hello(_)
            | Frame::SessionAdded(_)
            | Frame::SessionPrimed(_)
            | Frame::SessionStatus(_)
            | Frame::SessionState(_)
            | Frame::SessionRemoved(_)
            | Frame::TurnEnd(_)
            | Frame::Overflow(_)
            | Frame::Reply(_)
            | Frame::Cancelled(_)
            | Frame::Changed(_)
            | Frame::SessionsReplayed(_)
            | Frame::SessionFileGone(_)
            | Frame::SessionFileReread(_)
            | Frame::LinkData(_)
            | Frame::LinkEnd(_)
            | Frame::Transfer(_)
            | Frame::Probe(_)
            | Frame::FilesBatch(_)
            | Frame::TerminalScreen(_)
            | Frame::TerminalFollowEnd(_) => None,
        }
    }

    fn loss_identity(&self) -> LostFrame {
        let (kind, subject) = match self {
            Frame::Hello(_) => ("hello", None),
            Frame::Line(Line { session_id, .. }) => ("line", Some(session_id.clone())),
            Frame::SessionAdded(SessionAdded { sid, .. }) => ("session_added", Some(sid.clone())),
            Frame::SessionPrimed(SessionPrimed { sid, .. }) => {
                ("session_primed", Some(sid.clone()))
            }
            Frame::SessionStatus(SessionStatus { sid, .. }) => {
                ("session_status", Some(sid.clone()))
            }
            Frame::SessionRemoved(SessionRemoved { sid, .. }) => {
                ("session_removed", Some(sid.clone()))
            }
            Frame::SessionState(SessionState { sid, .. }) => ("session_state", Some(sid.clone())),
            Frame::TurnEnd(TurnEnd { session_id, .. }) => ("turn_end", Some(session_id.clone())),
            Frame::Overflow(_) => ("overflow", None),
            Frame::Reply(Reply { id, .. }) => ("reply", Some(id.clone())),
            Frame::Cancelled(Cancelled { id }) => ("cancelled", Some(id.clone())),
            Frame::Changed(Changed { topic, key, .. }) => (
                "changed",
                Some(match key {
                    Some(k) => format!("{}/{k}", topic.name()),
                    None => topic.name().to_string(),
                }),
            ),
            Frame::SessionsReplayed(_) => ("sessions_replayed", None),
            Frame::SessionFileGone(SessionFileGone { session_id, .. }) => {
                ("session_file_gone", Some(session_id.clone()))
            }
            Frame::SessionFileReread(SessionFileReread { session_id, .. }) => {
                ("session_file_reread", Some(session_id.clone()))
            }
            Frame::LinkData(LinkData { link, .. }) => ("link_data", Some(link.clone())),
            Frame::LinkEnd(LinkEnd { link, .. }) => ("link_end", Some(link.clone())),
            Frame::Transfer(Transfer { id, .. }) => ("transfer", Some(id.clone())),
            Frame::Probe(Probe { ticket, .. }) => ("probe", Some(ticket.clone())),
            Frame::FilesBatch(FilesBatch { ticket, .. }) => ("files_batch", Some(ticket.clone())),
            Frame::LiveCard(l) => ("live_card", Some(l.sid.clone())),
            Frame::SessionRuns(SessionRuns { sid, .. }) => ("session_runs", Some(sid.clone())),
            Frame::SessionBranch(SessionBranch { sid, .. }) => {
                ("session_branch", Some(sid.clone()))
            }
            Frame::TerminalScreen(TerminalScreen { ticket, .. }) => {
                ("terminal_screen", Some(ticket.clone()))
            }
            Frame::TerminalFollowEnd(TerminalFollowEnd { ticket, .. }) => {
                ("terminal_follow_end", Some(ticket.clone()))
            }
        };
        LostFrame {
            kind: kind.to_string(),
            subject,
        }
    }

    fn branch_pages(sid: &str, path: &str, off: &[String], at: usize) -> Vec<Frame> {
        let at = at.min(off.len());
        // 摘要沿清单一趟滚过去：第 k 帧的 `sum` ＝ 到它末尾为止那一截的摘要。
        let mut sum = BranchSum::default();
        off[..at].iter().for_each(|id| sum.push(id));
        let page = |at: usize, ids: &[String], sum: String| {
            Frame::SessionBranch(SessionBranch {
                sid: sid.to_string(),
                path: path.to_string(),
                at: at as u64,
                off: ids.to_vec(),
                sum,
            })
        };
        let fixed = serde_json::to_vec(&page(usize::MAX, &[], BranchSum::default().hex()))
            .map_or(0, |b| b.len())
            + 1;
        let room = crate::REPLY_PAGE_BYTES.saturating_sub(fixed);
        let mut out = Vec::new();
        let mut start = at;
        loop {
            let mut used = 0;
            let mut end = start;
            while end < off.len() {
                let c = serde_json::to_vec(&off[end]).map_or(0, |b| b.len()) + 1;
                if end > start && used + c > room {
                    break;
                }
                used += c;
                sum.push(&off[end]);
                end += 1;
            }
            out.push(page(start, &off[start..end], sum.hex()));
            if end >= off.len() {
                return out;
            }
            start = end;
        }
    }

    fn stamp(&mut self, tz: &crate::common::time::Tz) {
        match self {
            Frame::Line(Line {
                record: Some(r), ..
            }) => r.stamp(tz),
            Frame::SessionRuns(SessionRuns { runs, .. }) => {
                for r in runs {
                    r.started_text = r.started_ms.map(|ms| {
                        crate::common::time::ms_hm(i64::try_from(ms).unwrap_or(i64::MAX), tz)
                    });
                }
            }
            _ => {}
        }
    }
}

/// 主线外清单一段前缀的摘要（FNV-1a 64，每条 `id` 后跟一个换行；十六进制串）：跨进程、跨版本稳定 —— 客户端交回的是上一条连接、
/// 甚至上一个后端进程铸的。对不上只是多发（整份），不会错。
#[derive(Debug, Clone, Copy)]
pub struct BranchSum(u64);

impl Default for BranchSum {
    fn default() -> Self {
        BranchSum(0xcbf2_9ce4_8422_2325)
    }
}

impl BranchSum {
    fn push(&mut self, id: &str) {
        for b in id.bytes().chain(std::iter::once(b'\n')) {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn hex(self) -> String {
        format!("{:016x}", self.0)
    }
}

/// 清单前几条（`ids`）的摘要（同 `session_branch.sum`）。
pub fn branch_sum(ids: &[String]) -> String {
    let mut s = BranchSum::default();
    ids.iter().for_each(|id| s.push(id));
    s.hex()
}

/// 链路字节进 JSON 串的编码与它的逆：住拨号核心那一份（`ssh_dial::b64`；那边的 files 一问一答也用它，两处只许一份）。
pub use ssh_dial::b64::{b64_decode, b64_encode};

/// `stream-watch` 应答里的一格：这个会话刚进了这条流的名单，它的这份记录从第 `seq` 行起上流
/// （`seq` 与行帧同一个行号空间）；`[0, seq)` 那段客户端按骨架补（`history-index` 从上次的末端续）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct WatchFrom {
    /// 会话 id。
    pub sid: String,
    /// 这份会话记录在那台机器上的绝对路径。
    pub path: String,
    /// 这条流从这一行起发它。
    #[cfg_attr(test, ts(type = "number"))]
    pub seq: u64,
}

/// `stream-watch` 的入参：在看哪几个（整份换）＋ 要哪几个的主线外清单、手上各有什么（`branch`；不报 ＝ 一份都不要）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub struct WatchAsk {
    /// 在看的会话 id（空 ＝ 一个都不看）。
    pub sids: Vec<String>,
    /// 要哪几个会话的主线外清单、手上各有什么（可缺 ＝ 一份都不要）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub branch: Vec<BranchHeld>,
}

/// `stream-watch.branch` 的一格：这条连接要这个会话的主线外清单；手上已有那份记录（`path`）的前 `at` 条、摘要 `sum`（上一帧 `session_branch` 给的）。
/// 手上什么都没有 ⇒ `at: 0`、不带 `path` / `sum`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub struct BranchHeld {
    pub sid: String,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub path: Option<String>,
    /// 手上那份的前几条（照手上最后一帧 `session_branch` 的 `at + off.len()`；手上没有 ⇒ `0`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub at: u64,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub sum: Option<String>,
}

/// `stream-watch` 的应答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct WatchReply {
    /// 刚进名单的会话每份记录一格（先前就在名单里的不回）。
    pub from: Vec<WatchFrom>,
}

/// U6b-1：**入方向**请求信封。只 `Deserialize` —— backend 是读的那一方。
///
/// ```text
/// {"id":"<opaque>","cmd":"<name>","args":{...},"within_ms":10000,"tz":"Asia/Shanghai","view":{"omit":{"record":["blocks[type=tool_use].input"]}}}
/// ```
///
/// `id` **不透明**：backend 不解析、不校验格式、只回显。谁生成谁负责唯一 —— 客户端。
/// backend 自己发号的话，重连后号段会撞（同 F90「不许拿会变的东西当持久键」）。
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    /// 客户端发号的不透明串，应答原样回显；同一时刻在跑的命令里不许重复。
    pub id: String,
    /// 命令名（`hello.commands` 里的一个）。
    pub cmd: String,
    /// 命令的参数对象；缺 ＝ `null`。
    #[serde(default)]
    pub args: serde_json::Value,
    /// 发起方这一发愿意等多久（毫秒）。可缺；不是正整数 ⇒ 当没带（不拒）。
    #[serde(default, deserialize_with = "lenient_ms")]
    pub within_ms: Option<u64>,
    /// 出口的声明（要哪几格 · 哪几格不要）；缺 ＝ `null` ＝ 全量。登记处统一解、统一拒、统一投影（`stream/inbound/views.rs`）。
    #[serde(default)]
    pub view: serde_json::Value,
    /// 看的那一台的时区（IANA 名）：回包里「几点」「今天 / 昨天」按它写。可缺；缺 · 认不得 ⇒ UTC（不拒）。
    #[serde(default, deserialize_with = "lenient_tz")]
    pub(crate) tz: crate::common::time::Tz,
    /// 分派那一层由 `within_ms` 减余量换成的截止时刻（不上线）。
    #[serde(skip)]
    pub(crate) until: Option<crate::platform::child::Until>,
}

/// `within_ms` 的宽读：只认正整数，别的（字符串 · 负数 · 小数 · 零 · null）一律当没带。
fn lenient_ms<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(within_ms_of(&v))
}

/// `tz` 的宽读（[`crate::common::time::Tz::of`]）：认不得 ⇒ UTC。
fn lenient_tz<'de, D: serde::Deserializer<'de>>(d: D) -> Result<crate::common::time::Tz, D::Error> {
    Ok(crate::common::time::Tz::of(
        &serde_json::Value::deserialize(d)?,
    ))
}

/// `within_ms` 那一格的读法（帧面信封 · CLI 面的期限口同一处读）：只认正整数，别的当没带。
pub(crate) fn within_ms_of(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().filter(|&n| n > 0)
}

/// Serialize a frame to its compact one-line wire form with a trailing `\n`.
///
/// The returned string is exactly one JSON object followed by a single `\n`;
/// any newline inside string fields is escaped by `serde_json` as `\n`.
pub fn to_line(frame: &Frame) -> serde_json::Result<String> {
    let mut s = serde_json::to_string(frame)?;
    s.push('\n');
    Ok(s)
}

/// U6b-3：**Hello 已经写出并 flush 的见证。**
///
/// # 为什么要一个类型
///
/// 「入方向 reader 必须在 Hello flush 之后才起」是**时序约束**。U6b-1 用一条比较
/// `main.rs` 里两个字符串字节位置的机检来钉它 —— D 审计用一次**普通的函数抽取**就绕过了：
/// 把 `inbound::spawn(` 的调用点包进 `fn start_inbound(...)` 放到文件后段，
/// 位置比较看到的就是「reader 在后面」，而实际调用早在 Hello 之前。全量 211 passed。
///
/// 判据是「两个字符串的字节位置」，对**控制流**与**函数边界**都是瞎的。
///
/// 结论是审计给的：**别再往判据上加正则，让违规不可表示。**
/// 拿不到 `HelloFlushed` 就调不了 `inbound::spawn`，而它**只能由真的写完并 flush 了一帧
/// 才能产出**（构造函数私有，唯一出口是 [`write_and_flush_hello`]）。
/// ⇒ 那条机检可以整条删掉。
pub struct HelloFlushed(());

impl HelloFlushed {
    /// 只给测试用的见证。
    ///
    /// 生产路径拿不到它 —— `#[cfg(test)]` 在 release 构建里不存在，
    /// 所以「不可表示」这条性质不受它影响。
    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self(())
    }
}

/// 写出 Hello 帧并 flush，成功则产出 [`HelloFlushed`] 见证。
///
/// **这是 `HelloFlushed` 的唯一来源。** 放 `wire.rs` 而不是 `stream/inbound/`：
/// 握手是协议管道的事，让入方向模块去写出方向的首帧会把职责搅浑。
pub async fn write_and_flush_hello<W: tokio::io::AsyncWrite + Unpin>(
    out: &mut W,
    hello: &Frame,
) -> std::io::Result<HelloFlushed> {
    use tokio::io::AsyncWriteExt as _;
    debug_assert!(
        matches!(hello, Frame::Hello(_)),
        "write_and_flush_hello 只该用来发 Hello"
    );
    let line = to_line(hello).map_err(std::io::Error::other)?;
    out.write_all(line.as_bytes()).await?;
    out.flush().await?;
    Ok(HelloFlushed(()))
}

/// Per-file monotonic sequence counter.
///
/// The counter is keyed by file path, returns the current value then increments
/// by 1 (so the first line of a file gets seq 0, then 1, 2, ...), and climbs
/// until the reader restarts it ([`SeqCounter::restart`]: the file was
/// re-read from byte 0 ⇒ seq is again the line number in the file as it is now).
///
/// 从前这里写「逐字移植自 monitor 那份 jsonl 读者的 seq 语义」—— 那份读者 CF1 删了，
/// 今天全仓 seq 只有这一个生成器（本机会话也走本机后端的 `line` 帧）。
#[derive(Debug)]
pub struct SeqCounter {
    next: HashMap<String, u64>,
}

impl SeqCounter {
    pub fn new() -> Self {
        SeqCounter {
            next: HashMap::new(),
        }
    }

    /// Batch8：当前计数器值（= 下一个将分配的 seq = 已计完整行数），不推进。
    pub fn peek(&self, path: &str) -> u64 {
        self.next.get(path).copied().unwrap_or(0)
    }

    /// 这份文件从 0 重读（截短 / 改写 / 删了又长回来）⇒ 行号从 0 重数：seq ＝ 当前文件里的行号。
    /// 调用方先发 `session_file_reread` 再发重读出来的行（下游据它把这个会话的旧号整份作废）。
    pub fn restart(&mut self, path: &str) {
        self.next.remove(path);
    }

    /// 预读交回时装回计数：这份文件已计完 `n` 行（下一个分配的 seq 就是 `n`）。
    pub fn set(&mut self, path: &str, n: u64) {
        self.next.insert(path.to_string(), n);
    }

    /// Return the current seq for `path`, then bump it by one.
    pub fn next(&mut self, path: &str) -> u64 {
        let slot = self.next.entry(path.to_string()).or_insert(0);
        let seq = *slot;
        *slot += 1;
        seq
    }
}

impl Default for SeqCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// 读一份 hello 帧成人读摘要（`v=.. build=.. arch=.. home=.. caps=.. cmds=..`）。
/// `home`：`homes` 里第一项，缺 ⇒ 那个冻结字段（`S4` 的消费侧口径；通用层不认 agent 名字，只拿值）。
/// 住 hello 的家：测试连接（`dial/probe.rs`）读**那台**后端的 hello 时用（原 monitor 那一份人读摘要）。
pub(crate) fn hello_summary(h: &serde_json::Value) -> String {
    use serde_json::Value;
    let s = |k: &str| h.get(k).and_then(Value::as_str).unwrap_or("?").to_string();
    let home = h
        .get("homes")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|e| e.get("path").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| s("claude_dir"));
    let list = |k: &str| -> Vec<String> {
        h.get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    format!(
        "v={} build={} arch={} home={home} cmds={:?}",
        h.get("v").and_then(Value::as_u64).unwrap_or(0),
        s("build_id"),
        s("host_arch"),
        list("commands"),
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/wire/wire_tests.rs"]
pub(crate) mod tests;
