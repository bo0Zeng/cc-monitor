//! Phase-0 backend→client wire types.
//!
//! Wire contract: exactly one UTF-8 JSON object per line, terminated by `\n`,
//! with no bare `\n`/`\r` inside the object. `serde_json` compact output
//! escapes any inner newline as `\n` (two chars), so the only literal newline
//! on the wire is the trailing terminator appended by [`to_line`].

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single backend→client frame.
///
/// Serializes with an external `kind` tag, e.g.
/// `{"kind":"hello","v":1,...}` or `{"kind":"session_added","sid":"..."}`.
/// [`Frame::SessionRemoved`] 的原因。**双写点**：字面量 `"superseded"` 与 monitor
/// `src/frontend/shell/src/ssh_source.rs` 的解析处逐字一致，由 monitor 侧
/// `removal_cause_wire_literal_stays_in_sync`〔散文墓碑〕 钉住（同 `TMUX_LS_FMT` 的纪律）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovalCause {
    /// 真的没了：pidfile 被删 / 进程退出 / 原地翻成非交互 kind。
    /// **默认值** —— 缺字段就是它，保证旧 backend×新 monitor 与今天行为一致。
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

/// [`Frame::SessionAdded`] 的 `container`：**这条活着的会话住在什么容器里**。
///
/// 可恢复性（死了之后能不能接回去）由容器类型决定 —— 在 tmux 里的，claude 退了
/// 终端还在（「可重连」）；不在的，只能 resume（「已结束」）。死的那一刻 monitor 会现查一次 tmux，
/// 但**活着的时候**这一格此前没人报⇒ 前端只能写「没报」。
///
/// 判定住 `observe::watcher::container_of`（喂它的是 `control::identity_tag::tag` 那一次探测的结局，
/// 不多起进程）。**判不了就不写这个字段**（缺席 ≠ `none`）：环境读不到、pane 不在默认 socket 上、
/// 探测失败、非 Linux —— 都是「不知道」。
///
/// 线上两个字面量 `"tmux"` / `"none"` 与 monitor `ssh_source::parse_frame` 逐字一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionContainer {
    /// 这个 claude 进程在一个 tmux pane 里（`TMUX_PANE` 核过形状、tmux 认得那个 pane）。
    Tmux,
    /// 环境读得到、没有 `TMUX_PANE` ⇒ 不在任何 tmux 里。
    None,
}

/// [`Frame::SessionState`] 的 `state`：**一条会话离开「活」之后是什么** ——
/// 那台机器的后端自己裁（`observe::session_ledger`：摘除原因 ＋ 它自己那份 tmux 快照），客户端只收成品、不再猜。
///
/// 线上两个字面量 `"reconnectable"` / `"ended"` 与 monitor `ssh_source::parse_frame` 逐字一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFate {
    /// claude 退了、它的 tmux 会话还在（`@ccm_sid` 仍挂着它）⇒ 接得回去。
    Reconnectable,
    /// 进程没了、容器也没了（或被顶替了）⇒ 只能 resume。
    Ended,
}

/// 一条**丢了就不可恢复**的帧的身份。
///
/// `Overflow` 原来只说「丢了 N 条」。对**内容帧**那没问题（行还在远端 jsonl 里，
/// 重开会话就补上）；对**状态增量帧**（`session_added`/`session_removed`/`session_state`）
/// 就不行 —— 它是一次差分的结果，**别处不存在**，客户端只知道「丢了 N 条」是没法重同步的。
/// ⇒ 本结构给那些帧带上身份，让客户端能精确地重新问那几个主体。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LostFrame {
    /// 帧种（与 `kind` tag 同一套 snake_case 名字）。
    pub kind: &'static str,
    /// 主体：会话 sid / tmux 会话名。取不到就是 `None`（如 `hello`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// `hello.homes` 的一项 —— **某个 agent 在这台机器上的 home 目录**〔`S4` / `D3`〕。
///
/// `D3` 逐字：「agent 维度只许出现在**值**里（`agent_kind`），不许出现在**字段名**里」。
/// 这个结构就是那条 charter 的形状：两个字段名都与任何一个 agent 无关，
/// **接第三个 agent 是多一个元素，不是多一个字段**。
///
/// 它替掉的是并列 `<名>_dir` 那条路。那条路的终点 `D3` 已经写死了：
/// hello 帧里五个并列的目录字段，而客户端要靠 `if/else` 猜哪个有值。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentHome {
    /// 哪个 agent —— **值**，与 `session_added.agent_kind` 同一套取值空间。
    pub agent_kind: String,
    /// 该 agent 在这台机器上的 home 目录（绝对路径）。
    pub path: String,
}

/// `hello.unavailable` 的一项 —— **这条命令我接得下，但在这台机器上做不到，以及为什么**
///〔`K-P4` 09-04，用户逐字「事前协商是要的」〕。
///
/// # 它为什么是一张**负向**表（这一格是本结构最贵的判断，不是口味）
///
/// 三条既有的能力面都是**正向**清单。正向在这里**结构上表达不了「我什么都做不到」**：
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
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Unavailable {
    /// 哪条命令 —— 取值空间与 `Hello.commands` **同一套**（`inbound::COMMANDS`）。
    /// ⚠ 它**必须**同时出现在 `commands` 里：本表说的是「接得下但做不到」，
    /// 「根本不接」那一格由不在 `commands` 里表达（客户端 `accepts()` 一个字节都不发）。
    pub command: String,
    /// 为什么做不到 —— **就是真调用那一刻会回的那个命令级 code**（如 `no_tmux`）。
    pub code: String,
}

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

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Frame {
    /// Handshake sent once when a client connects.
    Hello {
        v: u32,
        build_id: String,
        host_arch: String,
        /// ⚠ **冻结兼容字段，不是欠账**。字段名里带 agent 名，违反 `D3` ——
        /// 但它是 hello 帧里**今天真的在线上、且有仓外消费方**（aterm，契约冻结 2026-07-18）
        /// 的那一个。改名 = 破坏性变更，而 `D3` 自己给的出路是「bump 或走 additive 迁移」。
        /// ⇒ 走 additive：新消费方读 `homes`，这个字段保持不动。
        /// 解锁条件登记在 `agent_boundary_guard::FROZEN_COMPAT`（monitor 与 aterm **都**改读
        /// `homes` 之后才能删）。**别把它挪回 `KNOWN_DEBT`** —— 那张表要清零，这条清不掉。
        claude_dir: String,
        /// `S4`（`D3`）：本机上**各 agent 的 home 目录**（`[{agent_kind, path}]`）。
        ///
        /// 它一次替掉了 DG3 那两个字段：`codex_dir`（并列的 `<名>_dir`，正是 `D3` 禁的形状）
        /// 与 `kinds`（服务的 agent 集 —— 由本表的 `agent_kind` 直接读出，不必并列第二个来源）。
        /// **那两个字段从来没上过线**（`main.rs` 一直硬写 `None` / 空 ⇒ skip），
        /// 所以这次替换对任何已部署的消费方都是零影响 —— `D3` 逐字「越晚改越贵」，
        /// 而它们还没被任何人收到的**现在**就是最便宜的一刻。
        ///
        /// skip_if_empty：今天 agent **发现**（DG1）未接线 ⇒ 恒空 ⇒ 省略
        /// ⇒ **hello 帧对 Claude 的线上字节逐字节不变** —— 由
        /// `wire_tests.rs::production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen`
        /// （生产路径**确实**给空表）与
        /// `wire_tests.rs::dg3_codex_fields_skipped_when_absent_claude_byte_equivalent`
        /// （**给了**空表就得到旧字节）两条**合起来**钉住 —— 缺任一条这句话都不成立。
        /// ⚠ 原先写的是 `hello_bytes_for_claude_are_frozen`〔散文墓碑〕，
        /// **那个名字全仓零定义**，而这句话是**当现状在说**，还撑着一条**契约级**结论
        /// （仓外 aterm 的 hello fixture 按精确字节对，契约冻结 2026-07-18）。
        /// `S5` 落地时往这里填，**不要再加第二个目录字段**。
        #[serde(skip_serializing_if = "Vec::is_empty")]
        homes: Vec<AgentHome>,
        /// F66（#58③，additive）：本后端声明支持的**能力 token 集**（开放字符串，
        /// 加法式）。monitor 按此声明决定发哪些流模式 flag（`--with-bg`/`--tail-only`），
        /// **不再靠 build_id 精确匹配**——闭合 2026-07-09 那类「身份确认不了就全降级」事故。
        /// 旧 monitor 忽略此字段（additive）；空/缺 = 按最小能力集待它。
        /// **§26 死循环护栏**：只声明本 backend **会先剥离对应 flag** 的能力（老到不剥离
        /// 未知 flag 的后端也老到不声明该能力，声明 = 自证认识该 flag）。
        #[serde(skip_serializing_if = "Vec::is_empty")]
        capabilities: Vec<String>,
        /// phase②（backend-08，additive，与 `capabilities` **正交**）：本 backend **会发射的帧 kind 集**
        /// （snake_case，如 "session_status"/"turn_end"）。aterm 据此**门控消费**（emits 含该 kind →
        /// 期待/依赖该帧；不含 → 不依赖、回退 β/watchdog）。**区别于 `capabilities`**（后者=流 flag-strip
        /// 能力、受 §26 死循环护栏 + `every_capability_token_is_strippable` 强制每 token 有可剥离 flag）——
        /// `emits` 是纯发射声明、无对应 flag、**不受 §26**。空/缺 → 省略（旧 client 忽略，additive）。
        #[serde(skip_serializing_if = "Vec::is_empty")]
        emits: Vec<String>,
        /// U6b-2（additive）：本 backend **接受的入方向命令集**。
        ///
        /// 能力协商此前只有出方向那一半（`capabilities` 说「我认识哪些流 flag」）。
        /// 客户端得知道发什么过去才有人接，否则只能试错。
        /// 空/缺 = **这个后端不读 stdin**（U6b-1 之前的所有版本），客户端别发命令。
        #[serde(skip_serializing_if = "Vec::is_empty")]
        commands: Vec<String>,
        /// `K-P4`（09-04，additive，**与上面三条都正交**）：本 backend **接得下、但在这台
        /// 机器上做不到**的命令，以及原因（`[{command, code}]`，见 [`Unavailable`]）。
        ///
        /// # 它买的是什么：**事前**那一半
        ///
        /// `commands` 说的是「**我接这条命令**」，不是「**我做得到这件事**」。差额今天
        /// **只有调用之后**才知道：Windows 上没有 tmux，握手帧照样宣称接 `kill`/`launch`，
        /// 前端照样画按钮，点了才收到 `no_tmux`。用户 09-04 逐字裁「**事前协商是要的**」。
        /// ⇒ 本字段是握手帧的第四条面，回答的是**「我做得到什么」**。
        ///
        /// # 三条既有面为什么都装不下它（每条都有各自的硬理由，不是嫌挤）
        ///
        /// · `capabilities`：受 §26 死循环护栏 + `every_capability_token_is_strippable` ——
        ///   **每个 token 必须有一条能被 `split_stream_flags` 剥掉的流 flag**。「做得到 kill」
        ///   没有对应的流 flag ⇒ 塞进去要么当场红，要么被迫编一个假 flag（更坏）。
        ///   而且它的默认方向是相反的：缺 = 最小能力集（往下降级安全）；本字段缺 = **没有把握**
        ///   （不许往下降级，否则能用的功能会消失）。**一个字段装两个方向的默认值**，
        ///   正是本工作区最贵的那一类病。
        /// · `emits`：取值空间是**帧 kind**，语义是「我会发什么」。
        /// · `commands`：取值空间是**命令名**，语义是「我接什么」；而且它是正向表，
        ///   把做不到的从里面**摘掉**是错的 —— 那会让客户端 `accepts()` 直接拒发，
        ///   连「点了告诉你为什么」这条兜底路都没了，也让 `COMMANDS`/`REGISTRY` 双向相等
        ///   那条判据被迫按平台分叉。
        /// ⇒ 本字段的键是 **(命令名 × 命令级 code)** 这个**对**，三条面没有一条是这个形状。
        ///
        /// # 消费侧口径（三句，缺一句就会读错）
        ///
        /// ① **空/缺 = 这台后端没有任何「做不到」的把握**，不是「全都做得到」——
        ///    客户端照今天的样子办（照发、点了看 code）。旧后端天然落这一格。
        /// ② 列出来的那条 = **别画那个按钮**（或画成灰的，配 `code` 那句人话）。
        /// ③ 🔴 **它是提示，不是闸门。** backend 自己**绝不**拿这张表去拒命令。
        ///    **过期窗口比「一次连接」大得多，别按直觉估**：`build_hello` 在分档**之前**
        ///    只调一次（`main.rs`），那一帧随后交给两条载体，而常驻那条（`listen.rs`）
        ///    的分档表逐字写着「**不限次的『只读 hello 就走』**」——
        ///    ⇒ **同一帧会被这个进程后续的所有连接共用，读数可以任意旧。**
        ///    真拿它去拒 = 把一份可能几小时前的读数变成一次真停机。
        ///    客户端硬发照样走真路，成不成由 `no_tmux` 那条老路回答。
        ///    ⇒ 将来要「**会变的**可用性」，走一条新帧（`emits` 那一轴），**不要回头改 hello**：
        ///    hello 结构上就是「连接建立时说一次」的东西。现成材料已经有 ——
        ///    `observe/tmux_observe.rs` 的 `OBS_NO_TMUX` 是一份运行期读数（watch loop 周期跑
        ///    本地 `tmux ls`），它不是做不到，是**来得比握手晚**。
        ///
        /// # 已真填
        ///
        /// `main.rs::build_hello` 填 `unavailable_here()`（tmux · unix 权限位两维）。仓外 aterm 不读这个字段
        /// （只读核过它的 `parseHello`：通用 map、未知字段忽略）；有 tmux 的 unix 机器上表为空 ⇒ 字节不变，
        /// 没 tmux / Windows 上字节变了 ⇒ 要 bump `BUILD_ID`。
        /// 钉它的：`main_fourth_face_tests::production_hello_fills_unavailable_from_this_machine` ·
        /// `hello_unavailable_is_additive_present_and_absent`（两形字节）。
        #[serde(skip_serializing_if = "Vec::is_empty")]
        unavailable: Vec<Unavailable>,
        /// 〔additive〕**起我的宿主交给我的那几格环境，原样回显**（`{名: 值}`，名单 [`HOST_ECHO_ENVS`]）。
        ///
        /// 它回答的是「这个后端住**哪个家**」：本机常驻后端被 monitor 起时交了中转端口 ＋（隔离跑时）家。口按家算，
        /// 但撞口的仍可能是另一个家的后端 ⇒ monitor 读完 hello 拿这一格与自己要交的那份比，
        /// 对不上就拒、出声（`local_backend_host.rs::hello_verdict`）—— 不接一个会把写落进别的家的后端。
        ///
        /// **一格都没被交 ⇒ 省略**（远端 · 被 ssh exec 起的 · aterm 连的那些）⇒ 那些 hello 的线上字节**逐字节不变**
        /// （`wire_tests.rs::hx2_production_hello_bytes_do_not_change_when_nothing_was_handed` 钉）。
        /// 🔴 **监听口的钥匙永远不在这里**：名单只有两格、钥匙文件路径不在名单里（`wire_tests.rs::hx2_the_listen_token_is_never_echoed` 钉）——
        /// hello 是「只读 hello 就走」那一档谁都读得到的东西。
        #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        host_env: std::collections::BTreeMap<String, String>,
        /// `commands` 里**撤不动**的那几条（阻塞档：开跑之后 `cancel` 回 `not_cancellable`）。
        /// 从 `inbound::REGISTRY` 的 `Run::Blocking` 派生（`inbound::uncancellable`，一个家）。客户端据此在本地撤单时
        /// 说「那台停不了它、可能还在跑」，也不再为它补发一条注定被拒的撤单。
        /// 空/缺 = 没有把握（旧后端）⇒ 客户端照旧补发、看回话。
        #[serde(skip_serializing_if = "Vec::is_empty")]
        uncancellable: Vec<String>,
    },
    /// One JSONL line tailed from a session file —— 带的是**成品**：
    /// 这一行在渲染模型里是什么（`message`，缺 ＝ 不进界面、照占号）与它自己的 `cwd`。解释住后端适配层，
    /// monitor 只原样转交。原文 `raw` 只给发了 `--with-raw` 的客户端（第二个前端自己解析记录）。
    Line {
        session_id: String,
        path: String,
        seq: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        /// backend-01（gap#2，additive 不 bump PROTO_VERSION）：本行末尾（含 `\n`）在文件中的**累计原始字节 offset**——
        /// 语义**逐字节对齐 aterm `LineFramer.endOffset`**：计 CRLF 的 `\r`、含 `\n`、残行不计；resume N ⇒
        /// `tail -c +(N+1)`。给 offset 续拉/截断检测（`seq` 是 per-stream 序数、非 resume 键）。
        /// 注：`Frame` 仅 derive `Serialize`，故此 `#[serde(default)]` 在**本 crate 装饰性**。
        ///
        /// 两个前端都读它：monitor 记续点（`ssh_source.rs` 的 `"line"` 分支 · `snapshot_resume.rs`），
        /// 第二个前端拿它续拉（冻结，`wire_tests::the_shapes_the_second_frontend_reads_stay_put`）。
        #[serde(default)]
        byte_offset: u64,
        /// 这一行的对账键（适配层 `RecordFace::response_id` 给；流的「开始」带同一个值）。没有 ⇒ 不上线。
        #[serde(skip_serializing_if = "Option::is_none")]
        rid: Option<String>,
        /// 〔additive〕这一行记录的**原文**（去掉行尾换行）。只在客户端发了 `--with-raw` 时才带
        /// （`ReaderState::with_raw`）—— 第二个前端自己解析记录，要它；没索要的客户端收到的字节与本字段加进来之前一字不差。
        #[serde(skip_serializing_if = "Option::is_none")]
        raw: Option<String>,
    },
    /// A new session file appeared.
    ///
    /// Batch7-F24（additive，向后兼容）：附带 pidfile 元信息——`session_kind`
    /// （"interactive"/"bg"；字段名避开 enum tag `kind`）、`cwd`、`name`。
    /// None 时不上线（旧行为字节不变）；旧 monitor 忽略未知字段。
    SessionAdded {
        sid: String,
        /// DG3（#2D，additive）：会话属哪 agent kind——`"codex"`（Codex 会话）。Claude 会话**省略**
        /// （skip_if_none）→ 消费侧缺=claude（向后兼容、旧后端无此字段）。
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
        /// **省略 = `true`**（存量会话与旧后端一律照旧，零迁移）。
        /// 来源：pidfile 的 `attachable` 布尔字段（契约见 `src/doc/IPC-PROTOCOL.md` §9.3）。
        #[serde(skip_serializing_if = "Option::is_none")]
        attachable: Option<bool>,
        /// pidfile 记的 `cwd`：进程起在哪个目录（客户端认「我刚起的那条起来了」用）。
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        /// 会话的项目目录：会话起在哪个目录（tab 标题 · 打开工作目录 · 分组都用它）。记录归哪一家就问哪一家
        /// （`agents::project_dir_of`，只读记录开头）；记录还没写出来 ⇒ pidfile 那一格（那一刻还没有命令跑过，就是起会话的目录）。
        #[serde(skip_serializing_if = "Option::is_none")]
        project_dir: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Batch8-F25（additive）：该会话 jsonl 的远端绝对路径（同 sid 多文件时
        /// 取 mtime 最新者）——monitor 旁路快照（`--read-session`）用。宣告时
        /// 未找到 jsonl（会话刚起还没写首行）→ None：此时无历史可拉，后续行
        /// 天然从 tail 的 seq 0 起全量到达，无需快照。
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        /// Batch8 审计 D-I2（additive）：tail-only 模式下 prime 时的完整行数 L
        /// ——monitor 校验快照拉到的行数 ≥ L 才算成功（不足 = 中途断/backend
        /// 报错，触发重试；exit status 经 ChannelStream 拿不到，行数校验更强）。
        /// 全量模式 None。
        #[serde(skip_serializing_if = "Option::is_none")]
        lines: Option<u64>,
        /// Batch9-F27（additive）：宣告时的初始 status/waitingFor——连接建立灯就对。
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        waiting_for: Option<String>,
        /// 〔additive〕这条会话住在什么容器里（见 [`SessionContainer`]）。
        ///
        /// **判不了 ⇒ 不上线**（`skip_serializing_if`）：旧客户端、以及判不了的会话，收到的字节
        /// 与本字段加进来之前一字不差。缺席的意思是「不知道」，**不是** `none`。
        #[serde(skip_serializing_if = "Option::is_none")]
        container: Option<SessionContainer>,
        /// 〔additive〕那个 claude 进程的 **pid**。
        ///
        /// 给谁：本机 monitor 的「↗ 拉前」—— 本机判活改由本机后端的帧来之后（monitor 不再自己读 pidfile），
        /// 它按 pid 找父 PowerShell 去绑窗口（`bind::SidHwndCache::record`，Windows）只能从这一格拿 pid。
        ///
        /// 只在客户端发了 `--with-pid` 时才带（`ReaderState::with_pid`；只有本机那条流发）—— 没索要的客户端
        /// 收到的字节与本字段加进来之前一字不差（仓外 aterm 那份按精确字节对的 fixture 因此不受影响，hello 也不变）。
        #[serde(skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
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
    /// **会话账本的成品**：这条会话离开「活」之后是可重连还是已结束（见 [`SessionFate`]）。
    ///
    /// 由 `observe::session_ledger` 在它看着发出去的 `session_removed` / `tmux_sessions` 之后补发（同一个 sink、同一条线程，
    /// 紧跟在引起它的那一帧之后）；新连接第一份可观测的 tmux 快照里「挂着 `@ccm_sid`、却不在活会话里」的也各发一帧可重连
    /// （在 `sessions_replayed` 之前）。旧 monitor / 仓外 aterm 不认这个 kind ⇒ 忽略（additive）。
    SessionState { sid: String, state: SessionFate },
    /// A session file went away.
    SessionRemoved {
        sid: String,
        /// **S0（additive）：这个 sid 是「死了」还是「被顶替了」。**
        ///
        /// 为什么必须由后端说：monitor 收到 removed 后要在「灰点（tmux 还在，可以回去
        /// attach）」和「归档」之间二选一，今天它靠**查自己缓存的那份 `tmux ls` 原文里
        /// `@ccm_sid` 还在不在**来猜。`/branch`（同 pidfile 原地换 sid）时这个猜法必错：
        /// 旧 sid 的 tmux 格子还在、只是 `@ccm_sid` 已被换成新 sid（`U-NP④` 之前由
        /// `shared/ccm` 的每秒 poller 换，之后由后端的 `control::identity_tag` 换 ——
        /// **换的时机与结果一样，这条推理不受影响**），
        /// 而那份缓存**在 P5 删掉 8s ticker 之后再没有任何事件路径会去刷新它**
        /// ⇒ 旧 tab 永久灰点、且按旧 sid 找不到 tmux 会话 ⇒ 杀不掉。
        ///
        /// backend 这边本来就**分得清**这两件事——它们是两个不同的调用点。把信息发出去，
        /// monitor 就不用猜，也就不受「缓存多旧」和「`@ccm_sid` 什么时候被回填」影响。
        ///
        /// 线上表现：[`RemovalCause::Gone`] **不写字段**（旧 monitor 原样工作，additive）；
        /// 只有 `Superseded` 才出现 `"cause":"superseded"`。
        #[serde(default, skip_serializing_if = "RemovalCause::is_gone")]
        cause: RemovalCause,
    },
    /// phase②（backend-09）：turn-end 边沿（一轮 assistant 完成）。**方案 C：raw-per-record、backend
    /// 不 dedup**——每见一条 turn-end 记录（判词住适配层，见 `agents/claudecode/turn.rs`）
    /// 发一帧；aterm 侧 **rolling-latest + debounce(1200ms) `baselineByPath`** 塌合同 turn 的多记录、
    /// 首见吞历史不通知、offset 续拉重放 uuid ≤ 基线不通知（**transport-agnostic、与 β 逐字同语义、
    /// gap#6 闭**；backend 不猜消息边界）。`uuid` = 完成记录**顶层 uuid** = 客户端 dedup 键。
    /// **不带 `byte_offset`**（只 Line 带）——α watcher：Line 推 currentOffset、TurnEnd 喂 rolling
    /// current，结算时 baseline+offset 同段提交。旧 monitor 未知 kind 忽略（additive）。
    TurnEnd { session_id: String, uuid: String },
    // 这里原是 `TmuxSessions`（B2 · P1：`tmux ls` 原文 ＋ 观测取值）与 `TmuxSessionClosed`（P5：差分出的
    //   正向死亡）两帧。会话账本进后端之后，客户端只收成品（`SessionState`），这两份原料再没有线上读者（monitor 已不消费；
    //   仓外 aterm 的 `DaemonTransport.parseFrame` 从来按未知 kind 跳过）⇒ 删。快照只喂 `observe::session_ledger`。
    /// The bounded frame channel back-pressured and the reader had to drop
    /// `dropped` frames (a slow/wedged SSH pipe). Emitted once when the channel
    /// drains enough to accept it, so the client can warn the user that live
    /// lines were lost (#32). `dropped` counts frames dropped since the last
    /// overflow signal.
    Overflow {
        dropped: u64,
        /// 〔audit-0805 F03，**additive**〕那批丢帧里**不可恢复**的那些的身份。
        /// 空集时**不序列化** ⇒ 旧客户端看到的字节与从前一字不差。
        #[serde(skip_serializing_if = "Vec::is_empty")]
        lost: Vec<LostFrame>,
        /// 身份表是**有界**的（见 `watcher.rs` 的 `LOST_IDENTITY_CAP`）。触顶后置位。
        /// ⚠ 没有这个界，就等于把 `CHANNEL_CAPACITY` 想防的内存增长从帧挪到了 `Overflow` 自己身上。
        #[serde(skip_serializing_if = "is_false")]
        lost_truncated: bool,
    },

    /// U6b-1：**入方向命令的应答**。`id` 是客户端给的不透明串，backend **原样回显、不解析**。
    ///
    /// 复用出方向的 `kind` tag 空间而不另开一条流：旧 monitor 见到未知 kind 会**忽略**
    /// （§10 已有的 additive 规律），所以新 backend × 旧 monitor 天然安全。
    ///
    /// 错误形状 `{code, message}` **对齐 `--resolve` 已冻结的那套**（协议 v1 §3），
    /// 不发明第二种错误 JSON。成功时两者都省略。
    Reply {
        id: String,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// U6b-3：命令的返回值（如 `resolve` 的 CommandPlan）。无返回值的命令省略。
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
    },

    /// U6b-1：某个在跑的命令**已被取消**。取消是一条普通命令（`cmd:"cancel"`）、不是带外信号——
    /// 带外要么另开通道要么发明转义序列，两者都要新的解析纪律，而取消排队等一下并无妨。
    Cancelled { id: String },

    /// **这台机器上的账号清单变了**（账号 manifest 被改写）。
    ///
    /// 无载荷：客户端收到就重拉一次账号清单（`accounts-list`），不在帧里带清单本身 ——
    /// 清单的唯一出口仍是那条查询，别让同一份数据有两个出口。watcher 盯 manifest 所在目录，
    /// 一批文件事件里 manifest 动了几次都只发一帧。旧 monitor / 仓外 aterm 不认这个 kind ⇒ 忽略（additive）。
    AccountsChanged,

    /// **这台机器上某个会话的任务清单变了**（`<agent 家>/tasks/<sid>/` 里有动静）。
    ///
    /// 只带 sid：客户端收到就重问一次 `tasks-list`（清单的唯一出口仍是那条查询，同 `accounts_changed`）。
    /// 一批文件事件里同一个 sid 动了几次都只发一帧。旧 monitor / 仓外 aterm 不认这个 kind ⇒ 忽略（additive）。
    TasksChanged { sid: String },

    /// **这台机器的活会话清单报完了**：`observe::watcher::watch_loop` 的 Phase 1
    /// （同步扫 `sessions/`、对每个活 pidfile 发一帧 `session_added`）走完那一刻发**一次**。
    ///
    /// 它是那张表要的判据：客户端手里有一条「固定」的会话条目、而这台机器
    /// 还没报过它 —— 是「这台还没说完」（显示**说不清**），还是「这台说完了、里面没有它」
    /// （显示**已结束**）？此前线上没有任何东西分得开这两件事（Phase 1 结束没有标记），
    /// 于是固定复活的 tab 一律被说成已结束（末条）。
    ///
    /// 无载荷：清单本身就是它前面那些 `session_added`（同一条有序的流），别让同一份数据有两个出口。
    /// 之后的增减照旧走 `session_added` / `session_removed`。旧 monitor / 仓外 aterm 不认这个 kind ⇒ 忽略（additive）。
    SessionsReplayed,

    /// **活会话的记录文件不见了**（被删 / 被改名走了）。
    ///
    /// 文件管理器改得动活会话的 jsonl；观察侧当它是「看的、不是管的」：不崩、不误判结束（判活不看 jsonl），
    /// 出声一次 —— 每次「在 → 不在」只发一帧；同名文件再出现（agent 按路径追加重建）从 0 读，当改写办：先发 [`Frame::SessionFileReread`]、行号从 0 重数；之后再不见才再发。
    /// 旧 monitor / 仓外 aterm 不认这个 kind ⇒ 忽略（additive）。
    SessionFileGone { session_id: String, path: String },

    /// **活会话的记录文件被改过了，已从头重读**（截短 · 或游标之前被原地改写）。
    ///
    /// 紧跟在这一趟重读出来的 `line` 帧**之前**（同一个 sink、同一条线程）。重读的行号从 0 重数
    /// （seq ＝ 当前文件里的行号，[`SeqCounter::restart`]）⇒ 下游据这一帧把这个会话旧的一代整份作废：monitor 丢留存与续点，
    /// 前端 tab 整份重来。
    SessionFileReread {
        session_id: String,
        path: String,
        why: RereadWhy,
    },

    /// **一条链路的下行字节**（`dial/link.rs`）。
    ///
    /// 本机只常驻一个后端，到各远端的 SSH 连接由它持有并复用；
    /// monitor 经**这条已有的流**向它开「链路」（`link-open`），链路上的字节就是 C2 那个
    /// `--dial` 子进程原来写在自己 stdout 上的那一串（阶段行 → 一行 ack → 按用法的字节），
    /// 一个字节的形状都没改 —— 变的只是载体：子进程的管子 → 本帧。
    ///
    /// `data` 是 base64（标准字母表、带补位，[`b64_encode`]）：链路搬的是**任意字节**，
    /// 而一帧是一行 UTF-8 JSON。解码后 ≤ `dial::link::LINK_CHUNK_BYTES`。
    ///
    /// 🔴 **不丢**：本帧走**应答那条独立通道**（阻塞 `send().await`），不走出方向那条会丢帧的大通道 ——
    /// 丢一块下行字节就是这条链路上的数据坏了，别处没有第二份。流控是逐链路的信用（`link-credit`），
    /// 由 `dial/link.rs` 的下行泵执行。旧 monitor / 仓外 aterm 不认这个 kind ⇒ 忽略（additive）。
    LinkData { link: String, data: String },

    /// **一条链路不会再有字节了**；后端已经忘掉这个 `link` id。
    ///
    /// `error` 省略 = 正常收尾（用法那一段自己结束了：远端断了 / capture 收全了 / 转发收工了）；
    /// 带上 = 非正常收尾（下行泵写不出去 · 连接表被拆），那句人话原样给调用方。
    /// 拨不通**不**走这里 —— 那是链路字节里那一行失败的 ack（与 C2 同形），本帧随后照常到。
    LinkEnd {
        link: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// **一趟传输此刻的样子**（`control/transfer.rs`，`transfer-start` 之后才出现）。
    ///
    /// 每一帧都是一整份快照（`got` / `total`），不是增量 ⇒ 转发任务按 `watch` 合并掉中间几格不丢信息；
    /// 带 `end` 的那一帧是这一趟的最后一帧（`done{bytes}` · `failed{why}` · `cancelled`）。
    /// 🔴 **不丢**：走应答那条独立通道（终局丢了，客户端那一侧的看的人就永远等下去）。
    /// 旧 monitor / 仓外 aterm 不认这个 kind ⇒ 忽略（additive）。
    Transfer {
        id: String,
        got: u64,
        total: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        end: Option<TransferEnd>,
    },

    /// 〔「测试连接的进度不许倒退」〕**测试连接那一趟的一格进度**（`dial/probe.rs`，`remote-probe` 在跑时才出现）。
    ///
    /// `ticket` = 界面发起时交来的票（进度流 `probe-progress/<ticket>` 的名字，本后端只当不透明的串回填）；`cell` 恰好一个键：
    /// `stage`（握手那几行，与界面 `ConnectStage` 同形）· `reached`（`ssh` / `hello` / `control` 走完了那一段）· `end`（结局，最后一格）。
    /// 🔴 **不丢**：走应答那条独立通道（`end` 丢了，界面就说不出结局；中间格丢了，就说不清停在哪一段）。
    /// 旧 monitor / 仓外 aterm 不认这个 kind ⇒ 忽略（additive）。
    Probe {
        ticket: String,
        cell: serde_json::Value,
    },

    /// **中转抄出来的一个 SSE 事件**（或一个响应的收尾）。
    ///
    /// 常驻后端会发（中转住在它进程里，`relay::host`；本机远端同形）。
    /// 四样东西，**没有业务词**（字段名就是「tee 线上字段名住哪」的答案：住这里，serde 名）：
    /// - `stream`：claude 自己请求头里带的会话标识（== 它的 sid，新开 / resume / 分叉同一形）；没带 ⇒ 空串。后端不解释它。
    /// - `resp`：本进程第几个响应（跨响应单调，后端重启从 0 起）。
    /// - `n`：这一个响应里第几个事件，**从 0 连续**。后端每个事件先占号再投递 ⇒ 丢了的号不出现 ⇒
    ///   接收侧看 `n` 连不连得上就知道缺在哪（`Gap{from_seq,to_seq}` 那一形，原位、纯算术）。
    /// - `data`（与 `end` 恰有一个）：SSE `data:` 后面那段原文，**一个 JSON 串**（上游字节敌手可控，不参与帧结构）。
    /// - `end`：这个响应不会再有事件了（`done` 上游说完 · `broken` 转发以错误收尾）；这一帧的 `n` = 一共占了几个号。
    ///
    /// 🔴 **可丢**：走后端自己那条有界 tap 通道（`tap::TAP_CAPACITY`），满了就丢，不回推中转、不挤出方向的内容帧。
    /// SSE 只保快，jsonl 保对。旧 monitor / 仓外 aterm 不认这个 kind ⇒ 忽略（additive）。
    Tap {
        stream: String,
        /// 这段流归哪个子运行（主运行 ⇒ 不上线）。由后端归位（`run_route`）：请求自报了就定；没自报 ⇒ 没有在跑的子运行就归主，
        /// 有 ⇒ 先挂起、等记录对上对账键再放出来。
        #[serde(skip_serializing_if = "Option::is_none")]
        run: Option<String>,
        resp: u64,
        n: u64,
        /// 归一事件（上游原始事件已在后端按协议面折过；界面不认任何一家的事件名）。
        #[serde(skip_serializing_if = "Option::is_none")]
        ev: Option<crate::agents::StreamEv>,
        #[serde(skip_serializing_if = "Option::is_none")]
        end: Option<TapEnd>,
    },
    /// 一个会话的运行表（主运行之外的那几个子运行：标签 · 状态 · 最近一件事 · 派出它的那次工具调用）。表变了就整份发一次；
    /// 按最近一次动静排（最早动过的在前）。`ended`：被挤出运行表的已收场子运行（对上了派出调用的那些，先挤出的在前）——
    /// 派出它们的那几张卡照样标得上终态。
    SessionRuns {
        sid: String,
        runs: Vec<RunInfo>,
        ended: Vec<RunEnded>,
    },
}

/// 被挤出运行表的一个已收场子运行（[`Frame::SessionRuns`] 的 `ended` 一项）：是哪个 · 派出它的那次工具调用 · 终态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct RunEnded {
    pub run: String,
    pub tool: String,
    pub state: RunState,
}

/// 一个子运行此刻的样子（[`Frame::SessionRuns`] 的一项）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct RunInfo {
    pub run: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub kind: Option<String>,
    /// 派出它的那次工具调用（父侧 id）；还没对上 ⇒ 不上线。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub tool: Option<String>,
    pub state: RunState,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub last: Option<crate::agents::RunDid>,
}

/// 子运行的五态。收场的三态以派出那一方说的为准（子记录自己写出终局也算，先到先算）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Running,
    Done,
    Failed,
    /// 被叫停。
    Stopped,
    /// 没有任何收场信号、子记录又久未再写（`observe::runs::STALE_AFTER`）：不当它在跑。
    Unknown,
}

/// 一个响应怎么收场的（[`Frame::Tap`] 的 `end`）。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TapEnd {
    /// 上游把响应说完了（转发正常收尾）。
    Done,
    /// 转发以错误收尾：下游走了（claude 被 Esc 打断）· 上游断了 · 写不动。
    Broken,
}

/// [`Frame::SessionFileReread`] 的「为什么从头重读」。线上两个字面量。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RereadWhy {
    /// 变短了（比读到过的最长还短）。
    Truncated,
    /// 没变短，但游标之前那一截被原地改写过（末尾指纹对不上）。
    Rewritten,
}

/// 一趟传输怎么收场的（[`Frame::Transfer`] 的 `end`）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TransferEnd {
    /// 传完了。上传那一路带 `sha256`（整份本机文件的摘要，窗口提交 `files-commit-upload` 时原样交回当
    /// `expect`，远端后端改名上位之前对暂存件核一遍）；下载那一路没有（不上线）。
    Done {
        bytes: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
    },
    /// 失败（带下层原话）。上传那一路的暂存件**留着**给续传；下载那一路的 `.part` 删了。
    /// `code`：调用方要按它换路的那几形（今天只有 `sftp_home_mismatch`）；缺席 ＝ 一般的失败。
    Failed {
        why: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
    /// 撤了（`transfer-stop` / 本机流断了）。上传那一路的暂存件已删；下载那一路的 `.part` 留着。
    Cancelled,
}

impl Frame {
    /// **丢了还能不能恢复**。
    ///
    /// # 这条判据是对的，此前错的是「它没有被应用到出方向」
    ///
    /// 仓里对同一类问题已经推理过一次，而且完全正确 —— 只是给 `reply` 帧做的：
    /// `inbound.rs` 让应答走**独立通道**且用 `.send().await` 阻塞背压，
    /// 头注逐字「丢一条应答会让客户端永远等下去」。
    /// **判据是「可恢复的才允许丢」，而出方向那条 10 000 容量的通道里混着两种性质完全不同的东西**，
    /// 丢弃策略却是**按通道**定的、不是按帧种类定的。本函数补的就是这个分类。
    ///
    /// # 拿不准的一律按「不可恢复」算
    ///
    /// 两种错判的代价**不对称**：错判成「可恢复」= 真丢了还没人知道（静默数据丢失）；
    /// 错判成「不可恢复」= 多报一条身份（`Overflow` 载荷大一点，仅此而已）。
    /// ⇒ 只有**能说清「它别处还在」**的才算可恢复。
    ///
    /// ⚠ **穷尽 `match`，不许 `_ =>`** —— 新增帧种时**编译期**就被逼着表态。
    /// 这正是 `audit-0805/LEDGER` **S1** 定下的钉法：判据不是「有没有登记」，
    /// 是「新增一个种类时你躲不掉」。
    pub fn loss_is_recoverable(&self) -> bool {
        match self {
            // 内容帧：行确实还在远端 jsonl 里，重开会话/重读就补上。
            Frame::Line { .. } => true,
            // 派生自某一行 jsonl（轮次判词只看那条记录）⇒ 与 `Line` 同命。
            Frame::TurnEnd { .. } => true,

            // ↓ 以下都是「丢了别处没有」或「拿不准」，一律按不可恢复算。
            //
            // 一次差分的结果，别处不存在 —— 这三个正是 B-3 的正题。
            Frame::SessionAdded { .. } => false,
            Frame::SessionRemoved { .. } => false,
            // 账本的成品：一次裁决的结果，别处没有 ⇒ 不可恢复。
            Frame::SessionState { .. } => false,
            // 状态变迁；没有「下一次必然重发」的保证 ⇒ 保守。
            Frame::SessionStatus { .. } => false,
            // 握手帧丢了这条连接就没有身份了。
            Frame::Hello { .. } => false,
            // 它自己就是「丢了东西」的信号，丢了它等于连丢失都没人知道。
            Frame::Overflow { .. } => false,
            // ⚠ 这两个**根本不走出方向那条通道**（应答走 `REPLY_CHANNEL_CAPACITY` 那条独立小通道，
            //   且是阻塞 `send().await`）。列在这里**只为穷尽** —— 真走到这条路上说明接线错了，
            //   按不可恢复算是保守的那一侧。
            Frame::Reply { .. } => false,
            Frame::Cancelled { .. } => false,
            // 一次状态变化的通知，没有「下一次必然重发」⇒ 保守（丢了客户端就一直拿着旧清单）。
            Frame::AccountsChanged => false,
            // 同上一行：一次变化的通知，丢了那个会话的任务面板就停在旧的（带身份 subject = sid，客户端可重问）。
            Frame::TasksChanged { .. } => false,
            // 一次性的标记，没有「下一次必然重发」⇒ 丢了客户端就一直停在「说不清」
            //   （保守的那一侧：不会把一条说不清的会话说成已结束）。按不可恢复报身份，客户端才知道要重连。
            Frame::SessionsReplayed => false,
            // 一次性的出声，没有「下一次必然重发」⇒ 丢了那个 tab 就不说那句话（内容本身照旧对：游标已按它处置）。
            Frame::SessionFileGone { .. } => false,
            Frame::SessionFileReread { .. } => false,
            // 链路字节：丢一块 = 那条链路上的数据坏了，别处没有第二份。
            // 与上面两个同理，它们**不走**会丢帧的那条通道（走应答通道、阻塞发送）。
            Frame::LinkData { .. } => false,
            Frame::LinkEnd { .. } => false,
            // 传输的进度 / 终局：丢了终局那一帧，看的人永远等下去；也走应答通道。
            Frame::Transfer { .. } => false,
            // 测试连接的进度 / 结局：同上（走应答通道）。
            Frame::Probe { .. } => false,
            // SSE 只保快：它说的事 jsonl 那一侧都有（落盘保对），丢了由位置号 `n` 原位说出来。
            // ⚠ 它**不走**出方向那条通道（走 tap 自己那条），列在这里只为穷尽。
            Frame::Tap { .. } => true,
            // 整份快照：下一次表一变就整份重发；丢了那个会话的子运行行停在旧的那一份，直到下一次变（带身份，客户端知道）。
            Frame::SessionRuns { .. } => false,
        }
    }

    /// 丢帧时给客户端用来**重同步**的身份：帧种 + 主体（sid / tmux 会话名）。
    ///
    /// ⚠ 与 [`Self::loss_is_recoverable`] 同样是穷尽 `match`：新增帧种时两处一起被逼着表态。
    pub fn loss_identity(&self) -> LostFrame {
        let (kind, subject) = match self {
            Frame::Hello { .. } => ("hello", None),
            Frame::Line { session_id, .. } => ("line", Some(session_id.clone())),
            Frame::SessionAdded { sid, .. } => ("session_added", Some(sid.clone())),
            Frame::SessionStatus { sid, .. } => ("session_status", Some(sid.clone())),
            Frame::SessionRemoved { sid, .. } => ("session_removed", Some(sid.clone())),
            Frame::SessionState { sid, .. } => ("session_state", Some(sid.clone())),
            Frame::TurnEnd { session_id, .. } => ("turn_end", Some(session_id.clone())),
            Frame::Overflow { .. } => ("overflow", None),
            Frame::Reply { id, .. } => ("reply", Some(id.clone())),
            Frame::Cancelled { id } => ("cancelled", Some(id.clone())),
            Frame::AccountsChanged => ("accounts_changed", None),
            Frame::TasksChanged { sid } => ("tasks_changed", Some(sid.clone())),
            Frame::SessionsReplayed => ("sessions_replayed", None),
            Frame::SessionFileGone { session_id, .. } => {
                ("session_file_gone", Some(session_id.clone()))
            }
            Frame::SessionFileReread { session_id, .. } => {
                ("session_file_reread", Some(session_id.clone()))
            }
            Frame::LinkData { link, .. } => ("link_data", Some(link.clone())),
            Frame::LinkEnd { link, .. } => ("link_end", Some(link.clone())),
            Frame::Transfer { id, .. } => ("transfer", Some(id.clone())),
            Frame::Probe { ticket, .. } => ("probe", Some(ticket.clone())),
            Frame::Tap { stream, .. } => ("tap", Some(stream.clone())),
            Frame::SessionRuns { sid, .. } => ("session_runs", Some(sid.clone())),
        };
        LostFrame { kind, subject }
    }
}

/// base64 的字母表（RFC 4648 §4，标准字母表、带 `=` 补位）。
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 链路字节进 JSON 串的编码（`link_data` 帧 / `link-data` 命令的 `data`）。
///
/// ⚠ **为什么手写而不加一条依赖**：本 crate 每加一条依赖都要过 `readonly_guard` 的依赖签字，
/// 而这一段是 20 行、无状态、有 RFC 4648 §10 的七条标准向量可对拍
/// （`wire_tests::b64_matches_the_rfc_4648_test_vectors` 与 monitor 侧那一份的判据
/// **各拿同一组 RFC 向量**核自己 —— 异源是 RFC，不是对面的实现）。
pub fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(B64[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// [`b64_encode`] 的逆。**严格**：长度不是 4 的倍数、字母表外的字符、补位不在末尾 ⇒ `Err`
/// （坏的就是坏的，不猜）。
pub fn b64_decode(text: &str) -> Result<Vec<u8>, String> {
    let s = text.as_bytes();
    if s.len() % 4 != 0 {
        return Err(crate::common::contract::malformed(&format!(
            "base64 length {s_count} is not a multiple of 4",
            s_count = s.len()
        )));
    }
    let val = |c: u8| -> Option<u32> { B64.iter().position(|&x| x == c).map(|p| p as u32) };
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let quads = s.len() / 4;
    for (qi, q) in s.chunks(4).enumerate() {
        let pad = q.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && qi + 1 != quads) {
            return Err(crate::common::contract::malformed(
                "base64 padding is not at the end",
            ));
        }
        let mut n: u32 = 0;
        for &c in &q[..4 - pad] {
            let v = val(c).ok_or_else(|| {
                crate::common::contract::malformed(&format!(
                    "base64 has a character outside the alphabet: {char:?}",
                    char = char::from(c)
                ))
            })?;
            n = (n << 6) | v;
        }
        n <<= 6 * pad as u32;
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    Ok(out)
}

/// U6b-1：**入方向**请求信封。只 `Deserialize` —— backend 是读的那一方。
///
/// ```text
/// {"id":"<opaque>","cmd":"<name>","args":{...}}
/// ```
///
/// `id` **不透明**：backend 不解析、不校验格式、只回显。谁生成谁负责唯一 —— 客户端。
/// backend 自己发号的话，重连后号段会撞（同 F90「不许拿会变的东西当持久键」）。
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub id: String,
    pub cmd: String,
    #[serde(default)]
    pub args: serde_json::Value,
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
/// **这是 `HelloFlushed` 的唯一来源。** 放 `wire.rs` 而不是 `inbound.rs`：
/// 握手是协议管道的事，让入方向模块去写出方向的首帧会把职责搅浑。
pub async fn write_and_flush_hello<W: tokio::io::AsyncWrite + Unpin>(
    out: &mut W,
    hello: &Frame,
) -> std::io::Result<HelloFlushed> {
    use tokio::io::AsyncWriteExt as _;
    debug_assert!(
        matches!(hello, Frame::Hello { .. }),
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
        "v={} build={} arch={} home={home} caps={:?} cmds={:?}",
        h.get("v").and_then(Value::as_u64).unwrap_or(0),
        s("build_id"),
        s("host_arch"),
        list("capabilities"),
        list("commands"),
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/stream/wire_tests.rs"]
mod tests;
