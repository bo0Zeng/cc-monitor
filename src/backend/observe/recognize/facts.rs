//! **会话事实**：分叉血缘 · 改动文件集 · 最新 usage（连同上下文上限）· 项目目录。
//! 子 agent 的列表与状态不在这里：它们是运行表（`observe::runs`，经 `session_runs` 帧），判定只有那一处。
//!
//! # 为什么在后端算
//!
//! 这几样要按对话序、读全量算：活 tab 在 `onLine` 旁路上攒的话，
//! **到达序不是对话序**（重放是尾块先到）、**不完整**（重放缓冲每个会话只留尾部 `REPLAY_TAIL_KEEP` 条 ⇒
//! F5 之后长会话的分叉血缘看不见、agent / 改动文件只剩尾巴那一截）、每个 tab 各攒一份（那三个病）。
//! 所以由本文件读一遍文件算出来，经帧命令 `history-facts`（宿主 `read_face.rs`）出成品，
//! 界面经通道直接问、按形状收（`src/frontend/ui/session-reads.ts`），**本机与远端同一条路**。
//!
//! # 续算住哪
//!
//! 事实要跟着会话长，而大会话每批整份重扫是撞墙的：调用方只交路径，累加器由后端按路径留着（`observe::record_scan`：
//! 扫描图在 ⇒ 图接着扫追加的那一截；图被挤掉 ⇒ 它留下的事实尾巴接着读）。判定与累加都只在这里（[`note_record`] · [`note_line`]）。
//!
//! # 口径（逐格，三样各一个住址）
//!
//! | 格 | 口径 |
//! |---|---|
//! | `forkedFrom` | 一拍的 `forked_from`（那一家答；Claude 与历史会话行同一个函数）；首条命中即锁定 |
//! | `touchedFiles` | 代理那侧的工具调用里写类工具的路径（一拍的 `Call.edits`，写类工具表是那一家的），去重、**近因序**（再碰一次移到末尾），至多 [`TOUCHED_FILES_KEEP`] 条 |
//! | `usage` | 代理那侧一拍的用量里新输入 · 读缓存 · 写缓存之和 > 0 ⇒ `{promptTokens, model}`，文件序最后一条胜；`peakPromptTokens` 取全会话最大；上限见 [`context_limit`] |
//! | `projectDir` | 适配层 `RecordFace.project_dir`（只读记录开头）；读到即锁定，不在本文件的逐行扫描里 |
//! | `writers` | 这台 pidfile 里此刻持着这条会话的活进程（`observe::accounts_query::session_writers`）；每次现查，不在逐行扫描里 |
//! | `pending` | 代理那侧的工具调用，等到用户那侧同 id 的工具结果才摘；人又发了一句（用户那侧没有工具结果）⇒ 全摘。文件序，至多 [`PENDING_KEEP`] 条 |
//! | `lastSay` | 文件序最后一段代理那侧正文的头一个非空行，至多 [`SAY_CHARS`] 字 |
//! | `pending[].state` / `.why` | 每次现判（[`settle_pending`]）：那台说在等的正是这一步 ⇒ `awaiting` · 有活进程持着这条会话 ⇒ `running` · 否则 `unclear`（不当它在跑），`why` 说为什么判不了（[`UnclearWhy`]） |
//! | `retries` | 一串相邻的上游重试（一拍的 `Retry`）按首条的 id 记一件，结局看它后面第一条代理那侧 / 人发的（不是注入、不带工具结果）用户那侧（[`RetryOutcome`]）；别的系统记录不算下文。文件序，至多 [`RETRY_KEEP`] 件 |
//! | `needs` | 那台 pidfile 说在等（[`PidWait`]）⇒ 配上 `pending` 判种类（[`needs_of`]）；每次现查，不累加 |
//! | `bgTasks` | 后台命令（适配层翻好的 [`crate::agents::BgMark`]）：起了一条 ⇒ 记下（工具调用 id · 命令原样 · 那条记录的 `timestamp`）；拿到任务号 ⇒ 补上；收场通知 / 当场回的结果出错 ⇒ 摘。文件序，至多 [`BG_KEEP`] 条 |
//! | `background` | 那台 pidfile 说「一轮停了、后台命令还在跑」⇒ 状态一句（[`background_of`]）；每次现查，不累加 |
//! | `mcp` | 适配层 `agents::mcp_said_of` 说的三张表（要登录 · 断开 · 还在连），每张以文件序最后一次写它的那一条为准；按名字排 |
//! | `quietUnread` | 认不出、又不成卡的行（适配层 `agents::quiet_unread_of` 判：没见过的顶层类型 ＋ 没有链上身份 ⇒ 那一家的簿记行）：几行 · 哪几种（首见序，至多 [`QUIET_KINDS_KEEP`] 种，再多只加数）· 写好的一句；一行都没有 ⇒ `null` |
//! | `handedBack` | 交回了的子运行：用户那侧「谁说的」是 agent 交回（一拍的 `Said` 是 `AgentMessage { handback: true }`）⇒ 它的 `from`；去重、文件序，至多 [`HANDED_BACK_KEEP`] 条。同一个子运行的收场通知（`taskNotification.taskId` ＝ 这个 id）以交回为准，界面不再另画 |
//!
//! # 快路
//!
//! 一行要不要解析，先按字节看它有没有可能改动事实（[`could_matter`]）：格式那一半问那一家（`BeatFace.hint`，比如分叉已锁就不找分叉那一格、
//! 工具结果那一大类连解析都不做），挂着的调用 / 后台命令的 id 在不在行里这一半在这里看。**只省时间、不改结果**：
//! 由判据逐行对拍「过滤 / 不过滤」两向相等。

use crate::agents::beat::{contains, one_line, Act, Beat, Side, StopWhy, Watch};
use crate::common::cells::Words;
pub(crate) use cmd_wire::needs::{Needs, NeedsKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 没结果的工具调用至多留几条（并发调用一批也就几条；超 ⇒ 丢最早的）。
pub(crate) const PENDING_KEEP: usize = 16;

/// 重试至多留多少串（超 ⇒ 丢最早的）。
pub(crate) const RETRY_KEEP: usize = 200;

/// 还没收场的后台命令至多留几条（超 ⇒ 丢最早的）。
pub(crate) const BG_KEEP: usize = 16;

/// 交回了的子运行至多留多少个（超 ⇒ 丢最早的）。回包有上限，一个 id 几十字节。
pub(crate) const HANDED_BACK_KEEP: usize = 500;

/// `quietUnread` 至多记多少种类型（再多的只加行数）。
pub(crate) const QUIET_KINDS_KEEP: usize = 16;
/// 改动文件集至多留多少条（超 ⇒ 丢最久没碰的）。回包有上限：
/// 1000 条 × 路径长（百来字节）≈ 140 KB；实际会话里多不过几十条。
pub(crate) const TOUCHED_FILES_KEEP: usize = 1000;

/// 一份会话的事实（**帧面成品的形状，键名一字不差**；跨语言金样 `tests/__fixtures__/session-reads.golden.json`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct SessionFacts {
    /// 源会话 sid；不是分叉来的 ⇒ `null`。
    pub(crate) forked_from: Option<String>,
    /// 写类工具碰过的文件（原样，可能相对 / Windows 路径），近因序：最近碰的在末尾。
    pub(crate) touched_files: Vec<String>,
    /// 最后一条带有效 usage 的 assistant 记录；一条都没有 ⇒ `null`。
    pub(crate) usage: Option<UsageFact>,
    /// `usage.model` 写给人看的名字（那一家的模型那一处给：`agents::model_text`）；没有 usage · 认不出是哪一家 · 这一家认不出这个值 ⇒ `null`
    /// （界面照抄，不自己拼）。每次现给，不累加。
    pub(crate) model_text: Option<Words>,
    /// 会话的项目目录（会话起在哪个目录）：适配层读记录开头给（`agents::project_dir_of`），读到即锁定；
    /// 开头里还没有 ⇒ `null`，下一次再读。与会话宣告那一帧的 `project_dir` 同一个函数。
    pub(crate) project_dir: Option<String>,
    /// 这份记录是哪一家的（线上的 kind，适配层按记录认：`agents::record_kind_of`）；认不出 ⇒ `null`（界面要分家的那几项灰着，不落哪一家）。
    pub(crate) agent: Option<String>,
    /// 此刻持着这条会话的活进程 pid（`accounts_query::session_holders`：照那一家声明的判活方式查），升序；不累加，每次现查。
    /// 不止一个 ⇒ 同一条会话有几个进程在同时写。判不了 ⇒ 空。
    pub(crate) writers: Vec<u32>,
    /// 还没有结果的工具调用（文件序，正在跑 / 在等批准的那几步）。
    pub(crate) pending: Vec<PendingCall>,
    /// 最后一段正文的头一行（悬停卡「它最后一句」）。
    pub(crate) last_say: Option<LastSay>,
    /// **需手动**：那台说在等、等的是什么（不累加，每次现查）。不在等 ⇒ `null`。
    pub(crate) needs: Option<Needs>,
    /// 交回了的子运行（子 agent 的 id，文件序、去重）：同一个子运行的收场通知以交回为准，只报一次。
    pub(crate) handed_back: Vec<String>,
    /// 一串一串的 API 重试与结局（按首条的 `uuid`，文件序）；最后一串可能还没有下文（`retrying`）。
    pub(crate) retries: Vec<RetryRun>,
    /// 此刻的许可档（最后一条许可档记录写的那一档，原样）；没有 ⇒ `null`。
    pub(crate) permission_mode: Option<String>,
    /// 同一档写给人看的那一句（那一家的权限档那一处给：`agents::permission_mode_text`，切档回包的 `modeText` 同一处）；
    /// 没有档 · 认不出是哪一家 · 这一家认不出这一档 ⇒ `null`（界面照抄，不自己拼）。每次现给，不累加。
    pub(crate) permission_mode_text: Option<Words>,
    /// 全会话用量（按请求去重）；一条带用量的回复都没有 ⇒ `null`。
    pub(crate) tokens: Option<TokenUse>,
    /// 全会话花费（记录里那一家自己记的花费那一条，最后一条为准）；记录里没有 ⇒ `null`（不按定价自己算）。
    pub(crate) cost: Option<Cost>,
    /// 还没收场的后台命令（文件序）：累加的那一份账。
    pub(crate) bg_tasks: Vec<BgTask>,
    /// **后台任务运行中**：那台说一轮停了、后台命令还在跑 ⇒ 状态一句（不累加，每次现查）。不是这一态 ⇒ `null`。
    pub(crate) background: Option<Background>,
    /// **答题口没接上**：ccm 起这条会话时挂不上答题 hook（用户自己给的会话级设置读不出 / 形状不认）· 不是 ccm 起的、持着它的进程身上看不出挂过 ⇒ 写好的那一句
    /// （「这条会话只能在终端里答批准」；不累加，每次按持着它的进程现查）。接上了 / 判不了 ⇒ `null`。
    pub(crate) answer_off: Option<Words>,
    /// 这个会话里那一家说有毛病的 MCP 服务器（要登录 · 断开 · 还在连），按名字排；没列的不等于连上了。
    pub(crate) mcp: Vec<McpTrouble>,
    /// 由谁启动（会话血缘里它的主会话）；不是谁起的 ⇒ `null`。不累加，每次现查。
    #[serde(default)]
    pub(crate) started_by: Option<Kin>,
    /// 它起的子会话（按起的先后）；不累加，每次现查。
    #[serde(default)]
    pub(crate) children: Vec<Kin>,
    /// 认不出、又不成卡的行（那一家的簿记行：没见过的类型、没有链上身份）：不在消息流里画，在这里照实说几行、哪几种。一行都没有 ⇒ `null`。
    pub(crate) quiet_unread: Option<QuietUnread>,
}

/// 会话血缘里的另一条会话（会话头「由 … 启动」· 「子会话 N」那一列）：编号 · 叫它什么 · 活不活 · 隐不隐藏（判定住 `control/session_hidden.rs`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Kin {
    pub(crate) sid: String,
    pub(crate) title: String,
    /// 此刻活着（这台 pidfile 说的）。
    pub(crate) live: bool,
    /// 隐藏那一格；不隐藏 ⇒ `null`（格恒在）。
    pub(crate) hidden: Option<cmd_wire::session::Hidden>,
}

/// 认不出、又不成卡的那几行（会话事实 `quietUnread`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct QuietUnread {
    /// 「无法识别 N 行（类型：a · b）」。
    pub(crate) text: Words,
    /// 几行。
    pub(crate) count: u32,
    /// 哪几种（原文里的 `type`，首见序，至多 [`QUIET_KINDS_KEEP`] 种）。
    pub(crate) kinds: Vec<String>,
}

/// 一条还没收场的后台命令。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BgTask {
    /// 起它的那次工具调用 id。
    pub(crate) call: String,
    /// 它当场拿到的任务号；还没回 ⇒ `null`。
    pub(crate) task: Option<String>,
    /// 那条命令原样（适配层给的）；没有 ⇒ `null`。
    pub(crate) cmd: Option<String>,
    /// 起它的那条记录的 `timestamp` 原样；没有 ⇒ `null`。
    pub(crate) at: Option<String>,
}

/// 后台任务运行中那一态的成品：写好的一句（发出那一刻的钟）· 会走的那一句（`{dur}` 由桌面那一个读口填）·
/// 命令那一格（监控板徽标用）· 几条 · 语气。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "FactsBackground"
    )
)]
pub(crate) struct Background {
    /// 「后台任务运行中 · make test-all · 12m」；命令拿不到 ⇒ 「后台任务运行中」。手机与 CLI 照抄、按节拍重问。
    pub(crate) text: Words,
    /// 同一句、时长那一截留 `{dur}`：`{text, from}`，桌面填 现在 − `from`（`duration-format.ts::fmtDur`，与 `copy_core::short_duration` 对同一份金样）。
    /// 命令或起始时刻拿不到 ⇒ `null`。
    pub(crate) clock: Option<Clock>,
    /// 命令那一格（「make test-all」·「python train.py 等 2 条」）；拿不到 ⇒ `null`。
    pub(crate) what: Option<Words>,
    /// 还在跑的后台命令几条（这一次进程起来之后起的；记录里一条都对不上 ⇒ 0）。
    pub(crate) count: u32,
    /// 语气（恒 `busy`）。
    pub(crate) tone: crate::common::cells::Tone,
}

/// 会走的一句：字里的 `{dur}` 填 现在 − `from`（epoch ms）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "FactsClock"
    )
)]
pub(crate) struct Clock {
    pub(crate) text: Words,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) from: u64,
}

/// 时长那一截的占位（桌面那一个读口填）。
const DUR_SLOT: &str = "{dur}";

/// **后台任务运行中那一句的唯一一处**：`tasks` ＝ 记录里还没收场的后台命令；`born_ms` ＝ 那台持着这条会话、说这一态的进程何时起的
/// （后台命令活不过它的进程：更早起的那几条是上一个进程留下的、早被掐了，不算）；`now_ms` ＝ 这台此刻。
/// 命令取最早起的那条（主参数一行，同悬停卡 peek 的截法）；不止一条 ⇒ 「{cmd} 等 {n} 条」；时长 ＝ 它起了多久。
pub(crate) fn background_of(tasks: &[BgTask], born_ms: Option<u64>, now_ms: u64) -> Background {
    let at = |t: &BgTask| {
        t.at.as_deref()
            .and_then(crate::common::time::parse_iso8601_ms)
            .and_then(|ms| u64::try_from(ms).ok())
    };
    let mut open: Vec<(&BgTask, Option<u64>)> = tasks
        .iter()
        .map(|t| (t, at(t)))
        .filter(|(_, a)| match (a, born_ms) {
            (Some(a), Some(b)) => *a >= b,
            _ => true,
        })
        .collect();
    open.sort_by_key(|(_, a)| a.unwrap_or(u64::MAX));
    let count = u32::try_from(open.len()).unwrap_or(u32::MAX);
    let word = || {
        Words(copy_core::copy_text(
            "beSession.activity.backgroundWork",
            &[],
        ))
    };
    let first = open.first();
    let cmd = first.and_then(|(t, _)| t.cmd.as_deref()).and_then(one_line);
    let what = cmd.map(|c| {
        if count > 1 {
            copy_core::copy_text(
                "beSession.activity.backgroundMany",
                &[("cmd", &c), ("n", &count.to_string())],
            )
        } else {
            c
        }
    });
    let from = first.and_then(|(_, a)| *a);
    let (text, clock) = match (&what, from) {
        (Some(w), Some(from)) => {
            let line = |dur: &str| {
                Words(copy_core::copy_text(
                    "beSession.activity.backgroundFor",
                    &[("cmd", w), ("dur", dur)],
                ))
            };
            (
                line(&copy_core::short_duration(now_ms.saturating_sub(from))),
                Some(Clock {
                    text: line(DUR_SLOT),
                    from,
                }),
            )
        }
        _ => (word(), None),
    };
    Background {
        text,
        clock,
        what: what.map(Words),
        count,
        tone: crate::common::cells::Tone::Busy,
    }
}

/// 一笔后台命令的账记到 `f` 上（口径见头注那张表）。
fn note_background(f: &mut SessionFacts, kind: &str, v: &Value, at: Option<&String>) {
    use crate::agents::BgMark;
    for m in crate::agents::background_marks(kind, v) {
        match m {
            BgMark::Started { call, cmd } => {
                f.bg_tasks.retain(|t| t.call != call);
                f.bg_tasks.push(BgTask {
                    call,
                    task: None,
                    cmd,
                    at: at.cloned(),
                });
                if f.bg_tasks.len() > BG_KEEP {
                    f.bg_tasks.remove(0);
                }
            }
            BgMark::Named { call, task } => {
                if let Some(t) = f.bg_tasks.iter_mut().find(|t| t.call == call) {
                    t.task = Some(task);
                }
            }
            BgMark::Ended { call, task } => f.bg_tasks.retain(|t| {
                !(call.as_deref() == Some(t.call.as_str()) || (task.is_some() && task == t.task))
            }),
        }
    }
}

/// 会话里一个有毛病的 MCP 服务器。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct McpTrouble {
    pub(crate) name: String,
    /// `needsLogin` · `failed` · `pending`（[`crate::agents::McpStatus`] 的这三种）。
    pub(crate) status: crate::agents::McpStatus,
    /// 断开时那一家写的原话（复制详情用，不当句子显示）；别的 ⇒ `null`。
    pub(crate) detail: Option<String>,
    /// 说它的那条记录的 `timestamp` 原样；没有 ⇒ `null`。
    pub(crate) at: Option<String>,
}

/// 全会话用量：同一次请求写出的几条回复只算一次（取最后一条的数）；写缓存分 5 分钟 / 1 小时两档（原文没分档 ⇒ 整份算 5 分钟档）。
/// `text` 是写好的成品串（随数一起更新）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct TokenUse {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) input: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) output: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) cache_read: u64,
    #[serde(rename = "cacheWrite5m")]
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) cache_write5m: u64,
    #[serde(rename = "cacheWrite1h")]
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) cache_write1h: u64,
    /// 算进来的请求数。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) requests: u64,
    /// 写好的串（输入 · 输出 · 读缓存 · 写缓存）。
    pub(crate) text: Words,
    /// 上一次请求（接着算时同一次请求的后一条要替掉它）：键 · 那一次的五个数。不上线，骨架落它（[`skel`]）。
    #[serde(skip)]
    #[cfg_attr(test, ts(skip))]
    pub(crate) last: Option<LastRequest>,
}

/// [`TokenUse::last`]。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LastRequest {
    pub(crate) id: String,
    pub(crate) tokens: [u64; 5],
}

/// 全会话花费：记录里那一家自己记的数（微美元）· 它说有没有定不了价的型号 · 写好的串（币种、精度、「约」都在里面）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "FactsCost"
    )
)]
pub(crate) struct Cost {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) micros: u64,
    pub(crate) partial: bool,
    pub(crate) text: Words,
}

/// 一串相邻的 API 重试。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RetryRun {
    /// 这一串首条重试记录的 `uuid`（界面那条细条就挂在它上面）。
    pub(crate) id: String,
    pub(crate) outcome: RetryOutcome,
}

/// 一串重试的结局：还没下文 · 后面来了正常回复（接上了）· 来了报错那条（重试耗尽 / 不可重试）· 人发了一句或打断。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) enum RetryOutcome {
    Retrying,
    Recovered,
    Failed,
    Interrupted,
}

/// 一个还没有结果的工具调用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PendingCall {
    /// 工具调用 id（等它的 `tool_result` 来摘）。
    pub(crate) id: String,
    /// 工具名原样。
    pub(crate) name: String,
    /// 主参数一行（那一家的「工具 → 主参数」表；提问取第一问）；没登记 / 入参里没有 ⇒ `null`。
    pub(crate) what: Option<String>,
    /// 那条记录的 `timestamp` 原样（从何时起在跑）；没有 ⇒ `null`。
    pub(crate) at: Option<String>,
    /// `at` 的毫秒（自 1970）：出口算「已经跑了多久」只用这一格（交那一个读口走字），不自己解析 `at`；解不出 ⇒ `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) at_ms: Option<i64>,
    /// 这一步此刻的样子（[`settle_pending`] 每次现判，不累加）。
    pub(crate) state: StepWait,
    /// `state` 是 `unclear` 时为什么判不了；别的 ⇒ `null`。
    pub(crate) why: Option<UnclearWhy>,
    /// 过程那一行右侧那一句（状态不明 ⇒「状态不明」；在跑 · 在等你 ⇒ `null`，在等你那一格出口写已等多久）。
    pub(crate) text: Option<Words>,
    /// 状态不明时那一行悬停说的为什么（照 `why` 写好）；别的 ⇒ `null`。
    pub(crate) why_text: Option<Words>,
}

/// 一步状态不明的原因：没有活进程持着这条会话 · 判不了活（这一家没声明怎么判，或这台认不出写者）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) enum UnclearWhy {
    NoWriter,
    Untracked,
}

/// 一步还没有结果时的样子：在跑（有活进程持着这条会话）· 在等你（那台说在等的正是这一步）· 说不清（没有活进程持着它，
/// 或判不了活）。有了结果之后是完成 / 失败（那条结果的成品 `toolResults` 说），不在这里。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) enum StepWait {
    Running,
    Awaiting,
    #[default]
    Unclear,
}

/// **一步还没结果时的样子的唯一判定**：那台说在等的正是它 ⇒ 在等你；否则有活进程持着这条会话 ⇒ 在跑；否则状态不明
/// （`tracked` ＝ 判得了活（`session_holders` 回了 `Some`）；判不了 ⇒ 原因 `untracked`，判得了 ⇒ `noWriter`）。要在 `writers` 与 `needs` 现查之后调。
pub(crate) fn settle_pending(f: &mut SessionFacts, tracked: bool) {
    let waiting_on = f.needs.as_ref().and_then(|n| n.call.clone());
    let live = !f.writers.is_empty();
    for p in &mut f.pending {
        (p.state, p.why) = if waiting_on.as_deref() == Some(p.id.as_str()) {
            (StepWait::Awaiting, None)
        } else if live {
            (StepWait::Running, None)
        } else if tracked {
            (StepWait::Unclear, Some(UnclearWhy::NoWriter))
        } else {
            (StepWait::Unclear, Some(UnclearWhy::Untracked))
        };
        p.text = (p.state == StepWait::Unclear)
            .then(|| Words(copy_core::copy_text("stream.step.unclear", &[])));
        p.why_text = p.why.map(|w| {
            Words(match w {
                UnclearWhy::NoWriter => copy_core::copy_text("stream.step.unclearNoWriter", &[]),
                UnclearWhy::Untracked => copy_core::copy_text("stream.step.unclearUntracked", &[]),
            })
        });
    }
}

/// 最后一段正文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct LastSay {
    pub(crate) text: String,
    /// 那条记录的 `timestamp` 原样；没有 ⇒ `null`。
    pub(crate) at: Option<String>,
    /// `at` 的毫秒（同 [`PendingCall::at_ms`]：「完成多久前」交那一个读口走字）。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) at_ms: Option<i64>,
}

/// 那台 pidfile 说「在等」（`observe::accounts_query::session_wait`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PidWait {
    /// 在等什么框（适配层翻好的 [`crate::agents::WaitOn`]）；没说 / 说不清 ⇒ `None`。
    pub(crate) waiting_for: Option<crate::agents::WaitOn>,
    pub(crate) since_ms: Option<u64>,
    /// 读那份 pidfile 的时刻（epoch ms，**同一台的钟**）：「已等多久」＝ 它减 `since_ms`。
    pub(crate) read_at_ms: u64,
}

/// **「需手动」的唯一判定**：那台说在等什么框 ＋ 记录里最早一个还没结果的调用。
/// - 批准框（或没说是哪种框）：那个调用是提问工具 ⇒ 回答（问题原文）；是计划工具 ⇒ 批准计划；
///   批准框里别的工具 ⇒ 批准（那一步的主参数）；没说是哪种框且不是那两种工具 ⇒ 判不出，不猜；
/// - 要填 / 要答 ⇒ 回答；联网 ⇒ 放行（那一步多半就是要联网的那条命令）；
/// - 协作请求 · 会话目标 · 别的对话框 ⇒ 各一种，不挂记录里的哪一步（那一步不在这份记录里 / 不是一步工具调用）。
///   这几种是顶上那个框：底下就算有提问 / 计划，也是先答它。
pub(crate) fn needs_of(
    agent: &str,
    pending: &[PendingCall],
    wait: Option<&PidWait>,
) -> Option<Needs> {
    use crate::agents::beat::CallAsk;
    use crate::agents::WaitOn as W;
    let wait = wait?;
    let first = pending.first();
    let ask_is =
        |want: CallAsk| move |p: &&PendingCall| crate::agents::call_ask(agent, &p.name) == want;
    let asks = pending.iter().find(ask_is(CallAsk::Answer));
    let plan = pending.iter().find(ask_is(CallAsk::Plan));
    let (kind, call) = match wait.waiting_for {
        Some(W::Permission) | None if asks.is_some() => (NeedsKind::Answer, asks),
        Some(W::Permission) | None if plan.is_some() => (NeedsKind::Plan, plan),
        Some(W::Permission) => (NeedsKind::Approve, first),
        None => (NeedsKind::Unknown, None),
        Some(W::Input) => (NeedsKind::Answer, asks.or(first)),
        Some(W::Network) => (NeedsKind::Network, first),
        Some(W::Worker) => (NeedsKind::Worker, None),
        Some(W::Goal) => (NeedsKind::Goal, None),
        Some(W::Dialog) => (NeedsKind::Choose, None),
    };
    let rank = NEEDS_BY_DANGER
        .iter()
        .position(|k| *k == kind)
        .map_or(u8::MAX, |i| i as u8);
    let waited_ms = wait.since_ms.map(|t| wait.read_at_ms.saturating_sub(t));
    Some(Needs {
        kind,
        tool: call.map(|c| c.name.clone()),
        call: call.map(|c| c.id.clone()),
        what: call.and_then(|c| c.what.clone()),
        since_ms: wait.since_ms,
        text: needs_words(kind),
        head: needs_head(kind, call.map(|c| c.name.as_str())),
        head_code: match kind {
            NeedsKind::Approve | NeedsKind::Answer | NeedsKind::Network => {
                call.and_then(|c| c.what.clone())
            }
            _ => None,
        },
        tone: crate::common::cells::Tone::Need,
        rank,
        waited_ms,
        waited_text: waited_ms.map(|ms| Words(copy_core::short_duration(ms))),
        // 配不配挂着的请求在顶层壳（`faces::read_face` 查常驻进程那一份登记表，经 [`dress_ask_with`] 配）。
        ask: None,
        manual_text: None,
    })
}

/// 会话的 `needs` 配上挂着的待答请求 `ask`（最早挂上的那一条；登记表住常驻进程，`control::asks`，由顶层壳查好交进来）：
/// 有 ⇒ `needs.ask` 是它（pidfile 没说在等 —— 挡着终端的那种 —— 就照它补出一份）；pidfile 说在等却没有 ⇒ `ask: null`、`manualText` 写好「只能在终端里答」。
pub(crate) fn dress_ask_with(
    needs: Option<Needs>,
    ask: Option<crate::agents::asks::Ask>,
    now_ms: u64,
) -> Option<Needs> {
    use crate::agents::AskKind;
    match (needs, ask) {
        (Some(mut n), Some(a)) => {
            n.manual_text = None;
            n.ask = Some(a);
            Some(n)
        }
        (Some(mut n), None) => {
            n.ask = None;
            n.manual_text = Some(Words(copy_core::copy_text("beAsk.manual.terminal", &[])));
            Some(n)
        }
        (None, Some(a)) => {
            let kind = match a.kind {
                AskKind::Approve => NeedsKind::Approve,
                AskKind::Question => NeedsKind::Answer,
                AskKind::Plan => NeedsKind::Plan,
            };
            let waited = now_ms.saturating_sub(a.since_ms);
            Some(Needs {
                kind,
                tool: Some(a.tool.clone()),
                call: None,
                what: a.what.clone(),
                since_ms: Some(a.since_ms),
                text: needs_words(kind),
                head: needs_head(kind, Some(&a.tool)),
                head_code: match kind {
                    NeedsKind::Approve | NeedsKind::Answer => a.what.clone(),
                    _ => None,
                },
                tone: crate::common::cells::Tone::Need,
                rank: NEEDS_BY_DANGER
                    .iter()
                    .position(|k| *k == kind)
                    .map_or(u8::MAX, |i| i as u8),
                waited_ms: Some(waited),
                waited_text: Some(Words(copy_core::short_duration(waited))),
                ask: Some(a),
                manual_text: None,
            })
        }
        (None, None) => None,
    }
}

/// 一种「需手动」写好的字（唯一一处：会话事实的 `needs.text` · 轮换在用名单都由它写）。
pub(crate) fn needs_words(kind: NeedsKind) -> Words {
    Words(match kind {
        NeedsKind::Approve => copy_core::copy_text("beSession.needs.approve", &[]),
        NeedsKind::Answer => copy_core::copy_text("beSession.needs.answer", &[]),
        NeedsKind::Plan => copy_core::copy_text("beSession.needs.plan", &[]),
        NeedsKind::Network => copy_core::copy_text("beSession.needs.network", &[]),
        NeedsKind::Worker => copy_core::copy_text("beSession.needs.worker", &[]),
        NeedsKind::Goal => copy_core::copy_text("beSession.needs.goal", &[]),
        NeedsKind::Choose => copy_core::copy_text("beSession.needs.choose", &[]),
        NeedsKind::Unknown => copy_core::copy_text("beSession.needs.unknown", &[]),
    })
}

/// 钉条第一行（[`Needs::head`] 的唯一一处）：种类 ＋ 工具名。批准判不出工具名 ⇒ 短的那一格（[`needs_words`]）。
pub(crate) fn needs_head(kind: NeedsKind, tool: Option<&str>) -> Words {
    Words(match kind {
        NeedsKind::Approve => match tool {
            Some(t) => copy_core::copy_text("needs.bar.approve", &[("tool", t)]),
            None => return needs_words(kind),
        },
        NeedsKind::Answer => copy_core::copy_text("needs.bar.answer", &[]),
        NeedsKind::Plan => copy_core::copy_text("needs.bar.plan", &[]),
        NeedsKind::Network => copy_core::copy_text("needs.bar.network", &[]),
        NeedsKind::Worker => copy_core::copy_text("needs.bar.worker", &[]),
        NeedsKind::Goal => copy_core::copy_text("needs.bar.goal", &[]),
        NeedsKind::Choose => copy_core::copy_text("needs.bar.choose", &[]),
        NeedsKind::Unknown => copy_core::copy_text("needs.bar.unknown", &[]),
    })
}

/// 「需手动」先答哪个（[`Needs::rank`] 的来历，唯一一处）：种类已按顶上那个框判（顶上的先答），这里再按危险度 ——
/// 放行联网（沙箱里的命令要出网）· 批准一步（工具要动手）在前；协作请求 · 会话目标 · 计划次之；回答 · 选一项 · 判不出最后。
pub(crate) const NEEDS_BY_DANGER: [NeedsKind; 8] = [
    NeedsKind::Network,
    NeedsKind::Approve,
    NeedsKind::Worker,
    NeedsKind::Goal,
    NeedsKind::Plan,
    NeedsKind::Answer,
    NeedsKind::Choose,
    NeedsKind::Unknown,
];

/// **几条「需手动」先答哪个**（唯一一处：`sessions-needs` 按它排）：危险度序在前（[`Needs::rank`]）；同一档里等得久的在前
/// （[`Needs::waited_ms`]，各自在那台算的）；不知道等了多久的排这一档最后。
pub(crate) fn needs_first(a: &Needs, b: &Needs) -> std::cmp::Ordering {
    a.rank
        .cmp(&b.rank)
        .then_with(|| match (a.waited_ms, b.waited_ms) {
            (Some(x), Some(y)) => y.cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
}

/// 最新 usage ＋ 这份会话的上下文上限 ＋ 写好的字（状态栏 · 监控板 · 手机读同一份；出口不算百分比、不排数）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct UsageFact {
    /// `input_tokens + cache_creation_input_tokens + cache_read_input_tokens`。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) prompt_tokens: u64,
    pub(crate) model: Option<String>,
    /// 全会话最大的一轮（见过超过标准上限的 ⇒ 上限不可能是标准那一档）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) peak_prompt_tokens: u64,
    /// 上下文上限（tokens），恒 ≥ `peak_prompt_tokens` ⇒ 百分比不会超过 100。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) limit: u64,
    pub(crate) limit_from: LimitFrom,
    /// 最新一轮占上限的百分比（四舍五入，0–100）；上限判不出（`assumed`）⇒ `null`。
    pub(crate) percent: Option<u8>,
    /// 上下文那一格的字：上限判得出写百分比（`35%`），判不出只写用了多少（`350k`）。
    pub(crate) context_text: Words,
    /// 那一格的语气：到了 [`CONTEXT_WARN_AT`] ⇒ `warn`，否则 `plain`（判不出上限 ⇒ 恒 `plain`）。
    pub(crate) context_tone: crate::common::cells::Tone,
    /// 最新一轮用了多少（`350k`）。
    pub(crate) prompt_tokens_text: Words,
    /// 上限（`1M`）。
    pub(crate) limit_text: Words,
    /// 上限从哪来的字（中转请求 · 设置 · 模型名 · 用量超过 200k）；判不出 ⇒ `null`。
    pub(crate) limit_from_text: Option<Words>,
}

/// 到这个百分比，上下文那一格的语气是 `warn`。
pub(crate) const CONTEXT_WARN_AT: u8 = 80;

impl UsageFact {
    /// 一份用量：上限与来源已定 ⇒ 字与语气随之写好（唯一一处）。
    pub(crate) fn new(
        prompt_tokens: u64,
        model: Option<String>,
        peak_prompt_tokens: u64,
        (limit, limit_from): (u64, LimitFrom),
    ) -> UsageFact {
        let mut u = UsageFact {
            prompt_tokens,
            model,
            peak_prompt_tokens,
            limit,
            limit_from,
            percent: None,
            context_text: Words::default(),
            context_tone: crate::common::cells::Tone::Plain,
            prompt_tokens_text: Words::default(),
            limit_text: Words::default(),
            limit_from_text: None,
        };
        u.settle((limit, limit_from));
        u
    }

    /// 上限换了（按调用方的上限表 / 中转标记重判）⇒ 连同字一起换。
    pub(crate) fn settle(&mut self, (limit, limit_from): (u64, LimitFrom)) {
        use crate::common::cells::Tone;
        self.limit = limit;
        self.limit_from = limit_from;
        self.percent = (limit_from != LimitFrom::Assumed && limit > 0).then(|| {
            let pct = (self.prompt_tokens as f64 / limit as f64 * 100.0).round();
            pct.min(100.0) as u8
        });
        self.prompt_tokens_text = Words(short_tokens(self.prompt_tokens));
        self.limit_text = Words(short_tokens(limit));
        self.context_text = match self.percent {
            Some(n) => Words(copy_core::copy_text(
                "beUsage.context.pct",
                &[("n", &n.to_string())],
            )),
            None => self.prompt_tokens_text.clone(),
        };
        self.context_tone = match self.percent {
            Some(n) if n >= CONTEXT_WARN_AT => Tone::Warn,
            _ => Tone::Plain,
        };
        self.limit_from_text = match limit_from {
            LimitFrom::Relay => Some(copy_core::copy_text("beUsage.from.relay", &[])),
            LimitFrom::Setting => Some(copy_core::copy_text("beUsage.from.setting", &[])),
            LimitFrom::Model => Some(copy_core::copy_text("beUsage.from.model", &[])),
            LimitFrom::Observed => Some(copy_core::copy_text("beUsage.from.observed", &[])),
            LimitFrom::Assumed => None,
        }
        .map(Words);
    }
}

/// 上限从哪来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) enum LimitFrom {
    /// 中转看见了这个会话的请求：带了扩展上下文那一项 ⇒ 1M，没带过 ⇒ 这个模型的默认（`observe::relay_marks`）。
    Relay,
    /// 设置里的上限表（按模型名子串）。
    Setting,
    /// 模型名自己带着 `[1m]`。
    Model,
    /// 这份会话见过超过标准上限的一轮。
    Observed,
    /// 判不出（`limit` 只是占位的 1M）：界面不算百分比，只写用了多少。
    Assumed,
}

/// 标准上下文（200k，模型的默认）与扩展那一档（1M）。记录里的模型名是接口回的名字、不带 `[1m]`，
/// 真来源是中转看见的请求（扩展上下文的请求带那一项）。
pub(crate) const CONTEXT_STANDARD: u64 = 200_000;
pub(crate) const CONTEXT_EXTENDED: u64 = 1_000_000;

/// 设置里的上限表：模型名子串（小写）→ 上限。住这台的偏好（`control/machine_prefs.rs` 的 `contextLimits`），`history-facts` 现读。
pub(crate) type ContextLimits = Vec<(String, u64)>;

/// **上下文上限的唯一判定**：中转看见过这个会话的请求（`relay`）⇒ 带过扩展上下文那一项是 1M、没带过是默认 200k；
/// 中转没看见过 ⇒ 设置里的上限表（最长匹配的子串胜）> 模型名带 `[1m]` > 见过超过 200k 的一轮 > 判不出（占位 1M，`Assumed`）。
/// 任何一档给出的数小于见过的最大一轮 ⇒ 那一档不对，按「见过」那一档（上限至少是 1M，再大就是见过的那么大）。
pub(crate) fn context_limit(
    model: Option<&str>,
    peak: u64,
    limits: &ContextLimits,
    relay: Option<bool>,
) -> (u64, LimitFrom) {
    let m = model.unwrap_or("").to_lowercase();
    let from_relay = relay.map(|wide| {
        let n = if wide {
            CONTEXT_EXTENDED
        } else {
            CONTEXT_STANDARD
        };
        (n, LimitFrom::Relay)
    });
    let setting = limits
        .iter()
        .filter(|(sub, _)| m.contains(sub.as_str()))
        .max_by_key(|(sub, _)| sub.len())
        .map(|(_, n)| (*n, LimitFrom::Setting));
    let chosen = from_relay.or(setting).unwrap_or(if m.contains("[1m]") {
        (CONTEXT_EXTENDED, LimitFrom::Model)
    } else if peak > CONTEXT_STANDARD {
        (CONTEXT_EXTENDED, LimitFrom::Observed)
    } else {
        (CONTEXT_EXTENDED, LimitFrom::Assumed)
    });
    if chosen.0 < peak {
        (peak.max(CONTEXT_EXTENDED), LimitFrom::Observed)
    } else {
        chosen
    }
}

/// 一个完整行（不含结尾 `\n`）累加到 `facts` 上：先过快路（[`could_matter`]），过了才解析。没解析过这一行的读者（事实尾巴接着读）走它。
/// `kind` ＝ 这份记录是哪一家的。
pub(crate) fn note_line(facts: &mut SessionFacts, kind: &str, line: &[u8]) {
    if could_matter(kind, line, facts) {
        if let Some(v) = super::super::lines::parse(line) {
            if let Some(beat) = crate::agents::beat_of(kind, &v) {
                note_record(facts, kind, &v, &beat);
            }
        }
    }
}

/// 只要 `mcp` 那一格：从 `r` 此刻的位置读到尾，只解析快路放过的完整行（那一家的快路只问 MCP 那一格）、
/// 经 [`note_mcp`] 接着 `mcp` 往下改（口径与整份事实那一格同一处）。
/// 回读过的字节数（只算到最后一个完整行：半行留给下一次）。扩展页问「这台活会话里谁断开」用（`observe::accounts_query::McpBook`）。
pub(crate) fn mcp_from<R: std::io::BufRead>(
    r: R,
    kind: &str,
    mcp: &mut Vec<McpTrouble>,
) -> std::io::Result<u64> {
    let mut f = SessionFacts {
        mcp: std::mem::take(mcp),
        ..SessionFacts::default()
    };
    let only = Watch {
        mcp_only: true,
        ..Watch::default()
    };
    let mut lines = super::super::lines::Lines::new(r, 0);
    let out = loop {
        let row = match lines.next() {
            Ok(Some(row)) => row,
            Ok(None) => break Ok(lines.end()),
            Err(e) => break Err(e),
        };
        let line = row.body();
        if crate::agents::beat_hint(kind, line, &only) {
            if let Some(v) = super::super::lines::parse(line) {
                let at =
                    crate::agents::beat_of(kind, &v).and_then(|b| b.at.map(|a| a.into_owned()));
                note_mcp(&mut f, kind, &v, at.as_ref());
            }
        }
    };
    *mcp = f.mcp;
    out
}

/// 扫完之后按上限表与中转标记定上下文上限（每次按调用方给的表重判，不进扫描）。
pub(crate) fn settle_limit(facts: &mut SessionFacts, limits: &ContextLimits, relay: Option<bool>) {
    if let Some(u) = facts.usage.as_mut() {
        let limit = context_limit(u.model.as_deref(), u.peak_prompt_tokens, limits, relay);
        u.settle(limit);
    }
}

/// 这一行有没有可能改动事实（快路，见头注）。**只许放过、不许误拦**：凡是 [`note_record`] 会动的记录，这里必为真。
/// 格式那一半问那一家（`BeatFace.hint`）；「行里带着某个还挂着的调用 / 后台命令的 id」这一半是通用的，在这里看。
pub(crate) fn could_matter(kind: &str, line: &[u8], facts: &SessionFacts) -> bool {
    let watch = Watch {
        fork_locked: facts.forked_from.is_some(),
        retry_open: open_retry(facts),
        pending: !facts.pending.is_empty(),
        background: !facts.bg_tasks.is_empty(),
        mcp_only: false,
    };
    crate::agents::beat_hint(kind, line, &watch)
        || facts
            .bg_tasks
            .iter()
            .any(|t| contains(line, t.call.as_bytes()))
        || facts
            .pending
            .iter()
            .any(|p| contains(line, p.id.as_bytes()))
}

/// 这条是 agent 交回 ⇒ 那个子运行进 `handed_back`（谁说的由适配层判，口径同全局搜索的「agent 回报」）。
fn note_handback(f: &mut SessionFacts, beat: &Beat) {
    let Some(crate::agents::Speaker::AgentMessage {
        from: Some(from),
        handback: true,
        ..
    }) = beat.said().map(|s| &s.speaker)
    else {
        return;
    };
    if f.handed_back.contains(from) {
        return;
    }
    f.handed_back.push(from.clone());
    if f.handed_back.len() > HANDED_BACK_KEEP {
        f.handed_back.remove(0);
    }
}

/// 那一家这一条说了 MCP 的哪几张表 ⇒ 那几种整种换掉（没说的那几种沿用）。
fn note_mcp(f: &mut SessionFacts, kind: &str, v: &Value, at: Option<&String>) {
    use crate::agents::McpStatus as S;
    let Some(said) = crate::agents::mcp_said_of(kind, v) else {
        return;
    };
    let at = at.cloned();
    let named = |names: Vec<String>, status| {
        let at = at.clone();
        names.into_iter().map(move |name| McpTrouble {
            name,
            status,
            detail: None,
            at: at.clone(),
        })
    };
    let mut put = |status: S, now: Vec<McpTrouble>| {
        f.mcp.retain(|m| m.status != status);
        f.mcp.extend(now);
    };
    if let Some(n) = said.pending {
        put(S::Pending, named(n, S::Pending).collect());
    }
    if let Some(n) = said.needs_login {
        put(S::NeedsLogin, named(n, S::NeedsLogin).collect());
    }
    if let Some(n) = said.failed {
        let now = n.into_iter().map(|(name, detail)| McpTrouble {
            name,
            status: S::Failed,
            detail,
            at: at.clone(),
        });
        put(S::Failed, now.collect());
    }
    f.mcp.sort_by(|a, b| a.name.cmp(&b.name));
}

/// 这一家说这一行认不出、又不成卡 ⇒ 记一行（种类首见序），那一句照新的数重写。
fn note_quiet(f: &mut SessionFacts, kind: &str, v: &Value) {
    let Some(t) = crate::agents::quiet_unread_of(kind, v) else {
        return;
    };
    let q = f.quiet_unread.get_or_insert_with(|| QuietUnread {
        text: Words(String::new()),
        count: 0,
        kinds: Vec::new(),
    });
    q.count += 1;
    if !q.kinds.contains(&t) && q.kinds.len() < QUIET_KINDS_KEEP {
        q.kinds.push(t);
    }
    let n = q.count.to_string();
    let types = q
        .kinds
        .join(&copy_core::copy_text("record.unread.quietSep", &[]));
    q.text = Words(copy_core::copy_text(
        "record.unread.quiet",
        &[("n", &n), ("types", &types)],
    ));
}

fn open_retry(f: &SessionFacts) -> bool {
    f.retries
        .last()
        .is_some_and(|r| r.outcome == RetryOutcome::Retrying)
}

/// 重试一串一串地记（口径见头注那张表）。
fn note_retry(f: &mut SessionFacts, beat: &Beat) {
    let open = open_retry(f);
    if beat.acts.contains(&Act::Retry) {
        if open {
            return;
        }
        let Some(id) = beat.id.as_deref() else {
            return;
        };
        f.retries.push(RetryRun {
            id: id.to_string(),
            outcome: RetryOutcome::Retrying,
        });
        if f.retries.len() > RETRY_KEEP {
            f.retries.remove(0);
        }
        return;
    }
    if !open {
        return;
    }
    match beat.side {
        Side::Reply => settle_retry(
            f,
            if beat.stops(StopWhy::Failed) {
                RetryOutcome::Failed
            } else {
                RetryOutcome::Recovered
            },
        ),
        Side::Input if !beat.injected && !beat.has_result() => {
            settle_retry(f, RetryOutcome::Interrupted)
        }
        _ => {}
    }
}

fn settle_retry(f: &mut SessionFacts, outcome: RetryOutcome) {
    if let Some(r) = f.retries.last_mut() {
        r.outcome = outcome;
    }
}

/// 一条**已解析**的记录（`kind` 那一家的，`beat` 是它的一拍）累加到 `f` 上。各格口径的唯一住址（见头注那张表）。
pub(crate) fn note_record(f: &mut SessionFacts, kind: &str, v: &Value, beat: &Beat) {
    if f.forked_from.is_none() {
        if let Some(sid) = &beat.forked_from {
            f.forked_from = Some(sid.clone());
        }
    }
    let at = beat.at.as_deref().map(str::to_string);
    note_retry(f, beat);
    note_background(f, kind, v, at.as_ref());
    note_mcp(f, kind, v, at.as_ref());
    note_quiet(f, kind, v);
    match beat.side {
        Side::Input => note_user(f, beat),
        Side::Reply => {
            for a in &beat.acts {
                match a {
                    Act::Text(t) => {
                        if let Some(text) = one_line(t) {
                            f.last_say = Some(LastSay {
                                text,
                                at: at.clone(),
                                at_ms: at
                                    .as_deref()
                                    .and_then(crate::common::time::parse_iso8601_ms),
                            });
                        }
                    }
                    Act::Call {
                        id,
                        name,
                        what,
                        edits,
                    } if !name.is_empty() => {
                        if !id.is_empty() {
                            f.pending.retain(|p| p.id != *id);
                            f.pending.push(PendingCall {
                                id: id.to_string(),
                                name: name.to_string(),
                                what: what.clone(),
                                at: at.clone(),
                                at_ms: at
                                    .as_deref()
                                    .and_then(crate::common::time::parse_iso8601_ms),
                                state: StepWait::default(),
                                why: None,
                                text: None,
                                why_text: None,
                            });
                            if f.pending.len() > PENDING_KEEP {
                                f.pending.remove(0);
                            }
                        }
                        if let Some(p) = edits {
                            touch(f, p);
                        }
                    }
                    _ => {}
                }
            }
            note_usage(f, beat);
            note_tokens(f, beat);
        }
        Side::Note => {}
    }
    if let Some(c) = beat.cost {
        note_cost(f, c);
    }
    if let Some(m) = &beat.mode {
        f.permission_mode = Some(m.to_string());
    }
}

/// 用户那侧：是 agent 交回 ⇒ 记下那个子运行；带着工具结果 ⇒ 摘掉结果对上的那几个调用；
/// 没有工具结果（人发的一句 · 中断）⇒ 没结果的全摘（那一轮过去了）。
fn note_user(f: &mut SessionFacts, beat: &Beat) {
    note_handback(f, beat);
    if f.pending.is_empty() {
        return;
    }
    if beat.has_result() {
        let results: Vec<&str> = beat
            .acts
            .iter()
            .filter_map(|a| match a {
                Act::Result { of: Some(of), .. } => Some(of.as_ref()),
                _ => None,
            })
            .collect();
        f.pending.retain(|p| !results.contains(&p.id.as_str()));
    } else {
        f.pending.clear();
    }
}

/// 近因序去重：再碰一次 ⇒ 移到末尾；超上界 ⇒ 丢最久没碰的。
fn touch(f: &mut SessionFacts, p: &str) {
    if let Some(i) = f.touched_files.iter().position(|x| x == p) {
        f.touched_files.remove(i);
    }
    f.touched_files.push(p.to_string());
    if f.touched_files.len() > TOUCHED_FILES_KEEP {
        f.touched_files.remove(0);
    }
}

fn note_usage(f: &mut SessionFacts, beat: &Beat) {
    let Some(u) = &beat.usage else {
        return;
    };
    let prompt = u
        .input
        .saturating_add(u.cache_write)
        .saturating_add(u.cache_read);
    if prompt == 0 {
        return;
    }
    let peak = f
        .usage
        .as_ref()
        .map_or(0, |u| u.peak_prompt_tokens)
        .max(prompt);
    let model = beat.model.as_deref().map(str::to_string);
    let limit = context_limit(model.as_deref(), peak, &Vec::new(), None);
    f.usage = Some(UsageFact::new(prompt, model, peak, limit));
}

/// 一条回复的用量记进全会话用量：同一次请求（请求键）紧跟着的后一条替掉前一条的数。
fn note_tokens(f: &mut SessionFacts, beat: &Beat) {
    let Some(u) = &beat.usage else {
        return;
    };
    let (w5, w1) = match (u.write_5m, u.write_1h) {
        (None, None) => (u.cache_write, 0),
        (a, b) => (a.unwrap_or(0), b.unwrap_or(0)),
    };
    let tokens = [u.input, u.output, u.cache_read, w5, w1];
    if tokens.iter().all(|&n| n == 0) {
        return;
    }
    let id = u.request.as_deref().unwrap_or("").to_string();
    let s = f.tokens.get_or_insert_with(TokenUse::default);
    let mut add = |sign: bool, t: &[u64; 5]| {
        let fields = [
            &mut s.input,
            &mut s.output,
            &mut s.cache_read,
            &mut s.cache_write5m,
            &mut s.cache_write1h,
        ];
        for (x, &n) in fields.into_iter().zip(t) {
            *x = if sign {
                x.saturating_add(n)
            } else {
                x.saturating_sub(n)
            };
        }
    };
    match s.last.take() {
        Some(prev) if !id.is_empty() && prev.id == id => add(false, &prev.tokens),
        _ => s.requests += 1,
    }
    add(true, &tokens);
    s.last = Some(LastRequest { id, tokens });
    s.text = Words(copy_core::copy_text(
        "beSpend.tokens.line",
        &[
            ("input", &short_tokens(s.input)),
            ("output", &short_tokens(s.output)),
            ("read", &short_tokens(s.cache_read)),
            ("write", &short_tokens(s.cache_write5m + s.cache_write1h)),
        ],
    ));
}

/// 花费那一条（那一家说的到此刻为止的全会话总数；`partial` ⇒ 有型号定不了价、数只是下限）。
fn note_cost(f: &mut SessionFacts, c: crate::agents::beat::CostSaid) {
    let crate::agents::beat::CostSaid { micros, partial } = c;
    let shown = format!("{:.2}", micros as f64 / 1e6);
    let text = match (micros, partial) {
        (1..=4_999, _) => copy_core::copy_text("beSpend.cost.tiny", &[]),
        (_, false) => copy_core::copy_text("beSpend.cost.exact", &[("usd", &shown)]),
        (_, true) => copy_core::copy_text("beSpend.cost.about", &[("usd", &shown)]),
    };
    f.cost = Some(Cost {
        micros,
        partial,
        text: Words(text),
    });
}

/// token 数写成短串（唯一一处：用量那一行 · 上下文那一格）：`800` · `1.2k` · `8k` · `350k` · `1M` · `1.3M`。
/// 一万以下的 k 与 M 留一位小数（`.0` 不写），一万到一百万的 k 取整。
fn short_tokens(n: u64) -> String {
    let one = |x: f64, unit: &str| {
        let t = format!("{:.1}", (x * 10.0).round() / 10.0);
        format!("{}{unit}", t.strip_suffix(".0").unwrap_or(&t))
    };
    match n {
        0..=999 => n.to_string(),
        1_000..=9_999 => one(n as f64 / 1e3, "k"),
        10_000..=999_999 => format!("{}k", (n as f64 / 1e3).round() as u64),
        _ => one(n as f64 / 1e6, "M"),
    }
}

/// 骨架落盘那一形（`observe::skeleton`）：线上那一形跳过的 [`TokenUse::last`] 另落一格，读回来放回去。
pub(crate) mod skel {
    use super::{LastRequest, SessionFacts};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize)]
    struct Ref<'a> {
        facts: &'a SessionFacts,
        last: Option<&'a LastRequest>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Owned {
        facts: SessionFacts,
        last: Option<LastRequest>,
    }

    pub(crate) fn serialize<S: Serializer>(f: &&SessionFacts, s: S) -> Result<S::Ok, S::Error> {
        Ref {
            facts: f,
            last: f.tokens.as_ref().and_then(|t| t.last.as_ref()),
        }
        .serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SessionFacts, D::Error> {
        let Owned { mut facts, last } = Owned::deserialize(d)?;
        if let Some(t) = facts.tokens.as_mut() {
            t.last = last;
        }
        Ok(facts)
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/observe/recognize/facts_tests.rs"]
mod tests;
