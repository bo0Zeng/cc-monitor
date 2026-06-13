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
/// **serde camelCase 必须与前端 RemoteConfig / lib.rs::load_remote_config 严格一致**：
/// host / port / user / keyPath / daemonPath / hostKeyFingerprint。前端多发的 `enabled`
/// 字段被忽略（serde 默认丢弃未知字段，测试连接不关心 enabled）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConfig {
    pub host: String,
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
}

fn default_ssh_port() -> u16 {
    22
}

/// 前端可选字段以空字符串下发（见 remote-section.ts 注释）；反序列化时把 `""` 归一成
/// `None`，与 lib.rs::load_remote_config 的 `.filter(|s| !s.is_empty())` 语义一致。
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
struct ClientHandler {
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
                    tracing::error!(
                        "ssh host key MISMATCH: expected {expected}, got {actual}; rejecting connection"
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
async fn connect_session(
    cfg: &RemoteConfig,
    inactivity_timeout: Option<Duration>,
) -> Result<(client::Handle<ClientHandler>, Arc<Mutex<Option<String>>>), String> {
    let observed_fingerprint: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

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

    let handler = ClientHandler {
        expected_fingerprint: cfg.host_key_fingerprint.clone(),
        observed_fingerprint: Arc::clone(&observed_fingerprint),
    };

    let mut session = client::connect(config, (cfg.host.as_str(), cfg.port), handler)
        .await
        .map_err(|e| format!("ssh connect {}:{} 失败: {e}", cfg.host, cfg.port))?;

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
pub async fn connect_and_exec(
    cfg: &RemoteConfig,
) -> Result<russh::ChannelStream<client::Msg>, String> {
    // 与 jsonl-watcher 不同，daemon 是长连接：inactivity_timeout=None → connect_session
    // 自动启用 30s keepalive（见 FIX 1 注释），靠 keepalive + EOF 检死链，不靠定时拆链。
    connect_and_exec_cmd(cfg, &shell_quote(&cfg.daemon_path)).await
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
    /// 握手帧：连接建立后 daemon 发一次。`v` = 协议版本，host_arch / claude_dir 用于 log
    /// 证明 daemon 真的在远端跑起来了。build_id 等额外字段被忽略（向前兼容）。
    Hello {
        v: u64,
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
    /// 远端新出现一个 session 文件。
    SessionAdded { sid: String },
    /// 远端一个 session 文件消失。
    SessionRemoved { sid: String },
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
            let host_arch = obj.get("host_arch")?.as_str()?.to_string();
            let claude_dir = obj.get("claude_dir")?.as_str()?.to_string();
            Some(InboundFrame::Hello {
                v,
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
            Some(InboundFrame::SessionAdded { sid })
        }
        "session_removed" => {
            let sid = obj.get("sid")?.as_str()?.to_string();
            Some(InboundFrame::SessionRemoved { sid })
        }
        // 未知 kind：向前兼容，跳过（调用方 warn）。绝不 panic。
        _ => None,
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
    loop {
        connected.store(false, Ordering::Release);
        let mut announced: std::collections::HashSet<String> = std::collections::HashSet::new();
        let result = stream_loop(
            &cfg,
            &replay,
            &app,
            &session_changes,
            &connected,
            &mut announced,
        )
        .await;
        // 每轮都归档本次连接残留的 announced sid（保持原 FIX 2 归档契约）
        if !announced.is_empty() {
            let removed: Vec<String> = announced.into_iter().collect();
            tracing::info!(
                "ssh_source connection ended; archiving {} remote session(s)",
                removed.len()
            );
            if let Err(e) = session_changes.send(SessionChange {
                added: vec![],
                removed,
                status_changed: vec![], // issue #23: 远端暂无 status 透传（v1 本地先行）
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
    announced: &mut std::collections::HashSet<String>,
) -> Result<(), String> {
    // issue #15：远端行的 origin 标签 = 远端主机名。前端据此给该 Tab 标题加
    // `[host]` 前缀以区分本地/远端。在进 loop 前 clone 出来。
    let host_label = cfg.host.clone();

    let stream = connect_and_exec(cfg).await?;
    let mut reader = BufReader::new(stream);
    // tokio LinesStream-free：read_line 复用 buffer，按 `\n` 切（协议保证每帧一行、
    // 帧内换行已被 daemon 转义成 `\n` 两字符，见 remote-daemon-proto/src/wire.rs）。
    let mut buf = String::new();

    loop {
        buf.clear();
        let n = reader
            .read_line(&mut buf)
            .await
            .map_err(|e| format!("ssh daemon stdout read error: {e}"))?;
        if n == 0 {
            // EOF：daemon 退出 / channel 关闭。明确报错，不静默冻结。
            return Err("ssh daemon stdout closed (EOF / connection dropped)".to_string());
        }
        let line = buf.trim_end_matches(['\n', '\r']);
        if line.is_empty() {
            continue;
        }

        match parse_frame(line) {
            Some(InboundFrame::Hello {
                v,
                host_arch,
                claude_dir,
            }) => {
                tracing::info!(
                    "ssh_source daemon hello: v={v} host_arch={host_arch} claude_dir={claude_dir}"
                );
                // 标记本次连接已健康(收到 daemon hello)，供 run() 重连循环判定是否重置退避。
                connected.store(true, Ordering::Release);
            }
            Some(InboundFrame::Line {
                session_id,
                path,
                seq,
                raw,
            }) => {
                // 与本地 watcher 完全相同的出口：batch_to_payloads → on_line_batch。
                // Phase-0 简单正确：一帧一条 batch。前端按 seq 自动排序，无视觉差异。
                let lines = vec![JsonlLine {
                    session_id,
                    path: std::path::PathBuf::from(path),
                    seq,
                    raw,
                }];
                let payloads = crate::batch_to_payloads(lines, Some(host_label.clone()));
                replay.on_line_batch(app, payloads);
            }
            Some(InboundFrame::SessionAdded { sid }) => {
                // FIX 2：记下已宣告的 sid，供连接结束时统一归档。
                announced.insert(sid.clone());
                if let Err(e) = session_changes.send(SessionChange {
                    added: vec![sid],
                    removed: vec![],
                    status_changed: vec![],
                }) {
                    tracing::warn!("ssh_source session_added send failed: {e}");
                }
            }
            Some(InboundFrame::SessionRemoved { sid }) => {
                // FIX 2：已显式 removed 的 sid 从 announced 摘掉，避免连接结束时重复归档。
                announced.remove(&sid);
                if let Err(e) = session_changes.send(SessionChange {
                    added: vec![],
                    removed: vec![sid],
                    status_changed: vec![],
                }) {
                    tracing::warn!("ssh_source session_removed send failed: {e}");
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
                    host_arch,
                    claude_dir,
                }) => Ok(Some(format!(
                    "v={v} arch={host_arch} claude_dir={claude_dir}"
                ))),
                _ => Ok(None), // 非 hello 帧 → daemon 未正常握手
            }
        }
    }
}

#[cfg(test)]
mod parse_frame_tests {
    use super::*;

    /// hello 帧解析：取 v / host_arch / claude_dir，daemon 多发的 build_id 等字段被忽略。
    #[test]
    fn parses_hello_and_ignores_build_id() {
        let line = r#"{"kind":"hello","v":1,"build_id":"abc123","host_arch":"aarch64","claude_dir":"/home/pi/.claude"}"#;
        let frame = parse_frame(line).expect("hello must parse");
        assert_eq!(
            frame,
            InboundFrame::Hello {
                v: 1,
                host_arch: "aarch64".to_string(),
                claude_dir: "/home/pi/.claude".to_string(),
            }
        );
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
                sid: "s-9".to_string()
            }
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
}
