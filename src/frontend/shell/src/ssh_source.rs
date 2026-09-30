//! SSH-remote 数据源（issue #15）。
//!
//! 本模块是**活代码**：从 lib.rs 的 `setup()` 调用（`remote.enabled=true` 且配置完整时）。
//! 它提供三块能力：
//! - **russh client 数据源**：[`run`] 连远端、exec backend、把 backend stdout 的
//!   line-delimited JSON 帧解析后，内容那一半交 [`LineIntake`]（`batch_to_payloads` →
//!   `on_line_batch_awaited`），会话起停的成品交 `session_book`（〔MIG-1〕）。与本机那条流
//!   **并行**作为附加数据源（远端行带 origin=host 标签）。
//! - 〔CF1 · 2026-09-24〕**本机会话内容的消费者** [`consume_local`]：本机常驻后端的内容帧经
//!   `local_lines` 送来，进**同一个** [`LineIntake`] —— 本机与远端走同一条帧路（`设计/01 §6.1`）。
//! - 〔MIG-1〕~~ssh-config 导入~~：搬进后端 `dial/ssh_config.rs`（`99 §2.1 ⑯`，monitor 从此不读 `.ssh`、不起 `ssh`）。
//! - **测试连接**：[`test_remote_connection`] 实连一次，回 SSH ✓/✗ + host key 指纹 +
//!   backend ✓/✗（hello），供 UI 分级展示 + TOFU→strict 指纹固化。
//!
//! 上述 `#[tauri::command]` 在 lib.rs 的 invoke_handler! 里注册。
//!
//! ## 🔴〔C2 · 2026-09-24，`设计/05 §13`〕本模块**不再拨 SSH**
//!
//! 上面第一条里的「russh client 数据源」今天是个旧名字：连远端、鉴权、开通道全在后端的拨号代理
//! （`src/backend/dial/`）；本模块拿链路只经宿主 `dial_host`（`connect_and_exec_cmd` /
//! `connect_and_exec_capture` / 测试连接），读代理应答的是通信层成员 `ssh_link`。
//! 本模块留下的是**业务**：远端流的帧解析与分派、会话 / tmux / idle 账本、旁路快照（续传见 `snapshot_resume`）、
//! ssh config 导入 —— 所以它不登记为通信层成员（理由住 `comm_boundary_registry_tests::TRANSPORT_LEFT_OUTSIDE`）。
//! 下面那段 crypto backend 的说明今天说的是后端拨号代理与 `inproc_dial.rs`（SFTP 那一份）用的 `russh`。
//!
//! ## Crypto backend 选择（S3 的核心风险点）
//!
//! russh 0.61 默认 crypto backend 是 `aws-lc-rs`，它在 windows-msvc 上构建需要
//! NASM（汇编器）+ 有时 cmake，CI / 开发机上常缺失导致整条依赖链编译失败。
//! 为规避这一构建风险，Cargo.toml 里用
//! `default-features = false, features = ["ring", "flate2", "rsa"]`
//! 切到 `ring` 后端（自带预编译/纯汇编路径，windows-msvc 无外部汇编器依赖）。
//! `flate2` / `rsa` 是 russh 默认开的非 crypto-backend feature，手动保留以维持
//! 与默认配置等价的功能面（压缩 + RSA key 支持）。

// S5 起本模块从 setup() 调用（remote.enabled=true 时）。run() / parse_frame /
// InboundFrame 都是活代码。
// 个别仅 S6+ 才读的字段（RemoteConfig 反序列化派生）保留 dead_code 容忍。

use crate::copy_table::copy_text;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::event_replay::EventReplay;
// 〔SR1b · 2026-09-24〕从前这里把 `connect_session` / `ClientHandler` 从 `inproc_dial.rs` 再导出给 `sftp.rs`
//   （界面进程里最后一份 russh 拨号，唯一调用方就是 SFTP）。SFTP 进了本机常驻后端，那份文件整份删了，这一行随之删。
// 〔MIG-1 · `99 §2.1 ⑬`〕会话起停的成品住 `session_book`（后端裁，monitor 只转交）；`session_removed.cause` 这一侧不再读
//   （那条双写点 `REMOVAL_CAUSE_SUPERSEDED`〔散文墓碑〕随「按 cause 裁可重连」搬进后端 `observe/session_ledger.rs`）。
use crate::session_book::{Fate, In as BookIn, LiveMeta};

/// 一行会话记录的成品 ＋ 它在那份文件里的行号（`seq`）—— 进 [`flush_lines`] 之前的形状。
///
/// 〔CF1 · 2026-09-24〕它原先住 monitor 自己的 jsonl watcher（`watcher.rs`，已删）。
/// 本机那条流改走后端的 `line` 帧之后，**所有**行都从后端的帧来（远端流 · 本机流 · 旁路快照），
/// 造它的只剩本模块 ⇒ 搬到这里。`seq` 是后端给的行号（`--tail-only` 下与快照同处一个行号空间），
/// 前端按 `(session_id, seq)` 去重、按 `seq` 排序（`INVARIANTS §5` / `§9`）。
#[derive(Debug, Clone)]
pub struct JsonlLine {
    pub session_id: String,
    pub path: std::path::PathBuf,
    pub seq: u64,
    /// 〔MOD · `设计/90 §3` 判据 3〕那台后端给的成品：这一行在渲染模型里的样子；`None` ＝ 不进界面（照占号）。
    pub message: Option<crate::ui_contract::RecordBody>,
    /// 这条记录自己的 `cwd`（后端给的）。
    pub cwd: Option<String>,
    /// 〔RENDER2 · `99 §2.1` ㊱②〕这一行之后（含它的 `\n`）那一个字节的偏移 = 下一行的起点（后端 `line.byte_offset` ·
    /// 快照的行区间末端）；说不准 ⇒ `None`。续点据它记「从哪个字节接着读」。
    pub end: Option<u64>,
}

/// 重连退避下界：每次连接掉线后至少等这么久再重连（也是连上过之后的快速重连值）。
const RECONNECT_MIN: Duration = Duration::from_secs(2);
/// 重连退避上界：指数退避封顶，避免长断网时无意义地拉长重连间隔。
const RECONNECT_MAX: Duration = Duration::from_secs(30);

/// ★ **「本次连接算不算健康」的最短存活时长**〔audit-0805 F05 / 报告 I-1〕。
///
/// # 为什么不能拿「收到过 hello」当健康
///
/// `connected` 是在**收到 backend hello 的那一刻**置位的（`stream_loop` 里那句
/// `connected.store(true)`）。于是一个「发完 hello 就死」的 backend —— 比如小机器上
/// 整读 jsonl 触发 OOM 被杀 —— 每一轮都算「连上过」⇒ 退避每次都被重置回 2 秒
/// ⇒ **永远不增长**。而每次重连要付 3 次完整 SSH 登录（arch 探测 / SFTP 预检 / 起流），
/// 折算约 **90 次握手/分钟/台**，正好压在那台已经撑不住的机器上。
///
/// ⇒ 判据换成「**这条连接活过了多久**」：hello 只说明握手成功，活过 30 秒才说明它真站住了。
///
/// ⚠ 30 秒的取法：要明显长于「起流 + 首批帧」的正常耗时（冷启动实测约 0.9 s @30ms RTT、
/// 约 6 s @200ms RTT，见 `ROADMAP §5`），又要短到不至于让一次真实的网络抖动被当成 flapping。
const MIN_HEALTHY_UPTIME: Duration = Duration::from_secs(30);

/// 纯函数：本轮连接结束后，退避该不该重置回 [`RECONNECT_MIN`]。
///
/// 两个条件**都要满足**：握手成功过（`saw_hello`）**且**这条连接活过 [`MIN_HEALTHY_UPTIME`]。
/// 只看前者就是 I-1 那个自激循环；只看后者会把「连了很久但从没握手成功」也当健康。
fn should_reset_backoff(saw_hello: bool, lived: Duration) -> bool {
    saw_hello && lived >= MIN_HEALTHY_UPTIME
}

/// 冷启动预检的**自证记忆**〔audit-0805 F05 下半，报告「可选 12」〕。
///
/// # 冷启动今天付三条 SSH 连接，其中两条常常是白付的
///
/// 实测（08-06 读码逐条对上）：
///
/// | # | 连接 | 谁发起 |
/// |---|---|---|
/// | ① | `uname -m` 一次性 exec（选内嵌二进制的 arch；〔DP1〕今天问 `uname -s -m`） | `byte_table::probe_key` |
/// | ② | SFTP 连接（读远端 `.build_id` marker） | `sftp::connect_sftp` |
/// | ③ | 接那台的常驻后端（`--resident-ensure` 一次 capture ＋ 隧道） | `remote_resident::attach` |
///
/// ①② 同属 `ensure_backend_deployed`。**即使远端已经是当前 build、什么都不用部署，
/// 每次重连也照付这两条**（各含一次 TCP + 握手 + 指纹校验 + auth）。
///
/// # 记什么：hello **自报**的 build_id，不是预检算出来的结论
///
/// 报告的原提法是「把 `arch`/`build_id` 记进 memo」。**记预检结论是猜，记 hello 是自证** ——
/// 只有后端自己说「我是 D」才写进来。于是这份记忆的含义是
/// 「**这台机器上一次真的跑起来的后端就是当前期望的那个**」，
/// 而不是「上一次我们检查时它看起来是对的」。
///
/// # 为什么这样跳预检是保守的
///
/// 跳的条件**只有一个**：记忆里那台机器的 build_id **恰好等于** [`EXPECTED_BACKEND_BUILD_ID`]。
/// 其余一律照跑（无记忆 / 记的是别的 build）—— 那些情况本来就**可能需要部署**，不能跳。
///
/// ⚠ 功能件 §8 把「缓存 miss 时 caps 决策必须保守」写成了这件事的阻塞。
/// 逐字复核之后：那句话约束的是 **miss 路径**，而 miss 路径的答案**早就在代码里** ——
/// `caps` 的三级阶梯（`hello_confirmed` → 部署侧确认 → **空集全降级**）本身就是保守的。
/// ⇒ 它挡住的是一部分（miss 怎么办），**不是整件**（hit 能不能跳）。
///
/// # 记忆过期怎么办（**如实写在这里**）
///
/// 远端二进制被人删掉/换旧、而记忆还说「是期望 build」时，本轮会跳过预检直接起流 ⇒
/// **exec 失败**。失败路径清掉记忆 ⇒ **下一轮重新预检并重新部署**。
/// 代价是**多一次重连**，不是永久坏掉。这是本设计唯一的退化，写下来不藏着。
static VERIFIED_BUILD: Mutex<Option<std::collections::HashMap<String, String>>> = Mutex::new(None);

/// 纯函数：这一轮**能不能跳过**那两条预检连接。
///
/// `verified` = [`VERIFIED_BUILD`] 里这台机器的记录（`None` = 没记过）。
/// 判据是**逐字相等**，不是包含 —— 前缀相等会让 `abc123` 与 `abc123-dirty` 混为一谈
/// （本区 F24 那一族）。
fn preflight_can_be_skipped(verified: Option<&str>, expected: &str) -> bool {
    matches!(verified, Some(v) if v == expected)
}

/// 读这台机器的自证记录。
fn verified_build_of(origin: &str) -> Option<String> {
    VERIFIED_BUILD.lock().ok()?.as_ref()?.get(origin).cloned()
}

/// 记下「这台机器上一次真的跑起来的后端是 `build_id`」。
///
/// ⚠ **只许在收到 hello 的那一处调**（backend 自报）。别处调就把「自证」变回了「猜」。
fn record_verified_build(origin: &str, build_id: &str) {
    if let Ok(mut g) = VERIFIED_BUILD.lock() {
        g.get_or_insert_with(std::collections::HashMap::new)
            .insert(origin.to_string(), build_id.to_string());
    }
}

/// 抹掉这台机器的自证记录（连接没起来 / backend 换了身份）。
fn forget_verified_build(origin: &str) {
    if let Ok(mut g) = VERIFIED_BUILD.lock() {
        if let Some(m) = g.as_mut() {
            m.remove(origin);
        }
    }
}

/// 〔DEL 续〕一轮连接结束之后怎么办。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AfterRound {
    /// 按退避再连。
    RetryIn(Duration),
    /// 那台永久不支持（非 unix）⇒ 不再自动重连，这条流收工（带那句话）。
    Stop(String),
}

/// 纯函数：这一轮记下了「永久不支持」⇒ 停；否则按当前退避再连（主会话裁：非 unix 不按退避空转）。
pub(crate) fn after_round(unsupported: Option<String>, backoff: Duration) -> AfterRound {
    match unsupported {
        Some(why) => AfterRound::Stop(why),
        None => AfterRound::RetryIn(backoff),
    }
}

/// 纯函数：把当前退避翻倍并封顶到 [`RECONNECT_MAX`]。run() 的重连循环在"仍未连上"时调用。
fn next_backoff(cur: Duration) -> Duration {
    (cur * 2).min(RECONNECT_MAX)
}

/// 远端后端的连接配置。S5 会从 monitor 的 config 文件反序列化出来；
/// Tier 1（issue #15）的「测试连接」命令直接收前端传来的同形对象（camelCase）。
///
/// **serde camelCase 必须与前端 RemoteHostConfig / lib.rs::load_remote_configs 严格一致**：
/// host / port / user / keyPath / hostKeyFingerprint / label（多机 #30，
/// 可选，缺省回退 host）。前端多发的 `enabled`
/// 字段被忽略（serde 默认丢弃未知字段，测试连接不关心 enabled）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConfig {
    pub host: String,
    /// 稳定的机器标识（origin tag，多机 #30）。空 = 回退用 `host`（见 `origin_label`）。
    /// 用作 Tab 前缀 / 历史分组 / 选台 key。前端可不传（serde default）。
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub user: String,
    /// 私钥文件路径（OpenSSH 格式）。None / 空 = 走 ssh-agent（见 connect_session）。
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub key_path: Option<String>,
    /// 期望的 server host key 指纹（`SHA256:...` 形式）。
    /// Some = 严格校验（TOFU 之后固化）；None = 首次连接 TOFU 接受并 LOUD warn。
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub host_key_fingerprint: Option<String>,
    /// Batch14-F45：备用地址（happy-eyeballs 竞发）。每项 `host` / `host:port` /
    /// `[IPv6]:port` / 裸 IPv6。空 = 仅用 `host`（老配置零迁移）。首选地址仍是 `host`
    /// 字段（见 [`RemoteConfig::endpoints`]，host 排首）。
    #[serde(default)]
    pub addresses: Vec<String>,
    /// Batch14-F56：跳板 ProxyJump——指向另一台已配置主机的 `origin_label`。Some = 经该跳板
    /// 机 `channel_open_direct_tcpip` 隧道连本机（fail-closed，跳板缺失/连不上即报错不直连）；
    /// None/空 = 直连。v1 单跳（跳板自身的 jump 忽略）+ 防自引用环。
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub jump: Option<String>,
}

/// Batch14-F45：单个连接目标（host + port）。〔MIG-1 收尾〕今天只剩一个用处：后端 ack 里结构化的胜者（`winner`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

// 〔MIG-1 收尾 · 主会话裁「一个判定一个家」〕解析一行地址（`host` · `host:port` · `[v6]:port` · 裸 v6 四形态）那个函数
//   `parse_address_line`〔散文墓碑〕删了：地址解析与组拨号请求只在本机常驻后端 `src/backend/dial/machine.rs`（起流时把这台原样的配置交过去）。

impl RemoteConfig {
    /// origin 标签 = 稳定身份。`label` 为空时回退用 `host`（向后兼容：旧配置 / 前端
    /// 未传 label 时与单机时代 `origin = host` 行为一致）。多机 #30 用作 Tab 前缀 /
    /// 历史分组 / `load_remote_config_by_label` 选台 key。
    pub fn origin_label(&self) -> String {
        if self.label.is_empty() {
            self.host.clone()
        } else {
            self.label.clone()
        }
    }

    // 〔MIG-1 收尾〕「所有连接目标」（`host` 排首 · `addresses` 追加 · 去重保序）那一格搬进后端 `dial/machine.rs::Machine::endpoints`。
}

fn default_ssh_port() -> u16 {
    22
}

/// 前端可选字段以空字符串下发（见 remote-section.ts 注释）；反序列化时把 `""` 归一成
/// `None`，与 lib.rs::parse_host_obj 的 `.filter(|s| !s.is_empty())` 语义一致。
fn empty_string_as_none<'de, D>(de: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(de)?;
    Ok(opt.filter(|s| !s.is_empty()))
}

/// F45：per-origin「上次成功地址」——下次当 `prefer` 交给后端排首（下次大概率同一条路最快），赢家更新。
/// 进程内软状态,丢了只是少一次优化,不影响正确性。
/// 〔MIG-1 收尾〕记的是后端 ack 里结构化的 `winner`，连同**记下那一刻这台的地址配置**（`host` · `port` · `addresses`）——
///   配置改过就失效（原先靠在界面进程里重新解析地址来判「它还在不在配置里」，那份解析搬进了后端）。
fn last_good_store() -> &'static Mutex<std::collections::HashMap<String, (AddrConfig, Endpoint)>> {
    static STORE: std::sync::OnceLock<
        Mutex<std::collections::HashMap<String, (AddrConfig, Endpoint)>>,
    > = std::sync::OnceLock::new();
    STORE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// 一台的地址配置（原样，不解析）：last-good 只在它没变时才算数。
type AddrConfig = (String, u16, Vec<String>);

fn addr_config(cfg: &RemoteConfig) -> AddrConfig {
    (cfg.host.clone(), cfg.port, cfg.addresses.clone())
}

pub(crate) fn last_good_for(cfg: &RemoteConfig) -> Option<Endpoint> {
    let g = last_good_store().lock().ok()?;
    let (seen, ep) = g.get(&cfg.origin_label())?;
    (seen == &addr_config(cfg)).then(|| ep.clone())
}

pub(crate) fn record_last_good(cfg: &RemoteConfig, ep: &Endpoint) {
    if let Ok(mut m) = last_good_store().lock() {
        m.insert(cfg.origin_label(), (addr_config(cfg), ep.clone()));
    }
}

// 〔FIX4 · V41〕F45 那个「当前该拨的首选地址」（`winner_address`〔散文墓碑〕，喂 monitor 自己拼的 PowerShell ssh 命令）删了：
//   开终端那一行进了本机后端（`terminal-ssh`），地址由 `dial/machine.rs::resolve` 按交过去的 `prefer`（[`last_good_for`]）排首。

// 〔MIG-1 收尾〕竞发拨号顺序（last-good 排首、其余保序）那个纯函数 `winner_order` 搬进后端 `dial/machine.rs::request`（`prefer`）。

/// 连接远端、鉴权、开 session channel、exec [`BACKEND_CMD`]，
/// 返回 channel 的双向流（`AsyncRead + AsyncWrite`）——读端即后端的 stdout 数据。
///
/// 鉴权委托给 [`connect_session`]（publickey 或 ssh-agent）。
/// 错误统一 map 成 `String`（本 crate 未直接依赖 anyhow，不为骨架引入新依赖）。
/// F66（#58③）流模式门控决策（纯函数，矩阵单测）：**从后端声明的能力 token 决定
/// 发哪些 flag**，不再靠 build_id 精确匹配（Batch7-F24/Batch8-F26 的旧机制）。
///
/// - `capabilities` = backend hello 自报的能力集（旧后端无声明 → 空集）。
/// - 空集（旧 backend / 尚未确认）→ `(false, false)`：全降级 = 2.18.0 行为，功能退化但
///   连接正常。
/// - `tail_only`（历史改走旁路快照，拥塞根除）需后端声明 `"tail-only"`。
/// - `with_bg`（放行 bg 会话）需后端声明 `"bg"` **且**用户开了 `show_bg`。
/// - `with_rbind_token`（`设计/80 §8.7` 步 3）需后端声明 `"rbind-token"`。
///   **没有用户开关**：这一位不是偏好，是「这台后端报不报得出启动期令牌」。
///   令牌默认不上 wire（`§8.6 ③`，敏感数据）⇒ 只有索要的客户端才拿得到，
///   而 monitor **就是**那个要拿它来做 `sid → token → HWND` join 的客户端。
///
/// **§26 死循环护栏靠声明本身保住**：旧后端把未知 flag 当一次性查询 → 退出 → 无
/// hello → 重连死循环。而只有**会先剥离该 flag** 的后端才声明对应能力（见 backend
/// `CAPABILITIES` 注释），故「声明了 = 发该 flag 安全」——比 build_id 精确匹配更强更干净，
/// 且直接闭合 2026-07-09「身份确认不了就全降级」事故（能力由后端自报，不靠脆弱身份链）。
/// monitor **认识**的能力 token。
///
/// U-CC1：它与 [`decide_stream_flags`] 是同一份事实 —— 由
/// `known_capability_tokens_match_decide_stream_flags` 钉住。
/// 有它才能回答「backend 声明了一个我们不认识的能力」这个问题（漂移记账的第四个面）。
///
/// 🔴 〔`设计/80 §8.7` 步 3，2026-09-23〕**`"rbind-token"` 登记进来了。**
/// 步 2 那一路留下的交接逐字：那一刀之后后端开始在 hello 里声明这个 token，
/// 而本名单还是 `["bg","tail-only"]` ⇒ monitor 每次握手都往 `drift_ledger`
/// 记一条 `UnknownBackendToken capabilities:rbind-token`（只记账、行为不变）。
/// 本刀同拍把它登记进来 **并** 扩了 [`decide_stream_flags`] —— 两件必须同拍：
/// 只登记不扩，上面那条恒等判据会当场红（名单里有、门控不看它）。
const KNOWN_CAPABILITY_TOKENS: &[&str] = &["bg", "rbind-token", "tail-only"];

/// U-CC1 第四个面的写点：hello 里**不认识的**能力 token 记一笔。〔ST3〕记在 `origin`（那台远端）名下。
/// 只记账，行为一字不改（不认识的 token 本来就按保守缺省忽略）。
fn note_unknown_capabilities(
    origin: &crate::origin::Origin,
    capabilities: &[String],
    build_id: &str,
) {
    for t in capabilities {
        if !KNOWN_CAPABILITY_TOKENS.contains(&t.as_str()) {
            crate::drift_ledger::record(
                origin,
                crate::drift_ledger::DriftFace::UnknownBackendToken,
                &format!("capabilities:{t}"),
                Some(&format!("build_id={build_id}")),
            );
        }
    }
}

/// 三位流模式 flag：`(with_bg, tail_only, with_rbind_token)`。
///
/// 🔴 〔步 3〕元组从 2 元扩成 3 元。**扩它会连带 [`should_upgrade_reconnect`]** ——
/// 那个函数吃的就是这个元组，而它是「防无限重连」的收敛判据。两处一起改、
/// 一起补穷举（`ssh_source_stream_flag_gate_tests.rs`），不许只改一边。
fn decide_stream_flags(capabilities: &[String], show_bg: bool) -> (bool, bool, bool) {
    let has = |c: &str| capabilities.iter().any(|t| t == c);
    (show_bg && has("bg"), has("tail-only"), has("rbind-token"))
}

/// F66（#58③）★ 防无限重连的收敛判据（纯函数，穷举单测）：收到后端能力声明后，
/// **是否值得重连一轮升级流模式**。`cur` = 本轮实际发的 `(with_bg, tail_only)`；`next` =
/// 据后端自报能力算出的下一轮 flag。
///
/// **仅当下一轮会开一个本轮关着的 flag** 才重连——每次重连严格增开 flag，flag 数有限
/// （〔步 3〕**2 → 3**）⟹ 最多 3 轮收敛，绝不无限重连。**关键定理**：一旦记账
/// `hello_confirmed=Some(D)`，下一轮 `caps=D` ⟹ `next==cur` ⟹ 本函数三项皆自相矛盾
/// （`next_x && !cur_x` 在 next==cur 时恒 false）⟹ 恒 `false`，不再重连。
///
/// 三项都写全 `&& !cur_*`（不靠调用点的外层 guard），使收敛不变式在函数内自洽、
/// 可独立穷举测试（审计：原 `next_tail` 裸项隐含依赖外层 guard，读者需回连才懂）。
///
/// 🔴 〔步 3〕**调用点那道 `if !tail_only` 外层 guard 也跟着搬走了，那不是顺手改的。**
/// 它原本的语义是「本轮若跑在降级模式」，而那句话在**两位**的世界里才成立
/// （`tail_only` 开着 ⇒ 后端至少声明过 `tail-only` ⇒ 不算旧后端）。三位之后它当场为假：
/// 一台后端完全可能 `tail_only` 已开、而 `rbind-token` 这一位**本轮没开**
/// （例：`hello_confirmed` 记的是上一版只声明了 `tail-only` 的能力集）。
/// 那时外层 guard 会把本函数整个跳过 ⇒ `--with-rbind-token` **永远发不出去**，
/// 而表现是「令牌字段恒缺席」—— 而缺席是合法值（`§8.5 ②` 那个布尔会读成
/// 「这条会话没有令牌」）⇒ **极安静**。⇒ 升级判定必须无条件问本函数。
fn should_upgrade_reconnect(cur: (bool, bool, bool), next: (bool, bool, bool)) -> bool {
    let ((cur_bg, cur_tail, cur_tok), (next_bg, next_tail, next_tok)) = (cur, next);
    (next_tail && !cur_tail) || (next_bg && !cur_bg) || (next_tok && !cur_tok)
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_coldstart_preflight_guard.rs"]
mod coldstart_preflight_guard;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_coldstart_perf_guard.rs"]
mod coldstart_perf_guard;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_stream_flag_gate_tests.rs"]
mod stream_flag_gate_tests;

// ═══════════ 〔C2 · `设计/05 §13`〕拨号归后端：接远端后端的每一跳都只经拨号代理 ═══════════
//
// 〔墓碑 —— `K-P6b` 那一段原话的要点逐字：「买到的是：**`backend 那条长连接流` 的那一跳 SSH 握手，
//  可以不发生在界面进程里**」「**界面进程仍然自己拨号 —— 7 处里搬走的是 1 处**」「回落有两条……
//  ① 拿不到代理二进制 ② 配置里没填 `keyPath`」。〕
//
// C2 之后这三句都不成立了：拨号**全部**在后端的拨号代理里（`src/backend/dial/`）；界面这一侧拿链路的
// 唯一入口是宿主 `dial_host`，读应答的是通信层成员 `ssh_link`。两条回落都删了：
// ② 的根因（代理不会 ssh-agent）在代理那侧补上了；① 按 `D11`「后端是给定的，不要退路」—— 找不到本机后端
// 二进制就**报**，不再进程内拨。**唯一还在界面进程里拨的是 SFTP**（`F7c` 独占的 `sftp.rs`，
// 用的是 `inproc_dial.rs` 那一份搬来的旧实现），登记在 `dial_move_judge::DIAL_SITES`。

/// 〔E2 · V28 · `设计/01 §6.7b`〕远端后端在 shell 里的写法：恒是那台的 `~/.cc-monitor/bin/ccm`（后端二进制本身），
/// 可填的 `backendPath` 删了。常量一份住 `relay_route_core`（后端往远端拼命令也读它）。
pub(crate) const BACKEND_CMD: &str = relay_route_core::BACKEND_LANDING_SHELL;

// 🔴 **这个模块的 `pub(crate)` 是 `K-R74` 的承重件，别顺手收回私有**〔09-12〕：
// `dial_home_registry`（另一份文件）那条递减棘轮拿 `DIAL_SITES` 里 `moved == false` 的
// **处数合计**当今天的读数；收回成私有那一边就编不过，抄一份数字过去则是「同一个值两个家」。
//
// 〔`K-R76` 09-12〕**这里原先多一道绕道，现在拆掉了**：模块写成私有 `mod`，再在文件顶层
// 加一行 `pub(crate) use dial_move_judge::DIAL_SITES;` 重导出一次。那道绕道**不是品味**，
// 它当时有一个真理由：`guard_core::test_module_ranges` 按**字面前缀**认 `mod ` ⇒
// 写成 `pub(crate) mod` 那一刻本文件整个测试段**不再被剥掉**（`K-R74` 09-12 实打：
// `cargo test -p monitor --lib` 从 `1403 passed; 0 failed` 变成 `1399 passed; 10 failed`，
// 红的是本文件的 `six_of_the_seven_dial_sites_are_still_in_this_process` 与
// `write_half_guard` 三条，外加 `structural_scan` · `cross_half_edge_registry` ·
// `exec_site_registry` · `local_read_surface_registry` · `byte_cap_registry`
// 那几份里「扫生产段」的判据 —— 它们那一刻扫的是测试代码）。
// `K-R75`（09-12）把那一步换成**按形状**剥可见性修饰（`guard_core::strip_visibility`）之后
// **那个理由不再成立** ⇒ 绕道拆掉，模块写回它本来的样子。
// ⚠ 「今天剥法真的接得住 `pub(crate) mod`」这句话**由机检守着，不靠这段散文**：
//   住 `local_backend_host.rs` 的 `the_strip_rule_this_file_leans_on_is_still_on_disk`
//   （`K-R76` `KR76D2`，拿一段逐字校验位去核 `guard-core` 那一行）。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_dial_move_judge.rs"]
pub(crate) mod dial_move_judge;

// === Batch8-F26：旁路快照拉取（"每管道一个对话，完就断"——用户设计） ===
//
// tail-only 下后端不再重放历史；每个已宣告会话的完整历史由这里经**独立
// SSH 连接**跑 `--read-session` 一次性查询拉回，按行号编 seq 灌进与 tail 行
// 完全相同的管线（flush_lines → on_line_batch_awaited）。两路 seq 同处行号
// 空间：重叠区是精确重复的 (sid,seq)，被前端既有去重吸收（MASTERPLAN-batch8 §2）。
// 并发 ≤SNAPSHOT_CONCURRENCY（不抢 tail 通道带宽）；F19 priority sid 优先出队。

const SNAPSHOT_CONCURRENCY: usize = 2;
/// 单会话快照体量上限（防御：远端超巨文件不无界拉取；超限截断 warn——
/// 历史浏览器按需查询不受此限）。
const SNAPSHOT_MAX_BYTES: u64 = 512 * 1024 * 1024;
const SNAPSHOT_CHUNK_LINES: usize = 500;
/// Batch9-F30：尾部优先——最新 N 行先到（第一批 emit 即最新内容），旧历史回填。
const SNAPSHOT_TAIL_LINES: usize = 500;

/// 每连接一个：待拉快照队列。sid 幂等（重复宣告不重拉）；`cancel(sid)`
/// （SessionRemoved 时调）摘除排队项 + 给 inflight 打取消标记 + 从 seen 摘除
/// （同连接内 removed→re-added 可重拉，审计 D-S4）；close（断连）**立即作废**
/// 未开拉的排队项——重连会重建队列重拉，断连后继续拉只会把行灌在归档清算
/// 之后（审计 D-B1 僵尸复活）。
struct SnapshotQueue {
    pending: std::sync::Mutex<SnapshotPending>,
    notify: tokio::sync::Notify,
    closed: std::sync::atomic::AtomicBool,
}

struct SnapshotPending {
    queue: std::collections::VecDeque<SnapshotItem>,
    seen: std::collections::HashSet<String>,
    /// 已取消（会话已 removed）的 sid——inflight fetch 每个 chunk 边界查它中止。
    cancelled: std::collections::HashSet<String>,
}

// 〔MIG-1 · `设计/99 §2.1 ⑬`〕这里原来住着 monitor 那一套会话 / tmux 账本 —— 宣告账（`announced_registry`〔散文墓碑〕，F5 重宣告的数据源）·
//   tmux 原文账（`record_tmux_raw`〔散文墓碑〕一族）· idle 账（`mark_idle`〔散文墓碑〕一族）· 摘除裁决（`classify_removed`〔散文墓碑〕）·
//   断连 flush · 重连后重新裁（`reannounce_after_reconnect`〔散文墓碑〕一族）· 「报完了清单」账 · F5 对账分流（`split_stale`〔散文墓碑〕）。
//   裁决搬进了那台后端（`src/backend/observe/session_ledger.rs`，成品帧 `session_state`），本机远端同一份；
//   monitor 只剩一本成品缓存（`session_book.rs`：转交 ＋ F5 重放 ＋ 断连说「说不清」）。

#[derive(Clone)]
struct SnapshotItem {
    sid: String,
    path: String,
    /// backend prime 时的完整行数 L（p1f 帧 `lines`）——完整性校验：快照行数
    /// < L = 中途断/backend 报错 → 判失败重试（审计 D-I2：exit status 拿不到）。
    expected_lines: Option<u64>,
}

/// stream_loop 退出（重连/EOF/错误任何路径）时关队列。
struct SnapshotQueueCloser(std::sync::Arc<SnapshotQueue>);
impl Drop for SnapshotQueueCloser {
    fn drop(&mut self) {
        self.0.close();
    }
}

impl SnapshotQueue {
    fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(SnapshotQueue {
            pending: std::sync::Mutex::new(SnapshotPending {
                queue: std::collections::VecDeque::new(),
                seen: std::collections::HashSet::new(),
                cancelled: std::collections::HashSet::new(),
            }),
            notify: tokio::sync::Notify::new(),
            closed: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn push(&self, item: SnapshotItem) {
        {
            let mut p = self.pending.lock().unwrap();
            // 重新宣告 = 会话回来了：解除既往取消标记（cancel 时 seen 已摘，
            // 这里 insert 成功才入队）。
            p.cancelled.remove(&item.sid);
            if !p.seen.insert(item.sid.clone()) {
                return; // 本连接内已拉/在拉
            }
            p.queue.push_back(item);
        }
        self.notify.notify_one();
    }

    /// SessionRemoved：摘排队项 + 标记 inflight 取消 + 允许 re-added 重拉。
    fn cancel(&self, sid: &str) {
        let mut p = self.pending.lock().unwrap();
        p.queue.retain(|it| it.sid != sid);
        p.seen.remove(sid);
        p.cancelled.insert(sid.to_string());
    }

    /// inflight fetch 的取消/作废检查（chunk 边界调）：会话已 removed 或连接
    /// 已断（断连后继续灌行会落在归档清算之后——B1）。
    fn is_cancelled(&self, sid: &str) -> bool {
        self.closed.load(std::sync::atomic::Ordering::SeqCst)
            || self.pending.lock().unwrap().cancelled.contains(sid)
    }

    fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// 出队：priority sid（若在队中）优先，否则 FIFO。**closed 即 None**（未开
    /// 拉的排队项作废，重连重拉）。
    ///
    /// 丢失唤醒防护（审计 D-I1）：`notify_waiters` 不给未注册者存 permit——
    /// 必须**先注册**（`enable`）再检查状态，close/push 发生在注册后必被捕获、
    /// 发生在注册前则状态检查看得到。
    async fn pop(&self, priority: Option<String>) -> Option<SnapshotItem> {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
                return None;
            }
            {
                let mut p = self.pending.lock().unwrap();
                if let Some(pri) = priority.as_deref() {
                    if let Some(i) = p.queue.iter().position(|it| it.sid == pri) {
                        return p.queue.remove(i);
                    }
                }
                if let Some(item) = p.queue.pop_front() {
                    return Some(item);
                }
            }
            notified.await;
        }
    }
}

/// 分发器：每连接一个 task。并发 ≤SNAPSHOT_CONCURRENCY 地把队列里的会话交给
/// [`fetch_snapshot`]；每项失败重试 1 次（间隔 1s），仍败 → remote-health toast
/// （该 tab 只有实时行，历史浏览器兜底可看全量）。取消（会话 removed/断连）
/// 不算失败、不重试不 toast。
async fn snapshot_dispatcher(
    q: std::sync::Arc<SnapshotQueue>,
    replay: Arc<EventReplay>,
    app: tauri::AppHandle,
    host_label: String,
) {
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(SNAPSHOT_CONCURRENCY));
    loop {
        let Ok(permit) = sem.clone().acquire_owned().await else {
            return; // semaphore closed（不可达，防御）
        };
        let Some(item) = q.pop(replay.priority_sid()).await else {
            return; // 队列已关（排队项作废，重连重拉）
        };
        let q = q.clone();
        let replay = replay.clone();
        let app = app.clone();
        let host_label = host_label.clone();
        // incr 在 spawn 之前（审计 D：上一 task 归零与下一 task 起跑之间的
        // 瞬时 0 窗口会让 300ms 定时器恰好放行 batch）
        snapshot_inflight_change(&replay, 1);
        tauri::async_runtime::spawn(async move {
            let _permit = permit;
            struct InflightGuard(Arc<EventReplay>);
            impl Drop for InflightGuard {
                fn drop(&mut self) {
                    snapshot_inflight_change(&self.0, -1);
                }
            }
            let _inflight = InflightGuard(replay.clone());
            let sid_short: String = item.sid.chars().take(8).collect();
            let mut last_err = String::new();
            for attempt in 1..=2 {
                match fetch_snapshot(&q, &item, &host_label, &replay).await {
                    Ok(FetchOutcome::Done(lines)) => {
                        tracing::info!(
                            "snapshot [{host_label}] {sid_short}: {lines} 行历史就位（attempt {attempt}）"
                        );
                        return;
                    }
                    Ok(FetchOutcome::Cancelled) => {
                        // 会话已 removed / 连接已断：静默中止（补偿归档已在
                        // fetch 内 emit），不重试不 toast。
                        tracing::info!(
                            "snapshot [{host_label}] {sid_short}: 取消（会话结束/断连）"
                        );
                        return;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "snapshot [{host_label}] {sid_short} attempt {attempt} 失败: {e}"
                        );
                        last_err = e;
                        if attempt == 1 {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                    }
                }
            }
            let payload = crate::ui_contract::RemoteHealthPayload {
                origin: host_label.clone(),
                kind: "snapshot".to_string(),
                message: copy_text(
                    "rsSshSource.snapshot.failed",
                    &[
                        ("sid", &sid_short.to_string()),
                        ("err", &last_err.to_string()),
                    ],
                ),
            };
            if let Err(e) = app.emit(crate::ui_contract::events::REMOTE_HEALTH, payload) {
                tracing::warn!("snapshot remote-health emit failed: {e}");
            }
        });
    }
}

/// Batch9-F30：全局快照 inflight 计数——前端 batch mode 的事件驱动信号
/// （回填在途时不提前退出 batch 模式，见 events.ts）。
/// 计数 + emit 在同一把锁下串行（审计 D：原子操作与 emit 分离时，两个并发
/// task 收尾的 emit 可乱序——{count:0} 先到、{count:1} 后到 → 前端计数粘在
/// 非零、batch 被压满 5min 防呆）。低频（每快照 2 次），锁开销可忽略。
static SNAPSHOT_INFLIGHT: std::sync::Mutex<usize> = std::sync::Mutex::new(0);

fn snapshot_inflight_change(replay: &EventReplay, delta: isize) {
    let mut n = SNAPSHOT_INFLIGHT.lock().unwrap_or_else(|e| e.into_inner());
    *n = if delta > 0 {
        *n + 1
    } else {
        n.saturating_sub(1)
    };
    // 持锁交：保证到达序 == 计数变化序（〔MIG-1〕会话流里的一格，不吃 credit、不丢）。
    replay.on_snapshot_inflight(*n as u32);
    drop(n);
}

/// F5 电平同步（审计 D）：inflight 是变化沿，重载后前端初值 0 ⇒ 〔MIG-1〕就绪点在重放最前面补一格当前电平（`event_replay::lifecycle_replay`）。
pub fn snapshot_inflight_level() -> u32 {
    *SNAPSHOT_INFLIGHT.lock().unwrap_or_else(|e| e.into_inner()) as u32
}

/// fetch 的三态结果：完成（行数）/ 被取消（不重试）。错误走 Err。
enum FetchOutcome {
    Done(u64),
    Cancelled,
}

// 〔MOD〕「快照那一行计不计号」（`snapshot_line_countable`〔散文墓碑〕）删了：后端只交可计行（口径只住后端 `history_query::line_counts`）。

/// 拉取单个会话的完整历史快照并灌进既有管线。
///
/// 🔴 〔`C1` · 2026-09-24〕**不再为每份快照单拨一条 SSH。** 此前这里 exec 一次
/// `<backend> --read-session-tail <p> 500`，读它一口气印出来的「meta ＋ 尾段 ＋ 头段」；
/// 现在走已有长连接：先 `history-tail` 问那张图（`total` / `tail_from` / 两段的字节边界），
/// 再按 `[split_at, end)`、`[0, split_at)` 两段用 `history-read` 分页取正文 ——
/// 与那条子命令印出的两段**逐字节相同**（后端扫的是同一个函数），行号映射（[`tail_seq`]）一个字没动。
///
/// 每个 chunk 边界查取消（会话 removed / 连接断）——中止并**补偿 emit 一次
/// ended 格**：若某个已 flush 的 chunk 恰把归档 tab"见行复活"，这里把它
/// 压回 archived（审计 D-B1 僵尸复活的封口；archiveTab 幂等，重复无害）。
///
/// 完整性校验（审计 D-I2）：到达的可计行数必须**恰好等于** `total`。
async fn fetch_snapshot(
    q: &std::sync::Arc<SnapshotQueue>,
    item: &SnapshotItem,
    host_label: &str,
    replay: &Arc<EventReplay>,
) -> Result<FetchOutcome, String> {
    use crate::frame_query;
    let sid = &item.sid;
    let path = &item.path;
    let origin = crate::origin::Origin(host_label.to_string());
    // 〔DL1 · `设计/05 §3.3.2`〕快照是两件事、各一个期限：先问图（一问，`PAGE_BUDGET`）；
    //   读正文（分页）在知道要读多少字节之后再造、按大小给（`frame_query::read_budget`），每一页都拿同一个时刻去等。
    let plan = frame_query::tail(
        &origin,
        path,
        SNAPSHOT_TAIL_LINES as u64,
        frame_query::Deadline::within(frame_query::PAGE_BUDGET),
    )
    .await?;
    // 〔C2 · U3 第 3 件〕断线重连后从续点接着拉（`snapshot_resume` 头注），续点对不上才整份。
    let cursor = crate::snapshot_resume::cursor_of(&origin, sid);
    let mut how = crate::snapshot_resume::plan_read(cursor.as_ref(), path, &plan);
    // 〔RENDER2 · `设计/10 §3.2`〕断线期间文件变短了（续点比这一次的图长）⇒ 这一次整份读出来的是另一代的行号：
    //   先交那个会话一格「变短了、已从头重读」（前端据它整份重来、留存丢旧的一代），再整份读。
    //   续点不必另丢：这一次整份读完立的新锚盖掉它。
    if crate::snapshot_resume::shrank(cursor.as_ref(), path, &plan) {
        replay
            .on_session_notice(crate::ui_contract::SessionFileNoticePayload {
                session_id: sid.to_string(),
                origin: host_label.to_string(),
                path: path.to_string(),
                change: FileChange::Truncated.as_wire().to_string(),
            })
            .await;
    }
    // 〔W5-VIS · `设计/15 §3.4 ②`〕续传之前先核锚那一行还是不是那一行（`snapshot_resume` 头注「截断 / 改写检测」）：
    //   断线期间被整份改写而且变长的文件，上面那道「文件没变短」拦不住。对不上 ⇒ 续点作废、整份重读、交那个会话一格「被改过」。
    if let (crate::snapshot_resume::Read::Resume { .. }, Some(w)) =
        (&how, cursor.as_ref().and_then(|c| c.witness.clone()))
    {
        let page = frame_query::read_page(
            &origin,
            path,
            w.start,
            Some(w.end),
            frame_query::Deadline::within(frame_query::PAGE_BUDGET),
        )
        .await?;
        // 一页没读满那一行（`next < end` 且没到头）⇒ 核不了，照续传（不许把「没读全」说成「被改过」）。
        let whole = page.eof || page.next >= w.end;
        if whole && !crate::snapshot_resume::witness_holds(&w, &page.rows) {
            tracing::warn!(
                "snapshot [{host_label}] {sid}: 续点那一行（字节 {}–{}）与上次不是同一行 —— 记录文件在断线期间被改写过，整份重读",
                w.start,
                w.end
            );
            crate::snapshot_resume::forget(&origin, sid);
            replay
                .on_session_notice(crate::ui_contract::SessionFileNoticePayload {
                    session_id: sid.to_string(),
                    origin: host_label.to_string(),
                    path: path.to_string(),
                    change: FileChange::Rewritten.as_wire().to_string(),
                })
                .await;
            how = crate::snapshot_resume::Read::Full;
        }
    }
    if let crate::snapshot_resume::Read::Resume {
        from_byte,
        upto,
        first_seq,
        skip_below,
    } = &how
    {
        tracing::info!(
            "snapshot [{host_label}] {sid}: 续传 —— 从字节 {from_byte}（第 {first_seq} 行）读到 {upto}，\
             第 {skip_below} 行之前的已发过、不再发"
        );
    }
    let mut walk = crate::snapshot_resume::Walk::new(&how, &plan);
    // 〔W5-VIS〕走读时顺手挑下一次续传要核的那一行（文件最后一个可计行）。
    let mut pick = crate::snapshot_resume::WitnessPick::default();
    let mut total_bytes: u64 = 0;
    let mut chunk: Vec<JsonlLine> = Vec::with_capacity(SNAPSHOT_CHUNK_LINES);
    let mut runs = crate::SkipRuns::default(); // 〔RENDER2〕这一次快照自己一份（与实时那一路不交错）
    let mut cancelled = false;
    let segments = walk.segments().to_vec();
    let body_bytes: u64 = segments
        .iter()
        .map(|(from, upto)| upto.saturating_sub(*from))
        .sum();
    let body =
        frame_query::Deadline::within(frame_query::read_budget(body_bytes.min(SNAPSHOT_MAX_BYTES)));
    'read: for (from, upto) in segments {
        let mut offset = from;
        while offset < upto {
            let page = frame_query::read_page(&origin, path, offset, Some(upto), body).await?;
            total_bytes += page.next - offset;
            if total_bytes > SNAPSHOT_MAX_BYTES {
                // 防御上限：不再继续拉（完整性校验会把截断判为失败 → toast）。
                tracing::warn!(
                    "snapshot [{host_label}] {sid}: 超过 {SNAPSHOT_MAX_BYTES} 字节上限，截断"
                );
                break 'read;
            }
            let spans = crate::snapshot_resume::row_spans(offset, &page.rows);
            // 〔MOD〕后端只交可计行、每行带成品（进不进界面 · `cwd` · 摘要都是它给的）；这里只编号、挑见证、攒批。
            for (row, span) in page.rows.into_iter().zip(spans) {
                pick.see(upto, plan.end, row.hash, span);
                let Some(seq) = walk.step() else {
                    continue; // 续传：锚到续点之间的行前端已有，数掉不发
                };
                chunk.push(JsonlLine {
                    session_id: sid.to_string(),
                    path: std::path::PathBuf::from(path),
                    seq,
                    message: row.message,
                    cwd: row.cwd,
                    end: span.map(|(_, e)| e),
                });
                if chunk.len() >= SNAPSHOT_CHUNK_LINES {
                    if q.is_cancelled(sid) {
                        cancelled = true;
                        break 'read;
                    }
                    flush_lines(replay, host_label, std::mem::take(&mut chunk), &mut runs).await;
                }
            }
            offset = page.next;
            if page.eof {
                break;
            }
        }
    }
    if cancelled || q.is_cancelled(sid) {
        // 补偿（见 doc comment）；丢弃未 flush 的 chunk。〔CF1〕只对远端补（[`compensates_on_cancel`]）。
        // 〔MIG-1〕补的是那条会话**此刻**的终局（成品缓存里后端说过的 · 连接断了 ⇒ 说不清），不再由 monitor 恒判「已结束」。
        if compensates_on_cancel(&origin) {
            let again = crate::session_book::book()
                .read()
                .settle_again(origin.as_wire_str(), sid);
            for out in again {
                replay.on_lifecycle(out.origin(), out.frames());
            }
        }
        return Ok(FetchOutcome::Cancelled);
    }
    if !chunk.is_empty() {
        flush_lines(replay, host_label, chunk, &mut runs).await;
    }
    // 完整性校验：`total` 精确对账（F30）—— 续传时对的是「锚之后那一截」。
    let (arrived, want) = (walk.arrived(), walk.want());
    if arrived != want {
        return Err(copy_text(
            "rsSshSource.snapshot.incomplete",
            &[
                ("arrived", &arrived.to_string()),
                ("want", &want.to_string()),
            ],
        ));
    }
    // 下界：宣告时 prime 的行数 L（`session_added.lines`）—— 文件在宣告之后被截短才会撞上。
    if let Some(expected) = item.expected_lines {
        if plan.total < expected {
            return Err(copy_text(
                "rsSshSource.snapshot.incompletePlan",
                &[
                    ("total", &plan.total.to_string()),
                    ("expected", &expected.to_string()),
                ],
            ));
        }
    }
    // `[0, total)` 全到了（整份：刚发完；续传：锚之前的早有、之后的刚发完）⇒ 立锚。
    crate::snapshot_resume::note_snapshot_done(&origin, sid, path, &plan);
    crate::snapshot_resume::note_witness(&origin, sid, pick.done());
    Ok(FetchOutcome::Done(arrived))
}

/// 〔CF1〕快照中途被撤时要不要补一个 `ended` 格：**远端补、本机不补。**
///
/// 补偿治的是「已 flush 的那一块把刚归档的 tab 见行复活」—— 而**只有远端的行会复活 tab**
/// （前端 `tabs.ts` 只有 `remote-line` 那一格；本机 tab 的活与死只由 PID 探活那一路翻）。
/// 本机再补一个 `ended` 格，没有要治的病，反倒会把本机那边刚判成「可重连」的会话压成「已结束」。
pub(crate) fn compensates_on_cancel(origin: &crate::origin::Origin) -> bool {
    !origin.is_local()
}

/// 两段编号映射（纯函数，与测试共用——审计 D：原测试在测试体内重实现映射，
/// 锤不到生产代码）：到达序 → 行号。前 total-tail_from 行是尾段（最新），
/// 其余是头段回填。调用方保证 tail_from <= total（`frame_query::tail` 校验）。
pub(crate) fn tail_seq(arrived: u64, total: u64, tail_from: u64) -> u64 {
    let seg1 = total.saturating_sub(tail_from);
    if arrived < seg1 {
        tail_from + arrived
    } else {
        arrived - seg1
    }
}

// 〔MIG-3b 续 · V41〕`connect_and_exec_cmd`〔散文墓碑〕（exec 一条命令、拿回 stdout 字节流）删了：最后一个调用方（公钥推送）进了本机后端 ⇒
//   monitor 不再开 `stream` 用法的链路；一次性远端命令只剩 [`connect_and_exec_capture`]（收全、有上限、带退出码）。

/// 一次远端 exec 的**完整**结果：stdout、stderr、退出码。
///
/// # 为什么需要它（而不是继续用 `connect_and_exec_cmd`）
///
/// `Channel::into_stream()` 只搬 `ChannelMsg::Data` —— **`ExtendedData`（= stderr）
/// 与 `ExitStatus` 都被丢掉**（russh 0.61 `channels/io/mod.rs`）。所以既有的
/// `run_list_query` 那条路〔散文墓碑〕（逐次拨号，C4d 已删）只能看见 stdout：远端命令失败时它读到 0 行，
/// 与「查询成功但结果为空」**在类型上不可区分**。
///
/// 列举类查询忍得了（空结果本来就合法），但 `--fork-session` 忍不了 ——
/// 分叉失败必须让用户看见原因，而后端恰恰把原因写在 **stderr + exit 2** 上。
/// 所以这里直接驱动 `channel.wait()` 收全三样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteExec {
    pub stdout: String,
    pub stderr: String,
    /// `None` = 远端没送 exit-status（连接被掐 / 服务端不守规矩）。
    /// **不许把 `None` 当成 0** —— 那正好会把「没跑成」读成「跑成了」。
    pub exit_status: Option<u32>,
}

/// stdout/stderr 各自的收集上限。查询类输出都是一行 JSON 量级；
/// 上限只是防「远端吐无穷字节」吃爆内存，正常路径远够不到。
const EXEC_CAPTURE_MAX_BYTES: usize = 4 * 1024 * 1024;

/// backend **出方向单行**的字节上限〔devbench F10b〕。
///
/// # ★ 这个数**刻意不等于** backend 侧的 `inbound::MAX_LINE_BYTES`（1 MiB）
///
/// 那一条限的是**入方向命令信封**（`inbound.rs` 逐字「命令信封比 `ResumeSpec` 还小，
/// 1 MiB 已是极宽松的上限」）。本条限的是**出方向内容帧** —— 一帧 = 一条 Claude jsonl 行。
/// **两者不是同一个量**，抄过来就是把不同的东西按数字凑到一起
/// （`byte_cap_registry` 头注对账本 S3 那次订正逐字写过这句）。
///
/// 实测本机 `~/.claude/projects/**/*.jsonl` 全量 **525,132 行**：最长一行
/// **3,117,370 字节（2.97 MiB）**，其中 **78 行超过 1 MiB**
/// （> 1 MiB 且 ≤ 2 MiB 有 75 条，> 2 MiB 有 3 条）。
/// ⇒ 抄 1 MiB 会在本机丢掉 78 条**真实**行，而超限语义是「丢弃 + 报告」——
/// 用户会看到一条「丢了帧」的健康提示，而那不是拥塞，是我们自己把上限设小了。
///
/// 取 64 MiB = 实测最长行的 21 倍。留这么大余量的理由有两条：
/// 帧内换行被后端转义成 `\n` 两字符（最坏接近翻倍），以及工具输出体量只会变大。
///
/// # 超限语义：**丢弃 + 带身份报告**，不许静默
///
/// 走 `REMOTE_HEALTH` + `kind: "line_too_long"`，与 `overflow_health_message` 那条
/// 现成的路同一个出口（定框 E4：静默失败要给身份、且抬到调用方能判定的那一层）。
pub(crate) const BACKEND_FRAME_LINE_CAP: usize = 64 * 1024 * 1024;

/// 一次有界读行的结果。
#[derive(Debug)]
pub(crate) enum CappedLine {
    /// 读到一行（内容在 `buf` 里，**不含**行尾 `\n`；可能是 EOF 前的残行）。
    Line,
    /// 这一行超过 [`BACKEND_FRAME_LINE_CAP`]，**已整行丢弃**。
    /// 带上它到底有多少字节 —— 超限之后只数不存，所以这个数是准的而内存是 O(上限) 的。
    TooLong(u64),
    /// 对端关了写半边，且没有残行。
    Eof,
}

/// 按 `\n` 读一行，**上限在读的时候生效**。
///
/// # ★ 为什么不是 `read_line` 加一句长度判断
///
/// 那是后端侧栽过的坑，逐字记在 `src/backend/stream/inbound.rs` 头注里：
/// 第一版用无界 `read_until`、读完再看长度，D 审计实测**喂 512 MiB 无换行的流 ⇒
/// RSS 从 6 MiB 涨到 518 MiB**，而它照样回了一条 `line_too_long`「看起来对」。
/// ⇒ 机制必须是 `fill_buf`/`consume`：超限之后**只找换行、不再往 buf 里塞字节**，
/// 整行的内存占用与行长无关。本函数是那段机制在 monitor 侧的同构实现
/// （**上限值不同、机制相同** —— 见 [`BACKEND_FRAME_LINE_CAP`] 头注）。
///
/// ⚠ **不是 cancellation-safe**：中途取消会丢掉 `overflowed`/计数状态，
/// 而 `buf` 里的半行留着。调用方要么把它放进独立 task（主帧读那样），
/// 要么取消之后就**不再复用这个 reader**（探测那两处那样）。
///
/// ★ `cap` **是参数而不是直接读常量**：生产调用点全传 [`BACKEND_FRAME_LINE_CAP`]，
/// 而测试要能传一个小数。否则「超限之后内存不涨」这条性质就只能靠量 RSS 来证
/// （backend 侧当年正是那么发现问题的），而**那种证法进不了单测**。
/// 传小 cap 之后同一条性质可以直接判：见 `over_limit_stops_growing_the_buffer`。
pub(crate) async fn read_capped_line<R>(
    rd: &mut R,
    buf: &mut Vec<u8>,
    cap: usize,
) -> std::io::Result<CappedLine>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    // `fill_buf`/`consume` 走文件顶部那条 `AsyncBufReadExt` 导入 ——
    // ⚠ 别在这里再本地 `use` 一次：本文件有一条判据把 `tokio::io` 导入清单钉死了
    // （`ALLOWED_IO_IMPORTS`，含反向锚点「放行清单不许留死行」），
    // 顶部那条一旦没人用就会被 `unused_imports` 逼着删，而删掉它那条判据当场红。
    buf.clear();
    let mut overflowed = false;
    let mut seen: u64 = 0;
    loop {
        let chunk = rd.fill_buf().await?;
        if chunk.is_empty() {
            // EOF。有残行就当一行交出去（`read_line` 旧行为逐字如此），否则报 EOF。
            return Ok(if seen == 0 {
                CappedLine::Eof
            } else if overflowed {
                CappedLine::TooLong(seen)
            } else {
                CappedLine::Line
            });
        }
        let (take, done) = match chunk.iter().position(|&c| c == b'\n') {
            Some(i) => (i, true),
            None => (chunk.len(), false),
        };
        seen += take as u64;
        if !overflowed {
            if buf.len() + take > cap {
                overflowed = true;
                buf.clear();
                buf.shrink_to_fit();
            } else {
                buf.extend_from_slice(&chunk[..take]);
            }
        }
        let consumed = if done { take + 1 } else { take };
        rd.consume(consumed);
        if done {
            return Ok(if overflowed {
                CappedLine::TooLong(seen)
            } else {
                CappedLine::Line
            });
        }
    }
}

/// exec 一条命令并**收全** stdout / stderr / 退出码（见 [`RemoteExec`]）。
///
/// 与 `connect_and_exec_cmd` 一样每次独立连接（一次性查询语义），
/// 不影响长连接流路径。超时由调用方套 `tokio::time::timeout`。
///
/// `abort_marker`：stdout 里一出现这个子串就**立刻收工返回**。存在的理由只有一个 ——
/// **不认参数的旧后端会掉进流模式**（长连接、永不 EOF）。老老实实收到通道关闭，
/// 就只能等调用方的超时兜底，而超时会把「backend 版本过旧」这条最有用的诊断吞成
/// 一句「超时」。给调用方一个字符串就能提前抽身。`None` = 收到底。
pub async fn connect_and_exec_capture(
    cfg: &RemoteConfig,
    cmd: &str,
    abort_marker: Option<&str>,
) -> Result<RemoteExec, String> {
    // 〔C2 → SR1a〕收全三样的活在本机常驻后端里（`use: capture`）；stdout/stderr 各自的上限照旧由这里给。
    crate::dial_host::capture(cfg, cmd, abort_marker, EXEC_CAPTURE_MAX_BYTES).await
}

// 〔THIN〕这里原有 POSIX 单引号转调壳（转 `shell_quote_core::posix_quote`）：最后一个生产调用方（monitor 侧 Gate 1 前检，
//   THIN 第 3 件删）走了 ⇒ 一起删；要 quote 直调 `shell_quote_core::posix_quote`。

/// backend→client 的一帧（解析后的 inbound 表示）。
///
/// 对应 `src/backend::wire::Frame`（外部 `kind` tag，snake_case）。这里**不**
/// 直接 import 那个 crate（它刻意不在 workspace 里、不被 root Cargo 引用，见其 README），
/// 而是用 schema-agnostic 的方式（serde_json::Value + 读 `kind`）解析，只取 Phase-0 需要的
/// 字段。这样：协议演进（backend 加 `build_id` / 加新 kind）不会 break 解析 —— 未知 kind /
/// 多余字段一律忽略（见 `parse_frame`）。
/// 一条**丢了就不可恢复**的帧的身份〔audit-0805 F21〕。
///
/// 与后端侧 `wire::LostFrame` 对应。**故意不共用类型**：那是 backend crate 的私有 wire
/// 形状，monitor 这边是从 JSON 现解的，共用会把两个 crate 绑死在一个结构体上，
/// 而 additive 演进恰恰要求两边能各自容忍对方多/少字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LostFrameInfo {
    pub kind: String,
    pub subject: Option<String>,
}

/// `hello.homes` 的一项 —— **某个 agent 在那台远端机器上的 home 目录**〔backend-split `S4`〕。
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
        /// U6b-2 / U8a-2a：backend 声明**接受哪些入方向命令**（`inbound::COMMANDS`）。
        /// `capabilities` 说的是「我认识哪些流 flag」（出方向），这一条说的是入方向 ——
        /// 两者正交。旧后端无此字段 ⇒ 空集 ⇒ monitor 一条入方向命令都不发。
        commands: Vec<String>,
        /// 〔NET2〕`hello.unavailable`：这台接得下却做不到的 `(命令, 码)`。旧后端无此字段 ⇒ 空（没把握）。
        unavailable: Vec<(String, String)>,
        /// 〔NET2〕`hello.uncancellable`：撤不动的那几条。旧后端无此字段 ⇒ 空（没把握，照旧补发撤单）。
        uncancellable: Vec<String>,
    },
    /// 一行从后端 session jsonl 尾随读到的原始行（远端流与〔CF1〕本机流同一种帧）。字段语义见 [`JsonlLine`]。
    Line {
        session_id: String,
        path: String,
        seq: u64,
        /// 〔MOD〕成品（`message`，缺 ＝ 不进界面）与这条记录自己的 `cwd`。
        message: Option<crate::ui_contract::RecordBody>,
        cwd: Option<String>,
        /// 〔RENDER2 · ㊱②〕后端的 `byte_offset`（这一行末尾含 `\n` 的累计字节）；老后端不带 ⇒ `None`。
        end: Option<u64>,
    },
    /// 远端新出现一个 session 文件。Batch7-F24：p1e backend 附带 pidfile 元信息
    /// （additive）；旧后端缺字段 → None（保守视为交互）。
    SessionAdded {
        sid: String,
        session_kind: Option<String>,
        /// E73（additive）：attach 进去对人有没有意义。缺席 = true（存量零迁移）。
        /// 语义与来源见 `src/backend/stream/wire.rs` 的同名字段 + `src/doc/IPC-PROTOCOL.md` §9.3。
        attachable: Option<bool>,
        cwd: Option<String>,
        name: Option<String>,
        /// Batch8-F25：远端 jsonl 绝对路径（p1f backend 起有值）——旁路快照用。
        path: Option<String>,
        /// Batch8 D-I2：backend prime 时的完整行数 L（快照完整性校验）。
        lines: Option<u64>,
        /// Batch9-F27：宣告时的初始 status/waitingFor（连接建立灯就对）。
        status: Option<String>,
        waiting_for: Option<String>,
        /// 🔴 〔`设计/80 §8.7` 步 3/4，2026-09-23，additive〕这条会话的**启动期令牌**
        /// （环境变量 `CCM_RBIND_TOKEN`，形状 `[0-9a-f]{32}`）。
        ///
        /// 它是 `↗ 拉前终端` 那个 join 的**远端那一半**：`sid ──wire──→ token`，
        /// 再经本地那张表 `token ──→ HWND`（`bind.rs::lookup_hwnd_for_token`）。
        /// 完整论证住 `src/backend/stream/wire.rs` 的同名字段与 `src/doc/IPC-PROTOCOL.md` §9.3。
        ///
        /// **缺席的三种来历，消费侧必须分得开**（`IPC-PROTOCOL.md` 那三句的 monitor 侧版本）：
        /// ① 后端没声明 `rbind-token` 能力 ⇒ monitor 压根没发 `--with-rbind-token`
        ///    ⇒ **诚实降级**回今天的标题路，不是「这条会话没令牌」；
        /// ② 发了 flag 而字段仍缺席 ⇒ **这条会话真的没有令牌**（用户自己裸 `ssh`
        ///    进去敲 `claude` 那一档，`§8.6 ①`）—— 这就是 `§8.5 ②` 要的那个布尔；
        /// ③ 形状不对 ⇒ 本解析器**当没有**（fail closed，见 `parse_frame`）。
        ///
        /// ⚠ **今天没有任何分派读它**：`↗` 改走 join 是 `§8.7` 的**步 4**，
        /// 而 `§8.7` 逐字警告「**不要先做 4**」—— 先改 UI 分派会造出一段
        /// 「令牌还没有、判断已经改」的窗口期。本字段今天买到的是「**键到手了**」。
        rbind_token: Option<String>,
        /// 〔U4b · 第四波，additive〕这条活会话住在什么容器里（`tmux` / `none`）。
        /// 缺席 / 不认识的取值 ⇒ `None` = 不知道（**不是**「不在 tmux 里」）。
        /// 〔MIG-1〕进 `session_book` 的活会话成品（本机那条流同一个口）。
        container: Option<crate::session_book::Container>,
        /// 〔LOC1b · 第四波 4D，additive〕那个 claude 进程的 pid。本机活会话的成品（`session_book::LiveMeta::pid`）
        /// 拿它给本机 ↗ 绑窗口（`bind::SidHwndCache::record`）；老后端不带 ⇒ `None`。远端那一支不读它。
        pid: Option<u32>,
    },
    /// 〔U4b · 第四波〕后端的活会话清单报完了（Phase 1 走完）。无载荷。
    SessionsReplayed,
    /// 〔FW1 · 第四波 4D · D-d〕活会话的记录文件不见了（`session_file_gone`）/ 被改过已从头重读（`session_file_reread`）。
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
    /// 远端一个 session 文件消失。〔MIG-1〕monitor 只拿它当内容流的边界（残批先冲、快照作废）；
    /// 它离开之后是可重连还是已结束，紧跟着的 [`InboundFrame::SessionState`] 说（后端裁）。
    SessionRemoved { sid: String },
    /// 〔MIG-1 · `99 §2.1 ⑬`〕后端会话账本的成品：这条会话离开「活」之后是什么（`session_state`）。
    SessionState { sid: String, state: Fate },
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
    // 〔MIG-1 续 · V41〕这里原是后端 tmux 观测两帧（整份快照 · 差分出的正向死亡）：收割与「可重连」进了那台后端的会话账本、
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
    /// 〔SR1a · `设计/05 §13.6 ③`〕那台机器上的账号清单变了（后端 `wire::Frame::AccountsChanged`，无载荷）。
    AccountsChanged,
    /// 〔MIG-3b · `99 §2.1 ㉓②`〕那台机器上某个会话的任务清单变了（后端 `wire::Frame::TasksChanged`，只带 sid）。
    TasksChanged { sid: String },
    /// 〔SR1a〕一条链路的下行字节（后端 `wire::Frame::LinkData`；`data` 在解帧这一步就解开了 base64）。
    /// 只有**本机后端**那条流上会有（monitor 只在那条流上开链路），交 `link_mux`。
    LinkData { link: String, data: Vec<u8> },
    /// 〔SR1a〕一条链路收尾了（后端 `wire::Frame::LinkEnd`）。
    LinkEnd { link: String, error: Option<String> },
    /// 〔SR1b〕一趟传输此刻的样子（后端 `wire::Frame::Transfer`）。只有**本机后端**那条流上会有
    /// （传输台住本机后端），交 `sftp_pool::deliver`。`end` 解不动 ⇒ 整帧 `None`（坏帧）。
    Transfer {
        id: String,
        got: u64,
        total: u64,
        end: Option<crate::sftp_pool::End>,
    },
    /// 〔TAP · V124〕中转抄出来的一个 SSE 事件 / 一个响应的收尾（后端 `wire::Frame::Tap`）。只有**本机后端**那条流上会有
    /// （中转住本机常驻后端），交 `session_tap::deliver`。`data` / `end` 都缺、或 `end` 认不出 ⇒ 整帧 `None`（坏帧）。
    Tap(crate::session_tap::Tap),
    /// 〔MIG-1 收尾〕测试连接那一趟的一格进度 / 结局（后端 `wire::Frame::Probe`）。只有**本机后端**那条流上会有
    /// （测试连接在本机常驻后端里跑），交 `probe_relay::deliver`。`cell` 原样（一个 JSON 对象的文本，monitor 不解释）。
    Probe { ticket: String, cell: String },
}

/// 拥塞提示的**措辞**：有没有不可恢复的丢失，说法完全不同〔audit-0805 F21〕。
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
fn overflow_health_message(
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
    // 〔MOD〕内容帧是最热的那一种：按类型直解，成品（`message`）以原文收下、不建 `Value`（monitor 不读它的字段）。
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
        }
        if let Ok(f) = serde_json::from_str::<LineFrame>(line) {
            return Some(InboundFrame::Line {
                session_id: f.session_id,
                path: f.path,
                seq: f.seq,
                message: f.message.map(crate::ui_contract::RecordBody),
                cwd: f.cwd,
                end: f.byte_offset,
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
            // 〔NET2〕能力事实的另两格（additive，同上口径：坏项逐项丢，不丢整帧）。〔RESYNC〕读法住 `Offer::facts_of`（`resync` 应答同形）。
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
        // 〔MOD〕一般走不到这里（`line` 帧在上面按类型直解，成品不经 `Value`）；形状不是那一形时落到这里再认一次。
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
            Some(InboundFrame::Line {
                session_id,
                path,
                seq,
                message,
                cwd,
                end,
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
                name: opt("name"),
                path: opt("path"),
                lines: obj.get("lines").and_then(|v| v.as_u64()),
                status: opt("status"),
                waiting_for: opt("waiting_for"),
                // 🔴 `设计/80 §8.7` 步 4 的读侧。**形状 fail closed**：不 `trim`、
                // 不认大写、长度必须恰好 32 —— 任何偏离一律当**没有**，而不是当
                // 「大概是它」。理由与后端读侧（`control::identity_tag::rbind_token_of`）
                // 逐字同一条：`§8.5 ②` 买的那个布尔（「这个会话有没有令牌」）
                // 只有在「有 ⇒ 形状确定对」时才说得准。
                //
                // ⚠ 形状那一条**不在这里重写一遍** —— 过 `bind::rbind_token_shape_ok`，
                // 即**本地那张 `token → HWND` 表用的同一条**。两处各写一遍的后果是
                // join 在某些取值上静默失配，而失配与「没有令牌」在界面上同形。
                rbind_token: opt("rbind_token").filter(|t| crate::bind::rbind_token_shape_ok(t)),
                // 〔U4b〕两个字面量之外一律当不知道（`Container::from_wire`）。
                container: opt("container")
                    .as_deref()
                    .and_then(crate::session_book::Container::from_wire),
                // 〔LOC1b〕只认装得进 u32 的非负整数；别的一律当没带。
                pid: obj
                    .get("pid")
                    .and_then(|v| v.as_u64())
                    .and_then(|n| u32::try_from(n).ok()),
            })
        }
        // 〔U4b · 第四波〕additive 新帧，无载荷。旧后端不发 ⇒ 这条分支永不命中，固定的 tab 停在「说不清」。
        "sessions_replayed" => Some(InboundFrame::SessionsReplayed),
        // 〔FW1 · 第四波 4D〕additive 新帧。`why` 认不出 ⇒ 整帧当坏帧跳过（不猜成哪一种）。
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
        "session_removed" => Some(InboundFrame::SessionRemoved {
            sid: obj.get("sid")?.as_str()?.to_string(),
        }),
        // 〔MIG-1〕会话账本的成品。`state` 认不出 ⇒ 整帧当坏帧跳过（不猜成哪一种）。
        "session_state" => Some(InboundFrame::SessionState {
            sid: obj.get("sid")?.as_str()?.to_string(),
            state: Fate::from_wire(obj.get("state")?.as_str()?)?,
        }),
        "overflow" => {
            // issue #32：dropped 必需且为数字；缺/错则当坏帧跳过（不 panic）。
            let dropped = obj.get("dropped")?.as_u64()?;
            // 〔audit-0805 F21〕additive：**缺字段必须仍能解析** —— 旧后端还在跑，
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

        // 〔SR1a · `设计/05 §13.6 ③`〕账号清单变了。
        "accounts_changed" => Some(InboundFrame::AccountsChanged),
        // 〔MIG-3b · ㉓②〕某个会话的任务清单变了（sid 缺 / 不是串 ⇒ 坏帧）。
        "tasks_changed" => Some(InboundFrame::TasksChanged {
            sid: obj.get("sid")?.as_str()?.to_string(),
        }),

        // 〔SR1a〕链路两帧。`data` 解不开 ⇒ 整帧 `None`（坏帧，调用方 warn）—— 不交一段猜出来的字节。
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

        // 〔SR1b〕传输进度 / 终局。`end` 在 ⇒ 必须是后端那三形之一，认不出 ⇒ 整帧 `None`（不猜一个结局）。
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

        // 〔MIG-1 收尾〕测试连接的一格进度：票 ＋ 原样那一格（必须是对象；内容由界面严格收）。
        "probe" => {
            let ticket = obj.get("ticket")?.as_str()?.to_string();
            let cell = obj.get("cell").filter(|c| c.is_object())?.to_string();
            Some(InboundFrame::Probe { ticket, cell })
        }

        // 〔TAP · V124〕中转抄出来的 SSE 事件。`data` 与 `end` 恰有一个：先认 `data`（原样，一个串），没有就必须是认得的 `end`。
        "tap" => {
            let stream = obj.get("stream")?.as_str()?.to_string();
            let resp = obj.get("resp")?.as_u64()?;
            let n = obj.get("n")?.as_u64()?;
            let body = match obj.get("data") {
                Some(d) => crate::session_tap::TapBody::Data(d.as_str()?.to_string()),
                None => crate::session_tap::TapBody::End(crate::session_tap::TapEnd::from_wire(
                    obj.get("end")?.as_str()?,
                )?),
            };
            Some(InboundFrame::Tap(crate::session_tap::Tap {
                stream,
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
    "reply",
    "session_added",
    "session_file_gone",
    "session_file_reread",
    "session_removed",
    "session_state",
    "session_status",
    "sessions_replayed",
    "tap",
    "transfer",
    "turn_end",
];

/// 〔SR1b〕`transfer` 帧的 `end`：后端 `wire::TransferEnd` 那三形之一；认不出 ⇒ `None`（调用方整帧丢）。
/// 抽出来住 `parse_frame` 外面：那张 match 的臂是帧 kind 的名单（`known_kinds_matches_parse_frame` 按臂抠），
/// 结局的三个名字不该混进去。
fn transfer_end(e: &serde_json::Value) -> Option<crate::sftp_pool::End> {
    Some(match e.get("state")?.as_str()? {
        "done" => crate::sftp_pool::End::Done {
            bytes: e.get("bytes")?.as_u64()?,
            // 〔FW1〕可缺席（下载那一路没有）；在就原样带着（窗口提交时交回，形状由后端那一关判）。
            sha256: e.get("sha256").and_then(|v| v.as_str()).map(str::to_string),
        },
        // 〔FILES2 · Q5〕带码的那一形（今天只有 `sftp_home_mismatch`）单列一形，窗口按码换路。
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
#[path = "../../../../tests/frontend/shell/ssh_source_emits_parity.rs"]
mod emits_parity;

/// U8a-2a：**握手完成 ⇒ 写半边解冻。**
///
/// 见证只能由一帧真的 Hello 换出来（`BackendHello::from_hello_frame`）⇒
/// 「hello 之前不许写」在 monitor 侧是类型上的事实，不是一条纪律。
/// 第二次 hello（不该有）时 `parked` 已被 `take` 走，静默跳过。
///
/// **抽成函数是为了让它可测**：D 审计变异 MU13 —— 把这段逻辑整个删掉（写半边永不解冻、
/// 客户端永不登记）⇒ `cargo test` **全绿**。它埋在 `stream_loop` 中段时没有任何判据碰得到。
fn attach_inbound_client<W>(
    host_label: &str,
    parked: &mut Option<crate::inbound_client::ParkedWriter<W>>,
    frame: Option<&InboundFrame>,
) -> Option<std::sync::Arc<crate::inbound_client::InboundClient>>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let witness = crate::inbound_client::BackendHello::from_hello_frame(frame?)?;
    let client = parked.take()?.into_client(witness);
    crate::inbound_client::register(host_label, client.clone());
    Some(client)
}

/// U8a-2a：把一帧入方向应答路由回请求方。返回是否真的交到了某个等待者手上。
///
/// 没有客户端 = backend 在 hello 之前就回了应答（协议倒错），照实报、不静默。
///
/// **抽成函数同样是为了可测**（D 审计变异 MU12：把 `route_reply` 换成丢弃 ⇒ 全绿）。
fn route_inbound_frame(
    host_label: &str,
    client: Option<&std::sync::Arc<crate::inbound_client::InboundClient>>,
    frame: InboundFrame,
) -> bool {
    let (kind, id) = match &frame {
        InboundFrame::Reply { id, .. } => ("reply", id.clone()),
        InboundFrame::Cancelled { id } => ("cancelled", id.clone()),
        other => {
            tracing::warn!("route_inbound_frame 收到非入方向帧，忽略：{other:?}");
            return false;
        }
    };
    let Some(c) = client else {
        tracing::warn!(
            "ssh_source [{host_label}] 收到 {kind}(id={id})，但本连接还没有入方向客户端 —— backend 在 hello 之前就回应答了？"
        );
        return false;
    };
    match frame {
        InboundFrame::Reply {
            id,
            ok,
            code,
            message,
            data,
        } => c.route_reply(&id, ok, code, message, data),
        InboundFrame::Cancelled { id } => c.route_cancelled(&id),
        _ => false,
    }
}

/// U8a-2a：`stream_loop` 那条**接缝**的判据。
///
/// # 为什么单独立一个模块
///
/// D 审计做了三次变异，三次 `cargo test` **全绿**：
/// - MU13：hello 臂里不 `into_client`/不 `register`（写半边永不解冻）
/// - MU12：`reply` 臂里不 `route_reply`（应答收到就扔）
/// - MU14：`probe_control_channel`〔散文墓碑〕 直接返回 `"control=ok(0ms)"`，一个字节都不发
///
/// 也就是「把发送端接上」这件事本身删掉之后 CI 一片绿 —— `inbound_client` 的单测走的是
/// 自造客户端，e2e 走的是真后端二进制，**两者之间的接缝没有任何判据**。
/// 这个模块就是那条接缝。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_seam_tests.rs"]
mod seam_tests;

/// U8a-2a：**写半边只许经 `inbound_client::park` 出手。**
///
/// 这条护栏存在的理由是它守的东西刚变过：这个文件从「只读一条流」变成了「双工」。
/// 一旦有人为了图省事在这里直接 `write_all` 一行，`ParkedWriter` 那层
/// 「Hello 之前不许写」的类型保证就被绕过了 —— 而且是**静默**绕过（编译、测试全绿）。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_write_half_guard.rs"]
mod write_half_guard;

// ============================================================================
// 版本协商（issue #33）：连接时比对后端的 hello.v / hello.build_id。
// ============================================================================

/// 本 monitor 期望的流式 wire 协议大版本。与后端的 `PROTO_VERSION` 对齐（语义同值）。
/// 类型用 `u64` 而非后端侧的 `u32`：JSON 数字无符号宽度之分，`parse_frame` 用
/// `as_u64()` 读 `v`，这里与之同宽以便直接比较，无需转换。
const EXPECTED_PROTO_V: u64 = 1;

/// 本 monitor 期望的 backend build_id。
///
/// **SS-B（issue #33/#29）已单源**：值来自编译期 env `BACKEND_BUILD_ID`，由 `build.rs` 从
/// `src/backend/lib.rs::BUILD_ID` 抠出 emit——与后端源码、F08b 内嵌二进制的
/// build_id **同一事实源**，无需手工同步（F08b 消除了 F06 时的手工同步债）。
pub(crate) const EXPECTED_BACKEND_BUILD_ID: &str = env!("BACKEND_BUILD_ID");

/// F66（#58③）：monitor **内嵌** backend 声明的能力 token（= backend `lib.rs::CAPABILITIES`）。
///
/// 用途：部署侧确认「装的是当前内嵌 build」（`confirmed_build == EXPECTED_BACKEND_BUILD_ID`）
/// 时，第一次连接还没收到 hello，用这份常量预知后端能力、直接发对应 flag——省一轮
/// 「降级→收 hello→重连升级」的往返（等价旧 build_id 门控的乐观路径，但换成能力粒度）。
/// 收到真实 hello 后一律以 backend **自报**的 `capabilities` 为准（见 `hello_confirmed`）。
///
/// **单一事实源（SS-B，同 `EXPECTED_BACKEND_BUILD_ID`）**：值来自 `build.rs::emit_backend_
/// capabilities` 从 backend `lib.rs::CAPABILITIES` 抠出的编译期 env `BACKEND_CAPABILITIES`
/// （逗号分隔）——**不再手抄**（审计 B1/S1：手抄副本漂移时乐观路径可能声明当前 backend
/// 不剥离的 flag → §26 死循环窄窗；单源杜绝之）。
fn embedded_backend_capabilities() -> Vec<String> {
    env!("BACKEND_CAPABILITIES")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// 版本协商结论（纯函数 [`negotiate_version`] 的产物）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionVerdict {
    /// 协议版本 + build_id 都匹配 —— 无需提示。
    Ok,
    /// 协议版本相同，但 build_id 不同 —— backend 偏旧/偏新，建议更新（非阻断，F08 将自动重推）。
    StaleBuild { reported: String },
    /// 协议大版本不符 —— 渲染可能异常，醒目提示需更新后端（仍不 hard-disconnect：
    /// 解析器向前兼容，能解析的仍照常呈现）。
    Incompatible { reported_v: u64 },
}

/// 纯函数版本协商：协议版本优先于 build_id（协议不兼容是更严重的问题）。
///
/// - `reported_v != EXPECTED_PROTO_V` → `Incompatible`（无论 build_id）。
/// - 协议同、`reported_build_id != EXPECTED_BACKEND_BUILD_ID` → `StaleBuild`。
/// - 全同 → `Ok`。
fn negotiate_version(reported_v: u64, reported_build_id: &str) -> VersionVerdict {
    if reported_v != EXPECTED_PROTO_V {
        VersionVerdict::Incompatible { reported_v }
    } else if reported_build_id != EXPECTED_BACKEND_BUILD_ID {
        VersionVerdict::StaleBuild {
            reported: reported_build_id.to_string(),
        }
    } else {
        VersionVerdict::Ok
    }
}

/// 把协商结论变成给用户看的提示文案（`None` = 兼容、无需提示）。`label` 是出问题的远端机器。
/// `remote_older` = 接上那一刻本机常驻后端答的「那台比手上这一版旧」（`remote_resident::Replayed::remote_is_older`）。
fn version_warning(
    reported_v: u64,
    reported_build_id: &str,
    label: &str,
    remote_older: bool,
) -> Option<String> {
    match negotiate_version(reported_v, reported_build_id) {
        VersionVerdict::Ok => None,
        // 〔HX2 · 主会话 D-b〕按新旧分两句（部署只升不降）：
        //   那台旧 ⇒ 下次连上的部署预检会换掉它；那台不比这一版旧 ⇒ 这个 monitor 不会把它换回去。
        //   〔THIN〕新旧不在这里比（从前调共享判定 `is_newer`，今天住后端 `control/deploy_plan.rs`）：本机常驻后端接上那一刻判过（`resident-verdict`），这里只按答挑句子。
        //   〔墓碑 —— 从前一句话不分新旧（`rsSshSource.version.buildMismatch`：「…建议更新后端（后续将支持自动部署）」），自动部署早已落地。〕
        VersionVerdict::StaleBuild { reported } if remote_older => Some(copy_text(
            "rsSshSource.version.remoteOlder",
            &[
                ("label", &label.to_string()),
                ("reported", &reported.to_string()),
                ("mine", &EXPECTED_BACKEND_BUILD_ID.to_string()),
            ],
        )),
        VersionVerdict::StaleBuild { reported } => Some(copy_text(
            "rsSshSource.version.remoteNotOlder",
            &[
                ("label", &label.to_string()),
                ("reported", &reported.to_string()),
                ("mine", &EXPECTED_BACKEND_BUILD_ID.to_string()),
            ],
        )),
        VersionVerdict::Incompatible { .. } => Some(copy_text(
            "rsSshSource.version.protoMismatch",
            &[("label", &label.to_string())],
        )),
    }
}

/// SSH-remote 数据源主循环（S5）。
///
/// 连接远端、exec backend、把 backend stdout 的 line-delimited JSON 帧逐行解析后分发：
/// - `hello` → log（证明 backend runtime 起来了）+ 置 `connected`（标记本次连接已健康，
///   供重连循环判定是否重置退避）。
/// - `line` → 组 [`JsonlLine`] 交 [`LineIntake`]（〔CF1〕本机那条流用的是同一个）：
///   攒批后 `crate::batch_to_payloads(...)` → `replay.on_line_batch_awaited(&app, ...)`
///   （前端按 seq 自动排序）。
/// - 〔MIG-1〕会话起停的成品（`session_added` · `session_status` · `session_state` · `sessions_replayed`）→ 原样交
///   `session_book::feed`（后端裁、monitor 只转交；本机那条流同一个口）；连接断了 ⇒ `session_book::In::LinkLost`。
/// - 未知 kind / garbage → `tracing::warn!` 跳过，绝不中断流。
///
/// stdout EOF / 读错误 → 返回 `Err`，调用方（S8/S9）据此大声报"connection dropped"，
/// 不静默冻结。
pub async fn run(
    cfg: RemoteConfig,
    replay: Arc<EventReplay>,
    app: tauri::AppHandle,
    connected: Arc<AtomicBool>,
) -> Result<(), String> {
    tracing::info!(
        "ssh_source connecting to {}@{}:{}",
        cfg.user,
        cfg.host,
        cfg.port
    );

    // 重连循环：每轮跑一次 stream_loop。失败/掉线后按指数退避（2→4→8→16→30s 封顶）重连；
    // 本轮**连上过**（收到 backend hello，connected=true）则下次立即以 MIN 快速重连。
    // INVARIANT §10：唯一的等待是 tokio::time::sleep（async、非阻塞），绝不 std::thread::sleep。
    let mut backoff = RECONNECT_MIN;
    // v2.22.1 hello 自愈账本:上一轮 hello 自证 backend==当前版本时记账,下一轮以此
    // 越过「部署侧确认失败」的降级(内嵌清单缺失的 CI 安装包 v2.19-v2.22 全中招)。
    // 若带 flag 的一轮连 hello 都没收到(真·旧后端把未知参数当一次性查询退出),
    // 清账回退降级,防止 flagged 重连死循环。
    // F66（#58③）：hello 自愈账本——存上一轮 backend **自报的能力 token 集**（原为
    // build_id）。None = 尚未收到能力声明；Some(caps) = 下一轮据此发 flag 升级。
    // 回退清账语义（`:is_some()` 那段）不变。
    let mut hello_confirmed: Option<Vec<String>> = None;
    // 〔DEL 续 · 主会话裁〕这台是不是「永久不支持」（非 unix）：`stream_loop` 接不上常驻时写，本循环读完即清。
    let mut unsupported: Option<String> = None;
    loop {
        connected.store(false, Ordering::Release);
        // F05：本轮连接的起点。退避重置的判据是「活过多久」，不是「握没握上手」。
        let conn_started = std::time::Instant::now();
        // 🔴 `K-R59`（定框 `K35`）：这里原来是**顶层二选一** —— `cfg.daemonless` 为真时
        //    走纯 exec tail 轮询（`daemonless_stream_loop`），否则走后端流。
        //    那一档整个没了 ⇒ **只剩一条路**：连后端。
        let result = stream_loop(
            &cfg,
            &replay,
            &app,
            &connected,
            &mut hello_confirmed,
            &mut unsupported,
        )
        .await;
        // 〔CF2〕这条连接没了 ⇒ 订了这台会话流的那些订阅原位收一格 `Unseen`（不是终点，`05 §4.5.2`）。
        replay.origin_seen(&crate::origin::Origin(cfg.origin_label()), false);
        if hello_confirmed.is_some() && !connected.load(Ordering::Acquire) {
            tracing::warn!("ssh_source hello 自愈轮未收到 hello,回退降级模式(backend 可能被换旧)");
            hello_confirmed = None;
        }
        // 〔MIG-1 · GP1〕连接断了 ≠ 会话死了：这台的成品整份作废，当时活的 / 可重连的一律「说不清」（`设计/30 §3.5.7a`）；
        //   重连之后那台的新连接自己重报一遍（它的账本从 tmux 推出可重连，`observe/session_ledger.rs`）。
        tracing::info!(
            "ssh_source [{}] connection ended ⇒ 这台的会话成品作废（说不清）",
            cfg.origin_label()
        );
        crate::session_book::feed(BookIn::LinkLost {
            origin: cfg.origin_label(),
        });
        match &result {
            Ok(()) => tracing::warn!("ssh_source stream returned Ok unexpectedly; reconnecting"),
            Err(e) => tracing::warn!("ssh_source remote source ended: {e}"),
        }
        // 两段式（非冗余）：先按**当前** backoff 睡，再在仍没连上时翻倍。这样首次失败也只等
        // MIN，退避序列是 2→4→8→16→30；若收成单个 if/else（睡前就翻倍），首次失败会直接等 4s。
        // sleep 期间 `connected` 不会变（其唯一写者 stream_loop 已返回），故两次 load 读到同值。
        // F05（报告 I-1）：**「连上过」不等于「站住了」**。判据从「收到过 hello」换成
        // 「这条连接活过 MIN_HEALTHY_UPTIME」——hello-then-die 的后端此前每轮都算连上过，
        // 退避永远重置回 2s、每分钟约 90 次 SSH 握手砸在那台已经撑不住的机器上。
        if should_reset_backoff(connected.load(Ordering::Acquire), conn_started.elapsed()) {
            backoff = RECONNECT_MIN; // 本次真站住过 → 下次立即快速重连
        }
        let wait = match after_round(unsupported.take(), backoff) {
            AfterRound::RetryIn(d) => d,
            AfterRound::Stop(why) => {
                // 〔DEL 续〕出声（界面 toast），然后这条流就此收工；机器页「起」会重起一条、再试一次。
                let payload = crate::ui_contract::RemoteHealthPayload {
                    origin: cfg.origin_label(),
                    kind: "unsupported".to_string(),
                    message: why.clone(),
                };
                if let Err(e) = app.emit(crate::ui_contract::events::REMOTE_HEALTH, payload) {
                    tracing::warn!("ssh_source remote-health (unsupported) emit failed: {e}");
                }
                return Err(why);
            }
        };
        tracing::info!("ssh_source reconnecting in {:?}", wait);
        tokio::time::sleep(wait).await;
        if !connected.load(Ordering::Acquire) {
            backoff = next_backoff(backoff); // 仍没连上 → 指数退避增长
        }
    }
}

/// Line 帧攒批缓冲（Batch5-F17）。
///
/// backend 线协议没有批量帧（一行一帧），首连 snapshot 的几千行历史若逐帧调
/// 一批一条地交重放缓冲，恒 1 < INCREMENTAL_BATCH_THRESHOLD → 全部走
/// 逐条 jsonl-line live 渲染管线（v2.4.2 给本地修掉的逐行刷屏在远端重现）。
/// 客户端把**连续到达**的 Line 帧聚合成批再交重放缓冲：snapshot 密集
/// 连发天然聚成大批 → 自动跨过阈值复用 chunked 回放路径；日常单行增量
/// 只多一个静默窗口（~30ms）的延迟。时序判定（静默窗口）留在 [`LineIntake::recv_or_flush`]
/// 的 `tokio::time::timeout` 里；本结构只管容量与顺序，纯逻辑可直测。
struct Batcher {
    pending: Vec<JsonlLine>,
    cap: usize,
    /// 首行入缓冲的时刻——批龄上限用（F17 审计 R3：帧间隔持续 < 静默窗口时
    /// 永不静默，首行可见延迟无界；批龄到点强制 flush 双保险）。
    born: Option<std::time::Instant>,
}

impl Batcher {
    fn new(cap: usize) -> Self {
        Self {
            pending: Vec::new(),
            cap,
            born: None,
        }
    }

    /// 收一行；达容量上限或批龄超限则返回整批待发（防无界内存/无界延迟）。
    fn push(&mut self, line: JsonlLine) -> Option<Vec<JsonlLine>> {
        if self.pending.is_empty() {
            self.born = Some(std::time::Instant::now());
        }
        self.pending.push(line);
        let over_age = self
            .born
            .is_some_and(|b| b.elapsed().as_millis() as u64 >= BATCH_MAX_AGE_MS);
        if self.pending.len() >= self.cap || over_age {
            return self.take();
        }
        None
    }

    /// 取走全部待发行（空则 None）。到达顺序 = 发出顺序（backend per-file seq
    /// 单调，前端按 seq 排序，跨 session 混流不需要拆分）。
    fn take(&mut self) -> Option<Vec<JsonlLine>> {
        self.born = None;
        if self.pending.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.pending))
        }
    }
}

/// 攒批静默窗口：一行到达后最多再等这么久看有没有后续行。首连 snapshot 帧间
/// 隔远小于此值 → 聚合；live 单行只付一次窗口的延迟（对流式渲染无感）。
const BATCH_QUIET_MS: u64 = 30;
/// 单批行数上限（与本地 replay CHUNK_SIZE 同量级，控制单次 IPC 体积）。
const BATCH_CAP: usize = 600;
/// 批龄上限：无论帧流多密集，首行入缓冲后最迟这么久必 flush（见 Batcher.born）。
const BATCH_MAX_AGE_MS: u64 = 200;

/// 攒批出口（Batch5-F17）：〔CF1〕远端流、本机流、旁路快照三路的行**都**从这里出去
/// （`batch_to_payloads` → `on_line_batch_awaited`），用 **awaited 变体**——大批的块序列发完才返回，保证行
/// emit 严格先于随后的 SessionRemoved/断连归档（审计 R1：spawn 化的行若晚于
/// ended 格 到达前端，会把刚归档的远端 Tab 复活成僵尸 live），同时对
/// backend 帧流形成天然背压。
async fn flush_lines(
    replay: &Arc<EventReplay>,
    host_label: &str,
    lines: Vec<JsonlLine>,
    runs: &mut crate::SkipRuns,
) {
    let flushed: Vec<(String, u64, Option<u64>)> = lines
        .iter()
        .map(|l| (l.session_id.clone(), l.seq, l.end))
        .collect();
    // 〔ST3〕同一个 origin 既是载荷上的机器名、也是看不懂的行记账的那台。
    let origin = crate::origin::Origin(host_label.to_string());
    let payloads = crate::batch_to_payloads(lines, &origin, runs);
    // 〔CF2〕交给订了它的那些会话流（`event_replay` 头注「订阅」）；出口在它手里，不再经 `app` 广播。
    replay.on_line_batch_awaited(payloads).await;
    // 〔C2〕发出去了才推续点（连续才推，见 `snapshot_resume::note_flushed`）。
    crate::snapshot_resume::note_flushed(
        &origin,
        flushed.iter().map(|(s, q, e)| (s.as_str(), *q, *e)),
    );
}

/// 〔CF1 · 2026-09-24〕**内容那一半的唯一收口** —— 远端每条连接一个、本机每条流一个。
///
/// # 为什么要它
///
/// `设计/01 §6.1`「一条流，一个来源；本机与远端走同一条帧路」。本机那条流改走后端的 `line` 帧之后，
/// 「行怎么攒批、什么时候冲、历史怎么旁路补、会话走了撤什么」这几件事若在本机再写一份，
/// 就又是两份实现（`真相源/10 §7.2` 那一形）。⇒ 收成一个结构，**两个帧源各构造一次**：
/// [`stream_loop`]（远端）与 [`consume_local`]（本机）。判据钉的就是「构造点恰好这两处」。
///
/// # 它管什么、不管什么
///
/// 管：[`Batcher`] ＋ 带静默窗的收（[`LineIntake::recv_or_flush`]）· 旁路快照队列与分发器（`tail_only` 时起）·
/// 续点（`snapshot_resume`）。**不管会话的起停**：那是后端出的成品，两条流各自交 `session_book`（〔MIG-1〕）。
///
/// 丢掉它 ⇒ 快照队列当场关（与原来 `stream_loop` 里那个 `SnapshotQueueCloser` 同一个时机）。
pub(crate) struct LineIntake {
    origin_label: String,
    replay: Arc<EventReplay>,
    batcher: Batcher,
    snapshots: std::sync::Arc<SnapshotQueue>,
    tail_only: bool,
    /// 〔RENDER2〕实时那一路的「连着的不可显示那一段」（`SkipRuns`）。
    runs: crate::SkipRuns,
    _closer: SnapshotQueueCloser,
}

impl LineIntake {
    /// `tail_only`：这条流的后端是不是按「不重放历史」起的 —— 是 ⇒ 历史走旁路快照（起分发器）。
    fn open(
        origin_label: String,
        tail_only: bool,
        replay: &Arc<EventReplay>,
        app: &tauri::AppHandle,
    ) -> Self {
        let snapshots = SnapshotQueue::new();
        if tail_only {
            tauri::async_runtime::spawn(snapshot_dispatcher(
                snapshots.clone(),
                replay.clone(),
                app.clone(),
                origin_label.clone(),
            ));
        }
        LineIntake {
            origin_label,
            replay: replay.clone(),
            batcher: Batcher::new(BATCH_CAP),
            _closer: SnapshotQueueCloser(snapshots.clone()),
            snapshots,
            tail_only,
            runs: crate::SkipRuns::default(),
        }
    }

    /// 带静默窗地收下一件：手里攒着行时最多等 [`BATCH_QUIET_MS`]，窗内没来新东西就先把攒的冲掉再回去等。
    ///
    /// ⚠ 超时打在 `recv` 上（cancel-safe），不打在读行上 —— 理由见 `stream_loop` 里那个读帧任务的注释。
    async fn recv_or_flush<T>(&mut self, rx: &mut tokio::sync::mpsc::Receiver<T>) -> Option<T> {
        loop {
            if self.batcher.pending.is_empty() {
                return rx.recv().await;
            }
            match tokio::time::timeout(Duration::from_millis(BATCH_QUIET_MS), rx.recv()).await {
                Ok(m) => return m,
                Err(_) => self.flush().await,
            }
        }
    }

    /// 收一行（达容量 / 批龄就整批冲出去）。
    async fn line(&mut self, line: JsonlLine) {
        if let Some(full) = self.batcher.push(line) {
            flush_lines(&self.replay, &self.origin_label, full, &mut self.runs).await;
        }
    }

    /// 把攒着的行冲出去（攒批边界：会话走了 / 流断了 / 静默窗到了）。
    async fn flush(&mut self) {
        if let Some(lines) = self.batcher.take() {
            flush_lines(&self.replay, &self.origin_label, lines, &mut self.runs).await;
        }
    }

    /// 一个会话被宣告了：tail-only 下带 `path` 就排一份旁路快照（无 path = 会话刚起还没写 jsonl ⇒ 无历史可拉）。
    fn announced(&self, sid: &str, path: Option<String>, lines: Option<u64>) {
        if !self.tail_only {
            return;
        }
        if let Some(p) = path {
            self.snapshots.push(SnapshotItem {
                sid: sid.to_string(),
                path: p,
                expected_lines: lines,
            });
        }
    }

    /// 〔FW1 · 第四波 4D · D-d〕一个会话的记录文件不见了 / 被改过已从头重读：残批先冲（出声那一格排在它之前的行后面、
    /// 重读出来的行前面 —— 后端发它就在重读的行之前），再交那个会话的内容流一格。
    async fn notice(&mut self, sid: &str, path: &str, change: FileChange) {
        self.flush().await;
        // 〔RENDER2 · `设计/10 §3.2`〕从头重读 ⇒ 后端的行号从 0 重数：在飞 / 排队的快照（旧的一代）撤掉、续点作废。
        //   （与「会话走了」同一件事：`removed`）。留存里旧的一代由 `on_session_notice` 同一拍丢。
        if change != FileChange::Gone {
            self.removed(sid);
        }
        self.replay
            .on_session_notice(crate::ui_contract::SessionFileNoticePayload {
                session_id: sid.to_string(),
                origin: self.origin_label.clone(),
                path: path.to_string(),
                change: change.as_wire().to_string(),
            })
            .await;
    }

    /// 〔RENDER2 · `99 §2.1` ㉓①〕这条流上有一行丢了、说不出是哪个会话的哪一行（超长整行丢弃）：残批先冲，
    /// 再给订了这台的每条订阅原位一格「丢了、不知道丢到哪」（`Item::Gap` 的 `to_seq` 缺）——前端照 `05 §15.3` 往后补。
    async fn lost(&mut self) {
        self.flush().await;
        self.replay.on_lost_somewhere(&self.origin_label);
    }

    /// 一个会话走了：撤它的快照（排队的摘掉、在飞的打取消标记）、续点作废（再宣告时整份拉）。
    fn removed(&mut self, sid: &str) {
        self.runs.forget(sid);
        self.snapshots.cancel(sid);
        crate::snapshot_resume::forget(&crate::origin::Origin(self.origin_label.clone()), sid);
    }
}

/// 〔FW1 · 第四波 4D · D-d〕活会话的记录文件怎么了。线上（后端 `session_file_gone` / `session_file_reread.why`）与交前端的
/// 那一格（`SessionFileNoticePayload.change`）同一组字面量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChange {
    /// 不见了（删了 / 改名走了）。
    Gone,
    /// 变短了，已从头重读。
    Truncated,
    /// 游标之前被原地改写过，已从头重读。
    Rewritten,
}

impl FileChange {
    /// `session_file_reread.why` → 那一形；认不出 ⇒ `None`（整帧跳过，不猜）。
    pub fn reread_from_wire(why: &str) -> Option<Self> {
        match why {
            "truncated" => Some(FileChange::Truncated),
            "rewritten" => Some(FileChange::Rewritten),
            _ => None,
        }
    }

    /// 交前端那一格的字面量。
    pub fn as_wire(self) -> &'static str {
        match self {
            FileChange::Gone => "gone",
            FileChange::Truncated => "truncated",
            FileChange::Rewritten => "rewritten",
        }
    }
}

/// 〔CF1〕本机那条流交进来的东西（`local_lines` 通道上的一件）。
#[derive(Debug)]
pub(crate) enum LocalItem {
    /// 读循环从 `absorb_local_frame` 手里接回的内容帧（`line` / `session_added` / `session_removed`）。
    Frame(InboundFrame),
    /// 这条流结束了（两条读循环的收尾各送一次）。
    StreamEnded,
    /// 〔RENDER2 · `99 §2.1` ㉓①〕这条流上一行超长、整行丢了（说不出是哪个会话的哪一行）。
    LineLost,
}

/// 〔CF1〕本机消费者对一件东西的处置 —— **纯函数**的输出，异步那半只照做。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LocalStep {
    /// 进 [`LineIntake::line`]。
    Line {
        session_id: String,
        path: String,
        seq: u64,
        message: Option<crate::ui_contract::RecordBody>,
        cwd: Option<String>,
        end: Option<u64>,
    },
    /// 进 [`LineIntake::announced`]。
    Announce {
        sid: String,
        path: Option<String>,
        lines: Option<u64>,
    },
    /// 冲掉残批，再进 [`LineIntake::removed`]。
    Remove { sid: String },
    /// 〔MIG-3b · ㉓②〕本机某个会话的任务清单变了 ⇒ 交重放缓冲那张订阅表（与远端同一个 `tasks_changed`）。
    Tasks { sid: String },
    /// 〔FW1〕进 [`LineIntake::notice`]（冲掉残批、交一格出声）。
    Notice {
        sid: String,
        path: String,
        change: FileChange,
    },
    /// 这一件不进内容流（被藏起来的 bg 会话的行 / 宣告，或不是内容帧）。
    Skip,
    /// 冲掉残批、换一个新的 [`LineIntake`]（下一条流从头来）。
    StreamEnded,
    /// 〔RENDER2〕进 [`LineIntake::lost`]（冲掉残批、原位给一格 `Gap`）。
    Lost,
}

/// 〔CF1〕`bg` 会话要不要藏：`kind` 在且不是 `interactive` 才算非交互；旧 CC 不写 kind ⇒ 当交互。
fn local_hides(kind: Option<&str>, show_bg: bool) -> bool {
    !show_bg && kind.is_some_and(|k| k != "interactive")
}

/// 〔MIG-1 · `99 §2.1 ⑬`〕本机那条流上的一件东西 ⇒ 交 `session_book` 的成品（**纯**；藏起来的 bg 会话不进）。
///
/// 与远端 [`stream_loop`] 那几条臂交同一种成品（本机 ＝ 不走 ssh 的远端，`INVARIANTS §40`）；与 [`local_step`] 读同一件东西、
/// 同一个「藏不藏」口径（[`local_hides`]），但**先于**它跑（它会改 `hidden`）。流断 ⇒ 这台的成品作废（说不清）。
pub(crate) fn local_product(
    item: &LocalItem,
    show_bg: bool,
    hidden: &std::collections::HashSet<String>,
) -> Option<BookIn> {
    let origin = || crate::origin::LOCAL.to_string();
    match item {
        LocalItem::StreamEnded => Some(BookIn::LinkLost { origin: origin() }),
        LocalItem::LineLost => None,
        LocalItem::Frame(InboundFrame::SessionAdded {
            sid,
            session_kind,
            attachable,
            cwd,
            name,
            status,
            waiting_for,
            container,
            pid,
            rbind_token,
            ..
        }) => (!local_hides(session_kind.as_deref(), show_bg)).then(|| BookIn::Live {
            origin: origin(),
            sid: sid.clone(),
            meta: LiveMeta {
                kind: session_kind.clone(),
                attachable: *attachable,
                cwd: cwd.clone(),
                name: name.clone(),
                status: status.clone(),
                waiting_for: waiting_for.clone(),
                container: *container,
                pid: *pid,
                rbind_token: rbind_token.clone(),
            },
        }),
        LocalItem::Frame(InboundFrame::SessionStatus {
            sid,
            status,
            waiting_for,
        }) => (!hidden.contains(sid)).then(|| BookIn::Status {
            origin: origin(),
            sid: sid.clone(),
            status: status.clone(),
            waiting_for: waiting_for.clone(),
        }),
        LocalItem::Frame(InboundFrame::SessionState { sid, state }) => (!hidden.contains(sid))
            .then(|| BookIn::Left {
                origin: origin(),
                sid: sid.clone(),
                fate: *state,
            }),
        LocalItem::Frame(InboundFrame::SessionsReplayed) => {
            Some(BookIn::Listed { origin: origin() })
        }
        LocalItem::Frame(_) => None,
    }
}

/// 〔LOC1b · 第四波 4D〕本机宣告的会话 `kind` 既不是 `interactive` 也不是 `bg` ⇒ 记一笔漂移账（记在本机名下）。
///
/// 这一笔从前住 `session_map·rs::is_interactive`〔散文墓碑〕（monitor 自己扫 pidfile 时顺手记）；本机判活改由本机后端的帧来之后，
/// 本机那条流是唯一看得见 `kind` 的地方。**排他 ≠ 无声**（U-CC1）：只记账，不改行为。
fn book_unknown_local_kind(item: &LocalItem) {
    if let LocalItem::Frame(InboundFrame::SessionAdded {
        session_kind: Some(k),
        ..
    }) = item
    {
        if k != "interactive" && k != "bg" {
            crate::drift_ledger::record(
                &crate::origin::Origin::local(),
                crate::drift_ledger::DriftFace::UnknownSessionKind,
                k,
                None,
            );
        }
    }
}

/// 〔CF1〕本机消费者的**纯分派核**：一件东西 × 「显示 bg 吗」× 「藏起来的 sid」⇒ 怎么处置。
///
/// 为什么 bg 在这里藏而不在后端那边按旗标分：本机常驻后端**跨 monitor 存活**，`adopt` 只比
/// `build_id` 与家目录、不比起参 ⇒ 起参里的 `--with-bg` 挡不住「用户关了 bg 显示、却接上了一个按开着起的后端」。
/// 于是两条载体一律带 `--with-bg`，显示与否在这一侧按 `session_added.session_kind` 定（协议序保证宣告先于行）。
pub(crate) fn local_step(
    item: LocalItem,
    show_bg: bool,
    hidden: &mut std::collections::HashSet<String>,
) -> LocalStep {
    match item {
        LocalItem::StreamEnded => {
            hidden.clear();
            LocalStep::StreamEnded
        }
        LocalItem::LineLost => LocalStep::Lost,
        LocalItem::Frame(InboundFrame::Line {
            session_id,
            path,
            seq,
            message,
            cwd,
            end,
        }) => {
            if hidden.contains(&session_id) {
                LocalStep::Skip
            } else {
                LocalStep::Line {
                    session_id,
                    path,
                    seq,
                    message,
                    cwd,
                    end,
                }
            }
        }
        LocalItem::Frame(InboundFrame::SessionAdded {
            sid,
            session_kind,
            path,
            lines,
            ..
        }) => {
            if local_hides(session_kind.as_deref(), show_bg) {
                hidden.insert(sid);
                LocalStep::Skip
            } else {
                // 同一个 sid 原地翻回交互（极少见）⇒ 不再藏。
                hidden.remove(&sid);
                LocalStep::Announce { sid, path, lines }
            }
        }
        // 〔MIG-1〕藏着的 sid 留到它的去向（`session_state`）那一帧才摘：去向也要照「藏」那一条滤掉。
        LocalItem::Frame(InboundFrame::SessionRemoved { sid }) => LocalStep::Remove { sid },
        LocalItem::Frame(InboundFrame::SessionState { sid, .. }) => {
            hidden.remove(&sid);
            LocalStep::Skip
        }
        // 〔FW1〕藏起来的 bg 会话照旧不出声（它的行也不进内容流）。
        LocalItem::Frame(InboundFrame::SessionFileNotice { sid, path, change }) => {
            if hidden.contains(&sid) {
                LocalStep::Skip
            } else {
                LocalStep::Notice { sid, path, change }
            }
        }
        // 〔MIG-3b · ㉓②〕任务清单变了：与 bg 藏不藏无关（任务面板按 sid 取，藏起来的会话本来就没有 tab）。
        LocalItem::Frame(InboundFrame::TasksChanged { sid }) => LocalStep::Tasks { sid },
        LocalItem::Frame(_) => LocalStep::Skip,
    }
}

/// 〔CF1〕本机常驻后端两种载体的起参里**恒有** `--tail-only` ⇒ 本机那条流的历史一律走旁路快照。
/// 两份起参与本常量的一致性由判据对拍（`local_lines_tests`），不靠这句注释。
pub(crate) const LOCAL_STREAM_TAIL_ONLY: bool = true;

/// 〔CF1 · 2026-09-24〕**本机会话内容的消费者**：吃 `local_lines` 通道，交给与远端同一个 [`LineIntake`]。
///
/// 每条流一个 `LineIntake`：收到 [`LocalItem::StreamEnded`] ⇒ 冲掉残批、丢掉它（快照队列随之关）、
/// 下一条流换新的 —— 与远端「每条连接一套」同形。本机后端重连之后会重新宣告每个活会话，
/// 旁路快照按续点接着拉（`snapshot_resume`）。
///
/// 〔LOC1b · MIG-1〕**本机会话的起停也从这条流来**：后端出的成品经 [`local_product`] 交 `session_book`（与远端同一个口）；
/// 流断 ⇒ 本机的成品作废、当时活的 / 可重连的说不清（`session_book::In::LinkLost`），不归档。
/// ⚠ 本任务**绝不**等一个经本机通道的应答（快照那几问在分发器的任务里）：读循环可能正停在往本通道送东西上，
///   这里要是也等它 ⇒ 互等。
pub(crate) async fn consume_local(
    mut rx: tokio::sync::mpsc::Receiver<LocalItem>,
    replay: Arc<EventReplay>,
    app: tauri::AppHandle,
) {
    let show_bg = crate::load_show_bg_sessions();
    let label = crate::origin::LOCAL.to_string();
    let mut hidden: std::collections::HashSet<String> = std::collections::HashSet::new();
    loop {
        let mut intake = LineIntake::open(label.clone(), LOCAL_STREAM_TAIL_ONLY, &replay, &app);
        // 〔CF2〕这条流交来第一件东西 ⇒ 本机那台「看得见」（订阅原位收 `Seen`）；流结束 ⇒ `Unseen`。
        let mut seen = false;
        loop {
            let Some(item) = intake.recv_or_flush(&mut rx).await else {
                // 发送端全没了（进程收摊）：残批照发，然后退出。
                intake.flush().await;
                return;
            };
            if !seen && !matches!(item, LocalItem::StreamEnded) {
                seen = true;
                replay.origin_seen(&crate::origin::Origin(label.clone()), true);
            }
            // 〔MIG-1〕本机起停的成品：先交 `session_book`（它按 `hidden` 滤，而下面 `local_step` 会改 `hidden`）。
            //   流断 / 去向那两件先冲掉残批再交（与远端同序：行先落、再说「离开了 / 看不见了」）。
            book_unknown_local_kind(&item);
            if matches!(
                item,
                LocalItem::StreamEnded | LocalItem::Frame(InboundFrame::SessionState { .. })
            ) {
                intake.flush().await;
            }
            if let Some(ev) = local_product(&item, show_bg, &hidden) {
                crate::session_book::feed(ev);
            }
            match local_step(item, show_bg, &mut hidden) {
                LocalStep::Line {
                    session_id,
                    path,
                    seq,
                    message,
                    cwd,
                    end,
                } => {
                    intake
                        .line(JsonlLine {
                            session_id,
                            path: std::path::PathBuf::from(path),
                            seq,
                            message,
                            cwd,
                            end,
                        })
                        .await
                }
                LocalStep::Announce { sid, path, lines } => intake.announced(&sid, path, lines),
                LocalStep::Remove { sid } => {
                    intake.flush().await;
                    intake.removed(&sid);
                }
                LocalStep::Notice { sid, path, change } => intake.notice(&sid, &path, change).await,
                LocalStep::Tasks { sid } => {
                    replay.tasks_changed(&crate::origin::Origin(label.clone()), &sid)
                }
                LocalStep::Skip => {}
                LocalStep::Lost => intake.lost().await,
                LocalStep::StreamEnded => {
                    intake.flush().await;
                    replay.origin_seen(&crate::origin::Origin(label.clone()), false);
                    tracing::info!(
                        "本机那条流结束：内容收口换新（下一条流重新宣告、快照按续点接着拉）"
                    );
                    break;
                }
            }
        }
    }
}

/// [`run`] 的内层流循环：connect → exec backend → 逐帧 dispatch。**所有**提前返回
/// （`?` / EOF / 读错误）都把 result 冒泡给 [`run`]，由后者统一说「这台的成品作废」，故本函数自身不管断连。
async fn stream_loop(
    cfg: &RemoteConfig,
    replay: &Arc<EventReplay>,
    app: &tauri::AppHandle,
    connected: &Arc<AtomicBool>,
    hello_confirmed: &mut Option<Vec<String>>,
    unsupported: &mut Option<String>,
) -> Result<(), String> {
    // issue #15 / #30：远端行的 origin 标签 = 该机器的稳定身份（label，默认 host）。
    // 前端据此给该 Tab 标题加 `[label]` 前缀以区分本地/各远端机器。进 loop 前 clone。
    let host_label = cfg.origin_label();

    // ★〔audit-0805 F05 下半，报告 §4.3〕**冷启动最贵的三段此前一个 `[perf]` 都没有**。
    //
    // 实测（08-06）：全仓 `[perf]` 前端 13 处、monitor Rust 14 处，而 `ssh_source.rs` **0 处** ——
    // 偏偏这里是「用户点开应用到看见远端会话」之间**唯一**的那条链。
    // 后果不是「不知道快慢」，是**报告里那些 50-200ms 的数字是外部常识值、不是本仓证据**
    //（`ROADMAP §5-6` 那条诚实边界就挂在这上面）。没有埋点，「冷启动三连接合并省了多少」
    // 这句话永远只能靠推。
    //
    // ⚠ 埋点本身**不改任何行为**，也不该改：它只是让下一次讨论有数可依。
    let t_connect_start = std::time::Instant::now();

    // issue #29（F08）：连接前确保远端后端已（自动）部署到固定落点（〔E2〕`~/.cc-monitor/bin/ccm`）。
    // 嵌入二进制就位前（F08b 未做）〔DP1〕`byte_table::choose` 回「这一版没带」→ ensure_backend_deployed
    // 优雅 no-op。**best-effort**：部署失败仅 warn，不阻断——手动部署的后端仍可连。
    // ★ F05 下半：**上一次这台机器的后端自报过就是期望 build ⇒ 跳过预检那两条连接**。
    // 判据与记忆的语义见 `VERIFIED_BUILD` 头注（记的是 hello 自证，不是预检结论）。
    // 跳过时 `confirmed_build` 直接给 `EXPECTED` —— 若给 `None`，下面的 caps 阶梯会掉进
    // ③ 空集全降级，那就**比不跳还糟**（省两条连接换来一轮降级 + 一轮升级重连）。
    let verified = verified_build_of(&host_label);
    let skip_preflight = preflight_can_be_skipped(verified.as_deref(), EXPECTED_BACKEND_BUILD_ID);
    let confirmed_build = if skip_preflight {
        Some(EXPECTED_BACKEND_BUILD_ID.to_string())
    } else {
        match crate::sftp::ensure_backend_deployed(cfg).await {
            Ok(c) => Some(c),
            Err(e) => {
                // 〔DP1 · 第四波〕**不阻断**（手动部署的后端照样能连），但那句话要到界面上 ——
                //   从前这里只 `warn!`、拒绝那几形更是 `debug!` ＋ `Ok(None)`，用户看到的是「什么都没发生」（`设计/96 §7.1.4`）。
                let msg = e.say();
                tracing::warn!(
                    "ssh_source [{host_label}] 后端没部署上（继续尝试连接已有后端）: {msg}"
                );
                let payload = crate::ui_contract::RemoteHealthPayload {
                    origin: host_label.clone(),
                    kind: "deploy".to_string(),
                    message: msg,
                };
                if let Err(e) = app.emit(crate::ui_contract::events::REMOTE_HEALTH, payload) {
                    tracing::warn!("ssh_source remote-health (deploy) emit failed: {e}");
                }
                None
            }
        }
    };
    tracing::info!(
        "[perf] ssh_source [{host_label}] 部署预检 {}ms（ensure_backend_deployed；\
         confirmed_build={:?}；skip={skip_preflight}）",
        t_connect_start.elapsed().as_millis(),
        confirmed_build.as_deref()
    );

    // F66（#58③）流模式门控：**从后端声明的能力 token 决定发哪些 flag**，不再靠
    // build_id 精确匹配。旧后端会把未知参数当一次性查询处理后退出（无 hello → 重连
    // 死循环，§26），故只对**声明了对应能力**的后端发 flag（声明 = 自证会剥离该 flag）。
    // 能力两条来源，hello 自愈账本优先：
    //   ① `hello_confirmed`（上一轮 backend **自报**的能力）—— 最权威，收过真 hello 才有。
    //   ② 否则部署侧确认了当前内嵌 build（`confirmed_build == EXPECTED`）→ 用内嵌后端的
    //      能力常量**预知**，省第一轮「降级→收 hello→重连升级」往返（乐观路径）。
    //   ③ 都没有 → 空集 → 全降级（= 2.18.0 行为，连接正常、功能退化）。
    // **hello 优先**于部署侧（②可能是陈旧内嵌的身份 ≠ 期望 → 空集 → 靠 hello 自愈救，
    //  见 v2.22.1 无限重连教训；`hello_confirmed` 只在收到真声明时写入，优先采纳恒安全）。
    let caps: Vec<String> = hello_confirmed.clone().unwrap_or_else(|| {
        if confirmed_build.as_deref() == Some(EXPECTED_BACKEND_BUILD_ID) {
            embedded_backend_capabilities()
        } else {
            Vec::new()
        }
    });
    let (with_bg, tail_only, with_rbind_token) =
        decide_stream_flags(&caps, crate::load_show_bg_sessions());
    let t_exec = std::time::Instant::now();
    // ★ F05 下半：起流失败就抹掉自证记忆 —— 否则一台后端被删/被换旧的机器会
    // **每一轮都跳预检、每一轮都失败**，永远等不到重新部署。代价是多一次重连，
    // 那正是 `VERIFIED_BUILD` 头注里如实写下的那个退化。
    // 〔HOST · V139 · DEL〕接那台的**常驻后端**（没有就起一个；与本机同形）。这是远端唯一的一形：
    //   起不了常驻（非 unix / 太旧）就是一次失败、说清为什么，不回落到随 SSH 生死的流模式。
    let flags = (with_bg, tail_only, with_rbind_token);
    let stream: crate::remote_resident::Replayed = match crate::remote_resident::attach(cfg, flags)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            // 〔DEL 续〕非 unix ⇒ 记进这台的连接状态（`run` 据此停下，不再按退避重连）。
            if let crate::remote_resident::AttachErr::Unsupported(why) = &e {
                *unsupported = Some(why.clone());
            }
            let e = e.said();
            if skip_preflight {
                tracing::warn!(
                    "ssh_source [{host_label}] 跳过预检后起流失败，抹掉自证记忆，下一轮重新预检: {e}"
                );
                forget_verified_build(&host_label);
            }
            return Err(e);
        }
    };
    tracing::info!(
        "[perf] ssh_source [{host_label}] 起流 {}ms（SSH 登录 + exec backend；\
         with_bg={with_bg} tail_only={tail_only} with_rbind_token={with_rbind_token}）\
         · 自本轮连接开始 T+{}ms",
        t_exec.elapsed().as_millis(),
        t_connect_start.elapsed().as_millis()
    );

    // U8a-2a：这条 channel 是**双工**的，此前只用了读半边。`split_and_park` 一步切开并
    // 把写半边停住 —— `ParkedWriter` 身上没有任何写方法，要等收到 hello 才换得出能发命令的
    // 客户端。切与停必须是同一步：中间留一个裸 `WriteHalf` 就等于留了一个「Hello 之前能写」
    // 的窗口（D 审计实测过那个窗口，两条护栏都拦不住）。见 `inbound_client` 头注。
    // 〔THIN〕接上那一刻本机常驻后端答的「那台比手上这一版旧」—— 版本提示那句话按它挑。
    let remote_older = stream.remote_is_older();
    let (stream, parked) = crate::inbound_client::split_and_park(stream);
    let mut parked = Some(parked);
    // 本连接的入方向客户端（收到 hello 后才有）。函数任何退出路径经 guard 摘除注册表
    // 并叫醒还在等应答的调用方 —— 同 `SnapshotQueueCloser` 的形状。
    let mut inbound: Option<std::sync::Arc<crate::inbound_client::InboundClient>> = None;
    struct InboundCloser(
        String,
        Option<std::sync::Arc<crate::inbound_client::InboundClient>>,
    );
    impl Drop for InboundCloser {
        fn drop(&mut self) {
            if let Some(c) = self.1.take() {
                crate::inbound_client::unregister(&self.0, &c);
            }
        }
    }
    let mut inbound_guard = InboundCloser(host_label.clone(), None);

    // Batch8-F26：旁路快照基础设施（仅 tail-only 生效；每连接一套，函数任何
    // 退出路径随 `intake` 被丢掉而关闭队列——已入队项仍会被分发器拉完，独立连接自灭）。
    // 〔CF1〕攒批 ＋ 静默窗 ＋ 旁路快照收成 [`LineIntake`]，本机那条流用的是同一个。
    let mut intake = LineIntake::open(host_label.clone(), tail_only, replay, app);
    // 〔W5-VIS · `设计/15 §3.4 ②`〕这条流上跳过了几帧认不出的（读任务那边另有一本记非 UTF-8 行）。
    let mut tally = crate::frame_tally::FrameTally::new(format!("ssh_source {host_label}"));

    // Batch5-F17：帧读取挪进独立 task、经 channel 交回——攒批需要"带静默窗口
    // 的读"，而 tokio 的 read_line **不是 cancellation-safe**（timeout 取消会
    // 丢 buffer 里的半帧）；mpsc::Receiver::recv 是 cancel-safe 的，超时打在
    // recv 上帧零丢失。reader task 在 EOF/读错时投递 Err 后退出；本函数返回
    // （重连）时 rx drop → task 的 send 失败 → task 自然退出，不泄漏。
    // `Ok(None)`：〔RENDER2 · `99 §2.1` ㉓①〕这里有一行超长、整行丢了（说不出是哪个会话的哪一行）⇒ 主循环原位给订阅一格 `Gap`（`to_seq` 缺）。
    let (frame_tx, mut frame_rx) =
        tokio::sync::mpsc::channel::<Result<Option<String>, String>>(1024);
    let reader_host = host_label.clone();
    tauri::async_runtime::spawn(async move {
        let mut reader = BufReader::new(stream);
        // 按 `\n` 切（协议保证每帧一行、帧内换行已被后端转义成 `\n` 两字符，
        // 见 src/backend/stream/wire.rs）。
        // ★ F10b：从无界 `read_line` 换成 [`read_capped_line`] —— 无界读遇「一条永远不结束
        // 的行」就是无界堆分配，而对端是**远端进程**（它坏掉或不是我们的后端都可能）。
        let mut buf: Vec<u8> = Vec::new();
        // 〔W5-VIS · `设计/15 §3.4 ②`〕这条流上有几行不是合法 UTF-8（按替换字符读的）—— 计数、按 2 的幂次说、流结束出总账。
        let mut tally = crate::frame_tally::FrameTally::new(format!("ssh_source {reader_host}"));
        loop {
            match read_capped_line(&mut reader, &mut buf, BACKEND_FRAME_LINE_CAP).await {
                Ok(CappedLine::Eof) => {
                    // EOF：backend 退出 / channel 关闭。明确报错，不静默冻结。
                    let _ = frame_tx
                        .send(Err(
                            "ssh backend stdout closed (EOF / connection dropped)".to_string()
                        ))
                        .await;
                    break;
                }
                Ok(CappedLine::TooLong(bytes)) => {
                    // 超限语义 = **丢弃 + 原位说出来**，绝不静默（定框 E4）。
                    // 〔RENDER2 · `99 §2.1` ㉓①〕不走 Err 臂（那会被当成致命错误去重连，而坏的只是这一行），
                    //   也不再走旁路健康提示：与行同一条路交 `Ok(None)`，主循环原位给订阅一格 `Gap`（本机两条载体同形）。
                    tracing::warn!(
                        "ssh_source remote [{reader_host}] line too long: {bytes} bytes \
                         (cap {BACKEND_FRAME_LINE_CAP}); line dropped"
                    );
                    if frame_tx.send(Ok(None)).await.is_err() {
                        break;
                    }
                }
                Ok(CappedLine::Line) => {
                    // 非 UTF-8 不该让整条连接死掉（与全批 exec 输出读取同一取舍）—— 但**记账、说出来**（W5-VIS）。
                    if std::str::from_utf8(&buf).is_err() {
                        if let Some(n) = tally.note_bad_utf8(&buf) {
                            tracing::warn!("{n}");
                        }
                    }
                    let text = String::from_utf8_lossy(&buf);
                    let line = text.trim_end_matches(['\n', '\r']);
                    if line.is_empty() {
                        continue;
                    }
                    if frame_tx.send(Ok(Some(line.to_string()))).await.is_err() {
                        break; // 主循环已退出（重连中）
                    }
                }
                Err(e) => {
                    let _ = frame_tx
                        .send(Err(format!("ssh backend stdout read error: {e}")))
                        .await;
                    break;
                }
            }
        }
    });

    loop {
        // pending 非空 → 带静默窗口收帧：窗口内没有新帧就先 flush 再回到阻塞收（`LineIntake::recv_or_flush`）。
        let Some(msg) = intake.recv_or_flush(&mut frame_rx).await else {
            // reader task 没投 Err 就消失（理论不可达）——同样明确报错走重连。
            intake.flush().await;
            return Err("ssh backend frame channel closed".to_string());
        };
        let line = match msg {
            Ok(Some(l)) => l,
            Ok(None) => {
                intake.lost().await;
                continue;
            }
            Err(e) => {
                // EOF/读错：flush 残余（at-least-once 安全；重连会从 seq 0 重放，
                // 但没有理由主动丢已收到的行）**并等它发完**再报错——run() 随后的
                // 断连归档（announced 清算）必须晚于这些行到达前端（审计 R1）。
                intake.flush().await;
                return Err(e);
            }
        };
        let line = line.as_str();

        let frame = parse_frame(line);
        // SessionRemoved 是唯一顺序敏感的攒批边界：它的行必须先落前端，否则
        // 归档后迟到的行把 Tab 复活成僵尸 live（审计 R1/R2）。SessionAdded /
        // Hello / Overflow / 坏帧**不再**作边界——多小会话的 snapshot 才能聚
        // 成大批跨过阈值（行先于 Added 到达无妨：前端 ensureTab 见行即建）。
        // 〔MIG-1〕`session_state`（可重连 / 已结束的成品）同理：它说的「离开了」必须排在这个会话的行之后。
        if matches!(
            frame,
            Some(InboundFrame::SessionRemoved { .. } | InboundFrame::SessionState { .. })
        ) {
            intake.flush().await;
        }

        // U8a-2a：**握手完成 ⇒ 写半边解冻。** 放在 match 之前是因为 Hello 那条臂按值解构了帧。
        if let Some(client) = attach_inbound_client(&host_label, &mut parked, frame.as_ref()) {
            inbound_guard.1 = Some(client.clone());
            inbound = Some(client);
            // 〔`C1` · 09-24〕「这台的长连接能问话了」—— 前端的账号刷新在这一刻强制拉一次。
            // 〔DL1〕原先这里发一个裸 Tauri 事件（`remote-backend-ready`）；今天由下面 Hello 臂里既有的
            //   `replay.origin_seen(.., true)` 说（订了这台 `accounts-changed` 的订阅原位收 `Seen`，`event_replay` 头注那张表）——
            //   同一个时刻、同一个事实，只留一个家。
            // 〔AS2 · V113〕连上那一刻：让本机常驻后端沿池里那条 SSH 同步资产目录（后台跑，零判定）。
            let accepts = inbound
                .as_ref()
                .is_some_and(|c| c.accepts(crate::asset_sync::REMOTE_NEEDS));
            crate::asset_sync::on_remote_ready(cfg, accepts);
            // 〔GP1 · 第四波〕升级那一格：连上那一刻后台看一眼旧版 `~/.local/bin/ccm`，认出是我们放的就删（`ccm_legacy`）。
            crate::ccm_legacy::on_remote_ready(cfg);
        }

        match frame {
            Some(InboundFrame::Hello {
                v,
                build_id,
                host_arch,
                claude_dir,
                homes,
                capabilities,
                commands,
                ..
            }) => {
                // `S4`：日志报的是**解析后**的 Claude home（优先 `homes`、回退 `claude_dir`），
                // 同时把原样的 `homes` 一起打出来 —— 排障时要能一眼看出
                // 「这台后端到底发没发新字段」，那正是 additive 迁移期最常问的问题。
                let claude_home = claude_home_from_hello(&homes, &claude_dir);
                tracing::info!(
                    "ssh_source backend hello: v={v} build_id={build_id} host_arch={host_arch} claude_home={claude_home} homes={homes:?} caps={capabilities:?} cmds={commands:?}"
                );
                // U-CC1：记下**我们不认识的**能力 token。多半是远端后端比 monitor 新
                // （自动部署会把它拉回同一个 build，但手工装 / 关了自动部署的用户会长期不一致）。
                // 只记账，行为一字不改：不认识的 token 本来就按保守缺省忽略。
                // 〔ST3〕记在这台名下。
                note_unknown_capabilities(
                    &crate::origin::Origin(host_label.clone()),
                    &capabilities,
                    &build_id,
                );
                // 标记本次连接已健康(收到 backend hello)，供 run() 重连循环判定是否重置退避。
                connected.store(true, Ordering::Release);
                // 〔CF2〕订了这台会话流的那些订阅原位收一格 `Seen`（`05 §3.3.4` 的 `Item::Seen`）。
                replay.origin_seen(&crate::origin::Origin(host_label.clone()), true);
                // issue #33：版本协商。不兼容/偏旧经 SS-F remote-health 通道醒目提示（前端
                // headlineFor 已含 version case，零前端改动）。不 hard-disconnect（向前兼容）。
                if let Some(msg) = version_warning(v, &build_id, &host_label, remote_older) {
                    tracing::warn!("ssh_source remote [{host_label}] version: {msg}");
                    let payload = crate::ui_contract::RemoteHealthPayload {
                        origin: host_label.clone(),
                        kind: "version".to_string(),
                        message: msg,
                    };
                    if let Err(e) = app.emit(crate::ui_contract::events::REMOTE_HEALTH, payload) {
                        tracing::warn!("ssh_source remote-health (version) emit failed: {e}");
                    }
                }
                // F66（#58③）：本轮若跑在降级模式（未开 tail_only）——用 backend **自报的
                // 能力**判断能否升级，不再靠 build_id 精确匹配（闭合 2026-07-09 事故）：
                // ① backend 声明了**能开本轮没开的 flag** 的能力 → 记 hello 自愈账（存能力集）,
                //    立即重连升级（connected 已置 true → 退避重置 MIN,~2s 内带 flag 回来）。
                //    **防无限循环**：仅当「下一轮据此算出的 flag 严格优于本轮」才重连——flag 数
                //    有限（2）、每次升级严格增开，最多 2 轮收敛。
                // ② backend 无任何能力声明（真旧后端）→ 降级可见化（否则用户看到「bg 会话
                //    消失+拥塞复发」却无从归因，实测连环误诊）——经 remote-health 提示。
                tracing::info!(
                    "[perf] ssh_source [{host_label}] 首个 hello T+{}ms（自本轮连接开始）· \
                     caps={capabilities:?}",
                    t_connect_start.elapsed().as_millis()
                );
                // ★ F05 下半：**自证记忆的唯一写入点**。backend 自己说它是谁，我们才记。
                // build_id 不是期望值 ⇒ **抹掉**（这台机器上装的不是当前 build，
                // 下一轮必须照跑预检去部署），不是「留着上次的」。
                if build_id == EXPECTED_BACKEND_BUILD_ID {
                    record_verified_build(&host_label, &build_id);
                } else {
                    forget_verified_build(&host_label);
                }
                // 🔴 〔`设计/80 §8.7` 步 3〕**升级判定从 `if !tail_only` 里搬出来了。**
                // 那道外层 guard 的语义是「本轮若跑在降级模式」，在**两位**的世界里
                // 它等价于 `!tail_only`；三位之后**当场为假** —— `tail_only` 已开、
                // 而 `rbind-token` 这一位本轮没开，是一个真实可达的状态
                // （`hello_confirmed` 记的是上一版只声明了 `tail-only` 的能力集）。
                // 那时 guard 会把升级整个跳过 ⇒ `--with-rbind-token` 永远发不出去，
                // 而症状只是「令牌字段恒缺席」= 一个**合法值** ⇒ 极安静。
                // 收敛性不依赖这道 guard（见 `should_upgrade_reconnect` 头注：三项都
                // 写全 `&& !cur_*`，记账后 `next==cur` ⇒ 恒 false，最多 3 轮）。
                let show_bg = crate::load_show_bg_sessions();
                let next = decide_stream_flags(&capabilities, show_bg);
                if should_upgrade_reconnect((with_bg, tail_only, with_rbind_token), next) {
                    *hello_confirmed = Some(capabilities.clone());
                    return Err(copy_text("rsSshSource.upgrade.reconnect", &[]));
                }
                // ⚠ 「旧后端降级可见化」那一格**仍然**留在 `!tail_only` 里 —— 它问的是
                //   另一件事（「这台后端一条能力都没声明」），口径一个字没动。
                if !tail_only {
                    if capabilities.is_empty() {
                        let payload = crate::ui_contract::RemoteHealthPayload {
                            origin: host_label.clone(),
                            kind: "degraded".to_string(),
                            message: copy_text(
                                "rsSshSource.health.degraded",
                                &[
                                    ("build", &build_id.to_string()),
                                    ("expected", &EXPECTED_BACKEND_BUILD_ID.to_string()),
                                ],
                            ),
                        };
                        if let Err(e) = app.emit(crate::ui_contract::events::REMOTE_HEALTH, payload)
                        {
                            tracing::warn!("ssh_source remote-health (degraded) emit failed: {e}");
                        }
                    }
                }
            }
            Some(InboundFrame::Line {
                session_id,
                path,
                seq,
                message,
                cwd,
                end,
            }) => {
                // Batch5-F17：进攒批缓冲（达 cap/批龄立即整批出）；静默窗口/
                // SessionRemoved 边界触发的 flush 在循环头。
                intake
                    .line(JsonlLine {
                        session_id,
                        path: std::path::PathBuf::from(path),
                        seq,
                        message,
                        cwd,
                        end,
                    })
                    .await;
            }
            Some(InboundFrame::SessionAdded {
                sid,
                session_kind,
                attachable,
                cwd,
                name,
                path,
                lines,
                status,
                waiting_for,
                rbind_token,
                container,
                // 〔LOC1b〕pid 只给本机那条流用（本机 ↗ 绑窗口）；远端这一支不读。
                pid: _,
            }) => {
                // 🔴 〔`设计/80 §8.7` 步 4，第二波 T4〕**记进令牌账本 —— ↗ 从此按它分派**（monitor 自己的事：拉前终端）。
                // `None` 也要记（= 删掉旧值）：重新宣告成「没令牌」时，上一次的令牌不许粘着。
                // ⚠ **只打布尔，不打值**（`§8.6 ③`：令牌是敏感数据；`ssh_source_parse_frame_tests.rs::the_token_value_never_reaches_a_log_macro` 钉）。
                crate::bind::remote_rbind_tokens().note(&sid, rbind_token.as_deref());
                tracing::debug!(
                    "ssh_source [{host_label}] session_added sid={sid} has_rbind_token={}",
                    rbind_token.is_some()
                );
                // ★★ 〔`P0b` 08-13〕这一跳要看得见（「帧到 monitor 了吗」）；每个会话一次，不淹日志。
                tracing::info!("session-added: [{host_label}] sid={sid} → 成品交出口");
                // 〔MIG-1〕活会话的成品（元信息 ＋ 初始灯 ＋ 容器）原样交 `session_book`（本机那条流同一个口）。
                crate::session_book::feed(BookIn::Live {
                    origin: host_label.clone(),
                    sid: sid.clone(),
                    meta: LiveMeta {
                        kind: session_kind,
                        attachable,
                        cwd,
                        name,
                        status,
                        waiting_for,
                        container,
                        pid: None,
                        rbind_token,
                    },
                });
                // Batch8-F26：tail-only 下历史改走旁路快照——宣告带 path 即入队
                // （无 path = 会话刚起还没写 jsonl → 无历史可拉，后续行天然从
                // tail 全量到达，无需快照）。队列按 sid 幂等（重复宣告不重拉）。
                intake.announced(&sid, path, lines);
            }
            Some(InboundFrame::SessionStatus {
                sid,
                status,
                waiting_for,
            }) => {
                // ★★〔08-14 实机排障补〕红绿灯这一跳也要看得见：「全绿」既可能是都在忙，也可能是 status 一条都没到。
                tracing::info!(
                    "session-status: [{host_label}] sid={sid} status={status:?} \
                     waiting_for={waiting_for:?} → 成品交出口"
                );
                crate::session_book::feed(BookIn::Status {
                    origin: host_label.clone(),
                    sid,
                    status,
                    waiting_for,
                });
            }
            Some(InboundFrame::SessionRemoved { sid }) => {
                // 〔MIG-1〕只剩内容流的边界：残批已在循环头冲掉；这里摘排队中的快照 ＋ 给在途的打取消标记
                //   （Batch8 D-B1：归档后迟到的快照行会经「见行复活」造出僵尸 tab）、续点作废。它离开之后是什么由下一帧 `session_state` 说。
                tracing::info!(
                    "session-removed: [{host_label}] sid={sid}（内容流收口；去向看 session_state）"
                );
                intake.removed(&sid);
            }
            // 〔MIG-1 · `99 §2.1 ⑬`〕后端裁好的去向（可重连 / 已结束）原样交出口（残批已在循环头冲掉）。
            Some(InboundFrame::SessionState { sid, state }) => {
                tracing::info!("session-state: [{host_label}] sid={sid} → {state:?}");
                crate::session_book::feed(BookIn::Left {
                    origin: host_label.clone(),
                    sid,
                    fate: state,
                });
            }
            Some(InboundFrame::Overflow {
                dropped,
                lost,
                lost_truncated,
            }) => {
                // issue #32：远端管道拥塞丢了 dropped 帧。warn + 经 SS-F remote-health
                // 通道提示用户（前端按 origin 节流弹 toast）。
                //
                // ★〔audit-0805 F21〕**这里此前对用户说了一句假话**：「重开该会话可看完整
                // 历史」只对**内容帧**成立。状态增量帧（session_added/session_removed/
                // tmux_session_closed/session_status）是一次差分的结果、**别处不存在**，
                // 重开会话补不回来 —— 那正是 B-3 的正题。backend 从 `p1x` 起会把这些帧的
                // 身份放进 `Overflow.lost`；有身份就说实话，并点名是哪几个会话。
                tracing::warn!(
                    "ssh_source remote [{host_label}] overflow: backend dropped {dropped} frame(s), \
                     {} unrecoverable{}",
                    lost.len(),
                    if lost_truncated { " (list truncated)" } else { "" }
                );
                let message = overflow_health_message(&host_label, dropped, &lost, lost_truncated);
                let payload = crate::ui_contract::RemoteHealthPayload {
                    origin: host_label.clone(),
                    kind: "overflow".to_string(),
                    message,
                };
                if let Err(e) = app.emit(crate::ui_contract::events::REMOTE_HEALTH, payload) {
                    tracing::warn!("ssh_source remote-health emit failed: {e}");
                }
            }
            // U8a-2a：入方向应答 —— 交给本连接的客户端按 `id` 路由回请求方。
            Some(f @ (InboundFrame::Reply { .. } | InboundFrame::Cancelled { .. })) => {
                route_inbound_frame(&host_label, inbound.as_ref(), f);
            }
            // 〔SR1a · `设计/05 §13.6 ③`〕那台的账号清单变了 ⇒ 告诉前端（账号表与 chip 据此重取）。
            // 〔DL1〕经通道 `subscribe`：订了这台 `accounts-changed` 的订阅收一格 `Frame`（原先是一个裸 Tauri 事件）。
            Some(InboundFrame::AccountsChanged) => {
                replay.accounts_changed(&crate::origin::Origin(host_label.clone()));
            }
            // 〔MIG-3b · `99 §2.1 ㉓②`〕某个会话的任务清单变了 ⇒ 订了这台 `session-tasks` 的订阅收一格 `{sid}`。
            Some(InboundFrame::TasksChanged { sid }) => {
                replay.tasks_changed(&crate::origin::Origin(host_label.clone()), &sid);
            }
            // 〔FW1 · 第四波 4D · D-d〕那台一条活会话的记录文件不见了 / 被改过已从头重读 ⇒ 残批先冲、再交那个会话的内容流一格。
            Some(InboundFrame::SessionFileNotice { sid, path, change }) => {
                intake.notice(&sid, &path, change).await;
            }
            // 〔U4b · 第四波〕那台的活会话清单报完了（〔MIG-1〕后端把它压到第一份 tmux 快照之后、那一份推出的可重连之后才放）。
            //   与上面的宣告同一条流、同序交出口 ⇒ 前端收到它时，这台全部的活会话与可重连会话都已报过（`设计/30 §3.5.7a`）。
            Some(InboundFrame::SessionsReplayed) => {
                tracing::info!("sessions-replayed: [{host_label}] 活会话清单报完了 → 成品交出口");
                crate::session_book::feed(BookIn::Listed {
                    origin: host_label.clone(),
                });
            }
            // 〔SR1a〕链路帧只该出现在**本机后端**那条流上（monitor 只在那里开链路）。
            // 远端后端发来 ⇒ 协议对不上，照实说、丢掉。
            Some(InboundFrame::LinkData { link, .. } | InboundFrame::LinkEnd { link, .. }) => {
                tracing::warn!(
                    "ssh_source [{host_label}] 远端后端发来了链路帧（link={link}）—— monitor 没在远端开过链路，丢掉"
                );
            }
            // 〔SR1b〕传输帧同理：传输台住**本机**后端，远端后端发来 ⇒ 协议对不上，照实说、丢掉。
            Some(InboundFrame::Transfer { id, .. }) => {
                tracing::warn!(
                    "ssh_source [{host_label}] 远端后端发来了传输帧（id={id}）—— 传输台在本机后端，丢掉"
                );
            }
            // 〔MIG-1 收尾〕测试连接的进度帧同理：测试连接在**本机**后端里跑，远端后端发来 ⇒ 协议对不上，照实说、丢掉。
            Some(InboundFrame::Probe { ticket, .. }) => {
                tracing::warn!(
                    "ssh_source [{host_label}] 远端后端发来了测试连接的进度帧（ticket={ticket}）—— 测试连接在本机后端，丢掉"
                );
            }
            // 〔HOST · V139〕远端中转住进远端常驻后端（进程内），它抄出来的 SSE 事件沿这条流回来 ⇒ 与本机那条流同一个口转前端
            //   （origin = 这台；V141 之后标签就是 claude 自己的 sid，不用对账）。从不阻塞、不进内容通道。
            Some(InboundFrame::Tap(t)) => crate::session_tap::deliver(&host_label, t),
            None => {
                // 未知 kind / 坏帧 / 非 JSON：跳过，绝不 panic、绝不中断流。
                // 〔W5-VIS · `设计/15 §3.4 ②`〕记账：按 2 的幂次说（带累计数），流结束出总账（原先逐帧一行、从不计数）。
                if let Some(n) = tally.note_unparsed(line) {
                    tracing::warn!("{n}");
                }
            }
        }
    }
}

// ============================================================================
// 🔴 `K-R59`（09-11，定框 `K35`）：**`daemonless` 降级读取整段删除。**
//
// 这里原来住着 Batch14-F59 的那一整段：`BACKENDLESS_POLL_INTERVAL` 2s 轮询 · `find` 发现
// 最近 30 分钟活跃的 jsonl · `tail -c +offset` 增量读 · `DlCursor` · `emit_degraded` /
// `announce_daemonless` / `archive_daemonless` / `daemonless_stream_loop`，
// 由 `run()` 顶层 `if cfg.daemonless` 二选一。
//
// `K35` 逐字：「**不要有 daemonless。没有没有后端的情况。前端应该就是去调用远程后端的。**」
// ⇒ 它不是「一条传输」，是**一个模式**：删的是开关 · 顶层分支 · 这一段轮询 · 界面那一格 ·
//   以及 `settings/readiness.ts` 里那条**不含这个词**的本机豁免。**五处一起走。**
//
// ⚠ 一起下岗的账（别只删代码不拧账本，那会让下一个人以为它们还在跑）：
//   · `rust_timer_registry` 的 ticker 从 **3 → 2**（那 2s 是本仓抓到的第二个真节拍器）；
//   · `byte_cap_registry` 少两条（`BACKENDLESS_READ_CAP` / `BACKENDLESS_DISCOVER_CAP`）；
//   · `dial_move_judge::DIAL_SITES` 里 `ssh_source.rs` 那格从 **5 → 4**、总数 **7 → 6**；
//   · `local_read_surface_registry` 里 `ssh_source.rs` 那条从 **10 → 9**（远端目录串少一行；
//     ⚠ 这个 9 是**跑出来的**，不是估的 —— 那条判据自己会红在「多一处/少一处」上）。
//
// ⚠ **没有一起走的**（`§0a-2`：删的是模式，不是传输）：russh 数据源 · `ssh -G` 导入 ·
//   测试连接 · `connect_and_exec_cmd` / `shell_quote` 的现有消费者，一个不少。
// ============================================================================

// 〔MIG-1〕`f032_idle_tests`〔散文墓碑〕（idle 账 · `classify_removed` · 断连 flush · 重连后重新裁的判据）随那本账搬进后端删了。

// ============================================================================
// Tier 1 SSH 连接 UX（issue #15）：测试连接 + 指纹固化。
// 下列 #[tauri::command] 在 lib.rs 的 invoke_handler! 里注册（漏注册=运行时
// "command not found"，非编译错，已 double-check）。
// ============================================================================

// 〔MIG-1 · `99 §2.1 ⑯`〕`~/.ssh/config` 导入那三条（`list_ssh_host_aliases`〔散文墓碑〕· `resolve_ssh_host`〔散文墓碑〕·
//   `import_ssh_hosts`〔散文墓碑〕）搬进了后端 `dial/ssh_config.rs`（帧命令 `ssh-config-*`，界面经 `chan.call(<local>, …)` 问）。

// 〔MIG-1 续 · `99 §2.1 ⑬` · 主会话裁「后端持有全部 SSH」〕这里原来住着测试连接整族：结局类型 `ConnTestResult`（ts-rs 生成物）·
//   Tauri 命令 `test_remote_connection`〔散文墓碑〕· 读首行的 `probe_backend`〔散文墓碑〕· hello 人读摘要 `describe_hello`〔散文墓碑〕·
//   控制通道往返 `probe_control_channel`〔散文墓碑〕与它的应答泵 `pump_inbound_replies`〔散文墓碑〕。界面把表单那一台交给本机后端
//   （帧命令 `remote-probe`，`src/backend/dial/probe.rs`：组请求 · 拨一次 · 回结局），monitor 零测试连接。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_batcher_tests.rs"]
mod batcher_tests;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_parse_frame_tests.rs"]
mod parse_frame_tests;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_tier1_tests.rs"]
mod tier1_tests;

// 〔MIG-1〕`reannounce_tests`（F5 重宣告收集（`collect_reannounce`〔散文墓碑〕））随那本账搬进后端删了。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_snapshot_tail_tests.rs"]
mod snapshot_tail_tests;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_frame_dispatch_shape.rs"]
mod frame_dispatch_shape;

// 〔MIG-1〕`tmux_snapshot_exposure_tests`〔散文墓碑〕（tmux 快照不开 IPC 出口（快照本身删了））随那本账搬进后端删了。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_snapshot_tests.rs"]
mod snapshot_tests;

/// 有界读行的**行为**对拍〔devbench F10b〕。
///
/// # 为什么必须有这一组
///
/// 判据那半（`byte_cap_registry` 的四条）钉的全是**形态**：常量进表了没、
/// 处置臂说话了没。它们**判不出**「上限到底生不生效」——
/// 而后端侧栽的那次正是这个缺口：上限写着 1 MiB、`line_too_long` 也回了，
/// 可它是**读完再判**，实测 RSS 从 6 MiB 涨到 518 MiB。头注逐字记着
/// 「常数抄了先例，**机制没抄**」。
///
/// ⇒ 本组直接喂 reader，用**小 cap**，把那条「超限之后内存不涨」判成断言。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ssh_source_capped_line_tests.rs"]
mod capped_line_tests;
