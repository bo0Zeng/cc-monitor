//! SSH-remote 数据源骨架（issue #15 Phase 0 / S3）。
//!
//! 本模块**当前是 dead code**：在 lib.rs 里 `mod ssh_source;` 声明，但**不**从
//! `setup()` 调用。S3 的唯一目标是证明 russh 能在 windows-msvc 下编译 + LTO 链接，
//! 并让一个 typed 的 `connect_and_exec` 骨架通过类型检查。运行时连通性留到 S5/S8。
//! S5 会把 `connect_and_exec` 接进 `setup()`（替代/补充 jsonl-watcher 数据源）。
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
use std::sync::Arc;
use std::time::Duration;

use russh::client;
use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKey};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::event_replay::EventReplay;
use crate::session_map::SessionChange;
use crate::watcher::JsonlLine;

/// 远端 daemon 的连接配置。S5 会从 monitor 的 config 文件反序列化出来。
#[derive(Debug, Clone)]
pub struct RemoteConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// 私钥文件路径（OpenSSH 格式）。None = 暂不支持（S3 骨架只走 publickey auth）。
    pub key_path: Option<String>,
    /// 远端要 exec 的 daemon 命令（含参数前缀由 S5 决定）。
    pub daemon_path: String,
    /// 期望的 server host key 指纹（`SHA256:...` 形式）。
    /// Some = 严格校验（TOFU 之后固化）；None = 首次连接 TOFU 接受并 LOUD warn。
    pub host_key_fingerprint: Option<String>,
}

/// russh client handler：负责 host key 校验（check_server_key）。
///
/// 持有期望指纹，`check_server_key` 据此决定接受 / 拒绝（见该方法注释）。
struct ClientHandler {
    expected_fingerprint: Option<String>,
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
        match &self.expected_fingerprint {
            Some(expected) => {
                if &actual == expected {
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

/// 连接远端、publickey 鉴权、开 session channel、exec `cfg.daemon_path`，
/// 返回 channel 的双向流（`AsyncRead + AsyncWrite`）——读端即 daemon 的 stdout 数据。
///
/// **S3 状态**：plausible + COMPILING 实现，尚未 runtime-tested（S5/S8 才接真连接）。
/// 错误统一 map 成 `String`（本 crate 未直接依赖 anyhow，不为骨架引入新依赖）。
pub async fn connect_and_exec(
    cfg: &RemoteConfig,
) -> Result<russh::ChannelStream<client::Msg>, String> {
    // publickey 是 S3 骨架唯一支持的鉴权方式；缺 key_path 直接拒。
    let key_path = cfg
        .key_path
        .as_ref()
        .ok_or_else(|| "remote config 缺 key_path（S3 骨架仅支持 publickey 鉴权）".to_string())?;

    let key_pair =
        load_secret_key(key_path, None).map_err(|e| format!("加载私钥 {key_path} 失败: {e}"))?;

    let config = Arc::new(client::Config {
        // 与 jsonl-watcher 不同，daemon 是长连接；给一个保守的 inactivity 上限，
        // 具体值 S5 再调（这里只要类型正确）。
        inactivity_timeout: Some(Duration::from_secs(3600)),
        ..Default::default()
    });

    let handler = ClientHandler {
        expected_fingerprint: cfg.host_key_fingerprint.clone(),
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

    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("打开 session channel 失败: {e}"))?;

    // want_reply = true：等远端确认 exec 成功再继续。
    channel
        .exec(true, cfg.daemon_path.as_bytes())
        .await
        .map_err(|e| format!("exec {} 失败: {e}", cfg.daemon_path))?;

    // into_stream 把 channel 变成 AsyncRead+AsyncWrite；读端就是 daemon stdout 流。
    Ok(channel.into_stream())
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
/// - `hello` → log（证明 daemon runtime 起来了）+ 置 `ready`（SSH 版 `initial_scan_done`，
///   解锁 frontend-ready 的 replay 等待）。
/// - `line` → 组 [`JsonlLine`] 走 **与本地 watcher 完全相同的出口**：
///   `crate::batch_to_payloads(...)` → `replay.on_line_batch(&app, ...)`。Phase-0 用最简正确
///   做法：每帧一条 batch（前端按 seq 自动排序，单条 emit 语义与本地小 batch 一致）。
/// - `session_added` / `session_removed` → 走**既有** `session_changes` 通道
///   （`SessionChange{added,removed}`），由 lib.rs 那个 session-changes-emitter 线程消费，
///   透传 session-ended 给前端（remote 模式下 rescan / SidHwndCache 部分对远端 no-op，
///   但 session-ended emit 路径复用）。
/// - 未知 kind / garbage → `tracing::warn!` 跳过，绝不中断流。
///
/// stdout EOF / 读错误 → 返回 `Err`，调用方（S8/S9）据此大声报"connection dropped"，
/// 不静默冻结。
pub async fn run(
    cfg: RemoteConfig,
    replay: Arc<EventReplay>,
    app: tauri::AppHandle,
    session_changes: std::sync::mpsc::Sender<SessionChange>,
    ready: Arc<AtomicBool>,
) -> Result<(), String> {
    tracing::info!(
        "ssh_source connecting to {}@{}:{} (daemon={})",
        cfg.user,
        cfg.host,
        cfg.port,
        cfg.daemon_path
    );

    let stream = connect_and_exec(&cfg).await?;
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
                // SSH 版 initial_scan_done：daemon 已就绪 → 解锁 frontend-ready 的 replay 等待。
                ready.store(true, Ordering::Release);
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
                let payloads = crate::batch_to_payloads(lines);
                replay.on_line_batch(&app, payloads);
            }
            Some(InboundFrame::SessionAdded { sid }) => {
                if let Err(e) = session_changes.send(SessionChange {
                    added: vec![sid],
                    removed: vec![],
                }) {
                    tracing::warn!("ssh_source session_added send failed: {e}");
                }
            }
            Some(InboundFrame::SessionRemoved { sid }) => {
                if let Err(e) = session_changes.send(SessionChange {
                    added: vec![],
                    removed: vec![sid],
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
