//! 后端→monitor 的帧：类型与解析。

use super::*;
use crate::copy_table::copy_text;
use crate::session_book::Fate;

/// 一行会话记录的成品 ＋ 它在那份文件里的行号（`seq`）—— 进 [`flush_lines`] 之前的形状。所有行都从后端的帧来（远端流 · 本机流 · 旁路快照）；
/// `seq` 是后端给的行号（`--tail-only` 下与快照同处一个行号空间），前端按 `(session_id, seq)` 去重、按 `seq` 排序（`INVARIANTS §5` / `§9`）。
#[derive(Debug, Clone)]
pub struct JsonlLine {
    pub session_id: String,
    pub path: std::path::PathBuf,
    pub seq: u64,
    /// 那台后端给的成品：这一行在渲染模型里的样子；`None` ＝ 不进界面（照占号）。
    pub message: Option<crate::ui_contract::RecordBody>,
    /// 这条记录自己的 `cwd`（后端给的）。
    pub cwd: Option<String>,
    /// 这一行之后（含它的 `\n`）那一个字节的偏移 = 下一行的起点（后端 `line.byte_offset` ·
    /// 快照的行区间末端）；说不准 ⇒ `None`。续点据它记「从哪个字节接着读」。
    pub end: Option<u64>,
    /// 这一行的对账键（后端 `line.rid`，原样转交；快照那一路没有 ⇒ `None`）。
    pub rid: Option<String>,
}

/// 一条**丢了就不可恢复**的帧的身份。
///
/// 与后端侧 `wire::LostFrame` 对应。**故意不共用类型**：那是 backend crate 的私有 wire
/// 形状，monitor 这边是从 JSON 现解的，共用会把两个 crate 绑死在一个结构体上，
/// 而 additive 演进恰恰要求两边能各自容忍对方多/少字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LostFrameInfo {
    pub kind: String,
    pub subject: Option<String>,
}

/// `hello.homes` 的一项 —— 某个 agent 在那台远端机器上的 home 目录。与后端侧 `wire::AgentHome` 对称（这一侧自己解析 JSON）；
/// agent 维度住在 `agent_kind` 这个值里，字段名里没有任何一个 agent 的名字。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentHome {
    pub agent_kind: String,
    pub path: String,
}

/// 从 hello 帧里解析出「Claude 的 home 目录」—— 优先 `homes`、回退 `claude_dir`。
/// `homes`（通用）：`[{agent_kind, path}]`；`claude_dir`（冻结兼容，字段名里带 agent 名，后端登记在 `agent_boundary_guard::FROZEN_COMPAT`，
/// 解锁条件是 monitor 与 aterm 都改读 `homes`）。monitor 不再依赖 `claude_dir` 的存在语义，它只是回退路径。后端的 `homes` 现在恒空 ⇒ 实际走回退分支。
pub(crate) fn claude_home_from_hello<'a>(homes: &'a [AgentHome], claude_dir: &'a str) -> &'a str {
    homes
        .iter()
        .find(|h| h.agent_kind == "claude")
        .map(|h| h.path.as_str())
        .unwrap_or(claude_dir)
}

/// backend→client 的一帧（解析后的 inbound 表示）。
///
/// 对应后端 `wire::Frame`（外部 `kind` tag，snake_case）；这一侧不链那个 crate，按 JSON 现解（[`parse_frame`]）。
/// 两侧形状由跨语言金样 `tests/__fixtures__/session-stream.golden.jsonl` 对拍。
#[derive(Debug, Clone, PartialEq)]
pub enum InboundFrame {
    /// 握手帧：连接建立后后端发一次。`v` = 协议大版本，`build_id` = backend 构建标识
    /// （#33 版本协商捕获 + 比对），host_arch / claude_dir 用于 log 证明后端真的在远端
    /// 跑起来了。多余字段仍忽略（向前兼容）。
    Hello {
        v: u64,
        build_id: String,
        host_arch: String,
        /// ⚠ **原样的线上值**，不做回退解析 —— 要「Claude 的 home」请走
        /// [`claude_home_from_hello`]（优先 `homes`）。两者分开是刻意的：
        /// 这个字段是**冻结兼容面**（仓外 aterm 还在读它），
        /// 把回退结果写回这里会让"backend 到底发了什么"变得不可观测。
        claude_dir: String,
        /// 远端各 agent 的 home 目录表（additive）。旧后端无此字段 ⇒ 空表 ⇒ 回退 `claude_dir`。非数组 / 元素缺字段一律滤掉，绝不 panic。
        homes: Vec<AgentHome>,
        /// backend 声明的能力 token 集。旧后端无此字段 → 空集（按最小能力集待它，不发流模式 flag）；monitor 按此决定发 `--with-bg` / `--tail-only`。
        capabilities: Vec<String>,
        /// backend 声明接受哪些入方向命令（后端 `inbound::command_names`，从命令表派生）。`capabilities` 说出方向的流 flag，这一条说入方向 —— 两者正交。
        /// 旧后端无此字段 ⇒ 空集 ⇒ monitor 一条入方向命令都不发。
        commands: Vec<String>,
        /// `hello.unavailable`：这台接得下却做不到的 `(命令, 码)`。旧后端无此字段 ⇒ 空（没把握）。
        unavailable: Vec<(String, String)>,
        /// `hello.uncancellable`：撤不动的那几条。旧后端无此字段 ⇒ 空（没把握，照旧补发撤单）。
        uncancellable: Vec<String>,
    },
    /// 一行从后端 session jsonl 尾随读到的原始行（远端流与本机流同一种帧）。字段语义见 [`JsonlLine`]。
    Line {
        session_id: String,
        path: String,
        seq: u64,
        /// 成品（`message`，缺 ＝ 不进界面）与这条记录自己的 `cwd`。
        message: Option<crate::ui_contract::RecordBody>,
        cwd: Option<String>,
        /// 后端的 `byte_offset`（这一行末尾含 `\n` 的累计字节）。
        end: u64,
        /// 对账键（后端 `rid`，原样转交）。
        rid: Option<String>,
    },
    /// 远端新出现一个 session 文件。附带 pidfile 元信息。
    SessionAdded {
        sid: String,
        /// 后台会话（后端判好的；缺 ＝ 交互）。
        background: bool,
        /// attach 进去对人有没有意义（additive）。缺席 = true。语义与来源见 `src/backend/stream/wire.rs` 的同名字段 + `src/doc/IPC-PROTOCOL.md` §9.3。
        attachable: Option<bool>,
        /// pidfile 记的起会话目录（认「我刚起的那条」用）。
        cwd: Option<String>,
        /// 会话的项目目录（那台后端读记录开头给的；tab 标题用它）。老后端不带 ⇒ `None`。
        project_dir: Option<String>,
        name: Option<String>,
        /// 远端 jsonl 绝对路径 —— 旁路快照用。
        path: Option<String>,
        /// backend prime 时的完整行数 L（快照完整性校验）。
        lines: Option<u64>,
        /// 宣告时此刻在干什么 ＋ 在等什么（连接建立灯就对）。
        activity: Option<crate::session_book::SessionActivity>,
        waiting_for: Option<String>,
        /// 〔additive〕这条活会话住在什么容器里（`{host, terminal?}`）。缺席 ⇒ `None` = 不知道（**不是**「不在任何宿主里」）；
        /// 不认识的宿主 ⇒ `Other`（不吞）。
        /// 进 `session_book` 的活会话成品（本机那条流同一个口）。
        container: Option<crate::session_book::SessionContainer>,
        /// 〔additive〕那个 claude 进程的 pid。本机活会话的成品（`session_book::LiveMeta::pid`）
        /// 拿它给本机 ↗ 绑窗口（`bind::SidHwndCache::record`）；老后端不带 ⇒ `None`。远端那一支不读它。
        pid: Option<u32>,
    },
    /// 后端的活会话清单报完了（Phase 1 走完）。无载荷。
    SessionsReplayed,
    /// 活会话的记录文件不见了（`session_file_gone`）/ 被改过已从头重读（`session_file_reread`）。
    /// 两个 kind 收成一形：下游只关心「哪个会话、怎么了」。
    SessionFileNotice {
        sid: String,
        path: String,
        change: FileChange,
    },
    /// 会话 status 变化（远端红绿灯）。
    SessionStatus {
        sid: String,
        activity: Option<crate::session_book::SessionActivity>,
        waiting_for: Option<String>,
    },
    /// 远端一个 session 文件消失。monitor 只拿它当内容流的边界（残批先冲、快照作废）；
    /// 它离开之后是可重连还是已结束，紧跟着的 [`InboundFrame::SessionState`] 说（后端裁）。
    SessionRemoved { sid: String },
    /// 后端会话账本的成品：这条会话离开「活」之后是什么（`session_state`）。
    SessionState { sid: String, state: Fate },
    /// 一个会话的运行表（`session_runs`；`runs` · `ended` 是 JSON 数组原文，不解释）。
    SessionRuns {
        sid: String,
        runs: crate::ui_contract::RecordBody,
        ended: crate::ui_contract::RecordBody,
    },
    /// 远端后端发送通道拥塞、丢了 `dropped` 帧（慢 SSH 管道）；monitor 收到后经 remote-health 通道提示用户。
    /// `lost` / `lost_truncated`（additive）：那批丢帧里不可恢复的那些的身份 —— 决定了要对用户说哪句话：丢内容帧「重开会话可看完整历史」是真的，
    /// 丢状态增量帧不是（它是一次差分的结果、别处不存在）。旧后端不发这两个字段 ⇒ 空集 / false。
    Overflow {
        dropped: u64,
        lost: Vec<LostFrameInfo>,
        lost_truncated: bool,
    },
    // 这里原是后端 tmux 观测两帧（整份快照 · 差分出的正向死亡）：收割与「可重连」进了那台后端的会话账本、
    //   后端也不再发它们 ⇒ 删。老后端发来 ⇒ 落未知 kind（照常 warn 后跳过）。
    /// 入方向命令的应答。`id` 是 monitor 自己生成的不透明串，backend 原样回显；由 `inbound_client` 按 `id` 路由回请求方。
    Reply {
        id: String,
        ok: bool,
        code: Option<String>,
        message: Option<String>,
        /// 「复制详情」那几行（后端写好；成功时缺）。
        detail: Option<String>,
        data: Option<serde_json::Value>,
    },
    /// 某条在跑的入方向命令已被取消。
    Cancelled { id: String },
    /// 那台机器上的账号清单变了（后端 `wire::Frame::AccountsChanged`，无载荷）。
    AccountsChanged,
    /// 那台机器上的配置文件变了（后端 `wire::Frame::ProfilesChanged`，无载荷）。
    ProfilesChanged,
    /// 那台机器上某个会话的任务清单变了（后端 `wire::Frame::TasksChanged`，只带 sid）。
    TasksChanged { sid: String },
    /// 一条链路的下行字节（后端 `wire::Frame::LinkData`；`data` 在解帧这一步就解开了 base64）。
    /// 只有**本机后端**那条流上会有（monitor 只在那条流上开链路），交 `link_mux`。
    LinkData { link: String, data: Vec<u8> },
    /// 一条链路收尾了（后端 `wire::Frame::LinkEnd`）。
    LinkEnd { link: String, error: Option<String> },
    /// 一趟传输此刻的样子（后端 `wire::Frame::Transfer`）。只有**本机后端**那条流上会有
    /// （传输台住本机后端），交 `sftp_pool::deliver`。`end` 解不动 ⇒ 整帧 `None`（坏帧）。
    Transfer {
        id: String,
        got: u64,
        total: u64,
        end: Option<crate::sftp_pool::End>,
    },
    /// 中转抄出来的一个 SSE 事件 / 一个响应的收尾（后端 `wire::Frame::Tap`）。只有**本机后端**那条流上会有
    /// （中转住本机常驻后端），交 `session_tap::deliver`。`data` / `end` 都缺、或 `end` 认不出 ⇒ 整帧 `None`（坏帧）。
    Tap(crate::session_tap::Tap),
    /// 测试连接那一趟的一格进度 / 结局（后端 `wire::Frame::Probe`）。只有**本机后端**那条流上会有
    /// （测试连接在本机常驻后端里跑），交 `probe_relay::deliver`。`cell` 原样（一个 JSON 对象的文本，monitor 不解释）。
    Probe { ticket: String, cell: String },
    /// 终端实时预览的一整屏（后端 `wire::Frame::TerminalScreen`）。本机远端两条流上都会有（订了哪台就是哪台推的），
    /// 交 `terminal_screen_relay::deliver`。`cell` ＝ `{"seq": n, "view": {…}}` 的文本（`view` 必须是对象；内容由界面严格收）。
    TerminalScreen { ticket: String, cell: String },
    /// 一条终端订阅停了（后端 `wire::Frame::TerminalFollowEnd`）。`cell` ＝ `{"why": 停因, "said": 那一句}`（同名原样，两格都得是串）。
    TerminalFollowEnd { ticket: String, cell: String },
    /// 一轮对话收尾（`turn_end`）。认识但不消费：轮次边界由 `line` 帧自己推（它是发给仓外消费方的）。
    TurnEnd,
    /// 那台的额度账变了（`quota_changed`）⇒ 交订了 `quota-changed` 的订阅一格；界面要额度就发 `quota-read` 读整份。
    QuotaChanged,
    /// 那台某个会话的轮换 / 「账号」格变了（`rotation_changed`，只带 sid）⇒ 同上一格 `{sid}`；界面要就发 `rotation-session-read`。
    RotationChanged { sid: String },
    /// 那台的轮换规则表 / 默认指向变了（`rotation_rules_changed`，无载荷）⇒ 同上一格 `{"rules":true}`；界面要就发 `rotation-rules-read`。
    RotationRulesChanged,
    /// 那台某个 pb 工作区的计划变了（`plan_changed`：工作区 · 新摘要 · 要你看的数，后端没给数 ⇒ `None`）⇒
    /// 交订了那台 `plan-changed` 的订阅一格 `{workspace, rev, needs}`；界面要就发 `plan-read`。
    PlanChanged {
        workspace: String,
        rev: String,
        needs: Option<u64>,
    },
}

/// 拥塞提示的措辞：有没有不可恢复的丢失，说法完全不同。抽成纯函数让措辞可判据（消费点要真 `AppHandle`、测不了）。
/// - 只丢了内容帧（`lost` 空；旧后端不发 `lost` 也落这一档）：行还在远端 jsonl 里，「重开会话可看完整历史」成立。
/// - 有不可恢复的丢失：点名主体，并明说重开会话补不回来。
/// - 身份表还被截断了：再加一句「清单不全」（理性做法是整体重取）。
pub(super) fn overflow_health_message(
    host_label: &str,
    dropped: u64,
    lost: &[LostFrameInfo],
    lost_truncated: bool,
) -> String {
    if lost.is_empty() {
        return copy_text(
            "rsSshSource.health.overflowLines",
            &[
                ("host", &host_label.to_string()),
                ("dropped", &dropped.to_string()),
            ],
        );
    }
    // 主体去重后点名（同一个会话可能连丢好几帧）。
    let mut subjects: Vec<&str> = lost.iter().filter_map(|l| l.subject.as_deref()).collect();
    subjects.sort_unstable();
    subjects.dedup();
    let named = if subjects.is_empty() {
        String::new()
    } else {
        format!("（{}）", subjects.join(" / "))
    };
    let truncated_note = if lost_truncated {
        &copy_text("rsSshSource.health.overflowTruncated", &[])
    } else {
        ""
    };
    copy_text(
        "rsSshSource.health.overflowLost",
        &[
            ("host", &host_label.to_string()),
            ("count", &(lost.len()).to_string()),
            ("named", &named.to_string()),
            ("truncatedNote", &truncated_note.to_string()),
        ],
    )
}

/// 一行解不成 [`InboundFrame`] 的原因。
///
/// 两种分开说：比这边新的后端发来这边不认识的种类是合法的（远端只升不降）⇒ 不断连、说一句；
/// 认识的种类缺必填格 / 类型不对 ⇒ 两端契约对不上，这一帧不用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unread {
    /// 这边不认识的种类。
    UnknownKind(String),
    /// 认识的种类，形状不对（`kind` 空 ＝ 连种类都读不出：不是 JSON 对象 / 没有 `kind`）。
    BadShape { kind: String, why: String },
}

impl std::fmt::Display for Unread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unread::UnknownKind(kind) => write!(f, "unknown frame kind `{kind}`"),
            Unread::BadShape { kind, why } => write!(f, "frame `{kind}` malformed: {why}"),
        }
    }
}

type Obj = serde_json::Map<String, serde_json::Value>;

fn bad(kind: &str, why: impl Into<String>) -> Unread {
    Unread::BadShape {
        kind: kind.to_string(),
        why: why.into(),
    }
}

/// 必填格：缺 ⇒ `BadShape`。
fn req<'a>(o: &'a Obj, kind: &str, key: &str) -> Result<&'a serde_json::Value, Unread> {
    o.get(key)
        .ok_or_else(|| bad(kind, format!("missing `{key}`")))
}

fn req_str(o: &Obj, kind: &str, key: &str) -> Result<String, Unread> {
    req(o, kind, key)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| bad(kind, format!("`{key}` is not a string")))
}

fn req_u64(o: &Obj, kind: &str, key: &str) -> Result<u64, Unread> {
    req(o, kind, key)?
        .as_u64()
        .ok_or_else(|| bad(kind, format!("`{key}` is not a non-negative integer")))
}

/// 必填的数组，原文收下（monitor 不解释）。
fn req_array_text(
    o: &Obj,
    kind: &str,
    key: &str,
) -> Result<crate::ui_contract::RecordBody, Unread> {
    let v = req(o, kind, key)?;
    v.is_array()
        .then(|| crate::ui_contract::RecordBody::from_json(v.to_string()))
        .flatten()
        .ok_or_else(|| bad(kind, format!("`{key}` is not an array")))
}

/// 认得的取值：认不出 ⇒ `BadShape`（不猜成哪一种）。
fn req_word<T>(o: &Obj, kind: &str, key: &str, read: fn(&str) -> Option<T>) -> Result<T, Unread> {
    let w = req_str(o, kind, key)?;
    read(&w).ok_or_else(|| bad(kind, format!("`{key}` = `{w}` is not a known value")))
}

/// 可缺的词：缺 ⇒ `None`；在但不是串 / 认不出 ⇒ 契约对不上。
fn opt_word<T>(
    o: &Obj,
    kind: &str,
    key: &str,
    read: fn(&str) -> Option<T>,
) -> Result<Option<T>, Unread> {
    match o.get(key) {
        None => Ok(None),
        Some(_) => req_word(o, kind, key, read).map(Some),
    }
}

/// 可缺的布尔：缺 ⇒ `None`；在但不是布尔 ⇒ 契约对不上。
fn opt_bool(o: &Obj, kind: &str, key: &str) -> Result<Option<bool>, Unread> {
    match o.get(key) {
        None => Ok(None),
        Some(v) => v
            .as_bool()
            .map(Some)
            .ok_or_else(|| bad(kind, format!("`{key}` is not a bool"))),
    }
}

/// `line` 帧的解码结构（最热的那一种：按类型直解，成品 `message` 以原文收下、不建 `Value`）。
/// 必填格不带 `#[serde(default)]`：缺了就是契约对不上。
#[derive(serde::Deserialize)]
struct LineFrame {
    session_id: String,
    path: String,
    seq: u64,
    byte_offset: u64,
    #[serde(default)]
    message: Option<Box<serde_json::value::RawValue>>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    rid: Option<String>,
}

fn decode_line(line: &str) -> Result<InboundFrame, Unread> {
    let f = serde_json::from_str::<LineFrame>(line).map_err(|e| bad("line", e.to_string()))?;
    Ok(InboundFrame::Line {
        session_id: f.session_id,
        path: f.path,
        seq: f.seq,
        message: f.message.map(crate::ui_contract::RecordBody),
        cwd: f.cwd,
        end: f.byte_offset,
        rid: f.rid,
    })
}

/// 把后端发来的一行（已去掉行尾 `\n`）解析成 [`InboundFrame`]。纯函数，绝不 panic。
///
/// - 必填与可选以后端 `wire::Frame` 为准（`skip_serializing_if` 的是可选）：必填格缺 / 类型不对 ⇒
///   [`Unread::BadShape`]；可选格缺 ⇒ 缺省。多出来的字段照旧忽略（加字段是契约允许的）。
/// - 不认识的 `kind` ⇒ [`Unread::UnknownKind`]。
/// - 两种都由调用方交 [`UnreadNotes`]：每条连接每种说一次，绝不中断流。
pub fn parse_frame(line: &str) -> Result<InboundFrame, Unread> {
    if line.starts_with(r#"{"kind":"line","#) {
        return decode_line(line);
    }
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|e| bad("", format!("not JSON: {e}")))?;
    let obj = value
        .as_object()
        .ok_or_else(|| bad("", "not a JSON object"))?;
    let kind = obj
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| bad("", "no string `kind`"))?;
    let k = kind;
    Ok(match kind {
        "hello" => {
            // `homes` / `capabilities` / `commands` 是可选格：缺 ⇒ 空；坏项逐项丢，不丢整帧。
            // `homes` 空 ⇒ 消费侧回落 `claude_dir`（[`claude_home_from_hello`]；今天后端恒发空表）。
            let homes: Vec<AgentHome> = obj
                .get("homes")
                .and_then(|h| h.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| {
                            let o = x.as_object()?;
                            Some(AgentHome {
                                agent_kind: o.get("agent_kind")?.as_str()?.to_string(),
                                path: o.get("path")?.as_str()?.to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let strings = |key: &str| -> Vec<String> {
                obj.get(key)
                    .and_then(|c| c.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            // 能力事实的另两格：读法住 `Offer::facts_of`（`resync` 应答同形）。
            let (unavailable, uncancellable) = crate::chan::wire::Offer::facts_of(obj);
            InboundFrame::Hello {
                v: req_u64(obj, k, "v")?,
                build_id: req_str(obj, k, "build_id")?,
                host_arch: req_str(obj, k, "host_arch")?,
                claude_dir: req_str(obj, k, "claude_dir")?,
                homes,
                capabilities: strings("capabilities"),
                commands: strings("commands"),
                unavailable,
                uncancellable,
            }
        }
        // 键序不是直解那一形时落到这里，同一个解码结构。
        "line" => decode_line(line)?,
        "session_added" => {
            let opt = |key: &str| obj.get(key).and_then(|v| v.as_str()).map(str::to_string);
            InboundFrame::SessionAdded {
                sid: req_str(obj, k, "sid")?,
                background: opt_bool(obj, k, "background")?.unwrap_or(false),
                // 只认真正的布尔；字符串 "false" 之类当没写（缺席 = true）。
                attachable: obj.get("attachable").and_then(|x| x.as_bool()),
                cwd: opt("cwd"),
                project_dir: opt("project_dir"),
                name: opt("name"),
                path: opt("path"),
                lines: obj.get("lines").and_then(|v| v.as_u64()),
                activity: opt_word(
                    obj,
                    k,
                    "activity",
                    crate::session_book::SessionActivity::from_wire,
                )?,
                waiting_for: opt("waiting_for"),
                // 开放联合：认得的宿主 · 不在宿主里 · 其它（原词带着）；形状不对 ⇒ 整帧 `BadShape`。
                container: match obj.get("container") {
                    None => None,
                    Some(v) => Some(
                        crate::session_book::SessionContainer::from_wire(v)
                            .map_err(|why| bad(k, why))?,
                    ),
                },
                // 只认装得进 u32 的非负整数；别的一律当没带。
                pid: obj
                    .get("pid")
                    .and_then(|v| v.as_u64())
                    .and_then(|n| u32::try_from(n).ok()),
            }
        }
        "sessions_replayed" => InboundFrame::SessionsReplayed,
        "session_file_gone" => InboundFrame::SessionFileNotice {
            sid: req_str(obj, k, "session_id")?,
            path: req_str(obj, k, "path")?,
            change: FileChange::Gone,
        },
        "session_file_reread" => InboundFrame::SessionFileNotice {
            sid: req_str(obj, k, "session_id")?,
            path: req_str(obj, k, "path")?,
            change: req_word(obj, k, "why", FileChange::reread_from_wire)?,
        },
        "session_status" => {
            let opt = |key: &str| obj.get(key).and_then(|v| v.as_str()).map(str::to_string);
            InboundFrame::SessionStatus {
                sid: req_str(obj, k, "sid")?,
                activity: opt_word(
                    obj,
                    k,
                    "activity",
                    crate::session_book::SessionActivity::from_wire,
                )?,
                waiting_for: opt("waiting_for"),
            }
        }
        "session_runs" => InboundFrame::SessionRuns {
            sid: req_str(obj, k, "sid")?,
            runs: req_array_text(obj, k, "runs")?,
            ended: req_array_text(obj, k, "ended")?,
        },
        "session_removed" => InboundFrame::SessionRemoved {
            sid: req_str(obj, k, "sid")?,
        },
        "session_state" => InboundFrame::SessionState {
            sid: req_str(obj, k, "sid")?,
            state: req_word(obj, k, "state", Fate::from_wire)?,
        },
        "overflow" => {
            // `lost` / `lost_truncated` 是可选格（缺 ⇒ 空 / false）；坏项逐项丢。
            let lost: Vec<LostFrameInfo> = obj
                .get("lost")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|it| {
                            let o = it.as_object()?;
                            Some(LostFrameInfo {
                                kind: o.get("kind")?.as_str()?.to_string(),
                                subject: o
                                    .get("subject")
                                    .and_then(|x| x.as_str())
                                    .map(str::to_string),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            InboundFrame::Overflow {
                dropped: req_u64(obj, k, "dropped")?,
                lost,
                lost_truncated: obj
                    .get("lost_truncated")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            }
        }
        // 入方向应答：由 `inbound_client` 按 `id` 路由回请求方。
        "reply" => {
            let opt = |key: &str| obj.get(key).and_then(|v| v.as_str()).map(str::to_string);
            InboundFrame::Reply {
                id: req_str(obj, k, "id")?,
                ok: req(obj, k, "ok")?
                    .as_bool()
                    .ok_or_else(|| bad(k, "`ok` is not a bool"))?,
                code: opt("code"),
                message: opt("message"),
                detail: opt("detail"),
                data: obj.get("data").cloned(),
            }
        }
        "cancelled" => InboundFrame::Cancelled {
            id: req_str(obj, k, "id")?,
        },
        "accounts_changed" => InboundFrame::AccountsChanged,
        "profiles_changed" => InboundFrame::ProfilesChanged,
        "tasks_changed" => InboundFrame::TasksChanged {
            sid: req_str(obj, k, "sid")?,
        },
        // 链路两帧。`data` 解不开 ⇒ 坏帧，不交一段猜出来的字节。
        "link_data" => InboundFrame::LinkData {
            link: req_str(obj, k, "link")?,
            data: crate::link_mux::b64_decode(&req_str(obj, k, "data")?)
                .map_err(|_| bad(k, "`data` is not base64"))?,
        },
        "link_end" => InboundFrame::LinkEnd {
            link: req_str(obj, k, "link")?,
            error: obj
                .get("error")
                .and_then(|e| e.as_str())
                .map(str::to_string),
        },
        // 传输进度 / 终局。`end` 在 ⇒ 必须是后端那三形之一（不猜一个结局）。
        "transfer" => InboundFrame::Transfer {
            id: req_str(obj, k, "id")?,
            got: req_u64(obj, k, "got")?,
            total: req_u64(obj, k, "total")?,
            end: match obj.get("end") {
                None => None,
                Some(e) => {
                    Some(transfer_end(e).ok_or_else(|| bad(k, "`end` is not a known shape"))?)
                }
            },
        },
        // 测试连接的一格进度：票 ＋ 原样那一格（必须是对象；内容由界面严格收）。
        "probe" => InboundFrame::Probe {
            ticket: req_str(obj, k, "ticket")?,
            cell: Some(req(obj, k, "cell")?)
                .filter(|c| c.is_object())
                .ok_or_else(|| bad(k, "`cell` is not an object"))?
                .to_string(),
        },
        // 终端实时预览：票 ＋ 序号 ＋ 那一屏（必须是对象，原样转交）。
        "terminal_screen" => InboundFrame::TerminalScreen {
            ticket: req_str(obj, k, "ticket")?,
            cell: serde_json::json!({
                "seq": req_u64(obj, k, "seq")?,
                "view": Some(req(obj, k, "view")?)
                    .filter(|v| v.is_object())
                    .ok_or_else(|| bad(k, "`view` is not an object"))?,
            })
            .to_string(),
        },
        // 终端订阅停了：停因（给程序认）与那一句（后端写好）原样转交，不认闭集、不改名 —— 停因有几种归后端说。
        "terminal_follow_end" => InboundFrame::TerminalFollowEnd {
            ticket: req_str(obj, k, "ticket")?,
            cell: serde_json::json!({
                "why": req_str(obj, k, "why")?,
                "said": req_str(obj, k, "said")?,
            })
            .to_string(),
        },
        // 一件归一事件。`ev` 与 `end` 恰有一个：先认 `ev`（必须是对象，原样转交），没有就必须是认得的 `end`。`run` 缺 ＝ 主运行。
        "tap" => {
            let run = match obj.get("run") {
                None => None,
                Some(r) => Some(
                    r.as_str()
                        .ok_or_else(|| bad(k, "`run` is not a string"))?
                        .to_string(),
                ),
            };
            let body = match obj.get("ev") {
                Some(e) => crate::session_tap::TapBody::Ev(
                    e.as_object()
                        .and_then(|_| crate::ui_contract::RecordBody::from_json(e.to_string()))
                        .ok_or_else(|| bad(k, "`ev` is not an object"))?,
                ),
                None => crate::session_tap::TapBody::End(req_word(
                    obj,
                    k,
                    "end",
                    crate::session_tap::TapEnd::from_wire,
                )?),
            };
            InboundFrame::Tap(crate::session_tap::Tap {
                stream: req_str(obj, k, "stream")?,
                run,
                resp: req_u64(obj, k, "resp")?,
                n: req_u64(obj, k, "n")?,
                body,
            })
        }
        // 认识但不消费：轮次边界由 `line` 帧自己推（它是发给仓外消费方的）。形状照样判。
        "turn_end" => {
            req_str(obj, k, "session_id")?;
            req_str(obj, k, "uuid")?;
            InboundFrame::TurnEnd
        }
        // 交订了 `quota-changed` 的订阅：界面要额度就发 `quota-read` 读整份。
        "quota_changed" => InboundFrame::QuotaChanged,
        // 同上一格 `{sid}`：界面要就发 `rotation-session-read`。
        "rotation_changed" => InboundFrame::RotationChanged {
            sid: req_str(obj, k, "sid")?,
        },
        // 同上一格 `{"rules":true}`：界面要就发 `rotation-rules-read`。
        "rotation_rules_changed" => InboundFrame::RotationRulesChanged,
        // 交订了 `plan-changed` 的订阅：界面要就发 `plan-read`。`needs` 可缺（老后端不给）。
        "plan_changed" => InboundFrame::PlanChanged {
            workspace: req_str(obj, k, "workspace")?,
            rev: req_str(obj, k, "rev")?,
            needs: obj.get("needs").and_then(serde_json::Value::as_u64),
        },
        _ => return Err(Unread::UnknownKind(kind.to_string())),
    })
}

/// 一条连接上解不出来的帧：日志与健康信息都**每种说一次**（不认识的种类 · 形状不对的种类分开算），
/// 之后只记账（[`crate::frame_tally::FrameTally`] 按 2 的幂次出一行、流结束出总账）。
pub(crate) struct UnreadNotes {
    /// 健康信息归哪台（`RemoteHealthPayload::origin`）。
    origin: String,
    /// 文案里那台叫什么。
    label: String,
    said: std::collections::BTreeSet<(bool, String)>,
}

impl UnreadNotes {
    pub(crate) fn new(origin: impl Into<String>, label: impl Into<String>) -> Self {
        UnreadNotes {
            origin: origin.into(),
            label: label.into(),
            said: Default::default(),
        }
    }

    /// 解一行。解不出来 ⇒ 记账；这一种在这条连接上第一次 ⇒ 日志一条（带原因）＋ 经 `health` 说一句。
    /// 回 `None` ＝ 这一行不用，调用方跳过（不断连）。
    pub(crate) fn take(
        &mut self,
        line: &str,
        tally: &mut crate::frame_tally::FrameTally,
        health: &dyn Fn(crate::ui_contract::RemoteHealthPayload),
    ) -> Option<InboundFrame> {
        let why = match parse_frame(line) {
            Ok(f) => return Some(f),
            Err(why) => why,
        };
        if let Some(n) = tally.note_unparsed(line) {
            tracing::warn!("{n}");
        }
        let (unknown, kind) = match &why {
            Unread::UnknownKind(kind) => (true, kind),
            Unread::BadShape { kind, .. } => (false, kind),
        };
        if self.said.insert((unknown, kind.clone())) {
            tracing::warn!("[{}] {why}", self.origin);
            health(unread_health(&self.origin, &self.label, &why));
        }
        None
    }
}

/// 解不出来那一帧的健康信息（每条连接每种一次）。
pub(crate) fn unread_health(
    origin: &str,
    label: &str,
    why: &Unread,
) -> crate::ui_contract::RemoteHealthPayload {
    let (kind, message) = match why {
        Unread::UnknownKind(k) => (
            "frame-unknown",
            copy_text(
                "rsSshSource.health.unknownFrame",
                &[("host", &label.to_string()), ("kind", k)],
            ),
        ),
        Unread::BadShape { kind: k, .. } => (
            "frame-shape",
            copy_text(
                "rsSshSource.health.badFrame",
                &[
                    ("host", &label.to_string()),
                    (
                        "kind",
                        &if k.is_empty() {
                            "?".to_string()
                        } else {
                            k.clone()
                        },
                    ),
                ],
            ),
        ),
    };
    crate::ui_contract::RemoteHealthPayload {
        origin: origin.to_string(),
        kind: kind.to_string(),
        message,
        detail: String::new(),
    }
}

/// 本机那两条载体（stdio · 脱离）的健康出口：窗口把手只在宿主层，`lib.rs` setup 装一次。
static LOCAL_HEALTH: std::sync::OnceLock<HealthOut> = std::sync::OnceLock::new();

pub(crate) fn install_local_health(out: HealthOut) {
    if LOCAL_HEALTH.set(out).is_err() {
        tracing::warn!("本机健康信息的出口装了第二次 —— 忽略");
    }
}

/// 本机载体说一句健康信息（出口没装 ⇒ 只有日志那一条）。
pub(crate) fn local_health(payload: crate::ui_contract::RemoteHealthPayload) {
    if let Some(out) = LOCAL_HEALTH.get() {
        if let Err(e) = out(payload) {
            tracing::warn!("local remote-health emit failed: {e}");
        }
    }
}

/// `transfer` 帧的 `end`：后端 `wire::TransferEnd` 那三形之一；认不出 ⇒ `None`（调用方整帧丢）。
/// 抽出来住 `parse_frame` 外面：那张 match 的臂是帧 kind 的名单（判据按臂抠），结局的三个名字不该混进去。
fn transfer_end(e: &serde_json::Value) -> Option<crate::sftp_pool::End> {
    Some(match e.get("state")?.as_str()? {
        "done" => crate::sftp_pool::End::Done {
            bytes: e.get("bytes")?.as_u64()?,
            // 可缺席（下载那一路没有）；在就原样带着（窗口提交时交回，形状由后端那一关判）。
            sha256: e.get("sha256").and_then(|v| v.as_str()).map(str::to_string),
        },
        // 带码的那一形（今天只有 `sftp_home_mismatch`）单列一形，窗口按码换路。
        // 码（今天只有 `sftp_home_mismatch`）与复制详情都可缺席，在就原样带着（窗口按码换路、详情进［复制详情］）。
        "failed" => crate::sftp_pool::End::Failed {
            why: e.get("why")?.as_str()?.to_string(),
            code: e.get("code").and_then(|v| v.as_str()).map(str::to_string),
            detail: e
                .get("detail")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        },
        "cancelled" => crate::sftp_pool::End::Cancelled,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/parse_frame_tests.rs"]
mod parse_frame_tests;

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/golden_tests.rs"]
mod golden_tests;
