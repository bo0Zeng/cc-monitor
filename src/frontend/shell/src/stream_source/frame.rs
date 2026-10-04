//! 后端→monitor 的帧：类型与解析。

use super::*;
use crate::copy_table::copy_text;
use crate::session_book::Fate;

/// 一行会话记录的成品 ＋ 它在那份文件里的行号（`seq`）—— 进 [`flush_lines`] 之前的形状。
///
/// 它原先住 monitor 自己的 jsonl watcher（`watcher.rs`，已删）。
/// 本机那条流改走后端的 `line` 帧之后，**所有**行都从后端的帧来（远端流 · 本机流 · 旁路快照），
/// 造它的只剩本模块 ⇒ 搬到这里。`seq` 是后端给的行号（`--tail-only` 下与快照同处一个行号空间），
/// 前端按 `(session_id, seq)` 去重、按 `seq` 排序（`INVARIANTS §5` / `§9`）。
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

/// backend→client 的一帧（解析后的 inbound 表示）。
///
/// 对应 `src/backend::wire::Frame`（外部 `kind` tag，snake_case）。这里**不**
/// 直接 import 那个 crate（它刻意不在 workspace 里、不被 root Cargo 引用，见其 README），
/// 而是用 schema-agnostic 的方式（serde_json::Value + 读 `kind`）解析，只取 Phase-0 需要的
/// 字段。这样：协议演进（backend 加 `build_id` / 加新 kind）不会 break 解析 —— 未知 kind /
/// 多余字段一律忽略（见 `parse_frame`）。
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

/// `hello.homes` 的一项 —— **某个 agent 在那台远端机器上的 home 目录**。
///
/// 与后端侧 `wire::AgentHome` 对称（这一侧刻意不依赖那个 crate，照 `InboundFrame`
/// 一贯的做法自己解析 JSON）。字段名里没有任何一个 agent 的名字：agent 维度住在
/// `agent_kind` 这个**值**里 —— backend 那边 `D3` 逐字要求的形状。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentHome {
    pub agent_kind: String,
    pub path: String,
}

/// 从 hello 帧里解析出「Claude 的 home 目录」—— **优先 `homes`、回退 `claude_dir`**。
///
/// 这是 `S4` additive 迁移在消费侧的那一半。两个字段的关系：
/// - `homes`（新，通用）：`[{agent_kind, path}]`，agent 维度在**值**里；
/// - `claude_dir`（旧，冻结兼容）：字段名里带 agent 名，backend 侧登记在
///   `agent_boundary_guard::FROZEN_COMPAT`，**解锁条件是 monitor 与 aterm 都改读 `homes`**。
///
/// ⇒ 本函数就是 monitor 那半的兑现：从今往后 monitor **不再依赖** `claude_dir` 的存在语义，
/// 它只是回退路径。`claude_dir` 的删除因此只卡在仓外 aterm 上，我们这边不欠。
///
/// ⚠ 今天的 backend `homes` 恒空（DG1 未接线）⇒ 实际走的一直是回退分支。
/// 这不是"没接上"，是 additive 迁移的正常中间态：先让消费侧认得新字段，
/// 生产侧（`S5`）再开始发 —— 反过来做会有一段时间新字段被丢掉。
pub(crate) fn claude_home_from_hello<'a>(homes: &'a [AgentHome], claude_dir: &'a str) -> &'a str {
    homes
        .iter()
        .find(|h| h.agent_kind == "claude")
        .map(|h| h.path.as_str())
        .unwrap_or(claude_dir)
}

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
        /// backend-split `S4`（additive）：远端各 agent 的 home 目录表。
        /// 旧后端（含今天所有已部署的）无此字段 ⇒ 空表 ⇒ 回退 `claude_dir`。
        /// 非数组 / 元素缺字段一律滤掉，绝不 panic（同 `capabilities` 口径）。
        homes: Vec<AgentHome>,
        /// F66（#58③）：backend 声明的能力 token 集。旧后端无此字段 → 空集
        /// （保守：按最小能力集待它，不发流模式 flag）。monitor 按此决定发
        /// `--with-bg`/`--tail-only`，不再靠 build_id 精确匹配。
        capabilities: Vec<String>,
        /// U6b-2 / U8a-2a：backend 声明**接受哪些入方向命令**（后端 `inbound::command_names`，从命令表派生）。
        /// `capabilities` 说的是「我认识哪些流 flag」（出方向），这一条说的是入方向 ——
        /// 两者正交。旧后端无此字段 ⇒ 空集 ⇒ monitor 一条入方向命令都不发。
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
        /// 后端的 `byte_offset`（这一行末尾含 `\n` 的累计字节）；老后端不带 ⇒ `None`。
        end: Option<u64>,
        /// 对账键（后端 `rid`，原样转交）。
        rid: Option<String>,
    },
    /// 远端新出现一个 session 文件。Batch7-F24：p1e backend 附带 pidfile 元信息
    /// （additive）；旧后端缺字段 → None（保守视为交互）。
    SessionAdded {
        sid: String,
        session_kind: Option<String>,
        /// E73（additive）：attach 进去对人有没有意义。缺席 = true（存量零迁移）。
        /// 语义与来源见 `src/backend/stream/wire.rs` 的同名字段 + `src/doc/IPC-PROTOCOL.md` §9.3。
        attachable: Option<bool>,
        /// pidfile 记的起会话目录（认「我刚起的那条」用）。
        cwd: Option<String>,
        /// 会话的项目目录（那台后端读记录开头给的；tab 标题用它）。老后端不带 ⇒ `None`。
        project_dir: Option<String>,
        name: Option<String>,
        /// Batch8-F25：远端 jsonl 绝对路径（p1f backend 起有值）——旁路快照用。
        path: Option<String>,
        /// Batch8 D-I2：backend prime 时的完整行数 L（快照完整性校验）。
        lines: Option<u64>,
        /// Batch9-F27：宣告时的初始 status/waitingFor（连接建立灯就对）。
        status: Option<String>,
        waiting_for: Option<String>,
        /// 〔additive〕这条活会话住在什么容器里（`tmux` / `none`）。
        /// 缺席 / 不认识的取值 ⇒ `None` = 不知道（**不是**「不在 tmux 里」）。
        /// 进 `session_book` 的活会话成品（本机那条流同一个口）。
        container: Option<crate::session_book::Container>,
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
    /// Batch9-F27：会话 status 变化（p1g backend；远端红绿灯）。
    SessionStatus {
        sid: String,
        status: Option<String>,
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
    /// issue #32：远端后端发送通道拥塞、丢了 `dropped` 帧（慢 SSH 管道）。
    /// monitor 收到后经 SS-F remote-health 通道提示用户。
    ///
    /// ★ `lost` / `lost_truncated`〔audit-0805 F21，additive〕：那批丢帧里**不可恢复**的
    /// 那些的身份。⚠ **它们的有无决定了要对用户说哪句话** —— 丢内容帧「重开会话可看完整
    /// 历史」是真的；丢状态增量帧**不是**（它是一次差分的结果、别处不存在）。
    /// 旧后端不发这两个字段 ⇒ 空集 / false，行为退回从前。
    Overflow {
        dropped: u64,
        lost: Vec<LostFrameInfo>,
        lost_truncated: bool,
    },
    // 这里原是后端 tmux 观测两帧（整份快照 · 差分出的正向死亡）：收割与「可重连」进了那台后端的会话账本、
    //   后端也不再发它们 ⇒ 删。老后端发来 ⇒ 落未知 kind（照常 warn 后跳过）。
    /// U6b-1 / U8a-2a：**入方向命令的应答**。`id` 是 monitor 自己生成的不透明串，
    /// backend 原样回显。由 `inbound_client` 按 `id` 路由回请求方。
    Reply {
        id: String,
        ok: bool,
        code: Option<String>,
        message: Option<String>,
        data: Option<serde_json::Value>,
    },
    /// U6b-1 / U8a-2a：某条在跑的入方向命令**已被取消**。
    Cancelled { id: String },
    /// 那台机器上的账号清单变了（后端 `wire::Frame::AccountsChanged`，无载荷）。
    AccountsChanged,
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
}

/// 拥塞提示的**措辞**：有没有不可恢复的丢失，说法完全不同。
///
/// # 为什么要一个纯函数
///
/// 这句话是**用户唯一能看到的东西**，而它此前是错的（对状态增量帧说「重开会话可看完整
/// 历史」）。抽成纯函数是为了让它**可判据** —— 消费点那一整块要真 `AppHandle`、
/// 测不了；措辞对不对却恰恰是本件的正题。
///
/// 三档（定框 **E4**：静默失败要给身份、且要抬到调用方能判定的那一层）：
/// - **只丢了内容帧**（`lost` 空）：老说法成立，行还在远端 jsonl 里。
///   ⚠ 旧后端（`p1x` 之前）不发 `lost` ⇒ 也落这一档，**行为与从前逐字相同**。
/// - **有不可恢复的丢失**：点名主体，并**明说重开会话补不回来**。
/// - **身份表还被截断了**：再加一句「清单不全」，暗示理性做法是整体重取。
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

/// 把后端发来的一行（已去掉行尾 `\n`）解析成 [`InboundFrame`]。
///
/// **纯函数 + 绝不 panic**：
/// - 非 JSON / JSON 不是 object → `None`
/// - 缺 `kind` 或 `kind` 不是字符串 → `None`
/// - 已知 kind 但必需字段缺失 / 类型不对 → `None`（坏帧当 garbage 跳过）
/// - **未知 kind**（如未来新增的 `{"kind":"future_thing"}`）→ `None`（向前兼容，调用方 warn+skip）
/// - **多余 / 未知字段**（如 hello 里的 `build_id`）→ 忽略，不影响解析
///
/// 调用方（[`run`]）对 `None` 一律 `tracing::warn!` 后 continue，永不中断流。
pub fn parse_frame(line: &str) -> Option<InboundFrame> {
    // 内容帧是最热的那一种：按类型直解，成品（`message`）以原文收下、不建 `Value`（monitor 不读它的字段）。
    if line.starts_with(r#"{"kind":"line","#) {
        #[derive(serde::Deserialize)]
        struct LineFrame {
            session_id: String,
            path: String,
            seq: u64,
            #[serde(default)]
            message: Option<Box<serde_json::value::RawValue>>,
            #[serde(default)]
            cwd: Option<String>,
            #[serde(default)]
            byte_offset: Option<u64>,
            #[serde(default)]
            rid: Option<String>,
        }
        if let Ok(f) = serde_json::from_str::<LineFrame>(line) {
            return Some(InboundFrame::Line {
                session_id: f.session_id,
                path: f.path,
                seq: f.seq,
                message: f.message.map(crate::ui_contract::RecordBody),
                cwd: f.cwd,
                end: f.byte_offset,
                rid: f.rid,
            });
        }
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let obj = value.as_object()?;
    let kind = obj.get("kind")?.as_str()?;
    match kind {
        "hello" => {
            let v = obj.get("v")?.as_u64()?;
            // #33：捕获 build_id 做版本协商（既有后端一直在发，故按必需字段解析）。
            let build_id = obj.get("build_id")?.as_str()?.to_string();
            let host_arch = obj.get("host_arch")?.as_str()?.to_string();
            let claude_dir = obj.get("claude_dir")?.as_str()?.to_string();
            // backend-split `S4`（additive）：`homes` = 远端各 agent 的 home 目录表
            // （`[{agent_kind, path}]`）。旧 backend **全部**没有这个字段 ⇒ 空表 ⇒
            // 消费侧回退 `claude_dir`（见 `claude_home_from_hello`）。
            // ⚠ 逐项要求 `agent_kind` 与 `path` 都是字符串，坏的那一项**单独丢掉**、
            //   不是丢整张表 —— 同 `capabilities` 的「非数组 / 元素类型不对一律滤掉」口径。
            //   整帧 `None` 是留给「已知 kind 但必需字段缺失」的，`homes` 不是必需字段。
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
            // F66（#58③，additive）：旧后端无 `capabilities` 字段 → 空集（保守缺省，
            // 同 §27「status 缺失恒未知」族）。非数组 / 元素非字符串一律滤掉，绝不 panic。
            let capabilities = obj
                .get("capabilities")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            // U8a-2a：入方向能力协商（additive，同上口径）。旧后端无此字段 ⇒ 空集
            // ⇒ `inbound_client` 一条入方向命令都不发。
            let commands = obj
                .get("commands")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            // 能力事实的另两格（additive，同上口径：坏项逐项丢，不丢整帧）。读法住 `Offer::facts_of`（`resync` 应答同形）。
            let (unavailable, uncancellable) = crate::chan::wire::Offer::facts_of(obj);
            Some(InboundFrame::Hello {
                v,
                build_id,
                host_arch,
                claude_dir,
                homes,
                capabilities,
                commands,
                unavailable,
                uncancellable,
            })
        }
        // 一般走不到这里（`line` 帧在上面按类型直解，成品不经 `Value`）；形状不是那一形时落到这里再认一次。
        "line" => {
            let session_id = obj.get("session_id")?.as_str()?.to_string();
            let path = obj.get("path")?.as_str()?.to_string();
            let seq = obj.get("seq")?.as_u64()?;
            let message = match obj.get("message") {
                None | Some(serde_json::Value::Null) => None,
                Some(m) => Some(crate::ui_contract::RecordBody::from_json(m.to_string())?),
            };
            let cwd = obj.get("cwd").and_then(|v| v.as_str()).map(str::to_string);
            let end = obj.get("byte_offset").and_then(serde_json::Value::as_u64);
            let rid = obj.get("rid").and_then(|v| v.as_str()).map(str::to_string);
            Some(InboundFrame::Line {
                session_id,
                path,
                seq,
                message,
                cwd,
                end,
                rid,
            })
        }
        "session_added" => {
            let sid = obj.get("sid")?.as_str()?.to_string();
            // Batch7-F24 附加字段（旧后端缺失 → None）
            let opt = |k: &str| obj.get(k).and_then(|v| v.as_str()).map(str::to_string);
            Some(InboundFrame::SessionAdded {
                sid,
                session_kind: opt("session_kind"),
                // E73：**只认真正的布尔**。字符串 "false" 之类当没写（缺席 = true）——
                // 宁可少一次门控，也不要把一个拼错的值读成「不可 attach」而把功能吞掉。
                attachable: obj.get("attachable").and_then(|x| x.as_bool()),
                cwd: opt("cwd"),
                project_dir: opt("project_dir"),
                name: opt("name"),
                path: opt("path"),
                lines: obj.get("lines").and_then(|v| v.as_u64()),
                status: opt("status"),
                waiting_for: opt("waiting_for"),
                // 两个字面量之外一律当不知道（`Container::from_wire`）。
                container: opt("container")
                    .as_deref()
                    .and_then(crate::session_book::Container::from_wire),
                // 只认装得进 u32 的非负整数；别的一律当没带。
                pid: obj
                    .get("pid")
                    .and_then(|v| v.as_u64())
                    .and_then(|n| u32::try_from(n).ok()),
            })
        }
        // additive 新帧，无载荷。旧后端不发 ⇒ 这条分支永不命中，固定的 tab 停在「说不清」。
        "sessions_replayed" => Some(InboundFrame::SessionsReplayed),
        // additive 新帧。`why` 认不出 ⇒ 整帧当坏帧跳过（不猜成哪一种）。
        "session_file_gone" => Some(InboundFrame::SessionFileNotice {
            sid: obj.get("session_id")?.as_str()?.to_string(),
            path: obj.get("path")?.as_str()?.to_string(),
            change: FileChange::Gone,
        }),
        "session_file_reread" => Some(InboundFrame::SessionFileNotice {
            sid: obj.get("session_id")?.as_str()?.to_string(),
            path: obj.get("path")?.as_str()?.to_string(),
            change: FileChange::reread_from_wire(obj.get("why")?.as_str()?)?,
        }),
        "session_status" => {
            let sid = obj.get("sid")?.as_str()?.to_string();
            let opt = |k: &str| obj.get(k).and_then(|v| v.as_str()).map(str::to_string);
            Some(InboundFrame::SessionStatus {
                sid,
                status: opt("status"),
                waiting_for: opt("waiting_for"),
            })
        }
        "session_runs" => Some(InboundFrame::SessionRuns {
            sid: obj.get("sid")?.as_str()?.to_string(),
            runs: crate::ui_contract::RecordBody::from_json(
                obj.get("runs").filter(|r| r.is_array())?.to_string(),
            )?,
            ended: crate::ui_contract::RecordBody::from_json(
                obj.get("ended").filter(|r| r.is_array())?.to_string(),
            )?,
        }),
        "session_removed" => Some(InboundFrame::SessionRemoved {
            sid: obj.get("sid")?.as_str()?.to_string(),
        }),
        // 会话账本的成品。`state` 认不出 ⇒ 整帧当坏帧跳过（不猜成哪一种）。
        "session_state" => Some(InboundFrame::SessionState {
            sid: obj.get("sid")?.as_str()?.to_string(),
            state: Fate::from_wire(obj.get("state")?.as_str()?)?,
        }),
        "overflow" => {
            // issue #32：dropped 必需且为数字；缺/错则当坏帧跳过（不 panic）。
            let dropped = obj.get("dropped")?.as_u64()?;
            // additive：**缺字段必须仍能解析** —— 旧后端还在跑，
            // 把它们当必需会让整帧变成坏帧、连 `dropped` 都丢掉，比不认识更糟。
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
            let lost_truncated = obj
                .get("lost_truncated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Some(InboundFrame::Overflow {
                dropped,
                lost,
                lost_truncated,
            })
        }
        // U8a-2a：入方向应答，**真消费** —— 由 `inbound_client` 按 `id` 路由回请求方。
        // `id`/`ok` 必需（缺则坏帧跳过，同其余帧的口径）；`code`/`message`/`data` 可选。
        "reply" => {
            let id = obj.get("id")?.as_str()?.to_string();
            let ok = obj.get("ok")?.as_bool()?;
            let opt = |k: &str| obj.get(k).and_then(|v| v.as_str()).map(str::to_string);
            Some(InboundFrame::Reply {
                id,
                ok,
                code: opt("code"),
                message: opt("message"),
                data: obj.get("data").cloned(),
            })
        }
        "cancelled" => {
            let id = obj.get("id")?.as_str()?.to_string();
            Some(InboundFrame::Cancelled { id })
        }

        // 账号清单变了。
        "accounts_changed" => Some(InboundFrame::AccountsChanged),
        // 某个会话的任务清单变了（sid 缺 / 不是串 ⇒ 坏帧）。
        "tasks_changed" => Some(InboundFrame::TasksChanged {
            sid: obj.get("sid")?.as_str()?.to_string(),
        }),

        // 链路两帧。`data` 解不开 ⇒ 整帧 `None`（坏帧，调用方 warn）—— 不交一段猜出来的字节。
        "link_data" => {
            let link = obj.get("link")?.as_str()?.to_string();
            let data = crate::link_mux::b64_decode(obj.get("data")?.as_str()?).ok()?;
            Some(InboundFrame::LinkData { link, data })
        }
        "link_end" => {
            let link = obj.get("link")?.as_str()?.to_string();
            let error = obj
                .get("error")
                .and_then(|e| e.as_str())
                .map(str::to_string);
            Some(InboundFrame::LinkEnd { link, error })
        }

        // 传输进度 / 终局。`end` 在 ⇒ 必须是后端那三形之一，认不出 ⇒ 整帧 `None`（不猜一个结局）。
        "transfer" => {
            let id = obj.get("id")?.as_str()?.to_string();
            let got = obj.get("got")?.as_u64()?;
            let total = obj.get("total")?.as_u64()?;
            let end = match obj.get("end") {
                None => None,
                Some(e) => Some(transfer_end(e)?),
            };
            Some(InboundFrame::Transfer {
                id,
                got,
                total,
                end,
            })
        }

        // 测试连接的一格进度：票 ＋ 原样那一格（必须是对象；内容由界面严格收）。
        "probe" => {
            let ticket = obj.get("ticket")?.as_str()?.to_string();
            let cell = obj.get("cell").filter(|c| c.is_object())?.to_string();
            Some(InboundFrame::Probe { ticket, cell })
        }

        // 一件归一事件（后端已按上游协议折过、归过位）。`ev` 与 `end` 恰有一个：先认 `ev`（必须是对象，原样转交），
        //   没有就必须是认得的 `end`。`run` 缺 ＝ 主运行。
        "tap" => {
            let stream = obj.get("stream")?.as_str()?.to_string();
            let run = match obj.get("run") {
                None => None,
                Some(r) => Some(r.as_str()?.to_string()),
            };
            let resp = obj.get("resp")?.as_u64()?;
            let n = obj.get("n")?.as_u64()?;
            let body = match obj.get("ev") {
                Some(e) => {
                    crate::session_tap::TapBody::Ev(crate::ui_contract::RecordBody::from_json(
                        e.as_object().map(|_| e.to_string())?,
                    )?)
                }
                None => crate::session_tap::TapBody::End(crate::session_tap::TapEnd::from_wire(
                    obj.get("end")?.as_str()?,
                )?),
            };
            Some(InboundFrame::Tap(crate::session_tap::Tap {
                stream,
                run,
                resp,
                n,
                body,
            }))
        }

        // ── `turn_end` **认识但刻意不消费**（U7-1）。──────────────────────────
        //
        // 「认识」与「消费」是两件事。落进 `_ => None` 的后果不是「忽略」，是
        // **每帧刷一条 `skipping unparseable/unknown frame` 的 warn** —— 那既是噪声，
        // 也让真正的坏帧淹没在里面。
        //
        // backend 的 `EMITS` 里**登记了、也真在发**（`watcher.rs` 每轮对话一帧），
        // 而 monitor 此前**根本不认它** —— 实测是 `EMITS` 八个 kind 里唯一一个漏的。
        // monitor 不需要它：轮次边界由本地 `parse_line` 管线从 `line` 帧的原始 jsonl 自己推。
        // 它是发给 **aterm** 的（aterm 按 `emits` 门控消费）。
        "turn_end" => None,

        // `quota_changed` 也是认识但不消费：界面怎么画额度还没定，今天要额度就发 `quota-read` 读整份。
        "quota_changed" => None,

        // 未知 kind：向前兼容，跳过（调用方 warn）。绝不 panic。
        _ => None,
    }
}

/// 本 monitor **认识**的全部帧 kind（消费 + 刻意不消费）。
///
/// 与 `parse_frame` 的 match 臂是同一份事实 —— 由
/// `every_kind_the_backend_emits_is_known_to_the_monitor` 与
/// `known_kinds_matches_parse_frame` 两条钉住。
#[cfg(test)]
const KNOWN_FRAME_KINDS: &[&str] = &[
    "accounts_changed",
    "tasks_changed",
    "cancelled",
    "hello",
    "line",
    "link_data",
    "link_end",
    "overflow",
    "probe",
    "quota_changed",
    "reply",
    "session_added",
    "session_file_gone",
    "session_file_reread",
    "session_removed",
    "session_runs",
    "session_state",
    "session_status",
    "sessions_replayed",
    "tap",
    "transfer",
    "turn_end",
];

/// `transfer` 帧的 `end`：后端 `wire::TransferEnd` 那三形之一；认不出 ⇒ `None`（调用方整帧丢）。
/// 抽出来住 `parse_frame` 外面：那张 match 的臂是帧 kind 的名单（`known_kinds_matches_parse_frame` 按臂抠），
/// 结局的三个名字不该混进去。
fn transfer_end(e: &serde_json::Value) -> Option<crate::sftp_pool::End> {
    Some(match e.get("state")?.as_str()? {
        "done" => crate::sftp_pool::End::Done {
            bytes: e.get("bytes")?.as_u64()?,
            // 可缺席（下载那一路没有）；在就原样带着（窗口提交时交回，形状由后端那一关判）。
            sha256: e.get("sha256").and_then(|v| v.as_str()).map(str::to_string),
        },
        // 带码的那一形（今天只有 `sftp_home_mismatch`）单列一形，窗口按码换路。
        "failed" => match e.get("code").and_then(|v| v.as_str()) {
            Some(code) => crate::sftp_pool::End::FailedCoded {
                why: e.get("why")?.as_str()?.to_string(),
                code: code.to_string(),
            },
            None => crate::sftp_pool::End::Failed(e.get("why")?.as_str()?.to_string()),
        },
        "cancelled" => crate::sftp_pool::End::Cancelled,
        _ => return None,
    })
}

/// U7-1：**backend 的产出面 ↔ monitor 的消费面**对拍。
///
/// # 这条抓到的第一个真缺陷
///
/// backend 的 `EMITS` 是一份**承诺**（那个常量的注释逐条写着「登记 = 承诺真发，已接线」），
/// monitor 的 `parse_frame` 是**实际消费面**。两者此前**没有任何对拍** ——
/// 实测 `turn_end` 是后端承诺发、也真在发、而 monitor 压根不认的那一个：
/// 每轮对话刷一条 `skipping unparseable/unknown frame` 的 warn。
///
/// 「读面合流」的第一步不是搬代码，是**让消费面追上产出面并钉住**。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/emits_parity.rs"]
mod emits_parity;

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/parse_frame_tests.rs"]
mod parse_frame_tests;
