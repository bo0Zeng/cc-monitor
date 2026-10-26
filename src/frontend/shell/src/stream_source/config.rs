//! 远端连接配置与 last-good（上次连上的那条地址）。

use serde::{Deserialize, Serialize};

/// 远端后端的连接配置。S5 会从 monitor 的 config 文件反序列化出来；
/// Tier 1（issue #15）的「测试连接」命令直接收前端传来的同形对象（camelCase）。
///
/// **serde camelCase 必须与前端 RemoteHostConfig / lib.rs::load_remote_configs 严格一致**：
/// host / port / user / keyPath / label（多机 #30，
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
    /// Batch14-F45：备用地址（happy-eyeballs 竞发）。每项 `host` / `host:port` /
    /// `[IPv6]:port` / 裸 IPv6。空 = 仅用 `host`（老配置零迁移）。首选地址仍是 `host`
    /// 字段（见 [`RemoteConfig::endpoints`]，host 排首）。
    #[serde(default)]
    pub addresses: Vec<String>,
    /// Batch14-F56：跳板 ProxyJump——指向另一台已配置主机的 `origin_label`。Some = 经该跳板
    /// 机 `channel_open_direct_tcpip` 隧道连本机（fail-closed，跳板缺失/断开即报错不直连）；
    /// None/空 = 直连。v1 单跳（跳板自身的 jump 忽略）+ 防自引用环。
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub jump: Option<String>,
}

// 〔「一个判定一个家」〕地址解析与组拨号请求只在本机常驻后端 `src/backend/ssh-dial/machine.rs`（起流时把这台原样的配置交过去）。

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

    // 「所有连接目标」（`host` 排首 · `addresses` 追加 · 去重保序）那一格搬进后端 `ssh-dial/machine.rs::Machine::endpoints`。
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

// 「上次连上的那条地址」由本机常驻后端自己记（`dial::last_good`，开链路那一趟读 ack 记下、组请求时排首）。

// 开终端那一行在本机后端（`terminal-ssh`），只交名字。

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/tier1_tests.rs"]
mod tier1_tests;
