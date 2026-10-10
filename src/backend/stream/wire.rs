//! Phase-0 backend→client wire types.
//!
//! Wire contract: exactly one UTF-8 JSON object per line, terminated by `\n`,
//! with no bare `\n`/`\r` inside the object. `serde_json` compact output
//! escapes any inner newline as `\n` (two chars), so the only literal newline
//! on the wire is the trailing terminator appended by [`to_line`].

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// [`Frame::SessionRemoved`] 的原因。**双写点**：字面量 `"superseded"` 与 monitor
/// `src/frontend/shell/src/stream_source/` 的解析处逐字一致，由 monitor 侧
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

/// 终端住在哪种宿主里（线上 `host`）。**宿主的字面量只住这里**：容器那一格（[`SessionContainer`]）与
/// `terminals-list` 每一行的 `host` 都从 [`TerminalHost::as_wire`] 取，收的两层按「认得的 ＋ 其它」收、不写第二份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalHost {
    Tmux,
}

impl TerminalHost {
    pub const fn as_wire(self) -> &'static str {
        match self {
            TerminalHost::Tmux => "tmux",
        }
    }
}

/// 「不在任何宿主里」的那个 `host` 词（＝ 就在起它的那个终端里）。
pub const HOST_NONE: &str = "none";

/// [`Frame::SessionAdded`] 的 `container`：**这条活着的会话住在什么容器里**，词与 `terminals-list` 同一套。
///
/// 线上是对象：`{"host":"tmux","terminal":"tmux-3-7"}` · `{"host":"none"}`。可恢复性由它定 —— 在宿主里的，
/// claude 退了终端还在（「可重连」）；不在的，只能 resume。`terminal` 是 `terminals-list` 里那一行的句柄
/// （同一个函数算，`control::terminals::handle_of`）；打标那一刻算不出 ⇒ 不写这一格。
///
/// 判定住 `control::identity_tag::Outcome::container`（喂它的是打标那一次探测的结局，不多起进程）。
/// **判不了就不写整个字段**（缺席 ≠ `none`）：环境读不到、pane 不在默认 socket 上、探测失败、非 Linux —— 都是「不知道」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionContainer {
    /// 在某个宿主的终端里（`TMUX_PANE` 核过形状、tmux 认得那个 pane）。
    Hosted {
        host: TerminalHost,
        terminal: Option<String>,
    },
    /// 环境读得到、不在任何宿主里。
    None,
}

impl Serialize for SessionContainer {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(None)?;
        match self {
            SessionContainer::Hosted { host, terminal } => {
                m.serialize_entry("host", host.as_wire())?;
                if let Some(t) = terminal {
                    m.serialize_entry("terminal", t)?;
                }
            }
            SessionContainer::None => m.serialize_entry("host", HOST_NONE)?,
        }
        m.end()
    }
}

/// [`Frame::SessionState`] 的 `state`：**一条会话离开「活」之后是什么** ——
/// 那台机器的后端自己裁（`observe::session_ledger`：摘除原因 ＋ 它自己那份 tmux 快照），客户端只收成品、不再猜。
///
/// 线上两个字面量 `"reconnectable"` / `"ended"` 与 monitor `stream_source::parse_frame` 逐字一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFate {
    /// claude 退了、它的 tmux 会话还在（`@ccm_sid` 仍挂着它）⇒ 接得回去。
    Reconnectable,
    /// 进程没了、容器也没了（或被顶替了）⇒ 只能 resume。
    Ended,
}

impl SessionFate {
    /// 写好的字与语气（短名 · 悬停那一句 · 语气）：`session_state` 帧上带着，出口照抄。
    pub(crate) fn cells(self) -> (Words, Words, Tone) {
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

/// **活会话此刻在干什么的那几格的唯一构造口**（`session_added` · `session_status` 都带它，平铺在帧上）：
/// 那一态（`activity`，说不清 ⇒ 不上线）· 写好的字 · 语气 · 监控板组内的序（`activity_order`，小的在前）。
/// 字段私有：帧上这几格只能经 [`ActivityFace::of`] 来，不许哪一处另拼。
///
/// 在跑 ⇒ 运行中 · `now`；在等人 ⇒ 需手动 · `need`；闲着 ⇒ 空闲 · `plain`；一轮停了、后台命令还在跑 ⇒ 后台任务运行中 · `busy`；
/// 活着、那一家没说在干什么（`None`）⇒ 运行中 · `now`（出口照画，不自己补一种默认）。
/// 序：等人 0 · 在干活 1 · 后台任务运行中 2 · 闲着 3 · 说不清 4（要人操作的先看）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ActivityFace {
    #[serde(skip_serializing_if = "Option::is_none")]
    activity: Option<SessionActivity>,
    activity_text: Words,
    activity_tone: Tone,
    activity_order: u8,
}

impl ActivityFace {
    /// 那一态 ⇒ 这几格。
    pub(crate) fn of(a: Option<SessionActivity>) -> Self {
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
            None => (
                copy_core::copy_text("beSession.activity.unclear", &[]),
                Tone::Now,
                4,
            ),
        };
        ActivityFace {
            activity: a,
            activity_text: Words(text),
            activity_tone: tone,
            activity_order: order,
        }
    }

    /// 那一态（说不清 ⇒ `None`）。
    pub(crate) fn activity(&self) -> Option<SessionActivity> {
        self.activity
    }

    /// 写好的字与语气（轮换那一侧的会话状态照抄这两格）。
    pub(crate) fn words(&self) -> (Words, Tone) {
        (self.activity_text.clone(), self.activity_tone)
    }
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

/// `hello.homes` 的一项（类型住 agent 注册表那一侧，帧面只引用它）。
pub use crate::agents::AgentHome;
pub use crate::agents::SessionActivity;
use crate::common::cells::{Tone, Words};
pub use crate::stream::topic::Topic;

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
    /// 哪条命令 —— 取值空间与 `Hello.commands` **同一套**（`inbound::command_names`）。
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

/// 后端发给客户端的一帧；`kind` 是标签，例如 `{"kind":"hello","v":1,...}`。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Frame {
    /// Handshake sent once when a client connects.
    Hello {
        /// 协议版本号。
        v: u32,
        /// 这份后端二进制的身份（`BUILD_ID`）；换不换后端按它判。
        build_id: String,
        /// 这台机器的架构（`x86_64` / `aarch64` …）。
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
        /// 🔴 **名单只有两格**、常驻开关不在名单里（`wire_tests.rs::hx2_the_echo_list_is_exactly_the_two_handed_names` 钉）——
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
    /// 会话记录里的一行 —— 带的是**成品**：这一行在界面里是什么（`record`，通用记录 `agents::record::Record`；
    /// 缺 ＝ 不进界面、照占号）与它自己的 `cwd`。解释住后端适配层，monitor 只原样转交。
    /// 原文 `raw` 是过渡格，只给发了 `--with-raw` 的客户端（逐字节等于记录里那一行，去掉行尾：`\n`，CRLF 行连 `\r` 一起去）；
    /// 两个前端读的是 `record`，手机那一侧的缺格补齐之后 `raw` 删。
    ///
    /// **两个前端共同的契约面**：`session_id` · `path` · `seq` · `byte_offset` · `raw` 五格在冻结表里（`wire_tests::the_shapes_the_second_frontend_reads_stay_put`）；
    /// `record` 是格目录里冻结的成品，格只许加（`cells_catalog_tests::the_golden_is_what_the_command_writes`）。`raw` 逐字节等于那一行
    /// （`watcher_tests::line_raw_is_the_record_line_byte_for_byte`）。
    Line {
        /// 会话 id。
        session_id: String,
        /// 这份会话记录在那台机器上的绝对路径。
        path: String,
        /// 这一行在本条流里的序号（按文件单调递增）；不是续传键，续传用 `byte_offset`。
        seq: u64,
        /// 这一行的通用记录；缺 ＝ 不进界面、照占号。
        #[serde(skip_serializing_if = "Option::is_none")]
        record: Option<crate::agents::record::Record>,
        /// 这条记录自己的工作目录。
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        /// backend-01（gap#2，additive 不 bump PROTO_VERSION）：本行末尾（含 `\n`）在文件中的**累计原始字节 offset**——
        /// 语义**逐字节对齐 aterm `LineFramer.endOffset`**：计 CRLF 的 `\r`、含 `\n`、残行不计；resume N ⇒
        /// `tail -c +(N+1)`。给 offset 续拉/截断检测（`seq` 是 per-stream 序数、非 resume 键）。
        /// 注：`Frame` 仅 derive `Serialize`，故此 `#[serde(default)]` 在**本 crate 装饰性**。
        ///
        /// 两个前端都读它：monitor 记续点（`stream_source/batch.rs` 的 `"line"` 分支 · `snapshot_resume.rs`），
        /// 第二个前端拿它续拉（冻结，`wire_tests::the_shapes_the_second_frontend_reads_stay_put`）。
        #[serde(default)]
        byte_offset: u64,
        /// 这一行的对账键（适配层 `RecordFace::response_id` 给；流的「开始」带同一个值）。没有 ⇒ 不上线。
        #[serde(skip_serializing_if = "Option::is_none")]
        rid: Option<String>,
        /// 〔additive〕这一行记录的**原文**（去掉行尾：`\n`，CRLF 行连 `\r` 一起去）。只在客户端发了 `--with-raw` 时才带
        /// （`ReaderState::with_raw`）。**过渡格**：两个前端都读核心的成品（`record`）；手机那一侧还缺的几格补齐之前它点着这一格，
        /// 补齐（缺格清单清零）之后从它的声明里摘掉、这一格随之删。没索要的客户端收到的字节与本字段加进来之前一字不差。
        #[serde(skip_serializing_if = "Option::is_none")]
        raw: Option<String>,
    },
    /// A new session file appeared.
    ///
    /// 附带 pidfile 元信息（`cwd` · `name` …）；缺的格不上线。
    SessionAdded {
        /// 会话 id。
        sid: String,
        /// DG3（#2D，additive）：会话属哪 agent kind——`"codex"`（Codex 会话）。Claude 会话**省略**
        /// （skip_if_none）→ 消费侧缺=claude（向后兼容、旧后端无此字段）。
        #[serde(skip_serializing_if = "Option::is_none")]
        agent_kind: Option<String>,
        /// DG3（#2D，additive）：判活置信度——`"heuristic"`（Codex 无 pidfile、mtime/proc 启发）。
        /// Claude（pidfile 权威）**省略**（skip_if_none）→ 消费侧缺=authoritative（向后兼容）。
        #[serde(skip_serializing_if = "Option::is_none")]
        liveness_confidence: Option<String>,
        /// 是不是后台会话（不是人坐在终端里对话的那种）。适配层判（`agents::pidfile_background`），客户端只读这一格。
        /// 只在 `true` 时上线（缺 ＝ 交互会话；交互会话的帧字节与本字段加进来之前一字不差）。
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        background: bool,
        /// **E73（additive）：attach 进去对人有没有意义。**
        ///
        /// pidfile 的会话种类把两件事压在一个轴上：①「该不该在 UI 出现」②「是不是一个人
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
        /// pidfile 里的会话名。
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
        /// 宣告时此刻在干什么（适配层翻好的那一态 · 写好的字 · 语气 · 监控板的序，[`ActivityFace`]，平铺在帧上）。
        #[serde(flatten)]
        face: ActivityFace,
        /// 宣告时在等什么（同 `session_status`）。
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
        /// 它在 ↗ 点那一刻从这个 pid 往上找窗口（`bind::bring_local_window`）只能从这一格拿 pid。
        ///
        /// 只在客户端发了 `--with-pid` 时才带（`ReaderState::with_pid`；只有本机那条流发）—— 没索要的客户端
        /// 收到的字节与本字段加进来之前一字不差（仓外 aterm 那份按精确字节对的 fixture 因此不受影响，hello 也不变）。
        #[serde(skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
    },
    /// Batch9-F27：会话 status 变化（pidfile modify diff；CC 仅状态转换时重写，
    /// 天然稀疏）。远端红绿灯数据源；旧 monitor 未知 kind 忽略（additive）。
    SessionStatus {
        /// 会话 id。
        sid: String,
        /// 此刻在干什么（同 `session_added` 那几格，[`ActivityFace`]）。
        #[serde(flatten)]
        face: ActivityFace,
        /// 在等什么（pidfile 里的 `waitingFor`）。
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
    SessionState {
        /// 会话 id。
        sid: String,
        /// 离开「活」之后的去向。
        state: SessionFate,
        /// 那一种写好的短名（[`SessionFate::cells`]）。
        state_text: Words,
        /// 悬停那一句。
        state_hint: Words,
        /// 语气。
        state_tone: Tone,
    },
    /// A session file went away.
    SessionRemoved {
        /// 会话 id。
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
    /// 发一帧；手机端（`src/mobile`：`TurnEndDebouncer.kt` 的 rolling-latest + debounce(1200ms) ＋ `SshKeepAliveService.kt` 的
    /// `baselineByPath`）塌合同 turn 的多记录、
    /// 首见吞历史不通知、offset 续拉重放 uuid ≤ 基线不通知（**transport-agnostic、与 β 逐字同语义、
    /// gap#6 闭**；backend 不猜消息边界）。`uuid` = 完成记录**顶层 uuid** = 客户端 dedup 键。
    /// **不带 `byte_offset`**（只 Line 带）——α watcher：Line 推 currentOffset、TurnEnd 喂 rolling
    /// current，结算时 baseline+offset 同段提交。旧 monitor 未知 kind 忽略（additive）。
    TurnEnd {
        /// 会话 id。
        session_id: String,
        /// 这一轮最后那条 assistant 记录的 uuid。
        uuid: String,
    },
    // 这里原是 `TmuxSessions`（B2 · P1：`tmux ls` 原文 ＋ 观测取值）与 `TmuxSessionClosed`（P5：差分出的
    //   正向死亡）两帧。会话账本进后端之后，客户端只收成品（`SessionState`），这两份原料再没有线上读者（monitor 已不消费；
    //   手机端 `DaemonTransport.kt::parseFrame` 按未知 kind 跳过）⇒ 删。快照只喂 `observe::session_ledger`。
    /// The bounded frame channel back-pressured and the reader had to drop
    /// `dropped` frames (a slow/wedged SSH pipe). Emitted once when the channel
    /// drains enough to accept it, so the client can warn the user that live
    /// lines were lost (#32). `dropped` counts frames dropped since the last
    /// overflow signal.
    Overflow {
        /// 自上一次哨兵以来丢掉的帧数。
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
        /// 回显请求的 `id`。
        id: String,
        /// 成功与否。
        ok: bool,
        /// 失败时的码（协议级或命令级）。
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        /// 失败时给人看的那一句（不含下层原话与码：那些进 `detail`）。
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// 失败时「复制详情」那几行（句子下面的「项名：值」：时刻 · 机器 · 命令 · 码 · 原话），后端写好、界面原样复制。成功时省略。
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        /// U6b-3：命令的返回值（如 `resolve` 的 CommandPlan）。无返回值的命令省略。
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
    },

    /// U6b-1：某个在跑的命令**已被取消**。取消是一条普通命令（`cmd:"cancel"`）、不是带外信号——
    /// 带外要么另开通道要么发明转义序列，两者都要新的解析纪律，而取消排队等一下并无妨。
    Cancelled {
        /// 被取消的那条命令的 `id`。
        id: String,
    },

    /// **这台的某样东西变了，客户端重读那一份**（账号清单 · 配置文件 · 额度账 · 会话轮换 · 规则表 · 计划 · 任务清单 —— 主题表 [`Topic`]）。
    ///
    /// 「X 变了 ⇒ 重读」只这一种帧：`topic` 说是哪一样；`key` 说是哪一个（会话 · 工作区，主题表写着带不带）；`rev` 说变成了哪一版；
    /// `body` 是那一样的小成品（主题表写着带什么、上限多少；超了就不带）。客户端没拿到 `body` 就重问主题表里那条查询（那一份的唯一出口仍是那条查询）。
    /// 可不可丢按主题（主题表的 `lossy`）：可丢的走 tap 那条，不可丢的走 watcher 的出方向。只经 [`Frame::changed`] 造。
    Changed {
        /// 哪一样变了。
        topic: Topic,
        /// 哪一个（会话 id · 工作区根）；主题不带 ⇒ 缺。
        #[serde(skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        /// 变成了哪一版；主题不带 ⇒ 缺。
        #[serde(skip_serializing_if = "Option::is_none")]
        rev: Option<String>,
        /// 那一样的小成品；主题不带或超了上限 ⇒ 缺（客户端重问）。
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<serde_json::Value>,
    },

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
    SessionFileGone {
        /// 会话 id。
        session_id: String,
        /// 不见了的那份记录文件。
        path: String,
    },

    /// **活会话的记录文件被改过了，已从头重读**（截短 · 或游标之前被原地改写）。
    ///
    /// 紧跟在这一趟重读出来的 `line` 帧**之前**（同一个 sink、同一条线程）。重读的行号从 0 重数
    /// （seq ＝ 当前文件里的行号，[`SeqCounter::restart`]）⇒ 下游据这一帧把这个会话旧的一代整份作废：monitor 丢留存与续点，
    /// 前端 tab 整份重来。
    SessionFileReread {
        /// 会话 id。
        session_id: String,
        /// 被改过、已从头重读的那份记录文件。
        path: String,
        /// 为什么从头重读。
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
    LinkData {
        /// 链路 id（`link-open` 时客户端给的）。
        link: String,
        /// 下行字节，标准 base64（解码后 ≤ 32 KiB）。
        data: String,
    },

    /// **一条链路不会再有字节了**；后端已经忘掉这个 `link` id。
    ///
    /// `error` 省略 = 正常收尾（用法那一段自己结束了：远端断了 / capture 收全了 / 转发收工了）；
    /// 带上 = 非正常收尾（下行泵写不出去 · 连接表被拆），那句人话原样给调用方。
    /// 拨不通**不**走这里 —— 那是链路字节里那一行失败的 ack（与 C2 同形），本帧随后照常到。
    LinkEnd {
        /// 链路 id。
        link: String,
        /// 非正常收尾的原话；缺 ＝ 正常收尾。
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
        /// 传输单 id。
        id: String,
        /// 已传字节。
        got: u64,
        /// 总字节。
        total: u64,
        /// 只在最后一帧：这一趟怎么收场的。
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
        /// 测试连接那一趟的票（`remote-probe` 交来的）。
        ticket: String,
        /// 一格进度，恰好一个键：`stage` · `reached` · `end`。
        cell: serde_json::Value,
    },

    /// **中转抄出来的一个 SSE 事件**（或一个响应的收尾）。
    ///
    /// 常驻后端会发（中转住在它进程里，`relay::host`；本机远端同形）。
    /// 四样东西，**没有业务词**（字段名就是「tee 线上字段名住哪」的答案：住这里，serde 名）：
    /// - `stream`：agent 自己请求头里带的会话标识（头名由适配层登记；== 它的 sid，新开 / resume / 分叉同一形）；没带 ⇒ 空串。后端不解释它。
    /// - `resp`：本进程第几个响应（跨响应单调，后端重启从 0 起）。
    /// - `n`：这一个响应里第几个事件，**从 0 连续**。后端每个事件先占号再投递 ⇒ 丢了的号不出现 ⇒
    ///   接收侧看 `n` 连不连得上就知道缺在哪（`Gap{from_seq,to_seq}` 那一形，原位、纯算术）。
    /// - `data`（与 `end` 恰有一个）：SSE `data:` 后面那段原文，**一个 JSON 串**（上游字节敌手可控，不参与帧结构）。
    /// - `end`：这个响应不会再有事件了（`done` 上游说完 · `broken` 转发以错误收尾）；这一帧的 `n` = 一共占了几个号。
    ///
    /// 🔴 **可丢**：走后端自己那条有界 tap 通道（`tap::TAP_CAPACITY`），满了就丢，不回推中转、不挤出方向的内容帧。
    /// SSE 只保快，jsonl 保对。旧 monitor / 仓外 aterm 不认这个 kind ⇒ 忽略（additive）。
    Tap {
        /// 请求自带的会话标识头的值。
        stream: String,
        /// 这段流归哪个子运行（主运行 ⇒ 不上线）。由后端归位（`run_route`）：请求自报了且不等于 `stream` 就定（等于 ⇒ 主运行）；没自报 ⇒ 没有在跑的子运行就归主，
        /// 有 ⇒ 先挂起、等记录对上对账键再放出来。
        #[serde(skip_serializing_if = "Option::is_none")]
        run: Option<String>,
        /// 本进程第几段。
        resp: u64,
        /// 这一段里第几件，从 0 连续。
        n: u64,
        /// 归一事件（上游原始事件已在后端按协议面折过；界面不认任何一家的事件名）。
        #[serde(skip_serializing_if = "Option::is_none")]
        ev: Option<crate::agents::StreamEv>,
        /// 这一段的收尾（与 `ev` 恰有一个）。
        #[serde(skip_serializing_if = "Option::is_none")]
        end: Option<TapEnd>,
    },
    /// 一个会话的运行表（主运行之外的那几个子运行：标签 · 状态 · 最近一件事 · 派出它的那次工具调用）。表变了就整份发一次；
    /// 按最近一次动静排（最早动过的在前）。`ended`：被挤出运行表的已收场子运行（对上了派出调用的那些，先挤出的在前）——
    /// 派出它们的那几张卡照样标得上终态。
    SessionRuns {
        /// 会话 id。
        sid: String,
        /// 在表里的子运行（整份）。
        runs: Vec<RunInfo>,
        /// 被挤出表的已收场子运行。
        ended: Vec<RunEnded>,
    },
    /// 一份会话记录的**主线外清单**（用户回退重发后留在文件里的旧那一支）：那几条记录的 `id`（与记录成品的 `id` 同值；只含进界面的，文件序）。
    /// 整份、变了才发（宣告之后历史里已经有就发一次）；从没发过 ＝ 空。冷读那一路是读命令 `history-branch`。这一家的记录没有链 ⇒ 从不发。
    SessionBranch {
        /// 会话 id。
        sid: String,
        /// 这份会话记录在那台机器上的绝对路径。
        path: String,
        /// 主线外那几条的 `id`。
        off: Vec<String>,
    },

    /// **一个终端此刻的一整屏**（终端实时预览，`control/terminal_follow.rs`；`terminal-follow` 之后才出现）。
    ///
    /// `view` 与 `terminal-preview` 的回话**同一份成品**（`lines` 带颜色段 · `screen` 指纹 · 尺寸 · 光标 · 抓的时刻）。
    /// 每帧整屏、不是增量；一帧在途：客户端 `terminal-follow-ack {ticket, seq}` 之后才推下一帧，画面没变不推。
    /// 一帧的 `view` 序列化后至多 `terminal_follow::SCREEN_FRAME_CAP` 字节，超了不推、改推 [`Frame::TerminalFollowEnd`]（`too_big`）。
    /// 🔴 **不丢**：走应答那条独立通道（中间那几屏本来就被合并掉，丢了在途那一帧订阅就停住）。旧客户端不认 ⇒ 忽略（additive）。
    TerminalScreen {
        /// 订阅票（`terminal-follow` 时客户端给的，本后端只当不透明的串回填）。
        ticket: String,
        /// 这条订阅里第几帧（从 1 连续）。
        seq: u64,
        /// 那一屏（同 `terminal-preview` 的回话）。
        view: serde_json::Value,
    },

    /// **一条终端订阅停了**（不会再有画面）；后端已经忘掉这张票。客户端自己退订的不发这一帧。
    /// 🔴 **不丢**：走应答通道。旧客户端不认 ⇒ 忽略（additive）。
    TerminalFollowEnd {
        /// 订阅票。
        ticket: String,
        /// 为什么停了（给程序认）。
        why: FollowEnd,
        /// 给人看的那一句（后端写好，界面原样上屏）。
        said: String,
    },
}

/// 一条终端订阅为什么停了（[`Frame::TerminalFollowEnd`] 的 `why`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct RunEnded {
    pub run: String,
    pub tool: String,
    pub state: RunState,
}

/// 一个子运行此刻的样子（[`Frame::SessionRuns`] 的一项）。三个时刻是自 1970 起的毫秒（记录自己写着的时刻；记录里没写 ⇒ 读到它的时刻）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
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
    /// 派出它的那个子运行；主运行派的（或还没对上派出它的那次调用）⇒ 不上线。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub parent: Option<String>,
    pub state: RunState,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub last: Option<crate::agents::RunDid>,
    /// 在等哪个工具的结果（它最近一条记录是一次还没拿到结果的工具调用、且还在跑）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub waiting: Option<String>,
    /// 开始：派出它的那条记录（没见到 ⇒ 它自己最早的一条）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub started_ms: Option<u64>,
    /// `started_ms` 在这台本地钟上的钟面 `HH:MM`（跟着 `started_ms` 一起写；界面照抄、不换算）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub started_text: Option<String>,
    /// 最近动静：它自己最近一条记录。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub active_ms: Option<u64>,
    /// 收场：说它收场的那一条（还没收场 / 状态不明 ⇒ 不上线）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub ended_ms: Option<u64>,
    /// 为什么是这个结局（在跑 ⇒ 不上线）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub why: Option<RunWhy>,
    /// 失败收场时的报错原话（说得出才有；至多 `observe::runs::ERROR_CHARS` 个字）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub error: Option<String>,
    /// 它调了几次工具（它自己的记录里数的；零 ⇒ 不上线）。
    #[serde(skip_serializing_if = "is_zero")]
    #[cfg_attr(test, ts(optional, as = "Option<u32>"))]
    pub calls: u32,
    /// 派出那一方没等它、接着做自己的事（后台派出；否则不上线）。
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub background: bool,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// 子运行的五态。收场的三态以派出那一方说的为准（子记录自己写出终局也算，先到先算）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
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
    /// 失败（那一句 ＋ 复制详情）。上传那一路的暂存件**留着**给续传；下载那一路的 `.part` 删了。
    /// `code`：调用方要按它换路的那几形（今天只有 `sftp_home_mismatch`）；缺席 ＝ 一般的失败。
    /// `detail`：复制详情那几行（时刻 · 机器 · 命令 · 下层原话），`why` 只带原因词；缺席 ＝ 没写。
    Failed {
        why: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// 撤了（`transfer-stop` / 本机流断了）。上传那一路的暂存件已删；下载那一路的 `.part` 留着。
    Cancelled,
}

impl TransferEnd {
    /// 跑起来之后才停下的那一形：那一句 ＋ 复制详情（命令 `cmd` · 下层原话，后端这一端写好）。
    pub(crate) fn failed(
        why: String,
        code: Option<String>,
        cmd: &str,
        raw: Option<&str>,
    ) -> TransferEnd {
        TransferEnd::Failed {
            why,
            code,
            detail: Some(crate::stream::detail::of_run(cmd, raw)),
        }
    }
}

impl Frame {
    /// 成功、不带数据的那一种应答。
    pub(crate) fn ok(id: &str) -> Frame {
        Frame::Reply {
            id: id.to_string(),
            ok: true,
            code: None,
            message: None,
            detail: None,
            data: None,
        }
    }

    /// `changed` 帧（只此一处造）：`body` 序列化后超了主题表那一格的上限 ⇒ 不带（客户端重问）；主题不带 `body` 的给了也不带。
    pub(crate) fn changed(
        topic: Topic,
        key: Option<String>,
        rev: Option<String>,
        body: Option<serde_json::Value>,
    ) -> Frame {
        let cap = topic.spec().body_cap;
        let body = body.filter(|b| serde_json::to_vec(b).is_ok_and(|v| v.len() <= cap));
        Frame::Changed {
            topic,
            key,
            rev,
            body,
        }
    }

    /// `session_state` 帧：去向 ＋ 它写好的字与语气（只此一处造这一帧）。
    pub(crate) fn session_state(sid: String, state: SessionFate) -> Frame {
        let (state_text, state_hint, state_tone) = state.cells();
        Frame::SessionState {
            sid,
            state,
            state_text,
            state_hint,
            state_tone,
        }
    }

    /// 失败应答：那一份失败（[`crate::stream::detail::Failed`]，CLI 面的信封出自同一份）装进 `reply`。
    pub(crate) fn failed(id: String, f: crate::stream::detail::Failed) -> Frame {
        Frame::Reply {
            id,
            ok: false,
            code: Some(f.code),
            message: Some(f.message),
            detail: Some(f.detail),
            data: f.data,
        }
    }

    /// 一条命令当场失败、带下层原话的那一种应答（原话进复制详情，句子 `message` 里没有它）。
    pub(crate) fn err_raw(id: &str, cmd: &str, code: &str, message: &str, raw: &str) -> Frame {
        let f = crate::stream::detail::Failed::new(
            Some(cmd),
            code,
            message.to_string(),
            Some(raw),
            None,
        );
        Frame::failed(id.to_string(), f)
    }

    /// 硬臂那几条命令（`Run::Builtin`：链路 · 传输 · 终端订阅）就地被拒：详情里带命令名（同处理器回的失败）。
    pub(crate) fn refused(id: &str, cmd: &str, code: &str, message: &str) -> Frame {
        let f =
            crate::stream::detail::Failed::new(Some(cmd), code, message.to_string(), None, None);
        Frame::failed(id.to_string(), f)
    }

    /// 协议级失败（还没落到哪条命令上：认不出 · 读不懂 · 收场中）：详情里没有命令那一项。
    pub(crate) fn err(id: &str, code: &str, message: &str) -> Frame {
        let f = crate::stream::detail::Failed::new(None, code, message.to_string(), None, None);
        Frame::failed(id.to_string(), f)
    }

    /// **丢了还能不能恢复**。
    ///
    /// # 这条判据是对的，此前错的是「它没有被应用到出方向」
    ///
    /// 仓里对同一类问题已经推理过一次，而且完全正确 —— 只是给 `reply` 帧做的：
    /// `stream/inbound/mod.rs` 让应答走**独立通道**且用 `.send().await` 阻塞背压，
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
            // 「X 变了」：按主题表答。可丢的那几样在盘上（重问随时拿得到）、走 tap 那条；
            //   不可丢的（账号清单 · 配置文件 · 任务清单）是一次变化的通知，没有「下一次必然重发」⇒ 丢了按身份报，客户端重问。
            Frame::Changed { topic, .. } => topic.spec().lossy,
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
            // 同上：整份快照，下一次变就整份重发。
            Frame::SessionBranch { .. } => false,
            // 终端实时预览：丢了在途那一帧，客户端等不到它就不回执，订阅停住；也走应答通道。
            Frame::TerminalScreen { .. } => false,
            Frame::TerminalFollowEnd { .. } => false,
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
            Frame::Changed { topic, key, .. } => (
                "changed",
                Some(match key {
                    Some(k) => format!("{}/{k}", topic.name()),
                    None => topic.name().to_string(),
                }),
            ),
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
            Frame::SessionBranch { sid, .. } => ("session_branch", Some(sid.clone())),
            Frame::TerminalScreen { ticket, .. } => ("terminal_screen", Some(ticket.clone())),
            Frame::TerminalFollowEnd { ticket, .. } => {
                ("terminal_follow_end", Some(ticket.clone()))
            }
        };
        LostFrame { kind, subject }
    }
}

/// base64 的字母表（RFC 4648 §4，标准字母表、带 `=` 补位）。
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 不在字母表里的字节在 [`B64_DECODE`] 里的值。
const NOT_B64: u8 = 0xff;

/// 字节 ⇒ 它在 [`B64`] 里的位置（不在 ⇒ [`NOT_B64`]）：解码每个字符查一次表。
const B64_DECODE: [u8; 256] = {
    let mut t = [NOT_B64; 256];
    let mut i = 0;
    while i < 64 {
        t[B64[i] as usize] = i as u8;
        i += 1;
    }
    t
};

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
    let val = |c: u8| -> Option<u32> {
        (B64_DECODE[usize::from(c)] != NOT_B64).then(|| u32::from(B64_DECODE[usize::from(c)]))
    };
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
/// {"id":"<opaque>","cmd":"<name>","args":{...},"within_ms":10000,"view":{"omit":{"record":["blocks[type=tool_use].input"]}}}
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
    /// 分派那一层由 `within_ms` 减余量换成的截止时刻（不上线）。
    #[serde(skip)]
    pub(crate) until: Option<crate::platform::child::Until>,
}

/// `within_ms` 的宽读：只认正整数，别的（字符串 · 负数 · 小数 · 零 · null）一律当没带。
fn lenient_ms<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(within_ms_of(&v))
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
pub(crate) mod tests;
