//! **后端发给客户端的帧**的线上形 —— 两侧同一份：后端按这里的类型发（判定与写字在后端 `wire` 的扩展特征里），客户端按这里的类型解（[`decode`]）。
//!
//! 一帧一行 JSON，`kind` 是标签、写在最前，其余格平铺；每种帧一个具名结构（变体名就是 `kind` 的驼峰写法）。
//! 记录那一格（`line.record`）是类型参数 `R`：后端用 [`Record`]；原文照转的一端用 `Box<RawValue>`（不建树）；
//! 手机用 [`crate::record::OrUnread`]（解不开的那一条成认不出那一条，整帧不失败）。
//! TS 名带 `Wire` 前缀的那几样（只进形状指纹，不导出）：界面另有壳给的同名形状。
//! 体格 `Probe.cell` · `Reply.data` · `Changed.body` 仍是 JSON 值（各有自己的严格收处）。

use crate::cells::{Tone, Words};
use crate::files::WirePath;
use crate::live::LiveCard;
use crate::record::Record;
use crate::session::Hidden;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// 后端发给客户端的一帧；`kind` 是标签，例如 `{"kind":"hello","v":1,...}`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Frame<R = Record> {
    Hello(Hello),
    Line(Line<R>),
    SessionAdded(SessionAdded),
    SessionPrimed(SessionPrimed),
    SessionStatus(SessionStatus),
    SessionState(SessionState),
    SessionRemoved(SessionRemoved),
    Overflow(Overflow),
    Reply(Reply),
    Cancelled(Cancelled),
    Changed(Changed),
    SessionsReplayed(SessionsReplayed),
    SessionFileGone(SessionFileGone),
    SessionFileReread(SessionFileReread),
    LinkData(LinkData),
    LinkEnd(LinkEnd),
    Transfer(Transfer),
    FilesBatch(FilesBatch),
    Probe(Probe),
    LiveCard(LiveCard),
    SessionRuns(SessionRuns),
    SessionBranch(SessionBranch),
    TerminalScreen(TerminalScreen),
    TerminalFollowEnd(TerminalFollowEnd),
}

/// `line` 帧的 `kind`（[`decode`] 认它走直解那一路）。
const LINE: &str = "line";

/// 一行 ⇒ 一帧。`line` 那一种直接解它自己的结构（记录那一格可能是进不了内部标签枚举的 `RawValue`），别的经枚举。
/// 标签照例在行首（`{"kind":"…"`），不在行首就整行扫一次找它。
pub fn decode<'a, R: Deserialize<'a>>(line: &'a str) -> serde_json::Result<Frame<R>> {
    let kind = kind_of(line)?;
    if kind == LINE {
        serde_json::from_str::<Line<R>>(line).map(Frame::Line)
    } else {
        serde_json::from_str(line)
    }
}

/// 帧表里 `line` 那一行按它算：记录那一格不进帧的指纹（记录认不认得一条一条判：手机的 [`crate::record::OrUnread`] · 界面认不出那张卡）。
#[cfg(any(test, feature = "ts"))]
#[derive(ts_rs::TS)]
#[ts(type = "unknown", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))]
pub struct OpaqueRecord;

/// 进帧表的每种帧（`hello` 除外：门靠它）与它的 TS 形状。变体多一种这里编不过（`all` 那张 `match` 是穷尽的）。
#[cfg(any(test, feature = "ts"))]
pub fn gated_tys() -> Vec<(&'static str, crate::fingerprint::Ty)> {
    use crate::fingerprint::ty_of;
    macro_rules! table {
        ($($v:ident => $k:literal),* $(,)?) => {{
            #[allow(dead_code)]
            fn all(f: &Frame<OpaqueRecord>) {
                match f {
                    Frame::Hello(_) => {}
                    $(Frame::$v(_) => {})*
                }
            }
            vec![$(($k, ty_of::<table!(@ty $v)>())),*]
        }};
        (@ty Line) => { Line<OpaqueRecord> };
        (@ty $v:ident) => { $v };
    }
    table! {
        Line => "line",
        SessionAdded => "session_added",
        SessionPrimed => "session_primed",
        SessionStatus => "session_status",
        SessionState => "session_state",
        SessionRemoved => "session_removed",
        Overflow => "overflow",
        Reply => "reply",
        Cancelled => "cancelled",
        Changed => "changed",
        SessionsReplayed => "sessions_replayed",
        SessionFileGone => "session_file_gone",
        SessionFileReread => "session_file_reread",
        LinkData => "link_data",
        LinkEnd => "link_end",
        Transfer => "transfer",
        FilesBatch => "files_batch",
        Probe => "probe",
        LiveCard => "live_card",
        SessionRuns => "session_runs",
        SessionBranch => "session_branch",
        TerminalScreen => "terminal_screen",
        TerminalFollowEnd => "terminal_follow_end",
    }
}

/// 一行的 `kind`（不解别的格）：照例在行首就直接切出来，不在就整行扫一次。
pub fn kind_of(line: &str) -> serde_json::Result<String> {
    match line
        .strip_prefix(r#"{"kind":""#)
        .and_then(|r| r.split_once('"'))
    {
        Some((k, _)) => Ok(k.to_string()),
        None => Ok(serde_json::from_str::<KindOnly>(line)?.kind),
    }
}

#[derive(Deserialize)]
struct KindOnly {
    kind: String,
}

/// hello 的头：版本门第一步只解它（全形解不开时也认得出是不是我们的后端、是哪一版）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HelloHead {
    /// 这份后端二进制的身份（`BUILD_ID`）。
    pub build_id: String,
}

impl HelloHead {
    /// 这一行是 hello、头那一格解得出 ⇒ `Some`；别的 ⇒ `None`（那头不是我们的后端）。
    pub fn of(line: &str) -> Option<HelloHead> {
        if kind_of(line).ok()? != "hello" {
            return None;
        }
        serde_json::from_str(line).ok()
    }
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// 握手：连上之后那台发的第一帧。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Hello {
    /// 这份后端二进制的身份（`BUILD_ID`）；换不换后端按它判。
    pub build_id: String,
    /// 这台机器的架构（`x86_64` / `aarch64` …）。
    pub host_arch: String,
    /// 这台后端接受的入方向命令；空 ⇒ 它不读命令。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<Vec<String>>"))]
    pub commands: Vec<String>,
    /// 接得下、但这台做不到的命令与原因（见 [`Unavailable`]）；空 ＝ 没有「做不到」的把握。是提示不是闸门。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(optional, as = "Option<Vec<Unavailable>>")
    )]
    pub unavailable: Vec<Unavailable>,
    /// 起它的宿主交给它的那几格环境，原样回显（`{名: 值}`，名单住后端 `wire::HOST_ECHO_ENVS`）。一格都没被交 ⇒ 省略。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(optional, as = "Option<BTreeMap<String, String>>")
    )]
    pub host_env: BTreeMap<String, String>,
    /// typed 命令的形状指纹 `{命令: 8 位十六进制}`（[`crate::shapes::SHAPES`]）。客户端拿它自己编进来的那一张比：
    /// 对得上就直接按类型用，对不上就说「那台后端版本不同」、一个字节不发。没有 typed 命令 ⇒ 省略。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(optional, as = "Option<BTreeMap<String, String>>")
    )]
    pub shapes: BTreeMap<String, String>,
    /// 每种帧（`hello` 除外）的形状指纹 `{kind: 8 位十六进制}`（[`crate::frame_shapes::FRAMES`]）。客户端拿它自己编进来的那一张比：
    /// 对不上的那几种不解、说一次「那台后端版本不同」，别的照常。空 ⇒ 省略（那就是一种都对不上）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(optional, as = "Option<BTreeMap<String, String>>")
    )]
    pub frames: BTreeMap<String, String>,
}

/// 会话记录里的一行 —— 带的是**成品**：这一行在界面里是什么（`record`，缺 ＝ 不进界面、照占号）与它自己的 `cwd`。
///
/// **两个前端共同的契约面**：`session_id` · `path` · `seq` · `byte_offset` 四格在冻结表里（`wire_tests::the_shapes_the_second_frontend_reads_stay_put`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Line<R = Record> {
    /// 会话 id。
    pub session_id: String,
    /// 这份会话记录在那台机器上的绝对路径。
    pub path: String,
    /// 这一行在本条流里的序号（按文件单调递增）；不是续传键，续传用 `byte_offset`。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub seq: u64,
    /// 这一行的通用记录；缺 ＝ 不进界面、照占号。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub record: Option<R>,
    /// 这条记录自己的工作目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub cwd: Option<String>,
    /// backend-01（gap#2，additive 不 bump PROTO_VERSION）：本行末尾（含 `\n`）在文件中的**累计原始字节 offset**——
    /// 语义逐字节对齐手机端 `LineFramer.endOffset`：计 CRLF 的 `\r`、含 `\n`、残行不计；resume N ⇒ `tail -c +(N+1)`。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub byte_offset: u64,
    /// 这一行的对账键（适配层给；流的「开始」带同一个值）。没有 ⇒ 不上线。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub rid: Option<String>,
}

/// A new session file appeared.
///
/// 附带 pidfile 元信息（`cwd` · `name` …）；缺的格不上线。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionAdded {
    /// 会话 id。
    pub sid: String,
    /// 会话属哪一家（注册表里那一家的 kind：`"claude"` · `"codex"` …）。每条都带。
    pub agent_kind: String,
    /// 是不是后台会话（不是人坐在终端里对话的那种）。适配层判，客户端只读这一格；只在 `true` 时上线。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub background: bool,
    /// **E73（additive）：attach 进去对人有没有意义。** `false` = 别给 attach / ↗ / 「杀死空 tmux」这几个动作；**省略 = `true`**。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub attachable: Option<bool>,
    /// pidfile 记的 `cwd`：进程起在哪个目录（客户端认「我刚起的那条起来了」用）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub cwd: Option<String>,
    /// 会话的项目目录：会话起在哪个目录（tab 标题 · 打开工作目录 · 分组都用它）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub project_dir: Option<String>,
    /// pidfile 里的会话名。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub name: Option<String>,
    /// Batch8-F25（additive）：该会话 jsonl 的远端绝对路径（同 sid 多文件时取 mtime 最新者）；宣告时还没写首行 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub path: Option<String>,
    /// 宣告时这条会话的整份脸。
    pub face: SessionFace,
    /// 叫这条会话什么（[`NameFace`]，平铺在帧上）。
    #[serde(flatten)]
    pub name_face: NameFace,
    /// 宣告时在等什么（同 `session_status`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub waiting_for: Option<String>,
    /// 隐藏那一格（不占标签页）；不隐藏 ⇒ 不上线。之后变了经主题 `session_hidden` 推。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub hidden: Option<Hidden>,
    /// 〔additive〕这条会话住在什么容器里（见 [`SessionContainer`]）。**判不了 ⇒ 不上线**（缺席是「不知道」，不是 `none`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub container: Option<SessionContainer>,
    /// 那个 claude 进程的 **pid**（恒在）。不要它的出口在声明里去掉（`omit: {session_added: ["pid"]}`）。
    pub pid: u32,
}

/// 这条会话的记录读完了（宣告之后后台那一趟预读做完）：从这一帧起它的 `line` 帧从第 `lines` 行起、连续地来。
///
/// 会话没有记录（刚起还没写首行）⇒ 两格都缺：之后的行天然从 0 起全量到。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionPrimed {
    /// 会话 id。
    pub sid: String,
    /// 读完的那份记录（修改时刻最新的那份）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub path: Option<String>,
    /// 那份记录此刻的完整行数 L（之后的第一帧 `line` 的 `seq` 就是它）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub lines: Option<u64>,
}

/// Batch9-F27：会话 status 变化（pidfile modify diff；CC 仅状态转换时重写，天然稀疏）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionStatus {
    /// 会话 id。
    pub sid: String,
    /// 此刻这条会话的整份脸。
    pub face: SessionFace,
    /// 叫这条会话什么（同 `session_added` 那两格；改了名、来了标题 / 第一句也发这一帧）。
    #[serde(flatten)]
    pub name_face: NameFace,
    /// 在等什么（pidfile 里的 `waitingFor`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub waiting_for: Option<String>,
    /// 这条会话的记录路径（同 `session_added` 那一格）：宣告之后才定下来 · 换了一份 ⇒ 带上最新的；没变 ⇒ 不上线。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub path: Option<String>,
}

/// **会话账本的成品**：这条会话离开「活」之后是可重连还是已结束（见 [`SessionFate`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionState {
    /// 会话 id。
    pub sid: String,
    /// 离开「活」之后的去向。
    pub state: SessionFate,
    /// 离开「活」之后这条会话的整份脸（可重连 · 已结束 · 记录已不在）。
    pub face: SessionFace,
}

/// A session file went away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionRemoved {
    /// 会话 id。
    pub sid: String,
    /// **S0（additive）：这个 sid 是「死了」还是「被顶替了」。** [`RemovalCause::Gone`] 不写字段；只有 `Superseded` 才出现 `"cause":"superseded"`。
    #[serde(default, skip_serializing_if = "RemovalCause::is_gone")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<RemovalCause>"))]
    pub cause: RemovalCause,
    /// 被顶替时（`cause: superseded`），顶替它的那条会话的 sid；不是顶替 ⇒ 不上线。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub by: Option<String>,
}

/// 这条连接拥塞丢过帧：那台写好的那一句（只丢了行 · 会话状态已重新对齐 · 两样都有）。
/// 丢了会话状态那一类的，那台紧跟着在同一条连接上把此刻的样子整份重报一次，客户端不判、不重接。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Overflow {
    /// 那一句。
    pub said: Words,
}

/// U6b-1：**入方向命令的应答**。`id` 是客户端给的不透明串，backend **原样回显、不解析**。
///
/// 错误形状 `{code, message}`（协议 v1 §3），不发明第二种错误 JSON。成功时两者都省略。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Reply {
    /// 回显请求的 `id`。
    pub id: String,
    /// 成功与否。
    pub ok: bool,
    /// 失败时的码（协议级或命令级）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub code: Option<String>,
    /// 失败时给人看的那一句（不含下层原话与码：那些进 `detail`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub message: Option<String>,
    /// 失败时「复制详情」那几行（时刻 · 机器 · 命令 · 码 · 原话），后端写好、界面原样复制。成功时省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub detail: Option<String>,
    /// U6b-3：命令的返回值（如 `resolve` 的 CommandPlan）。无返回值的命令省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "unknown"))]
    pub data: Option<Value>,
}

/// U6b-1：某个在跑的命令**已被取消**。取消是一条普通命令（`cmd:"cancel"`）、不是带外信号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Cancelled {
    /// 被取消的那条命令的 `id`。
    pub id: String,
}

/// **这台的某样东西变了，客户端重读那一份**（主题 [`Topic`]）。
///
/// `key` 说是哪一个（会话 · 工作区）；`rev` 说变成了哪一版；`body` 是那一样的小成品（超了上限就不带，客户端重问）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Changed {
    /// 哪一样变了。
    pub topic: Topic,
    /// 哪一个（会话 id · 工作区根）；主题不带 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub key: Option<String>,
    /// 变成了哪一版；主题不带 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub rev: Option<String>,
    /// 那一样的小成品；主题不带或超了上限 ⇒ 缺（客户端重问）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "unknown"))]
    pub body: Option<Value>,
}

/// **这台机器的活会话清单报完了**：观察那一侧第一趟扫完、对每个活会话发过 `session_added` 之后发**一次**。
///
/// 无载荷：清单本身就是它前面那些 `session_added`（同一条有序的流）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionsReplayed {}

/// **活会话的记录文件不见了**（被删 / 被改名走了）。每次「在 → 不在」只发一帧。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionFileGone {
    /// 会话 id。
    pub session_id: String,
    /// 不见了的那份记录文件。
    pub path: String,
}

/// **活会话的记录文件被改过了，已从头重读**（截短 · 或游标之前被原地改写）。
///
/// 重读那一趟照读照数、不逐行上流；行号从 0 重数 ⇒ 下游据这一帧把这个会话旧的一代整份作废，照宣告那样从尾段取到第 `lines` 行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionFileReread {
    /// 会话 id。
    pub session_id: String,
    /// 被改过、已从头重读的那份记录文件。
    pub path: String,
    /// 为什么从头重读。
    pub why: RereadWhy,
    /// 重读完那份记录的完整行数 L（之后的第一帧 `line` 的 `seq` 就是它）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub lines: u64,
}

/// **一条链路的下行字节**。`data` 是 base64（标准字母表、带补位）。🔴 **不丢**：走应答那条独立通道。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct LinkData {
    /// 链路 id（`link-open` 时客户端给的）。
    pub link: String,
    /// 下行字节，标准 base64（解码后 ≤ 32 KiB）。
    pub data: String,
}

/// **一条链路不会再有字节了**；后端已经忘掉这个 `link` id。`error` 省略 = 正常收尾。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct LinkEnd {
    /// 链路 id。
    pub link: String,
    /// 非正常收尾的原话；缺 ＝ 正常收尾。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub error: Option<String>,
}

/// **一趟传输此刻的样子**（`transfer-start` 之后才出现）。每一帧都是一整份快照；带 `end` 的那一帧是这一趟的最后一帧。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Transfer {
    /// 传输单 id。
    pub id: String,
    /// 已传字节。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub got: u64,
    /// 总字节。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub total: u64,
    /// 只在最后一帧：这一趟怎么收场的。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub end: Option<TransferEnd>,
}

/// **一组（同一台上的复制 · 删除 · 改权限，两台之间的复制）此刻做到哪了**。每一帧都是一整份快照；结局在那一问的应答里。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct FilesBatch {
    /// 那一组的票（发起时交来的，后端只当不透明的串回填）。
    pub ticket: String,
    /// 做完了几项。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub done: u64,
    /// 一共几项。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub total: u64,
    /// 手上那一项（根底下的相对段）；做完了缺席。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub current: Option<WirePath>,
    /// 整组按字节已传几个（只有搬字节的那一种报：两台之间的复制）；不报 ⇒ 缺席。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub bytes_done: Option<u64>,
    /// 整组按字节一共几个（同 `bytes_done`，两格同在同缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub bytes_total: Option<u64>,
}

/// 〔「测试连接的进度不许倒退」〕**测试连接那一趟的一格进度**（`remote-probe` 在跑时才出现）。
///
/// `cell` 恰好一个键：`stage`（握手那几行）· `reached`（`ssh` / `hello` / `control` 走完了那一段）· `end`（结局，最后一格）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Probe {
    /// 测试连接那一趟的票（`remote-probe` 交来的）。
    pub ticket: String,
    /// 一格进度，恰好一个键：`stage` · `reached` · `end`。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
    pub cell: Value,
}

/// 一个会话的运行表（主运行之外的那几个子运行）。表变了就整份发一次；`ended`：被挤出运行表的已收场子运行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionRuns {
    /// 会话 id。
    pub sid: String,
    /// 在表里的子运行（整份）。
    pub runs: Vec<RunInfo>,
    /// 被挤出表的已收场子运行。
    pub ended: Vec<RunEnded>,
}

/// 一份会话记录的**主线外清单**（用户回退重发后留在文件里的旧那一支）：那几条记录的 `id`。
///
/// **「前 `at` 条照旧，其后换成 `off`」**：变了才发，只发变了的那一截；整份从 `at = 0` 起、按页切成几帧接着发。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionBranch {
    /// 会话 id。
    pub sid: String,
    /// 这份会话记录在那台机器上的绝对路径。
    pub path: String,
    /// 前几条照旧。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub at: u64,
    /// 第 `at` 条起的那几条 `id`（之后没有了：清单到此为止）。
    pub off: Vec<String>,
    /// 清单前 `at + off.len()` 条的摘要：客户端在 `stream-watch` 的 `branch` 里原样交回，后端照它判手上那份前缀还对不对。
    pub sum: String,
}

/// **一个终端此刻的一整屏**（终端实时预览；`terminal-follow` 之后才出现）。
///
/// `view` 与 `terminal-preview` 的回话同一份成品；每帧整屏、不是增量；一帧在途（客户端回执之后才推下一帧）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct TerminalScreen {
    /// 订阅票（`terminal-follow` 时客户端给的，后端只当不透明的串回填）。
    pub ticket: String,
    /// 这条订阅里第几帧（从 1 连续）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub seq: u64,
    /// 那一屏（同 `terminal-preview` 的回话）。
    pub view: crate::session::Preview,
}

/// **一条终端订阅停了**（不会再有画面）；后端已经忘掉这张票。客户端自己退订的不发这一帧。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct TerminalFollowEnd {
    /// 订阅票。
    pub ticket: String,
    /// 为什么停了（给程序认）。
    pub why: FollowEnd,
    /// 给人看的那一句（后端写好，界面原样上屏）。
    pub said: String,
}

// ───────────────────────────── 帧里的零件 ─────────────────────────────

/// 一条活会话此刻在干什么（与哪一家无关的几态；适配层从那一家的进程状态翻过来，翻不出 ⇒ 不给）。
/// 线上整份脸 `face.live.activity` 送它。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "WireSessionActivity", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub enum SessionActivity {
    /// 一轮在跑。
    Working,
    /// 在等人（批准 · 回答 · 弹窗）。
    NeedsYou,
    /// 闲着，等下一句输入。
    Idle,
    /// 一轮停了，它在后台起的命令还在跑（跑完多半会接着干）。
    BackgroundWork,
}

/// **一条会话此刻的整份脸**（会话三帧 · 清单行 · 需手动行 · 计划 · 血缘都带它，嵌一格 `face`）：点 · 短名 · 悬停句 · 状态一句 · 序 · 能做什么。
/// 只由后端那一处造（`SessionFace::of`，判据 `the_written_faces_are_built_in_one_place_only` 守）；两端只取同一 sid 最新那一份来画。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionFace {
    pub dot: SessionDot,
    /// 进程在不在；在干什么只住在「在」那一支里。
    pub live: FaceLive,
    /// 那台看不见时照旧作数（已结束 · 记录已不在）。
    pub settled: bool,
    /// 短名（运行中 · 需手动 · 已结束 …）。
    pub name: Words,
    /// 悬停那一句。
    pub tip: Words,
    /// 没有会话事实时的状态一句。
    pub line: Words,
    pub tone: Tone,
    /// 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
    pub order: u8,
    pub can: SessionCan,
}

/// 进程在不在：`{is: "yes", activity?}`（判得出在干什么才带 `activity`）· `{is: "no"}`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "is", rename_all = "snake_case")]
pub enum FaceLive {
    Yes {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        activity: Option<SessionActivity>,
    },
    No,
}

/// 会话那颗点（线上名与界面状态点同一套）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "kebab-case")]
pub enum SessionDot {
    Running,
    NeedsYou,
    Background,
    Idle,
    Exited,
    Ended,
    Gone,
    Unknown,
}

/// 这条会话此刻能做什么（界面照 `can` 灰不灰、照 `why` 写跳过的理由，不自己判）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionCan {
    /// 关掉它的标签页。
    pub close: CanClose,
    /// 恢复（起一个新进程接着那份记录）。
    pub resume: Allow,
    /// 还有终端可去（拉前 · 停 · 杀 · 终端页）。
    pub term: CanTerm,
    /// 程序退了、tmux 会话还在 ⇒ 接回。
    pub reattach: Allow,
    /// 换模型（`session-model`）。
    pub model: Allow,
    /// 切权限档（`session-permission`）。
    pub permission: Allow,
}

/// 一样此刻能不能做：`{can: "yes"}` · `{can: "no", why: 写好的那一句}`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "can", rename_all = "snake_case")]
pub enum Allow {
    Yes,
    No { why: Words },
}

/// 关不关得掉：`quiet` × · 中键 · W · 批量都关得掉 · `menu` 只在右键菜单里关（那台看不见时界面自己给的那一形）· `no`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "can", rename_all = "snake_case")]
pub enum CanClose {
    Quiet,
    Menu,
    No { why: Words },
}

/// 还有没有终端可去：`yes` 带它在不在 tmux 里（终端窗口关了也能「在终端里打开」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "can", rename_all = "snake_case")]
pub enum CanTerm {
    Yes { tmux: bool },
    No { why: Words },
}

/// **这条会话叫什么那两格**（`session_added` · `session_status` 都带它，平铺在帧上；一轮结束的通知标题也读它）：
/// `label` 与历史清单 · 全文搜索同一个函数算；`label_project` ＝ 项目目录最后一截（不知道项目目录 ⇒ 不带）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct NameFace {
    pub label: Words,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub label_project: Option<Words>,
}

/// 终端住在哪种宿主里（线上 `host`）。容器那一格（[`SessionContainer`]）与 `terminals-list` 每一行的 `host` 同一套词。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "WireTerminalHost", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub enum TerminalHost {
    Tmux,
}

impl TerminalHost {
    /// 线上那个词（`terminals-list` 的 `host` 那一格是串，从这里取）。
    pub const fn as_wire(self) -> &'static str {
        match self {
            TerminalHost::Tmux => "tmux",
        }
    }
}

/// `session_added` 的 `container`：**这条活着的会话住在什么容器里**，词与 `terminals-list` 同一套。
///
/// 线上是对象：`{"host":"tmux","terminal":"tmux-3-7"}` · `{"host":"none"}`。在宿主里的，claude 退了终端还在（「可重连」）；
/// 不在的，只能 resume。`terminal` 是 `terminals-list` 里那一行的句柄；打标那一刻算不出 ⇒ 不写这一格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "WireSessionContainer", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "host", rename_all = "snake_case")]
pub enum SessionContainer {
    /// 在 tmux 的终端里（`TMUX_PANE` 核过形状、tmux 认得那个 pane）。
    Tmux {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        terminal: Option<String>,
    },
    /// 环境读得到、不在任何宿主里。
    None,
}

/// `session_state` 的 `state`：**一条会话离开「活」之后是什么** —— 那台机器的后端自己裁，客户端只收成品、不再猜。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub enum SessionFate {
    /// claude 退了、它的 tmux 会话还在 ⇒ 接得回去。
    Reconnectable,
    /// 进程没了、容器也没了（或被顶替了）⇒ 只能 resume。
    Ended,
}

/// `session_removed` 的 `cause`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub enum RemovalCause {
    /// 真的没了：pidfile 被删 / 进程退出 / 原地翻成非交互 kind。**默认值** —— 缺字段就是它。
    #[default]
    Gone,
    /// 同一个 pidfile 原地换了 sid（`/branch`、`/clear`）：旧 sid **不是死了，是被顶替了**（直接归档，不再去查 tmux 快照）。
    Superseded,
}

impl RemovalCause {
    /// `Gone` 不上线。
    fn is_gone(&self) -> bool {
        matches!(self, RemovalCause::Gone)
    }
}

/// `hello.unavailable` 的一项 —— **这条命令我接得下，但在这台机器上做不到，以及为什么**
///〔`K-P4` 09-04，用户逐字「事前协商是要的」〕。
///
/// # 它为什么是一张**负向**表（这一格是本结构最贵的判断，不是口味）
///
/// 既有的两条面（`emits` · `commands`）都是**正向**清单。正向在这里**结构上表达不了「我什么都做不到」**：
/// additive 要求空表省略（`skip_serializing_if`），而「省略」必须等于**旧后端的语义**。
/// 于是正向表的空集只有两种读法，两种都坏：
/// ① 空=「一条都做不到」⇒ 每台旧后端都变成「什么都不能干」，功能当场全消失；
/// ② 空=「全都做得到」⇒ 一台**真的什么都做不到**的机器无法把这件事说出口。
/// 负向表没有这个二选一：**空 = 我没有任何「做不到」的把握** ——
/// 这**逐字就是今天的语义**（客户端照发、点了才由命令级 code 兜底），旧后端天然落在这一格。
///
/// # `code` 为什么与**调用时**的错误码同一套取值空间
///
/// 「做不到」这件事今天**已经**有表达手段，只是发生在调用之后：`kill`/`launch` 回 `no_tmux`
/// （`control/kill.rs` · `control/gate.rs` · `control/launch.rs`），`bus-*` 回 `not_installed`。
/// 事前那一句要是自造一套词，客户端就得维护**两张**「这句话怎么翻成人话」的表 ——
/// 而 monitor 侧那张表已经写好了（`backend/control/backend_launch.rs` 等三处逐字「远端未安装 tmux」）。
/// ⇒ 复用同一套 code，**事前与事后是同一句话，只是来得早**。
/// 这一条由 `main_fourth_face_tests.rs::the_declared_code_is_one_the_registry_already_declares` 钉住：
/// 本表只许说 `inbound::REGISTRY` 里那条命令**自己登记过**的 code。
///
/// # 为什么不带一句 `message`
///
/// 那句人话今天归 monitor（见上）。再发一份等于给同一句话开第二个源头，
/// 而后端这一侧连用户的语言都不知道。**要诊断细节的场合走真调用**，那条路的 message
/// 本来就说得更细（`not_installed_message` 会列出查过哪几个目录）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Unavailable {
    /// 哪条命令 —— 取值空间与 `Hello.commands` **同一套**（`inbound::command_names`）。
    /// ⚠ 它**必须**同时出现在 `commands` 里：本表说的是「接得下但做不到」，
    /// 「根本不接」那一格由不在 `commands` 里表达（客户端 `accepts()` 一个字节都不发）。
    pub command: String,
    /// 为什么做不到 —— **就是真调用那一刻会回的那个命令级 code**（如 `no_tmux`）。
    pub code: String,
}

/// 推送主题（线上 `changed.topic`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    /// 这台的账号清单（账号 manifest 被改写）。清单每号带的「5h 那一格」跟着额度走：那一格变了推的是 [`Topic::Quota`]。
    Accounts,
    /// 这台的配置文件（`~/.cc-monitor/profiles.toml`）。
    Profiles,
    /// 这台的额度账显示得出来的那几格。
    Quota,
    /// 这台某个会话的轮换（`key` ＝ sid）：那一条 · 它用的那份策略 · 事实变了。
    Rotation,
    /// 这台的策略目录 · 新会话用哪份 · 哪个会话用哪份。
    RotationStrategies,
    /// 这台某个 pb 工作区的计划（`key` ＝ 工作区根，`rev` ＝ 新的输出摘要，与手上那一份相同 ⇒ 不用问；
    /// `body` ＝ `{needs}`：这个工作区此刻要你看的数，没认可的、不含 agent 问人那一种，同 `plan-read` 的 `needCount`）。
    Plan,
    /// 这台某个会话的任务清单（`key` ＝ sid）。
    Tasks,
    /// 这台某个会话挂着的待答请求（`key` ＝ sid：挂上 · 撤掉 · 答掉 · 「没人答时自动答」那一格改了；重问 `sessions-needs` 或那个会话的 `history-facts`）。
    Needs,
    /// 一份登录作业自己收了（`key` ＝ 作业号；`body` ＝ `{state: done | failed, said?, detail?}`）。只推给发起它的那条连接。
    Login,
    /// 这台会话外的需手动清单可能变了（账号清单动了 · 一份登录作业收了 · 某个号记「MCP 要登录」的那份文件翻了；重问 `manual-list`）。
    Manual,
    /// 这一端的机器表（`machines.json` 被改写；重问 `machines-read`）。
    Machines,
    /// 这一端连的某台远端此刻的状态（`key` ＝ 那台的名字；`body` ＝ 那台的成品 `MachineState`；丢了重问 `machine-read`）。
    Machine,
    /// 这台某个活会话隐不隐藏变了（`key` ＝ sid；`body` ＝ `{hidden}`：那一格，不隐藏 ⇒ `null`）：
    /// 注解 · 这台偏好 · 会话血缘动了之后判出来的。不重读整份清单，收到就换（丢了重问 `history-list {sid}` 那一行）。
    SessionHidden,
}

/// [`Frame::SessionFileReread`] 的「为什么从头重读」。线上两个字面量。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub enum RereadWhy {
    /// 变短了（比读到过的最长还短）。
    Truncated,
    /// 没变短，但游标之前那一截被原地改写过（末尾指纹对不上）。
    Rewritten,
}

/// 一趟传输怎么收场的（[`Frame::Transfer`] 的 `end`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TransferEnd {
    /// 传完了。上传那一路带 `sha256`（整份本机文件的摘要，窗口提交 `files-commit-upload` 时原样交回当 `expect`）；下载那一路没有（不上线）。
    Done {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        bytes: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        sha256: Option<String>,
    },
    /// 失败（那一句 ＋ 复制详情）。上传那一路的暂存件**留着**给续传；下载那一路的 `.part` 删了。
    /// `code`：调用方要按它换路的那几形（今天只有 `sftp_home_mismatch`）；缺席 ＝ 一般的失败。`detail` 缺席 ＝ 没写。
    Failed {
        why: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        detail: Option<String>,
    },
    /// 撤了（`transfer-stop` / 本机流断了）。上传那一路的暂存件已删；下载那一路的 `.part` 留着。
    Cancelled,
}

/// 一条终端订阅为什么停了（[`Frame::TerminalFollowEnd`] 的 `why`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub enum FollowEnd {
    /// 那个终端没了（窗格 / tmux 会话关了）。
    Gone,
    /// 看着它的那条路断了（tmux 控制模式客户端退了、抓屏失败），终端也许还在。
    Lost,
    /// 那一屏大过一帧的上限。
    TooBig,
}

/// 被挤出运行表的一个已收场子运行（[`Frame::SessionRuns`] 的 `ended` 一项）：是哪个 · 派出它的那次工具调用 · 终态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct RunEnded {
    pub run: String,
    pub tool: String,
    pub state: RunState,
}

/// 一个子运行此刻的样子（[`Frame::SessionRuns`] 的一项）。三个时刻是自 1970 起的毫秒（记录自己写着的时刻；记录里没写 ⇒ 读到它的时刻）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct RunInfo {
    pub run: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub kind: Option<String>,
    /// 派出它的那次工具调用（父侧 id）；还没对上 ⇒ 不上线。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub tool: Option<String>,
    /// 派出它的那个子运行；主运行派的（或还没对上派出它的那次调用）⇒ 不上线。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub parent: Option<String>,
    pub state: RunState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub last: Option<RunDid>,
    /// 在等哪个工具的结果（它最近一条记录是一次还没拿到结果的工具调用、且还在跑）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub waiting: Option<String>,
    /// 开始：派出它的那条记录（没见到 ⇒ 它自己最早的一条）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub started_ms: Option<u64>,
    /// `started_ms` 在看的那一台钟上的钟面 `HH:MM`（推出去那一下按这条流的时区写，[`Frame::stamp`]；界面照抄、不换算）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub started_text: Option<String>,
    /// 最近动静：它自己最近一条记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub active_ms: Option<u64>,
    /// 收场：说它收场的那一条（还没收场 / 状态不明 ⇒ 不上线）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub ended_ms: Option<u64>,
    /// 为什么是这个结局（在跑 ⇒ 不上线）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub why: Option<RunWhy>,
    /// 失败收场时的报错原话（说得出才有；至多 `observe::runs::ERROR_CHARS` 个字）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub error: Option<String>,
    /// 它调了几次工具（它自己的记录里数的；零 ⇒ 不上线）。
    #[serde(default, skip_serializing_if = "is_zero")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<u32>"))]
    pub calls: u32,
    /// 派出那一方没等它、接着做自己的事（后台派出；否则不上线）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub background: bool,
}

/// 子运行的五态。收场的三态以派出那一方说的为准（子记录自己写出终局也算，先到先算）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    #[default]
    Running,
    Done,
    Failed,
    /// 被叫停。
    Stopped,
    /// 没有任何收场信号、子记录又久未再写（`observe::runs::STALE_AFTER`）：不当它在跑。
    Unknown,
}

/// 一个子运行为什么是这个结局（[`RunInfo::why`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "lowercase")]
pub enum RunWhy {
    /// 派出它的那一方说的（拿到了结果 / 收到了收场通知）。
    Reported,
    /// 它自己的记录写出了终局。
    Own,
    /// 没有收场信号、它的记录久未再写（状态不明）。
    Quiet,
    /// 派出它的会话退了，再也等不到收场信号（状态不明）。
    Orphaned,
}

/// 一个运行最近做的那件事（通用的值域；某一家的哪种记录算哪一件由那一家判）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum RunDid {
    /// 在写回复。
    Say,
    /// 在想。
    Think,
    /// 调用了一个工具。
    Tool { name: String },
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/frame_tests.rs"]
mod tests;
