//! SSH-remote 数据源（issue #15）。
//!
//! 本模块是**活代码**：从 lib.rs 的 `setup()` 调用（`remote.enabled=true` 且配置完整时）。
//! 它提供三块能力：
//! - **russh client 数据源**：[`run`] 连远端、exec daemon、把 daemon stdout 的
//!   line-delimited JSON 帧解析后走与本地 watcher 相同的出口（`batch_to_payloads` →
//!   `on_line_batch`），session 增减走专用 `session_changes` 通道。与本地 jsonl-watcher
//!   **并行**作为附加数据源（远端行带 origin=host 标签）。
//! - **ssh-config 导入**：[`list_ssh_host_aliases`] / [`resolve_ssh_host`]（`ssh -G`）
//!   供前端「从 ~/.ssh/config 导入」自动填连接参数。
//! - **测试连接**：[`test_remote_connection`] 实连一次，回 SSH ✓/✗ + host key 指纹 +
//!   daemon ✓/✗（hello），供 UI 分级展示 + TOFU→strict 指纹固化。
//!
//! 上述三个 `#[tauri::command]` 在 lib.rs 的 invoke_handler! 里注册。
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

use russh::client;
use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKey};
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::event_replay::EventReplay;
use crate::session_map::{RemovalCause, RemovedSid, SessionChange};

/// S0 **跨语言双写点**：daemon 那侧 `RemovalCause::Superseded` 的 serde 线上名。
/// 改这里必须同步 `remote-daemon-proto/src/wire.rs`（同 `TMUX_LS_FMT` 的纪律）。
const REMOVAL_CAUSE_SUPERSEDED: &str = "superseded";
use crate::watcher::JsonlLine;

/// 重连退避下界：每次连接掉线后至少等这么久再重连（也是连上过之后的快速重连值）。
const RECONNECT_MIN: Duration = Duration::from_secs(2);
/// 重连退避上界：指数退避封顶，避免长断网时无意义地拉长重连间隔。
const RECONNECT_MAX: Duration = Duration::from_secs(30);

/// 纯函数：把当前退避翻倍并封顶到 [`RECONNECT_MAX`]。run() 的重连循环在"仍未连上"时调用。
fn next_backoff(cur: Duration) -> Duration {
    (cur * 2).min(RECONNECT_MAX)
}

/// 远端 daemon 的连接配置。S5 会从 monitor 的 config 文件反序列化出来；
/// Tier 1（issue #15）的「测试连接」命令直接收前端传来的同形对象（camelCase）。
///
/// **serde camelCase 必须与前端 RemoteHostConfig / lib.rs::load_remote_configs 严格一致**：
/// host / port / user / keyPath / daemonPath / hostKeyFingerprint / label（多机 #30，
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
    /// 远端要 exec 的 daemon 命令（含参数前缀由 S5 决定）。
    pub daemon_path: String,
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
    /// Batch14-F59：daemonless 降级读取开关（per-host）。true = 该主机**不部署/不连 daemon**，
    /// 走纯 SSH exec `find`+`tail -c +offset` 轮询读会话 jsonl（[`daemonless_stream_loop`]，
    /// 能力子集：无 bg kind/无状态灯/无拥塞信号/仅最近活跃会话）。false（默认）= 现有 daemon
    /// 流路径（[`stream_loop`]）一行不动。`run()` 顶层据此二选一。
    #[serde(default)]
    pub daemonless: bool,
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

/// russh client handler：负责 host key 校验（check_server_key）。
///
/// 持有期望指纹，`check_server_key` 据此决定接受 / 拒绝（见该方法注释）。
/// 另持有一个共享 cell（`observed_fingerprint`），**无论接受 / 拒绝**都把实际看到的
/// server key 指纹写进去 —— Tier 1（issue #15）的「测试连接」据此向用户展示指纹、
/// 供 TOFU→严格校验固化（known_hosts 式）。
/// F58：已鉴权的 SSH 客户端会话句柄别名（端口转发把它存注册表保活 + 开 direct-tcpip channel）。
pub(crate) type SshSession = client::Handle<ClientHandler>;

pub(crate) struct ClientHandler {
    expected_fingerprint: Option<String>,
    /// check_server_key 观察到的实际指纹回传通道（与调用方共享）。
    observed_fingerprint: Arc<Mutex<Option<String>>>,
    /// F46：分阶段事件 emitter（仅测试连接路径 Some）。check_server_key 命中 emit HostKey。
    stage_emitter: Option<tauri::ipc::Channel<ConnectStage>>,
    /// F46：本 handler 对应的拨号地址（`host:port`），emit 时标注泳道。
    endpoint: Option<String>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    /// host key 校验（russh 0.61 签名：`&mut self, &PublicKey -> Result<bool, Error>`，async）。
    ///
    /// 返回 `Ok(true)` = 接受该 host key，`Ok(false)` = 拒绝（russh 会中止握手）。
    ///
    /// 策略：
    /// - `expected_fingerprint = Some(fp)`：计算 server key 的 SHA256 指纹，**仅**在
    ///   匹配时返回 `Ok(true)`，否则 `Ok(false)` 拒绝（防 MITM / key 轮换未同步）。
    /// - `expected_fingerprint = None`：trust-on-first-use 暂行接受，但发一条**显眼**的
    ///   `tracing::warn!` 说明这是未经验证的 host key（S5 应把首次拿到的指纹固化回 config，
    ///   之后转入严格校验）。**绝不**静默接受任意 key。
    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let actual = server_public_key.fingerprint(HashAlg::Sha256).to_string();
        // 无论后续接受 / 拒绝，都先把实际指纹写回共享 cell（测试连接据此展示 + 固化）。
        if let Ok(mut slot) = self.observed_fingerprint.lock() {
            *slot = Some(actual.clone());
        }
        // F46：到 host key 校验 = 该地址 TCP+KEX 已过,emit HostKey 泳道事件。
        if let Some(ep) = &self.endpoint {
            emit_stage(
                &self.stage_emitter,
                ConnectStage::HostKey {
                    endpoint: ep.clone(),
                    fingerprint: actual.clone(),
                },
            );
        }
        match &self.expected_fingerprint {
            Some(expected) => {
                // FIX 7：比对前 trim 掉存储指纹两侧的换行/空白，否则配置里残留的尾随
                // 空白会让一个本应匹配的指纹永远被拒（误判 MITM）。
                if actual == expected.trim() {
                    tracing::info!("ssh host key fingerprint verified: {actual}");
                    Ok(true)
                } else {
                    // F43：失配时附上实际 key 的算法——诊断里区分「合法换 key 类型」
                    // （如 ed25519→rsa，算法不同）与「同类型 key 被换（真 MITM 疑点）」;
                    // 措辞指向重置入口（服务器合法轮换 host key 后走它解锁，而非误判永锁）。
                    let alg = server_public_key.algorithm();
                    tracing::error!(
                        "ssh host key MISMATCH: expected {expected}, got {actual} (alg={alg}); \
                         rejecting connection. 若确系服务器合法更换过 host key（重装/轮换），\
                         请在设置里「重置为 TOFU」后重连;否则可能是中间人攻击。"
                    );
                    Ok(false)
                }
            }
            None => {
                // 明确标注的 TOFU stopgap：接受但大声 warn，不静默。
                tracing::warn!(
                    "ssh host key NOT verified (trust-on-first-use): accepting unverified key {actual}; \
                     S5 应把该指纹固化进 config 并转严格校验"
                );
                Ok(true)
            }
        }
    }
}

/// `default_ssh_agent_pipe` —— Windows OpenSSH agent 的命名管道路径。
///
/// Win10+/Win11 自带的 OpenSSH agent 监听 `\\.\pipe\openssh-ssh-agent`（SSH_AUTH_SOCK
/// 在 Windows 上不是标准；OpenSSH-for-Windows 用固定命名管道）。非 Windows 留 `None`
/// （Tier 1 的 agent 仅在 Windows 上尝试；Unix 走 key_path 即可，本 app 也只发 Windows）。
#[cfg(windows)]
fn default_ssh_agent_pipe() -> Option<&'static str> {
    Some(r"\\.\pipe\openssh-ssh-agent")
}

/// 连接 + 鉴权的共享实现：被 [`connect_and_exec`]（长连接数据源）和
/// [`test_remote_connection`]（一次性探活）复用。
///
/// 返回已鉴权的 session + 共享的 `observed_fingerprint` cell（握手时 check_server_key 已
/// 把实际 server key 指纹写进去，调用方可读出展示 / 固化）。
///
/// 鉴权策略（Tier 1, issue #15）：
/// - `cfg.key_path = Some(path)`：publickey 鉴权（既有默认路径，最稳）。
/// - `cfg.key_path = None`：尝试 ssh-agent（Windows 命名管道），枚举 agent 身份逐个
///   `authenticate_publickey_with`。agent 不可用 / 无匹配身份 → 返回清晰 Err。
/// Batch14-F45：happy-eyeballs 竞发阶梯（第 i 个地址延迟 i*STAGGER 起拨，首个成功者胜后
/// 其余在飞连接被 abort）。250ms 是 RFC 8305 常用值。
const RACE_STAGGER: Duration = Duration::from_millis(250);
/// 握手看门狗默认上限（长连接 inactivity_timeout=None 时用）——黑洞地址 TCP 连上后
/// 握手无限阻塞时兜底,到点整批 abort（drop 关 socket）。
const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(45);

/// Batch14-F46：连接分阶段事件（测试连接时经 Tauri Channel 流给前端做泳道日志）。
/// 只在 `test_remote_connection` 路径 emit（emitter=Some）;daemon 流/exec/SFTP 传 None,
/// 零开销零事件。阶段取 russh 能干净观测的粒度——不含 KEX（russh 不暴露 KEX 回调,
/// HostKey 触发即隐含 TCP+KEX 已过）。
#[derive(Serialize, Clone, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConnectStage {
    /// 某地址开始拨号（TCP+握手）。
    Dialing { endpoint: String },
    /// 某地址握手到 host key 校验（带指纹;隐含 TCP+KEX 已过）。
    HostKey {
        endpoint: String,
        fingerprint: String,
    },
    /// 某地址连接失败（reason = 粗分类 + 原始错误）。
    Failed { endpoint: String, reason: String },
    /// 竞发胜出地址（其余在飞已 abort）。
    Won { endpoint: String },
    /// 鉴权结果。
    Auth { ok: bool, detail: Option<String> },
    /// 连接就绪（握手+鉴权全过）。
    Established,
}

/// F46：把 russh 连接错误串粗分类成阶段标签（前端泳道用不同图标/文案）。
/// 保守分类:命中关键词才归类,否则 `other`。
pub fn classify_stage(err: &str) -> &'static str {
    let e = err.to_ascii_lowercase();
    if e.contains("refused") || e.contains("no route") || e.contains("unreachable") {
        "tcp" // TCP 层拒绝/不可达
    } else if e.contains("timeout") || e.contains("超时") || e.contains("timed out") {
        "timeout"
    } else if e.contains("key") || e.contains("mismatch") || e.contains("指纹") {
        // host key 校验失败/不匹配（russh 拒绝 host key 报 "Unknown server key"）。
        "hostkey"
    } else {
        "other"
    }
}

/// F46：安全 emit（emitter=None 直接 no-op;send 失败仅 warn 不阻断连接）。
fn emit_stage(emitter: &Option<tauri::ipc::Channel<ConnectStage>>, stage: ConnectStage) {
    if let Some(ch) = emitter {
        if let Err(e) = ch.send(stage) {
            tracing::warn!("connect stage emit failed: {e}");
        }
    }
}

/// F45：per-origin「上次成功地址」——竞发时排首（下次大概率同一条路最快），赢家更新。
/// 进程内软状态,丢了只是少一次优化,不影响正确性。
fn last_good_store() -> &'static Mutex<std::collections::HashMap<String, Endpoint>> {
    static STORE: std::sync::OnceLock<Mutex<std::collections::HashMap<String, Endpoint>>> =
        std::sync::OnceLock::new();
    STORE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn last_good_for(origin: &str) -> Option<Endpoint> {
    last_good_store().lock().ok()?.get(origin).cloned()
}

fn record_last_good(origin: &str, ep: &Endpoint) {
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
fn winner_order(endpoints: Vec<Endpoint>, last_good: Option<&Endpoint>) -> Vec<Endpoint> {
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

/// F45：happy-eyeballs 竞发——按 `order` 阶梯并发拨号（每个 `client::connect` = TCP+SSH
/// 握手+host key 校验,各自独立 handler+cell）,首个握手成功者胜、立即 abort 其余在飞
/// （drop 关 socket,trap #6/#8）,鉴权留给调用方只对胜者做一次。整体 `deadline` 看门狗兜
/// 黑洞地址。全部失败 → 聚合各地址错误（trap #3）;被 abort 的输家不计入错误（trap #2）。
///
/// aterm F20 陷阱 #1(disconnect 不清 configs/sessions 致 map 无界增长)与 #5(在飞重连
/// 被 disconnect 后完成的纪元幽灵)对本实现**不适用**:每次 connect 自建一个局部 JoinSet、
/// 无跨调用持久竞发态或共享 channel(晚到 task 随 set drop 弃),唯一跨调用状态 `last_good_store`
/// 按 origin 键、有界,既非无界 disconnect map 也无纪元计数器。
async fn race_connect(
    config: Arc<client::Config>,
    expected_fp: Option<String>,
    order: Vec<Endpoint>,
    deadline: Duration,
    stage_emitter: Option<tauri::ipc::Channel<ConnectStage>>,
) -> Result<
    (
        client::Handle<ClientHandler>,
        Arc<Mutex<Option<String>>>,
        Endpoint,
    ),
    String,
> {
    use tokio::task::JoinSet;

    let addr_list = order
        .iter()
        .map(|e| format!("{}:{}", e.host, e.port))
        .collect::<Vec<_>>()
        .join(", ");

    let race = async move {
        let mut set: JoinSet<Result<_, String>> = JoinSet::new();
        for (i, ep) in order.into_iter().enumerate() {
            let config = Arc::clone(&config);
            let fp = expected_fp.clone();
            let emitter = stage_emitter.clone();
            set.spawn(async move {
                if i > 0 {
                    tokio::time::sleep(RACE_STAGGER * i as u32).await;
                }
                let ep_label = format!("{}:{}", ep.host, ep.port);
                emit_stage(
                    &emitter,
                    ConnectStage::Dialing {
                        endpoint: ep_label.clone(),
                    },
                );
                let cell: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
                let handler = ClientHandler {
                    expected_fingerprint: fp,
                    observed_fingerprint: Arc::clone(&cell),
                    stage_emitter: emitter.clone(),
                    endpoint: Some(ep_label.clone()),
                };
                match client::connect(config, (ep.host.as_str(), ep.port), handler).await {
                    Ok(h) => Ok((h, cell, ep)),
                    Err(e) => {
                        emit_stage(
                            &emitter,
                            ConnectStage::Failed {
                                endpoint: ep_label.clone(),
                                reason: format!("[{}] {e}", classify_stage(&e.to_string())),
                            },
                        );
                        Err(format!("{ep_label} {e}"))
                    }
                }
            });
        }

        let mut errors: Vec<String> = Vec::new();
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok(winner)) => {
                    // 首个成功者胜；drop set → abort 其余在飞（关 socket，不等死地址超时）。
                    set.abort_all();
                    emit_stage(
                        &stage_emitter,
                        ConnectStage::Won {
                            endpoint: format!("{}:{}", winner.2.host, winner.2.port),
                        },
                    );
                    return Ok(winner);
                }
                Ok(Err(e)) => errors.push(e),
                // trap #2 的真正实现是「首个 Ok 即 return、根本不收集输家错误」；此分支
                // 防御性存在(当前控制流下不可达:abort_all 后立即 return,不再 join_next),
                // 显式声明取消不算错误、防未来重构改动早返回结构时回归。
                Err(je) if je.is_cancelled() => {}
                Err(je) => errors.push(format!("拨号任务异常: {je}")),
            }
        }
        Err(if errors.is_empty() {
            "无可用地址".to_string()
        } else {
            format!("所有地址连接失败: {}", errors.join("; "))
        })
    };

    match tokio::time::timeout(deadline, race).await {
        Ok(r) => r,
        Err(_) => Err(format!("所有地址握手超时（{addr_list}）")),
    }
}

/// F56：持有跳板机 session,让 direct-tcpip 隧道在目标连接存活期间不被 drop 关闭
/// （drop 跳板 Handle → russh 关跳板连接 → 隧道 channel 死 → 目标断）。按**目标 origin** 键——
/// 每次经跳板重连替换旧 holder（drop 旧跳板连接），按配置的被跳板目标数有界。
fn jump_holders() -> &'static Mutex<std::collections::HashMap<String, client::Handle<ClientHandler>>>
{
    static STORE: std::sync::OnceLock<
        Mutex<std::collections::HashMap<String, client::Handle<ClientHandler>>>,
    > = std::sync::OnceLock::new();
    STORE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// F56：经跳板机隧道连目标。连+鉴权跳板（复用 `connect_session`，白嫖跳板多地址竞速+指纹校验+
/// 鉴权）→ `channel_open_direct_tcpip` 到目标主地址 → `connect_stream` 在隧道流上跑目标 SSH 握手
/// （同 `ClientHandler` 验目标指纹）→ 跳板 session 存 `jump_holders` 保活。**fail-closed**：
/// 环/查无/连不上 → Err（绝不回退直连目标）。v1 单跳（忽略跳板自身的 jump）。
async fn connect_via_jump(
    jump_label: &str,
    target: &RemoteConfig,
    config: Arc<client::Config>,
    stage_emitter: Option<tauri::ipc::Channel<ConnectStage>>,
) -> Result<
    (
        client::Handle<ClientHandler>,
        Arc<Mutex<Option<String>>>,
        Endpoint,
    ),
    String,
> {
    if jump_label == target.origin_label() {
        return Err("跳板配置指向自己（环）".to_string());
    }
    let mut jump_cfg = crate::load_remote_config_by_label(jump_label)
        .ok_or_else(|| format!("跳板配置未找到: {jump_label}"))?;
    jump_cfg.jump = None; // v1 单跳：忽略跳板自身的 jump，防链式递归/环
    emit_stage(
        &stage_emitter,
        ConnectStage::Dialing {
            endpoint: format!("跳板 {}", jump_cfg.origin_label()),
        },
    );
    // 复用 connect_session 连+鉴权跳板。Box::pin：递归 async 需装箱定尺寸。
    let (jump_session, _jump_fp) = Box::pin(connect_session(&jump_cfg, None, None))
        .await
        .map_err(|e| format!("跳板 {} 连接失败: {e}", jump_cfg.origin_label()))?;
    // 经跳板开 direct-tcpip 到目标主地址（v1 单地址，不经跳板对目标多地址竞速）。
    let channel = jump_session
        .channel_open_direct_tcpip(
            target.host.clone(),
            target.port as u32,
            "127.0.0.1".to_string(),
            0,
        )
        .await
        .map_err(|e| format!("经跳板开隧道到 {}:{} 失败: {e}", target.host, target.port))?;
    let stream = channel.into_stream();
    // 隧道流上跑目标 SSH 握手（同 ClientHandler 验目标指纹）。
    let cell: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let handler = ClientHandler {
        expected_fingerprint: target.host_key_fingerprint.clone(),
        observed_fingerprint: Arc::clone(&cell),
        stage_emitter: stage_emitter.clone(),
        endpoint: Some(format!("{}:{}（经跳板）", target.host, target.port)),
    };
    let session = client::connect_stream(config, stream, handler)
        .await
        .map_err(|e| format!("目标 SSH 握手失败（经跳板）: {e}"))?;
    // 跳板 session 保活（否则 drop 关连接 → 隧道死）。按目标 origin 键，替换旧的。
    if let Ok(mut m) = jump_holders().lock() {
        m.insert(target.origin_label(), jump_session);
    }
    let winner = Endpoint {
        host: target.host.clone(),
        port: target.port,
    };
    Ok((session, cell, winner))
}

pub(crate) async fn connect_session(
    cfg: &RemoteConfig,
    inactivity_timeout: Option<Duration>,
    stage_emitter: Option<tauri::ipc::Channel<ConnectStage>>,
) -> Result<(client::Handle<ClientHandler>, Arc<Mutex<Option<String>>>), String> {
    // FIX 1（issue #15 review）：长连接数据源**绝不**靠 inactivity_timeout 兜底死链——
    // russh 0.61 的 inactivity timer 在没有 keepalive 时会在到点直接拆掉一条**健康的**
    // 空闲连接（idle 1h 的 Claude 会话很常见）。改用 SSH 层 keepalive：每 30s 无收包就
    // 发一个 keepalive，连发 keepalive_max(默认 3) 次无回应才判死（≈90s 探活）。死链由
    // keepalive 超时 + daemon EOF 检出，inactivity_timeout 对长连接置 None。
    // 测试连接（短命探活）仍可传 Some(短超时)，故 keepalive 与 inactivity 同时支持。
    let keepalive_interval = inactivity_timeout
        .is_none()
        .then(|| Duration::from_secs(30));
    let config = Arc::new(client::Config {
        inactivity_timeout,
        keepalive_interval,
        ..Default::default()
    });

    // F45：多地址 happy-eyeballs 竞发（单地址时退化为一次直连，行为等价老实现）。
    // 竞发只到 TCP+握手+host key 校验；鉴权只对胜者做一次（防 agent 并发 MaxAuthTries）。
    // 同一 host_key_fingerprint 跨地址钉身份：错连别机的 endpoint 因指纹失配自 reject 出局。
    // F56：cfg.jump 有值 → 经跳板隧道连（fail-closed，不回退直连）；否则 → 多地址竞速直连。
    // 两路都产出 (session, observed_fingerprint, winner)，汇合到下方同一鉴权块。
    let origin = cfg.origin_label();
    let (mut session, observed_fingerprint, winner) =
        match cfg.jump.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(jump_label) => {
                connect_via_jump(jump_label, cfg, Arc::clone(&config), stage_emitter.clone())
                    .await?
            }
            None => {
                let order = winner_order(cfg.endpoints(), last_good_for(&origin).as_ref());
                let deadline = inactivity_timeout.unwrap_or(HANDSHAKE_DEADLINE);
                race_connect(
                    Arc::clone(&config),
                    cfg.host_key_fingerprint.clone(),
                    order,
                    deadline,
                    stage_emitter.clone(),
                )
                .await?
            }
        };

    // RSA key 需要协商出 server 支持的 hash alg；非 RSA key 时 flatten 成 None。
    let best_hash = session
        .best_supported_rsa_hash()
        .await
        .map_err(|e| format!("协商 rsa hash 失败: {e}"))?
        .flatten();

    // F46：鉴权阶段——失败时 emit Auth{ok:false}+错误,便于泳道定位「卡在鉴权」。
    let auth_result: Result<(), String> = async {
        match cfg.key_path.as_ref() {
            Some(key_path) => {
                let key_pair = load_secret_key(key_path, None)
                    .map_err(|e| format!("加载私钥 {key_path} 失败: {e}"))?;
                let authenticated = session
                    .authenticate_publickey(
                        &cfg.user,
                        PrivateKeyWithHashAlg::new(Arc::new(key_pair), best_hash),
                    )
                    .await
                    .map_err(|e| format!("publickey 鉴权失败: {e}"))?;
                if !authenticated.success() {
                    return Err(format!("publickey 鉴权被拒（user={}）", cfg.user));
                }
                Ok(())
            }
            None => authenticate_via_agent(&mut session, &cfg.user, best_hash).await,
        }
    }
    .await;
    if let Err(e) = auth_result {
        emit_stage(
            &stage_emitter,
            ConnectStage::Auth {
                ok: false,
                detail: Some(e.clone()),
            },
        );
        return Err(e);
    }
    emit_stage(
        &stage_emitter,
        ConnectStage::Auth {
            ok: true,
            detail: None,
        },
    );

    // D 审计建议-1：last-good = 上次**完整成功**（握手+鉴权）的地址。放在鉴权成功后,
    // 避免 TOFU×异机误配时「粘住」一个连得上但认证失败的地址（下次仍先拨它、仍失败,
    // 真机永不被试）。正常固化下 A/B 同机同 key,放前放后等价;此处取更严谨语义。
    record_last_good(&origin, &winner);
    emit_stage(&stage_emitter, ConnectStage::Established);

    Ok((session, observed_fingerprint))
}

/// ssh-agent 鉴权（Tier 1 便利路径，issue #15 Part 3）。
///
/// 连到 Windows OpenSSH agent 命名管道，`request_identities` 枚举身份，逐个
/// `authenticate_publickey_with`（签名委托给 agent）。任一成功即返回 Ok。
///
/// 全程 best-effort：agent 连不上 / 没身份 / 全被拒 → 返回带原因的 Err（测试连接会把它
/// 映射成可读的 message，不 panic）。
#[cfg(windows)]
async fn authenticate_via_agent(
    session: &mut client::Handle<ClientHandler>,
    user: &str,
    best_hash: Option<HashAlg>,
) -> Result<(), String> {
    use russh::keys::agent::client::AgentClient;

    let pipe = default_ssh_agent_pipe()
        .ok_or_else(|| "未配置私钥路径，且本平台无 ssh-agent 支持".to_string())?;
    let mut agent = AgentClient::connect_named_pipe(pipe).await.map_err(|e| {
        format!(
            "未配置私钥路径(keyPath)，尝试 ssh-agent 失败：连不上 {pipe}（agent 未运行？）: {e}"
        )
    })?;

    let identities = agent
        .request_identities()
        .await
        .map_err(|e| format!("ssh-agent 枚举身份失败: {e}"))?;
    if identities.is_empty() {
        return Err("ssh-agent 没有任何身份（ssh-add 了吗？），且未配置 keyPath".to_string());
    }

    let mut last_err: Option<String> = None;
    for id in identities {
        let pubkey = id.public_key().into_owned();
        match session
            .authenticate_publickey_with(user, pubkey, best_hash, &mut agent)
            .await
        {
            Ok(res) if res.success() => return Ok(()),
            Ok(_) => last_err = Some(format!("agent 身份被拒（user={user}）")),
            Err(e) => last_err = Some(format!("agent 签名鉴权出错: {e}")),
        }
    }
    Err(last_err.unwrap_or_else(|| "ssh-agent 所有身份均鉴权失败".to_string()))
}

/// 非 Windows：本 app 只发 Windows，且 Unix 走 key_path 即可。无 agent 时直接报缺 keyPath。
#[cfg(not(windows))]
async fn authenticate_via_agent(
    _session: &mut client::Handle<ClientHandler>,
    _user: &str,
    _best_hash: Option<HashAlg>,
) -> Result<(), String> {
    // TODO(Phase 1): 非 Windows 的 ssh-agent（SSH_AUTH_SOCK / UnixStream）。
    Err("未配置私钥路径(keyPath)，且本平台暂不支持 ssh-agent".to_string())
}

/// 连接远端、鉴权、开 session channel、exec `cfg.daemon_path`，
/// 返回 channel 的双向流（`AsyncRead + AsyncWrite`）——读端即 daemon 的 stdout 数据。
///
/// 鉴权委托给 [`connect_session`]（publickey 或 ssh-agent）。
/// 错误统一 map 成 `String`（本 crate 未直接依赖 anyhow，不为骨架引入新依赖）。
/// F66（#58③）流模式门控决策（纯函数，矩阵单测）：**从 daemon 声明的能力 token 决定
/// 发哪些 flag**，不再靠 build_id 精确匹配（Batch7-F24/Batch8-F26 的旧机制）。
///
/// - `capabilities` = daemon hello 自报的能力集（旧 daemon 无声明 → 空集）。
/// - 空集（旧 daemon / 尚未确认）→ `(false, false)`：全降级 = 2.18.0 行为，功能退化但
///   连接正常。
/// - `tail_only`（历史改走旁路快照，拥塞根除）需 daemon 声明 `"tail-only"`。
/// - `with_bg`（放行 bg 会话）需 daemon 声明 `"bg"` **且**用户开了 `show_bg`。
///
/// **§26 死循环护栏靠声明本身保住**：旧 daemon 把未知 flag 当一次性查询 → 退出 → 无
/// hello → 重连死循环。而只有**会先剥离该 flag** 的 daemon 才声明对应能力（见 daemon
/// `CAPABILITIES` 注释），故「声明了 = 发该 flag 安全」——比 build_id 精确匹配更强更干净，
/// 且直接闭合 2026-07-09「身份确认不了就全降级」事故（能力由 daemon 自报，不靠脆弱身份链）。
fn decide_stream_flags(capabilities: &[String], show_bg: bool) -> (bool, bool) {
    let has = |c: &str| capabilities.iter().any(|t| t == c);
    (show_bg && has("bg"), has("tail-only"))
}

/// F66（#58③）★ 防无限重连的收敛判据（纯函数，穷举单测）：收到 daemon 能力声明后，
/// **是否值得重连一轮升级流模式**。`cur` = 本轮实际发的 `(with_bg, tail_only)`；`next` =
/// 据 daemon 自报能力算出的下一轮 flag。
///
/// **仅当下一轮会开一个本轮关着的 flag** 才重连——每次重连严格增开 flag，flag 数有限（2）
/// ⟹ 最多 2 轮收敛，绝不无限重连。**关键定理**：一旦记账 `hello_confirmed=Some(D)`，下一轮
/// `caps=D` ⟹ `next==cur` ⟹ 本函数两项皆自相矛盾（`next_tail && !cur_tail` 与
/// `next_bg && !cur_bg` 在 next==cur 时恒 false）⟹ 恒 `false`，不再重连。
///
/// 两项都写全 `&& !cur_*`（不靠调用点的外层 `!tail_only` guard），使收敛不变式在函数内自洽、
/// 可独立穷举测试（审计：原 `next_tail` 裸项隐含依赖外层 guard，读者需回连才懂）。
fn should_upgrade_reconnect(cur: (bool, bool), next: (bool, bool)) -> bool {
    let ((cur_bg, cur_tail), (next_bg, next_tail)) = (cur, next);
    (next_tail && !cur_tail) || (next_bg && !cur_bg)
}

#[cfg(test)]
mod stream_flag_gate_tests {
    use super::decide_stream_flags;

    fn caps(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// F66 DoD：能力门控矩阵——空集（旧 daemon/未确认）恒 (false,false)；
    /// 声明 bg+tail-only 后 tail_only 恒开、with_bg 随 showBgSessions；
    /// 部分声明只开对应位；未知 token 忽略。
    #[test]
    fn capability_gate_matrix() {
        // 空集 = 旧 daemon / 尚未收到 hello → 全降级
        assert_eq!(decide_stream_flags(&caps(&[]), true), (false, false));
        assert_eq!(decide_stream_flags(&caps(&[]), false), (false, false));
        // 全能力声明
        assert_eq!(
            decide_stream_flags(&caps(&["bg", "tail-only"]), true),
            (true, true)
        );
        assert_eq!(
            decide_stream_flags(&caps(&["bg", "tail-only"]), false),
            (false, true),
            "关 showBgSessions 只关 with_bg，tail-only 照开"
        );
        // 部分声明：只有 tail-only → with_bg 恒 false（即便 show_bg）
        assert_eq!(
            decide_stream_flags(&caps(&["tail-only"]), true),
            (false, true),
            "daemon 没声明 bg → 即便用户想看也不发 --with-bg"
        );
        // 部分声明：只有 bg
        assert_eq!(
            decide_stream_flags(&caps(&["bg"]), true),
            (true, false),
            "daemon 没声明 tail-only → 不发 --tail-only（历史走全量推流）"
        );
        // 未知 token 忽略（加法式向前兼容：未来 daemon 声明我们还不认识的能力）
        assert_eq!(
            decide_stream_flags(&caps(&["bg", "tail-only", "future-x"]), true),
            (true, true),
            "未知能力 token 不影响已知门控"
        );
    }

    use super::should_upgrade_reconnect as up;

    /// F66 ★ 防无限重连：`should_upgrade_reconnect` 只在「下一轮严格增开一个本轮关着的
    /// flag」时才 true——保证收敛。这条测试是收 hello 自愈升级那段的回归护栏
    /// （审计阻塞：那段防死循环逻辑此前零测试；抽成纯函数后在此穷举）。
    #[test]
    fn upgrade_reconnect_converges() {
        // 升级值得：本轮全关，下一轮能开
        assert!(up((false, false), (true, true)), "全关→全开 该升级");
        assert!(up((false, false), (false, true)), "开 tail 该升级");
        assert!(up((false, false), (true, false)), "开 bg 该升级");
        // ★ 关键收敛点：记账后本轮 caps=声明集 → next==cur → 恒不再重连
        assert!(!up((true, true), (true, true)), "记账后 next==cur 不再重连");
        assert!(
            !up((true, false), (true, false)),
            "只 bg：记账后不抖（!cur_bg 挡住）"
        );
        assert!(!up((false, true), (false, true)), "只 tail：记账后不抖");
        // 本轮已开 bg、下一轮又加 tail → tail 项触发升级（严格增开）
        assert!(
            up((true, false), (true, true)),
            "本轮 bg、下一轮加 tail → 升级"
        );
        // 下一轮反而关了某 flag（不该发生，但函数须安全）→ 不重连
        assert!(!up((true, true), (false, false)), "下一轮更弱 → 不重连");
        // swap 边角（本轮 bg、下一轮只 tail）：tail 项触发（靠 tail latch 后续收敛）
        assert!(up((true, false), (false, true)), "swap：新开 tail 该升级");
    }

    /// F66：确认 `EMBEDDED_DAEMON_CAPABILITIES` 的 build.rs 单源管道真的通（非空、含当前
    /// token）——否则乐观路径静默退化成「第一轮降级 + hello 自愈」（仍正确，只慢一轮）。
    /// 用 `contains` 而非精确相等：daemon 将来加 token 时本测试仍过，不误红。
    #[test]
    fn embedded_capabilities_single_source_wired() {
        let caps = super::embedded_daemon_capabilities();
        assert!(caps.contains(&"bg".to_string()), "单源应含 bg：{caps:?}");
        assert!(
            caps.contains(&"tail-only".to_string()),
            "单源应含 tail-only：{caps:?}"
        );
    }

    /// U-1（2026-08-01）：**`build_id` 那半单源管道一直没有等价断言。**
    ///
    /// `build.rs::emit_daemon_build_id` 抠不到就 `unwrap_or_else(|| "unknown")` —— **静默退化**。
    /// 一旦 daemon crate 改名 / `BUILD_ID` 挪出 `main.rs` / `const` 写法换行，
    /// `EXPECTED_DAEMON_BUILD_ID` 会变成 `"unknown"`，而**编译通过、测试全绿**，
    /// 运行期把每台远端 daemon 都判成 `StaleBuild` → 无限重装。
    ///
    /// capabilities 那半有 `embedded_capabilities_single_source_wired` 兜着，这半没有。
    /// U13 的仓库级重命名**必须**先有这条，否则那次重命名是静默失败。
    ///
    /// 判据刻意宽松（不写死具体 id）：只要不是兜底值、且长得像一个 build id 就行 ——
    /// 写死 id 会让每次正常 bump 都误红，那种守卫最后会被人删掉。
    #[test]
    fn embedded_build_id_single_source_wired() {
        let id = super::EXPECTED_DAEMON_BUILD_ID;
        assert_ne!(
            id, "unknown",
            "`build.rs::emit_daemon_build_id` 没抠到 daemon 的 `const BUILD_ID` —— \
             多半是路径失效（crate 改名 / 文件搬家）或 `const` 写法变了。\
             它是**静默退化**：不修的话每台远端都会被判 StaleBuild 并无限重装。"
        );
        assert!(
            !id.is_empty() && id.len() <= 64,
            "build_id 形状可疑：{id:?}"
        );
        assert!(
            id.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'),
            "build_id 含意外字符（多半是抠错了行）：{id:?}"
        );
    }
}

pub async fn connect_and_exec(
    cfg: &RemoteConfig,
    with_bg: bool,
    tail_only: bool,
) -> Result<russh::ChannelStream<client::Msg>, String> {
    // 与 jsonl-watcher 不同，daemon 是长连接：inactivity_timeout=None → connect_session
    // 自动启用 30s keepalive（见 FIX 1 注释），靠 keepalive + EOF 检死链，不靠定时拆链。
    // Batch7-F24/Batch8-F26：两个流模式 flag 都由调用方决定（run_stream 里绑定
    // "部署确认为当前版本"，见该处注释）。tail_only=true → daemon 不重放历史
    // （历史由本侧旁路 --read-session 快照拉取），实时通道流量趋零。
    let mut cmd = shell_quote(&cfg.daemon_path);
    if with_bg {
        cmd.push_str(" --with-bg");
    }
    if tail_only {
        cmd.push_str(" --tail-only");
    }
    connect_and_exec_cmd(cfg, &cmd).await
}

// === Batch8-F26：旁路快照拉取（"每管道一个对话，完就断"——用户设计） ===
//
// tail-only 下 daemon 不再重放历史；每个已宣告会话的完整历史由这里经**独立
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
const SNAPSHOT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

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

/// B2：全局 tmux 状态账本 origin → 最新 `tmux ls` 原文（daemon `TmuxSessions` 帧推来）。写者 = 各主机
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
/// tmux 状态（daemon 未发 / 连接刚起 / 断连已清）→ poller 本轮跳过该 origin（同「观测无效不累计缺失」）。
pub fn snapshot_tmux_by_origin() -> std::collections::HashMap<String, String> {
    tmux_raw_registry().lock().unwrap().clone()
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
/// 字段），**不看 command**——command-agnostic：`TmuxSessions` 帧最长 8s 陈旧，claude 退出瞬间那帧
/// 的 command 列可能仍是 claude，卡 `command!=claude` 会正常退出高频误判 archived、丢灰灯。故改用
/// 「claude 死」由 daemon-removed（emitter 触发边沿）判、「tmux 在」由 `@ccm_sid` present 判（claude
/// 退出后 wrapper watcher 停写但**不 unset**，session 级 option 恒 present——aya 已实测）。`NO_TMUX`/空跳过。
fn tmux_origin_for_sid(
    by_origin: &std::collections::HashMap<String, String>,
    sid: &str,
) -> Option<String> {
    for (origin, raw) in by_origin {
        if crate::tmux::parse_tmux_ls(raw)
            .iter()
            .any(|s| s.sid.as_deref() == Some(sid))
        {
            return Some(origin.clone());
        }
    }
    None
}

/// F03.2：`tmux_origin_for_sid` 的公开包装——读当前 tmux 快照后调纯函数。emitter 判 idle vs archived 用。
pub fn find_tmux_origin_for_sid(sid: &str) -> Option<String> {
    tmux_origin_for_sid(&snapshot_tmux_by_origin(), sid)
}

/// audit-fixes F03.2（D 审计②覆盖缺口）：daemon-removed 到达时的分流决策。emitter 收 removed 后
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
    /// daemon prime 时的完整行数 L（p1f 帧 `lines`）——完整性校验：快照行数
    /// < L = 中途断/daemon 报错 → 判失败重试（审计 D-I2：exit status 拿不到）。
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
    cfg: RemoteConfig,
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
        let cfg = cfg.clone();
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
                match fetch_snapshot(&q, &cfg, &item, &host_label, &replay, &app).await {
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
                origin: Some(host_label.clone()),
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

/// 判定快照流的一行是否计入行号（**必须与 daemon `read_new_lines` 一字一致**：
/// BOM + 全空白的行跳过且不消耗 seq——两路 seq 同处行号空间的前提）。
fn snapshot_line_countable(line: &str) -> bool {
    !line.trim_start_matches('\u{feff}').trim().is_empty()
}

/// 拉取单个会话的完整历史快照并灌进既有管线。
///
/// 读取是 **fill_buf 字节级**（审计 D-I3/S2/S5）：超时打在"单次底层读无进展"
/// 而非整行（多 MB 的 base64 图片行在慢链路上不再假超时）；`from_utf8_lossy`
/// 解码对齐 daemon（坏字节不再让该会话历史永不可得）；EOF 处无 `\n` 的残行
/// 丢弃不计 seq（与 daemon `read_new_lines` 的 F14 语义一字一致）。
///
/// 每个 chunk 边界查取消（会话 removed / 连接断）——中止并**补偿 emit 一次
/// session-ended**：若某个已 flush 的 chunk 恰把归档 tab"见行复活"，这里把它
/// 压回 archived（审计 D-B1 僵尸复活的封口；archiveTab 幂等，重复无害）。
///
/// 完整性校验（审计 D-I2）：p1f 帧带 prime 时的行数 L——拉到的行数 < L 即
/// 判失败（daemon exit 2 时 stdout 零字节、512MB take 截断等都会在此兜住）。
async fn fetch_snapshot(
    q: &std::sync::Arc<SnapshotQueue>,
    cfg: &RemoteConfig,
    item: &SnapshotItem,
    host_label: &str,
    replay: &Arc<EventReplay>,
    app: &tauri::AppHandle,
) -> Result<FetchOutcome, String> {
    let sid = &item.sid;
    let path = &item.path;
    // Batch9-F30：尾部优先变体（p1g；快照仅在 confirmed 时运行故无兼容分支，
    // meta 解析仍留防御回退）。
    let cmd = format!(
        "{} --read-session-tail {} {SNAPSHOT_TAIL_LINES}",
        shell_quote(&cfg.daemon_path),
        shell_quote(path)
    );
    let stream = connect_and_exec_cmd(cfg, &cmd).await?;
    use tokio::io::AsyncBufReadExt;
    let mut reader = tokio::io::BufReader::new(stream);
    let mut acc: Vec<u8> = Vec::new();
    let mut total_bytes: u64 = 0;
    // Batch9-F30：arrived = 可计行到达序号；meta 到手后两段映射成行号 seq
    // （前 total-tail_from 行 = tail_from+i，其余 = i-seg1）。meta 缺失
    // （防御，理论不可达）→ seq=arrived 旧行为。
    let mut arrived: u64 = 0;
    let mut tail_map: Option<(u64, u64)> = None; // (total, tail_from)
    let mut first_countable = true;
    let mut chunk: Vec<JsonlLine> = Vec::with_capacity(SNAPSHOT_CHUNK_LINES);
    let mut cancelled = false;
    'read: loop {
        // 无进展超时：计时对象是单次底层读（fill_buf），不是一整行。
        let n = {
            let buf = tokio::time::timeout(SNAPSHOT_READ_TIMEOUT, reader.fill_buf())
                .await
                .map_err(|_| "快照读取超时（60s 无数据进展）".to_string())?
                .map_err(|e| format!("快照读取失败: {e}"))?;
            if buf.is_empty() {
                break 'read; // EOF；acc 里的无 \n 残行按 F14 语义丢弃
            }
            acc.extend_from_slice(buf);
            buf.len()
        };
        reader.consume(n);
        total_bytes += n as u64;
        if total_bytes > SNAPSHOT_MAX_BYTES {
            // 防御上限：不再继续拉（完整性校验会把截断判为失败 → toast）。
            tracing::warn!(
                "snapshot [{host_label}] {sid}: 超过 {SNAPSHOT_MAX_BYTES} 字节上限，截断"
            );
            break 'read;
        }
        // 切出 acc 中所有完整行
        while let Some(pos) = acc.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = acc.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes[..line_bytes.len() - 1]);
            let line = line.trim_end_matches('\r');
            if !snapshot_line_countable(line) {
                continue;
            }
            if first_countable {
                first_countable = false;
                if let Some(m) = parse_snapshot_meta(line) {
                    tail_map = Some(m);
                    continue;
                }
            }
            let seq = match tail_map {
                Some((total, tail_from)) => tail_seq(arrived, total, tail_from),
                None => arrived,
            };
            chunk.push(JsonlLine {
                session_id: sid.to_string(),
                path: std::path::PathBuf::from(path),
                seq,
                raw: line.to_string(),
            });
            arrived += 1;
            if chunk.len() >= SNAPSHOT_CHUNK_LINES {
                if q.is_cancelled(sid) {
                    cancelled = true;
                    break 'read;
                }
                flush_lines(replay, app, host_label, std::mem::take(&mut chunk)).await;
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
    // 完整性校验：meta.total 精确对账（F30）；无 meta 退回帧 lines 下界校验
    if let Some((total, _)) = tail_map {
        if arrived != total {
            return Err(format!(
                "快照不完整：{arrived}/{total} 行（连接中断或 daemon 报错）"
            ));
        }
    } else if let Some(expected) = item.expected_lines {
        if arrived < expected {
            return Err(format!(
                "快照不完整：{arrived}/{expected} 行（连接中断或 daemon 报错）"
            ));
        }
    }
    Ok(FetchOutcome::Done(arrived))
}

/// Batch9-F30：解析快照流首行的 meta。非 meta（普通 jsonl 行）/ 关系非法
/// （tail_from > total——自家 daemon saturating_sub 不可达，但 meta 是远端
/// 进程输出，防御式拒收退回旧编号，审计 D）→ None。
fn parse_snapshot_meta(line: &str) -> Option<(u64, u64)> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("kind")?.as_str()? != "snapshot_meta" {
        return None;
    }
    let total = v.get("total")?.as_u64()?;
    let tail_from = v.get("tail_from")?.as_u64()?;
    if tail_from > total {
        tracing::warn!("snapshot_meta 非法（tail_from {tail_from} > total {total}），按旧格式处理");
        return None;
    }
    Some((total, tail_from))
}

/// 两段编号映射（纯函数，与测试共用——审计 D：原测试在测试体内重实现映射，
/// 锤不到生产代码）：到达序 → 行号。前 total-tail_from 行是尾段（最新），
/// 其余是头段回填。调用方保证 tail_from <= total（parse_snapshot_meta 校验）。
fn tail_seq(arrived: u64, total: u64, tail_from: u64) -> u64 {
    let seg1 = total.saturating_sub(tail_from);
    if arrived < seg1 {
        tail_from + arrived
    } else {
        arrived - seg1
    }
}

/// [`connect_and_exec`] 的通用形态：exec 任意命令行（issue #16：历史查询走
/// `<daemon_path> --list-projects` 等一次性命令，与流式 daemon 同一连接建立逻辑、
/// 各自独立连接互不影响）。
pub async fn connect_and_exec_cmd(
    cfg: &RemoteConfig,
    cmd: &str,
) -> Result<russh::ChannelStream<client::Msg>, String> {
    // 长连接/exec 路径不 emit 分阶段事件（F46 仅测试连接路径,避免每次重连刷屏）。
    let (session, _fp) = connect_session(cfg, None, None).await?;

    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("打开 session channel 失败: {e}"))?;

    // want_reply = true：等远端确认 exec 成功再继续。
    channel
        .exec(true, cmd.as_bytes())
        .await
        .map_err(|e| format!("exec {cmd} 失败: {e}"))?;

    // into_stream 把 channel 变成 AsyncRead+AsyncWrite；读端就是 daemon stdout 流。
    Ok(channel.into_stream())
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
/// 分叉失败必须让用户看见原因，而 daemon 恰恰把原因写在 **stderr + exit 2** 上。
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

/// exec 一条命令并**收全** stdout / stderr / 退出码（见 [`RemoteExec`]）。
///
/// 与 `connect_and_exec_cmd` 一样每次独立连接（一次性查询语义），
/// 不影响长连接流路径。超时由调用方套 `tokio::time::timeout`。
///
/// `abort_marker`：stdout 里一出现这个子串就**立刻收工返回**。存在的理由只有一个 ——
/// **不认参数的旧 daemon 会掉进流模式**（长连接、永不 EOF）。老老实实收到通道关闭，
/// 就只能等调用方的超时兜底，而超时会把「daemon 版本过旧」这条最有用的诊断吞成
/// 一句「超时」。给调用方一个字符串就能提前抽身。`None` = 收到底。
pub async fn connect_and_exec_capture(
    cfg: &RemoteConfig,
    cmd: &str,
    abort_marker: Option<&str>,
) -> Result<RemoteExec, String> {
    let (session, _fp) = connect_session(cfg, None, None).await?;
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("打开 session channel 失败: {e}"))?;
    channel
        .exec(true, cmd.as_bytes())
        .await
        .map_err(|e| format!("exec {cmd} 失败: {e}"))?;

    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let mut status: Option<u32> = None;
    // EOF 之后服务端才送 exit-status，所以**不能见 Eof 就 break**；
    // 收到 Close / 通道关闭（wait 返回 None）才算结束。
    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { data } => {
                if out.len() < EXEC_CAPTURE_MAX_BYTES {
                    out.extend_from_slice(&data);
                }
                if let Some(marker) = abort_marker {
                    if String::from_utf8_lossy(&out).contains(marker) {
                        break;
                    }
                }
            }
            russh::ChannelMsg::ExtendedData { data, .. } => {
                if err.len() < EXEC_CAPTURE_MAX_BYTES {
                    err.extend_from_slice(&data);
                }
            }
            russh::ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
            russh::ChannelMsg::Close => break,
            _ => {}
        }
    }

    Ok(RemoteExec {
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
        exit_status: status,
    })
}

/// POSIX shell 单引号转义（issue #16：历史查询的路径参数经远端 shell 解析，
/// 含空格/特殊字符必须包引号；单引号本身按 `'\''` 规则逃逸）。
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// daemon→client 的一帧（解析后的 inbound 表示）。
///
/// 对应 `remote-daemon-proto::wire::Frame`（外部 `kind` tag，snake_case）。这里**不**
/// 直接 import 那个 crate（它刻意不在 workspace 里、不被 root Cargo 引用，见其 README），
/// 而是用 schema-agnostic 的方式（serde_json::Value + 读 `kind`）解析，只取 Phase-0 需要的
/// 字段。这样：协议演进（daemon 加 `build_id` / 加新 kind）不会 break 解析 —— 未知 kind /
/// 多余字段一律忽略（见 `parse_frame`）。
#[derive(Debug, Clone, PartialEq)]
pub enum InboundFrame {
    /// 握手帧：连接建立后 daemon 发一次。`v` = 协议大版本，`build_id` = daemon 构建标识
    /// （#33 版本协商捕获 + 比对），host_arch / claude_dir 用于 log 证明 daemon 真的在远端
    /// 跑起来了。多余字段仍忽略（向前兼容）。
    Hello {
        v: u64,
        build_id: String,
        host_arch: String,
        claude_dir: String,
        /// F66（#58③）：daemon 声明的能力 token 集。旧 daemon 无此字段 → 空集
        /// （保守：按最小能力集待它，不发流模式 flag）。monitor 按此决定发
        /// `--with-bg`/`--tail-only`，不再靠 build_id 精确匹配。
        capabilities: Vec<String>,
    },
    /// 一行从远端 session jsonl 尾随读到的原始行。字段语义与本地 `watcher::JsonlLine` 对齐。
    Line {
        session_id: String,
        path: String,
        seq: u64,
        raw: String,
    },
    /// 远端新出现一个 session 文件。Batch7-F24：p1e daemon 附带 pidfile 元信息
    /// （additive）；旧 daemon 缺字段 → None（保守视为交互）。
    SessionAdded {
        sid: String,
        session_kind: Option<String>,
        /// E73（additive）：attach 进去对人有没有意义。缺席 = true（存量零迁移）。
        /// 语义与来源见 `remote-daemon-proto/src/wire.rs` 的同名字段 + `doc/IPC-PROTOCOL.md` §9.3。
        attachable: Option<bool>,
        cwd: Option<String>,
        name: Option<String>,
        /// Batch8-F25：远端 jsonl 绝对路径（p1f daemon 起有值）——旁路快照用。
        path: Option<String>,
        /// Batch8 D-I2：daemon prime 时的完整行数 L（快照完整性校验）。
        lines: Option<u64>,
        /// Batch9-F27：宣告时的初始 status/waitingFor（连接建立灯就对）。
        status: Option<String>,
        waiting_for: Option<String>,
    },
    /// Batch9-F27：会话 status 变化（p1g daemon；远端红绿灯）。
    SessionStatus {
        sid: String,
        status: Option<String>,
        waiting_for: Option<String>,
    },
    /// 远端一个 session 文件消失。
    SessionRemoved {
        sid: String,
        /// S0：daemon 明说的移除原因（缺字段 ⇒ [`RemovalCause::Gone`]，与旧 daemon 兼容）。
        cause: RemovalCause,
    },
    /// issue #32：远端 daemon 发送通道拥塞、丢了 `dropped` 帧（慢 SSH 管道）。
    /// monitor 收到后经 SS-F remote-health 通道提示用户可能丢实时行。
    Overflow { dropped: u64 },
    /// B2：daemon 在远端本地跑 `tmux ls` 的原始 stdout（或哨兵 `NO_TMUX`）——喂 tmux 对账，
    /// 替掉每 8s 新建 SSH 的刷屏轮询。`raw` 由 `tmux::parse_tmux_ls` 解析（`NO_TMUX`→无 tmux）。
    ///
    /// P1（zero-poll-liveness）：`observation` = daemon 的**显式观测分类**（additive；旧 daemon
    /// 为 `None`）。分类判定见 `tmux::classify_tmux_observation`——它把「确证零会话」与
    /// 「观测失败」分开，这是修掉 §24bis 灰灯卡死的关键（空 `raw` 在旧协议里两义不可分）。
    /// P5：daemon 差分算出的**正向死亡帧**——某个 tmux 会话确定关闭了。
    /// 收到即 retire、绕过 miss 计数（快照 + miss 那条路原样保留作兜底）。
    TmuxSessionClosed { name: String },
    TmuxSessions {
        raw: String,
        observation: Option<String>,
    },
}

/// 把 daemon 发来的一行（已去掉行尾 `\n`）解析成 [`InboundFrame`]。
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
            // #33：捕获 build_id 做版本协商（既有 daemon 一直在发，故按必需字段解析）。
            let build_id = obj.get("build_id")?.as_str()?.to_string();
            let host_arch = obj.get("host_arch")?.as_str()?.to_string();
            let claude_dir = obj.get("claude_dir")?.as_str()?.to_string();
            // F66（#58③，additive）：旧 daemon 无 `capabilities` 字段 → 空集（保守缺省，
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
            Some(InboundFrame::Hello {
                v,
                build_id,
                host_arch,
                claude_dir,
                capabilities,
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
            // Batch7-F24 附加字段（旧 daemon 缺失 → None）
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
            })
        }
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
            // ★ S0（additive）：`cause` 缺省 = `Gone`，旧 daemon 原样工作。
            // **双写点**：字面量与 daemon `remote-daemon-proto/src/wire.rs::RemovalCause`
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
            Some(InboundFrame::Overflow { dropped })
        }
        // P5：additive 新帧。缺 `name` / 非字符串 → 坏帧跳过（不 panic），
        // 与其余帧同一口径。**旧 daemon 不发它** ⇒ 这条分支永不命中，行为退回快照+miss。
        "tmux_session_closed" => {
            let name = obj.get("name")?.as_str()?.to_string();
            Some(InboundFrame::TmuxSessionClosed { name })
        }
        "tmux_sessions" => {
            // B2：raw = tmux ls 原文（或 NO_TMUX）。缺/非字符串 → 坏帧跳过。
            let raw = obj.get("raw")?.as_str()?.to_string();
            // P1：observation 是 additive 可选字段——**缺失/非字符串都当 None**（不是坏帧）。
            // 旧 daemon 没有它；非字符串是坏 daemon，退化成旧判据即今天的保守行为。
            let observation = obj
                .get("observation")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            Some(InboundFrame::TmuxSessions { raw, observation })
        }
        // 未知 kind：向前兼容，跳过（调用方 warn）。绝不 panic。
        _ => None,
    }
}

// ============================================================================
// 版本协商（issue #33）：连接时比对 daemon 的 hello.v / hello.build_id。
// ============================================================================

/// 本 monitor 期望的流式 wire 协议大版本。与 daemon 的 `PROTO_VERSION` 对齐（语义同值）。
/// 类型用 `u64` 而非 daemon 侧的 `u32`：JSON 数字无符号宽度之分，`parse_frame` 用
/// `as_u64()` 读 `v`，这里与之同宽以便直接比较，无需转换。
const EXPECTED_PROTO_V: u64 = 1;

/// 本 monitor 期望的 daemon build_id。
///
/// **SS-B（issue #33/#29）已单源**：值来自编译期 env `DAEMON_BUILD_ID`，由 `build.rs` 从
/// `remote-daemon-proto/src/main.rs::BUILD_ID` 抠出 emit——与 daemon 源码、F08b 内嵌二进制的
/// build_id **同一事实源**，无需手工同步（F08b 消除了 F06 时的手工同步债）。
const EXPECTED_DAEMON_BUILD_ID: &str = env!("DAEMON_BUILD_ID");

/// F66（#58③）：monitor **内嵌** daemon 声明的能力 token（= daemon `main.rs::CAPABILITIES`）。
///
/// 用途：部署侧确认「装的是当前内嵌 build」（`confirmed_build == EXPECTED_DAEMON_BUILD_ID`）
/// 时，第一次连接还没收到 hello，用这份常量预知 daemon 能力、直接发对应 flag——省一轮
/// 「降级→收 hello→重连升级」的往返（等价旧 build_id 门控的乐观路径，但换成能力粒度）。
/// 收到真实 hello 后一律以 daemon **自报**的 `capabilities` 为准（见 `hello_confirmed`）。
///
/// **单一事实源（SS-B，同 `EXPECTED_DAEMON_BUILD_ID`）**：值来自 `build.rs::emit_daemon_
/// capabilities` 从 daemon `main.rs::CAPABILITIES` 抠出的编译期 env `DAEMON_CAPABILITIES`
/// （逗号分隔）——**不再手抄**（审计 B1/S1：手抄副本漂移时乐观路径可能声明当前 daemon
/// 不剥离的 flag → §26 死循环窄窗；单源杜绝之）。
fn embedded_daemon_capabilities() -> Vec<String> {
    env!("DAEMON_CAPABILITIES")
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
    /// 协议版本相同，但 build_id 不同 —— daemon 偏旧/偏新，建议更新（非阻断，F08 将自动重推）。
    StaleBuild { reported: String },
    /// 协议大版本不符 —— 渲染可能异常，醒目提示需更新 daemon（仍不 hard-disconnect：
    /// 解析器向前兼容，能解析的仍照常呈现）。
    Incompatible { reported_v: u64 },
}

/// 纯函数版本协商：协议版本优先于 build_id（协议不兼容是更严重的问题）。
///
/// - `reported_v != EXPECTED_PROTO_V` → `Incompatible`（无论 build_id）。
/// - 协议同、`reported_build_id != EXPECTED_DAEMON_BUILD_ID` → `StaleBuild`。
/// - 全同 → `Ok`。
fn negotiate_version(reported_v: u64, reported_build_id: &str) -> VersionVerdict {
    if reported_v != EXPECTED_PROTO_V {
        VersionVerdict::Incompatible { reported_v }
    } else if reported_build_id != EXPECTED_DAEMON_BUILD_ID {
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
            "远端 [{label}] daemon 版本 {reported} 与本机期望 {EXPECTED_DAEMON_BUILD_ID} 不一致，建议更新 daemon（后续将支持自动部署）。"
        )),
        VersionVerdict::Incompatible { reported_v } => Some(format!(
            "远端 [{label}] daemon 协议版本 v={reported_v} 与本机期望 v={EXPECTED_PROTO_V} 不兼容，渲染可能异常，请更新 daemon。"
        )),
    }
}

/// SSH-remote 数据源主循环（S5）。
///
/// 连接远端、exec daemon、把 daemon stdout 的 line-delimited JSON 帧逐行解析后分发：
/// - `hello` → log（证明 daemon runtime 起来了）+ 置 `connected`（标记本次连接已健康，
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
        "ssh_source connecting to {}@{}:{} (daemon={})",
        cfg.user,
        cfg.host,
        cfg.port,
        cfg.daemon_path
    );

    // FIX 2（issue #15 review）：跟踪当前**已向前端宣告**的远端 sid。stream_loop 在每条
    // SessionAdded forward 时 insert、SessionRemoved forward 时 remove。无论 stream_loop
    // 因 EOF / 读错误 / connect 失败 / 正常返回哪条路径退出，下面都把 announced 里**仍存活**
    // 的 sid 一次性当 removed flush 出去 → lib.rs 的 remote-session-emitter emit SESSION_ENDED
    // → 对应远端 Tab 归档，不再永久卡在虚假 "live"。announced 每轮重连都新建（fresh per
    // iteration），故只归档本次连接残留。
    //
    // 重连循环：每轮跑一次 stream_loop。失败/掉线后按指数退避（2→4→8→16→30s 封顶）重连；
    // 本轮**连上过**（收到 daemon hello，connected=true）则下次立即以 MIN 快速重连。
    // INVARIANT §10：唯一的等待是 tokio::time::sleep（async、非阻塞），绝不 std::thread::sleep。
    let mut backoff = RECONNECT_MIN;
    // v2.22.1 hello 自愈账本:上一轮 hello 自证 daemon==当前版本时记账,下一轮以此
    // 越过「部署侧确认失败」的降级(内嵌清单缺失的 CI 安装包 v2.19-v2.22 全中招)。
    // 若带 flag 的一轮连 hello 都没收到(真·旧 daemon 把未知参数当一次性查询退出),
    // 清账回退降级,防止 flagged 重连死循环。
    // F66（#58③）：hello 自愈账本——存上一轮 daemon **自报的能力 token 集**（原为
    // build_id）。None = 尚未收到能力声明；Some(caps) = 下一轮据此发 flag 升级。
    // 回退清账语义（`:is_some()` 那段）不变。
    let mut hello_confirmed: Option<Vec<String>> = None;
    loop {
        connected.store(false, Ordering::Release);
        // Batch9 账本：HashSet → HashMap<sid, AnnouncedMeta>（F27 status 写回 +
        // F28 frontend-ready 重发的数据源）。归档清算语义不变（keys = 存活 sid）。
        let mut announced: std::collections::HashMap<String, AnnouncedMeta> =
            std::collections::HashMap::new();
        // Batch14-F59：daemonless 降级读取(per-host 开关)。顶层二选一——default
        // false 走现有 daemon 流路径(stream_loop,一行不动);true 走纯 exec tail 轮询
        // (daemonless_stream_loop)。两路共用 run() 的 announced 断连归档(FIX 2)+
        // announced_registry 清本 host(1597)+ connected 退避语义;hello_confirmed
        // 自愈账本仅 daemon 路径写(daemonless 恒 None,那段 check 天然跳过)。
        let result = if cfg.daemonless {
            daemonless_stream_loop(
                &cfg,
                &replay,
                &app,
                &session_changes,
                &connected,
                &mut announced,
            )
            .await
        } else {
            stream_loop(
                &cfg,
                &replay,
                &app,
                &session_changes,
                &connected,
                &mut announced,
                &mut hello_confirmed,
            )
            .await
        };
        if hello_confirmed.is_some() && !connected.load(Ordering::Acquire) {
            tracing::warn!("ssh_source hello 自愈轮未收到 hello,回退降级模式(daemon 可能被换旧)");
            hello_confirmed = None;
        }
        // Batch9-F28：连接结束清本 host 的 registry（断连=骨架不该再被 F5 重建；
        // 重连宣告会重新填充）。
        announced_registry()
            .lock()
            .unwrap()
            .remove(&cfg.origin_label());
        // B2：断连也清本 host 的 tmux 状态——防重连后、daemon 首个 TmuxSessions 帧到达前，
        // 对账 poller 读到陈旧 tmux 状态误灰（重连后由新帧重新填充）。
        tmux_raw_registry()
            .lock()
            .unwrap()
            .remove(&cfg.origin_label());
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
        if connected.load(Ordering::Acquire) {
            backoff = RECONNECT_MIN; // 本次连上过 → 下次立即快速重连
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
/// daemon 线协议没有批量帧（一行一帧），首连 snapshot 的几千行历史若逐帧调
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

    /// 取走全部待发行（空则 None）。到达顺序 = 发出顺序（daemon per-file seq
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
/// daemon 帧流形成天然背压。
async fn flush_lines(
    replay: &Arc<EventReplay>,
    app: &tauri::AppHandle,
    host_label: &str,
    lines: Vec<JsonlLine>,
) {
    let payloads = crate::batch_to_payloads(lines, Some(host_label.to_string()));
    replay.on_line_batch_awaited(app, payloads).await;
}

/// [`run`] 的内层流循环：connect → exec daemon → 逐帧 dispatch。**所有**提前返回
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

    // issue #29（F08）：连接前确保远端 daemon 已（自动）部署到 cfg.daemon_path。
    // 嵌入二进制就位前（F08b 未做）daemon_binary() 返回 None → ensure_daemon_deployed
    // 优雅 no-op。**best-effort**：部署失败仅 warn，不阻断——手动部署的 daemon 仍可连。
    let confirmed_build = match crate::sftp::ensure_daemon_deployed(cfg).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                "ssh_source [{host_label}] daemon 自动部署失败（继续尝试连接已有 daemon）: {e}"
            );
            None
        }
    };
    // F66（#58③）流模式门控：**从 daemon 声明的能力 token 决定发哪些 flag**，不再靠
    // build_id 精确匹配。旧 daemon 会把未知参数当一次性查询处理后退出（无 hello → 重连
    // 死循环，§26），故只对**声明了对应能力**的 daemon 发 flag（声明 = 自证会剥离该 flag）。
    // 能力两条来源，hello 自愈账本优先：
    //   ① `hello_confirmed`（上一轮 daemon **自报**的能力）—— 最权威，收过真 hello 才有。
    //   ② 否则部署侧确认了当前内嵌 build（`confirmed_build == EXPECTED`）→ 用内嵌 daemon 的
    //      能力常量**预知**，省第一轮「降级→收 hello→重连升级」往返（乐观路径）。
    //   ③ 都没有 → 空集 → 全降级（= 2.18.0 行为，连接正常、功能退化）。
    // **hello 优先**于部署侧（②可能是陈旧内嵌的身份 ≠ 期望 → 空集 → 靠 hello 自愈救，
    //  见 v2.22.1 无限重连教训；`hello_confirmed` 只在收到真声明时写入，优先采纳恒安全）。
    let caps: Vec<String> = hello_confirmed.clone().unwrap_or_else(|| {
        if confirmed_build.as_deref() == Some(EXPECTED_DAEMON_BUILD_ID) {
            embedded_daemon_capabilities()
        } else {
            Vec::new()
        }
    });
    let (with_bg, tail_only) = decide_stream_flags(&caps, crate::load_show_bg_sessions());
    let stream = connect_and_exec(cfg, with_bg, tail_only).await?;

    // Batch8-F26：旁路快照基础设施（仅 tail-only 生效；每连接一套，函数任何
    // 退出路径经 guard 关闭队列——已入队项仍会被分发器拉完，独立连接自灭）。
    let snapshots = SnapshotQueue::new();
    let _snapshots_guard = SnapshotQueueCloser(snapshots.clone());
    if tail_only {
        tauri::async_runtime::spawn(snapshot_dispatcher(
            snapshots.clone(),
            cfg.clone(),
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
    tauri::async_runtime::spawn(async move {
        let mut reader = BufReader::new(stream);
        // tokio LinesStream-free：read_line 复用 buffer，按 `\n` 切（协议保证每帧
        // 一行、帧内换行已被 daemon 转义成 `\n` 两字符，见 remote-daemon-proto/src/wire.rs）。
        let mut buf = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf).await {
                Ok(0) => {
                    // EOF：daemon 退出 / channel 关闭。明确报错，不静默冻结。
                    let _ = frame_tx
                        .send(Err(
                            "ssh daemon stdout closed (EOF / connection dropped)".to_string()
                        ))
                        .await;
                    break;
                }
                Ok(_) => {
                    let line = buf.trim_end_matches(['\n', '\r']);
                    if line.is_empty() {
                        continue;
                    }
                    if frame_tx.send(Ok(line.to_string())).await.is_err() {
                        break; // 主循环已退出（重连中）
                    }
                }
                Err(e) => {
                    let _ = frame_tx
                        .send(Err(format!("ssh daemon stdout read error: {e}")))
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
            return Err("ssh daemon frame channel closed".to_string());
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
        match frame {
            Some(InboundFrame::Hello {
                v,
                build_id,
                host_arch,
                claude_dir,
                capabilities,
            }) => {
                tracing::info!(
                    "ssh_source daemon hello: v={v} build_id={build_id} host_arch={host_arch} claude_dir={claude_dir} caps={capabilities:?}"
                );
                // 标记本次连接已健康(收到 daemon hello)，供 run() 重连循环判定是否重置退避。
                connected.store(true, Ordering::Release);
                // issue #33：版本协商。不兼容/偏旧经 SS-F remote-health 通道醒目提示（前端
                // headlineFor 已含 version case，零前端改动）。不 hard-disconnect（向前兼容）。
                if let Some(msg) = version_warning(v, &build_id, &host_label) {
                    tracing::warn!("ssh_source remote [{host_label}] version: {msg}");
                    let payload = crate::bridge::RemoteHealthPayload {
                        origin: Some(host_label.clone()),
                        kind: "version".to_string(),
                        message: msg,
                    };
                    if let Err(e) = app.emit(crate::bridge::events::REMOTE_HEALTH, payload) {
                        tracing::warn!("ssh_source remote-health (version) emit failed: {e}");
                    }
                }
                // F66（#58③）：本轮若跑在降级模式（未开 tail_only）——用 daemon **自报的
                // 能力**判断能否升级，不再靠 build_id 精确匹配（闭合 2026-07-09 事故）：
                // ① daemon 声明了**能开本轮没开的 flag** 的能力 → 记 hello 自愈账（存能力集）,
                //    立即重连升级（connected 已置 true → 退避重置 MIN,~2s 内带 flag 回来）。
                //    **防无限循环**：仅当「下一轮据此算出的 flag 严格优于本轮」才重连——flag 数
                //    有限（2）、每次升级严格增开，最多 2 轮收敛。
                // ② daemon 无任何能力声明（真旧 daemon）→ 降级可见化（否则用户看到「bg 会话
                //    消失+拥塞复发」却无从归因，实测连环误诊）——经 remote-health 提示。
                if !tail_only {
                    let show_bg = crate::load_show_bg_sessions();
                    let next = decide_stream_flags(&capabilities, show_bg);
                    if should_upgrade_reconnect((with_bg, tail_only), next) {
                        *hello_confirmed = Some(capabilities.clone());
                        return Err(format!(
                            "daemon hello 声明能力({capabilities:?})——重连升级流模式(tail-only/with-bg)"
                        ));
                    }
                    if capabilities.is_empty() {
                        let payload = crate::bridge::RemoteHealthPayload {
                            origin: Some(host_label.clone()),
                            kind: "degraded".to_string(),
                            message: format!(
                                "远端 daemon 为旧版本({build_id},当前 {EXPECTED_DAEMON_BUILD_ID}),本连接降级运行:后台(bg)会话不可见、历史全量推流(易拥塞)。请在设置里重装该机器的 daemon。"
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
            }) => {
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
                if let Err(e) = app.emit(crate::bridge::events::REMOTE_SESSION_ADDED, &payload) {
                    tracing::warn!("ssh_source remote-session-added emit failed: {e}");
                }
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
                // 旧 daemon → 前端"未知不加类"，与本地一字一致）。
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
                // FIX 2：已显式 removed 的 sid 从 announced 摘掉，避免连接结束时重复归档。
                announced.remove(&sid);
                if let Some(hm) = announced_registry().lock().unwrap().get_mut(&host_label) {
                    hm.remove(&sid);
                }
                // Batch8 D-B1：摘除排队中的快照 + 标记 inflight 取消——归档后
                // 迟到的快照行会经"见行复活"造出关不掉的僵尸 live tab。
                snapshots.cancel(&sid);
                if let Err(e) = session_changes.send(SessionChange {
                    added: vec![],
                    // ★ S0：cause 由 daemon 说了算，monitor 不猜（原先靠查会陈旧的 tmux 快照）。
                    removed: vec![RemovedSid { sid, cause }],
                    status_changed: vec![],
                }) {
                    tracing::warn!("ssh_source session_removed send failed: {e}");
                }
            }
            Some(InboundFrame::Overflow { dropped }) => {
                // issue #32：远端管道拥塞丢了 dropped 帧。warn + 经 SS-F remote-health
                // 通道提示用户（前端按 origin 节流弹 toast）。丢的实时行仍在远端 jsonl
                // 文件里，重开该会话即可看完整历史（不做实时补齐，见计划 R5）。
                tracing::warn!(
                    "ssh_source remote [{host_label}] overflow: daemon dropped {dropped} frame(s)"
                );
                let payload = crate::bridge::RemoteHealthPayload {
                    origin: Some(host_label.clone()),
                    kind: "overflow".to_string(),
                    message: format!(
                        "远端 [{host_label}] 管道拥塞，可能丢失约 {dropped} 条实时行；重开该会话可看完整历史。"
                    ),
                };
                if let Err(e) = app.emit(crate::bridge::events::REMOTE_HEALTH, payload) {
                    tracing::warn!("ssh_source remote-health emit failed: {e}");
                }
            }
            // P5：正向死亡帧 —— daemon 已经**确定**这个会话没了（它与上一份快照差分算出来的），
            // 不需要 monitor 再靠「连续两次没看见」去猜。
            //
            // **为什么仍然按名字反查 sid 而不是让 daemon 带上**：`#{@ccm_sid}` 在 hook 上下文
            // 取不到（P0 实测拿到空 ⇒ 会把活会话判灰）；而 name→sid 的映射 monitor 这边本来
            // 就有（最新那份 `tmux ls` 原文）。让知道的人去查，比让不知道的人硬传更稳。
            //
            // **快照路径与 `RETIRE_MISS_THRESHOLD` 原样保留**：重同步 / 旧 daemon 降级都靠它。
            // 同一 sid 两条路都可能到 ⇒ retire 必须幂等（`SidTrack.retired` 本就是）。
            Some(InboundFrame::TmuxSessionClosed { name }) => {
                let sid = {
                    let reg = tmux_raw_registry().lock().unwrap();
                    reg.get(&host_label).and_then(|raw| {
                        crate::tmux::parse_tmux_ls(raw)
                            .into_iter()
                            .find(|e| e.name == name)
                            .and_then(|e| e.sid)
                    })
                };
                match sid {
                    Some(sid) => {
                        tracing::info!(
                            "tmux 会话 {name} 关闭（daemon 死亡帧）⇒ 立刻 retire sid={sid}"
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
                // daemon 每 ~8s 推本 origin 最新 `tmux ls` 原文；收到即对账——把 tmux 后端已消失的 tracked
                // sid 去抖 retire → 当 removed 送 emitter（emitter 再判 None→archived+clear_idle）。tracked =
                // 本连接 announced（live 会话）∪ 本 origin idle 会话（后者使 idle→archived 有产出者，补齐红线④）。
                //
                // P1（zero-poll-liveness）：原先这里内联着
                // `if raw.trim() != "NO_TMUX" { … if !backend.is_empty() { … } }`——**把五种语义
                // 不同的观测压成两条路**，其中「daemon 确证零会话」被误并进「观测失败」一律跳过
                // ⇒ 杀掉某 origin 最后一个 tmux 会话时灰灯卡到断连（§24bis 预登记的残留 bug）。
                // 现在判断提成纯函数 `tmux::classify_tmux_observation`（可 CI 单测，生产与测试
                // 同一条路径），空集也是**有效观测**、照常累计缺失。
                if let crate::tmux::TmuxObservation::Backend(backend) =
                    crate::tmux::classify_tmux_observation(&raw, observation.as_deref())
                {
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
                tmux_raw_registry()
                    .lock()
                    .unwrap()
                    .insert(host_label.clone(), raw);
            }
            None => {
                // 未知 kind / 坏帧 / 非 JSON：跳过，绝不 panic、绝不中断流。
                tracing::warn!("ssh_source skipping unparseable/unknown frame: {line}");
            }
        }
    }
}

// ============================================================================
// Batch14-F59：daemonless 降级读取——无 daemon 时纯 SSH exec `find`+`tail -c +offset`
// 轮询读会话 jsonl,复用 flush_lines→batch_to_payloads→emit 下游,绕开 daemon 线协议。
// 触发 = per-host `cfg.daemonless`(run() 顶层二选一)。能力子集:无 bg kind/无状态灯/
// 无拥塞信号/仅最近活跃会话(mtime 窗口近似 live)。
// ============================================================================

/// 轮询间隔(实时性 vs 负载权衡;同 aterm 量级)。
const DAEMONLESS_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// 「最近活跃」窗口(**分钟**,喂 find `-mmin`):只发现 mtime 在此窗口内有写入的 jsonl,
/// 近似 live 会话——避免把该主机**所有历史会话**灌成 Tab(daemon 靠 pidfile 精确判活,
/// daemonless 无)。
const DAEMONLESS_ACTIVE_WINDOW_MINUTES: u32 = 30;
/// 单批行数上限(与 flush 下游 BATCH_CAP 同量级)。
const DAEMONLESS_CHUNK_LINES: usize = 600;
/// 单文件单轮读字节上限:超大历史会话切多轮读(下轮从 consumed 续),防一轮拉 GB。
/// ⚠ **行粒度**:consumed 只在整行 drain 后自增,故单条 >CAP 的巨行仍一轮全读(内存尖峰
/// ≈ 行长,与 `watcher` 的 `read_until` 同源取舍);CAP 限的是「一轮读多少条完整行的字节」。
const DAEMONLESS_READ_CAP: u64 = 8 * 1024 * 1024;
/// 发现命令 stdout 上限(输出很小:每活跃会话一行 `<bytes> <path>`)。
const DAEMONLESS_DISCOVER_CAP: usize = 4 * 1024 * 1024;

/// F59 增量读游标(= 客户端 `watcher::FileCursor` 同语义)。**刻意本地复刻而非共享**:
/// `FileCursor` 是 watcher 私有类型,为它跨模块导出会把 watcher 内部实现细节耦合进 ssh_source;
/// 这里是 2 字段的小 POD + `plan_file_read`/`drain_complete_lines` 已单测,复刻成本可忽略。
/// `consumed` = 已消费(完整行)字节 offset;`seen_len` = 见过的最大文件长度(截断判定)。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct DlCursor {
    consumed: u64,
    seen_len: u64,
}

/// F59(纯函数,单测):解析 `wc -c <file>` 一行 → (bytes, path)。形如 `  1234 /a/b.jsonl`
/// (wc 前导空白对齐);`trim_start` 后按**首个空白** `split_once`,右侧整体是 path(容路径含
/// 空格)。无数字 / 空路径 → None。
fn parse_wc_line(line: &str) -> Option<(u64, String)> {
    let s = line.trim_start();
    let (num, rest) = s.split_once(char::is_whitespace)?;
    let bytes: u64 = num.parse().ok()?;
    let path = rest.trim();
    if path.is_empty() {
        return None;
    }
    Some((bytes, path.to_string()))
}

/// F59(纯函数,单测):从远端 jsonl 路径(POSIX `/` 分隔)取 session_id = 文件名去 `.jsonl`。
fn jsonl_sid(path: &str) -> Option<String> {
    path.rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".jsonl"))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// F59(纯函数,单测):给定游标 + 当前文件大小,算增量读起点 + 是否截断(与
/// `watcher::process_file` 一字一致)。`truncated = size < seen_len` → 从 0 全量重读
/// (seq **不重置**——前端按更大 seq 排在旧行后、uuid 幂等);否则从 `consumed` 续读。
fn plan_file_read(cursor: DlCursor, size: u64) -> (u64, bool) {
    let truncated = size < cursor.seen_len;
    let start = if truncated { 0 } else { cursor.consumed };
    (start, truncated)
}

/// F59(纯函数,单测——头号雷区的可测核心;抽出以对齐 daemon 侧 `read_new_lines` 的纯函数
/// 设计并直测)。从字节累加器 `acc` 排出所有 `\n` 结尾的**完整行**,每行分配 `*next_seq`
/// 递增 seq;返回 `(每行 (seq, raw), 本次消费字节)`。
/// - **torn tail**(末尾无 `\n` 的残字节)**留在 acc、不消费、不产行**(下次 fill 补全 / EOF 丢弃)。
/// - 完整行(**含空行**)的字节都计入 consumed(推进 offset);但空/BOM/纯空白行
///   (`!snapshot_line_countable`)**跳过、不产行、不占 seq**(与 `watcher::process_file` 一字一致)。
/// - 剥行尾 `\n` 及可选 `\r`;`from_utf8_lossy` 解码。
/// - `next_seq` 以 `&mut` 传入 → 跨多次调用(多次 fill_buf / 多轮 poll)**连续单调、不回退**。
fn drain_complete_lines(acc: &mut Vec<u8>, next_seq: &mut u64) -> (Vec<(u64, String)>, u64) {
    let mut out: Vec<(u64, String)> = Vec::new();
    let mut consumed: u64 = 0;
    while let Some(pos) = acc.iter().position(|&b| b == b'\n') {
        let line_bytes: Vec<u8> = acc.drain(..=pos).collect();
        consumed += line_bytes.len() as u64; // 完整行(含空行)均推进 offset
        let mut end = line_bytes.len() - 1; // 剥 \n
        if end > 0 && line_bytes[end - 1] == b'\r' {
            end -= 1; // 剥 \r
        }
        let line = String::from_utf8_lossy(&line_bytes[..end]).into_owned();
        if !snapshot_line_countable(&line) {
            continue; // 空/BOM/纯空白:推进 offset 但不占 seq、不产行
        }
        out.push((*next_seq, line));
        *next_seq += 1;
    }
    (out, consumed)
}

/// F59:在**已建立**的持久会话上开一条 channel exec 命令,返回 stdout 流。与
/// [`connect_and_exec_cmd`] 的区别:后者每次新建 SSH 连接(一次性历史查询用),daemonless
/// 轮询须复用同一会话省握手 → 只开 channel。
async fn exec_on_session(
    session: &SshSession,
    cmd: &str,
) -> Result<russh::ChannelStream<client::Msg>, String> {
    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("打开 session channel 失败: {e}"))?;
    channel
        .exec(true, cmd.as_bytes())
        .await
        .map_err(|e| format!("exec {cmd} 失败: {e}"))?;
    Ok(channel.into_stream())
}

/// F59:exec 命令、读全部 stdout 为 String(带上限防失控)。发现命令输出很小。
async fn read_to_string_capped(session: &SshSession, cmd: &str) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let stream = exec_on_session(session, cmd).await?;
    let mut reader = tokio::io::BufReader::new(stream);
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        // 无进展超时(60s):与快照路径统一,挂起(半死 TCP 无 RST)不干等 SSH keepalive(~90s)。
        let n = tokio::time::timeout(SNAPSHOT_READ_TIMEOUT, reader.read(&mut tmp))
            .await
            .map_err(|_| "daemonless 发现读取超时(60s 无进展)".to_string())?
            .map_err(|e| format!("读发现输出失败: {e}"))?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() > DAEMONLESS_DISCOVER_CAP {
            return Err(format!("发现输出超过 {DAEMONLESS_DISCOVER_CAP} 字节上限"));
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// F59:读一个会话文件从 `start` 字节起的增量内容,组 `JsonlLine` 切块 `flush_lines`。
/// 返回 `(本次消费完整行字节, torn_tail 字节, 新 next_seq)`。**只消费 `\n` 结尾的完整行**;
/// torn tail(EOF/cap 处无 `\n` 残行)不消费只计字节;空/BOM/纯空白行跳过不占 seq(与
/// `watcher`/daemon 一字一致);单轮读满 [`DAEMONLESS_READ_CAP`] 即停(余量下轮续读)。
#[allow(clippy::too_many_arguments)]
async fn read_incremental(
    session: &SshSession,
    path: &str,
    start: u64,
    sid: &str,
    mut next_seq: u64,
    replay: &Arc<EventReplay>,
    app: &tauri::AppHandle,
    host_label: &str,
) -> Result<(u64, u64, u64), String> {
    use tokio::io::AsyncBufReadExt;
    // `tail -c +N` 是 **1-based 字节**:跳过前 `start` 字节 → `+(start+1)`。off-by-one 头号雷。
    // `2>/dev/null` 弃 stderr(文件竞态删/权限报错不混入解析;当前 russh `into_stream` 只透
    // stdout,此为防御未来若合流 stderr)。
    let cmd = format!("tail -c +{} {} 2>/dev/null", start + 1, shell_quote(path));
    let stream = exec_on_session(session, &cmd).await?;
    let mut reader = tokio::io::BufReader::new(stream);
    let mut acc: Vec<u8> = Vec::new();
    let mut consumed: u64 = 0;
    let mut chunk: Vec<JsonlLine> = Vec::with_capacity(DAEMONLESS_CHUNK_LINES);
    'read: loop {
        let n = {
            // 无进展超时(60s):与快照路径统一,挂起不干等 SSH keepalive(~90s)。
            let buf = tokio::time::timeout(SNAPSHOT_READ_TIMEOUT, reader.fill_buf())
                .await
                .map_err(|_| "daemonless tail 读取超时(60s 无进展)".to_string())?
                .map_err(|e| format!("tail 读取失败: {e}"))?;
            if buf.is_empty() {
                break 'read; // EOF
            }
            acc.extend_from_slice(buf);
            buf.len()
        };
        reader.consume(n);
        // 纯函数排出所有完整行 + 推进 seq(torn tail 留 acc);切块 flush 保留在 async 侧。
        let (lines, c) = drain_complete_lines(&mut acc, &mut next_seq);
        consumed += c;
        for (seq, raw) in lines {
            chunk.push(JsonlLine {
                session_id: sid.to_string(),
                path: std::path::PathBuf::from(path),
                seq,
                raw,
            });
            if chunk.len() >= DAEMONLESS_CHUNK_LINES {
                flush_lines(replay, app, host_label, std::mem::take(&mut chunk)).await;
            }
        }
        if consumed >= DAEMONLESS_READ_CAP {
            // 单轮读满:停(下轮从 start+consumed 续读);acc 残留 = torn tail。
            break 'read;
        }
    }
    let tail_bytes = acc.len() as u64; // EOF/cap 处无 \n 残行:不消费,只计 seen_len
    if !chunk.is_empty() {
        flush_lines(replay, app, host_label, chunk).await;
    }
    Ok((consumed, tail_bytes, next_seq))
}

/// F59:每连接 emit 一次 degraded 健康提示(复用 SS-F remote-health 通道;前端
/// `remote-health.ts` 已有 `case "degraded"`,零前端改动)。诚实列能力缺口。
fn emit_degraded(app: &tauri::AppHandle, host_label: &str) {
    let payload = crate::bridge::RemoteHealthPayload {
        origin: Some(host_label.to_string()),
        kind: "degraded".to_string(),
        message: format!(
            "远端 [{host_label}] daemonless 降级读取:后台会话不可见 / 无运行状态灯 / \
             无拥塞信号 / 仅显示最近活跃会话(空闲久的暂隐)。会话内容正常。"
        ),
    };
    if let Err(e) = app.emit(crate::bridge::events::REMOTE_HEALTH, payload) {
        tracing::warn!("daemonless [{host_label}] degraded emit failed: {e}");
    }
}

/// F59:宣告一个 daemonless 会话骨架(降级:kind/cwd/name 不可知——cwd 前端从后续行
/// payload 补)。镜像 daemon 路径:emit REMOTE_SESSION_ADDED + 入 announced(FIX 2 断连
/// 归档源)+ 入全局 announced_registry(F28 F5 重宣告源)+ send SessionChange added。
fn announce_daemonless(
    sid: &str,
    host_label: &str,
    announced: &mut std::collections::HashMap<String, AnnouncedMeta>,
    session_changes: &std::sync::mpsc::Sender<SessionChange>,
    app: &tauri::AppHandle,
) {
    let payload = crate::bridge::RemoteSessionAddedPayload {
        session_id: sid.to_string(),
        origin: host_label.to_string(),
        kind: None,
        // 这条路是「daemon 之外的兜底宣告」（无 pidfile 元信息）⇒ 一律 None = 照旧可 attach。
        attachable: None,
        cwd: None,
        name: None,
    };
    let meta = AnnouncedMeta {
        payload: payload.clone(),
        status: None,
        waiting_for: None,
    };
    // 先入 registry 再 emit(与 daemon 路径同序:反序时 F5 落中间不丢)。
    announced_registry()
        .lock()
        .unwrap()
        .entry(host_label.to_string())
        .or_default()
        .insert(sid.to_string(), meta.clone());
    if let Err(e) = app.emit(crate::bridge::events::REMOTE_SESSION_ADDED, &payload) {
        tracing::warn!("daemonless [{host_label}] session-added emit failed: {e}");
    }
    announced.insert(sid.to_string(), meta);
    if let Err(e) = session_changes.send(SessionChange {
        added: vec![sid.to_string()],
        removed: vec![],
        status_changed: vec![],
    }) {
        tracing::warn!("daemonless [{host_label}] session_added send failed: {e}");
    }
}

/// F59:归档掉出活跃窗口/消失的 daemonless 会话(镜像 SessionRemoved 臂:摘 announced +
/// registry + send removed)。再有写入 → 下轮重新首见、Tab 回来。
fn archive_daemonless(
    sids: Vec<String>,
    host_label: &str,
    announced: &mut std::collections::HashMap<String, AnnouncedMeta>,
    session_changes: &std::sync::mpsc::Sender<SessionChange>,
) {
    if sids.is_empty() {
        return;
    }
    for sid in &sids {
        announced.remove(sid);
    }
    if let Some(hm) = announced_registry().lock().unwrap().get_mut(host_label) {
        for sid in &sids {
            hm.remove(sid);
        }
    }
    if let Err(e) = session_changes.send(SessionChange {
        added: vec![],
        removed: sids.into_iter().map(RemovedSid::gone).collect(),
        status_changed: vec![],
    }) {
        tracing::warn!("daemonless [{host_label}] 归档 send failed: {e}");
    }
}

/// [`run`] 的 daemonless 分支:连一次持久 SSH 会话 → 轮询(`find` 发现最近活跃 jsonl +
/// 对增长文件 `tail -c +offset` 增量读)→ 复用 [`flush_lines`] 下游 → announced/session_changes
/// 生命周期。exec 错/EOF(会话死)冒泡 Err 交 [`run`] 按退避重连。**所有提前返回都把 result
/// 交 run() 统一归档 announced 残留**(同 stream_loop 契约,故本函数自身不做最终归档)。
async fn daemonless_stream_loop(
    cfg: &RemoteConfig,
    replay: &Arc<EventReplay>,
    app: &tauri::AppHandle,
    session_changes: &std::sync::mpsc::Sender<SessionChange>,
    connected: &Arc<AtomicBool>,
    announced: &mut std::collections::HashMap<String, AnnouncedMeta>,
) -> Result<(), String> {
    let host_label = cfg.origin_label();
    // 连一次,持久复用(自动继承 F45 竞速 / F56 跳板)。会话随本函数栈存活;返回即 drop。
    let (session, _fp) = connect_session(cfg, None, None).await?;

    // 发现命令:最近活跃窗口内的会话 jsonl + 字节数。POSIX `find`+`wc`+`-mmin`(可移植——
    // daemonless 主机常是装不了 daemon 的异构/BSD/macOS,故不用 GNU `-printf`)。`$HOME`
    // 由远端登录 shell 展开;固定命令无用户输入注入面。`wc -c … \;`(非 `+`)每文件独立
    // 调用 → 无 total 汇总行。stderr 弃(无权限目录/竞态删文件不噪)。
    // batch20 审计修:尊重 `CLAUDE_CONFIG_DIR`(与 daemon 路径 + `fetch_remote_claude_json` 口径一致)——
    // 原硬编码 `$HOME/.claude/projects` 会让重定位过 claude 目录的主机在 daemonless 模式下静默零结果。
    let discover_cmd = format!(
        "find \"${{CLAUDE_CONFIG_DIR:-$HOME/.claude}}/projects\" -type f -name '*.jsonl' ! -path '*/subagents/*' \
         -mmin -{DAEMONLESS_ACTIVE_WINDOW_MINUTES} -exec wc -c {{}} \\; 2>/dev/null"
    );

    // per-sid 跨轮状态。
    let mut cursors: std::collections::HashMap<String, DlCursor> = std::collections::HashMap::new();
    let mut seqs: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut first = true;

    loop {
        // 1. 发现(会话死 → Err 冒泡 → run() 重连)。
        let listing = read_to_string_capped(&session, &discover_cmd).await?;
        if first {
            // 首次成功 exec = 连上(替代 daemon hello 的角色:让 run() 退避重置/最终归档
            // 语义正常)。绕过 hello 自愈账本(那是 daemon flag 协商专属)。
            connected.store(true, Ordering::Release);
            emit_degraded(app, &host_label);
            first = false;
        }
        // 解析活跃集 sid → (size, path)。
        let mut current: std::collections::HashMap<String, (u64, String)> =
            std::collections::HashMap::new();
        for line in listing.lines() {
            if let Some((size, path)) = parse_wc_line(line) {
                if let Some(sid) = jsonl_sid(&path) {
                    current.insert(sid, (size, path));
                }
            }
        }
        // 2. 归档掉出窗口/消失的(曾 announced 但本轮不在 current)。
        let gone: Vec<String> = announced
            .keys()
            .filter(|sid| !current.contains_key(*sid))
            .cloned()
            .collect();
        for sid in &gone {
            cursors.remove(sid);
            seqs.remove(sid);
        }
        archive_daemonless(gone, &host_label, announced, session_changes);
        // 3. 每个活跃会话:新 → 宣告骨架 + 首见全量读;旧 → 增量读。
        for (sid, (size, path)) in &current {
            if !announced.contains_key(sid) {
                announce_daemonless(sid, &host_label, announced, session_changes, app);
            }
            let cursor = cursors.get(sid).copied().unwrap_or_default();
            let (start, truncated) = plan_file_read(cursor, *size);
            if truncated {
                tracing::warn!(
                    "daemonless [{host_label}] {sid} 截断(size {size} < seen {}),全量重读",
                    cursor.seen_len
                );
            }
            if start >= *size {
                // 无新字节;截断到空需即时重置游标(否则重新长回 ≥ 旧 consumed 时漏检)。
                if truncated {
                    cursors.insert(
                        sid.clone(),
                        DlCursor {
                            consumed: 0,
                            seen_len: *size,
                        },
                    );
                }
                continue;
            }
            let next_seq = seqs.get(sid).copied().unwrap_or(0);
            // 单文件读失败不杀整轮(文件可能刚被删/竞态);记日志跳过。真·会话死由下轮
            // 发现 exec 失败兜住 → Err → 重连。
            let (read_bytes, tail_bytes, new_seq) = match read_incremental(
                &session,
                path,
                start,
                sid,
                next_seq,
                replay,
                app,
                &host_label,
            )
            .await
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("daemonless [{host_label}] {sid} 增量读失败: {e}");
                    continue;
                }
            };
            seqs.insert(sid.clone(), new_seq);
            let consumed = start + read_bytes;
            cursors.insert(
                sid.clone(),
                DlCursor {
                    consumed,
                    // 高点取 max:size 是 find 快照,可能 < 读中真实增长。
                    seen_len: (*size).max(consumed + tail_bytes),
                },
            );
        }
        // 4. 轮询间隔(唯一等待 = async sleep,INVARIANT §10)。
        tokio::time::sleep(DAEMONLESS_POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod f032_idle_tests {
    use super::*;
    use std::collections::HashMap;

    // TMUX_LS_FMT: name\tpath\tcommand\tattached\twindows\t@ccm_sid（6 列真 TAB 分隔）
    fn raw(name: &str, cmd: &str, sid: &str) -> String {
        format!("{name}\t/p\t{cmd}\t0\t1\t{sid}")
    }

    #[test]
    fn tmux_origin_for_sid_matches_ccm_sid_column_command_agnostic() {
        // @ccm_sid==目标 + command=bash（claude 已退）→ 命中 origin
        let by = HashMap::from([("A".to_string(), raw("cc-x", "bash", "target-full"))]);
        assert_eq!(
            tmux_origin_for_sid(&by, "target-full"),
            Some("A".to_string())
        );
        // command-agnostic：command 仍是 claude（帧陈旧）也命中——**变异锚点**：改看 command 则此断言红
        let by2 = HashMap::from([("A".to_string(), raw("cc-x", "claude", "target-full"))]);
        assert_eq!(
            tmux_origin_for_sid(&by2, "target-full"),
            Some("A".to_string())
        );
    }

    #[test]
    fn tmux_origin_for_sid_only_ccm_sid_column_not_name_or_command() {
        // sid 只作 name/command 出现、@ccm_sid 列是别的 → 不命中（不误把 name/command 当 sid）
        let by = HashMap::from([("A".to_string(), raw("target", "target", "other-sid"))]);
        assert_eq!(tmux_origin_for_sid(&by, "target"), None);
    }

    #[test]
    fn tmux_origin_for_sid_no_tmux_empty_missing() {
        let by = HashMap::from([
            ("A".to_string(), "NO_TMUX".to_string()),
            ("B".to_string(), String::new()),
        ]);
        assert_eq!(tmux_origin_for_sid(&by, "x"), None);
    }

    #[test]
    fn tmux_origin_for_sid_multi_origin_routes() {
        let by = HashMap::from([
            ("A".to_string(), raw("cc-a", "bash", "sid-a")),
            ("B".to_string(), raw("cc-b", "bash", "sid-b")),
        ]);
        assert_eq!(tmux_origin_for_sid(&by, "sid-b"), Some("B".to_string()));
        assert_eq!(tmux_origin_for_sid(&by, "sid-z"), None);
    }

    #[test]
    fn classify_removed_some_is_idle_none_is_archive() {
        // audit-fixes F03.2（D 审计②）：分流映射的变异锚点——把 Some/None 两臂写反则本测红。
        assert_eq!(
            classify_removed(Some("pi".to_string()), RemovalCause::Gone),
            RemovedDisposition::Idle {
                origin: "pi".to_string()
            }
        );
        assert_eq!(
            classify_removed(None, RemovalCause::Gone),
            RemovedDisposition::Archive
        );
    }

    /// ★ S0：`Superseded` 恒归档 —— **且不看 tmux 快照**。
    ///
    /// 这条钉的是用户 2026-07-30 实测的那个 bug：`/branch` 之后原 tab 变成一个永远
    /// 消不掉、也 attach 不上的灰点。四象限里出问题的就是 `(Some(origin), Superseded)`
    /// 这一格 —— 快照说「tmux 还在」（对的，格子确实在），但那一格已经改挂新 sid 了。
    #[test]
    fn superseded_always_archives_even_when_tmux_snapshot_still_shows_the_sid() {
        // ★ 关键格：快照有它、但它是被顶替的 ⇒ 必须归档，不能灰点。
        assert_eq!(
            classify_removed(Some("pi".to_string()), RemovalCause::Superseded),
            RemovedDisposition::Archive
        );
        // 快照里没有时当然也归档（这格两个 cause 同答案，单独列出来是为了说明
        // Superseded 的判定**与快照无关**，不是碰巧和 Gone 一致）。
        assert_eq!(
            classify_removed(None, RemovalCause::Superseded),
            RemovedDisposition::Archive
        );
        // 反向对照：同一份「快照里有」的输入，Gone 仍然是灰点 —— 证明上面第一条不是
        // 因为把灰点分支整个删了才绿的（那种"修法"会把真正的 idle-tmux 功能砸掉）。
        assert_eq!(
            classify_removed(Some("pi".to_string()), RemovalCause::Gone),
            RemovedDisposition::Idle {
                origin: "pi".to_string()
            }
        );
    }

    /// ★ S0 跨语言双写点：monitor 认的字面量必须与 daemon 发的逐字一致。
    ///
    /// 照本仓既有纪律（`TMUX_LS_FMT` / 观测取值那几条）：**读另一侧的源文件 + 锚定那一行**。
    /// 漂了的表现是**静默失效**——monitor 认不出 `cause`、退回 `Gone`、灰点 bug 悄悄复活，
    /// 而两侧各自的测试都是绿的。
    #[test]
    fn removal_cause_wire_literal_stays_in_sync() {
        let daemon_wire = include_str!("../../remote-daemon-proto/src/wire.rs");
        // 反向自检：真读到了那个文件，且它确实是那个 enum 所在的文件。
        assert!(daemon_wire.len() > 2000, "没读到 daemon wire.rs");
        assert!(
            daemon_wire.contains("pub enum RemovalCause"),
            "daemon 侧 RemovalCause 不在预期文件里，双写点锚点已失效"
        );
        // daemon 用 `#[serde(rename_all = "snake_case")]` + 变体名 `Superseded`
        // ⇒ 线上就是 "superseded"。两个锚点都钉住，任一侧改名都红。
        assert!(
            daemon_wire.contains(r#"#[serde(rename_all = "snake_case")]"#),
            "daemon 侧 RemovalCause 的 serde 命名策略变了，线上字面量可能已不是 snake_case"
        );
        assert_eq!(REMOVAL_CAUSE_SUPERSEDED, "superseded");
        assert!(
            daemon_wire.contains("    Superseded,"),
            "daemon 侧变体名 Superseded 变了 ⇒ 线上字面量跟着变，monitor 会认不出"
        );
    }

    #[test]
    fn idle_registry_mark_clear_snapshot() {
        mark_idle("f032_origX", "f032_sidX");
        assert!(snapshot_idle_for_origin("f032_origX").contains("f032_sidX"));
        assert!(snapshot_idle_by_origin().get("f032_origX").is_some());
        clear_idle("f032_sidX"); // 幂等 + 跨 origin
        assert!(!snapshot_idle_for_origin("f032_origX").contains("f032_sidX"));
        clear_idle("f032_sidX"); // 再清一次不报错
    }

    #[test]
    fn remote_idle_single_writer_guard() {
        // §24bis 机器护栏（Phase G / full-audit Agent1「重要」结构化）：REMOTE_IDLE 唯一写者 =
        // lib.rs 的 remote-session-emitter。`mark_idle`/`clear_idle` 是 pub fn、全 crate 可达——
        // 单写者此前靠注释约定、`cargo check` 抓不住（同 §8「漏 manage 带病 5 版本」失败类）。本测把
        // 约定机器化：扫 src-tauri 生产源码（剥 cfg(test) 块 + 跳注释/定义行），断言对 mark_idle/
        // clear_idle 的**调用**只出现在 lib.rs。emitter 之外新增写者 → 本测红。
        fn strip_cfg_test(src: &str) -> String {
            // 括号配平剥掉 `#[cfg(test)]` 修饰的块（同 daemon readonly_guard 的证明过的做法）。
            let mut out = String::new();
            let mut rest = src;
            while let Some(pos) = rest.find("#[cfg(test)]") {
                out.push_str(&rest[..pos]);
                let after = &rest[pos..];
                match after.find('{') {
                    Some(brace) => {
                        let b = after.as_bytes();
                        let (mut depth, mut end) = (0i32, brace);
                        while end < after.len() {
                            match b[end] {
                                b'{' => depth += 1,
                                b'}' => {
                                    depth -= 1;
                                    if depth == 0 {
                                        end += 1;
                                        break;
                                    }
                                }
                                _ => {}
                            }
                            end += 1;
                        }
                        rest = &after[end..];
                    }
                    None => rest = &after["#[cfg(test)]".len()..],
                }
            }
            out.push_str(rest);
            out
        }
        fn is_comment(l: &str) -> bool {
            let t = l.trim_start();
            t.starts_with("//") || t.starts_with('*') || t.starts_with("/*")
        }
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src_dir];
        let mut offenders: Vec<String> = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let fname = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let prod = strip_cfg_test(&std::fs::read_to_string(&path).expect("read rs"));
                for line in prod.lines() {
                    if is_comment(line) {
                        continue;
                    }
                    let t = line.trim_start();
                    // 跳过定义行（mark_idle/clear_idle 定义在 ssh_source.rs）。
                    if t.starts_with("pub fn mark_idle")
                        || t.starts_with("fn mark_idle")
                        || t.starts_with("pub fn clear_idle")
                        || t.starts_with("fn clear_idle")
                    {
                        continue;
                    }
                    if (line.contains("mark_idle(") || line.contains("clear_idle("))
                        && fname != "lib.rs"
                    {
                        offenders.push(format!("{fname}: {}", t.trim_end()));
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "§24bis 违规：REMOTE_IDLE 写者 mark_idle/clear_idle 只准 lib.rs 的 remote-session-emitter 调用；\
             发现 emitter 之外的调用点（如确需，先想清楚是否破坏单写者不变量）：{offenders:?}"
        );
    }

    #[test]
    fn reaper_tracked_unions_announced_and_idle() {
        let announced = ["live-a".to_string(), "live-b".to_string()].into_iter();
        let idle = std::collections::HashSet::from(["idle-c".to_string()]);
        let tracked = reaper_tracked(announced, &idle);
        assert!(tracked.contains("live-a"));
        assert!(tracked.contains("live-b"));
        // **变异锚点**：idle sid 必须在 tracked 里——否则 idle→archived 无产出者=灰灯卡死。
        // 若把实现改成「只 announced 不并 idle」，此断言红。
        assert!(tracked.contains("idle-c"));
        assert_eq!(tracked.len(), 3);
    }

    #[test]
    fn reaper_tracked_empty_idle_is_just_announced() {
        let announced = ["live-a".to_string()].into_iter();
        let tracked = reaper_tracked(announced, &std::collections::HashSet::new());
        assert_eq!(
            tracked,
            std::collections::HashSet::from(["live-a".to_string()])
        );
    }
}

#[cfg(test)]
mod daemonless_tests {
    use super::*;

    #[test]
    fn parse_wc_line_ok() {
        // 典型:前导空白对齐 + 单空格分隔。
        assert_eq!(
            parse_wc_line("  1234 /home/u/.claude/projects/p/abc.jsonl"),
            Some((1234, "/home/u/.claude/projects/p/abc.jsonl".to_string()))
        );
        // 无前导空白。
        assert_eq!(
            parse_wc_line("0 /a/b.jsonl"),
            Some((0, "/a/b.jsonl".to_string()))
        );
        // 路径含空格:首个空白 split_once 后右侧整体是 path。
        assert_eq!(
            parse_wc_line("  42 /a/dir with space/x.jsonl"),
            Some((42, "/a/dir with space/x.jsonl".to_string()))
        );
    }

    #[test]
    fn parse_wc_line_rejects_garbage() {
        assert_eq!(parse_wc_line(""), None);
        assert_eq!(parse_wc_line("   "), None);
        assert_eq!(parse_wc_line("notanumber /a.jsonl"), None);
        assert_eq!(parse_wc_line("123"), None); // 无路径
        assert_eq!(parse_wc_line("123   "), None); // 数字后全空白 → 空路径
    }

    #[test]
    fn jsonl_sid_extracts_stem() {
        assert_eq!(
            jsonl_sid("/home/u/.claude/projects/p/abc-123.jsonl").as_deref(),
            Some("abc-123")
        );
        assert_eq!(jsonl_sid("abc.jsonl").as_deref(), Some("abc"));
        assert_eq!(jsonl_sid("/a/b/notjsonl.txt"), None);
        assert_eq!(jsonl_sid("/a/b/.jsonl"), None); // 空 stem
        assert_eq!(jsonl_sid("/a/b/"), None);
    }

    #[test]
    fn plan_file_read_incremental_vs_truncated() {
        // 首见(默认游标):从 0 读全量,非截断。
        let (start, tr) = plan_file_read(DlCursor::default(), 500);
        assert_eq!((start, tr), (0, false));
        // 正常增长:从 consumed 续读。
        let c = DlCursor {
            consumed: 300,
            seen_len: 300,
        };
        assert_eq!(plan_file_read(c, 800), (300, false));
        // 无新字节:start==size(下游据 start>=size 跳过)。
        assert_eq!(plan_file_read(c, 300), (300, false));
        // 截断(size < seen_len):从 0 全量重读 + truncated 标记。
        let c2 = DlCursor {
            consumed: 800,
            seen_len: 800,
        };
        assert_eq!(plan_file_read(c2, 120), (0, true));
        // 截断到空。
        assert_eq!(plan_file_read(c2, 0), (0, true));
    }

    #[test]
    fn tail_offset_is_one_based() {
        // 文档化 off-by-one 契约:跳过前 `start` 字节 = `tail -c +(start+1)`。
        // start=0 → +1(全量);start=300 → +301(跳过前 300 字节)。
        assert_eq!(format!("tail -c +{}", 0 + 1), "tail -c +1");
        assert_eq!(format!("tail -c +{}", 300 + 1), "tail -c +301");
    }

    // ---- drain_complete_lines:头号雷区(torn tail / 只消费完整行 / seq 单调)直测 ----

    fn drain(bytes: &str, seq: &mut u64) -> (Vec<(u64, String)>, u64) {
        let mut acc = bytes.as_bytes().to_vec();
        let r = drain_complete_lines(&mut acc, seq);
        // 断言:未消费的残字节数 = 原长 - consumed(torn tail 留 acc)。
        assert_eq!(acc.len() as u64, bytes.len() as u64 - r.1);
        r
    }

    #[test]
    fn drain_only_complete_lines_and_consumed_bytes() {
        let mut seq = 0;
        // 两条完整行:consumed=4(含两个 \n),残 acc 空。
        let (lines, consumed) = drain("a\nb\n", &mut seq);
        assert_eq!(lines, vec![(0, "a".to_string()), (1, "b".to_string())]);
        assert_eq!(consumed, 4);
        assert_eq!(seq, 2);
    }

    #[test]
    fn drain_torn_tail_stays_and_not_consumed() {
        let mut seq = 0;
        // "a\nb":只有 "a\n" 完整 → 消费 2 字节 seq 一条;"b" 是 torn tail 留 acc、不消费。
        let mut acc = b"a\nb".to_vec();
        let (lines, consumed) = drain_complete_lines(&mut acc, &mut seq);
        assert_eq!(lines, vec![(0, "a".to_string())]);
        assert_eq!(consumed, 2);
        assert_eq!(acc, b"b"); // torn tail 原样留存
        assert_eq!(seq, 1);
        // 下次 fill 把剩余补全:acc 追加 "c\n" → "bc\n" 成完整行,seq 从 1 续(不回退)。
        acc.extend_from_slice(b"c\n");
        let (lines2, consumed2) = drain_complete_lines(&mut acc, &mut seq);
        assert_eq!(lines2, vec![(1, "bc".to_string())]);
        assert_eq!(consumed2, 3);
        assert!(acc.is_empty());
        assert_eq!(seq, 2);
    }

    #[test]
    fn drain_strips_crlf() {
        let mut seq = 0;
        let (lines, consumed) = drain("a\r\n", &mut seq);
        assert_eq!(lines, vec![(0, "a".to_string())]); // \r 剥掉
        assert_eq!(consumed, 3); // 但 \r\n 两字节都计入 offset
    }

    #[test]
    fn drain_skips_empty_bom_whitespace_but_consumes_offset() {
        let mut seq = 0;
        // 空行 "\n"(1B)+ 纯空白 "  \n"(3B)+ BOM 行 "\u{feff}\n"(4B)均跳过不占 seq;
        // "x\n"(2B)产一条 seq=0。consumed = 1+3+4+2 = 10,seq 只推进 1。
        let (lines, consumed) = drain("\n  \n\u{feff}\nx\n", &mut seq);
        assert_eq!(lines, vec![(0, "x".to_string())]);
        assert_eq!(consumed, 10);
        assert_eq!(seq, 1); // 空/空白/BOM 行不占 seq
    }

    #[test]
    fn drain_seq_monotonic_across_calls_not_reset() {
        // 多轮调用共享 &mut seq:seq 连续单调、不因新调用/新 acc 重置(截断重读靠此不冲突)。
        let mut seq = 5;
        let (l1, _) = drain("p\nq\n", &mut seq);
        assert_eq!(l1, vec![(5, "p".to_string()), (6, "q".to_string())]);
        let (l2, _) = drain("r\n", &mut seq);
        assert_eq!(l2, vec![(7, "r".to_string())]);
        assert_eq!(seq, 8);
    }

    #[test]
    fn drain_empty_input_noop() {
        let mut seq = 3;
        let (lines, consumed) = drain("", &mut seq);
        assert!(lines.is_empty());
        assert_eq!(consumed, 0);
        assert_eq!(seq, 3);
    }
}

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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
        std::process::Command::new("ssh")
            .arg("-G")
            .arg(&alias_for_exec)
            .output()
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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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

/// F57：别名基名前缀——首个 `-`/`_`/`.` 前的段（`aya-lan` → `aya`；`pi` → `pi`）。同机聚合判据之一。
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
/// （如「SSH 连上了，但 daemon 没响应/未部署」）。仅参数级硬错误才返回 Err。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ConnTestResult {
    /// SSH 连接 + 鉴权是否成功。
    pub ssh_ok: bool,
    /// 握手时观察到的 server host key 指纹（`SHA256:...`）。用于展示 + TOFU 固化。
    pub fingerprint: Option<String>,
    /// F45 / D 审计重要-1：竞发胜出的地址（`host:port`）。多地址 TOFU 首连时,让用户明确
    /// 自己正在固化**哪条路径**观察到的指纹（而非盲信「最快那条」）。单地址时即该地址。
    pub endpoint: Option<String>,
    /// daemon 是否在 SHORT timeout 内回了可解析的 hello 帧。
    pub daemon_ok: bool,
    /// daemon hello 的人读摘要（`v=.. arch=.. claude_dir=..`）。
    pub daemon_hello: Option<String>,
    /// 人读的总体状态 / 失败原因。
    pub message: String,
}

/// 测试一条远端配置：连 SSH → 读指纹 → exec daemon → 等首行 hello（SHORT timeout）。
///
/// 步骤化、每步把结果填进 [`ConnTestResult`]：
/// 1. `connect_session`（publickey 或 agent）。失败 → ssh_ok=false + message，返回 Ok。
/// 2. 成功 → ssh_ok=true，从 observed cell 读指纹。
/// 3. channel_open + exec daemon + 读首行 stdout（`tokio::time::timeout` 8s）。
///    解析成 hello → daemon_ok=true + 摘要；否则 message 说明 daemon 未响应/未部署。
///
/// 只有"无法构造测试"这类硬错才返回 Err；连接/鉴权/daemon 失败都收进结果里，UI 据此分级展示。
#[tauri::command]
pub async fn test_remote_connection(
    cfg: RemoteConfig,
    on_stage: tauri::ipc::Channel<ConnectStage>,
) -> Result<ConnTestResult, String> {
    let mut result = ConnTestResult {
        ssh_ok: false,
        fingerprint: None,
        endpoint: None,
        daemon_ok: false,
        daemon_hello: None,
        message: String::new(),
    };

    // 1. 连接 + 鉴权（短 inactivity：测试连接不需要长保活）。F46：传 Some(on_stage) 让
    //    竞发/握手/鉴权按地址泳道流式 emit 到前端「连接过程」日志。
    let (session, observed) =
        match connect_session(&cfg, Some(Duration::from_secs(30)), Some(on_stage)).await {
            Ok(s) => s,
            Err(e) => {
                // 握手失败（含 host key 不匹配被拒）。check_server_key 可能已写过指纹，但
                // connect_session 在 Err 路径不回传 cell，这里只报失败原因即可。
                result.ssh_ok = false;
                result.message = format!("SSH 连接/鉴权失败：{e}");
                return Ok(result);
            }
        };
    result.ssh_ok = true;
    result.fingerprint = observed.lock().ok().and_then(|g| g.clone());
    // 连接成功 → last-good 已记为胜者;回传给前端展示「你正连上/将固化哪个地址」。
    let win = winner_address(&cfg);
    result.endpoint = Some(format!("{}:{}", win.host, win.port));

    // 3. exec daemon 并等首行 hello。
    let daemon_path = cfg.daemon_path.clone();
    match probe_daemon(&session, &daemon_path).await {
        Ok(Some(summary)) => {
            result.daemon_ok = true;
            result.daemon_hello = Some(summary);
            result.message = "SSH 与 daemon 均正常。".to_string();
        }
        Ok(None) => {
            result.message =
                "SSH 连上了，但 daemon 在超时内未回 hello（未部署 / 路径错 / 启动失败？）。"
                    .to_string();
        }
        Err(e) => {
            result.message = format!("SSH 连上了，但 daemon 探测失败：{e}");
        }
    }

    // 礼貌关闭 session（best-effort，忽略错误）。
    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "", "")
        .await;
    Ok(result)
}

/// exec daemon、读首行 stdout（SHORT timeout）、若是 hello 帧返回人读摘要。
///
/// 返回：
/// - `Ok(Some(summary))` —— 读到并解析成 hello。
/// - `Ok(None)`          —— 超时 / EOF / 非 hello（daemon 未正常响应）。
/// - `Err(_)`            —— channel/exec/IO 硬错误。
async fn probe_daemon(
    session: &client::Handle<ClientHandler>,
    daemon_path: &str,
) -> Result<Option<String>, String> {
    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("打开 session channel 失败: {e}"))?;
    channel
        .exec(true, daemon_path.as_bytes())
        .await
        .map_err(|e| format!("exec {daemon_path} 失败: {e}"))?;

    let mut reader = BufReader::new(channel.into_stream());
    let mut line = String::new();
    let read = tokio::time::timeout(Duration::from_secs(8), reader.read_line(&mut line)).await;

    // 读完后尽量优雅关掉写半边（daemon 看到 EOF 自行退出）。best-effort。
    let _ = reader.get_mut().shutdown().await;

    match read {
        Err(_elapsed) => Ok(None), // 超时
        Ok(Err(e)) => Err(format!("读 daemon stdout 出错: {e}")),
        Ok(Ok(0)) => Ok(None), // EOF（daemon 立即退出 / 未输出）
        Ok(Ok(_)) => {
            let trimmed = line.trim_end_matches(['\n', '\r']);
            match parse_frame(trimmed) {
                Some(InboundFrame::Hello {
                    v,
                    build_id,
                    host_arch,
                    claude_dir,
                    capabilities,
                }) => Ok(Some(format!(
                    "v={v} build={build_id} arch={host_arch} claude_dir={claude_dir} caps={capabilities:?}"
                ))),
                _ => Ok(None), // 非 hello 帧 → daemon 未正常握手
            }
        }
    }
}

#[cfg(test)]
mod batcher_tests {
    use super::*;

    fn line(sid: &str, seq: u64) -> JsonlLine {
        JsonlLine {
            session_id: sid.to_string(),
            path: std::path::PathBuf::from(format!("/fake/{sid}.jsonl")),
            seq,
            raw: format!("{{\"seq\":{seq}}}"),
        }
    }

    #[test]
    fn push_below_cap_accumulates_take_drains_in_order() {
        let mut b = Batcher::new(600);
        assert!(b.push(line("s1", 0)).is_none());
        assert!(b.push(line("s2", 0)).is_none()); // 跨 session 混流不拆
        assert!(b.push(line("s1", 1)).is_none());
        let out = b.take().expect("non-empty");
        assert_eq!(
            out.iter()
                .map(|l| (l.session_id.as_str(), l.seq))
                .collect::<Vec<_>>(),
            vec![("s1", 0), ("s2", 0), ("s1", 1)],
            "到达顺序 = 发出顺序"
        );
        assert!(b.take().is_none(), "drained");
    }

    #[test]
    fn cap_triggers_immediate_full_flush() {
        let mut b = Batcher::new(3);
        assert!(b.push(line("s", 0)).is_none());
        assert!(b.push(line("s", 1)).is_none());
        let full = b.push(line("s", 2)).expect("cap reached → full batch out");
        assert_eq!(full.len(), 3);
        assert!(b.take().is_none(), "buffer empty after cap flush");
        // 继续攒下一批不受影响
        assert!(b.push(line("s", 3)).is_none());
        assert_eq!(b.take().unwrap().len(), 1);
    }

    #[test]
    fn take_on_empty_is_none() {
        let mut b = Batcher::new(600);
        assert!(b.take().is_none());
    }
}

#[cfg(test)]
mod parse_frame_tests {
    use super::*;

    /// hello 帧解析：取 v / build_id / host_arch / claude_dir（#33 起捕获 build_id）。
    #[test]
    fn parses_hello_and_captures_build_id() {
        let line = r#"{"kind":"hello","v":1,"build_id":"abc123","host_arch":"aarch64","claude_dir":"/home/pi/.claude"}"#;
        let frame = parse_frame(line).expect("hello must parse");
        assert_eq!(
            frame,
            InboundFrame::Hello {
                v: 1,
                build_id: "abc123".to_string(),
                host_arch: "aarch64".to_string(),
                claude_dir: "/home/pi/.claude".to_string(),
                // F66：旧 daemon（本样本无 capabilities 字段）→ 空集（保守缺省）
                capabilities: Vec::new(),
            }
        );
    }

    /// #33：hello 缺 build_id → None（按必需字段，坏帧跳过；既有 daemon 总在发它）。
    #[test]
    fn hello_missing_build_id_returns_none() {
        let line = r#"{"kind":"hello","v":1,"host_arch":"x86_64","claude_dir":"/c"}"#;
        assert_eq!(parse_frame(line), None);
    }

    /// F66（#58③）wire 契约：hello 的 `capabilities` 字段。
    /// ① 缺字段（旧 daemon）→ 空集（向后兼容，保守缺省，同 §27 族）。
    /// ② 声明数组 → 原样解析（monitor 按此决定发哪些 flag）。
    /// ③ 非数组 / 元素非字符串 → 滤成空集，绝不 panic（宽容解析，§18）。
    #[test]
    fn hello_capabilities_backward_compat_and_declared() {
        // ① 旧 daemon：无 capabilities → 空集
        let old =
            r#"{"kind":"hello","v":1,"build_id":"p1e","host_arch":"x86_64","claude_dir":"/c"}"#;
        match parse_frame(old).unwrap() {
            InboundFrame::Hello { capabilities, .. } => {
                assert!(capabilities.is_empty(), "旧 daemon 无声明 → 空集");
            }
            _ => panic!("expected Hello"),
        }
        // ② 新 daemon：声明能力
        let new = r#"{"kind":"hello","v":1,"build_id":"p1h","host_arch":"x86_64","claude_dir":"/c","capabilities":["bg","tail-only"]}"#;
        match parse_frame(new).unwrap() {
            InboundFrame::Hello { capabilities, .. } => {
                assert_eq!(
                    capabilities,
                    vec!["bg".to_string(), "tail-only".to_string()]
                );
            }
            _ => panic!("expected Hello"),
        }
        // ③ 畸形 capabilities（非数组 / 混入非字符串）→ 不 panic，滤成空/仅字符串
        let bad = r#"{"kind":"hello","v":1,"build_id":"p1x","host_arch":"x86_64","claude_dir":"/c","capabilities":"not-array"}"#;
        match parse_frame(bad).unwrap() {
            InboundFrame::Hello { capabilities, .. } => {
                assert!(capabilities.is_empty(), "非数组 capabilities → 空集，不崩");
            }
            _ => panic!("expected Hello"),
        }
        let mixed = r#"{"kind":"hello","v":1,"build_id":"p1x","host_arch":"x86_64","claude_dir":"/c","capabilities":["bg",42,null,"tail-only"]}"#;
        match parse_frame(mixed).unwrap() {
            InboundFrame::Hello { capabilities, .. } => {
                assert_eq!(
                    capabilities,
                    vec!["bg".to_string(), "tail-only".to_string()],
                    "非字符串元素被滤掉，字符串保留"
                );
            }
            _ => panic!("expected Hello"),
        }
    }

    /// #33：版本协商真值表。协议不符优先于 build 差异。
    #[test]
    fn negotiate_version_truth_table() {
        // 全同 → Ok。
        assert_eq!(
            negotiate_version(EXPECTED_PROTO_V, EXPECTED_DAEMON_BUILD_ID),
            VersionVerdict::Ok
        );
        // 协议同、build 异 → StaleBuild（带上报值）。
        assert_eq!(
            negotiate_version(EXPECTED_PROTO_V, "p1a-history"),
            VersionVerdict::StaleBuild {
                reported: "p1a-history".to_string()
            }
        );
        // 协议异 → Incompatible，且即便 build 也不同，协议优先。
        assert_eq!(
            negotiate_version(999, "whatever"),
            VersionVerdict::Incompatible { reported_v: 999 }
        );
        assert_eq!(
            negotiate_version(999, EXPECTED_DAEMON_BUILD_ID),
            VersionVerdict::Incompatible { reported_v: 999 },
            "协议不符时即使 build 匹配也算不兼容"
        );
    }

    /// #33：version_warning 文案——Ok→None，其余→Some 且含 label。
    #[test]
    fn version_warning_messages() {
        assert_eq!(
            version_warning(EXPECTED_PROTO_V, EXPECTED_DAEMON_BUILD_ID, "pi"),
            None
        );
        let stale = version_warning(EXPECTED_PROTO_V, "p1a-history", "pi").expect("stale warns");
        assert!(stale.contains("pi") && stale.contains("p1a-history"));
        let incompat = version_warning(2, EXPECTED_DAEMON_BUILD_ID, "wsl").expect("incompat warns");
        assert!(incompat.contains("wsl") && incompat.contains("不兼容"));
    }

    /// 两条 line 帧：逐字段断言 session_id / path / seq / raw 都原样取出。
    #[test]
    fn parses_two_line_frames_with_all_fields() {
        let l0 = r#"{"kind":"line","session_id":"s-1","path":"/home/pi/.claude/projects/p/s-1.jsonl","seq":0,"raw":"{\"type\":\"user\"}"}"#;
        let l1 = r#"{"kind":"line","session_id":"s-1","path":"/home/pi/.claude/projects/p/s-1.jsonl","seq":1,"raw":"second"}"#;

        let f0 = parse_frame(l0).expect("line 0 must parse");
        assert_eq!(
            f0,
            InboundFrame::Line {
                session_id: "s-1".to_string(),
                path: "/home/pi/.claude/projects/p/s-1.jsonl".to_string(),
                seq: 0,
                raw: r#"{"type":"user"}"#.to_string(),
            }
        );

        let f1 = parse_frame(l1).expect("line 1 must parse");
        match f1 {
            InboundFrame::Line { seq, raw, .. } => {
                assert_eq!(seq, 1);
                assert_eq!(raw, "second");
            }
            other => panic!("expected Line, got {other:?}"),
        }
    }

    // ---------- P5（zero-poll-liveness）：正向死亡帧的解析 ----------

    #[test]
    fn tmux_session_closed_parses() {
        assert_eq!(
            parse_frame(r#"{"kind":"tmux_session_closed","name":"cc-abc123"}"#),
            Some(InboundFrame::TmuxSessionClosed {
                name: "cc-abc123".to_string()
            })
        );
    }

    /// 缺 `name` / 非字符串 ⇒ 坏帧跳过（`None`），**不 panic**，与其余帧同一口径。
    #[test]
    fn tmux_session_closed_bad_payload_is_skipped() {
        assert_eq!(parse_frame(r#"{"kind":"tmux_session_closed"}"#), None);
        assert_eq!(
            parse_frame(r#"{"kind":"tmux_session_closed","name":42}"#),
            None
        );
    }

    /// 未知 kind（协议向前演进新增的帧类型）→ None，调用方 warn+skip，绝不 panic。
    #[test]
    fn unknown_kind_returns_none() {
        let line = r#"{"kind":"future_thing","x":1}"#;
        assert_eq!(parse_frame(line), None);
    }

    /// 完全非 JSON 的 garbage 行 → None，绝不 panic。
    #[test]
    fn garbage_non_json_returns_none() {
        assert_eq!(parse_frame("not json"), None);
        assert_eq!(parse_frame(""), None);
        // 合法 JSON 但不是 object（数组 / 标量）也 → None
        assert_eq!(parse_frame("[1,2,3]"), None);
        assert_eq!(parse_frame("42"), None);
        // object 但缺 kind
        assert_eq!(parse_frame(r#"{"v":1}"#), None);
    }

    /// 已知 kind + 额外未知字段：仍正常解析，多余字段被忽略（不 fail）。
    #[test]
    fn known_kind_with_extra_fields_still_parses() {
        let line = r#"{"kind":"session_added","sid":"s-9","extra":"ignored","nested":{"a":1}}"#;
        let frame = parse_frame(line).expect("session_added with extras must parse");
        assert_eq!(
            frame,
            InboundFrame::SessionAdded {
                sid: "s-9".to_string(),
                session_kind: None,
                attachable: None,
                cwd: None,
                name: None,
                path: None,
                lines: None,
                status: None,
                waiting_for: None,
            }
        );
    }

    /// Batch7-F24：p1e daemon 的 session_added 附加元信息正确解析；
    /// 旧 daemon 缺字段 → None（上一测试已覆盖）。
    #[test]
    fn session_added_metadata_parses() {
        let line = r#"{"kind":"session_added","sid":"s-bg","session_kind":"bg","cwd":"/proj/x","name":"评估任务","path":"/home/u/.claude/projects/p/s-bg.jsonl","lines":42}"#;
        let frame = parse_frame(line).expect("must parse");
        assert_eq!(
            frame,
            InboundFrame::SessionAdded {
                sid: "s-bg".to_string(),
                session_kind: Some("bg".to_string()),
                attachable: None,
                cwd: Some("/proj/x".to_string()),
                name: Some("评估任务".to_string()),
                path: Some("/home/u/.claude/projects/p/s-bg.jsonl".to_string()),
                lines: Some(42),
                status: None,
                waiting_for: None,
            }
        );
    }

    /// Batch9-F27：session_status 帧解析 + session_added 初始 status。
    #[test]
    fn session_status_frame_parses() {
        let line = r#"{"kind":"session_status","sid":"s-1","status":"waiting","waiting_for":"permission prompt"}"#;
        assert_eq!(
            parse_frame(line),
            Some(InboundFrame::SessionStatus {
                sid: "s-1".to_string(),
                status: Some("waiting".to_string()),
                waiting_for: Some("permission prompt".to_string()),
            })
        );
        // 缺 waiting_for → None
        let line = r#"{"kind":"session_status","sid":"s-2","status":"busy"}"#;
        assert_eq!(
            parse_frame(line),
            Some(InboundFrame::SessionStatus {
                sid: "s-2".to_string(),
                status: Some("busy".to_string()),
                waiting_for: None,
            })
        );
    }

    /// session_removed 映射到对应 variant。
    #[test]
    fn parses_session_removed() {
        let line = r#"{"kind":"session_removed","sid":"s-dead"}"#;
        let frame = parse_frame(line).expect("session_removed must parse");
        assert_eq!(
            frame,
            InboundFrame::SessionRemoved {
                sid: "s-dead".to_string(),
                // ★ S0 向后兼容：**旧 daemon 不发 cause** ⇒ 必须解析成 Gone，
                // 即维持今天的行为（查快照判灰点）。
                cause: RemovalCause::Gone,
            }
        );
    }

    /// ★ S0：带 cause 的帧解析 + 未知取值的降级方向。
    #[test]
    fn parses_session_removed_cause() {
        assert_eq!(
            parse_frame(r#"{"kind":"session_removed","sid":"s","cause":"superseded"}"#),
            Some(InboundFrame::SessionRemoved {
                sid: "s".to_string(),
                cause: RemovalCause::Superseded,
            })
        );
        // 未知取值退回 Gone：宁可保守判活（可能多留一个灰点），也不能凭一个不认识的词
        // 直接归档掉一个其实还活着的会话——归档是**破坏性**的（forget 绑定 + 关 tab）。
        assert_eq!(
            parse_frame(r#"{"kind":"session_removed","sid":"s","cause":"从未见过的词"}"#),
            Some(InboundFrame::SessionRemoved {
                sid: "s".to_string(),
                cause: RemovalCause::Gone,
            })
        );
    }

    /// issue #32：overflow 帧解析出 dropped 计数；缺/错 dropped 当坏帧跳过（None）。
    #[test]
    fn parses_overflow_and_rejects_bad_dropped() {
        let frame = parse_frame(r#"{"kind":"overflow","dropped":12}"#).expect("overflow parses");
        assert_eq!(frame, InboundFrame::Overflow { dropped: 12 });
        // 缺 dropped → None
        assert_eq!(parse_frame(r#"{"kind":"overflow"}"#), None);
        // dropped 类型错（字符串）→ None
        assert_eq!(parse_frame(r#"{"kind":"overflow","dropped":"12"}"#), None);
    }

    /// B2：tmux_sessions 帧解析出 raw（tmux ls 原文，含转义 TAB）；缺/错 raw 当坏帧跳过（None）。
    #[test]
    fn parses_tmux_sessions_and_rejects_bad_raw() {
        let frame = parse_frame(
            "{\"kind\":\"tmux_sessions\",\"raw\":\"s1\\t/p\\tclaude\\t1\\t2\\tsid-a\"}",
        )
        .expect("tmux_sessions parses");
        assert_eq!(
            frame,
            InboundFrame::TmuxSessions {
                raw: "s1\t/p\tclaude\t1\t2\tsid-a".to_string(),
                // P1：旧 daemon 无该字段 ⇒ None（**不是**坏帧）。
                observation: None,
            }
        );
        // NO_TMUX 哨兵也是合法 raw。
        assert!(matches!(
            parse_frame(r#"{"kind":"tmux_sessions","raw":"NO_TMUX"}"#),
            Some(InboundFrame::TmuxSessions { .. })
        ));
        // 缺 raw / raw 非字符串 → None（坏帧跳过）。
        assert_eq!(parse_frame(r#"{"kind":"tmux_sessions"}"#), None);
        assert_eq!(parse_frame(r#"{"kind":"tmux_sessions","raw":5}"#), None);
        // P1（additive 字段）：observation 存在则读出；**非字符串不是坏帧**、退化成 None
        // （坏 daemon 也只该让 monitor 退回保守判据，不该让整帧被丢）。
        assert_eq!(
            parse_frame(r#"{"kind":"tmux_sessions","raw":"","observation":"zero_sessions"}"#),
            Some(InboundFrame::TmuxSessions {
                raw: String::new(),
                observation: Some("zero_sessions".to_string()),
            })
        );
        assert_eq!(
            parse_frame(r#"{"kind":"tmux_sessions","raw":"","observation":7}"#),
            Some(InboundFrame::TmuxSessions {
                raw: String::new(),
                observation: None,
            }),
            "observation 类型错只该退化成 None，不该把整帧当坏帧丢掉"
        );
    }

    /// 已知 kind 但必需字段缺失 / 类型错 → None（坏帧当 garbage 跳过，不 panic）。
    #[test]
    fn known_kind_missing_or_wrong_field_returns_none() {
        // line 缺 seq
        assert_eq!(
            parse_frame(r#"{"kind":"line","session_id":"s","path":"/p","raw":"x"}"#),
            None
        );
        // seq 类型错（字符串而非数字）
        assert_eq!(
            parse_frame(r#"{"kind":"line","session_id":"s","path":"/p","seq":"0","raw":"x"}"#),
            None
        );
        // session_added 缺 sid
        assert_eq!(parse_frame(r#"{"kind":"session_added"}"#), None);
    }

    /// 模拟一段帧序列逐行喂入：hello → 两条 line → 未知 → garbage → session_removed。
    /// 断言 dispatch 正确性 + 未知/garbage 为 None（不 panic）。
    #[test]
    fn dispatch_over_a_frame_sequence() {
        let lines = [
            r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/c"}"#,
            r#"{"kind":"line","session_id":"s","path":"/p","seq":0,"raw":"a"}"#,
            r#"{"kind":"line","session_id":"s","path":"/p","seq":1,"raw":"b"}"#,
            r#"{"kind":"future_thing","x":1}"#,
            "not json",
            r#"{"kind":"session_removed","sid":"s"}"#,
        ];
        let parsed: Vec<Option<InboundFrame>> = lines.iter().map(|l| parse_frame(l)).collect();

        assert!(matches!(parsed[0], Some(InboundFrame::Hello { v: 1, .. })));
        assert!(matches!(parsed[1], Some(InboundFrame::Line { seq: 0, .. })));
        assert!(matches!(parsed[2], Some(InboundFrame::Line { seq: 1, .. })));
        assert_eq!(parsed[3], None, "unknown kind → None");
        assert_eq!(parsed[4], None, "garbage → None");
        assert!(matches!(
            parsed[5],
            Some(InboundFrame::SessionRemoved { .. })
        ));
    }
}

#[cfg(test)]
mod tier1_tests {
    use super::*;

    /// host 别名解析：取 Host 行 token、排除通配 / `!`、去重保序、跳过非 Host 指令。
    #[test]
    fn parse_host_aliases_basics() {
        let cfg = "\
# comment
Host pi server1 server2
    HostName 1.2.3.4
    User pi

Host=eqsign
    Port 2222

Host *.internal wild*card prod ?q !neg
    User admin

Host prod
    User dup
";
        let aliases = parse_host_aliases(cfg);
        // pi/server1/server2 来自第一块；eqsign 用 `=` 分隔；prod 出现两次只留一次；
        // `*.internal` / `wild*card` / `?q` / `!neg` 全被通配/否定规则排除。
        assert_eq!(aliases, vec!["pi", "server1", "server2", "eqsign", "prod"]);
    }

    /// 排除字面量 `*`（catch-all）。
    #[test]
    fn parse_host_aliases_excludes_star() {
        let aliases = parse_host_aliases("Host *\n    User x\n");
        assert!(aliases.is_empty());
    }

    /// 没有任何 Host 指令 → 空。
    #[test]
    fn parse_host_aliases_empty_when_no_host() {
        let aliases = parse_host_aliases("# just comments\nUser nobody\nPort 22\n");
        assert!(aliases.is_empty());
    }

    // === F57：智能聚合 ===

    fn rh(host: &str, key: Option<&str>, user: &str, pj: Option<&str>) -> ResolvedHost {
        ResolvedHost {
            host: host.into(),
            port: 22,
            user: user.into(),
            key_path: key.map(String::from),
            proxy_jump: pj.map(String::from),
        }
    }

    #[test]
    fn alias_base_variants() {
        assert_eq!(alias_base("aya-lan"), "aya");
        assert_eq!(alias_base("aya_wan"), "aya");
        assert_eq!(alias_base("aya.internal"), "aya");
        assert_eq!(alias_base("pi"), "pi");
    }

    #[test]
    fn aggregate_same_machine_multi_address() {
        // aya-lan / aya-wan 同 key+user+基名 → 聚合成 1 台多地址;pi 基名不同 → 独立。
        let groups = aggregate_ssh_hosts(vec![
            ("aya-lan".into(), rh("10.0.0.2", Some("/k"), "zbl", None)),
            (
                "aya-wan".into(),
                rh("aya.example.com", Some("/k"), "zbl", None),
            ),
            ("pi".into(), rh("pi.local", Some("/k"), "zbl", None)),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].label, "aya");
        assert_eq!(groups[0].host, "10.0.0.2");
        assert_eq!(groups[0].addresses, vec!["aya.example.com".to_string()]);
        let aliases: Vec<&str> = groups[0].members.iter().map(|m| m.alias.as_str()).collect();
        assert_eq!(aliases, vec!["aya-lan", "aya-wan"]);
        assert_eq!(groups[1].label, "pi");
        assert!(groups[1].addresses.is_empty(), "单机组无备用地址");
    }

    #[test]
    fn aggregate_different_key_not_merged() {
        // 同基名但不同 key → 不聚合(一把 key 一台机)。
        let groups = aggregate_ssh_hosts(vec![
            ("web-a".into(), rh("1.1.1.1", Some("/ka"), "u", None)),
            ("web-b".into(), rh("2.2.2.2", Some("/kb"), "u", None)),
        ]);
        assert_eq!(groups.len(), 2, "不同 key → 独立");
        // F57-1：单成员组用**完整别名**当 label(否则都叫 web → 前端落卡碰撞丢一台)。
        assert_eq!(groups[0].label, "web-a");
        assert_eq!(groups[1].label, "web-b");
    }

    #[test]
    fn parse_ssh_g_proxyjump_and_fields() {
        let with = parse_ssh_g_output(
            "hostname 1.2.3.4\nport 2222\nuser pi\nproxyjump bastion\n",
            "a",
        );
        assert_eq!(with.host, "1.2.3.4");
        assert_eq!(with.port, 2222);
        assert_eq!(with.user, "pi");
        assert_eq!(with.proxy_jump.as_deref(), Some("bastion"));
        // none(大小写不敏感)→ 无跳板;无 proxyjump 行 → None。
        assert_eq!(
            parse_ssh_g_output("hostname h\nproxyjump None\n", "a").proxy_jump,
            None
        );
        assert_eq!(
            parse_ssh_g_output("hostname h\nuser u\n", "a").proxy_jump,
            None
        );
        // hostname/port 缺 → 回退别名 / 22。
        let fb = parse_ssh_g_output("user u\n", "myalias");
        assert_eq!(fb.host, "myalias");
        assert_eq!(fb.port, 22);
    }

    #[test]
    fn aggregate_proxyjump_and_dedup() {
        // 同 host 去重;jump 取组内首个非空 proxyjump。
        let groups = aggregate_ssh_hosts(vec![
            (
                "aya-lan".into(),
                rh("10.0.0.2", Some("/k"), "u", Some("bastion")),
            ),
            ("aya-wan".into(), rh("10.0.0.2", Some("/k"), "u", None)),
        ]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].jump.as_deref(), Some("bastion"));
        assert!(groups[0].addresses.is_empty(), "同 host → 去重无额外地址");
    }

    /// allowlist：合法字符通过，含空格 / 选项前缀 / shell 元字符的别名被拒。
    #[test]
    fn is_safe_alias_allowlist() {
        assert!(is_safe_alias("pi"));
        assert!(is_safe_alias("my-host.example.com"));
        assert!(is_safe_alias("user@host:22"));
        assert!(is_safe_alias("a_b.c-1"));

        assert!(!is_safe_alias(""));
        assert!(!is_safe_alias("has space"));
        assert!(!is_safe_alias("a;b")); // shell metachar
        assert!(!is_safe_alias("a$b"));
        assert!(!is_safe_alias("a/b")); // 路径分隔不在 allowlist
        assert!(!is_safe_alias("a&b"));
        // 以 `-` 开头本身在 allowlist 内（`-` 是合法字符），option-injection 由
        // resolve_ssh_host 里单独的 starts_with('-') 检查兜住，这里只验字符集。
        assert!(is_safe_alias("-oProxyCommand"));
    }

    /// `~` 展开：`~` / `~/x` 展开到 home；非 `~` 前缀原样。
    #[test]
    fn expand_tilde_basics() {
        let home = dirs::home_dir().expect("home dir for test");
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(
            expand_tilde("~/.ssh/id_ed25519"),
            home.join(".ssh/id_ed25519")
        );
        // 非 ~ 前缀原样（不是 home-relative）。
        assert_eq!(
            expand_tilde("/etc/ssh/key"),
            std::path::PathBuf::from("/etc/ssh/key")
        );
        // `~user` 形式（非 `~/`）不展开（我们只处理自己的 home）。
        assert_eq!(
            expand_tilde("~otheruser/key"),
            std::path::PathBuf::from("~otheruser/key")
        );
    }

    /// RemoteConfig 反序列化：camelCase key + 空串可选字段归 None + port 默认 22 + 忽略 enabled。
    #[test]
    fn remote_config_deserializes_frontend_shape() {
        // 前端 collect() 的形状：所有字段都在，可选字段可能是空串，外加 enabled。
        let json = r#"{
            "enabled": true,
            "host": "pi.local",
            "port": 2200,
            "user": "pi",
            "keyPath": "",
            "daemonPath": "/home/pi/cc-monitor-remote",
            "hostKeyFingerprint": ""
        }"#;
        let cfg: RemoteConfig = serde_json::from_str(json).expect("must deserialize");
        assert_eq!(cfg.host, "pi.local");
        assert_eq!(cfg.port, 2200);
        assert_eq!(cfg.user, "pi");
        assert_eq!(cfg.key_path, None, "空串 keyPath → None");
        assert_eq!(cfg.daemon_path, "/home/pi/cc-monitor-remote");
        assert_eq!(cfg.host_key_fingerprint, None, "空串指纹 → None");
    }

    /// RemoteConfig：缺 port 时默认 22，非空可选字段保留为 Some。
    #[test]
    fn remote_config_defaults_and_some() {
        let json = r#"{
            "host": "h",
            "user": "u",
            "keyPath": "C:\\k",
            "daemonPath": "d",
            "hostKeyFingerprint": "SHA256:abc"
        }"#;
        let cfg: RemoteConfig = serde_json::from_str(json).expect("must deserialize");
        assert_eq!(cfg.port, 22, "缺 port → 默认 22");
        assert_eq!(cfg.key_path.as_deref(), Some("C:\\k"));
        assert_eq!(cfg.host_key_fingerprint.as_deref(), Some("SHA256:abc"));
    }

    /// next_backoff：翻倍直到封顶 RECONNECT_MAX(30s)，封顶后饱和不再增长。
    #[test]
    fn next_backoff_doubles_then_caps() {
        assert_eq!(next_backoff(Duration::from_secs(2)), Duration::from_secs(4));
        assert_eq!(next_backoff(Duration::from_secs(4)), Duration::from_secs(8));
        // 16s*2=32s 被封顶到 30s。
        assert_eq!(
            next_backoff(Duration::from_secs(16)),
            Duration::from_secs(30)
        );
        // 已在上界 → 翻倍后仍被 min 拉回 30s（饱和）。
        assert_eq!(
            next_backoff(Duration::from_secs(30)),
            Duration::from_secs(30)
        );
    }

    // F43：check_server_key 三分支——匹配接受 / 失配拒绝 / 无期望指纹 TOFU 接受，
    // 且无论哪支都把实际指纹写回 observed cell（测试连接据此展示 + 固化）。
    use russh::client::Handler as _; // check_server_key 是 trait 方法

    fn handler_with(expected: Option<&str>) -> (ClientHandler, Arc<Mutex<Option<String>>>) {
        let observed = Arc::new(Mutex::new(None));
        let h = ClientHandler {
            expected_fingerprint: expected.map(String::from),
            observed_fingerprint: Arc::clone(&observed),
            stage_emitter: None,
            endpoint: None,
        };
        (h, observed)
    }

    /// 固定 ed25519 公钥 + 其 SHA256 指纹（ssh-keygen 一次性生成后固化进测试，
    /// SAMPLE_FP 可用 `ssh-keygen -lf` 对 SAMPLE_PUB 独立复算核对）。
    const SAMPLE_PUB: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEwdsNpXeLF3bjmkjNIpFsbGCxLntS8RsfA6BPOv/Ykv f43";
    const SAMPLE_FP: &str = "SHA256:fPFSH7moeRu2I96lFjdo8lO2iB7KgVLtL4LXvHVZWDk";

    fn sample_key() -> PublicKey {
        PublicKey::from_openssh(SAMPLE_PUB).expect("parse sample pubkey")
    }

    #[tokio::test]
    async fn check_server_key_matching_fingerprint_accepts() {
        let (mut h, observed) = handler_with(Some(SAMPLE_FP));
        assert!(h.check_server_key(&sample_key()).await.unwrap());
        assert_eq!(observed.lock().unwrap().as_deref(), Some(SAMPLE_FP));
    }

    #[tokio::test]
    async fn check_server_key_matching_tolerates_trailing_whitespace() {
        let padded = format!("{SAMPLE_FP}\n  ");
        let (mut h, _observed) = handler_with(Some(&padded));
        assert!(
            h.check_server_key(&sample_key()).await.unwrap(),
            "尾随空白不应误判 MITM"
        );
    }

    #[tokio::test]
    async fn check_server_key_mismatch_rejects_but_records() {
        let (mut h, observed) =
            handler_with(Some("SHA256:deadbeefwrongfingerprintvalueAAAAAAAAAAAA"));
        assert!(
            !h.check_server_key(&sample_key()).await.unwrap(),
            "失配必须拒绝"
        );
        // 失配也把实际指纹写回 cell（供「重置为 TOFU / 重新固化」）。
        assert_eq!(observed.lock().unwrap().as_deref(), Some(SAMPLE_FP));
    }

    #[tokio::test]
    async fn check_server_key_no_expected_tofu_accepts() {
        let (mut h, observed) = handler_with(None);
        assert!(
            h.check_server_key(&sample_key()).await.unwrap(),
            "TOFU 首连接受"
        );
        assert_eq!(observed.lock().unwrap().as_deref(), Some(SAMPLE_FP));
    }

    // === F45：地址解析 + endpoints ===

    fn ep(host: &str, port: u16) -> Endpoint {
        Endpoint {
            host: host.into(),
            port,
        }
    }

    #[test]
    fn parse_address_line_four_forms() {
        assert_eq!(parse_address_line("pi.local", 22), Some(ep("pi.local", 22)));
        assert_eq!(
            parse_address_line("10.0.0.2:2222", 22),
            Some(ep("10.0.0.2", 2222))
        );
        // [IPv6]:port 与 [IPv6]
        assert_eq!(
            parse_address_line("[fe80::1]:2200", 22),
            Some(ep("fe80::1", 2200))
        );
        assert_eq!(parse_address_line("[::1]", 22), Some(ep("::1", 22)));
        // 裸 IPv6（trap #7：>1 冒号不误当 host:port）
        assert_eq!(parse_address_line("::1", 22), Some(ep("::1", 22)));
        assert_eq!(parse_address_line("fe80::1", 22), Some(ep("fe80::1", 22)));
    }

    #[test]
    fn parse_address_line_rejects_garbage() {
        assert_eq!(parse_address_line("", 22), None);
        assert_eq!(parse_address_line("   ", 22), None);
        assert_eq!(parse_address_line("h:notaport", 22), None);
        assert_eq!(parse_address_line(":2222", 22), None); // 无 host
        assert_eq!(parse_address_line("[", 22), None); // 未闭合方括号
        assert_eq!(parse_address_line("[]:22", 22), None); // 空 host
        assert_eq!(parse_address_line("[fe80::1]:bad", 22), None); // 端口非法
    }

    #[test]
    fn endpoints_host_first_dedup_preserve_order() {
        let cfg = RemoteConfig {
            host: "pi.local".into(),
            label: "pi".into(),
            port: 22,
            user: "pi".into(),
            key_path: None,
            daemon_path: "d".into(),
            host_key_fingerprint: None,
            addresses: vec![
                "10.0.0.2".into(),
                "pi.local".into(),    // 与 host 重复 → 去重
                "10.0.0.2:22".into(), // 与上面同 (host,port) → 去重
                "pub.example.com:2222".into(),
                "".into(), // 空行跳过
            ],
            jump: None,
            daemonless: false,
        };
        assert_eq!(
            cfg.endpoints(),
            vec![
                ep("pi.local", 22),
                ep("10.0.0.2", 22),
                ep("pub.example.com", 2222),
            ]
        );
    }

    #[test]
    fn endpoints_empty_addresses_is_just_host() {
        let cfg = RemoteConfig {
            host: "h".into(),
            label: String::new(),
            port: 2200,
            user: "u".into(),
            key_path: None,
            daemon_path: "d".into(),
            host_key_fingerprint: None,
            addresses: vec![],
            jump: None,
            daemonless: false,
        };
        assert_eq!(cfg.endpoints(), vec![ep("h", 2200)]);
    }

    // === F45：winner_order（last-good 排首）===

    #[test]
    fn winner_order_puts_last_good_first() {
        let eps = vec![ep("a", 22), ep("b", 22), ep("c", 22)];
        // last-good = b → b 排首，其余保序
        assert_eq!(
            winner_order(eps.clone(), Some(&ep("b", 22))),
            vec![ep("b", 22), ep("a", 22), ep("c", 22)]
        );
        // last-good = 已移除的 endpoint → 无视，原序
        assert_eq!(
            winner_order(eps.clone(), Some(&ep("gone", 22))),
            eps.clone()
        );
        // 无 last-good → 原序
        assert_eq!(winner_order(eps.clone(), None), eps);
        // last-good 已在首位 → 幂等
        assert_eq!(winner_order(eps.clone(), Some(&ep("a", 22))), eps);
    }

    // === F45：race_connect 编排（可控 mock，不依赖真 SSH）===
    // 用一个内存 TCP listener 模拟「快地址」（accept 即断=握手必失败但 TCP 连得上），
    // 及不存在端口模拟「立即拒绝」；黑洞地址模拟握手挂起。断言编排语义：首个可用者
    // 决定结果、全失败聚合、看门狗生效。注：这些测走 race_connect 的错误路径（无真
    // SSH server 故握手都失败），验证的是编排（顺序/聚合/超时/取消），非握手成功路径。

    use tokio::net::TcpListener;

    async fn dead_port() -> u16 {
        // 绑后立即释放 → 该端口大概率无监听 → connect 立即 RST（快速失败）。
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    }

    fn test_config() -> Arc<client::Config> {
        Arc::new(client::Config::default())
    }

    /// race_connect 的 Ok 分支持有不实现 Debug 的 Handle,不能直接 unwrap_err；
    /// 这个 helper 压成错误串便于断言错误路径。
    fn race_err(
        r: Result<
            (
                client::Handle<ClientHandler>,
                Arc<Mutex<Option<String>>>,
                Endpoint,
            ),
            String,
        >,
    ) -> String {
        match r {
            Ok(_) => panic!("expected Err, got a live connection"),
            Err(e) => e,
        }
    }

    #[tokio::test]
    async fn race_all_dead_aggregates_errors() {
        let p1 = dead_port().await;
        let p2 = dead_port().await;
        let order = vec![ep("127.0.0.1", p1), ep("127.0.0.1", p2)];
        let err =
            race_err(race_connect(test_config(), None, order, Duration::from_secs(5), None).await);
        // trap #3：聚合报告，含「所有地址连接失败」且提到两个地址（至少首个立即失败）。
        assert!(err.contains("所有地址连接失败"), "应聚合: {err}");
        assert!(err.contains(&p1.to_string()), "应含首地址: {err}");
    }

    #[tokio::test]
    async fn race_watchdog_times_out_on_blackhole() {
        // 10.255.255.1 = 保留黑洞地址，TCP connect 挂起 → 看门狗到点整批 abort。
        let order = vec![ep("10.255.255.1", 22)];
        let start = std::time::Instant::now();
        let err = race_err(
            race_connect(test_config(), None, order, Duration::from_millis(400), None).await,
        );
        assert!(err.contains("握手超时"), "应超时: {err}");
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "看门狗应在 deadline 附近返回,不吊死"
        );
    }

    #[tokio::test]
    async fn race_single_endpoint_dead_reports_that_endpoint() {
        // 单地址退化路径：错误里报该地址（保留老实现的可诊断性）。
        let p = dead_port().await;
        let order = vec![ep("127.0.0.1", p)];
        let err =
            race_err(race_connect(test_config(), None, order, Duration::from_secs(5), None).await);
        assert!(err.contains(&p.to_string()), "单地址错误应含该地址: {err}");
    }

    #[tokio::test]
    async fn race_empty_order_errors_cleanly() {
        let err =
            race_err(race_connect(test_config(), None, vec![], Duration::from_secs(1), None).await);
        assert!(err.contains("无可用地址"), "空 order: {err}");
    }

    // === F45 / D 审计 R-1：胜者 happy-path（live server 胜、慢地址被弃）===
    // 起一个 mock russh server（run_stream 自动完成 KEX/握手,握手成功即客户端 Ok——race
    // 只到握手,不需要真鉴权）。expected_fp=None 走 TOFU 接受该 mock key。

    /// mock server 用的固定 ed25519 host key（ssh-keygen 生成）。
    const MOCK_SERVER_KEY: &str = "\
-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACABVcXnVsSgWL3RAZE1r7ebLdEi510GsSqfaTYYzM26GwAAAJiEWV/KhFlf
ygAAAAtzc2gtZWQyNTUxOQAAACABVcXnVsSgWL3RAZE1r7ebLdEi510GsSqfaTYYzM26Gw
AAAEDRp5kloww4Jpr8K56RETPX0tLdId9XD8a+yNz5Tx0XOQFVxedWxKBYvdEBkTWvt5st
0SLnXQaxKp9pNhjMzbobAAAAD2Y0NS1tb2NrLXNlcnZlcgECAwQFBg==
-----END OPENSSH PRIVATE KEY-----";

    struct MockServer;
    impl russh::server::Handler for MockServer {
        type Error = russh::Error;
    }

    fn mock_server_config() -> Arc<russh::server::Config> {
        let key = russh::keys::PrivateKey::from_openssh(MOCK_SERVER_KEY).expect("parse mock key");
        Arc::new(russh::server::Config {
            keys: vec![key],
            ..Default::default()
        })
    }

    /// 起一个只接一条连接的 mock SSH server,返回其监听地址。
    async fn spawn_mock_server() -> Endpoint {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let _ = russh::server::run_stream(mock_server_config(), stream, MockServer).await;
            }
        });
        ep("127.0.0.1", addr.port())
    }

    /// 从 race_connect 的 Ok 分支取胜者 Endpoint（Handle 不实现 Debug,丢弃即关连接）。
    fn race_win(
        r: Result<
            (
                client::Handle<ClientHandler>,
                Arc<Mutex<Option<String>>>,
                Endpoint,
            ),
            String,
        >,
    ) -> Endpoint {
        match r {
            Ok((_h, _cell, ep)) => ep,
            Err(e) => panic!("expected a winner, got Err: {e}"),
        }
    }

    #[tokio::test]
    async fn race_live_server_wins_when_first() {
        let live = spawn_mock_server().await;
        // live 排 i=0 立即拨、黑洞 i=1 延迟 → live 握手先成功即胜。
        let order = vec![live.clone(), ep("10.255.255.1", 22)];
        let win =
            race_win(race_connect(test_config(), None, order, Duration::from_secs(5), None).await);
        assert_eq!(win, live, "live server 应胜出");
    }

    #[tokio::test]
    async fn race_live_server_wins_when_blackhole_first() {
        // 黑洞排首(i=0 立即拨但永不完成)、live 排 i=1(250ms 后拨)——慢地址被弃,live 仍胜。
        // 佐证 trap #8:首地址挂起不吊死整批,后位可达地址照样赢。
        let live = spawn_mock_server().await;
        let order = vec![ep("10.255.255.1", 22), live.clone()];
        let win =
            race_win(race_connect(test_config(), None, order, Duration::from_secs(5), None).await);
        assert_eq!(win, live, "首地址黑洞时后位 live 仍应胜出");
    }

    // === F46：连接分阶段事件 ===

    #[test]
    fn classify_stage_buckets() {
        assert_eq!(classify_stage("Connection refused (os error 111)"), "tcp");
        assert_eq!(classify_stage("No route to host"), "tcp");
        assert_eq!(classify_stage("operation timed out"), "timeout");
        assert_eq!(classify_stage("握手超时"), "timeout");
        assert_eq!(classify_stage("host key mismatch"), "hostkey");
        assert_eq!(classify_stage("Unknown server key"), "hostkey");
        assert_eq!(classify_stage("something else entirely"), "other");
    }

    /// 收集 Channel emit 的阶段 kind（send→on_message(InvokeResponseBody::Json)）。
    fn collecting_channel() -> (tauri::ipc::Channel<ConnectStage>, Arc<Mutex<Vec<String>>>) {
        let sink = Arc::new(Mutex::new(Vec::<String>::new()));
        let s2 = Arc::clone(&sink);
        let ch = tauri::ipc::Channel::new(move |body: tauri::ipc::InvokeResponseBody| {
            if let tauri::ipc::InvokeResponseBody::Json(json) = body {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                    if let Some(k) = v.get("kind").and_then(|k| k.as_str()) {
                        s2.lock().unwrap().push(k.to_string());
                    }
                }
            }
            Ok(())
        });
        (ch, sink)
    }

    #[tokio::test]
    async fn race_emits_dialing_hostkey_won_for_live_server() {
        let live = spawn_mock_server().await;
        let (ch, sink) = collecting_channel();
        let _ = race_win(
            race_connect(
                test_config(),
                None,
                vec![live],
                Duration::from_secs(5),
                Some(ch),
            )
            .await,
        );
        let kinds = sink.lock().unwrap().clone();
        assert!(
            kinds.contains(&"dialing".to_string()),
            "缺 dialing: {kinds:?}"
        );
        assert!(
            kinds.contains(&"hostKey".to_string()),
            "缺 hostKey: {kinds:?}"
        );
        assert!(kinds.contains(&"won".to_string()), "缺 won: {kinds:?}");
    }

    #[tokio::test]
    async fn race_emits_dialing_and_failed_for_dead_address() {
        let p = dead_port().await;
        let (ch, sink) = collecting_channel();
        let _ = race_err(
            race_connect(
                test_config(),
                None,
                vec![ep("127.0.0.1", p)],
                Duration::from_secs(3),
                Some(ch),
            )
            .await,
        );
        let kinds = sink.lock().unwrap().clone();
        assert!(
            kinds.contains(&"dialing".to_string()),
            "缺 dialing: {kinds:?}"
        );
        assert!(
            kinds.contains(&"failed".to_string()),
            "缺 failed: {kinds:?}"
        );
        assert!(
            !kinds.contains(&"won".to_string()),
            "死地址不应 won: {kinds:?}"
        );
    }

    #[tokio::test]
    async fn race_emitter_none_still_works() {
        // emitter=None 路径不 panic、与 F45 行为等价（此处验死地址聚合）。
        let p = dead_port().await;
        let err = race_err(
            race_connect(
                test_config(),
                None,
                vec![ep("127.0.0.1", p)],
                Duration::from_secs(3),
                None,
            )
            .await,
        );
        assert!(err.contains(&p.to_string()));
    }

    // === F45：winner_address（喂 remote-launch 的拨号地址）===

    fn cfg_with(label: &str, host: &str, port: u16, addresses: Vec<String>) -> RemoteConfig {
        RemoteConfig {
            host: host.into(),
            label: label.into(),
            port,
            user: "u".into(),
            key_path: None,
            daemon_path: "d".into(),
            host_key_fingerprint: None,
            addresses,
            jump: None,
            daemonless: false,
        }
    }

    #[test]
    fn winner_address_falls_back_to_host_when_no_last_good() {
        let cfg = cfg_with("wa-none", "h.example", 2200, vec!["10.0.0.9".into()]);
        assert_eq!(winner_address(&cfg), ep("h.example", 2200));
    }

    #[test]
    fn winner_address_uses_last_good_then_invalidates_on_config_change() {
        // 用独立 origin 避免与其它测试共享的 last-good store 串味。
        let cfg = cfg_with("wa-lg", "h.example", 22, vec!["10.0.0.9".into()]);
        record_last_good("wa-lg", &ep("10.0.0.9", 22));
        assert_eq!(
            winner_address(&cfg),
            ep("10.0.0.9", 22),
            "已连过 → last-good 胜者"
        );
        // 配置改掉备用地址 → 旧 last-good 不在 endpoints 里 → 回退 host。
        let cfg2 = cfg_with("wa-lg", "h.example", 22, vec![]);
        assert_eq!(
            winner_address(&cfg2),
            ep("h.example", 22),
            "配置变更失效 last-good"
        );
    }
}

#[cfg(test)]
mod reannounce_tests {
    use super::{collect_reannounce, AnnouncedMeta};
    use std::collections::HashMap;

    fn meta(origin: &str, sid: &str, status: Option<&str>) -> AnnouncedMeta {
        AnnouncedMeta {
            payload: crate::bridge::RemoteSessionAddedPayload {
                session_id: sid.into(),
                origin: origin.into(),
                kind: None,
                attachable: None,
                cwd: Some("/p".into()),
                name: None,
            },
            status: status.map(str::to_string),
            waiting_for: None,
        }
    }

    /// F28 DoD：重发快照收集——拍平 + (origin, sid) 稳定排序 + status 保真。
    #[test]
    fn collect_flattens_sorts_and_preserves_status() {
        let mut reg: HashMap<String, HashMap<String, AnnouncedMeta>> = HashMap::new();
        reg.entry("pi".into())
            .or_default()
            .insert("sid-b".into(), meta("pi", "sid-b", Some("busy")));
        reg.entry("pi".into())
            .or_default()
            .insert("sid-a".into(), meta("pi", "sid-a", None));
        reg.entry("aya".into())
            .or_default()
            .insert("sid-z".into(), meta("aya", "sid-z", Some("waiting")));
        let out = collect_reannounce(&reg);
        let keys: Vec<(String, String)> = out
            .iter()
            .map(|m| (m.payload.origin.clone(), m.payload.session_id.clone()))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("aya".into(), "sid-z".into()),
                ("pi".into(), "sid-a".into()),
                ("pi".into(), "sid-b".into()),
            ],
            "稳定 (origin, sid) 排序"
        );
        assert_eq!(out[0].status.as_deref(), Some("waiting"));
        assert_eq!(out[2].status.as_deref(), Some("busy"));
        assert!(collect_reannounce(&HashMap::new()).is_empty());
    }
}

#[cfg(test)]
mod snapshot_tail_tests {
    use super::parse_snapshot_meta;

    #[test]
    fn meta_parses_and_content_lines_dont() {
        assert_eq!(
            parse_snapshot_meta(r#"{"kind":"snapshot_meta","total":100,"tail_from":95}"#),
            Some((100, 95))
        );
        // 普通 jsonl 行（含 kind 字段的行也不行——kind 值不匹配）
        assert_eq!(parse_snapshot_meta(r#"{"type":"user","uuid":"u1"}"#), None);
        assert_eq!(parse_snapshot_meta(r#"{"kind":"line","seq":0}"#), None);
        assert_eq!(parse_snapshot_meta("not json"), None);
    }

    /// 两段编号映射（真函数）：到达序 → 行号（尾段先到）。
    #[test]
    fn tail_numbering_maps_arrival_to_line_numbers() {
        use super::tail_seq;
        // 到达序：尾段 [3,4]，头段 [0,1,2]
        assert_eq!(
            (0..5).map(|i| tail_seq(i, 5, 3)).collect::<Vec<_>>(),
            vec![3, 4, 0, 1, 2],
            "全部行号恰覆盖 0..total 且顺序=尾部优先"
        );
        // 全尾（tail_from=0）与空文件退化
        assert_eq!(
            (0..3).map(|i| tail_seq(i, 3, 0)).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(tail_seq(0, 0, 0), 0);
    }

    /// meta 关系防御：tail_from > total 拒收（防远端损坏输出打乱 seq 空间）。
    #[test]
    fn malformed_meta_rejected() {
        use super::parse_snapshot_meta;
        assert_eq!(
            parse_snapshot_meta(r#"{"kind":"snapshot_meta","total":5,"tail_from":10}"#),
            None
        );
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    /// 行计数口径必须与 daemon read_new_lines 一字一致：BOM+全空白跳过。
    #[test]
    fn snapshot_line_countable_matches_daemon_semantics() {
        assert!(snapshot_line_countable(r#"{"a":1}"#));
        assert!(snapshot_line_countable("\u{feff}{\"a\":1}")); // BOM+内容 → 计
        assert!(!snapshot_line_countable("")); // 空行 → 跳
        assert!(!snapshot_line_countable("   ")); // 全空白 → 跳
        assert!(!snapshot_line_countable("\u{feff}")); // 纯 BOM → 跳
        assert!(!snapshot_line_countable("\u{feff}  \t")); // BOM+空白 → 跳
    }

    /// 队列语义：sid 幂等、priority 优先出队、close 后清空账再 None。
    #[tokio::test]
    async fn snapshot_queue_priority_idempotent_and_close() {
        fn item(sid: &str, path: &str) -> SnapshotItem {
            SnapshotItem {
                sid: sid.into(),
                path: path.into(),
                expected_lines: None,
            }
        }
        let q = SnapshotQueue::new();
        q.push(item("s1", "/p1"));
        q.push(item("s2", "/p2"));
        q.push(item("s3", "/p3"));
        q.push(item("s2", "/p2-dup")); // 幂等：不重拉
                                       // priority=s2 → 先出 s2
        let got = q.pop(Some("s2".into())).await.unwrap();
        assert_eq!((got.sid.as_str(), got.path.as_str()), ("s2", "/p2"));
        // priority 不在队 → FIFO
        let got = q.pop(Some("nope".into())).await.unwrap();
        assert_eq!(got.sid, "s1");
        // Batch8 审计 D-B1：cancel 摘排队项 + 标记取消 + seen 可重入队
        q.cancel("s3");
        assert!(q.is_cancelled("s3"), "cancel 后 inflight 检查命中");
        q.push(item("s3", "/p3-again")); // removed→re-added：解除取消、重新入队
        assert!(!q.is_cancelled("s3"), "重新宣告解除取消标记");
        let got = q.pop(None).await.unwrap();
        assert_eq!(got.path, "/p3-again");
        // Batch8 审计 D-B1：close 立即作废排队项（不清账）
        q.push(item("s4", "/p4"));
        q.close();
        assert!(q.pop(None).await.is_none(), "close 后排队项作废");
        assert!(q.is_cancelled("s4"), "close 后 inflight 检查也命中（作废）");
    }

    /// close 唤醒等待中的 pop（分发器不悬挂）。
    #[tokio::test]
    async fn snapshot_queue_close_wakes_waiting_pop() {
        let q = SnapshotQueue::new();
        let q2 = q.clone();
        let waiter = tokio::spawn(async move { q2.pop(None).await });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        q.close();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), waiter)
                .await
                .expect("pop 必须被 close 唤醒")
                .unwrap()
                .is_none()
        );
    }
}
