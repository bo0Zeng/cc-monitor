//! **拨号的线上形状**：拨号核心（`ssh-dial`）写、读它的那一侧（手机 Kotlin）读的那几样，两头同一份。
//!
//! 一条链路上 ack 之前只有一种行 `{"stage":{…}}`（[`StageRow`]：拨号阶段或「过了一步」，同一族 [`Stage`]），然后恰好一行 ack（[`DialAck`]），
//! 之后按用法（`capture` 那一形是一行 [`Captured`]）。纯数据：怎么拨、详情怎么写住拨号核心。

use serde::{Deserialize, Serialize};

use crate::machine::Why;

/// ack 里的协议版本：v2 = 竞速 · 跳板 · agent · 几种用法 · 阶段行。多一种用法不 bump 它（新界面凭 `uses` 认出老后端），
/// 版本号只在同一种用法的字节形状变了时才动。
pub const ACK_V: u32 = 2;

/// 拨号代理认得的用法 —— ack 的 `uses` 原样回这张表，读的那一侧据它判「代理够不够新」。
pub const USES: &[&str] = &["stream", "capture", "forward", "shell", "hold"];

/// capture 那一形的收工记号：不认一次性子命令的老后端会把它当未知旗标、直接进流模式、第一行发 hello ——
/// 收全的一侧见到它就提前收（不让它装 hook、不挂住），读的一侧当「太旧、要更新」。后端问远端 · 桌面与手机的重连循环同读这一份。
pub const OLD_BACKEND_MARKER: &str = "\"kind\":\"hello\"";

/// 一个拨号地址。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

/// ack 之前那一行（[`StageRow`]）的值：拨号又过了一步，或者一条拨号阶段。阶段取 russh 能干净观测的粒度 —— 不含 KEX（`HostKey` 触发即隐含 TCP ＋ KEX 已过）。
/// 带地址的那几支：`endpoint` 是结构化的那一台，`said` 是拨号核心照这一支写好的那一句（地址怎么称呼 · 原因都在句子里），读的那一侧照摆。
/// 只经 `ssh_dial::stage` 那几个构造函数造（字在那里写）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Stage {
    /// 拨号又过了一步（`tcp` · `banner` · `hostkey` · `auth` · `tunnel` · `channel`，经跳板时每一台各走一遍）。
    /// 当场就写，不攒；读的那一侧只拿它重新计时，不按名字分支，也不上屏。
    Step { step: String },
    /// 某地址开始拨号（TCP ＋ 握手）。
    Dialing { endpoint: Endpoint, said: String },
    /// 某地址握手到 host key 校验（句子里带指纹）。
    HostKey { endpoint: Endpoint, said: String },
    /// 某地址连接失败（句子里带粗分类 ＋ 原始错误）。
    Failed { endpoint: Endpoint, said: String },
    /// 竞速胜出地址（其余在飞的已丢弃）。
    Won { endpoint: Endpoint, said: String },
    /// 鉴权结果（没登上时句子里带原因词；原话随 ack 走）。
    Auth { ok: bool, said: String },
    /// 连接就绪（握手 ＋ 鉴权全过）。
    Established { said: String },
}

/// ack 之前的一行：`{"stage":…}`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct StageRow {
    pub stage: Stage,
}

/// 拨号代理回的那一行 ack。读的那一侧对老代理宽容：除 `ok` 之外的格缺席都按缺省读。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct DialAck {
    pub ok: bool,
    /// `ok=false` 时的那一句（只带原因词，不带下层原话）。
    #[serde(default)]
    pub said: Option<String>,
    /// `ok=false` 时的复制详情（时刻 · 机器 · 命令 · 下层原话）。成功 ⇒ 不出这一格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub detail: Option<String>,
    /// 实际观察到的 host key 指纹（展示用；失败时也尽量带上）。
    #[serde(default)]
    pub fingerprint: Option<String>,
    /// 竞速胜出的地址（`host:port`，给人看）。
    #[serde(default)]
    pub endpoint: Option<String>,
    /// 同一条胜者，结构化 —— 记 last-good、下一趟当 `prefer` 交回来用它（地址不在读的那一侧解析）。失败 ⇒ `None`。
    #[serde(default)]
    pub winner: Option<Endpoint>,
    /// 没拨成时的原因码；拨成了 / 说不清 ⇒ `None`。
    #[serde(default)]
    pub reason: Option<Why>,
    /// 目标那一台握手时给的 host key 类型。没握到那一步 ⇒ 不出这一格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub key_type: Option<String>,
    /// 「钥匙变了」时库里同类型那一把的指纹（没有 ⇒ 空串）。没因钥匙被拒 ⇒ 不出这一格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub expected_fingerprint: Option<String>,
    /// 协议版本（[`ACK_V`]；v1 代理没有这一格 ⇒ 0）。
    #[serde(default)]
    pub v: u32,
    /// 这个代理认得的用法（[`USES`]）。
    #[serde(default)]
    pub uses: Vec<String>,
    /// 没够着那一台（[`unreached`]：解析 · 拨不通 · 到期限一步都没动 · 跳板那一台断了）：读的那一侧照它分「那台不在」与别的失败，不自己按原因码判。
    #[serde(default)]
    pub unreached: bool,
}

/// 没够着那一台的那几个原因码（拨号的 [`Why`]）。
pub fn unreached(why: Option<Why>) -> bool {
    matches!(
        why,
        Some(Why::Resolve | Why::Unreachable | Why::Timeout | Why::Jump)
    )
}

impl DialAck {
    /// 开成了：看到的指纹 · 胜出的地址（给人看的一串 ＋ 结构化的一份）。
    pub fn opened(fingerprint: Option<String>, endpoint: String, winner: Endpoint) -> Self {
        DialAck {
            ok: true,
            said: None,
            detail: None,
            fingerprint,
            endpoint: Some(endpoint),
            winner: Some(winner),
            reason: None,
            key_type: None,
            expected_fingerprint: None,
            v: ACK_V,
            uses: uses(),
            unreached: false,
        }
    }

    /// 没开成的那一份写成线上那一行。
    pub fn of_fail(f: DialFail) -> Self {
        DialAck {
            ok: false,
            said: Some(f.said),
            detail: f.detail,
            fingerprint: f.fingerprint,
            endpoint: None,
            winner: None,
            reason: f.reason,
            key_type: f.key_type,
            expected_fingerprint: f.expected_fingerprint,
            v: ACK_V,
            uses: uses(),
            unreached: unreached(f.reason),
        }
    }
}

fn uses() -> Vec<String> {
    USES.iter().map(|u| (*u).to_string()).collect()
}

/// 没开成：线上就是一行 `ok:false` 的 ack（[`DialAck::of_fail`]），进程内的调用方直接拿这一份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialFail {
    /// 那一句（只带原因词，不带下层原话）。
    pub said: String,
    /// 复制详情（时刻 · 机器 · 命令 · 下层原话）。
    pub detail: Option<String>,
    /// 握手时看到的 host key 指纹（失败时也尽量带上）。
    pub fingerprint: Option<String>,
    /// 原因码；说不清 ⇒ `None`。
    pub reason: Option<Why>,
    /// 目标那一台握手时给的 host key 类型（没握到那一步 ⇒ `None`）。
    pub key_type: Option<String>,
    /// 「钥匙变了」时库里同类型那一把的指纹（没有 ⇒ 空串；没因钥匙被拒 ⇒ `None`）。
    pub expected_fingerprint: Option<String>,
}

/// `use: capture` 的结果行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Captured {
    pub stdout: String,
    pub stderr: String,
    /// `None` = 远端没送 exit-status（连接被掐 / 服务端不守规矩）。**不许把 `None` 当成 0**。
    pub exit_status: Option<u32>,
}

// ───── 拨号请求（界面 · 手机 Kotlin 交给拨号核心的那一份） ─────

/// 跳板那一台。它自己还要经别的跳板 ⇒ `jump` 再套一层（多跳，同 `ssh -J 外,内`；桌面的机器表今天只给一跳，手机给多跳）。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct JumpHop {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub key_path: Option<String>,
    /// 报错与阶段行里称呼它用的名字（界面那边的配置标签）。
    #[serde(default)]
    pub label: String,
    /// 这一台自己经哪台跳板到（再外一台；`None` = 本机直连它）。
    #[serde(default)]
    pub jump: Option<Box<JumpHop>>,
}

/// 开出 channel 之后怎么用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "DialUse", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum Use {
    /// exec `command`，stdin/stdout 与 channel 原样对拷（后端长连接流 · 一次性查询）。
    #[default]
    Stream,
    /// exec `command`，收全 stdout / stderr / 退出码，印一行 [`Captured`] 后退出。
    Capture,
    /// 本进程绑本机回环口、每接进一条连接开一条 direct-tcpip（端口转发）。
    Forward,
    /// 要一个 PTY（[`PtyOpts`]）开交互 shell，之后同 `stream` 原样对拷；改窗口大小走 `uses::Control`。
    Shell,
    /// 只握着这条连接（不开通道）：ack 之后链路不收工，同一身份的别的链路都复用它；对面走了（上行 EOF）
    /// 或连接没了（russh 报断开）⇒ 收工。手机那一形：「连上一台机器」就是开一条这个。
    Hold,
}

/// `use: shell` 的 PTY。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct PtyOpts {
    #[serde(default = "PtyOpts::default_term")]
    pub term: String,
    pub cols: u32,
    pub rows: u32,
}

impl PtyOpts {
    fn default_term() -> String {
        "xterm-256color".to_string()
    }
}

impl Default for PtyOpts {
    fn default() -> Self {
        PtyOpts {
            term: Self::default_term(),
            cols: 80,
            rows: 24,
        }
    }
}

/// `use: capture` 的参数。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct CaptureOpts {
    /// stdout / stderr 各自的收集上限（字节）。
    pub max_bytes: usize,
    /// stdout 一出现这个子串就收工（不认参数的老后端会掉进流模式、永不 EOF）。
    #[serde(default)]
    pub abort_marker: Option<String>,
    /// exec 之后原样写进远端进程 stdin 的字节（缺席 = 一个字节不写）。**不关 stdin** ——
    /// 收它的那一侧是后端 CLI 面的「只读一行」入口（`wire/cli_words.rs::STDIN_LINE_FLAG`），读到换行就动手。
    /// 有了它，载荷不必拼进命令行（那要求远端登录 shell 认 POSIX 引号与管道）。
    #[serde(default)]
    pub stdin: Option<String>,
}

/// `use: forward` 的参数：每接进一条连接开一条到哪儿的隧道。本机口不在这里 —— 宿主绑好交给拨号核心（`uses::run_forward`）。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct ForwardSpec {
    pub remote_host: String,
    pub remote_port: u16,
}

/// 界面交给代理的那份 JSON。蛇形键（不是界面配置那套 camelCase）：这条管子两端都是我们自己，不与给前端看的契约拴在一起。
/// v2 的字段全是可选的（老界面发来的请求照样读得动）。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct DialRequest {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// 钥匙那一格（不是私钥本体）：桌面是 OpenSSH 格式私钥的**路径**，手机是那台机器的钥匙编号；
    /// 由宿主的 `env::KeySource` 换成能鉴权的东西（钥匙 · 外部签名器 · 密码）。缺席 = 走 ssh-agent。
    pub key_path: Option<String>,
    /// 要 exec 的命令行（`stream` / `capture`；界面已 `shell_quote` 过；`forward` 不用它）。
    #[serde(default)]
    pub command: String,
    /// 竞速的地址顺序（组请求的那一侧按 `prefer` 排好）。空 = 只有 `host:port`。
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
    /// 经这台跳板（fail-closed：它断开就报错，不回落直连）。
    #[serde(default)]
    pub jump: Option<JumpHop>,
    #[serde(default, rename = "use")]
    pub use_: Use,
    #[serde(default)]
    pub capture: Option<CaptureOpts>,
    #[serde(default)]
    pub forward: Option<ForwardSpec>,
    /// `use: shell` 的 PTY（缺 ⇒ [`PtyOpts::default`]）。
    #[serde(default)]
    pub pty: Option<PtyOpts>,
    /// ack 之前逐行报阶段（测试连接那六格）。
    #[serde(default)]
    pub stages: bool,
    /// 短命探活（inactivity 拆链）而不是长连接（keepalive 保活）。
    #[serde(default)]
    pub probe: bool,
    /// ssh-agent 套接字的路径（Unix；界面进程此刻的 `SSH_AUTH_SOCK`）。要界面交过来：常驻后端活得比任何一个界面进程都长，
    /// 它自己环境里那份是第一个起它的界面给的，用户重新登录之后 agent 换了路径。缺席 = 用本进程自己的。
    /// 它是一条路径、不是凭据本体；不进连接池的身份（换 agent 不该换连接）。
    #[serde(default)]
    pub agent_sock: Option<String>,
    /// ack 之前每过一步当场写一行「过了一步」那一形阶段行（`{"stage":{"kind":"step",…}}`，`step`）。重连循环拿它判「这一拨还在动」（`resident-client` 的 `DIAL_WITHIN`）。
    #[serde(default)]
    pub progress: bool,
}

impl DialRequest {
    /// 竞速顺序：给了就用给的，没给就只有 `host:port`。
    pub fn race_order(&self) -> Vec<Endpoint> {
        if self.endpoints.is_empty() {
            vec![Endpoint {
                host: self.host.clone(),
                port: self.port,
            }]
        } else {
            self.endpoints.clone()
        }
    }
}
