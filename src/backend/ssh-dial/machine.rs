//! **一台机器**（[`Host`]）与它的规矩，只此一份：名字 · 地址 · 判准 · 组拨号请求 · 跳板。桌面本机后端与手机核心都链它。
//!
//! - 名字：`label` 去首尾空白后非空取它，否则 `host` 去首尾空白（[`Host::name`]；与 monitor 的 origin 同一个串）。
//! - 地址：`host` 排首，`addresses` 依次追加（`host` / `host:port` / `[v6]:port` / 裸 v6），按 `(host, port)` 去重保序；
//!   解析不了的那一行丢掉（配置笔误不静默成默认端口）。
//! - 判准（[`Host::fault`] · [`check_table`]）：地址与用户非空 · 端口 1–65535 · 地址（含备用地址）与用户不以 `-` 打头 · 表里名字不重。
//!   写表的口与拨号的口同一份。
//! - 跳板：`jump` 指表里另一台的名字；那一台自己还有跳板 ⇒ 再套一层（多跳，同 `ssh -J 外,内`）。
//!   成环 · 表里找不到 ⇒ `bad_jump`（fail-closed，不回落直连）。
//!
//! 只放路径，不放私钥本体。ssh-agent 套接字不交 ⇒ 用本进程自己的（`DialRequest::agent_sock` 缺席那一格）。
//! 竞速顺序：交了 `prefer`（monitor 记的「上次赢的那条」）且它仍在这台的地址里 ⇒ 排首，其余保序；
//! 所有地址本来就同时起拨，顺序只决定同时完成时谁先被看到。
//!
//! 线上交来的那一形（一台原样的配置，键可缺、可为 `null`、可多带）经 [`loose`] 读成 [`Host`]；
//! 线上的一份拨号（`{machine, jump?, prefer?, use?, …}`）经 [`resolve`] 进来。

use crate::Endpoint;
use copy_core::copy_text;
use serde_json::{json, Value};

type Err = (&'static str, String);

/// 一台（盘上那一形，也是线上那一形）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Host {
    /// 这台的名字（origin）；空 ⇒ 用 `host`。
    #[serde(default)]
    pub label: String,
    /// 首选地址。
    #[serde(default)]
    pub host: String,
    /// 端口（缺 ＝ 22）。
    #[serde(default = "port_22")]
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub port: u64,
    #[serde(default)]
    pub user: String,
    /// 钥匙文件路径（这台上的；空 ＝ 照 ssh 的缺省找）。
    #[serde(default)]
    pub key_path: String,
    /// 备用地址（竞发）。
    #[serde(default)]
    pub addresses: Vec<String>,
    /// 跳板：另一台的名字（空 ＝ 直连）。
    #[serde(default)]
    pub jump: String,
    /// 连不连这台。
    #[serde(default = "yes")]
    pub connect: bool,
}

fn port_22() -> u64 {
    22
}
fn yes() -> bool {
    true
}

impl Default for Host {
    fn default() -> Self {
        Host {
            label: String::new(),
            host: String::new(),
            port: port_22(),
            user: String::new(),
            key_path: String::new(),
            addresses: Vec::new(),
            jump: String::new(),
            connect: yes(),
        }
    }
}

/// 机器表那一道没过的是哪一种 · 那一处（形状住两端同一份线上类型；判法 · 说法在这里）。
pub use cmd_wire::machine::{MachineFault, MachineFaultCode};

impl Host {
    /// 这台的名字：`label` 去首尾空白后非空取它，否则 `host`（去首尾空白）。名字的规矩只此一处。
    pub fn name(&self) -> &str {
        match self.label.trim() {
            "" => self.host.trim(),
            l => l,
        }
    }

    /// 钥匙路径（空白 ⇒ 没给，走 ssh-agent）。
    pub fn key(&self) -> Option<&str> {
        Some(self.key_path.trim()).filter(|k| !k.is_empty())
    }

    /// 跳板那一台的名字（空白 ⇒ 直连）。
    pub fn jump_name(&self) -> Option<&str> {
        Some(self.jump.trim()).filter(|j| !j.is_empty())
    }

    /// 这一台自己成不成立（不看别的台）：地址与用户非空 · 端口 1–65535 · 地址（含备用地址）与用户不以 `-` 打头。
    pub fn fault(&self) -> Option<MachineFaultCode> {
        if self.host.trim().is_empty() {
            return Some(MachineFaultCode::NoHost);
        }
        if self.user.trim().is_empty() {
            return Some(MachineFaultCode::NoUser);
        }
        if !(1..=65535).contains(&self.port) {
            return Some(MachineFaultCode::Port);
        }
        let dash = |s: &str| s.trim().starts_with('-');
        if dash(&self.host) || dash(&self.user) || self.addresses.iter().any(|a| dash(a)) {
            return Some(MachineFaultCode::LeadingDash);
        }
        None
    }

    /// 全部拨号地址：`host` 排首、`addresses` 追加，去重保序；解析不了的那一行丢掉。`host` 空 ⇒ 一条都没有。
    pub fn endpoints(&self) -> Vec<Endpoint> {
        let host = self.host.trim();
        if host.is_empty() {
            return Vec::new();
        }
        let port = u16::try_from(self.port).unwrap_or(22);
        let mut out = vec![Endpoint {
            host: host.to_string(),
            port,
        }];
        for line in &self.addresses {
            if let Some(ep) = parse_address_line(line, port) {
                if !out.contains(&ep) {
                    out.push(ep);
                }
            }
        }
        out
    }
}

/// 机器表成不成立：每一台过 [`Host::fault`]，名字（[`Host::name`]，大小写敏感）不重。
/// 不成立 ⇒ 第一处（码 ＋ 那一台）；成立 ⇒ `None`。**纯函数**。
pub fn check_table(hosts: &[Host]) -> Option<(MachineFaultCode, &Host)> {
    let mut seen = std::collections::HashSet::new();
    for h in hosts {
        if let Some(code) = h.fault() {
            return Some((code, h));
        }
        if !seen.insert(h.name()) {
            return Some((MachineFaultCode::NameTaken, h));
        }
    }
    None
}

/// 机器表那一道（[`check_table`]）没过的第一处，说成界面那一句（桌面 `machines-try` · 写口 · 手机编辑页同读这一处）。**纯函数**。
pub fn table_fault(hosts: &[Host]) -> Option<MachineFault> {
    check_table(hosts).map(|(code, h)| MachineFault::new(code, h.name().to_string()))
}

/// 机器表里的一行（线上形）：后端算好的名字 ＋ 那一台原样（盘上那一形）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct MachineRow {
    /// 那台的名字（寻址用的那一个：`label`，空 ⇒ `host`，都去掉首尾空白）。
    pub name: String,
    /// 那一台原样。
    pub host: Host,
}

impl MachineRow {
    /// 一台 ⇒ 一行（名字照 [`Host::name`]）。桌面 `machines-read` 与手机核心同交这一形。
    pub fn of(host: Host) -> Self {
        MachineRow {
            name: host.name().to_string(),
            host,
        }
    }
}

/// 一行读起来就是那一台（名字 · 连不连 · 各格）。
impl std::ops::Deref for MachineRow {
    type Target = Host;
    fn deref(&self) -> &Host {
        &self.host
    }
}

/// 一行地址 → 一个拨号地址。四形态：`host` · `host:port` · `[v6]:port` · `[v6]` / 裸 v6（「>1 个冒号」判裸 v6）。
/// 空白 ⇒ `None`；端口非法 ⇒ `None`（拒绝而非静默默认）。
pub fn parse_address_line(line: &str, default_port: u16) -> Option<Endpoint> {
    let ep = |host: &str, port| Endpoint {
        host: host.to_string(),
        port,
    };
    let s = line.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        if host.is_empty() {
            return None;
        }
        let port = match after {
            "" => default_port,
            p => p.strip_prefix(':')?.parse().ok()?,
        };
        return Some(ep(host, port));
    }
    if s.matches(':').count() > 1 {
        return Some(ep(s, default_port));
    }
    match s.split_once(':') {
        Some((h, p)) if !h.is_empty() => Some(ep(h, p.parse().ok()?)),
        Some(_) => None,
        None => Some(ep(s, default_port)),
    }
}

/// 线上那一形（一台原样的配置：键可缺、可为 `null`，多带的键不读）⇒ 一台。键的类型不对 ⇒ `bad_args`。
/// 不判成不成立：那是组请求时 [`Host::fault`] 的事。
pub fn loose(v: &Value) -> Result<Host, Err> {
    let bad = |d: &str| ("bad_args", crate::malformed(d));
    let obj = v
        .as_object()
        .ok_or_else(|| bad("machine is not an object"))?;
    let text = |k: &str| -> Result<String, Err> {
        match obj.get(k) {
            None | Some(Value::Null) => Ok(String::new()),
            Some(Value::String(s)) => Ok(s.clone()),
            Some(_) => Err(bad(&format!("`{k}` is not a string"))),
        }
    };
    let port = match obj.get("port") {
        None | Some(Value::Null) => port_22(),
        Some(p) => p
            .as_u64()
            .ok_or_else(|| bad("`port` is not a port number"))?,
    };
    let addresses = match obj.get("addresses") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(_) => return Err(bad("`addresses` is not a list")),
    };
    Ok(Host {
        label: text("label")?,
        host: text("host")?,
        port,
        user: text("user")?,
        key_path: text("keyPath")?,
        addresses,
        jump: text("jump")?,
        connect: obj.get("connect").and_then(Value::as_bool).unwrap_or(true),
    })
}

/// 按类型收的入参里那一格经 [`loose`] 读（`#[serde(deserialize_with = "ssh_dial::machine::loose_field")]`）。
pub fn loose_field<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Host, D::Error> {
    let v = <Value as serde::Deserialize>::deserialize(d)?;
    loose(&v).map_err(|(_, m)| serde::de::Error::custom(m))
}

/// 拨不了这一台（没地址 · 没用户 · 端口不在 1–65535）⇒ `bad_args`（调用方是我们自己：契约错）。
/// 以 `-` 打头那一条不在这里拒：russh 拨号不经命令行；写表那一处照 [`Host::fault`] 拒，开终端那一行另按它的裸词白名单拒、说清是哪一格。
fn usable(h: &Host) -> Result<&Host, Err> {
    match h.fault() {
        None | Some(MachineFaultCode::LeadingDash) => Ok(h),
        Some(code) => Err((
            "bad_args",
            crate::malformed(&format!("machine `{}`: {code:?}", h.name())),
        )),
    }
}

/// 一跳（蛇形键）：`outer` = 它自己还要经的那一台。
fn hop(h: &Host, outer: Option<Value>) -> Value {
    let mut v = json!({
        "host": h.host.trim(),
        "port": h.port,
        "user": h.user.trim(),
        "key_path": h.key(),
        "label": h.name(),
    });
    if let Some(o) = outer {
        v["jump"] = o;
    }
    v
}

/// 表里叫 `name` 的那一台 ⇒ 拨号请求（蛇形键，`DialRequest` 的形状）。跳板一路照表找下去（多跳）：
/// 成环 ⇒ `bad_jump`（`beMachine.jump.loop`）；找不到 ⇒ `bad_jump`（`beMachine.jump.notFound`）；
/// 表里没有这一台 · 路上哪一台不成立 ⇒ `bad_args`。`use_` 与 `extra` 由调用方补（`forward` 的规格 · 探针的命令与阶段行）。
pub fn request(
    table: &[Host],
    name: &str,
    prefer: Option<&Endpoint>,
    use_: &str,
    extra: Value,
) -> Result<Value, Err> {
    let find = |n: &str| table.iter().find(|h| h.name() == n);
    let machine = find(name).ok_or_else(|| {
        (
            "bad_args",
            crate::malformed(&format!("no machine named `{name}`")),
        )
    })?;
    usable(machine)?;
    // 跳板一路：从这一台往外走，走到直连的那一台为止。
    let mut chain: Vec<&Host> = Vec::new();
    let mut seen = vec![machine.name()];
    let mut at = machine;
    while let Some(j) = at.jump_name() {
        if seen.contains(&j) {
            return Err(("bad_jump", copy_text("beMachine.jump.loop", &[])));
        }
        let next = find(j).ok_or_else(|| {
            (
                "bad_jump",
                copy_text("beMachine.jump.notFound", &[("jumpLabel", j)]),
            )
        })?;
        usable(next)?;
        seen.push(j);
        chain.push(next);
        at = next;
    }
    let jump = chain
        .iter()
        .rev()
        .fold(None, |outer, h| Some(hop(h, outer)));

    let mut order = machine.endpoints();
    // 上次赢的那条排首（它仍在这台的地址里才算；配置改过就失效）。
    if let Some(i) = prefer.and_then(|p| order.iter().position(|e| e == p)) {
        let won = order.remove(i);
        order.insert(0, won);
    }
    let mut req = hop(machine, jump);
    if let Some(o) = req.as_object_mut() {
        o.remove("label");
    }
    req["endpoints"] = json!(order);
    req["use"] = json!(use_);
    if let (Some(obj), Value::Object(more)) = (req.as_object_mut(), extra) {
        obj.extend(more);
    }
    Ok(req)
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_machine_tests.rs"]
mod tests;
