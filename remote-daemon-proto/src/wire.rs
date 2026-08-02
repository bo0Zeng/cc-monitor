//! Phase-0 daemon→client wire types.
//!
//! Wire contract: exactly one UTF-8 JSON object per line, terminated by `\n`,
//! with no bare `\n`/`\r` inside the object. `serde_json` compact output
//! escapes any inner newline as `\n` (two chars), so the only literal newline
//! on the wire is the trailing terminator appended by [`to_line`].

use serde::Serialize;
use std::collections::HashMap;

/// A single daemon→client frame.
///
/// Serializes with an external `kind` tag, e.g.
/// `{"kind":"hello","v":1,...}` or `{"kind":"session_added","sid":"..."}`.
/// [`Frame::SessionRemoved`] 的原因。**双写点**：字面量 `"superseded"` 与 monitor
/// `src-tauri/src/ssh_source.rs` 的解析处逐字一致，由 monitor 侧
/// `removal_cause_wire_literal_stays_in_sync` 钉住（同 `TMUX_LS_FMT` 的纪律）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovalCause {
    /// 真的没了：pidfile 被删 / 进程退出 / 原地翻成非交互 kind。
    /// **默认值** —— 缺字段就是它，保证旧 daemon×新 monitor 与今天行为一致。
    #[default]
    Gone,
    /// 同一个 pidfile 原地换了 sid（`/branch`、`/clear`）：旧 sid **不是死了，是被顶替了**。
    /// monitor 收到它必须直接归档，**不要**再去查 tmux 快照（那份快照对这个场景恒错）。
    Superseded,
}

impl RemovalCause {
    /// 给 `skip_serializing_if` 用：`Gone` 不上线，保持帧最小 + additive。
    fn is_gone(&self) -> bool {
        matches!(self, RemovalCause::Gone)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Frame {
    /// Handshake sent once when a client connects.
    Hello {
        v: u32,
        build_id: String,
        host_arch: String,
        claude_dir: String,
        /// DG3（#2D，additive）：对称 `claude_dir`——Codex 记录根（`<codex_dir>/sessions`）。
        /// skip_if_none：Codex 未启用 / 旧 daemon 省略（旧 client 忽略）。消费侧取路径用。
        #[serde(skip_serializing_if = "Option::is_none")]
        codex_dir: Option<String>,
        /// DG3（#2D，additive）：本 daemon **服务的 agent kind 集**（如 `["claude","codex"]`）——
        /// 消费侧**显式判支持**（比「codex_dir 存在=支持 codex」推断清晰、可扩展未来第三种 agent）。
        /// skip_if_empty：旧 daemon 省略 = 只 claude（向后兼容）。
        #[serde(skip_serializing_if = "Vec::is_empty")]
        kinds: Vec<String>,
        /// F66（#58③，additive）：本 daemon 声明支持的**能力 token 集**（开放字符串，
        /// 加法式）。monitor 按此声明决定发哪些流模式 flag（`--with-bg`/`--tail-only`），
        /// **不再靠 build_id 精确匹配**——闭合 2026-07-09 那类「身份确认不了就全降级」事故。
        /// 旧 monitor 忽略此字段（additive）；空/缺 = 按最小能力集待它。
        /// **§26 死循环护栏**：只声明本 daemon **会先剥离对应 flag** 的能力（老到不剥离
        /// 未知 flag 的 daemon 也老到不声明该能力，声明 = 自证认识该 flag）。
        #[serde(skip_serializing_if = "Vec::is_empty")]
        capabilities: Vec<String>,
        /// phase②（daemon-08，additive，与 `capabilities` **正交**）：本 daemon **会发射的帧 kind 集**
        /// （snake_case，如 "session_status"/"turn_end"）。aterm 据此**门控消费**（emits 含该 kind →
        /// 期待/依赖该帧；不含 → 不依赖、回退 β/watchdog）。**区别于 `capabilities`**（后者=流 flag-strip
        /// 能力、受 §26 死循环护栏 + `every_capability_token_is_strippable` 强制每 token 有可剥离 flag）——
        /// `emits` 是纯发射声明、无对应 flag、**不受 §26**。空/缺 → 省略（旧 client 忽略，additive）。
        #[serde(skip_serializing_if = "Vec::is_empty")]
        emits: Vec<String>,
    },
    /// One raw JSONL line tailed from a session file.
    Line {
        session_id: String,
        path: String,
        seq: u64,
        raw: String,
        /// daemon-01（gap#2，additive 不 bump PROTO_VERSION）：本行末尾（含 `\n`）在文件中的**累计原始字节 offset**——
        /// 语义**逐字节对齐 aterm `LineFramer.endOffset`**：计 CRLF 的 `\r`、含 `\n`、残行不计；resume N ⇒
        /// `tail -c +(N+1)`。给 offset 续拉/截断检测（`seq` 是 per-stream 序数、非 resume 键）。
        /// 注：`Frame` 仅 derive `Serialize`，故此 `#[serde(default)]` 在**本 crate 装饰性**。
        ///
        /// ⚠ **别把它当成向后兼容的落点**。这里曾写着「向后兼容实现在 cc-monitor 反序列化侧」——
        /// **那是假的**：`ssh_source.rs` 的 `"line"` 分支只取 `session_id/path/seq/raw`，
        /// 全仓 `byte_offset` / `byteOffset` 零命中，**cc-monitor 根本不读这个字段**。
        /// 今天唯一的消费者是仓外 aterm（它自己决定缺字段怎么办）。
        /// U6a 审计抓到：一条把不存在的实现点写成契约锚的注释，下一个人照它去 monitor 找会扑空。
        #[serde(default)]
        byte_offset: u64,
    },
    /// A new session file appeared.
    ///
    /// Batch7-F24（additive，向后兼容）：附带 pidfile 元信息——`session_kind`
    /// （"interactive"/"bg"；字段名避开 enum tag `kind`）、`cwd`、`name`。
    /// None 时不上线（旧行为字节不变）；旧 monitor 忽略未知字段。
    SessionAdded {
        sid: String,
        /// DG3（#2D，additive）：会话属哪 agent kind——`"codex"`（Codex 会话）。Claude 会话**省略**
        /// （skip_if_none）→ 消费侧缺=claude（向后兼容、旧 daemon 无此字段）。
        #[serde(skip_serializing_if = "Option::is_none")]
        agent_kind: Option<String>,
        /// DG3（#2D，additive）：判活置信度——`"heuristic"`（Codex 无 pidfile、mtime/proc 启发）。
        /// Claude（pidfile 权威）**省略**（skip_if_none）→ 消费侧缺=authoritative（向后兼容）。
        #[serde(skip_serializing_if = "Option::is_none")]
        liveness_confidence: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_kind: Option<String>,
        /// **E73（additive）：attach 进去对人有没有意义。**
        ///
        /// `session_kind` 今天把两件事压在一个轴上：①「该不该在 UI 出现」②「是不是一个人
        /// 坐在终端里跟它对话」。SDK / 脚本驱动的会话正好是「①要②不要」—— 它有 tmux、
        /// `@ccm_sid` 也对，但 `stdin=DEVNULL`，用户敲的字会被脚本吃掉。
        ///
        /// 判据**不是**「有没有终端后端」（它有 tmux，那样问答不出），而是
        /// **「attach 进去对人有没有意义」**。
        ///
        /// `false` = 别给 attach / ↗ / 「杀死空 tmux」这几个动作。
        /// **省略 = `true`**（存量会话与旧 daemon 一律照旧，零迁移）。
        /// 来源：pidfile 的 `attachable` 布尔字段（契约见 `doc/IPC-PROTOCOL.md` §9.3）。
        #[serde(skip_serializing_if = "Option::is_none")]
        attachable: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Batch8-F25（additive）：该会话 jsonl 的远端绝对路径（同 sid 多文件时
        /// 取 mtime 最新者）——monitor 旁路快照（`--read-session`）用。宣告时
        /// 未找到 jsonl（会话刚起还没写首行）→ None：此时无历史可拉，后续行
        /// 天然从 tail 的 seq 0 起全量到达，无需快照。
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        /// Batch8 审计 D-I2（additive）：tail-only 模式下 prime 时的完整行数 L
        /// ——monitor 校验快照拉到的行数 ≥ L 才算成功（不足 = 中途断/daemon
        /// 报错，触发重试；exit status 经 ChannelStream 拿不到，行数校验更强）。
        /// 全量模式 None。
        #[serde(skip_serializing_if = "Option::is_none")]
        lines: Option<u64>,
        /// Batch9-F27（additive）：宣告时的初始 status/waitingFor——连接建立灯就对。
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        waiting_for: Option<String>,
    },
    /// Batch9-F27：会话 status 变化（pidfile modify diff；CC 仅状态转换时重写，
    /// 天然稀疏）。远端红绿灯数据源；旧 monitor 未知 kind 忽略（additive）。
    SessionStatus {
        sid: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        waiting_for: Option<String>,
        /// DG3（#2D，additive）：判活置信度（同 SessionAdded；状态变化时带）。Claude 省→缺=authoritative。
        #[serde(skip_serializing_if = "Option::is_none")]
        liveness_confidence: Option<String>,
    },
    /// A session file went away.
    SessionRemoved {
        sid: String,
        /// **S0（additive）：这个 sid 是「死了」还是「被顶替了」。**
        ///
        /// 为什么必须由 daemon 说：monitor 收到 removed 后要在「灰点（tmux 还在，可以回去
        /// attach）」和「归档」之间二选一，今天它靠**查自己缓存的那份 `tmux ls` 原文里
        /// `@ccm_sid` 还在不在**来猜。`/branch`（同 pidfile 原地换 sid）时这个猜法必错：
        /// 旧 sid 的 tmux 格子还在、只是 `@ccm_sid` 已被 `shared/ccm` 的 poller 换成新 sid，
        /// 而那份缓存**在 P5 删掉 8s ticker 之后再没有任何事件路径会去刷新它**
        /// ⇒ 旧 tab 永久灰点、且按旧 sid 找不到 tmux 会话 ⇒ 杀不掉。
        ///
        /// daemon 这边本来就**分得清**这两件事——它们是两个不同的调用点。把信息发出去，
        /// monitor 就不用猜，也就不受「缓存多旧」和「ccm 的 1s poller 什么时候回填」影响。
        ///
        /// 线上表现：[`RemovalCause::Gone`] **不写字段**（旧 monitor 原样工作，additive）；
        /// 只有 `Superseded` 才出现 `"cause":"superseded"`。
        #[serde(default, skip_serializing_if = "RemovalCause::is_gone")]
        cause: RemovalCause,
    },
    /// phase②（daemon-09）：turn-end 边沿（一轮 assistant 完成）。**方案 C：raw-per-record、daemon
    /// 不 dedup**——每见一条 turn-end 记录（`end_turn && !isApiError && !isSidechain`，见 `turn_detect`）
    /// 发一帧；aterm 侧 **rolling-latest + debounce(1200ms) `baselineByPath`** 塌合同 turn 的多记录、
    /// 首见吞历史不通知、offset 续拉重放 uuid ≤ 基线不通知（**transport-agnostic、与 β 逐字同语义、
    /// gap#6 闭**；daemon 不猜消息边界）。`uuid` = 完成记录**顶层 uuid** = 客户端 dedup 键。
    /// **不带 `byte_offset`**（只 Line 带）——α watcher：Line 推 currentOffset、TurnEnd 喂 rolling
    /// current，结算时 baseline+offset 同段提交。旧 monitor 未知 kind 忽略（additive）。
    TurnEnd { session_id: String, uuid: String },
    /// B2（tmux 对账改 daemon 推送）：daemon 在**远端本地**跑 `tmux ls -F '<TMUX_LS_FMT>'` 的**原始 stdout**
    /// （或哨兵 `NO_TMUX`），周期性推给 monitor——替掉 monitor 每 8s 新建 SSH 跑 tmux ls 的刷屏轮询。
    /// **送 raw、client 解析**（照 `Line` 帧哲学，复用 monitor 现有 `tmux::parse_tmux_ls`，零解析重复）。
    /// **monitor 专属**：aterm DaemonTransport 未知 kind 跳过。旧 monitor 忽略未知 kind（additive）。
    /// **P5（zero-poll-liveness，additive、不 bump `PROTO_VERSION`）**：某个 tmux 会话
    /// **关闭了**——正向死亡帧。
    ///
    /// **它补的是什么**：`TmuxSessions` 是「当前还剩哪些」的快照，monitor 靠**连续两次
    /// 没看见**（`RETIRE_MISS_THRESHOLD >= 2`）才敢 retire —— 那道门是为了容忍观测抖动，
    /// 但也意味着「多个会话里关掉一个」至少要等两个节拍。本帧是 daemon 与上一份快照
    /// 差分出来的**确定结论**，monitor 收到即可直接 retire，**绕过 miss 计数**。
    ///
    /// **快照路径与 miss 计数原样保留**（重同步 / 旧 daemon 降级都靠它）⇒ 同一 sid 可能
    /// 两条路都到，retire 必须幂等（`SidTrack.retired` 本就是幂等设计）。
    ///
    /// **旧 monitor 忽略本帧**：未知 kind 走 `warn` 后跳过（`ssh_source.rs` 那条已有测试
    /// `unknown_kind_returns_none` 钉住）⇒ 行为退回今天的「靠快照 + miss 计数」，不崩。
    TmuxSessionClosed {
        /// 会话名（`tmux ls` 第一列）。**不带 sid**：`#{@ccm_sid}` 在 hook 上下文里取不到
        /// （P0 实测会拿到空 ⇒ 把活会话判灰），而 daemon 这边是**差分算出来的名字**，
        /// sid 由 monitor 用最新快照反查 —— 那份映射它本来就有。
        name: String,
    },
    TmuxSessions {
        raw: String,
        /// P1（zero-poll-liveness，**additive、不 bump `PROTO_VERSION`**）：本次观测的**分类**
        /// ——`"zero_sessions"`（确证零会话）/ `"no_tmux"` / `"unobservable"`（观测失败）。
        ///
        /// **有会话时省略**（`skip_serializing_if`）⇒ 热路径字节与 P1 之前**逐字节一致**。
        ///
        /// **为什么需要它**：P1 之前 `raw` 的空串同时意味着「零会话」和「`tmux ls` 出错被
        /// `|| true` 吞了」，两者不可分 ⇒ monitor 只能一律保守跳过 ⇒ 当被杀的是该 origin
        /// 最后一个 tmux 会话时（server 随之退出、`tmux ls` 回空）对账整段跳过 ⇒ idle 灰灯
        /// **卡到断连 flush 才清**（`doc/INVARIANTS.md` §24bis 预先登记的残留 bug）。
        ///
        /// **旧 monitor 忽略本字段**：它看到空 `raw` ⇒ 空 backend ⇒ 保守跳过 = 今天的行为，
        /// 无回归。新 monitor 读本字段才能安全 retire。取值集与 monitor
        /// `src-tauri/src/tmux.rs` 的 `OBS_*` const 是**双写点**（有守卫钉住）。
        #[serde(skip_serializing_if = "Option::is_none")]
        observation: Option<String>,
    },
    /// The bounded frame channel back-pressured and the reader had to drop
    /// `dropped` frames (a slow/wedged SSH pipe). Emitted once when the channel
    /// drains enough to accept it, so the client can warn the user that live
    /// lines were lost (#32). `dropped` counts frames dropped since the last
    /// overflow signal.
    Overflow { dropped: u64 },
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

/// Per-file monotonic sequence counter.
///
/// Faithful port of the per-file seq semantics in
/// `../src-tauri/src/watcher.rs` (process_file): the counter is keyed by file
/// path, returns the current value then increments by 1 (so the first line of
/// a file gets seq 0, then 1, 2, ...), is monotonic across calls, and is never
/// reset — there is no truncation handling here, the counter only ever climbs
/// for a given path within the process.
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Helper: assert a line is a single JSON object ending in exactly one
    /// `\n` with no other bare newline, then return its parsed `kind`.
    fn parse_kind(line: &str) -> String {
        assert!(line.ends_with('\n'), "line must end in newline: {line:?}");
        let body = line.strip_suffix('\n').unwrap();
        assert!(
            !body.contains('\n'),
            "body must contain no bare newline: {body:?}"
        );
        assert!(
            !body.contains('\r'),
            "body must contain no bare carriage return: {body:?}"
        );
        let v: Value = serde_json::from_str(body).expect("body must be valid JSON");
        v.get("kind")
            .and_then(Value::as_str)
            .expect("kind must be a string")
            .to_string()
    }

    #[test]
    fn each_variant_serializes_to_single_line_with_expected_kind() {
        let cases: Vec<(Frame, &str)> = vec![
            (
                Frame::Hello {
                    v: 1,
                    build_id: "b".into(),
                    host_arch: "x86_64".into(),
                    claude_dir: "/home/u/.claude".into(),
                    codex_dir: None,
                    kinds: vec![],
                    capabilities: vec!["bg".into(), "tail-only".into()],
                    emits: vec!["line".into(), "session_status".into()],
                },
                "hello",
            ),
            (
                Frame::Line {
                    session_id: "s".into(),
                    path: "/p".into(),
                    seq: 0,
                    raw: "{}".into(),
                    byte_offset: 0,
                },
                "line",
            ),
            (
                Frame::SessionAdded {
                    sid: "s".into(),
                    agent_kind: None,
                    liveness_confidence: None,
                    session_kind: None,
                    attachable: None,
                    cwd: None,
                    name: None,
                    path: None,
                    lines: None,
                    status: None,
                    waiting_for: None,
                },
                "session_added",
            ),
            (
                Frame::SessionStatus {
                    sid: "s".into(),
                    status: Some("busy".into()),
                    waiting_for: None,
                    liveness_confidence: None,
                },
                "session_status",
            ),
            (
                Frame::SessionRemoved {
                    sid: "s".into(),
                    cause: RemovalCause::Gone,
                },
                "session_removed",
            ),
            (
                Frame::TurnEnd {
                    session_id: "s".into(),
                    uuid: "u".into(),
                },
                "turn_end",
            ),
            (Frame::Overflow { dropped: 7 }, "overflow"),
            (
                Frame::TmuxSessions {
                    raw: "s1\t/p\tclaude\t1\t2\tsid-a".into(),
                    observation: None,
                },
                "tmux_sessions",
            ),
        ];

        for (frame, expected_kind) in cases {
            let line = to_line(&frame).expect("serialize");
            assert_eq!(parse_kind(&line), expected_kind);
        }
    }

    /// daemon-09：TurnEnd 上线形——`{"kind":"turn_end","session_id","uuid"}`，**无 byte_offset**
    /// （只 Line 带）。uuid = 客户端 dedup 键。
    #[test]
    fn turn_end_frame_serializes_with_session_and_uuid_only() {
        let line = to_line(&Frame::TurnEnd {
            session_id: "sid-9".into(),
            uuid: "u-abc".into(),
        })
        .expect("serialize");
        let v: Value = serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("json");
        assert_eq!(v["kind"], "turn_end");
        assert_eq!(v["session_id"], "sid-9");
        assert_eq!(v["uuid"], "u-abc");
        assert!(v.get("byte_offset").is_none(), "TurnEnd 不带 byte_offset");
    }

    #[test]
    fn overflow_frame_serializes_with_dropped_count() {
        let line = to_line(&Frame::Overflow { dropped: 42 }).expect("serialize");
        let body = line.strip_suffix('\n').unwrap();
        let v: Value = serde_json::from_str(body).expect("valid json");
        assert_eq!(v["kind"], "overflow");
        assert_eq!(v["dropped"], 42);
    }

    /// B2：TmuxSessions 帧带 tmux ls 原文——含**真 TAB**（列分隔）+ **换行**（多会话）→ 必须是**单行**
    /// wire（TAB/换行经 serde 转义、无裸换行），roundtrip 字节还原（monitor `parse_tmux_ls` 靠真 TAB 分列）。
    #[test]
    fn tmux_sessions_frame_ships_raw_as_one_line() {
        let raw = "s1\t/p\tclaude\t1\t2\tsid-a\ns2\t/q\tnode\t0\t1\t";
        let line = to_line(&Frame::TmuxSessions {
            raw: raw.into(),
            observation: None,
        })
        .expect("serialize");
        assert!(line.ends_with('\n'));
        let body = line.strip_suffix('\n').unwrap();
        assert!(!body.contains('\n'), "内嵌换行须被转义、无裸换行: {body:?}");
        let v: Value = serde_json::from_str(body).expect("json");
        assert_eq!(v["kind"], "tmux_sessions");
        assert_eq!(v["raw"], raw); // TAB + 换行逐字还原
    }

    /// F66（#58③）wire 契约：hello 的 `capabilities`。
    /// ① 非空 → 序列化为数组（monitor 据此发 flag）。
    /// ② 空集 → `skip_serializing_if` 省略字段（additive：等价旧 hello，旧 monitor
    ///    收到即空集缺省——向后兼容的关键）。
    fn hello(caps: Vec<String>) -> Frame {
        hello_with(caps, vec![])
    }

    fn hello_with(caps: Vec<String>, emits: Vec<String>) -> Frame {
        Frame::Hello {
            v: 1,
            build_id: "b".into(),
            host_arch: "x86_64".into(),
            claude_dir: "/c".into(),
            codex_dir: None,
            kinds: vec![],
            capabilities: caps,
            emits,
        }
    }

    #[test]
    fn hello_capabilities_serializes_when_present_and_omits_when_empty() {
        // ① 非空 → 数组在线上
        let line = to_line(&hello(vec!["bg".into(), "tail-only".into()])).expect("serialize");
        let v: Value = serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("json");
        assert_eq!(v["kind"], "hello");
        assert_eq!(v["capabilities"], serde_json::json!(["bg", "tail-only"]));
        // ② 空集 → 字段省略（旧 hello 等价形态，旧 monitor 忽略缺失 = 空集缺省）
        let line = to_line(&hello(vec![])).expect("serialize");
        let v: Value = serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("json");
        assert!(
            v.get("capabilities").is_none(),
            "空 capabilities 必须被 skip（additive 向后兼容）"
        );
    }

    /// phase②：Hello.emits 与 capabilities 同 additive 规律、且**独立正交**（一个非空一个空时
    /// 各自序列化/省略互不牵连）。aterm 门控读 emits 判「daemon 发不发某帧」。
    #[test]
    fn hello_emits_serializes_orthogonally_to_capabilities() {
        // emits 非空、capabilities 空 → 只 emits 在线上、capabilities 省略。
        let line = to_line(&hello_with(
            vec![],
            vec!["session_status".into(), "turn_end".into()],
        ))
        .expect("serialize");
        let v: Value = serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("json");
        assert_eq!(
            v["emits"],
            serde_json::json!(["session_status", "turn_end"])
        );
        assert!(
            v.get("capabilities").is_none(),
            "capabilities 空 → 省略（不受 emits 非空牵连）"
        );
        // 空 emits → 字段省略（additive：旧 client 忽略缺失 = 不依赖任何 α 专属帧）。
        let line = to_line(&hello_with(vec!["bg".into()], vec![])).expect("serialize");
        let v: Value = serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("json");
        assert!(v.get("emits").is_none(), "空 emits 必须被 skip（additive）");
    }

    #[test]
    fn seq_counter_is_monotonic_per_path_and_independent_across_paths() {
        let mut c = SeqCounter::new();
        assert_eq!(c.next("/a"), 0);
        assert_eq!(c.next("/a"), 1);
        assert_eq!(c.next("/a"), 2);

        // A different path starts its own sequence at 0.
        assert_eq!(c.next("/b"), 0);
        assert_eq!(c.next("/b"), 1);

        // /a is unaffected and keeps climbing.
        assert_eq!(c.next("/a"), 3);

        // Default impl behaves the same.
        let mut d = SeqCounter::default();
        assert_eq!(d.next("/x"), 0);
        assert_eq!(d.next("/x"), 1);
    }

    #[test]
    fn line_with_quotes_backslashes_and_newline_roundtrips() {
        let raw = "before\"quote\\backslash\nafter-newline";
        let frame = Frame::Line {
            session_id: "sid".into(),
            path: "/some/path.jsonl".into(),
            seq: 42,
            raw: raw.to_string(),
            byte_offset: 99,
        };

        let line = to_line(&frame).expect("serialize");
        assert!(line.ends_with('\n'));

        // Minus the single trailing terminator, there must be NO bare newline:
        // the embedded newline in `raw` is escaped by serde_json as "\n".
        let body = &line[..line.len() - 1];
        assert!(
            !body.contains('\n'),
            "embedded newline must be escaped, got bare newline in: {body:?}"
        );

        // Parses back and the raw field is recovered byte-for-byte.
        let v: Value = serde_json::from_str(body).expect("parse");
        assert_eq!(v["kind"], "line");
        assert_eq!(v["raw"], raw);
        assert_eq!(v["seq"], 42);
    }

    // ─── DG3（#2D）：Codex wire additive 面 · 序列化 parity（aterm 消费侧 fixture 交叉核点）───

    /// **present 形**（Codex 会话 / 支持 Codex 的 daemon）：新字段在线上、snake_case、值域正确。
    #[test]
    fn dg3_codex_fields_serialize_when_present() {
        let hello = to_line(&Frame::Hello {
            v: 1,
            build_id: "b".into(),
            host_arch: "x86_64".into(),
            claude_dir: "/c".into(),
            codex_dir: Some("/home/u/.codex".into()),
            kinds: vec!["claude".into(), "codex".into()],
            capabilities: vec![],
            emits: vec![],
        })
        .unwrap();
        // ★ 精确字节（aterm fixture 交叉核真值）：字段按声明序，codex_dir 在 claude_dir 后、kinds 次之。
        assert_eq!(
            hello,
            "{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/c\",\"codex_dir\":\"/home/u/.codex\",\"kinds\":[\"claude\",\"codex\"]}\n"
        );

        let sa = to_line(&Frame::SessionAdded {
            sid: "s".into(),
            agent_kind: Some("codex".into()),
            liveness_confidence: Some("heuristic".into()),
            session_kind: None,
            attachable: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        })
        .unwrap();
        assert_eq!(
            sa,
            "{\"kind\":\"session_added\",\"sid\":\"s\",\"agent_kind\":\"codex\",\"liveness_confidence\":\"heuristic\"}\n"
        );

        let ss = to_line(&Frame::SessionStatus {
            sid: "s".into(),
            status: Some("busy".into()),
            waiting_for: None,
            liveness_confidence: Some("heuristic".into()),
        })
        .unwrap();
        assert_eq!(
            ss,
            "{\"kind\":\"session_status\",\"sid\":\"s\",\"status\":\"busy\",\"liveness_confidence\":\"heuristic\"}\n"
        );
    }

    /// **absent 形**（Claude 会话 / 旧 daemon）：skip_if_none/empty → 字段**完全省略**，帧对 Claude
    /// **字节等价旧形**（向后兼容红线）。aterm 消费侧「省=null→缺省 claude/authoritative」据此对齐。
    /// ★ 精确字节串 = aterm fixture 交叉核的真值。
    #[test]
    fn dg3_codex_fields_skipped_when_absent_claude_byte_equivalent() {
        let hello = to_line(&Frame::Hello {
            v: 1,
            build_id: "b".into(),
            host_arch: "x86_64".into(),
            claude_dir: "/c".into(),
            codex_dir: None,
            kinds: vec![],
            capabilities: vec![],
            emits: vec![],
        })
        .unwrap();
        assert_eq!(
            hello,
            "{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/c\"}\n",
            "codex_dir/kinds 空 → 省略，Hello 字节等价旧形"
        );

        let sa = to_line(&Frame::SessionAdded {
            sid: "s".into(),
            agent_kind: None,
            liveness_confidence: None,
            session_kind: None,
            attachable: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        })
        .unwrap();
        assert_eq!(
            sa, "{\"kind\":\"session_added\",\"sid\":\"s\"}\n",
            "agent_kind(缺=claude)/liveness_confidence(缺=authoritative) 省略，字节等价旧形"
        );

        let ss = to_line(&Frame::SessionStatus {
            sid: "s".into(),
            status: Some("idle".into()),
            waiting_for: None,
            liveness_confidence: None,
        })
        .unwrap();
        assert_eq!(
            ss, "{\"kind\":\"session_status\",\"sid\":\"s\",\"status\":\"idle\"}\n",
            "liveness_confidence 省略，字节等价旧形"
        );
    }

    /// ★ S0：`cause` 的线上表现 —— `Gone` **不写字段**（additive，旧 monitor 原样工作），
    /// 只有 `Superseded` 才出现。这条同时是**跨语言双写点**的本侧锚：字面量
    /// `"superseded"` 与 monitor `src-tauri/src/ssh_source.rs` 的解析处逐字一致。
    #[test]
    fn removal_cause_is_additive_on_the_wire() {
        let gone = to_line(&Frame::SessionRemoved {
            sid: "s".into(),
            cause: RemovalCause::Gone,
        })
        .unwrap();
        assert!(
            !gone.contains("cause"),
            "Gone 必须不写 cause 字段（否则旧 monitor 看到未知字段、帧也白白变大）：{gone}"
        );
        let sup = to_line(&Frame::SessionRemoved {
            sid: "s".into(),
            cause: RemovalCause::Superseded,
        })
        .unwrap();
        assert!(
            sup.contains(r#""cause":"superseded""#),
            "Superseded 必须逐字发 \"superseded\"（monitor 侧按这个字面量解析）：{sup}"
        );
        // 反向自检：两条不是同一个串（否则上面两个断言可能同时被一个退化实现满足）。
        assert_ne!(gone, sup);
    }
}
