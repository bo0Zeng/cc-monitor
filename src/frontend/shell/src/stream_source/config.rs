//! 远端连接配置与 last-good（上次连上的那条地址）。

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

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

/// Batch14-F45：单个连接目标（host + port）。今天只剩一个用处：后端 ack 里结构化的胜者（`winner`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

// 〔「一个判定一个家」〕地址解析与组拨号请求只在本机常驻后端 `src/backend/dial/machine.rs`（起流时把这台原样的配置交过去）。

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

    // 「所有连接目标」（`host` 排首 · `addresses` 追加 · 去重保序）那一格搬进后端 `dial/machine.rs::Machine::endpoints`。
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
/// 记的是后端 ack 里结构化的 `winner`，连同**记下那一刻这台的地址配置**（`host` · `port` · `addresses`）——
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

// 开终端那一行在本机后端（`terminal-ssh`），地址由 `dial/machine.rs::resolve` 按交过去的 `prefer`（[`last_good_for`]）排首。

// 竞发拨号顺序（last-good 排首、其余保序）那个纯函数 `winner_order` 搬进后端 `dial/machine.rs::request`（`prefer`）。
