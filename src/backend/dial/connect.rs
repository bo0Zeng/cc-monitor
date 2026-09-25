//! 连 → 校验 host key → 鉴权。**本 crate 里唯一与远端跑 SSH 传输层握手的地方**
//! （`tests/backend/dial_tests.rs::the_dial_only_happens_under_dial_home` 钉着「只在 `dial/`」）。
//!
//! 搬自界面侧 `ssh_source` 的 `connect_session` / `race_connect` / `connect_via_jump`〔散文墓碑〕 /
//! `authenticate_via_agent`〔散文墓碑〕 与 `ClientHandler` —— 那几样在界面侧**删掉了**，这里是它们唯一的家。
//! 与原来相比只有两处不同，都写在 `mod.rs` 头注「边界」一节：竞速同时起拨（不错开）；
//! ssh-agent 两个平台都有（界面侧原来只有 Windows）。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client;
use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKey};

use super::{DialRequest, Endpoint, Stage, StageSink};

/// 长连接的 SSH 层 keepalive：每 30 秒无收包发一个，连发三个无回应才判死（≈90 s 探活）。
/// **不靠 inactivity 拆链**：russh 的 inactivity 计时在无 keepalive 时会拆掉一条健康的空闲连接
/// （空闲一小时的会话很常见）。死链由 keepalive 超时 ＋ EOF 检出。
const KEEPALIVE: Duration = Duration::from_millis(30_000);

/// 短命探活（`probe`）的 inactivity 上限 —— 测试连接那条不需要保活。
const PROBE_INACTIVITY: Duration = Duration::from_millis(30_000);

/// 握手鉴权都过了的一条 SSH 连接。
pub(crate) struct Linked {
    pub(crate) session: client::Handle<Checker>,
    pub(crate) fingerprint: Option<String>,
    pub(crate) endpoint: String,
    /// 经跳板时跳板那条连接：**必须与目标连接同生命周期**（drop 它 ⇒ 隧道死 ⇒ 目标断）。
    pub(crate) _jump: Option<client::Handle<Checker>>,
    /// 〔SR1b〕这条连接上的通道预算（`pool::Budget`）：长流 · 查询 · SFTP 同一条连接，同一道闸。
    pub(crate) budget: super::pool::Budget,
}

/// host key 校验：给了期望值就严格比（比之前 `trim`），没给就 TOFU 接受并显眼 `warn`。
/// **无论接受与否**都把实得指纹写回共享格 —— 界面要拿它做 TOFU 固化与展示。
pub(crate) struct Checker {
    expected: Option<String>,
    observed: Arc<Mutex<Option<String>>>,
    stages: StageSink,
    endpoint: String,
}

impl client::Handler for Checker {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let actual = server_public_key.fingerprint(HashAlg::Sha256).to_string();
        if let Ok(mut slot) = self.observed.lock() {
            *slot = Some(actual.clone());
        }
        // 到 host key 校验 = 该地址 TCP ＋ KEX 已过。
        self.stages.emit(Stage::HostKey {
            endpoint: self.endpoint.clone(),
            fingerprint: actual.clone(),
        });
        match &self.expected {
            Some(expected) => {
                if actual == expected.trim() {
                    tracing::info!("dial: host key 指纹校验通过：{actual}");
                    Ok(true)
                } else {
                    // 失配时附上实际 key 的算法：分得开「合法换 key 类型」与「同类型 key 被换」。
                    let alg = server_public_key.algorithm();
                    tracing::error!(
                        "dial: host key 失配：期望 {expected}，实得 {actual}（alg={alg}）—— 拒绝这条连接。\
                         若服务器确系合法轮换过 host key，请在设置里「重置为 TOFU」后重连；否则可能是中间人攻击。"
                    );
                    Ok(false)
                }
            }
            None => {
                tracing::warn!(
                    "dial: **未经校验**接受 host key {actual}（TOFU）—— 界面侧应当把它固化回配置，之后转严格校验。"
                );
                Ok(true)
            }
        }
    }
}

/// 把 russh 的连接错误粗分成阶段标签（界面泳道用不同图标）。命中关键词才归类，否则 `other`。
/// 〔搬自界面侧 `ssh_source::classify_stage`，那一份随拨号一起删了 —— 只此一份。〕
pub(crate) fn classify_stage(err: &str) -> &'static str {
    let e = err.to_ascii_lowercase();
    if e.contains("refused") || e.contains("no route") || e.contains("unreachable") {
        "tcp"
    } else if e.contains("timeout") || e.contains("超时") || e.contains("timed out") {
        "timeout"
    } else if e.contains("key") || e.contains("mismatch") || e.contains("指纹") {
        "hostkey"
    } else {
        "other"
    }
}

fn label(ep: &Endpoint) -> String {
    format!("{}:{}", ep.host, ep.port)
}

fn config(probe: bool) -> Arc<client::Config> {
    Arc::new(client::Config {
        inactivity_timeout: probe.then_some(PROBE_INACTIVITY),
        keepalive_interval: (!probe).then_some(KEEPALIVE),
        ..Default::default()
    })
}

/// 多地址竞速：**同时**对每个地址起拨（TCP ＋ 握手 ＋ host key 校验，各自独立的校验器与指纹格），
/// 首个握手成功者胜、其余立即丢弃（drop 关 socket）。鉴权留给调用方只对胜者做一次
/// （防 agent 并发撞 `MaxAuthTries`）。全失败 ⇒ 聚合各地址的错误；被丢弃的输家不算错误。
async fn race(
    config: Arc<client::Config>,
    expected: Option<String>,
    order: Vec<Endpoint>,
    stages: &StageSink,
) -> Result<
    (
        client::Handle<Checker>,
        Arc<Mutex<Option<String>>>,
        Endpoint,
    ),
    (String, Option<String>),
> {
    use tokio::task::JoinSet;
    let addr_list = order.iter().map(label).collect::<Vec<_>>().join(", ");
    // 任一地址看到过的指纹（失败时也回报给界面，TOFU 展示要它）。
    let seen: Arc<Mutex<Option<String>>> = Arc::default();
    let seen_for_race = Arc::clone(&seen);
    let race = async move {
        let mut set: JoinSet<Result<_, String>> = JoinSet::new();
        for ep in order {
            let config = Arc::clone(&config);
            let stages = stages.clone();
            let observed: Arc<Mutex<Option<String>>> = Arc::default();
            let expected = expected.clone();
            let seen = Arc::clone(&seen_for_race);
            set.spawn(async move {
                let ep_label = label(&ep);
                stages.emit(Stage::Dialing {
                    endpoint: ep_label.clone(),
                });
                let checker = Checker {
                    expected,
                    observed: Arc::clone(&observed),
                    stages: stages.clone(),
                    endpoint: ep_label.clone(),
                };
                let r = client::connect(config, (ep.host.as_str(), ep.port), checker).await;
                if let Some(fp) = observed.lock().ok().and_then(|g| g.clone()) {
                    if let Ok(mut s) = seen.lock() {
                        *s = Some(fp);
                    }
                }
                match r {
                    Ok(h) => Ok((h, observed, ep)),
                    Err(e) => {
                        stages.emit(Stage::Failed {
                            endpoint: ep_label.clone(),
                            reason: format!("[{}] {e}", classify_stage(&e.to_string())),
                        });
                        Err(format!("{ep_label} {e}"))
                    }
                }
            });
        }
        let mut errors: Vec<String> = Vec::new();
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok(winner)) => {
                    set.abort_all();
                    stages.emit(Stage::Won {
                        endpoint: label(&winner.2),
                    });
                    return Ok(winner);
                }
                Ok(Err(e)) => errors.push(e),
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
    // ⚠ **这里没有握手看门狗**：本 crate 按调用形态禁 `timeout(`（`no_timer_guard`）。
    //   黑洞地址那一格由界面侧兜：它等 ack 有期限，到点就收掉本进程（drop 关 socket）。
    match race.await {
        Ok(w) => Ok(w),
        Err(e) => {
            let fp = seen.lock().ok().and_then(|g| g.clone());
            Err((format!("{e}（竞速：{addr_list}）"), fp))
        }
    }
}

/// 鉴权：配了私钥路径走 publickey；没配走 ssh-agent（平台那一半在 `platform::ssh_agent`）。
async fn authenticate(
    session: &mut client::Handle<Checker>,
    user: &str,
    key_path: Option<&str>,
    agent_sock: Option<&str>,
) -> Result<(), String> {
    // RSA key 要协商出服务端支持的 hash；非 RSA 时 flatten 成 None。
    let best_hash = session
        .best_supported_rsa_hash()
        .await
        .map_err(|e| format!("协商 rsa hash 失败: {e}"))?
        .flatten();
    match key_path.filter(|s| !s.trim().is_empty()) {
        Some(key_path) => {
            let key_pair = load_secret_key(key_path, None)
                .map_err(|e| format!("加载私钥 {key_path} 失败: {e}"))?;
            let authenticated = session
                .authenticate_publickey(
                    user,
                    PrivateKeyWithHashAlg::new(Arc::new(key_pair), best_hash),
                )
                .await
                .map_err(|e| format!("publickey 鉴权失败: {e}"))?;
            if !authenticated.success() {
                return Err(format!("publickey 鉴权被拒（user={user}）"));
            }
            Ok(())
        }
        None => {
            // 〔SR1a〕agent 这一支**在当前线程上就地跑完**（`block_in_place` ＋ `block_on`），不留在外层 future 里。
            //
            // ⚠ 不是口味：拨号从此跑在要 `tokio::spawn` 的链路任务里（`dial/link.rs`），外层 future 必须 `Send`；
            //   而 russh 的 `authenticate_publickey_with(.., &mut agent)` 经 `Signer::auth_sign(&AgentIdentity, ..)`
            //   那条高阶生命周期，rustc 证不出它的 future 是 `Send`（`implementation of Send is not general enough`，
            //   实打）。`block_on` 不要求 `Send` ⇒ 这一段就地跑完、不跨外层的任何 await。
            //   代价如实写：这一段期间占住一个 worker（鉴权是毫秒级到秒级的一趟），并且**取消不掉**
            //   （链路被关时它照样跑完这一趟鉴权；服务端的 `LoginGraceTime` 兜底）。
            //   本 crate 的运行时是多线程的（`main.rs` 的 `#[tokio::main]`）；`block_in_place` 在单线程运行时上会 panic
            //   ⇒ 走到这一支的判据要用多线程运行时。
            let handle = tokio::runtime::Handle::current();
            tokio::task::block_in_place(|| {
                handle.block_on(agent_auth(session, user, best_hash, agent_sock))
            })
        }
    }
}

/// ssh-agent 那一支的本体（见调用点：它**不在** `Send` 的 future 里跑）。
async fn agent_auth(
    session: &mut client::Handle<Checker>,
    user: &str,
    best_hash: Option<HashAlg>,
    agent_sock: Option<&str>,
) -> Result<(), String> {
    let mut agent = crate::platform::ssh_agent::connect(agent_sock)
        .await
        .map_err(|e| format!("未配置私钥路径(keyPath)，尝试 ssh-agent 失败：{e}"))?;
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

/// 连 ＋ 鉴权。`jump` 在就先连跳板、经它开 direct-tcpip 到目标主地址、在隧道上跑目标的握手
/// （fail-closed：跳板任一步失败就报错，**绝不**回落直连目标）。
pub(crate) async fn establish(
    req: &DialRequest,
    stages: &StageSink,
) -> Result<Linked, (String, Option<String>)> {
    let config = config(req.probe);
    let (mut session, observed, winner, jump) = match &req.jump {
        None => {
            let (s, o, w) = race(
                Arc::clone(&config),
                req.host_key_fingerprint.clone(),
                req.race_order(),
                stages,
            )
            .await?;
            (s, o, w, None)
        }
        Some(hop) => {
            let hop_name = if hop.label.is_empty() {
                format!("{}:{}", hop.host, hop.port)
            } else {
                hop.label.clone()
            };
            stages.emit(Stage::Dialing {
                endpoint: format!("跳板 {hop_name}"),
            });
            let hop_ep = Endpoint {
                host: hop.host.clone(),
                port: hop.port,
            };
            let (mut jump_session, _jo, _jw) = race(
                Arc::clone(&config),
                hop.host_key_fingerprint.clone(),
                vec![hop_ep],
                stages,
            )
            .await
            .map_err(|(e, fp)| (format!("跳板 {hop_name} 连接失败: {e}"), fp))?;
            authenticate(
                &mut jump_session,
                &hop.user,
                hop.key_path.as_deref(),
                req.agent_sock.as_deref(),
            )
            .await
            .map_err(|e| (format!("跳板 {hop_name} 鉴权失败: {e}"), None))?;
            let channel = jump_session
                .channel_open_direct_tcpip(
                    req.host.clone(),
                    u32::from(req.port),
                    "127.0.0.1".to_string(),
                    0,
                )
                .await
                .map_err(|e| {
                    (
                        format!("经跳板开隧道到 {}:{} 失败: {e}", req.host, req.port),
                        None,
                    )
                })?;
            let observed: Arc<Mutex<Option<String>>> = Arc::default();
            let ep_label = format!("{}:{}（经跳板）", req.host, req.port);
            let checker = Checker {
                expected: req.host_key_fingerprint.clone(),
                observed: Arc::clone(&observed),
                stages: stages.clone(),
                endpoint: ep_label,
            };
            let session =
                client::connect_stream(Arc::clone(&config), channel.into_stream(), checker)
                    .await
                    .map_err(|e| {
                        let fp = observed.lock().ok().and_then(|g| g.clone());
                        (format!("目标 SSH 握手失败（经跳板）: {e}"), fp)
                    })?;
            let winner = Endpoint {
                host: req.host.clone(),
                port: req.port,
            };
            (session, observed, winner, Some(jump_session))
        }
    };
    let fingerprint = observed.lock().ok().and_then(|g| g.clone());
    if let Err(e) = authenticate(
        &mut session,
        &req.user,
        req.key_path.as_deref(),
        req.agent_sock.as_deref(),
    )
    .await
    {
        stages.emit(Stage::Auth {
            ok: false,
            detail: Some(e.clone()),
        });
        return Err((e, fingerprint));
    }
    stages.emit(Stage::Auth {
        ok: true,
        detail: None,
    });
    stages.emit(Stage::Established);
    Ok(Linked {
        session,
        fingerprint,
        endpoint: label(&winner),
        _jump: jump,
        budget: super::pool::Budget::new(),
    })
}
