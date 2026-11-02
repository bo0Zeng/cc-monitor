//! Phase-0 backend→client wire types.
//!
//! Wire contract: exactly one UTF-8 JSON object per line, terminated by `\n`,
//! with no bare `\n`/`\r` inside the object. `serde_json` compact output
//! escapes any inner newline as `\n` (two chars), so the only literal newline
//! on the wire is the trailing terminator appended by [`to_line`].
//!
//! 这一份是**帧面的协议零件**那一层的根（只引 `platform` · `common` · `agents` · `relay`）：
//! 帧本身住这里，其余零件各一份子模块。命令表与接线（分派 · 门 · 期限 · 出口声明 · tap）住主 crate 的 `stream`。
//!
//! 独立的包 `backend-wire`：引了上层就编不过。上层只够得到这里的 `pub` 项。

// 测试段可以直接问系统钟（量具、夹具的时刻）；生产段只许纯工具那一层的 `time::now`（`clippy.toml`）。
// 测试段收尾时丢结果不写理由；生产段丢一个 `Result` 写 `.discard(Why::…)`。
#![cfg_attr(
    test,
    allow(
        clippy::disallowed_methods,
        clippy::let_underscore_must_use,
        clippy::unused_result_ok
    )
)]

// 包里照旧写 `crate::wire::…` · `crate::agents::…` · `crate::relay::…` · `crate::common::…` · `crate::platform::…`。
extern crate self as wire;
use backend_agents as agents;
use backend_common as common;
use backend_platform as platform;
use backend_relay as relay;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本包的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

pub mod cli_words; // CLI 面的词：流模式显式词 · 子命令后面的选项词（与请求信封同名的那几格）· 帧命令名到子命令的拼法
pub mod conn; // 连接本身那几条帧命令的类型（`ping` · `cancel` · `stream-watch`）
pub mod detail; // 失败应答的「复制详情」那一格（时刻 · 机器 · 命令 · 码 · 原话）· 失败投到 CLI 面
pub mod drain; // 流模式收场：退出之前先排空开跑之后停不下来的那一档（闸 · 票 · 期限）
pub mod listen; // 常驻监听口的纯判定（判 attach · 拒绝那一行 · 分档 · 模式 · 常量；接受循环住 `stream/entry.rs`）
pub mod spec; // 一条入方向命令的登记形状（各族命令表只 use 这一份）· 按档装处理器 `prepare`
pub mod topic; // 「X 变了 ⇒ 重读」那一种推送（`changed`）的主题表（只此一处：名字 · 可丢性 · 小成品上限）
pub mod topic_hook; // 帧里的小成品怎么现算：进程里装的那一个函数（发端经它调；进程入口 `stream/entry.rs` 装 `stream::topic_body`）
pub mod typed; // typed 命令：入参 · 回包 · 错误码各是一个 Rust 类型

use crate::agents::record::RecordExt as _;
use crate::common::cells::{Tone, Words};
use serde::Deserialize;
use std::collections::HashMap;
pub use topic::TopicExt;

/// 线上那一格一生（[`cmd_wire::frame::Life`]）：这一层的 [`Life`] 是造脸的入参，名字让给它。
pub use cmd_wire::frame::Life as WireLife;
// 帧的线上形住 `cmd-wire`（`cmd_wire::frame`，客户端按同一份解）；这里是后端在它上面做的事：写字 · 判定 · 丢帧规矩 · 分页 · 钟面
// （扩展特征：孤儿规则不许在那边的类型上加固有方法）。
pub use cmd_wire::frame::{Allow, CanClose, CanSay, CanTerm, SessionCan, SessionDot, SessionFace};
pub use cmd_wire::frame::{Cancelled, Changed, Overflow, Reply, SessionLeft};
pub use cmd_wire::frame::{FilesBatch, Probe, SessionsReplayed, Transfer};
pub use cmd_wire::frame::{FollowEnd, RereadWhy, SessionContainer, TerminalHost, TransferEnd};
pub use cmd_wire::frame::{Frame, Hello, Line, SessionAdded, SessionPrimed, SessionStatus};
pub use cmd_wire::frame::{NameFace, SessionActivity};
pub use cmd_wire::frame::{RunDid, RunEnded, RunInfo, RunState, RunWhy};
pub use cmd_wire::frame::{SessionBranch, SessionRuns, TerminalFollowEnd, TerminalScreen};
pub use cmd_wire::frame::{SessionFileGone, SessionFileReread};
pub use cmd_wire::frame::{Topic, Unavailable};
use cmd_wire::session::Launch;

/// 一条会话此刻处在哪一段（[`SessionFaceExt::of`] 的入参）：比线上那一格 [`WireLife`] 多带写字要的几样
/// （住在哪种宿主里 · 记录读没读完 · 那台看不见）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Life {
    /// 进程在：那一家说在干什么（说不清 ⇒ `None`）· 住在哪（判不了 ⇒ `None`）。
    Live {
        activity: Option<SessionActivity>,
        host: Option<Housed>,
    },
    /// 进程在、此刻在干什么只有记录说、记录还没读完。
    Reading { host: Option<Housed> },
    /// 程序退了、tmux 会话还在。
    Reconnectable,
    /// 进程没了、容器也没了。
    Ended,
    /// 已结束、记录也不在了。
    Gone,
    /// 这条路上判不了活不活。
    Unclear,
}

impl Life {
    /// 活会话的那一段：容器那一格（宣告时判的；判不了 ⇒ 不知道在不在 tmux 里）。
    pub fn live(activity: Option<SessionActivity>, c: Option<&SessionContainer>) -> Self {
        Life::Live {
            activity,
            host: housed(c),
        }
    }

    /// 记录还没读完的那一段（容器同上）。
    pub fn reading(c: Option<&SessionContainer>) -> Self {
        Life::Reading { host: housed(c) }
    }

    /// 清单行 · 搜索命中那一段：那台判的一生（清单不看容器）。
    pub fn of_wire(life: &WireLife) -> Self {
        match life {
            WireLife::Live { activity } => Life::Live {
                activity: *activity,
                host: None,
            },
            WireLife::Reconnectable => Life::Reconnectable,
            WireLife::Ended { .. } => Life::Ended,
            WireLife::Gone => Life::Gone,
            WireLife::Unclear => Life::Unclear,
        }
    }
}

/// 一条活会话住在哪（容器那一格去掉句柄）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Housed {
    /// 在某种宿主的终端里。
    In(TerminalHost),
    /// 不在任何宿主里。
    Bare,
}

fn housed(c: Option<&SessionContainer>) -> Option<Housed> {
    c.map(|c| match c {
        SessionContainer::Tmux { .. } => Housed::In(TerminalHost::Tmux),
        SessionContainer::Managed { .. } => Housed::In(TerminalHost::Managed),
        SessionContainer::None => Housed::Bare,
    })
}

/// **一条会话整份脸的构造口**（[`SessionFace`]）：点 · 字 · 语气 · 序 · 能做什么都只在这里判；那台看不见时那一张住会话簿那一层
/// （`resident_client::book::unseen`，桌面与手机核心同一处）。
pub trait SessionFaceExt: Sized {
    fn of(life: Life) -> Self;
    /// 带上这条会话跑在哪个号下（`account`，宣告时判的那一个）。
    fn under(self, account: Option<String>) -> Self;
}

fn no(why: Words) -> Allow {
    Allow::No { why }
}

/// 说得了：框里那一句占位（文案表那一条）。
fn say_yes(hint: String) -> CanSay {
    CanSay::Yes { hint: Words(hint) }
}

impl SessionFaceExt for SessionFace {
    fn under(mut self, account: Option<String>) -> Self {
        self.account = account;
        self
    }

    fn of(life: Life) -> Self {
        let running = || Words(copy_core::copy_text("tabBatch.why.live", &[]));
        let ended_why = |name: &Words| {
            Words(copy_core::copy_text(
                "tabBatch.why.ended",
                &[("ended", &name.0)],
            ))
        };
        match life {
            Life::Live { activity, host } => live_face(activity, host, false),
            Life::Reading { host } => live_face(None, host, true),
            Life::Reconnectable => {
                let tip = Words(copy_core::copy_text(
                    "sessionState.reconnectable.tooltip",
                    &[],
                ));
                SessionFace {
                    dot: SessionDot::Exited,
                    life: WireLife::Reconnectable,
                    settled: false,
                    name: Words(copy_core::copy_text("sessionState.reconnectable.name", &[])),
                    line: tip.clone(),
                    tip: tip.clone(),
                    tone: Tone::Plain,
                    order: 4,
                    account: None,
                    can: SessionCan {
                        close: CanClose::No { why: running() },
                        resume: no(running()),
                        term: CanTerm::Yes { host: Launch::Tmux },
                        reattach: Allow::Yes,
                        // 离开「活」两样都能换：记在注解上、下次恢复用上（没有终端也收）。
                        model: Allow::Yes,
                        permission: Allow::Yes,
                        stop: no(tip.clone()),
                        // 程序退了、tmux 会话还在：结束的是那个 tmux 会话。
                        end: Allow::Yes,
                        // 程序退了：先接回（tmux 会话还在），再说。
                        say: CanSay::No { why: tip.clone() },
                    },
                }
            }
            Life::Ended | Life::Gone => {
                let gone = life == Life::Gone;
                let (dot, name, tip) = if gone {
                    (
                        SessionDot::Gone,
                        Words(copy_core::copy_text("sessionState.gone.name", &[])),
                        Words(copy_core::copy_text("sessionState.gone.tooltip", &[])),
                    )
                } else {
                    (
                        SessionDot::Ended,
                        Words(copy_core::copy_text("sessionState.ended.name", &[])),
                        Words(copy_core::copy_text("sessionState.ended.tooltip", &[])),
                    )
                };
                let why = ended_why(&name);
                SessionFace {
                    dot,
                    life: if gone {
                        WireLife::Gone
                    } else {
                        WireLife::Ended { by: None }
                    },
                    settled: true,
                    line: tip.clone(),
                    account: None,
                    can: SessionCan {
                        close: CanClose::Quiet,
                        resume: if gone { no(tip.clone()) } else { Allow::Yes },
                        term: CanTerm::No { why: why.clone() },
                        reattach: no(why.clone()),
                        stop: no(why.clone()),
                        end: no(why),
                        // 已结束：记在注解上、下次恢复用上；记录已不在 ⇒ 恢复不了，换了也用不上（同恢复那一句）。
                        model: if gone { no(tip.clone()) } else { Allow::Yes },
                        permission: if gone { no(tip.clone()) } else { Allow::Yes },
                        // 已结束：发一句 ＝ 先恢复再把这句交进它的队；记录已不在 ⇒ 恢复不了。
                        say: if gone {
                            CanSay::No { why: tip.clone() }
                        } else {
                            say_yes(copy_core::copy_text("composer.input.ended", &[]))
                        },
                    },
                    name,
                    tip,
                    tone: Tone::Plain,
                    order: 5,
                }
            }
            Life::Unclear => {
                let name = Words(copy_core::copy_text("beSession.activity.unclear", &[]));
                SessionFace {
                    dot: SessionDot::Unknown,
                    life: WireLife::Unclear,
                    settled: false,
                    tip: name.clone(),
                    line: name.clone(),
                    tone: Tone::Plain,
                    order: 3,
                    account: None,
                    can: SessionCan {
                        close: CanClose::No { why: name.clone() },
                        resume: no(Words(copy_core::copy_text(
                            "sessionState.unseen.noResume",
                            &[],
                        ))),
                        term: CanTerm::No { why: name.clone() },
                        reattach: no(name.clone()),
                        model: no(name.clone()),
                        permission: no(name.clone()),
                        stop: no(name.clone()),
                        end: no(name.clone()),
                        say: CanSay::No { why: name.clone() },
                    },
                    name,
                }
            }
        }
    }
}

/// 进程在的那几段（`reading` ＝ 记录还没读完、此刻在干什么只有记录说）。
fn live_face(
    activity: Option<SessionActivity>,
    host: Option<Housed>,
    reading: bool,
) -> SessionFace {
    let running = || Words(copy_core::copy_text("tabBatch.why.live", &[]));
    let (dot, key, tone, order) = match activity {
        Some(SessionActivity::NeedsYou) => (
            SessionDot::NeedsYou,
            Words(copy_core::copy_text("beSession.activity.needsYou", &[])),
            Tone::Need,
            0,
        ),
        Some(SessionActivity::Working) => (
            SessionDot::Running,
            Words(copy_core::copy_text("beSession.activity.working", &[])),
            Tone::Now,
            1,
        ),
        Some(SessionActivity::BackgroundWork) => (
            SessionDot::Background,
            Words(copy_core::copy_text(
                "beSession.activity.backgroundWork",
                &[],
            )),
            Tone::Busy,
            2,
        ),
        Some(SessionActivity::Idle) => (
            SessionDot::Idle,
            Words(copy_core::copy_text("beSession.activity.idle", &[])),
            Tone::Plain,
            3,
        ),
        // 判不出就照实说「状态不明」，不说一句没判出来的「运行中」；序与闲着同一档。
        None if reading => (
            SessionDot::Idle,
            Words(copy_core::copy_text("beSession.activity.reading", &[])),
            Tone::Plain,
            3,
        ),
        None => (
            SessionDot::Idle,
            Words(copy_core::copy_text("beSession.activity.unclear", &[])),
            Tone::Plain,
            3,
        ),
    };
    let name = key;
    let tip = match host {
        Some(Housed::In(TerminalHost::Tmux)) => Words(copy_core::copy_text(
            "sessionState.liveAttachable.tooltip",
            &[],
        )),
        Some(Housed::In(TerminalHost::Managed) | Housed::Bare) => Words(copy_core::copy_text(
            "sessionState.liveResumable.tooltip",
            &[],
        )),
        None => name.clone(),
    };
    // 在等人 ⇒ 都不能换（按键 · 送的那一行会落进那个框里）；一轮在跑 ⇒ 模型不能（排成下一条消息）、权限档能；其余都能。
    let (model, permission) = match activity {
        Some(SessionActivity::NeedsYou) => (
            no(Words(copy_core::copy_text(
                "beSessionSwitch.why.waiting",
                &[],
            ))),
            no(Words(copy_core::copy_text(
                "beSessionSwitch.why.waiting",
                &[],
            ))),
        ),
        Some(SessionActivity::Working) => (
            no(Words(copy_core::copy_text(
                "beSessionSwitch.why.running",
                &[],
            ))),
            Allow::Yes,
        ),
        _ => (Allow::Yes, Allow::Yes),
    };
    // 一轮在跑 ⇒ 停得下（送 Esc）；别的时候没有在跑的一轮可停，理由就是此刻在干什么那一句。
    let stop = match activity {
        Some(SessionActivity::Working) => Allow::Yes,
        _ => no(name.clone()),
    };
    SessionFace {
        dot,
        life: WireLife::Live { activity },
        settled: false,
        line: name.clone(),
        name,
        tip,
        tone,
        order,
        account: None,
        can: SessionCan {
            close: CanClose::No { why: running() },
            resume: no(running()),
            term: CanTerm::Yes {
                host: match host {
                    Some(Housed::In(TerminalHost::Tmux)) => Launch::Tmux,
                    Some(Housed::In(TerminalHost::Managed)) => Launch::Managed,
                    Some(Housed::Bare) | None => Launch::Direct,
                },
            },
            reattach: no(running()),
            model,
            permission,
            stop,
            // 不在任何宿主里（非 cc-monitor 起的）⇒ 没有终端可结束；住哪判不了 ⇒ 交了由那台认。
            end: match host {
                Some(Housed::Bare) => {
                    no(Words(copy_core::copy_text("terminal.empty.notOurs", &[])))
                }
                _ => Allow::Yes,
            },
            // 不在任何宿主里（非 cc-monitor 起的）⇒ 投不进去；住哪判不了 ⇒ 交了由那台说。
            say: match host {
                Some(Housed::Bare) => CanSay::No {
                    why: Words(copy_core::copy_text("terminal.empty.notOurs", &[])),
                },
                _ => say_yes(copy_core::copy_text("chat.input.placeholder", &[])),
            },
        },
    }
}

/// **这条会话叫什么那两格的唯一构造口**（[`NameFace`]）：`label` ＝ 历史那一层的 `search_rules::session_label`（与历史清单 · 全文搜索同一个函数）；
/// `label_project` ＝ 项目目录最后一截。
pub trait NameFaceExt: Sized {
    fn of_parts(label: String, label_project: Option<String>) -> Self;
}

impl NameFaceExt for NameFace {
    /// 已经算好的那两格（只由 `history::search_rules::name_face` 调）。
    fn of_parts(label: String, label_project: Option<String>) -> Self {
        NameFace {
            label: Words(label),
            label_project: label_project.map(Words::from),
        }
    }
}

/// 分页的那几条帧命令（`history-index` · `-user-inputs` · `-turns` · `-branch`）一条应答至多多少字节，`session_branch` 一帧同此。
///
/// 一条应答就是那台流上的一整行，写它的时候那台别的会话的实时帧都在后面等（写出方向写完一页就先让实时帧走，`stream/writer.rs::writer_task`）
/// ⇒ 页越大，实时行被挡得越久；页越小，往返越多。取 1 MiB：同 `history-read` 一页、同远端链路的窗口；
/// 一份 394 MB 的合成记录冷开一趟分页那四条共 19 页（256 KiB 要 71 页，远端每页一个往返）。
pub const REPLY_PAGE_BYTES: relay_route_core::Cap = relay_route_core::Cap {
    bytes: 1 << 20,
    over: relay_route_core::Over::Chunk,
};

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
pub trait TransferEndExt {
    fn failed(why: String, code: Option<String>, cmd: &str, raw: Option<&str>) -> TransferEnd;
}

impl TransferEndExt for TransferEnd {
    /// 跑起来之后才停下的那一形：那一句 ＋ 复制详情（命令 `cmd` · 下层原话，后端这一端写好）。
    fn failed(why: String, code: Option<String>, cmd: &str, raw: Option<&str>) -> TransferEnd {
        TransferEnd::Failed(crate::wire::detail::Failed {
            code: crate::wire::detail::Failed::code_or_fallback(code),
            said: why,
            detail: crate::wire::detail::of_run(cmd, raw),
            data: None,
        })
    }
}

/// 造帧的那几处（写字的那几种帧只经这里造）。
pub trait FrameMake {
    /// 成功、不带数据的那一种应答。
    fn ok(id: &str) -> Frame;

    /// `changed` 帧（只此一处装进帧）：发端造好那个主题那一臂；体序列化后超了主题表那一格的上限 ⇒ 不带（客户端重问）。
    fn changed(c: Changed) -> Frame;

    /// 离开「活」的那一帧（[`Life::Reconnectable`] · [`Life::Ended`] · [`Life::Gone`]；只此一处造这一帧）：
    /// `by` ＝ 顶替它的那条会话（只在已结束时作数）；`account` 是宣告时判的那个号（脸上带着）。
    fn session_left(sid: String, life: Life, by: Option<String>, account: Option<String>) -> Frame;

    /// `overflow` 帧：这条连接拥塞丢了约 `lines` 行 · 会话状态要不要（已）重新对齐，写成那一句。
    fn overflow(lines: u64, realigned: bool) -> Frame;

    /// 失败应答：那一份失败（[`crate::wire::detail::Failed`]，CLI 面的信封出自同一份）装进 `reply`。
    fn failed(id: String, f: crate::wire::detail::Failed) -> Frame;

    /// 硬臂那几条命令（`Run::Builtin`：链路 · 传输 · 终端订阅）就地被拒：详情里带命令名（同处理器回的失败）。
    fn refused(id: &str, cmd: &str, code: &str, said: &str) -> Frame;

    /// 协议级失败（还没落到哪条命令上：认不出 · 读不懂 · 收场中）：详情里没有命令那一项。
    fn err(id: &str, code: &str, said: &str) -> Frame;
}

impl FrameMake for Frame {
    fn ok(id: &str) -> Frame {
        Frame::Reply(Reply {
            id: id.to_string(),
            ok: true,
            code: None,
            said: None,
            detail: None,
            data: None,
        })
    }

    fn changed(mut c: Changed) -> Frame {
        c.cap_body(c.topic().spec().body_cap);
        Frame::Changed(c)
    }

    fn overflow(lines: u64, realigned: bool) -> Frame {
        let n = lines.to_string();
        let said = match (realigned, lines) {
            (false, _) => copy_core::copy_text("beStream.overflow.lines", &[("n", &n)]),
            (true, 0) => copy_core::copy_text("beStream.overflow.realigned", &[]),
            (true, _) => copy_core::copy_text("beStream.overflow.realignedLines", &[("n", &n)]),
        };
        Frame::Overflow(Overflow { said: Words(said) })
    }

    fn session_left(sid: String, life: Life, by: Option<String>, account: Option<String>) -> Frame {
        let mut face = SessionFace::of(life).under(account);
        if let WireLife::Ended { by: b } = &mut face.life {
            *b = by;
        }
        Frame::SessionLeft(SessionLeft { sid, face })
    }

    fn failed(id: String, f: crate::wire::detail::Failed) -> Frame {
        Frame::Reply(Reply::refused(id, f))
    }

    fn refused(id: &str, cmd: &str, code: &str, said: &str) -> Frame {
        let f = crate::wire::detail::failed(Some(cmd), code, said.to_string(), None, None);
        Frame::failed(id.to_string(), f)
    }

    fn err(id: &str, code: &str, said: &str) -> Frame {
        let f = crate::wire::detail::failed(None, code, said.to_string(), None, None);
        Frame::failed(id.to_string(), f)
    }
}

/// 帧上的规矩：丢了能不能恢复 · 是不是某个会话的整行内容 · 主线外清单分页 · 推出去那一下写钟面。
pub trait FrameRules {
    /// **丢了还能不能恢复**：出方向那条大通道按帧种分「丢了别处还在」与「丢了就没了」（应答走独立通道、阻塞背压，不在此列）。
    /// 拿不准的一律按不可恢复算（错判成可恢复 ＝ 静默丢数据；错判成不可恢复 ＝ 多一次就地重报）。
    /// ⚠ **穷尽 `match`，不许 `_ =>`** —— 新增帧种时编译期就被逼着表态。
    fn loss_is_recoverable(&self) -> bool;

    /// 这一帧是不是**某个会话的整行内容**（回那个会话的 id）：流只发要看的会话（`stream-watch`），
    /// 不在这条流的名单里的会话，这几种帧不上流；别的（宣告 · 状态 · 轮次边沿 · 去向 · 文件重读 / 不在了 · 推送 …）照发。
    ///
    /// 穷尽 `match`，同 [`Self::loss_is_recoverable`]：新增帧种时编译期就得表态。
    fn content_of(&self) -> Option<&str>;

    /// 一份主线外清单「前 `at` 条照旧，其后是 `off[at..]`」切成几帧 `session_branch`（每帧至多 `REPLY_PAGE_BYTES` 字节，一条单独超了 ⇒ 那一帧只有它）。
    /// 后面没有了（`at` 就是全长）也发一帧空的：它说「清单到第 `at` 条为止」。
    fn branch_pages(sid: &str, path: &str, off: &[String], at: usize) -> Vec<Frame>;

    /// **推出去那一下**按这条流看的那一台的时区写钟面（`stream/writer.rs::writer_task` 对出方向每一帧调它；应答在答的那一下已按请求的 `tz` 写好）：
    /// `line.record.timeText` · `session_runs.runs[].startedText`。产这些帧的那几层不写钟面（同一份状态可能推给几个时区不同的看的人）。
    fn stamp(&mut self, tz: &crate::common::time::Tz);
}

impl FrameRules for Frame {
    fn loss_is_recoverable(&self) -> bool {
        match self {
            // 内容帧：行确实还在远端 jsonl 里，重开会话/重读就补上。
            Frame::Line(_) => true,

            // ↓ 以下都是「丢了别处没有」或「拿不准」，一律按不可恢复算。
            //
            // 一次差分的结果，别处不存在 —— 这三个正是 B-3 的正题。
            Frame::SessionAdded(_) => false,
            // 一次性的「读完了、从第几行起」：没有「下一次必然重发」⇒ 丢了客户端不知道从哪接、也不拉快照。
            Frame::SessionPrimed(_) => false,
            // 账本的成品：一次裁决的结果，别处没有 ⇒ 不可恢复。
            Frame::SessionLeft(_) => false,
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
            //   不可丢的（账号清单 · 配置文件 · 任务清单）是一次变化的通知，没有「下一次必然重发」⇒ 丢了那台在重报里再发一帧（不带体），客户端重问。
            Frame::Changed(c) => c.topic().spec().lossy,
            // 一次性的标记，没有「下一次必然重发」⇒ 丢了客户端就一直停在「说不清」
            //   （保守的那一侧：不会把一条说不清的会话说成已结束）。按不可恢复算，那台就地重报一次。
            Frame::SessionsReplayed(_) => false,
            // 一次性的出声，没有「下一次必然重发」⇒ 丢了那个 tab 就不说那句话（内容本身照旧对：游标已按它处置）。
            Frame::SessionFileGone(_) => false,
            Frame::SessionFileReread(_) => false,
            // 传输的进度 / 终局：丢了终局那一帧，看的人永远等下去；走应答通道（阻塞发送，不走会丢帧的那条）。
            Frame::Transfer(_) => false,
            // 测试连接的进度 / 结局：同上（走应答通道）。
            Frame::Probe(_) => false,
            // 一组的进度：只是快照（结局在应答里），但走应答通道、不丢。
            Frame::FilesBatch(_) => false,
            // SSE 只保快：它说的事 jsonl 那一侧都有（落盘保对），丢了由位置号 `n` 原位说出来。
            // ⚠ 它**不走**出方向那条通道（走 tap 自己那条），列在这里只为穷尽。
            Frame::LiveCard(_) => true,
            // 整份快照：丢了那台就地重报时再给一份（重报里有在看的会话的运行表）。
            Frame::SessionRuns(_) => false,
            // 只发变了的那一截：丢了那一帧，那条连接手上的清单就对不上 —— 发帧那一层记下（`FrameSink`），下一次给它整份。
            Frame::SessionBranch(_) => false,
            // 终端实时预览：丢了在途那一帧，客户端等不到它就不回执，订阅停住；也走应答通道。
            Frame::TerminalScreen(_) => false,
            Frame::TerminalFollowEnd(_) => false,
            // 别台那条流上的事：丢一行那台那条连接就对不上了 —— 发帧那一层记下（`FrameSink`），下一帧之前补那台一格 `gap`。
            Frame::Relayed(_) => false,
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
            | Frame::SessionLeft(_)
            | Frame::Overflow(_)
            | Frame::Reply(_)
            | Frame::Cancelled(_)
            | Frame::Changed(_)
            | Frame::SessionsReplayed(_)
            | Frame::SessionFileGone(_)
            | Frame::SessionFileReread(_)
            | Frame::Transfer(_)
            | Frame::Probe(_)
            | Frame::FilesBatch(_)
            | Frame::TerminalScreen(_)
            | Frame::TerminalFollowEnd(_)
            | Frame::Relayed(_) => None,
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
        let room = REPLY_PAGE_BYTES.as_len().saturating_sub(fixed);
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
                        let hm =
                            crate::common::time::ms_hm(i64::try_from(ms).unwrap_or(i64::MAX), tz);
                        copy_core::copy_text("agentWindow.facts.started", &[("time", &hm)])
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
        BranchSum(cmd_wire::fnv::FNV64_BASIS)
    }
}

impl BranchSum {
    fn push(&mut self, id: &str) {
        self.0 =
            cmd_wire::fnv::fnv1a64_from(cmd_wire::fnv::fnv1a64_from(self.0, id.as_bytes()), b"\n");
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

pub mod b64;
pub use b64::{b64_decode, b64_encode};

/// 「这条流在看哪几个会话」那一问（`stream-watch`，attach 行的 `watch` 同形）的入参与应答：住 `cmd-wire`（客户端同一份）。
pub use cmd_wire::watch::{BranchHeld, WatchAsk, WatchFrom, WatchReply};

/// **入方向**请求信封。只 `Deserialize` —— backend 是读的那一方。
///
/// ```text
/// {"id":"<opaque>","cmd":"<name>","args":{...},"within_ms":10000,"tz":"Asia/Shanghai","view":{"omit":{"record":["blocks[type=tool_use].input"]}}}
/// ```
///
/// `id` **不透明**：backend 不解析、不校验格式、只回显。谁生成谁负责唯一 —— 客户端。
/// backend 自己发号的话，重连后号段会撞（会变的东西不当持久键）。
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
    pub tz: crate::common::time::Tz,
    /// 分派那一层由 `within_ms` 减余量换成的截止时刻（不上线）。
    #[serde(skip)]
    pub until: Option<crate::platform::child::Until>,
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
pub fn within_ms_of(v: &serde_json::Value) -> Option<u64> {
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

/// **Hello 已经写出并 flush 的见证。**
///
/// # 为什么要一个类型
///
/// 「入方向 reader 必须在 Hello flush 之后才起」是**时序约束**。按源码里字符串的字节位置判先后，
/// 对**控制流**与**函数边界**都是瞎的：把 `inbound::spawn(` 的调用点包进一个函数放到文件后段，
/// 位置看着在后，实际调用早在 Hello 之前。
///
/// 所以不在判据上加正则，让违规不可表示：
/// 拿不到 `HelloFlushed` 就调不了 `inbound::spawn`，而它**只能由真的写完并 flush 了一帧
/// 才能产出**（构造函数私有，唯一出口是 [`write_and_flush_hello`]）。
pub struct HelloFlushed(());

impl HelloFlushed {
    /// 只给测试用的见证。
    ///
    /// 生产路径拿不到它 —— 测试档与 `test-support`（只由上层的 dev 依赖开）在 release 构建里都不存在，
    /// 所以「不可表示」这条性质不受它影响。
    #[cfg(any(test, feature = "test-support"))]
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
/// 全仓 seq 只有这一个生成器（本机会话也走本机后端的 `line` 帧）。
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
    /// 重读那一趟照数不出行，读完发 `session_file_reread`（带行数；下游据它把这个会话的旧号整份作废）。
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

/// 读一份 hello 帧成人读摘要（`build=.. arch=.. cmds=..`）：测试连接（`dial/probe.rs`）读**那台**后端的 hello 时进日志。
pub fn hello_summary(h: &serde_json::Value) -> String {
    use serde_json::Value;
    let s = |k: &str| h.get(k).and_then(Value::as_str).unwrap_or("?").to_string();
    let cmds: Vec<&str> = h
        .get("commands")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    format!(
        "build={} arch={} cmds={cmds:?}",
        s("build_id"),
        s("host_arch")
    )
}

// ── 第四面：本机到底缺什么（hello 的 `unavailable` 那一格用的两个码） ───────────
/// 命令级 code —— **「这台机器上没有 tmux」**。
///
/// ⚠ 这是这个字面量在仓里的第 N 份，但它**不是第 N 个源头**：真相是
/// 主 crate 命令表 `stream::inbound::REGISTRY` 里各条命令自己登记的 `codes`，本常量只拿它去**查那张表**
/// （`stream::inbound::unavailable_from`）。查不到就红（`the_declared_code_is_one_the_registry_already_declares`）⇒
/// 谁把那边的拼写改了，这边不会静默跟丢。
pub const NO_TMUX: &str = "no_tmux";

/// 命令级 code —— **「这个平台没有 unix 权限位」**。
///
/// 只有 `files-chmod` 声明它（`files/write.rs::change_mode_reporting` 在非 unix 上回它）。
/// 与 [`NO_TMUX`] 同形：真相是命令表里命令自己登记的 `codes`，本常量只拿去查那张表；
/// `hello.unavailable` 经 `stream::inbound::unix_mode_unavailable` 读它。
pub const NO_UNIX_MODE: &str = "no_unix_mode";

// 上层测试读的那几样夹具（二前端读的字段表 · 全部主题 · 带一行字的记录）：本包与上层判据经 `test-support` 共用。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../tests/backend/wire/wire_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "../../../tests/backend/wire/wire_tests.rs"]
mod tests;
