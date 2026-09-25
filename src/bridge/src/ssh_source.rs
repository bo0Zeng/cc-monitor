//! SSH-remote 数据源（issue #15）。
//!
//! 本模块是**活代码**：从 lib.rs 的 `setup()` 调用（`remote.enabled=true` 且配置完整时）。
//! 它提供三块能力：
//! - **russh client 数据源**：[`run`] 连远端、exec backend、把 backend stdout 的
//!   line-delimited JSON 帧解析后走与本地 watcher 相同的出口（`batch_to_payloads` →
//!   `on_line_batch`），session 增减走专用 `session_changes` 通道。与本地 jsonl-watcher
//!   **并行**作为附加数据源（远端行带 origin=host 标签）。
//! - **ssh-config 导入**：[`list_ssh_host_aliases`] / [`resolve_ssh_host`]（`ssh -G`）
//!   供前端「从 ~/.ssh/config 导入」自动填连接参数。
//! - **测试连接**：[`test_remote_connection`] 实连一次，回 SSH ✓/✗ + host key 指纹 +
//!   backend ✓/✗（hello），供 UI 分级展示 + TOFU→strict 指纹固化。
//!
//! 上述三个 `#[tauri::command]` 在 lib.rs 的 invoke_handler! 里注册。
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
// InboundFrame 都是活代码；connect_and_exec 的 ClientHandler 等仍是骨架但已被 run 串起。
// 个别仅 S6+ 才读的字段（RemoteConfig 反序列化派生）保留 dead_code 容忍。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::event_replay::EventReplay;
use crate::ssh_link::ConnectStage;
// 〔SR1b · 2026-09-24〕从前这里把 `connect_session` / `ClientHandler` 从 `inproc_dial.rs` 再导出给 `sftp.rs`
//   （界面进程里最后一份 russh 拨号，唯一调用方就是 SFTP）。SFTP 进了本机常驻后端，那份文件整份删了，这一行随之删。
use crate::session_map::{RemovalCause, RemovedSid, SessionChange};

/// S0 **跨语言双写点**：backend 那侧 `RemovalCause::Superseded` 的 serde 线上名。
/// 改这里必须同步 `src/backend/wire.rs`（同 `TMUX_LS_FMT` 的纪律）。
const REMOVAL_CAUSE_SUPERSEDED: &str = "superseded";
use crate::watcher::JsonlLine;

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
/// | ① | `uname -m` 一次性 exec（选内嵌二进制的 arch） | `sftp::probe_remote_arch` |
/// | ② | SFTP 连接（读远端 `.build_id` marker） | `sftp::connect_sftp` |
/// | ③ | exec backend 起流 | `connect_and_exec` |
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

/// 纯函数：把当前退避翻倍并封顶到 [`RECONNECT_MAX`]。run() 的重连循环在"仍未连上"时调用。
fn next_backoff(cur: Duration) -> Duration {
    (cur * 2).min(RECONNECT_MAX)
}

/// 远端后端的连接配置。S5 会从 monitor 的 config 文件反序列化出来；
/// Tier 1（issue #15）的「测试连接」命令直接收前端传来的同形对象（camelCase）。
///
/// **serde camelCase 必须与前端 RemoteHostConfig / lib.rs::load_remote_configs 严格一致**：
/// host / port / user / keyPath / backendPath / hostKeyFingerprint / label（多机 #30，
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
    /// 远端要 exec 的后端命令（含参数前缀由 S5 决定）。
    pub backend_path: String,
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

/// Batch14-F45：单个连接目标（host + port）。竞发把 [`RemoteConfig::endpoints`] 的每项
/// 并发拨号，首个握手成功者胜。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

/// 解析一行地址 → [`Endpoint`]。支持四形态（与 android-terminal `parseAddressLine` 同语义）：
/// - `host`               → default_port
/// - `host:port`          → 显式端口
/// - `[IPv6]:port`        → 方括号 IPv6 + 端口
/// - `[IPv6]` / 裸 `IPv6` → default_port（裸 IPv6 靠「>1 个冒号」判定，不误当 host:port）
///
/// 空白/空串 → None；端口非法 → None（拒绝而非静默默认，防配置笔误）。
pub fn parse_address_line(line: &str, default_port: u16) -> Option<Endpoint> {
    let s = line.trim();
    if s.is_empty() {
        return None;
    }
    // 方括号形态：[v6] 或 [v6]:port
    if let Some(rest) = s.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        if host.is_empty() {
            return None;
        }
        let port = match after {
            "" => default_port,
            p => p.strip_prefix(':')?.parse().ok()?,
        };
        return Some(Endpoint {
            host: host.to_string(),
            port,
        });
    }
    // 裸 IPv6（>1 个冒号且无方括号）→ 整体是 host，无端口。
    if s.matches(':').count() > 1 {
        return Some(Endpoint {
            host: s.to_string(),
            port: default_port,
        });
    }
    // host:port 或 host
    match s.split_once(':') {
        Some((host, port)) if !host.is_empty() => Some(Endpoint {
            host: host.to_string(),
            port: port.parse().ok()?,
        }),
        Some(_) => None, // ":port" 无 host
        None => Some(Endpoint {
            host: s.to_string(),
            port: default_port,
        }),
    }
}

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

    /// Batch14-F45：所有连接目标，`host` 排首，`addresses` 依次追加，按 (host,port) 去重
    /// 保序。竞发按此顺序（配合 last-good 重排）拨号。空 addresses → `[host]`（老行为）。
    pub fn endpoints(&self) -> Vec<Endpoint> {
        let mut out: Vec<Endpoint> = Vec::new();
        let mut seen: std::collections::HashSet<Endpoint> = std::collections::HashSet::new();
        let mut push = |ep: Endpoint| {
            if seen.insert(ep.clone()) {
                out.push(ep);
            }
        };
        push(Endpoint {
            host: self.host.clone(),
            port: self.port,
        });
        for line in &self.addresses {
            if let Some(ep) = parse_address_line(line, self.port) {
                push(ep);
            }
        }
        out
    }
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

/// F45：per-origin「上次成功地址」——竞发时排首（下次大概率同一条路最快），赢家更新。
/// 进程内软状态,丢了只是少一次优化,不影响正确性。
fn last_good_store() -> &'static Mutex<std::collections::HashMap<String, Endpoint>> {
    static STORE: std::sync::OnceLock<Mutex<std::collections::HashMap<String, Endpoint>>> =
        std::sync::OnceLock::new();
    STORE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

pub(crate) fn last_good_for(origin: &str) -> Option<Endpoint> {
    last_good_store().lock().ok()?.get(origin).cloned()
}

pub(crate) fn record_last_good(origin: &str, ep: &Endpoint) {
    if let Ok(mut m) = last_good_store().lock() {
        m.insert(origin.to_string(), ep.clone());
    }
}

/// F45：当前应向该 origin 拨号的首选地址（PowerShell resume/attach 命令用它，而非盲取
/// `cfg.host`）。已连过 → last-good 胜者;否则 → endpoints 首个（= `host`）。永不 None
/// （endpoints 至少含 host）。
pub fn winner_address(cfg: &RemoteConfig) -> Endpoint {
    let origin = cfg.origin_label();
    if let Some(lg) = last_good_for(&origin) {
        // last-good 仍在当前配置里才用（配置改过则失效）。
        if cfg.endpoints().iter().any(|e| e == &lg) {
            return lg;
        }
    }
    cfg.endpoints().into_iter().next().unwrap_or(Endpoint {
        host: cfg.host.clone(),
        port: cfg.port,
    })
}

/// F45：竞发拨号顺序 = last-good 排首（若它仍在 endpoints 里），其余保序。纯函数,可测。
pub(crate) fn winner_order(
    endpoints: Vec<Endpoint>,
    last_good: Option<&Endpoint>,
) -> Vec<Endpoint> {
    let Some(lg) = last_good else {
        return endpoints;
    };
    if !endpoints.iter().any(|e| e == lg) {
        return endpoints; // last-good 已从配置移除 → 无视
    }
    let mut out = Vec::with_capacity(endpoints.len());
    out.push(lg.clone());
    for e in endpoints {
        if &e != lg {
            out.push(e);
        }
    }
    out
}

/// 连接远端、鉴权、开 session channel、exec `cfg.backend_path`，
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
#[path = "../../../tests/bridge/ssh_source_coldstart_preflight_guard.rs"]
mod coldstart_preflight_guard;

#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_coldstart_perf_guard.rs"]
mod coldstart_perf_guard;

#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_stream_flag_gate_tests.rs"]
mod stream_flag_gate_tests;

// ═══════════ 〔C2 · `设计/05 §13`〕拨号归后端：backend 那条长连接流也只经拨号代理 ═══════════
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

pub async fn connect_and_exec(
    cfg: &RemoteConfig,
    with_bg: bool,
    tail_only: bool,
    with_rbind_token: bool,
) -> Result<crate::dial_host::DialStream, String> {
    // backend 是长连接：代理那一侧 inactivity_timeout=None、keepalive 30s，靠 keepalive + EOF 检死链。
    // Batch7-F24/Batch8-F26：两个流模式 flag 都由调用方决定（run_stream 里绑定
    // "部署确认为当前版本"，见该处注释）。tail_only=true → backend 不重放历史
    // （历史由本侧旁路快照拉取），实时通道流量趋零。
    let mut cmd = shell_quote(&cfg.backend_path);
    if with_bg {
        cmd.push_str(" --with-bg");
    }
    if tail_only {
        cmd.push_str(" --tail-only");
    }
    // 🔴 `设计/80 §8.7` 步 3：**索要启动期令牌。**
    //
    // 这条 flag 的字面量是**跨进程双写点** —— 另一侧是后端的
    // `lib.rs::STREAM_FLAGS`（它必须认得并**剥离**这条 flag，否则会当成一次性查询、
    // 处理完就退出 ⇒ 无 hello ⇒ §26 那条重连死循环）。
    // 「声明了那条能力 ⟹ 会剥离对应 flag」是后端那侧的自证纪律，
    // 而「monitor 发的这一串与后端认的那一串逐字相同」由
    // `ssh_source_stream_flag_gate_tests.rs::the_stream_flags_monitor_sends_are_all_strippable`
    // 从**后端源文件**现抠着钉住（改任一侧会红）。
    if with_rbind_token {
        cmd.push_str(" --with-rbind-token");
    }
    // 〔C2〕`connect_and_exec_cmd` 从此只经拨号代理拿链路（它的函数体由 `dial_move_judge` 钉着）——
    // 所以这一行**不再是**「进程内回落」，它就是唯一那条路。
    connect_and_exec_cmd(cfg, &cmd).await
}

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
#[path = "../../../tests/bridge/ssh_source_dial_move_judge.rs"]
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

/// Batch9：已宣告会话的元数据缓存——归档清算（keys）+ F28 frontend-ready 重发
/// （payload+最新 status）+ F27 status 写回。
#[derive(Clone)]
pub(crate) struct AnnouncedMeta {
    pub(crate) payload: crate::bridge::RemoteSessionAddedPayload,
    pub(crate) status: Option<String>,
    pub(crate) waiting_for: Option<String>,
}

/// Batch9-F28：全局宣告账本 origin → (sid → meta)。写者 = 各主机 stream_loop
/// （added/status/removed + 连接退出清本 host）；读者 = frontend-ready 重发
/// （F5 后重建远端骨架/bg 元数据/初始灯——remote-session-added 不进 replay
/// buffer，Batch5 I-1 留档的缺口由此补上）。
static REMOTE_ANNOUNCED: std::sync::OnceLock<
    std::sync::Mutex<
        std::collections::HashMap<String, std::collections::HashMap<String, AnnouncedMeta>>,
    >,
> = std::sync::OnceLock::new();

fn announced_registry() -> &'static std::sync::Mutex<
    std::collections::HashMap<String, std::collections::HashMap<String, AnnouncedMeta>>,
> {
    REMOTE_ANNOUNCED.get_or_init(Default::default)
}

/// B2：全局 tmux 状态账本 origin → 最新 `tmux ls` 原文（backend `TmuxSessions` 帧推来）。写者 = 各主机
/// stream_loop（收 TmuxSessions 帧更新 / 连接退出清本 host）；读者 = tmux 对账 poller
/// （[`snapshot_tmux_by_origin`] 读 + `tmux::parse_tmux_ls` 解析），**替掉每 8s 新建 SSH 的
/// `list_remote_tmux` 轮询**（B2 治远端 sshd 日志刷屏）。
static REMOTE_TMUX_RAW: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, String>>,
> = std::sync::OnceLock::new();

fn tmux_raw_registry() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    REMOTE_TMUX_RAW.get_or_init(Default::default)
}

/// B2：快照「origin → 最新 tmux ls 原文」，供 tmux 对账 poller 读（零 SSH）。缺该 origin = 尚未推来
/// tmux 状态（backend 未发 / 连接刚起 / 断连已清）→ poller 本轮跳过该 origin（同「观测无效不累计缺失」）。
///
/// # ⚠ 刻意**不开** IPC 出口〔devbench F08, 08-10〕
///
/// 有人（包括一份审计清单）会看到「backend 推来的 `tmux_sessions` 帧只进这张表、前端却每 1s
/// 新建一条 SSH 跑 `list_remote_tmux`」，然后得出「开个 `#[tauri::command]` 把它暴露给前端
/// 就能消掉那条轮询」。**那个因果不成立**，理由是这份快照的**刷新时机**：
///
/// - backend 的 `TmuxProbeDue` **只在 `initial_tmux_probe` 发一次**（一次性初探）；
///   之后每一拍由 `Poke` 驱动，而 `Poke` 来自 tmux hook，**hook 只有 3 条**：
///   `session-created` / `session-closed` / `session-renamed`（`control/tmux_hook.rs::HOOK_EVENTS`）。
/// - 而 `tabs.ts` 的 `awaitExitFor` 等的是「**pane 前台命令从 claude 变回 shell**」——
///   会话还在，只是里面的命令换了。**那个变化不触发任何一条 hook** ⇒ 这份快照在那个场景下
///   **永不刷新** ⇒ 改读它 = 每次都等到 10s 超时再降级 kill，**功能退化**。
///
/// ⇒ 今天开出口**没有消费者**：另两个真实调用点（`fork-flow.ts` · `settings/machine-card.ts`）
/// 是**一次性查询**、不是轮询，走 SSH 没问题。开一个没人用的出口是装饰。
///
/// **解锁条件**（真出现了再回来开，别提前开）：
/// 先有一个「pane 前台命令变化」的事件源 —— tmux 没有这种 hook，能想到的路只有
/// 轮询 `capture-pane`（拿一个轮询换另一个）或让 claude 自己上报。
/// 那条轮询的账在 `polling_registry` 的 `src/tabs.ts` 一条里，**已如实登记为未排期**，
/// 本处只指过去、不抄一份。
/// **这张表唯一的写入口**（P3 刀 1）。
///
/// # 为什么要收成一个函数
///
/// 原来远端那处是就地 `tmux_raw_registry().lock().unwrap().insert(host_label, raw)`。
/// 刀 1 要让**本机**也写这张表，两处各写各的迟早分叉（一边存原文一边存解析后的、
/// 一边清一边不清）。⇒ 收成一个口，两侧共用 —— 这正是 `C1` 在数据面上的样子。
///
/// `origin` 的取值域**只有两类**：远端的 `host_label`/`origin_label`，
/// 或本机的 [`crate::backend::control::inbound_client::LOCAL_ORIGIN`]。
/// 由 `ssh_source_f032_idle_tests.rs::the_tmux_cache_has_one_writer_and_only_origin_keys` 钉住。
///
/// 〔`K-R19` 订正 09-03〕这一句原先点的是
/// `the_local_path_is_safe_only_because_local_sids_never_enter_the_tmux_cache`。
/// 那个名字**本文件里那条判据自己的 `///` 就写着「名字换过一次」**（原名今天主动误导：
/// 本地 sid 已经进表了），而这一句仍**当成现状在说**——同一份文件里两句话互相矛盾。
/// ⚠ 它之所以没人管：`K-R17` 的 ㈠ 只收 `文件.rs::符号` 形，**裸 `符号` 形不进人群**。
pub(crate) fn record_tmux_raw(origin: &str, raw: String) {
    // ⚠ **中毒也要拿到锁**〔D 阶段补审 08-11 修，B3〕。
    //
    // 原来是 `.lock().unwrap()`。本函数**跑在本机消费者那条裸 `std::thread` 上**
    // （`local_backend::local_stdio_consumer`），而那个线程没有 `catch_unwind`：
    // 一次锁中毒 panic 会 unwind 出整个消费者闭包 ⇒ `child` 锁里还留着活的 `Child`、
    // `pid` 没归 0、`unregister` 被跳过 ⇒ 没人再读后端的 stdout ⇒ 管道缓冲填满
    // ⇒ **backend 阻塞在 write 上冻死**；而 `backend_status` 照回 `channel: true` + 一个活 pid，
    // `start_local_backend` 也会以「已经在跑」拒绝重起。**全绿的死锁态，没有一处会响。**
    //
    // 处置对齐本仓既定做法（`inbound_client` 的两处 `unwrap_or_else(|e| e.into_inner())`）：
    // 一条陈旧的 `tmux ls` 原文远好过把整条读帧路炸掉。
    tmux_raw_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(origin.to_string(), raw);
}

/// 这张表唯一的**清除口**（与 [`record_tmux_raw`] 并列）。
///
/// # 为什么必须有它，且必须两侧都调
///
/// 断连 / 后端停掉之后那份 `tmux ls` 原文就是**陈旧证据**：
/// `find_tmux_origin_for_sid` 仍会按它返回 `Some(origin)`，
/// 而 `classify_removed(Some(_), Gone)` = `Idle` = **那个「永远消不掉、也 attach 不上的灰点」**。
///
/// ⚠ **补审 08-11 逮到本机那半从来不清**：远端断连走这条路（Batch9-F28 就写着），
/// 而本机消费者流结束时只 `unregister` 入方向 client、不碰这张表 ⇒
/// 停掉本机后端之后 `<local>` 那份原文**永久留着**。
/// 判据当时没发现，因为它只数 `insert`（`.remove(` 也是写者，见那条判据的订正）。
pub(crate) fn forget_tmux_raw(origin: &str) {
    tmux_raw_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(origin);
}

pub fn snapshot_tmux_by_origin() -> std::collections::HashMap<String, String> {
    tmux_raw_registry().lock().unwrap().clone()
}

/// 只取**一个** origin 的那份原文〔D 阶段补审 08-12〕。
///
/// 两处与 [`snapshot_tmux_by_origin`] 不同，都是补审逼出来的：
/// ① **中毒也要拿到锁**。写入口 `record_tmux_raw` 早就是 `unwrap_or_else(|e| e.into_inner())`
///    （B3 那条：它跑在本机消费者那条没有 `catch_unwind` 的裸线程上，一次中毒 panic
///    会滚成「全绿的死锁态」）。而这个读口若还是 `.unwrap()`，中毒之后
///    `tmux.rs::list_local_tmux` 就变成必 panic 的 tauri 命令 —— 等于把写侧刚补好的那道又从读侧漏掉。
///    〔`K-R19` 订正 09-03〕这里原先写的是 `local_tmux_names`，**全仓零定义**：
///    真名从来就是 `list_local_tmux`（`lib.rs` 的注册表与 `commands.ts` 的包装层都是这个）。
/// ② **不克隆整张表**。调用方只要本机那一份，`snapshot_tmux_by_origin` 会把所有 origin 的
///    `tmux ls` 原文全拷一遍；那是每次本机 resume 都白付一次的钱。
pub(crate) fn tmux_raw_for(origin: &str) -> Option<String> {
    tmux_raw_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(origin)
        .cloned()
}

/// audit-fixes F03.2：idle-tmux 账本 origin → idle sids（claude 退出但 tmux 会话尚在）。
/// **唯一写者 = remote-session-emitter**（`mark_idle`/`clear_idle` 只在 lib.rs emitter 调）；读者 =
/// 收帧收割器（`snapshot_idle_for_origin`）、断连 flush、F5 对账（`snapshot_idle_by_origin`）均**只读**。
/// 守 §24：idle 是 `remote_active` **之外**的第三态，此账本与 remote_active 正交、不互写。
static REMOTE_IDLE: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, std::collections::HashSet<String>>>,
> = std::sync::OnceLock::new();

fn idle_registry(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, std::collections::HashSet<String>>>
{
    REMOTE_IDLE.get_or_init(Default::default)
}

/// F03.2：标记 sid 在 origin 上进入 idle-tmux。**只由 emitter 调**（唯一写者）。
pub fn mark_idle(origin: &str, sid: &str) {
    idle_registry()
        .lock()
        .unwrap()
        .entry(origin.to_string())
        .or_default()
        .insert(sid.to_string());
}

/// F03.2：清 sid 的 idle 标记（跨 origin，sid 全局唯一）。**只由 emitter 调**（added / archived 时）。幂等。
pub fn clear_idle(sid: &str) {
    let mut reg = idle_registry().lock().unwrap();
    for sids in reg.values_mut() {
        sids.remove(sid);
    }
    reg.retain(|_, sids| !sids.is_empty());
}

/// F03.2：读某 origin 的 idle sid 集（收帧收割器把 idle 并入 tracked，使其能被去抖归档）。只读。
pub fn snapshot_idle_for_origin(origin: &str) -> std::collections::HashSet<String> {
    idle_registry()
        .lock()
        .unwrap()
        .get(origin)
        .cloned()
        .unwrap_or_default()
}

/// F03.2：读全部 idle 账本（F5 对账：把 idle sid 排除出"死"判据 + 重发 SESSION_IDLE）。只读。
pub fn snapshot_idle_by_origin(
) -> std::collections::HashMap<String, std::collections::HashSet<String>> {
    idle_registry().lock().unwrap().clone()
}

/// audit-fixes F03.2（**纯函数**，idle 判定核心，Linux 可单测）：给「origin → tmux ls 原文」账本，
/// 找 `@ccm_sid == sid` 出现在哪个 origin 的帧里。**只认 @ccm_sid 列**（`parse_tmux_ls` 的 `sid`
/// 字段），**不看 command**——command-agnostic：`TmuxSessions` 帧的新鲜度**由 hook 决定**（P5 后零定时器；
/// hook 覆盖到的近乎即时，覆盖不到的可能**永不刷新** —— 见 `classify_removed` 头注那个 `/branch` 洞），claude 退出瞬间那帧
/// 的 command 列可能仍是 claude，卡 `command!=claude` 会正常退出高频误判 archived、丢灰灯。故改用
/// 「claude 死」由 backend-removed（emitter 触发边沿）判、「tmux 在」由 `@ccm_sid` present 判（claude
/// 退出后 wrapper watcher 停写但**不 unset**，session 级 option 恒 present——devbox 已实测）。`NO_TMUX`/空跳过。
fn tmux_origin_for_sid(
    by_origin: &std::collections::HashMap<String, String>,
    sid: &str,
) -> Option<String> {
    for (origin, raw) in by_origin {
        if crate::backend::control::tmux::parse_tmux_ls(raw)
            .iter()
            .any(|s| s.sid.as_deref() == Some(sid))
        {
            return Some(origin.clone());
        }
    }
    None
}

/// F03.2：`tmux_origin_for_sid` 的公开包装——读当前 tmux 快照后调纯函数。emitter 判 idle vs archived 用。
/// ★★ `P0b-Y2` 第十七拍〔08-13〕：从一份 `tmux ls` 原文里**摘掉**某个会话那一行。
///
/// # 为什么需要它（`#60` 现象 2：永久灰点）
///
/// 会话关闭帧到达时 `ssh_source` 会 retire 那个 sid；而下游 `classify_removed` 要查
/// 「这个 sid 的 tmux 还在不在」——它读的是**同一份还没更新的快照** ⇒ 仍看得见那个会话
/// ⇒ 判 `Idle`（灰）而不是 `Archive`（归档）⇒ **永久灰点、按旧 sid 也 attach 不上**。
///
/// 08-13 全链实测（灰灯修好之后那一跑）：`tmux 会话 cc-b471a22f 关闭 ⇒ 立刻 retire`
/// 紧接着 `remote removed 到达 … tmux_origin=Some("e2e-loopback")` → `remote session idle-tmux`。
/// ★ `lib.rs` 那段头注早就写下过同一个病（当时针对 `Superseded`）：
/// 「查快照必然误判成灰点，且那份快照在 P5 删掉 ticker 之后**没有任何事件路径会刷新它**」。
///
/// # 为什么是「摘一行」而不是「重新探一次」
///
/// 重新探要跨 SSH、要等，而**我们已经知道结论了**（backend 的正向死亡帧就是结论）。
/// 摘一行是**把已知事实写进账本**，不是猜。
/// ⚠ 只按**第一列（会话名）逐字相等**摘 —— 不做前缀匹配（`cc-a` 与 `cc-ab` 会互相误伤）。
pub fn remove_tmux_line(raw: &str, name: &str) -> String {
    let mut out: Vec<&str> = raw
        .lines()
        .filter(|l| l.split('\t').next() != Some(name))
        .collect();
    // 原文以 `\n` 结尾（`tmux ls` 的形状）—— 保持它，免得下游按行切时多出一格空行差异。
    if raw.ends_with('\n') && !out.is_empty() {
        out.push("");
    }
    out.join("\n")
}

pub fn find_tmux_origin_for_sid(sid: &str) -> Option<String> {
    tmux_origin_for_sid(&snapshot_tmux_by_origin(), sid)
}

/// 〔U4b · 第四波 · G2〕**只在一个 origin 那份 `tmux ls` 原文里**找 `@ccm_sid == sid`（纯函数）。
///
/// 本机那一臂要问的是「**本机**的 tmux 里还有没有它」—— 不许跨 origin 猜：sid 在别的机器的原文里出现
/// （拷过去的会话、同 sid 的 resume）说的是那台机器，不是本机。远端那一臂仍用
/// [`find_tmux_origin_for_sid`]（它本来就要知道「在哪台」，答案会被 `mark_idle(origin, …)` 记下）。
pub(crate) fn tmux_origin_for_sid_at(
    by_origin: &std::collections::HashMap<String, String>,
    at: &crate::origin::Origin,
    sid: &str,
) -> Option<String> {
    let key = at.as_wire_str();
    let raw = by_origin.get(key)?;
    crate::backend::control::tmux::parse_tmux_ls(raw)
        .iter()
        .any(|s| s.sid.as_deref() == Some(sid))
        .then(|| key.to_string())
}

/// [`tmux_origin_for_sid_at`] 读本机那一格（`<local>`）的生产包装。
pub fn find_local_tmux_origin_for_sid(sid: &str) -> Option<String> {
    let origin = crate::backend::control::inbound_client::LOCAL_ORIGIN;
    let raw = tmux_raw_for(origin)?;
    let one: std::collections::HashMap<String, String> =
        std::iter::once((origin.to_string(), raw)).collect();
    tmux_origin_for_sid_at(&one, &crate::origin::Origin(origin.to_string()), sid)
}

/// audit-fixes F03.2（D 审计②覆盖缺口）：backend-removed 到达时的分流决策。emitter 收 removed 后
/// 据「该 sid 的 tmux 是否仍在」（`find_tmux_origin_for_sid` 的 Option）择一：
/// `Idle{origin}`=tmux 会话尚在 → 灰灯（mark_idle + emit SESSION_IDLE + **不 forget**）；
/// `Archive`=tmux 也没了 → 归档（clear_idle + forget + emit SESSION_ENDED）。
#[derive(Debug, PartialEq, Eq)]
pub enum RemovedDisposition {
    Idle { origin: String },
    Archive,
}

/// **纯决策**（可单测，锁住「Some/None 不写反」——emitter 里的实际接线在 run() 闭包内无法单测，
/// 抽出映射层给最易犯的分支互换上变异锚点）。
///
/// ★ S0：**`cause` 先于快照裁决**。
/// - [`RemovalCause::Superseded`]（同 pidfile 原地换 sid = `/branch`）⇒ 恒 `Archive`，
///   **根本不看 `tmux_origin`**。原因是那个入参对这个场景恒错：旧 sid 的 tmux 格子还在，
///   但它现在挂的是**新** sid；而 `tmux_origin` 读的是缓存的 `tmux ls` 原文，那份缓存
///   在 P5 删掉 8s ticker 之后**没有任何事件路径会因 /branch 去刷新它**。
///   ⇒ 判成 Idle 就是一个永远消不掉、也 attach 不上的灰点（用户 2026-07-30 实测）。
/// - [`RemovalCause::Gone`] ⇒ 维持原语义：Some(origin)→Idle；None→Archive。
pub fn classify_removed(tmux_origin: Option<String>, cause: RemovalCause) -> RemovedDisposition {
    if cause == RemovalCause::Superseded {
        return RemovedDisposition::Archive;
    }
    match tmux_origin {
        Some(origin) => RemovedDisposition::Idle { origin },
        None => RemovedDisposition::Archive,
    }
}

// audit-fixes F03.2：`snapshot_announced_by_origin` 已删——其唯一读者是已删的 8s poller。
// 收帧收割器直接用 stream_loop 本连接的 `announced` 局部（keys=live sids），不需全局快照。

/// audit-fixes F03.2（**纯函数**，收帧收割器的 tracked 集）：收割器要对账「哪些 sid 该在 tmux 后端里」
/// = 本连接 announced（live 会话）**∪** 本 origin idle 会话。**idle 必须并进来**——否则 idle→archived
/// 无产出者：idle sid 一旦离开 announced，就不再被 reconcile 追踪，tmux 真没了也永不 retire = 灰灯永久
/// 卡死关不掉（三轮独立复审的红线④）。抽成纯函数即为给这条不变量上单测（变异：只 announced 不并 idle→红）。
fn reaper_tracked(
    announced_sids: impl Iterator<Item = String>,
    idle: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    let mut tracked: std::collections::HashSet<String> = announced_sids.collect();
    tracked.extend(idle.iter().cloned());
    tracked
}

/// F28：frontend-ready 时重发所有已宣告远端会话（骨架 + 初始灯）。幂等
/// （createSkeletonTab/updateActivity 均幂等）；宣告先于该会话 replay 行 emit
/// 由调用方保证（lib.rs 在 replay 之前调本函数）。
pub fn reannounce_all(app: &tauri::AppHandle) {
    // F5 电平同步（先于骨架重发——batch 调度信号越早越好）
    emit_snapshot_inflight_level(app);
    let snapshot = {
        let reg = announced_registry().lock().unwrap();
        collect_reannounce(&reg)
    };
    if snapshot.is_empty() {
        return;
    }
    tracing::info!(
        "F28 reannounce: {} 个远端会话（F5 骨架/灯重建）",
        snapshot.len()
    );
    for meta in snapshot {
        let sid = meta.payload.session_id.clone();
        if let Err(e) = app.emit(crate::bridge::events::REMOTE_SESSION_ADDED, &meta.payload) {
            tracing::warn!("reannounce remote-session-added emit failed: {e}");
        }
        let act = crate::bridge::SessionActivityPayload {
            session_id: sid,
            status: meta.status,
            waiting_for: meta.waiting_for,
        };
        if let Err(e) = app.emit(crate::bridge::events::SESSION_ACTIVITY, &act) {
            tracing::warn!("reannounce session-activity emit failed: {e}");
        }
    }
}

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
        snapshot_inflight_change(&app, 1);
        tauri::async_runtime::spawn(async move {
            let _permit = permit;
            struct InflightGuard(tauri::AppHandle);
            impl Drop for InflightGuard {
                fn drop(&mut self) {
                    snapshot_inflight_change(&self.0, -1);
                }
            }
            let _inflight = InflightGuard(app.clone());
            let sid_short: String = item.sid.chars().take(8).collect();
            let mut last_err = String::new();
            for attempt in 1..=2 {
                match fetch_snapshot(&q, &item, &host_label, &replay, &app).await {
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
            let payload = crate::bridge::RemoteHealthPayload {
                origin: host_label.clone(),
                kind: "snapshot".to_string(),
                message: format!(
                    "会话 {sid_short} 的历史快照拉取失败（{last_err}）——该 Tab 暂只有实时消息，可从历史浏览器查看完整内容。"
                ),
            };
            if let Err(e) = app.emit(crate::bridge::events::REMOTE_HEALTH, payload) {
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

fn snapshot_inflight_change(app: &tauri::AppHandle, delta: isize) {
    let mut n = SNAPSHOT_INFLIGHT.lock().unwrap();
    *n = if delta > 0 {
        *n + 1
    } else {
        n.saturating_sub(1)
    };
    let count = *n;
    // 持锁 emit：保证事件到达序 == 计数变化序（emit 是入队非阻塞，临界区极短）
    if let Err(e) = app.emit(
        crate::bridge::events::SNAPSHOT_INFLIGHT,
        &serde_json::json!({ "count": count }),
    ) {
        tracing::warn!("snapshot-inflight emit failed: {e}");
    }
    drop(n);
}

/// F28 重发收集（纯函数，单测锚定）：拍平全部主机的已宣告元数据并按
/// (origin, sid) 稳定排序——HashMap 迭代序每次 F5 洗牌 tab 栏（审计 D）。
fn collect_reannounce(
    reg: &std::collections::HashMap<String, std::collections::HashMap<String, AnnouncedMeta>>,
) -> Vec<AnnouncedMeta> {
    let mut v: Vec<AnnouncedMeta> = reg.values().flat_map(|m| m.values().cloned()).collect();
    v.sort_by(|a, b| {
        (&a.payload.origin, &a.payload.session_id).cmp(&(&b.payload.origin, &b.payload.session_id))
    });
    v
}

/// F5 电平同步（审计 D）：inflight 是变化沿事件，重载后前端初值 0——回填在途
/// 时 F5 会退回纯 300ms 启发式。frontend-ready（reannounce）时补发当前电平。
pub fn emit_snapshot_inflight_level(app: &tauri::AppHandle) {
    let count = *SNAPSHOT_INFLIGHT.lock().unwrap();
    if let Err(e) = app.emit(
        crate::bridge::events::SNAPSHOT_INFLIGHT,
        &serde_json::json!({ "count": count }),
    ) {
        tracing::warn!("snapshot-inflight level emit failed: {e}");
    }
}

/// fetch 的三态结果：完成（行数）/ 被取消（不重试）。错误走 Err。
enum FetchOutcome {
    Done(u64),
    Cancelled,
}

/// 判定快照流的一行是否计入行号（**必须与 backend `read_new_lines` 一字一致**：
/// BOM + 全空白的行跳过且不消耗 seq——两路 seq 同处行号空间的前提）。
fn snapshot_line_countable(line: &str) -> bool {
    !line.trim_start_matches('\u{feff}').trim().is_empty()
}

/// 拉取单个会话的完整历史快照并灌进既有管线。
///
/// 🔴 〔`C1` · 2026-09-24〕**不再为每份快照单拨一条 SSH。** 此前这里 exec 一次
/// `<backend> --read-session-tail <p> 500`，读它一口气印出来的「meta ＋ 尾段 ＋ 头段」；
/// 现在走已有长连接：先 `history-tail` 问那张图（`total` / `tail_from` / 两段的字节边界），
/// 再按 `[split_at, end)`、`[0, split_at)` 两段用 `history-read` 分页取正文 ——
/// 与那条子命令印出的两段**逐字节相同**（后端扫的是同一个函数），行号映射（[`tail_seq`]）一个字没动。
///
/// 每个 chunk 边界查取消（会话 removed / 连接断）——中止并**补偿 emit 一次
/// session-ended**：若某个已 flush 的 chunk 恰把归档 tab"见行复活"，这里把它
/// 压回 archived（审计 D-B1 僵尸复活的封口；archiveTab 幂等，重复无害）。
///
/// 完整性校验（审计 D-I2）：到达的可计行数必须**恰好等于** `total`。
async fn fetch_snapshot(
    q: &std::sync::Arc<SnapshotQueue>,
    item: &SnapshotItem,
    host_label: &str,
    replay: &Arc<EventReplay>,
    app: &tauri::AppHandle,
) -> Result<FetchOutcome, String> {
    use crate::backend::control::frame_query;
    let sid = &item.sid;
    let path = &item.path;
    let origin = crate::origin::Origin(host_label.to_string());
    let plan = frame_query::tail(&origin, path, SNAPSHOT_TAIL_LINES as u64).await?;
    // 〔C2 · U3 第 3 件〕断线重连后从续点接着拉（`snapshot_resume` 头注），续点对不上才整份。
    let how = crate::snapshot_resume::plan_read(
        crate::snapshot_resume::cursor_of(&origin, sid).as_ref(),
        path,
        &plan,
    );
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
    let mut total_bytes: u64 = 0;
    let mut chunk: Vec<JsonlLine> = Vec::with_capacity(SNAPSHOT_CHUNK_LINES);
    let mut cancelled = false;
    'read: for (from, upto) in walk.segments().to_vec() {
        let mut offset = from;
        while offset < upto {
            let page = frame_query::read_page(&origin, path, offset, Some(upto)).await?;
            total_bytes += page.next - offset;
            if total_bytes > SNAPSHOT_MAX_BYTES {
                // 防御上限：不再继续拉（完整性校验会把截断判为失败 → toast）。
                tracing::warn!(
                    "snapshot [{host_label}] {sid}: 超过 {SNAPSHOT_MAX_BYTES} 字节上限，截断"
                );
                break 'read;
            }
            for line in page.text.split('\n') {
                let line = line.trim_end_matches('\r');
                if !snapshot_line_countable(line) {
                    continue;
                }
                let Some(seq) = walk.step() else {
                    continue; // 续传：锚到续点之间的行前端已有，数掉不发
                };
                chunk.push(JsonlLine {
                    session_id: sid.to_string(),
                    path: std::path::PathBuf::from(path),
                    seq,
                    raw: line.to_string(),
                });
                if chunk.len() >= SNAPSHOT_CHUNK_LINES {
                    if q.is_cancelled(sid) {
                        cancelled = true;
                        break 'read;
                    }
                    flush_lines(replay, app, host_label, std::mem::take(&mut chunk)).await;
                }
            }
            offset = page.next;
            if page.eof {
                break;
            }
        }
    }
    if cancelled || q.is_cancelled(sid) {
        // 补偿归档（见 doc comment）；丢弃未 flush 的 chunk。
        let payload = crate::bridge::SessionEndedPayload {
            session_id: sid.to_string(),
        };
        if let Err(e) = app.emit(crate::bridge::events::SESSION_ENDED, payload) {
            tracing::warn!("snapshot 补偿归档 emit failed: {e}");
        }
        return Ok(FetchOutcome::Cancelled);
    }
    if !chunk.is_empty() {
        flush_lines(replay, app, host_label, chunk).await;
    }
    // 完整性校验：`total` 精确对账（F30）—— 续传时对的是「锚之后那一截」。
    let (arrived, want) = (walk.arrived(), walk.want());
    if arrived != want {
        return Err(format!(
            "快照不完整：{arrived}/{want} 行（连接中断或后端报错）"
        ));
    }
    // 下界：宣告时 prime 的行数 L（`session_added.lines`）—— 文件在宣告之后被截短才会撞上。
    if let Some(expected) = item.expected_lines {
        if plan.total < expected {
            return Err(format!(
                "快照不完整：{}/{expected} 行（连接中断或后端报错）",
                plan.total
            ));
        }
    }
    // `[0, total)` 全到了（整份：刚发完；续传：锚之前的早有、之后的刚发完）⇒ 立锚。
    crate::snapshot_resume::note_snapshot_done(&origin, sid, path, &plan);
    Ok(FetchOutcome::Done(arrived))
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

/// [`connect_and_exec`] 的通用形态：exec 任意命令行（issue #16：历史查询走
/// `<backend_path> --list-projects` 等一次性命令，与流式后端同一连接建立逻辑、
/// 各自独立连接互不影响）。
pub async fn connect_and_exec_cmd(
    cfg: &RemoteConfig,
    cmd: &str,
) -> Result<crate::dial_host::DialStream, String> {
    // 〔C2 → SR1a〕拨号在本机常驻后端里；这里拿到的是它开的一条链路（读端 = 远端命令的 stdout）。
    crate::dial_host::open_stream(cfg, cmd).await
}

/// 一次远端 exec 的**完整**结果：stdout、stderr、退出码。
///
/// # 为什么需要它（而不是继续用 `connect_and_exec_cmd`）
///
/// `Channel::into_stream()` 只搬 `ChannelMsg::Data` —— **`ExtendedData`（= stderr）
/// 与 `ExitStatus` 都被丢掉**（russh 0.61 `channels/io/mod.rs`）。所以既有的
/// `run_list_query` 那条路只能看见 stdout：远端命令失败时它读到 0 行，
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
/// 那是后端侧栽过的坑，逐字记在 `src/backend/inbound.rs` 头注里：
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

/// 超限那一行的用户可见说法。**抽成纯函数**的理由与 [`overflow_health_message`] 逐字相同：
/// 消费点要真 `AppHandle` 测不了，而措辞对不对恰恰是要钉的东西。
fn line_too_long_health_message(host_label: &str, bytes: u64) -> String {
    format!(
        "远端 [{host_label}] 发来一行 {bytes} 字节，超过单行上限 {BACKEND_FRAME_LINE_CAP} 字节，\
         这一行**已整行丢弃**。这不是网络拥塞 —— 要么该会话里有异常巨大的一条记录，\
         要么对端不是本工具的后端。重开该会话可看完整历史。"
    )
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

/// POSIX shell 单引号转义（issue #16：历史查询的路径参数经远端 shell 解析，
/// 含空格/特殊字符必须包引号；单引号本身按 `'\''` 规则逃逸）。
pub fn shell_quote(s: &str) -> String {
    // U8c-2b-0（账本 S5）：实现收进 `shell-quote-core`（P4c 前叫 `launch-core`），
    // 此处只留名字（`pub`，全仓多处在用）。
    shell_quote_core::posix_quote(s)
}

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
    },
    /// 一行从远端 session jsonl 尾随读到的原始行。字段语义与本地 `watcher::JsonlLine` 对齐。
    Line {
        session_id: String,
        path: String,
        seq: u64,
        raw: String,
    },
    /// 远端新出现一个 session 文件。Batch7-F24：p1e backend 附带 pidfile 元信息
    /// （additive）；旧后端缺字段 → None（保守视为交互）。
    SessionAdded {
        sid: String,
        session_kind: Option<String>,
        /// E73（additive）：attach 进去对人有没有意义。缺席 = true（存量零迁移）。
        /// 语义与来源见 `src/backend/wire.rs` 的同名字段 + `src/doc/IPC-PROTOCOL.md` §9.3。
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
        /// 完整论证住 `src/backend/wire.rs` 的同名字段与 `src/doc/IPC-PROTOCOL.md` §9.3。
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
        /// 交给 `session_facts::note_container`（本机那条流同一个口）。
        container: Option<crate::session_facts::Container>,
    },
    /// 〔U4b · 第四波〕后端的活会话清单报完了（Phase 1 走完）。无载荷。
    SessionsReplayed,
    /// Batch9-F27：会话 status 变化（p1g backend；远端红绿灯）。
    SessionStatus {
        sid: String,
        status: Option<String>,
        waiting_for: Option<String>,
    },
    /// 远端一个 session 文件消失。
    SessionRemoved {
        sid: String,
        /// S0：backend 明说的移除原因（缺字段 ⇒ [`RemovalCause::Gone`]，与旧后端兼容）。
        cause: RemovalCause,
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
    /// B2：backend 在远端本地跑 `tmux ls` 的原始 stdout（或哨兵 `NO_TMUX`）——喂 tmux 对账，
    /// 替掉每 8s 新建 SSH 的刷屏轮询。`raw` 由 `tmux::parse_tmux_ls` 解析（`NO_TMUX`→无 tmux）。
    ///
    /// P1（zero-poll-liveness）：`observation` = backend 的**显式观测分类**（additive；旧 backend
    /// 为 `None`）。分类判定见 `tmux::classify_tmux_observation`——它把「确证零会话」与
    /// 「观测失败」分开，这是修掉 §24bis 灰灯卡死的关键（空 `raw` 在旧协议里两义不可分）。
    /// P5：backend 差分算出的**正向死亡帧**——某个 tmux 会话确定关闭了。
    /// 收到即 retire、绕过 miss 计数（快照 + miss 那条路原样保留作兜底）。
    TmuxSessionClosed { name: String },
    TmuxSessions {
        raw: String,
        observation: Option<String>,
    },
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
        return format!(
            "远端 [{host_label}] 管道拥塞，可能丢失约 {dropped} 条实时行；重开该会话可看完整历史。"
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
        "；受影响清单**不全**，建议刷新该来源"
    } else {
        ""
    };
    format!(
        "远端 [{host_label}] 管道拥塞，丢了约 {dropped} 条帧，其中 {} 条是**会话状态变化**{named}\
         —— 这部分**重开会话补不回来**，请手动刷新该来源{truncated_note}。",
        lost.len()
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
            Some(InboundFrame::Hello {
                v,
                build_id,
                host_arch,
                claude_dir,
                homes,
                capabilities,
                commands,
            })
        }
        "line" => {
            let session_id = obj.get("session_id")?.as_str()?.to_string();
            let path = obj.get("path")?.as_str()?.to_string();
            let seq = obj.get("seq")?.as_u64()?;
            let raw = obj.get("raw")?.as_str()?.to_string();
            Some(InboundFrame::Line {
                session_id,
                path,
                seq,
                raw,
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
                    .and_then(crate::session_facts::Container::from_wire),
            })
        }
        // 〔U4b · 第四波〕additive 新帧，无载荷。旧后端不发 ⇒ 这条分支永不命中，固定的 tab 停在「说不清」。
        "sessions_replayed" => Some(InboundFrame::SessionsReplayed),
        "session_status" => {
            let sid = obj.get("sid")?.as_str()?.to_string();
            let opt = |k: &str| obj.get(k).and_then(|v| v.as_str()).map(str::to_string);
            Some(InboundFrame::SessionStatus {
                sid,
                status: opt("status"),
                waiting_for: opt("waiting_for"),
            })
        }
        "session_removed" => {
            let sid = obj.get("sid")?.as_str()?.to_string();
            // ★ S0（additive）：`cause` 缺省 = `Gone`，旧后端原样工作。
            // **双写点**：字面量与 backend `src/backend/wire.rs::RemovalCause`
            // 的 serde 名逐字一致，由 `removal_cause_wire_literal_stays_in_sync` 钉住。
            // 未知取值也退回 `Gone`（宁可保守判活，不可凭一个不认识的词直接归档）。
            let cause = match obj.get("cause").and_then(|v| v.as_str()) {
                Some(REMOVAL_CAUSE_SUPERSEDED) => RemovalCause::Superseded,
                _ => RemovalCause::Gone,
            };
            Some(InboundFrame::SessionRemoved { sid, cause })
        }
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
        // P5：additive 新帧。缺 `name` / 非字符串 → 坏帧跳过（不 panic），
        // 与其余帧同一口径。**旧后端不发它** ⇒ 这条分支永不命中，行为退回快照+miss。
        "tmux_session_closed" => {
            let name = obj.get("name")?.as_str()?.to_string();
            Some(InboundFrame::TmuxSessionClosed { name })
        }
        "tmux_sessions" => {
            // B2：raw = tmux ls 原文（或 NO_TMUX）。缺/非字符串 → 坏帧跳过。
            let raw = obj.get("raw")?.as_str()?.to_string();
            // P1：observation 是 additive 可选字段——**缺失/非字符串都当 None**（不是坏帧）。
            // 旧后端没有它；非字符串是坏后端，退化成旧判据即今天的保守行为。
            let observation = obj
                .get("observation")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            Some(InboundFrame::TmuxSessions { raw, observation })
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
    "cancelled",
    "hello",
    "line",
    "link_data",
    "link_end",
    "overflow",
    "reply",
    "session_added",
    "session_removed",
    "session_status",
    "sessions_replayed",
    "tmux_session_closed",
    "tmux_sessions",
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
        },
        "failed" => crate::sftp_pool::End::Failed(e.get("why")?.as_str()?.to_string()),
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
#[path = "../../../tests/bridge/ssh_source_emits_parity.rs"]
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
    parked: &mut Option<crate::backend::control::inbound_client::ParkedWriter<W>>,
    frame: Option<&InboundFrame>,
) -> Option<std::sync::Arc<crate::backend::control::inbound_client::InboundClient>>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let witness = crate::backend::control::inbound_client::BackendHello::from_hello_frame(frame?)?;
    let client = parked.take()?.into_client(witness);
    crate::backend::control::inbound_client::register(host_label, client.clone());
    Some(client)
}

/// U8a-2a：把一帧入方向应答路由回请求方。返回是否真的交到了某个等待者手上。
///
/// 没有客户端 = backend 在 hello 之前就回了应答（协议倒错），照实报、不静默。
///
/// **抽成函数同样是为了可测**（D 审计变异 MU12：把 `route_reply` 换成丢弃 ⇒ 全绿）。
fn route_inbound_frame(
    host_label: &str,
    client: Option<&std::sync::Arc<crate::backend::control::inbound_client::InboundClient>>,
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
/// - MU14：`probe_control_channel` 直接返回 `"control=ok(0ms)"`，一个字节都不发
///
/// 也就是「把发送端接上」这件事本身删掉之后 CI 一片绿 —— `inbound_client` 的单测走的是
/// 自造客户端，e2e 走的是真后端二进制，**两者之间的接缝没有任何判据**。
/// 这个模块就是那条接缝。
#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_seam_tests.rs"]
mod seam_tests;

/// U8a-2a：**写半边只许经 `inbound_client::park` 出手。**
///
/// 这条护栏存在的理由是它守的东西刚变过：这个文件从「只读一条流」变成了「双工」。
/// 一旦有人为了图省事在这里直接 `write_all` 一行，`ParkedWriter` 那层
/// 「Hello 之前不许写」的类型保证就被绕过了 —— 而且是**静默**绕过（编译、测试全绿）。
#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_write_half_guard.rs"]
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
const EXPECTED_BACKEND_BUILD_ID: &str = env!("BACKEND_BUILD_ID");

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
fn version_warning(reported_v: u64, reported_build_id: &str, label: &str) -> Option<String> {
    match negotiate_version(reported_v, reported_build_id) {
        VersionVerdict::Ok => None,
        VersionVerdict::StaleBuild { reported } => Some(format!(
            "远端 [{label}] backend 版本 {reported} 与本机期望 {EXPECTED_BACKEND_BUILD_ID} 不一致，建议更新后端（后续将支持自动部署）。"
        )),
        VersionVerdict::Incompatible { reported_v } => Some(format!(
            "远端 [{label}] backend 协议版本 v={reported_v} 与本机期望 v={EXPECTED_PROTO_V} 不兼容，渲染可能异常，请更新后端。"
        )),
    }
}

/// SSH-remote 数据源主循环（S5）。
///
/// 连接远端、exec backend、把 backend stdout 的 line-delimited JSON 帧逐行解析后分发：
/// - `hello` → log（证明 backend runtime 起来了）+ 置 `connected`（标记本次连接已健康，
///   供重连循环判定是否重置退避）。
/// - `line` → 组 [`JsonlLine`] 走 **与本地 watcher 完全相同的出口**：
///   `crate::batch_to_payloads(...)` → `replay.on_line_batch(&app, ...)`。Phase-0 用最简正确
///   做法：每帧一条 batch（前端按 seq 自动排序，单条 emit 语义与本地小 batch 一致）。
/// - `session_added` / `session_removed` → 走**专用** remote `session_changes` 通道
///   （`SessionChange{added,removed}`），由 lib.rs 那个 remote-session-emitter 线程消费：
///   removed → emit session-ended（远端 Tab 归档）；added 无操作（远端 Tab 由 line 帧
///   经前端 ensureTab 创建，无本地 jsonl 可重扫、无本地 HWND 可绑定）。
/// - 未知 kind / garbage → `tracing::warn!` 跳过，绝不中断流。
///
/// stdout EOF / 读错误 → 返回 `Err`，调用方（S8/S9）据此大声报"connection dropped"，
/// 不静默冻结。
pub async fn run(
    cfg: RemoteConfig,
    replay: Arc<EventReplay>,
    app: tauri::AppHandle,
    session_changes: std::sync::mpsc::Sender<SessionChange>,
    connected: Arc<AtomicBool>,
) -> Result<(), String> {
    tracing::info!(
        "ssh_source connecting to {}@{}:{} (backend={})",
        cfg.user,
        cfg.host,
        cfg.port,
        cfg.backend_path
    );

    // FIX 2（issue #15 review）：跟踪当前**已向前端宣告**的远端 sid。stream_loop 在每条
    // SessionAdded forward 时 insert、SessionRemoved forward 时 remove。无论 stream_loop
    // 因 EOF / 读错误 / connect 失败 / 正常返回哪条路径退出，下面都把 announced 里**仍存活**
    // 的 sid 一次性当 removed flush 出去 → lib.rs 的 remote-session-emitter emit SESSION_ENDED
    // → 对应远端 Tab 归档，不再永久卡在虚假 "live"。announced 每轮重连都新建（fresh per
    // iteration），故只归档本次连接残留。
    //
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
    loop {
        connected.store(false, Ordering::Release);
        // F05：本轮连接的起点。退避重置的判据是「活过多久」，不是「握没握上手」。
        let conn_started = std::time::Instant::now();
        // Batch9 账本：HashSet → HashMap<sid, AnnouncedMeta>（F27 status 写回 +
        // F28 frontend-ready 重发的数据源）。归档清算语义不变（keys = 存活 sid）。
        let mut announced: std::collections::HashMap<String, AnnouncedMeta> =
            std::collections::HashMap::new();
        // 🔴 `K-R59`（定框 `K35`）：这里原来是**顶层二选一** —— `cfg.daemonless` 为真时
        //    走纯 exec tail 轮询（`daemonless_stream_loop`），否则走后端流。
        //    那一档整个没了 ⇒ **只剩一条路**：连后端。
        let result = stream_loop(
            &cfg,
            &replay,
            &app,
            &session_changes,
            &connected,
            &mut announced,
            &mut hello_confirmed,
        )
        .await;
        if hello_confirmed.is_some() && !connected.load(Ordering::Acquire) {
            tracing::warn!("ssh_source hello 自愈轮未收到 hello,回退降级模式(backend 可能被换旧)");
            hello_confirmed = None;
        }
        // Batch9-F28：连接结束清本 host 的 registry（断连=骨架不该再被 F5 重建；
        // 重连宣告会重新填充）。
        announced_registry()
            .lock()
            .unwrap()
            .remove(&cfg.origin_label());
        // B2：断连也清本 host 的 tmux 状态——防重连后、backend 首个 TmuxSessions 帧到达前，
        // 对账 poller 读到陈旧 tmux 状态误灰（重连后由新帧重新填充）。
        forget_tmux_raw(&cfg.origin_label());
        // 每轮都归档本次连接残留的 announced sid（保持原 FIX 2 归档契约）+ audit-fixes F03.2：本 origin
        // 的 idle-tmux sid 也一并归档（断连=tmux 状态已清[上方 :1853]，idle 会话也该 archived；emitter
        // 处理这些 removed 时 tmux_raw 本 host 已空 → find_tmux_origin_for_sid=None → archived+clear_idle）。
        // **§24 单写者不破**：run() 只**读** snapshot_idle_for_origin，REMOTE_IDLE 的写（clear_idle）仍只在 emitter。
        let idle_here = snapshot_idle_for_origin(&cfg.origin_label());
        if !announced.is_empty() || !idle_here.is_empty() {
            let mut removed: Vec<String> = announced.into_keys().collect();
            removed.extend(idle_here);
            tracing::info!(
                "ssh_source connection ended; archiving {} remote session(s)",
                removed.len()
            );
            if let Err(e) = session_changes.send(SessionChange {
                added: vec![],
                // 连接断了兜底归档 = 真死（不是被顶替）。
                removed: removed.into_iter().map(RemovedSid::gone).collect(),
                status_changed: vec![], // 本分支无状态变化（F27 起 status 走 SessionAdded/SessionStatus 臂）
            }) {
                tracing::warn!("ssh_source final session archival send failed: {e}");
            }
        }
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
        tracing::info!("ssh_source reconnecting in {:?}", backoff);
        tokio::time::sleep(backoff).await;
        if !connected.load(Ordering::Acquire) {
            backoff = next_backoff(backoff); // 仍没连上 → 指数退避增长
        }
    }
}

/// Line 帧攒批缓冲（Batch5-F17）。
///
/// backend 线协议没有批量帧（一行一帧），首连 snapshot 的几千行历史若逐帧调
/// `on_line_batch(vec![1条])`，恒 1 < INCREMENTAL_BATCH_THRESHOLD → 全部走
/// 逐条 jsonl-line live 渲染管线（v2.4.2 给本地修掉的逐行刷屏在远端重现）。
/// 客户端把**连续到达**的 Line 帧聚合成批再交 on_line_batch：snapshot 密集
/// 连发天然聚成大批 → 自动跨过阈值复用本地 chunked 回放路径；日常单行增量
/// 只多一个静默窗口（~30ms）的延迟。时序判定（静默窗口）留在 stream_loop 的
/// `tokio::time::timeout` 里；本结构只管容量与顺序，纯逻辑可直测。
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

/// 攒批出口（Batch5-F17）：与本地 watcher 完全相同（batch_to_payloads →
/// on_line_batch），但用 **awaited 变体**——大批的块序列发完才返回，保证行
/// emit 严格先于随后的 SessionRemoved/断连归档（审计 R1：spawn 化的行若晚于
/// session-ended 到达前端，会把刚归档的远端 Tab 复活成僵尸 live），同时对
/// backend 帧流形成天然背压。
async fn flush_lines(
    replay: &Arc<EventReplay>,
    app: &tauri::AppHandle,
    host_label: &str,
    lines: Vec<JsonlLine>,
) {
    let flushed: Vec<(String, u64)> = lines
        .iter()
        .map(|l| (l.session_id.clone(), l.seq))
        .collect();
    // 〔ST3〕同一个 origin 既是载荷上的机器名、也是看不懂的行记账的那台。
    let origin = crate::origin::Origin(host_label.to_string());
    let payloads = crate::batch_to_payloads(lines, &origin);
    replay.on_line_batch_awaited(app, payloads).await;
    // 〔C2〕发出去了才推续点（连续才推，见 `snapshot_resume::note_flushed`）。
    crate::snapshot_resume::note_flushed(&origin, flushed.iter().map(|(s, q)| (s.as_str(), *q)));
}

/// [`run`] 的内层流循环：connect → exec backend → 逐帧 dispatch。**所有**提前返回
/// （`?` / EOF / 读错误）都把 result 冒泡给 [`run`]，由后者统一做最终 sid 归档 flush
/// （见 FIX 2 注释），故本函数自身不负责归档。
#[allow(clippy::too_many_arguments)]
async fn stream_loop(
    cfg: &RemoteConfig,
    replay: &Arc<EventReplay>,
    app: &tauri::AppHandle,
    session_changes: &std::sync::mpsc::Sender<SessionChange>,
    connected: &Arc<AtomicBool>,
    announced: &mut std::collections::HashMap<String, AnnouncedMeta>,
    hello_confirmed: &mut Option<Vec<String>>,
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

    // issue #29（F08）：连接前确保远端后端已（自动）部署到 cfg.backend_path。
    // 嵌入二进制就位前（F08b 未做）backend_binary() 返回 None → ensure_backend_deployed
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
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "ssh_source [{host_label}] backend 自动部署失败（继续尝试连接已有后端）: {e}"
                );
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
    let stream = match connect_and_exec(cfg, with_bg, tail_only, with_rbind_token).await {
        Ok(s) => s,
        Err(e) => {
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
    let (stream, parked) = crate::backend::control::inbound_client::split_and_park(stream);
    let mut parked = Some(parked);
    // 本连接的入方向客户端（收到 hello 后才有）。函数任何退出路径经 guard 摘除注册表
    // 并叫醒还在等应答的调用方 —— 同 `SnapshotQueueCloser` 的形状。
    let mut inbound: Option<
        std::sync::Arc<crate::backend::control::inbound_client::InboundClient>,
    > = None;
    struct InboundCloser(
        String,
        Option<std::sync::Arc<crate::backend::control::inbound_client::InboundClient>>,
    );
    impl Drop for InboundCloser {
        fn drop(&mut self) {
            if let Some(c) = self.1.take() {
                crate::backend::control::inbound_client::unregister(&self.0, &c);
            }
        }
    }
    let mut inbound_guard = InboundCloser(host_label.clone(), None);

    // Batch8-F26：旁路快照基础设施（仅 tail-only 生效；每连接一套，函数任何
    // 退出路径经 guard 关闭队列——已入队项仍会被分发器拉完，独立连接自灭）。
    let snapshots = SnapshotQueue::new();
    let _snapshots_guard = SnapshotQueueCloser(snapshots.clone());
    if tail_only {
        tauri::async_runtime::spawn(snapshot_dispatcher(
            snapshots.clone(),
            replay.clone(),
            app.clone(),
            host_label.clone(),
        ));
    }

    // Batch5-F17：帧读取挪进独立 task、经 channel 交回——攒批需要"带静默窗口
    // 的读"，而 tokio 的 read_line **不是 cancellation-safe**（timeout 取消会
    // 丢 buffer 里的半帧）；mpsc::Receiver::recv 是 cancel-safe 的，超时打在
    // recv 上帧零丢失。reader task 在 EOF/读错时投递 Err 后退出；本函数返回
    // （重连）时 rx drop → task 的 send 失败 → task 自然退出，不泄漏。
    let (frame_tx, mut frame_rx) = tokio::sync::mpsc::channel::<Result<String, String>>(1024);
    let reader_app = app.clone();
    let reader_host = host_label.clone();
    tauri::async_runtime::spawn(async move {
        let mut reader = BufReader::new(stream);
        // 按 `\n` 切（协议保证每帧一行、帧内换行已被后端转义成 `\n` 两字符，
        // 见 src/backend/wire.rs）。
        // ★ F10b：从无界 `read_line` 换成 [`read_capped_line`] —— 无界读遇「一条永远不结束
        // 的行」就是无界堆分配，而对端是**远端进程**（它坏掉或不是我们的后端都可能）。
        let mut buf: Vec<u8> = Vec::new();
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
                    // 超限语义 = **丢弃 + 带身份报告**，绝不静默（定框 E4）。
                    // ⚠ 走 REMOTE_HEALTH 而不是 frame_tx 的 Err 臂 —— Err 会被主循环
                    // 当成致命错误去重连，而超长行只是**这一行**坏了，连接本身没问题。
                    tracing::warn!(
                        "ssh_source remote [{reader_host}] line too long: {bytes} bytes \
                         (cap {BACKEND_FRAME_LINE_CAP}); line dropped"
                    );
                    let payload = crate::bridge::RemoteHealthPayload {
                        origin: reader_host.clone(),
                        kind: "line_too_long".to_string(),
                        message: line_too_long_health_message(&reader_host, bytes),
                    };
                    if let Err(e) = reader_app.emit(crate::bridge::events::REMOTE_HEALTH, payload) {
                        tracing::warn!("ssh_source line-too-long emit failed: {e}");
                    }
                }
                Ok(CappedLine::Line) => {
                    // 非 UTF-8 不该让整条连接死掉（与全批 exec 输出读取同一取舍）。
                    let text = String::from_utf8_lossy(&buf);
                    let line = text.trim_end_matches(['\n', '\r']);
                    if line.is_empty() {
                        continue;
                    }
                    if frame_tx.send(Ok(line.to_string())).await.is_err() {
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

    let mut batcher = Batcher::new(BATCH_CAP);
    // audit-fixes F03.2：收帧驱动的 tmux 存活收割器状态（跨帧累计缺失，随本连接存活；断连=函数返回、
    // 自然重置=清账）。取代已删的 8s poller（甲-evented 零轮询）。
    let mut reconcile_state = crate::tmux_reconcile::ReconcileState::default();
    // P8c：上一帧的观测分类（`None` = 还没收过帧）。**只用来判「变了没有」，不参与任何决策** ——
    // 决策仍然只看 `classify_tmux_observation` 的结果，这个字段是纯观测。
    let mut last_observation_kind: Option<String> = None;

    loop {
        // pending 非空 → 带静默窗口收帧：窗口内没有新帧就先 flush 再回到阻塞收。
        let msg = if batcher.pending.is_empty() {
            frame_rx.recv().await
        } else {
            match tokio::time::timeout(
                std::time::Duration::from_millis(BATCH_QUIET_MS),
                frame_rx.recv(),
            )
            .await
            {
                Ok(m) => m,
                Err(_) => {
                    if let Some(lines) = batcher.take() {
                        flush_lines(replay, app, &host_label, lines).await;
                    }
                    continue;
                }
            }
        };
        let Some(msg) = msg else {
            // reader task 没投 Err 就消失（理论不可达）——同样明确报错走重连。
            if let Some(lines) = batcher.take() {
                flush_lines(replay, app, &host_label, lines).await;
            }
            return Err("ssh backend frame channel closed".to_string());
        };
        let line = match msg {
            Ok(l) => l,
            Err(e) => {
                // EOF/读错：flush 残余（at-least-once 安全；重连会从 seq 0 重放，
                // 但没有理由主动丢已收到的行）**并等它发完**再报错——run() 随后的
                // 断连归档（announced 清算）必须晚于这些行到达前端（审计 R1）。
                if let Some(lines) = batcher.take() {
                    flush_lines(replay, app, &host_label, lines).await;
                }
                return Err(e);
            }
        };
        let line = line.as_str();

        let frame = parse_frame(line);
        // SessionRemoved 是唯一顺序敏感的攒批边界：它的行必须先落前端，否则
        // 归档后迟到的行把 Tab 复活成僵尸 live（审计 R1/R2）。SessionAdded /
        // Hello / Overflow / 坏帧**不再**作边界——多小会话的 snapshot 才能聚
        // 成大批跨过阈值（行先于 Added 到达无妨：前端 ensureTab 见行即建）。
        if matches!(frame, Some(InboundFrame::SessionRemoved { .. })) {
            if let Some(lines) = batcher.take() {
                flush_lines(replay, app, &host_label, lines).await;
            }
        }

        // U8a-2a：**握手完成 ⇒ 写半边解冻。** 放在 match 之前是因为 Hello 那条臂按值解构了帧。
        if let Some(client) = attach_inbound_client(&host_label, &mut parked, frame.as_ref()) {
            inbound_guard.1 = Some(client.clone());
            inbound = Some(client);
            // 〔`C1` · 09-24〕告诉前端「这台的长连接能问话了」：账号那两条查询从此走它，
            // 前端的账号刷新（替掉那个 10 秒轮询的事件驱动刷新器）在这一刻强制拉一次。
            if let Err(e) = app.emit(
                crate::bridge::events::REMOTE_BACKEND_READY,
                &serde_json::json!({ "origin": host_label }),
            ) {
                tracing::warn!("remote-backend-ready emit failed: {e}");
            }
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
                // issue #33：版本协商。不兼容/偏旧经 SS-F remote-health 通道醒目提示（前端
                // headlineFor 已含 version case，零前端改动）。不 hard-disconnect（向前兼容）。
                if let Some(msg) = version_warning(v, &build_id, &host_label) {
                    tracing::warn!("ssh_source remote [{host_label}] version: {msg}");
                    let payload = crate::bridge::RemoteHealthPayload {
                        origin: host_label.clone(),
                        kind: "version".to_string(),
                        message: msg,
                    };
                    if let Err(e) = app.emit(crate::bridge::events::REMOTE_HEALTH, payload) {
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
                    return Err(format!(
                        "backend hello 声明能力({capabilities:?})——重连升级流模式(tail-only/with-bg/with-rbind-token)"
                    ));
                }
                // ⚠ 「旧后端降级可见化」那一格**仍然**留在 `!tail_only` 里 —— 它问的是
                //   另一件事（「这台后端一条能力都没声明」），口径一个字没动。
                if !tail_only {
                    if capabilities.is_empty() {
                        let payload = crate::bridge::RemoteHealthPayload {
                            origin: host_label.clone(),
                            kind: "degraded".to_string(),
                            message: format!(
                                "远端后端为旧版本({build_id},当前 {EXPECTED_BACKEND_BUILD_ID}),本连接降级运行:后台(bg)会话不可见、历史全量推流(易拥塞)。请在设置里重装该机器的后端。"
                            ),
                        };
                        if let Err(e) = app.emit(crate::bridge::events::REMOTE_HEALTH, payload) {
                            tracing::warn!("ssh_source remote-health (degraded) emit failed: {e}");
                        }
                    }
                }
            }
            Some(InboundFrame::Line {
                session_id,
                path,
                seq,
                raw,
            }) => {
                // Batch5-F17：进攒批缓冲（达 cap/批龄立即整批出）；静默窗口/
                // SessionRemoved 边界触发的 flush 在循环头。
                if let Some(full) = batcher.push(JsonlLine {
                    session_id,
                    path: std::path::PathBuf::from(path),
                    seq,
                    raw,
                }) {
                    flush_lines(replay, app, &host_label, full).await;
                }
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
            }) => {
                // 🔴 〔`设计/80 §8.7` 步 4，第二波 T4〕**记进令牌账本 —— ↗ 从此按它分派。**
                //
                // 步 3 那一拍这里「只记一句账，一个分派都不改」（`§8.7` 逐字「不要先做 4」）。
                // 步 3 的本地半与它的生产写入方（`launch.rs` 的令牌握手前奏）都落了，
                // 「令牌还没有、判断已经改」那段窗口期不再存在 ⇒ 本拍把它接上：
                // `bind::bring_remote_front` 先查这本账，有令牌走 `token → HWND`，没有走标题退路。
                // `None` 也要记（= 删掉旧值）：重新宣告成「没令牌」时，上一次的令牌不许粘着。
                //
                // ⚠ **只打布尔，不打值**（`§8.6 ③`：令牌是敏感数据）。
                //   后端那一侧对同一条性质有判据钉着，这一侧由
                //   `ssh_source_parse_frame_tests.rs::the_token_value_never_reaches_a_log_macro` 钉。
                crate::bind::remote_rbind_tokens().note(&sid, rbind_token.as_deref());
                tracing::debug!(
                    "ssh_source [{host_label}] session_added sid={sid} has_rbind_token={}",
                    rbind_token.is_some()
                );
                // Batch5-F18：透传前端建骨架 Tab——协议序保证本帧先于该会话的
                // 内容行，这里同步 emit（先于行 flush），骨架必先于内容出现。
                let payload = crate::bridge::RemoteSessionAddedPayload {
                    session_id: sid.clone(),
                    origin: host_label.clone(),
                    kind: session_kind,
                    attachable,
                    cwd,
                    name,
                };
                // Batch9 审计 D：先入 registry 再 emit——反序时 F5 恰落在中间
                // 会直发丢失且 reannounce 读不到（亚毫秒缝，一并闭合）
                let meta_for_registry = AnnouncedMeta {
                    payload: payload.clone(),
                    status: status.clone(),
                    waiting_for: waiting_for.clone(),
                };
                announced_registry()
                    .lock()
                    .unwrap()
                    .entry(host_label.clone())
                    .or_default()
                    .insert(sid.clone(), meta_for_registry);
                // ★★ 〔`P0b` 08-13〕**这一跳此前是静默的** —— 只有 emit **失败**才打日志，
                // 成功一个字不留。于是全链台架报「30s 内未见灰灯 tab-state」时，
                // 日志**回答不了**最基本的那一问：**帧到 monitor 了吗？**
                //
                // 实测（连续两跑、四格自证全绿、读数逐字一致）：帧级套件 `graylight-backend-frames`
                // **12 过 / 0 败**（backend 那侧发得对），而全链 **1 过 / 2 败**、前端
                // `建卡 rendered=0 · drained=0`（一条会话载荷都没收到）⇒ 断点在这两者之间，
                // 而这里正是那段路上唯一的分叉点。
                //
                // ⚠ 射程：本行只说「**收到了、并已 emit**」。emit 之后前端有没有建出 tab
                // 是另一跳（那要看前端的 `[e2e] tab-state` 探针）。**别把它读成「tab 建出来了」。**
                // ⚠ 量级：`SessionAdded` 是**每个会话一次**，不是每帧一次 ⇒ 不会淹日志
                //（与 `tmux-observation` 那条只记变化的理由不同：那条是逐帧的）。
                tracing::info!("session-added: [{host_label}] sid={sid} → 已 emit 给前端");
                if let Err(e) = app.emit(crate::bridge::events::REMOTE_SESSION_ADDED, &payload) {
                    tracing::warn!("ssh_source remote-session-added emit failed: {e}");
                }
                // 〔U4b · 第四波〕容器事实交 `session_facts`（本机那条流同一个口、同一个事件）。
                //   排在 `remote-session-added` 之后：前端先建 tab、再落容器（早到的也有暂存兜着）。
                crate::session_facts::note_container(&sid, container);
                // Batch8-F26：tail-only 下历史改走旁路快照——宣告带 path 即入队
                // （无 path = 会话刚起还没写 jsonl → 无历史可拉，后续行天然从
                // tail 全量到达，无需快照）。队列按 sid 幂等（重复宣告不重拉）。
                if tail_only {
                    if let Some(p) = path {
                        snapshots.push(SnapshotItem {
                            sid: sid.clone(),
                            path: p,
                            expected_lines: lines,
                        });
                    }
                }
                // FIX 2：记下已宣告的 sid + 元数据（Batch9：连接结束统一归档 +
                // F28 frontend-ready 重发数据源；F27 status 后续变化写回）。
                announced.insert(
                    sid.clone(),
                    AnnouncedMeta {
                        payload: payload.clone(),
                        status: status.clone(),
                        waiting_for: waiting_for.clone(),
                    },
                );
                // Batch9-F27：初始 status 一并透传（连接建立灯就对；None=旧 CC/
                // 旧 backend → 前端"未知不加类"，与本地一字一致）。
                if let Err(e) = session_changes.send(SessionChange {
                    added: vec![sid.clone()],
                    removed: vec![],
                    status_changed: vec![crate::session_map::SessionActivity {
                        session_id: sid,
                        status,
                        waiting_for,
                    }],
                }) {
                    tracing::warn!("ssh_source session_added send failed: {e}");
                }
            }
            Some(InboundFrame::SessionStatus {
                sid,
                status,
                waiting_for,
            }) => {
                // Batch9-F27：写回 announced（F28 重发时灯是最新的）+ 透传前端。
                if let Some(meta) = announced.get_mut(&sid) {
                    meta.status = status.clone();
                    meta.waiting_for = waiting_for.clone();
                }
                if let Some(hm) = announced_registry().lock().unwrap().get_mut(&host_label) {
                    if let Some(meta) = hm.get_mut(&sid) {
                        meta.status = status.clone();
                        meta.waiting_for = waiting_for.clone();
                    }
                }
                // ★★〔08-14 实机排障补〕**这一跳与 `session_removed` 那条是同一个盲区**，
                // 而那条已经补过、理由逐字写在下面（「分不清『收到了但没转发』与『根本没收到』」）。
                // 本条当时漏了，代价在 08-14 兑现：用户报「Windows 前端上会话全是绿灯」，
                // 而**绿灯正是 `status` 缺席时的默认值**（`src/session-status.ts` 逐字：
                // `busy` / `null` → 默认绿点）⇒ 「全绿」既可能是"都在忙"，也可能是
                // "status 一条都没到"，**两者在日志里长得一模一样**，排障当场卡死在这里。
                // ⚠ 量级与那条一致：**每次状态变化一行**（CC 仅在状态转换时重写 pidfile，
                // 天然稀疏），不是每帧一行 ⇒ 不会淹日志。
                tracing::info!(
                    "session-status: [{host_label}] sid={sid} status={status:?} \
                     waiting_for={waiting_for:?} → 已 emit 给前端"
                );
                if let Err(e) = session_changes.send(SessionChange {
                    added: vec![],
                    removed: vec![],
                    status_changed: vec![crate::session_map::SessionActivity {
                        session_id: sid,
                        status,
                        waiting_for,
                    }],
                }) {
                    tracing::warn!("ssh_source session_status send failed: {e}");
                }
            }
            Some(InboundFrame::SessionRemoved { sid, cause }) => {
                // ★★ `P0b-Y2` 第十拍〔08-13〕：**这一跳原先是不可观测的。**
                //
                // `#60`（灰灯不出现）问的正是「死亡这件事走到哪一步丢了」，而全链实测时
                // backend 侧 tap 里明明有 `session_removed`、monitor 日志里**一个字都没有**
                // ⇒ 分不清「收到了但没转发」与「根本没收到」。加了这一行才分得清。
                // ⚠ 量级同 `SessionAdded` 那条：**每个会话一次**，不是每帧一次 ⇒ 不会淹日志。
                // `cause` 一起打：灰灯与归档走的是**不同的 cause**（`Superseded` 直接归档），
                // 少了它，看见一行「removed」仍答不出 UI 该变成什么样。
                tracing::info!(
                    "session-removed: [{host_label}] sid={sid} cause={cause:?} → 已 emit 给前端"
                );
                // FIX 2：已显式 removed 的 sid 从 announced 摘掉，避免连接结束时重复归档。
                announced.remove(&sid);
                if let Some(hm) = announced_registry().lock().unwrap().get_mut(&host_label) {
                    hm.remove(&sid);
                }
                // Batch8 D-B1：摘除排队中的快照 + 标记 inflight 取消——归档后
                // 迟到的快照行会经"见行复活"造出关不掉的僵尸 live tab。
                snapshots.cancel(&sid);
                // 〔C2〕会话真结束 ⇒ 续点作废（再宣告时整份拉，与今天同）。
                crate::snapshot_resume::forget(&crate::origin::Origin(host_label.clone()), &sid);
                if let Err(e) = session_changes.send(SessionChange {
                    added: vec![],
                    // ★ S0：cause 由后端说了算，monitor 不猜（原先靠查会陈旧的 tmux 快照）。
                    removed: vec![RemovedSid { sid, cause }],
                    status_changed: vec![],
                }) {
                    tracing::warn!("ssh_source session_removed send failed: {e}");
                }
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
                let payload = crate::bridge::RemoteHealthPayload {
                    origin: host_label.clone(),
                    kind: "overflow".to_string(),
                    message,
                };
                if let Err(e) = app.emit(crate::bridge::events::REMOTE_HEALTH, payload) {
                    tracing::warn!("ssh_source remote-health emit failed: {e}");
                }
            }
            // P5：正向死亡帧 —— backend 已经**确定**这个会话没了（它与上一份快照差分算出来的），
            // 不需要 monitor 再靠「连续两次没看见」去猜。
            //
            // **为什么仍然按名字反查 sid 而不是让后端带上**：`#{@ccm_sid}` 在 hook 上下文
            // 取不到（P0 实测拿到空 ⇒ 会把活会话判灰）；而 name→sid 的映射 monitor 这边本来
            // 就有（最新那份 `tmux ls` 原文）。让知道的人去查，比让不知道的人硬传更稳。
            //
            // **快照路径与 `RETIRE_MISS_THRESHOLD` 原样保留**：重同步 / 旧后端降级都靠它。
            // 同一 sid 两条路都可能到 ⇒ retire 必须幂等（`SidTrack.retired` 本就是）。
            Some(InboundFrame::TmuxSessionClosed { name }) => {
                let sid = {
                    let reg = tmux_raw_registry().lock().unwrap();
                    reg.get(&host_label).and_then(|raw| {
                        crate::backend::control::tmux::parse_tmux_ls(raw)
                            .into_iter()
                            .find(|e| e.name == name)
                            .and_then(|e| e.sid)
                    })
                };
                match sid {
                    Some(sid) => {
                        // ★★ `P0b-Y2` 第十七拍：**先把这一格从账本摘掉，再 retire。**
                        //   顺序反了的话下游 `classify_removed` 查到的还是「tmux 还在」
                        //   ⇒ 判灰不判归档 ⇒ **永久灰点**（`#60` 现象 2，08-13 全链实测）。
                        // ⚠ **走既有写口 `record_tmux_raw`，不新开第二个** ——
                        //   `the_tmux_cache_has_one_writer_and_only_origin_keys` 只许两处写口，
                        //   而它报得对：账本一旦有第三个写点，「一边存原文一边存解析后的、
                        //   一边清一边不清」就会长出来（首版就是内联 `get_mut`，当场被拦）。
                        // ⚠ 读-改-写不是原子的：并发的整份快照更新可能覆盖这次摘除。
                        //   那是**良性**的 —— 下一份快照本来就是权威，它会说出同样的事实。
                        if let Some(raw) = snapshot_tmux_by_origin().get(&host_label) {
                            record_tmux_raw(&host_label, remove_tmux_line(raw, &name));
                        }
                        tracing::info!(
                            "tmux 会话 {name} 关闭（backend 死亡帧）⇒ 已从账本摘除 + 立刻 retire sid={sid}"
                        );
                        if let Err(e) = session_changes.send(SessionChange {
                            added: vec![],
                            // tmux 会话关了 = 那一格没了，真死。
                            removed: vec![RemovedSid::gone(sid)],
                            status_changed: vec![],
                        }) {
                            tracing::warn!("ssh_source tmux_session_closed send failed: {e}");
                        }
                    }
                    // 查不到 sid 的两种正常情形：① 那个会话本就没绑过 `@ccm_sid`
                    //（never-bound：bg / 直起 claude）② 快照还没到过。两者都**不该猜**——
                    // 交给快照 + miss 那条兜底路，它对 never-bound 有专门的不误判逻辑。
                    None => tracing::debug!(
                        "tmux 会话 {name} 关闭，但最新快照里查不到它的 sid ⇒ 交给对账兜底"
                    ),
                }
            }
            Some(InboundFrame::TmuxSessions { raw, observation }) => {
                // audit-fixes F03.2（收帧驱动 tmux 存活收割器，取代已删的 8s poller = 甲-evented 零轮询）：
                // backend **事件驱动**推本 origin 最新 `tmux ls` 原文（tmux hook → SIGUSR1 → Poke）；收到即对账——把 tmux 后端已消失的 tracked
                // sid 去抖 retire → 当 removed 送 emitter（emitter 再判 None→archived+clear_idle）。tracked =
                // 本连接 announced（live 会话）∪ 本 origin idle 会话（后者使 idle→archived 有产出者，补齐红线④）。
                //
                // P1（zero-poll-liveness）：原先这里内联着
                // `if raw.trim() != "NO_TMUX" { … if !backend.is_empty() { … } }`——**把五种语义
                // 不同的观测压成两条路**，其中「backend 确证零会话」被误并进「观测失败」一律跳过
                // ⇒ 杀掉某 origin 最后一个 tmux 会话时灰灯卡到断连（§24bis 预登记的残留 bug）。
                // 现在判断提成纯函数 `tmux::classify_tmux_observation`（可 CI 单测，生产与测试
                // 同一条路径），空集也是**有效观测**、照常累计缺失。
                // ★★ P8c（`U3`〔自批 08-11〕的裁定）：**把这一维记进日志，让它变得可测**。
                //
                // `U3` 的读数逐字：「`Unobservable` 计数 = 0，而**那个 0 是瞎的** ——
                // 日志根本不记这一维 ⇒ 分母不存在。『0 次』与『记不下来』在这份数据里
                // 长得一模一样，而后者才是事实」。⇒ 裁定是「先补一行可观测性，再拿真实使用量去裁 `#82`」。
                //
                // ⚠ **只记「变化」，不是每帧都记**：帧由 tmux hook 驱动，逐帧记会把日志淹掉
                // （而淹掉的日志与没有日志一样不可读）。记变化反而更有用 —— 它给的是
                // 「何时进入不可观测、何时恢复」，那比一个计数更能回答 `#82`。
                //
                // ⚠⚠ **射程如实登记**〔D 阶段补审〕：这条日志给的是**区间**，不是**帧计数**。
                // 要问「多大比例的帧是 unobservable」得再加一个计数器 —— 本件**不做**，
                // 因为 `U3` 要的是「让这个数变得可测」，而区间对 `#82`（控制模式值不值得做）
                // 更直接：它回答的是「不可观测**持续了多久**」。
                // **别把本件读成「频率已经可测了」。**
                let verdict = crate::backend::control::tmux::classify_tmux_observation(
                    &raw,
                    observation.as_deref(),
                );
                let kind = match &verdict {
                    crate::backend::control::tmux::TmuxObservation::Backend(_) => "backend",
                    crate::backend::control::tmux::TmuxObservation::Skip(r) => r.as_str(),
                };
                if last_observation_kind.as_deref() != Some(kind) {
                    tracing::info!(
                        "tmux-observation: [{host_label}] {} → {kind}",
                        last_observation_kind.as_deref().unwrap_or("<首帧>")
                    );
                    last_observation_kind = Some(kind.to_string());
                }
                if let crate::backend::control::tmux::TmuxObservation::Backend(backend) = verdict {
                    let idle = snapshot_idle_for_origin(&host_label);
                    let tracked = reaper_tracked(announced.keys().cloned(), &idle);
                    // idle 集当 pre_bound 传入：@ccm_sid 证明绑过 tmux，播种 ever_bound，免跨线程
                    // 缝漏置导致 idle→archived 无产出者（D 审计②修，见 reconcile_step 注释）。
                    let retire = crate::tmux_reconcile::reconcile_step(
                        &mut reconcile_state,
                        &tracked,
                        &backend,
                        &idle,
                        crate::tmux_reconcile::RETIRE_MISS_THRESHOLD,
                    );
                    if !retire.is_empty() {
                        tracing::info!(
                            "tmux-reconcile(收帧): [{host_label}] retire {} sid(s)（tmux 后端已不在）",
                            retire.len()
                        );
                        if let Err(e) = session_changes.send(SessionChange {
                            added: vec![],
                            // 对账收割 = tmux 后端已不见它，真死。
                            removed: retire.into_iter().map(RemovedSid::gone).collect(),
                            status_changed: vec![],
                        }) {
                            tracing::warn!("ssh_source tmux-reconcile(收帧) send failed: {e}");
                        }
                    }
                }
                // 存最新一份原文（emitter 判 idle/archived 时经 snapshot_tmux_by_origin 读；仅存最新）。
                record_tmux_raw(&host_label, raw);
            }
            // U8a-2a：入方向应答 —— 交给本连接的客户端按 `id` 路由回请求方。
            Some(f @ (InboundFrame::Reply { .. } | InboundFrame::Cancelled { .. })) => {
                route_inbound_frame(&host_label, inbound.as_ref(), f);
            }
            // 〔SR1a · `设计/05 §13.6 ③`〕那台的账号清单变了 ⇒ 发前端既有的「这台就绪」那一个事件
            //   （账号表与 chip 听的就是它，`main.ts`），多带一个 `reason` 说清这一次为什么（additive）。
            Some(InboundFrame::AccountsChanged) => {
                if let Err(e) = app.emit(
                    crate::bridge::events::REMOTE_BACKEND_READY,
                    &serde_json::json!({ "origin": host_label, "reason": "accounts_changed" }),
                ) {
                    tracing::warn!("remote-backend-ready（accounts_changed）emit failed: {e}");
                }
            }
            // 〔U4b · 第四波〕那台的活会话清单报完了 ⇒ 发前端 `origin-sessions-listed`。
            //   与上面 `remote-session-added` 同一条线程、同序 emit ⇒ 前端收到它时，这台全部的活会话都已宣告过。
            //   前端据此把这台「固定、却没被报过」的 tab 从说不清落到已结束（`设计/30 §3.5.7a`）。
            Some(InboundFrame::SessionsReplayed) => {
                tracing::info!(
                    "sessions-replayed: [{host_label}] 活会话清单报完了 → 已 emit 给前端"
                );
                if let Err(e) = app.emit(
                    crate::bridge::events::ORIGIN_SESSIONS_LISTED,
                    &crate::bridge::OriginSessionsListedPayload {
                        origin: crate::origin::Origin(host_label.clone()),
                    },
                ) {
                    tracing::warn!("origin-sessions-listed emit failed: {e}");
                }
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
            None => {
                // 未知 kind / 坏帧 / 非 JSON：跳过，绝不 panic、绝不中断流。
                tracing::warn!("ssh_source skipping unparseable/unknown frame: {line}");
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

#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_f032_idle_tests.rs"]
mod f032_idle_tests;

// ============================================================================
// Tier 1 SSH 连接 UX（issue #15）：~/.ssh/config 导入 + 测试连接 + 指纹固化。
// 下列 #[tauri::command] 在 lib.rs 的 invoke_handler! 里注册（漏注册=运行时
// "command not found"，非编译错，已 double-check）。
// ============================================================================

/// `~/.ssh/config` 解析出的一个 host 的有效连接参数（`resolve_ssh_host` 的产物）。
///
/// serde camelCase：host / port / user / keyPath，与前端 fill 逻辑对齐。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ResolvedHost {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// 第一个**存在**的 IdentityFile（`~` 已展开）。无则 None（用户可改走 agent）。
    pub key_path: Option<String>,
    /// Batch14-F57：`ssh -G` 的 `proxyjump`（"none" → None）。批量导入时映射到 RemoteConfig.jump。
    pub proxy_jump: Option<String>,
}

/// 列出 `~/.ssh/config` 里的 host 别名（供前端「从 config 导入」下拉）。
///
/// 解析规则（保守、宽松）：
/// - 逐行扫，匹配以 `Host`（大小写不敏感）开头的指令行；其后所有 token 都是别名。
/// - **排除** 含通配符的 pattern：包含 `*` 或 `?` 的 token，以及字面量 `*`。
/// - 去重、保留首次出现顺序。
/// - 文件不存在 → 返回空 Vec（不是错误：用户没有 config 是正常的）。
///
/// 不展开 `Include`、不解析 `Match`（Tier 1 只要给用户一份「可点的别名清单」，真正的
/// 参数解析交给 `ssh -G`，它会完整处理 Include/Match/通配）。
#[tauri::command]
pub async fn list_ssh_host_aliases() -> Result<Vec<String>, String> {
    // FIX 3（INVARIANT §10）：同步 fs 读也别卡 runtime 线程——挪进 spawn_blocking。
    tokio::task::spawn_blocking(|| {
        let path = match dirs::home_dir() {
            Some(h) => h.join(".ssh").join("config"),
            None => return Vec::new(),
        };
        match std::fs::read_to_string(&path) {
            Ok(content) => parse_host_aliases(&content),
            // 文件不存在 / 读不了 → 空列表（前端据此提示「没有可导入的别名」）。
            Err(_) => Vec::new(),
        }
    })
    .await
    .map_err(|e| format!("读取 ~/.ssh/config 任务调度失败: {e}"))
}

/// 从 `~/.ssh/config` 文本里抽出非通配的 host 别名（纯函数，便于单测）。
/// 规则见 [`list_ssh_host_aliases`] 文档。
fn parse_host_aliases(content: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut aliases = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // `Host` 指令：关键字大小写不敏感，关键字与别名间可用空白或 `=` 分隔。
        let mut parts = trimmed.splitn(2, |c: char| c.is_whitespace() || c == '=');
        let keyword = parts.next().unwrap_or("");
        if !keyword.eq_ignore_ascii_case("host") {
            continue;
        }
        let rest = parts.next().unwrap_or("").trim();
        for tok in rest.split_whitespace() {
            // 排除通配 pattern（`*` / `?`）和反向否定 pattern（`!...`）。
            if tok.contains('*') || tok.contains('?') || tok.starts_with('!') {
                continue;
            }
            if seen.insert(tok.to_string()) {
                aliases.push(tok.to_string());
            }
        }
    }
    aliases
}

/// 别名 allowlist：只允许 host 别名的安全字符，挡住 ssh 选项/参数注入
/// （别名里不会有空格 / `-` 开头会被下面单独防、`=` 等危险字符直接拒）。
fn is_safe_alias(alias: &str) -> bool {
    !alias.is_empty()
        && alias
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '@' | ':' | '-'))
}

/// 把以 `~` 开头的路径展开为绝对路径（`~` / `~/...`）。其余原样返回。
fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
            if let Some(home) = dirs::home_dir() {
                let rest = rest.trim_start_matches(['/', '\\']);
                return if rest.is_empty() {
                    home
                } else {
                    home.join(rest)
                };
            }
        }
    }
    std::path::PathBuf::from(path)
}

/// 用系统 `ssh -G <alias>` 解析一个别名的有效连接参数（OpenSSH client 在 Win10+/Win11
/// 自带）。`ssh -G` 会完整处理 Include / Match / 通配 / 默认值，比自己解析 config 可靠得多。
///
/// **注入防护**：别名先过 [`is_safe_alias`] allowlist（`^[A-Za-z0-9._@:-]+$`），再额外挡掉
/// 以 `-` 开头的值（否则会被 ssh 当成选项）。参数以独立 arg 传给 Command（不经 shell），
/// 配合 allowlist 杜绝 arg/option 注入。
///
/// 解析 stdout（每行 `key value`，key 小写）：
/// - `hostname <X>` → host
/// - `port <N>`     → port（u16）
/// - `user <U>`     → user
/// - 第一个**展开后存在**的 `identityfile <path>` → key_path（None = 都不存在）
#[tauri::command]
pub async fn resolve_ssh_host(alias: String) -> Result<ResolvedHost, String> {
    let alias = alias.trim().to_string();
    if !is_safe_alias(&alias) {
        return Err(format!("非法的 host 别名（含不安全字符）: {alias}"));
    }
    if alias.starts_with('-') {
        return Err("host 别名不能以 '-' 开头".to_string());
    }

    // FIX 3（INVARIANT §10）：`Command::output()` 是同步阻塞调用，直接在 async fn 里跑会
    // 卡住 tokio runtime 线程。挪进 spawn_blocking（allowlist 守卫已在上方先过，不进线程池）。
    let alias_for_exec = alias.clone();
    let output = tokio::task::spawn_blocking(move || {
        use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
        let mut cmd = std::process::Command::new("ssh");
        cmd.arg("-G")
            .arg(&alias_for_exec)
            .stdout(std::process::Stdio::piped());
        // 三条策略（`00 §1.5.2`）：
        // · `Hidden` —— 🔴 **先前是裸 `.output()`**：Windows 上 `ssh.exe` 是控制台子系统，
        //   而 monitor 没有控制台 ⇒ 每解析一次别名闪一个黑框。
        // · `JobKillOnClose` —— 一次性、就地等；`ssh -G` 只读配置不建连接，不该留后代。
        // · `Captured` —— stderr **是返回值**（下面 `退出非 0: {stderr}` 逐字要用它）。
        spawn_managed_cmd(
            &mut cmd,
            ConsolePolicy::Hidden,
            Lifetime::JobKillOnClose,
            StderrSink::Captured,
        )
        .and_then(|c| c.wait_with_output())
    })
    .await
    .map_err(|e| format!("ssh -G 任务调度失败: {e}"))?
    .map_err(|e| format!("运行 ssh -G 失败（OpenSSH client 装了吗？）: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ssh -G {alias} 退出非 0: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_ssh_g_output(&stdout, &alias))
}

/// F57：解析 `ssh -G <alias>` 的输出为 ResolvedHost（纯逻辑,便于单测）。识别 hostname/port/
/// user/identityfile（第一个**展开后存在**的）/proxyjump（`none` → None）。hostname 缺省回退别名。
/// identityfile 的「存在性」检查依赖文件系统,单测里非存在路径 → key_path=None,不影响其余字段判定。
fn parse_ssh_g_output(stdout: &str, alias: &str) -> ResolvedHost {
    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut user: Option<String> = None;
    let mut key_path: Option<String> = None;
    let mut proxy_jump: Option<String> = None;

    for line in stdout.lines() {
        let mut it = line.splitn(2, char::is_whitespace);
        let key = it.next().unwrap_or("").trim().to_ascii_lowercase();
        let val = it.next().unwrap_or("").trim();
        if val.is_empty() {
            continue;
        }
        match key.as_str() {
            "hostname" => host = Some(val.to_string()),
            "port" => port = val.parse::<u16>().ok(),
            "user" => user = Some(val.to_string()),
            "identityfile" if key_path.is_none() => {
                // 第一个**展开后存在**的 identityfile 才采用。
                let expanded = expand_tilde(val);
                if expanded.exists() {
                    key_path = Some(expanded.to_string_lossy().to_string());
                }
            }
            // F57：proxyjump 值通常是另一别名（`none` = 无跳板）。
            "proxyjump" if !val.eq_ignore_ascii_case("none") => {
                proxy_jump = Some(val.to_string());
            }
            _ => {}
        }
    }

    ResolvedHost {
        // hostname 缺省回退到别名本身（ssh -G 通常总会给 hostname，但稳妥兜底）。
        host: host.unwrap_or_else(|| alias.to_string()),
        port: port.unwrap_or(22),
        user: user.unwrap_or_default(),
        key_path,
        proxy_jump,
    }
}

/// F57：一个来源别名 + 其 HostName/port/proxyjump（供前端「拆分」精确还原成独立机）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ImportMember {
    pub alias: String,
    pub host: String,
    pub port: u16,
    pub proxy_jump: Option<String>,
}

/// F57：批量导入预览的一组——聚合后的一台建议主机（含多地址与来源成员）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ImportGroup {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub key_path: Option<String>,
    /// F45 备用地址：组内除 host 外的其余 HostName（去重）。单机组为空。
    pub addresses: Vec<String>,
    /// F56 跳板：组内首个非空 proxyjump（别名）。
    pub jump: Option<String>,
    /// 来源成员（别名+HostName）：预览展示；用户「拆分」时据此建独立机。
    pub members: Vec<ImportMember>,
}

/// F57：别名基名前缀——首个 `-`/`_`/`.` 前的段（`devbox-lan` → `devbox`；`pi` → `pi`）。同机聚合判据之一。
fn alias_base(alias: &str) -> String {
    alias
        .split(|c| c == '-' || c == '_' || c == '.')
        .next()
        .unwrap_or(alias)
        .to_string()
}

/// F57：智能聚合——同 `(keyPath, user, 基名前缀)` 的多别名判为**同一台机的多地址**，聚合成 1 组
/// （host=首 HostName、addresses=其余 HostName 去重、label=基名、jump=组内首个 proxyjump）；否则各自
/// 独立。保序（按别名首次出现）。纯函数便于单测；聚合激进/保守由前端预览「拆分/勾选」兜底。
pub fn aggregate_ssh_hosts(hosts: Vec<(String, ResolvedHost)>) -> Vec<ImportGroup> {
    type GKey = (Option<String>, String, String); // (keyPath, user, base)
    let mut groups: Vec<(GKey, ImportGroup)> = Vec::new();
    for (alias, r) in hosts {
        let gkey: GKey = (r.key_path.clone(), r.user.clone(), alias_base(&alias));
        let member = ImportMember {
            alias,
            host: r.host.clone(),
            port: r.port,
            proxy_jump: r.proxy_jump.clone(),
        };
        if let Some((k, g)) = groups.iter_mut().find(|(k, _)| *k == gkey) {
            // 副地址：端口与组首端口不同则存 `host:port`（F45 支持），否则裸 host；去重。
            let addr = if r.port == g.port {
                r.host.clone()
            } else {
                format!("{}:{}", r.host, r.port)
            };
            if r.host != g.host && !g.addresses.contains(&addr) {
                g.addresses.push(addr);
            }
            g.members.push(member);
            if g.jump.is_none() {
                g.jump = r.proxy_jump.clone();
            }
            let _ = k;
        } else {
            groups.push((
                gkey,
                ImportGroup {
                    label: alias_base(&member.alias),
                    host: r.host.clone(),
                    port: r.port,
                    user: r.user.clone(),
                    key_path: r.key_path.clone(),
                    addresses: Vec::new(),
                    jump: r.proxy_jump.clone(),
                    members: vec![member],
                },
            ));
        }
    }
    // F57-1（D 修）：单成员组用**完整别名**当 label（否则 `prod-web`/`prod-db` 异 key 拆成两组却都叫
    // `prod`，前端落卡时同名碰撞会丢一台）；仅真·多地址聚合组用基名前缀。
    let mut out: Vec<ImportGroup> = groups.into_iter().map(|(_, g)| g).collect();
    for g in &mut out {
        if g.members.len() == 1 {
            g.label = g.members[0].alias.clone();
        }
    }
    out
}

/// F57：批量从 `~/.ssh/config` 导入——列全部别名、逐个 `ssh -G` 解析（保序）、智能聚合成预览组。
/// resolve 失败的别名跳过（best-effort）；无 config/无别名 → 空。前端弹预览可拆分/勾选再落。
#[tauri::command]
pub async fn import_ssh_hosts() -> Result<Vec<ImportGroup>, String> {
    let aliases = list_ssh_host_aliases().await?;
    let mut resolved: Vec<(String, ResolvedHost)> = Vec::new();
    for alias in aliases {
        // 顺序 resolve 保序（聚合按别名首次出现）；ssh -G 快（本地 config 解析），别名数通常个位。
        if let Ok(r) = resolve_ssh_host(alias.clone()).await {
            resolved.push((alias, r));
        }
    }
    Ok(aggregate_ssh_hosts(resolved))
}

/// 「测试连接」的结果（issue #15 Part 2）。serde camelCase 与前端渲染对齐。
///
/// 偏好「返回 populated 结果 + message」而非 Err：让 UI 能展示部分成功
/// （如「SSH 连上了，但后端没响应/未部署」）。仅参数级硬错误才返回 Err。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ConnTestResult {
    /// SSH 连接 + 鉴权是否成功。
    pub ssh_ok: bool,
    /// 握手时观察到的 server host key 指纹（`SHA256:...`）。用于展示 + TOFU 固化。
    pub fingerprint: Option<String>,
    /// F45 / D 审计重要-1：竞发胜出的地址（`host:port`）。多地址 TOFU 首连时,让用户明确
    /// 自己正在固化**哪条路径**观察到的指纹（而非盲信「最快那条」）。单地址时即该地址。
    pub endpoint: Option<String>,
    /// backend 是否在 SHORT timeout 内回了可解析的 hello 帧。
    pub backend_ok: bool,
    /// backend hello 的人读摘要（`v=.. arch=.. claude_home=..`）。
    /// ⚠ `S4` 起 `claude_home` 是**解析后**的值（优先 `homes`、回退 `claude_dir`），
    /// 不是某个线上字段的原样照抄。
    pub backend_hello: Option<String>,
    /// 人读的总体状态 / 失败原因。
    pub message: String,
}

/// 测试一条远端配置：连 SSH → 读指纹 → exec backend → 等首行 hello（SHORT timeout）。
///
/// 步骤化、每步把结果填进 [`ConnTestResult`]：
/// 1. `connect_session`（publickey 或 agent）。失败 → ssh_ok=false + message，返回 Ok。
/// 2. 成功 → ssh_ok=true，从 observed cell 读指纹。
/// 3. channel_open + exec backend + 读首行 stdout（`tokio::time::timeout` 8s）。
///    解析成 hello → backend_ok=true + 摘要；否则 message 说明后端未响应/未部署。
///
/// 只有"无法构造测试"这类硬错才返回 Err；连接/鉴权/backend 失败都收进结果里，UI 据此分级展示。
#[tauri::command]
pub async fn test_remote_connection(
    cfg: RemoteConfig,
    on_stage: tauri::ipc::Channel<ConnectStage>,
) -> Result<ConnTestResult, String> {
    let mut result = ConnTestResult {
        ssh_ok: false,
        fingerprint: None,
        endpoint: None,
        backend_ok: false,
        backend_hello: None,
        message: String::new(),
    };

    // 1. 连接 + 鉴权 + exec backend（短命探活：测试连接不需要长保活）。F46：阶段行由拨号代理逐行报，
    //    这里原样转给前端「连接过程」日志（〔C2〕此前只有进程内那条路有阶段；拨号搬进后端之后只有这一个来源）。
    let mut to_ui = |s: ConnectStage| {
        if let Err(e) = on_stage.send(s) {
            tracing::warn!("connect stage emit failed: {e}");
        }
    };
    let (link, ack) = match crate::dial_host::probe(&cfg, &cfg.backend_path, &mut to_ui).await {
        Ok(v) => v,
        Err((e, _seen_fingerprint)) => {
            // 握手失败（含 host key 不匹配被拒）。代理那侧看到过的指纹**刻意不回给前端**：
            // 失败时给一个指纹，前端那条「固化指纹」的路就可能把一把失配的 key 固化进去（与改动前同一个取舍）。
            result.ssh_ok = false;
            result.message = format!("SSH 连接/鉴权失败：{e}");
            return Ok(result);
        }
    };
    result.ssh_ok = true;
    result.fingerprint = ack.fingerprint.clone();
    // 竞速胜出的地址（代理 ack 带回来，宿主已记成 last-good）：回传给前端展示「你正连上/将固化哪个地址」。
    let win = winner_address(&cfg);
    result.endpoint = Some(
        ack.endpoint
            .clone()
            .unwrap_or_else(|| format!("{}:{}", win.host, win.port)),
    );

    // 2. 读后端首行 hello ＋ 探一次控制通道（链路里跑的就是 `backend_path`）。
    match probe_backend(link).await {
        Ok(Some(probe)) => {
            result.backend_ok = true;
            result.message = if probe.control_ok {
                "SSH 与后端均正常（含控制通道往返）。".to_string()
            } else if probe.control_unsupported {
                "SSH 与后端正常，但该 backend **不支持控制通道**（旧版本）——                 远端起会话等功能不可用，请在设置里重装该机器的后端。"
                    .to_string()
            } else {
                // 控制通道真失败：**不许报「均正常」**。这一步就是为了让它在这里显形。
                "SSH 与后端正常，但**控制通道不通**（详见下方摘要的 control=… 段）——                 远端起会话会失败。"
                    .to_string()
            };
            result.backend_hello = Some(probe.summary);
        }
        Ok(None) => {
            result.message =
                "SSH 连上了，但后端在超时内未回 hello（未部署 / 路径错 / 启动失败？）。"
                    .to_string();
        }
        Err(e) => {
            result.message = format!("SSH 连上了，但后端探测失败：{e}");
        }
    }

    // 链路在 `probe_backend` 里用完即丢 ⇒ 后端收掉它；测试连接的链路不进连接池 ⇒ 那条 SSH 连接随之关。
    Ok(result)
}

/// [`probe_backend`] 的结果。**不是一个摘要串** —— 顶层结论要能分辨「控制通道通不通」，
/// 否则 D 审计点名的那件事会再发生一次：控制通道不通时仍然报「SSH 与后端均正常」，
/// 失败只藏在括号里，而这一步的立项理由恰恰是「别让用户在全绿之后才发现起不了会话」。
struct BackendProbe {
    /// 人读摘要（进 `ConnTestResult::backend_hello`）。
    summary: String,
    /// 控制通道 ping 往返成功。
    control_ok: bool,
    /// 旧后端：没声明任何入方向命令（不是失败，是能力缺失）。
    control_unsupported: bool,
}

/// exec backend、读首行 stdout、若是 hello 帧再**探一次控制通道往返**。
///
/// 三步（第三步是 U8a-2a 新增）：
///
/// 1. exec + 读首行，超时 8s；
/// 2. 解析成 `hello` 帧 → 人读摘要；
/// 3. 用同一条 channel 发一条 `ping` 等应答，超时 [`CONTROL_PROBE_TIMEOUT`]。
///
/// **时间预算是两段串联**：最坏 8s + 5s = 13s，只在「backend 发了 hello、声明了 `ping`、
/// 却不回应答」时吃满；旧后端（未声明入方向）走 `control=unsupported` 立即返回。
///
/// 返回：
///
/// - `Ok(Some(probe))` —— 读到并解析成 hello（`probe.control_*` 说明控制通道结论）。
/// - `Ok(None)`        —— 超时 / EOF / 非 hello（backend 未正常响应）。
/// - `Err(_)`          —— channel/exec/IO 硬错误。
async fn probe_backend(link: crate::dial_host::DialStream) -> Result<Option<BackendProbe>, String> {
    // U8a-2a：切成两半，写半边同一步停住（见 `inbound_client::split_and_park`）。
    let (rh, parked) = crate::backend::control::inbound_client::split_and_park(link);
    let mut reader = BufReader::new(rh);

    // ★ F10b：与主帧读同一个量（backend 出方向单行）⇒ 同一个上限、同一个机制。
    // 取消（超时）之后本函数直接返回，reader 不再复用 ⇒ 满足 `read_capped_line` 的使用条件。
    let mut line: Vec<u8> = Vec::new();
    let read = tokio::time::timeout(
        Duration::from_secs(8),
        read_capped_line(&mut reader, &mut line, BACKEND_FRAME_LINE_CAP),
    )
    .await;

    match read {
        Err(_elapsed) => Ok(None), // 超时
        Ok(Err(e)) => Err(format!("读 backend stdout 出错: {e}")),
        Ok(Ok(CappedLine::Eof)) => Ok(None), // EOF（backend 立即退出 / 未输出）
        // 握手行超限 ⇒ 与「非 hello 帧」同一档：backend 未正常握手。
        // ⚠ 这里**不**发 REMOTE_HEALTH —— 探测阶段还没有 host_label 语境，
        // 且返回 `None` 本身就会被调用方当成「没握上手」如实报出去。
        Ok(Ok(CappedLine::TooLong(bytes))) => {
            tracing::warn!("probe_backend: hello 行 {bytes} 字节超上限，判未握手");
            Ok(None)
        }
        Ok(Ok(CappedLine::Line)) => {
            let text = String::from_utf8_lossy(&line);
            let trimmed = text.trim_end_matches(['\n', '\r']);
            match parse_frame(trimmed) {
                Some(frame @ InboundFrame::Hello { .. }) => {
                    let head = describe_hello(&frame);
                    let control = probe_control_channel(parked, reader, &frame).await;
                    Ok(Some(BackendProbe {
                        control_ok: control.starts_with("control=ok("),
                        control_unsupported: control.starts_with("control=unsupported"),
                        summary: format!("{head} {control}"),
                    }))
                }
                // 非 hello 帧 → backend 未正常握手。写半边随 `parked` 一起 drop。
                _ => Ok(None),
            }
        }
    }
}

/// hello 帧的人读摘要。
fn describe_hello(frame: &InboundFrame) -> String {
    match frame {
        InboundFrame::Hello {
            v,
            build_id,
            host_arch,
            claude_dir,
            homes,
            capabilities,
            commands,
        } => format!(
            "v={v} build={build_id} arch={host_arch} claude_home={} caps={capabilities:?} cmds={commands:?}",
            claude_home_from_hello(homes, claude_dir)
        ),
        other => format!("(非 hello 帧: {other:?})"),
    }
}

/// 探测 `ping` 往返的超时。测试连接是人在等着看结果，别让它挂太久。
const CONTROL_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// U8a-2a：**入方向（控制通道）往返探测** —— 「测试连接」的第三步。
///
/// # 为什么加这一步
///
/// 此前「测试连接」只证明了两件事：SSH 通、backend 会说 hello。它**没有**证明
/// 反方向能走 —— 而 U8a-2b 之后起会话正是走那条反方向。少了这一步，用户会在
/// 「测试连接全绿」之后才在真起会话时发现控制通道根本不通，且无从归因。
///
/// 探测用 `ping`：backend 侧它是空操作（`Ok(None)`），零副作用、零写入。
///
/// 结果只追加进 `backend_hello` 摘要串（前端已有展示位），不新增字段 ⇒ 零 TS 绑定改动。
async fn probe_control_channel<R, W>(
    parked: crate::backend::control::inbound_client::ParkedWriter<W>,
    reader: BufReader<R>,
    hello: &InboundFrame,
) -> String
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let Some(witness) =
        crate::backend::control::inbound_client::BackendHello::from_hello_frame(hello)
    else {
        // 不可达（调用方已确认是 Hello），但不 panic —— 探测路径宁可少报一行。
        return "control=n/a".to_string();
    };
    let client = parked.into_client(witness);
    if !client.accepts("ping") {
        client.close_write();
        return "control=unsupported(backend 未声明入方向命令)".to_string();
    }
    // 应答走的是同一条流的读半边 —— 起一个只做路由的泵，探完就撤。
    let pump = {
        let c = client.clone();
        tauri::async_runtime::spawn(pump_inbound_replies(reader, c))
    };
    let t0 = std::time::Instant::now();
    let out = match client
        .call("ping", serde_json::Value::Null, CONTROL_PROBE_TIMEOUT)
        .await
    {
        Ok(_) => format!("control=ok({}ms)", t0.elapsed().as_millis()),
        Err(e) => format!("control=failed({e})"),
    };
    pump.abort();
    // 探测专用：告诉后端我们不会再发命令了（关写半边 ⇒ 它的入方向 reader 寿终）。
    // ⚠ 这**不会**让后端退出 —— 那句流传已久的注释是错的，e2e 第 9 条实测钉住。
    // 探测真正的收尾是本函数返回后整条 SSH channel 被 drop（stdout 断 ⇒ writer_task 结束 ⇒ exit）。
    client.close_write();
    out
}

/// 只做一件事：把读到的 `reply`/`cancelled` 路由给客户端。其余帧丢弃（探测不关心）。
async fn pump_inbound_replies<R>(
    mut reader: BufReader<R>,
    client: std::sync::Arc<crate::backend::control::inbound_client::InboundClient>,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    // ★ F10b：同一个量（backend 出方向单行）⇒ 同一个上限。
    // 超限那一行**丢掉继续泵** —— 它不可能是本泵关心的 `reply`/`cancelled`
    // （那两种帧都是几百字节量级），而整个泵是探测期临时物、`pump.abort()` 就撤。
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let text = match read_capped_line(&mut reader, &mut buf, BACKEND_FRAME_LINE_CAP).await {
            Ok(CappedLine::Eof) | Err(_) => break,
            Ok(CappedLine::TooLong(bytes)) => {
                tracing::warn!("pump_inbound_replies: 丢掉一行 {bytes} 字节（超上限）");
                continue;
            }
            Ok(CappedLine::Line) => String::from_utf8_lossy(&buf).into_owned(),
        };
        match parse_frame(text.trim_end_matches(['\n', '\r'])) {
            Some(InboundFrame::Reply {
                id,
                ok,
                code,
                message,
                data,
            }) => {
                client.route_reply(&id, ok, code, message, data);
            }
            Some(InboundFrame::Cancelled { id }) => {
                client.route_cancelled(&id);
            }
            // 〔audit-0805 08-06，定框 E4〕**静默失败一律给身份**。
            // 原来这里是 `_ => {}`：这条泵只认 Reply/Cancelled，其余帧**一声不响地丢掉** ——
            // 而它的兄弟 `route_inbound_frame` 对同一情况是 `warn!` 报身份。
            // 同一件事两条路一条报一条不报，正是本区反复记的不对称。
            Some(other) => {
                tracing::warn!("入方向应答泵收到非应答帧，忽略：{other:?}");
            }
            None => {}
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_batcher_tests.rs"]
mod batcher_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_parse_frame_tests.rs"]
mod parse_frame_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_tier1_tests.rs"]
mod tier1_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_reannounce_tests.rs"]
mod reannounce_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_snapshot_tail_tests.rs"]
mod snapshot_tail_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_frame_dispatch_shape.rs"]
mod frame_dispatch_shape;

#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_tmux_snapshot_exposure_tests.rs"]
mod tmux_snapshot_exposure_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/ssh_source_snapshot_tests.rs"]
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
#[path = "../../../tests/bridge/ssh_source_capped_line_tests.rs"]
mod capped_line_tests;
