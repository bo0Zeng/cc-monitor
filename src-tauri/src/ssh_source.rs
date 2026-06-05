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

#![allow(dead_code)] // S5 才把本模块接进 setup()；在那之前整模块故意不被调用。

use std::sync::Arc;
use std::time::Duration;

use russh::client;
use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKey};

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
