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
use crate::session_map::SessionChange;
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
pub(crate) struct ClientHandler {
    expected_fingerprint: Option<String>,
    /// check_server_key 观察到的实际指纹回传通道（与调用方共享）。
    observed_fingerprint: Arc<Mutex<Option<String>>>,
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
            set.spawn(async move {
                if i > 0 {
                    tokio::time::sleep(RACE_STAGGER * i as u32).await;
                }
                let cell: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
                let handler = ClientHandler {
                    expected_fingerprint: fp,
                    observed_fingerprint: Arc::clone(&cell),
                };
                match client::connect(config, (ep.host.as_str(), ep.port), handler).await {
                    Ok(h) => Ok((h, cell, ep)),
                    Err(e) => Err(format!("{}:{} {e}", ep.host, ep.port)),
                }
            });
        }

        let mut errors: Vec<String> = Vec::new();
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok(winner)) => {
                    // 首个成功者胜；drop set → abort 其余在飞（关 socket，不等死地址超时）。
                    set.abort_all();
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

pub(crate) async fn connect_session(
    cfg: &RemoteConfig,
    inactivity_timeout: Option<Duration>,
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
    let origin = cfg.origin_label();
    let order = winner_order(cfg.endpoints(), last_good_for(&origin).as_ref());
    let deadline = inactivity_timeout.unwrap_or(HANDSHAKE_DEADLINE);
    let (mut session, observed_fingerprint, winner) = race_connect(
        Arc::clone(&config),
        cfg.host_key_fingerprint.clone(),
        order,
        deadline,
    )
    .await?;

    // RSA key 需要协商出 server 支持的 hash alg；非 RSA key 时 flatten 成 None。
    let best_hash = session
        .best_supported_rsa_hash()
        .await
        .map_err(|e| format!("协商 rsa hash 失败: {e}"))?
        .flatten();

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
        }
        None => {
            authenticate_via_agent(&mut session, &cfg.user, best_hash).await?;
        }
    }

    // D 审计建议-1：last-good = 上次**完整成功**（握手+鉴权）的地址。放在鉴权成功后,
    // 避免 TOFU×异机误配时「粘住」一个连得上但认证失败的地址（下次仍先拨它、仍失败,
    // 真机永不被试）。正常固化下 A/B 同机同 key,放前放后等价;此处取更严谨语义。
    record_last_good(&origin, &winner);

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
/// Batch7-F24/Batch8-F26 版本门控决策（纯函数，矩阵单测）：只对**确认为当前
/// 版本**的 daemon 传流模式 flag——旧 daemon 会把未知参数当一次性查询处理后
/// 退出（无 hello → 重连死循环），确认不了（手动部署 / ~ 路径 / 无内嵌 arch /
/// stale 内嵌被拒）一律降级不传：全量推流 = 2.18.0 行为，功能退化但连接正常。
/// tail_only=true 时历史改走旁路快照（实时通道不再背 64MB 级重放，拥塞根除）。
fn decide_stream_flags(
    confirmed_build: Option<&str>,
    expected: &str,
    show_bg: bool,
) -> (bool, bool) {
    let confirmed = confirmed_build == Some(expected);
    (show_bg && confirmed, confirmed)
}

#[cfg(test)]
mod stream_flag_gate_tests {
    use super::decide_stream_flags;

    /// F26 DoD：版本门控矩阵——未确认（None/旧版本）恒 (false,false)；
    /// 确认后 tail_only 恒开、with_bg 随 showBgSessions。
    #[test]
    fn version_gate_matrix() {
        const EXP: &str = "p1f-tail-snapshot";
        assert_eq!(decide_stream_flags(None, EXP, true), (false, false));
        assert_eq!(decide_stream_flags(None, EXP, false), (false, false));
        assert_eq!(
            decide_stream_flags(Some("p1e-bg-tree"), EXP, true),
            (false, false),
            "旧版本确认值 ≠ 期望 → 全降级"
        );
        assert_eq!(decide_stream_flags(Some(EXP), EXP, true), (true, true));
        assert_eq!(
            decide_stream_flags(Some(EXP), EXP, false),
            (false, true),
            "关 showBgSessions 只关 with_bg，tail-only 照开"
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
    let (session, _fp) = connect_session(cfg, None).await?;

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
    SessionRemoved { sid: String },
    /// issue #32：远端 daemon 发送通道拥塞、丢了 `dropped` 帧（慢 SSH 管道）。
    /// monitor 收到后经 SS-F remote-health 通道提示用户可能丢实时行。
    Overflow { dropped: u64 },
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
            Some(InboundFrame::Hello {
                v,
                build_id,
                host_arch,
                claude_dir,
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
            Some(InboundFrame::SessionRemoved { sid })
        }
        "overflow" => {
            // issue #32：dropped 必需且为数字；缺/错则当坏帧跳过（不 panic）。
            let dropped = obj.get("dropped")?.as_u64()?;
            Some(InboundFrame::Overflow { dropped })
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
    let mut hello_confirmed: Option<String> = None;
    loop {
        connected.store(false, Ordering::Release);
        // Batch9 账本：HashSet → HashMap<sid, AnnouncedMeta>（F27 status 写回 +
        // F28 frontend-ready 重发的数据源）。归档清算语义不变（keys = 存活 sid）。
        let mut announced: std::collections::HashMap<String, AnnouncedMeta> =
            std::collections::HashMap::new();
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
            tracing::warn!("ssh_source hello 自愈轮未收到 hello,回退降级模式(daemon 可能被换旧)");
            hello_confirmed = None;
        }
        // Batch9-F28：连接结束清本 host 的 registry（断连=骨架不该再被 F5 重建；
        // 重连宣告会重新填充）。
        announced_registry()
            .lock()
            .unwrap()
            .remove(&cfg.origin_label());
        // 每轮都归档本次连接残留的 announced sid（保持原 FIX 2 归档契约）
        if !announced.is_empty() {
            let removed: Vec<String> = announced.into_keys().collect();
            tracing::info!(
                "ssh_source connection ended; archiving {} remote session(s)",
                removed.len()
            );
            if let Err(e) = session_changes.send(SessionChange {
                added: vec![],
                removed,
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
    hello_confirmed: &mut Option<String>,
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
    // Batch7-F24/Batch8-F26：只对**确认为当前版本**的 daemon 传流模式 flag——
    // 旧 daemon 会把未知参数当一次性查询处理后退出（无 hello → 重连死循环），
    // 确认不了（手动部署 / ~ 路径 / 无内嵌 arch）一律降级不传（功能退化但连接
    // 正常：全量推流 = 2.18.0 行为）。
    // v2.22.1 hello 自愈:上一轮 hello 已自证 daemon==当前版本 → 以 hello 为准。
    // **hello 优先**于部署侧结论:部署侧可能返回 Some(陈旧内嵌的身份)(≠期望,
    // 会压回降级)——首版用 or_else 只补 None,被 E2E 抓出无限重连循环(部署侧
    // Some(p1x)≠期望 → hello 账本永不被采纳 → 每轮降级→hello→重连)。
    // hello_confirmed 只在 ==EXPECTED 时写入,优先采纳恒安全。
    let confirmed_build = hello_confirmed.clone().or(confirmed_build);
    let (with_bg, tail_only) = decide_stream_flags(
        confirmed_build.as_deref(),
        EXPECTED_DAEMON_BUILD_ID,
        crate::load_show_bg_sessions(),
    );
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
            }) => {
                tracing::info!(
                    "ssh_source daemon hello: v={v} build_id={build_id} host_arch={host_arch} claude_dir={claude_dir}"
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
                // v2.22.1:本轮跑在降级模式(未传 --tail-only,部署侧确认失败)时——
                // ① daemon 自报 == 当前版本 → 记 hello 自愈账,立即重连升级流模式
                //   (connected 已置 true → 退避重置为 MIN,~2s 内带 flag 回来);
                // ② 确实是旧 daemon → 降级可见化:此前只写日志,用户看到的是「bg 会话
                //   消失+拥塞复发」却无从归因(实测连环误诊)——经 remote-health 提示。
                if !tail_only {
                    if build_id == EXPECTED_DAEMON_BUILD_ID {
                        *hello_confirmed = Some(build_id.clone());
                        return Err(format!(
                            "daemon hello 自证为当前版本({build_id})——重连升级流模式(tail-only/with-bg)"
                        ));
                    }
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
            Some(InboundFrame::SessionRemoved { sid }) => {
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
                    removed: vec![sid],
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
            None => {
                // 未知 kind / 坏帧 / 非 JSON：跳过，绝不 panic、绝不中断流。
                tracing::warn!("ssh_source skipping unparseable/unknown frame: {line}");
            }
        }
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
#[serde(rename_all = "camelCase")]
pub struct ResolvedHost {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// 第一个**存在**的 IdentityFile（`~` 已展开）。无则 None（用户可改走 agent）。
    pub key_path: Option<String>,
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
    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut user: Option<String> = None;
    let mut key_path: Option<String> = None;

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
            _ => {}
        }
    }

    Ok(ResolvedHost {
        // hostname 缺省回退到别名本身（ssh -G 通常总会给 hostname，但稳妥兜底）。
        host: host.unwrap_or_else(|| alias.clone()),
        port: port.unwrap_or(22),
        user: user.unwrap_or_default(),
        key_path,
    })
}

/// 「测试连接」的结果（issue #15 Part 2）。serde camelCase 与前端渲染对齐。
///
/// 偏好「返回 populated 结果 + message」而非 Err：让 UI 能展示部分成功
/// （如「SSH 连上了，但 daemon 没响应/未部署」）。仅参数级硬错误才返回 Err。
#[derive(Debug, Clone, Serialize)]
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
pub async fn test_remote_connection(cfg: RemoteConfig) -> Result<ConnTestResult, String> {
    let mut result = ConnTestResult {
        ssh_ok: false,
        fingerprint: None,
        endpoint: None,
        daemon_ok: false,
        daemon_hello: None,
        message: String::new(),
    };

    // 1. 连接 + 鉴权（短 inactivity：测试连接不需要长保活）。
    let (session, observed) = match connect_session(&cfg, Some(Duration::from_secs(30))).await {
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
                }) => Ok(Some(format!(
                    "v={v} build={build_id} arch={host_arch} claude_dir={claude_dir}"
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
            }
        );
    }

    /// #33：hello 缺 build_id → None（按必需字段，坏帧跳过；既有 daemon 总在发它）。
    #[test]
    fn hello_missing_build_id_returns_none() {
        let line = r#"{"kind":"hello","v":1,"host_arch":"x86_64","claude_dir":"/c"}"#;
        assert_eq!(parse_frame(line), None);
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
                sid: "s-dead".to_string()
            }
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
        let err = race_err(race_connect(test_config(), None, order, Duration::from_secs(5)).await);
        // trap #3：聚合报告，含「所有地址连接失败」且提到两个地址（至少首个立即失败）。
        assert!(err.contains("所有地址连接失败"), "应聚合: {err}");
        assert!(err.contains(&p1.to_string()), "应含首地址: {err}");
    }

    #[tokio::test]
    async fn race_watchdog_times_out_on_blackhole() {
        // 10.255.255.1 = 保留黑洞地址，TCP connect 挂起 → 看门狗到点整批 abort。
        let order = vec![ep("10.255.255.1", 22)];
        let start = std::time::Instant::now();
        let err =
            race_err(race_connect(test_config(), None, order, Duration::from_millis(400)).await);
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
        let err = race_err(race_connect(test_config(), None, order, Duration::from_secs(5)).await);
        assert!(err.contains(&p.to_string()), "单地址错误应含该地址: {err}");
    }

    #[tokio::test]
    async fn race_empty_order_errors_cleanly() {
        let err = race_err(race_connect(test_config(), None, vec![], Duration::from_secs(1)).await);
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
        let win = race_win(race_connect(test_config(), None, order, Duration::from_secs(5)).await);
        assert_eq!(win, live, "live server 应胜出");
    }

    #[tokio::test]
    async fn race_live_server_wins_when_blackhole_first() {
        // 黑洞排首(i=0 立即拨但永不完成)、live 排 i=1(250ms 后拨)——慢地址被弃,live 仍胜。
        // 佐证 trap #8:首地址挂起不吊死整批,后位可达地址照样赢。
        let live = spawn_mock_server().await;
        let order = vec![ep("10.255.255.1", 22), live.clone()];
        let win = race_win(race_connect(test_config(), None, order, Duration::from_secs(5)).await);
        assert_eq!(win, live, "首地址黑洞时后位 live 仍应胜出");
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
