//! 〔C2 · `设计/05 §13.5`〕**界面进程里最后一份 `russh` 拨号 —— 只剩 SFTP 一个用户。**
//!
//! # 它为什么还在
//!
//! `设计/05 §13` 把拨 SSH 整个搬进了后端的拨号代理（`src/backend/dial/`，宿主 `dial_host.rs`）：
//! 后端长连接流、十来处一次性查询、收全 exec、测试连接、端口转发全走代理，**没有进程内回落**（`D11`）。
//! **唯一没搬的是 SFTP**：`sftp.rs` / `sftp_pool.rs` 是第三波 `F7c` 独占的写区，而它要的不是一条字节流，
//! 是一个 russh 连接句柄 —— 在**同一条连接上**再开第 2、3… 条 sftp 通道（池 ＋ 车道预算 `6 − 4 = 2`）。
//! 代理今天一个进程一条 channel；让 SFTP 走代理，要么「一条通道一条 SSH 连接」（预算的意义变了），
//! 要么等「常驻本机后端复用连接」那个要改协议的形状 —— 两条都是要人拍板的题，不在本拍。
//! 另外后端的远端写那一层（`readonly_guard`）把「请求 sftp 子系统」判作远端写能力，红线 `I7` 未裁。
//!
//! ⇒ 本文件是**原样搬来**的那一份（`ssh_source.rs` 的 `ClientHandler` / `connect_session` / 竞速 / 跳板 /
//! Windows agent），行为一个字没改，**唯一调用方是 `sftp.rs`**（判据钉着）。
//! 🔴 **F7c 换走那天整份删**，`Cargo.toml` 里的 `russh` 随之删 —— 那是「`russh` 出界面 crate」的最后一步。
//! ⚠ 与后端那一份是**两份实现**（host key 校验 · 竞速 · 跳板 · agent）：它们会漂，而这正是它要尽快删的理由。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client;
use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKey};

use crate::ssh_link::ConnectStage;
use crate::ssh_source::{last_good_for, record_last_good, winner_order, Endpoint, RemoteConfig};

/// russh client handler：负责 host key 校验（check_server_key）。
///
/// 持有期望指纹，`check_server_key` 据此决定接受 / 拒绝（见该方法注释）。
/// 另持有一个共享 cell（`observed_fingerprint`），**无论接受 / 拒绝**都把实际看到的
/// server key 指纹写进去 —— Tier 1（issue #15）的「测试连接」据此向用户展示指纹、
/// 供 TOFU→严格校验固化（known_hosts 式）。
/// 〔C2〕原来这里还有一个 `SshSession` 类型别名（端口转发存 russh 句柄用）—— 端口转发搬进拨号代理之后没人用了，删掉。〔散文墓碑〕
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

/// 连接 + 鉴权的共享实现（〔C2〕原来被长连接数据源与测试连接复用；今天**只剩 `sftp.rs`** 一个调用方，
/// 那两处改经拨号代理了）。
///
/// 返回已鉴权的 session + 共享的 `observed_fingerprint` cell（握手时 check_server_key 已
/// 把实际 server key 指纹写进去，调用方可读出展示 / 固化）。
///
/// 鉴权策略（Tier 1, issue #15）：
/// - `cfg.key_path = Some(path)`：publickey 鉴权（既有默认路径，最稳）。
/// - `cfg.key_path = None`：尝试 ssh-agent（Windows 命名管道），枚举 agent 身份逐个
///   `authenticate_publickey_with`。agent 不可用 / 无匹配身份 → 返回清晰 Err。
///
/// Batch14-F45：happy-eyeballs 竞发阶梯（第 i 个地址延迟 i*STAGGER 起拨，首个成功者胜后
/// 其余在飞连接被 abort）。250ms 是 RFC 8305 常用值。
const RACE_STAGGER: Duration = Duration::from_millis(250);
/// 握手看门狗默认上限（长连接 inactivity_timeout=None 时用）——黑洞地址 TCP 连上后
/// 握手无限阻塞时兜底,到点整批 abort（drop 关 socket）。
const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(45);

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
    // keepalive 超时 + backend EOF 检出，inactivity_timeout 对长连接置 None。
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
    // ★ F05 下半：**SSH 握手这一段**（TCP + 握手 + host key 校验 + 鉴权）。
    // 它被 `connect_and_exec` 那条埋点整个包在里面 —— 分开量才知道
    //「起流慢」到底慢在登录还是慢在后端起来。
    let t_handshake = std::time::Instant::now();
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

    tracing::info!(
        "[perf] ssh_source [{origin}] SSH 握手+鉴权 {}ms（TCP+握手+指纹校验+auth；\
         winner={winner:?}）",
        t_handshake.elapsed().as_millis()
    );
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
#[cfg(test)]
#[path = "../../../tests/bridge/inproc_dial_tests.rs"]
mod tests;
