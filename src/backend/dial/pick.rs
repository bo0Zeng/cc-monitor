//! 帧命令入参里「拨哪台」那几格的类型：那台的配置（[`MachineConfig`]）· 已保存的那一份 · 跳板那一台 · 上次赢的那条（[`Prefer`]）。
//!
//! 组请求只在 `ssh_dial::machine`（地址排序 · 指纹继承 · 跳板查无 / 环）；这里只把按类型收下的那几格交过去。
//! 内建档那几条（`link-open` · `transfer-*` · 测试连接）的拨号还是线上原样那一份，经 `ssh_dial::machine::resolve` 读。

use serde_json::{json, Value};

use super::machine::{Fields, Machine};

type CmdErr = (&'static str, String);

/// 一台机器的配置（配置文件里那台原样的一条；界面 `remote-config.ts` 那一格，camelCase）里拨号读的那几格。
///
/// **这一格不拒多出来的键**：它是配置文件里的一条，界面那份多带 `resumeCommand` · `connect` · `hostKeyPinnedAt`，壳那份不带，
/// 拨号只读下面这几格。外面那一层入参照拒多出来的键。空白当没给；缺 `host` / `user` ⇒ `bad_args`。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct MachineConfig {
    /// 首选地址。
    pub(crate) host: String,
    /// 登录用户。
    pub(crate) user: String,
    /// 这台的名字（空 ⇒ 用 `host`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) label: Option<String>,
    /// 口（缺 ⇒ 22）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) port: Option<u16>,
    /// 私钥路径（缺 / 空 ⇒ 走 ssh-agent）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) key_path: Option<String>,
    /// 记下的主机指纹（`SHA256:…`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) host_key_fingerprint: Option<String>,
    /// 其余地址（`host` / `host:port` / `[v6]:port` / 裸 v6）。
    #[serde(default)]
    pub(crate) addresses: Vec<String>,
    /// 跳板那一台的名字（空 ⇒ 直连）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) jump: Option<String>,
}

impl MachineConfig {
    /// 判过空白、补过缺省的那一台（判法在 `ssh_dial::machine::Machine::of`，只此一处）。
    pub(crate) fn machine(&self) -> Result<Machine, CmdErr> {
        Machine::of(Fields {
            host: Some(&self.host),
            user: Some(&self.user),
            label: self.label.as_deref(),
            port: self.port,
            key_path: self.key_path.as_deref(),
            fingerprint: self.host_key_fingerprint.as_deref(),
            addresses: self.addresses.clone(),
            jump: self.jump.as_deref(),
        })
    }
}

/// 上次赢的那条地址（monitor 记的）：仍在这台的地址里 ⇒ 排首。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct Prefer {
    pub(crate) host: String,
    pub(crate) port: u16,
}

/// 「拨哪台」那几格（入参里各自平铺：`machine` 必有，`saved` · `jump` · `prefer` 可缺）。
pub(crate) struct Pick<'a> {
    pub(crate) machine: &'a MachineConfig,
    pub(crate) saved: Option<&'a MachineConfig>,
    pub(crate) jump: Option<&'a MachineConfig>,
    pub(crate) prefer: Option<&'a Prefer>,
}

impl Pick<'_> {
    /// 三台都读成 [`Machine`]（哪一台坏了 ⇒ `bad_args`）。
    pub(crate) fn machines(&self) -> Result<(Machine, Option<Machine>, Option<Machine>), CmdErr> {
        let opt = |m: Option<&MachineConfig>| m.map(MachineConfig::machine).transpose();
        Ok((self.machine.machine()?, opt(self.saved)?, opt(self.jump)?))
    }

    /// 组好的拨号请求（`use_` · `extra` 由调用方补，同 `ssh_dial::machine::request`）。
    pub(crate) fn request(&self, use_: &str, extra: Value) -> Result<super::DialRequest, CmdErr> {
        let (m, saved, jump) = self.machines()?;
        let prefer = self.prefer.map(|p| (p.host.clone(), p.port));
        super::machine::dial_request(
            &m,
            saved.as_ref(),
            jump.as_ref(),
            prefer.as_ref(),
            use_,
            extra,
        )
    }

    /// 线上那一份拨号（`{machine, saved, jump, prefer}`）：存进可达表 · 交给还收原样拨号的那几处（它们经 `resolve` 读）。
    pub(crate) fn dial_value(&self) -> Value {
        json!({
            "machine": self.machine,
            "saved": self.saved,
            "jump": self.jump,
            "prefer": self.prefer,
        })
    }
}

/// 一份拨号（线上形）：拨哪台那几格 ＋ 用法 ＋ 界面进程此刻的 ssh-agent 套接字。可达表存它（`remote-reach` · `assets-sync`），部署计划拿它去问那台（`deploy-plan`）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct MachineDial {
    pub(crate) machine: MachineConfig,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) saved: Option<MachineConfig>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) jump: Option<MachineConfig>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) prefer: Option<Prefer>,
    /// 用法（存着不读：问那台的每一趟按自己的用法改，`capture` · `forward`）。
    #[serde(default, rename = "use")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) use_: Option<String>,
    /// ssh-agent 套接字的路径（界面进程此刻的 `SSH_AUTH_SOCK`；缺 ⇒ 用本进程自己的）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) agent_sock: Option<String>,
}

impl MachineDial {
    /// 交给还收原样拨号的那几处的那一份（之后经 `ssh_dial::machine::resolve` 读；先在这里读一遍，坏了当场拒）。
    pub(crate) fn stored(&self) -> Result<Value, (&'static str, String)> {
        let pick = Pick {
            machine: &self.machine,
            saved: self.saved.as_ref(),
            jump: self.jump.as_ref(),
            prefer: self.prefer.as_ref(),
        };
        pick.machines()?;
        let mut v = pick.dial_value();
        v["use"] = json!(self.use_);
        v["agent_sock"] = json!(self.agent_sock);
        Ok(v)
    }
}
