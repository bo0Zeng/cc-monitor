//! 一台机器的配置 → 一份拨号请求，在本机常驻后端里组。
//!
//! 界面交来的是它手里那台机器的配置（设置页表单里可能还没保存的那一份 · 已保存的那一份 · 跳板那一台），形状就是界面
//! `remote-config.ts` 的那一格（camelCase：`host` · `label` · `port` · `user` · `keyPath` · `hostKeyFingerprint` · `addresses` · `jump`）。
//!
//! - 地址：`host` 排首，`addresses` 依次追加（`host` / `host:port` / `[v6]:port` / 裸 v6），按 `(host, port)` 去重保序；
//! - 指纹：表单那一格有 ⇒ 用它；没有 ⇒ 已保存那一份、且同一个 host 的那一格（换了 host 不许继承）；
//! - 跳板：`jump` 指另一台的名字 ⇒ 那一台的配置由界面一并交来；指自己 ⇒ 环（拒）；交不来 ⇒ 拒（fail-closed，不回落直连）；单跳。
//!
//! 只放路径，不放私钥本体。ssh-agent 套接字不交 ⇒ 用本进程自己的（`DialRequest::agent_sock` 缺席那一格）。
//! 竞速顺序：交了 `prefer`（monitor 记的「上次赢的那条」，结构化的 `{host, port}`）且它仍在这台的地址里 ⇒ 排首，其余保序；
//! 所有地址本来就同时起拨（`dial/mod.rs` 头注），顺序只决定同时完成时谁先被看到。
//!
//! 线上交来的每一份拨号都经 [`resolve`] 进来（monitor 起流 / 一次性查询 / 隧道 · 可达表里存的那份 · 传输台的 `dial` · 测试连接 · 端口转发）：
//! 界面只交那台原样的配置，地址解析与组请求只在这里。

use copy_core::copy_text;
use serde_json::{json, Value};

/// 一台机器的配置（界面那一格）里本模块读的几样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Machine {
    pub host: String,
    pub label: String,
    pub port: u16,
    pub user: String,
    pub key_path: Option<String>,
    pub fingerprint: Option<String>,
    pub addresses: Vec<String>,
    pub jump: Option<String>,
}

/// 一台机器的配置里本模块读的那几格（原样，还没判空白）：[`Machine::of`] 的入参。
#[derive(Debug, Clone, Default)]
pub struct Fields<'a> {
    pub host: Option<&'a str>,
    pub user: Option<&'a str>,
    pub label: Option<&'a str>,
    pub port: Option<u16>,
    pub key_path: Option<&'a str>,
    pub fingerprint: Option<&'a str>,
    pub addresses: Vec<String>,
    pub jump: Option<&'a str>,
}

impl Machine {
    /// 读界面那一格。缺 `host` / `user` ⇒ `bad_args`（调用方是我们自己的界面：契约错）。
    pub fn parse(v: &Value) -> Result<Machine, (&'static str, String)> {
        let bad = |d: &str| ("bad_args", crate::malformed(d));
        let port = match v.get("port") {
            None | Some(Value::Null) => None,
            Some(p) => Some(
                p.as_u64()
                    .and_then(|n| u16::try_from(n).ok())
                    .ok_or_else(|| bad("`port` is not a port number"))?,
            ),
        };
        let addresses = v
            .get("addresses")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let s = |k: &str| v.get(k).and_then(Value::as_str);
        Machine::of(Fields {
            host: s("host"),
            user: s("user"),
            label: s("label"),
            port,
            key_path: s("keyPath"),
            fingerprint: s("hostKeyFingerprint"),
            addresses,
            jump: s("jump"),
        })
    }

    /// 已经按类型收下的那几格 ⇒ 一台机器（空白当没给；缺 `host` / `user` ⇒ `bad_args`；口缺 ⇒ 22）。
    /// 线上那一份（[`Machine::parse`]）与按类型收的那一份（后端帧命令的入参）都经这里，判法只有这一处。
    pub fn of(f: Fields<'_>) -> Result<Machine, (&'static str, String)> {
        let bad = |d: &str| ("bad_args", crate::malformed(d));
        let nonempty = |v: Option<&str>| {
            v.map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let host = nonempty(f.host).ok_or_else(|| bad("machine without `host`"))?;
        let user = nonempty(f.user).ok_or_else(|| bad("machine without `user`"))?;
        Ok(Machine {
            label: nonempty(f.label).unwrap_or_default(),
            port: f.port.unwrap_or(22),
            user,
            key_path: nonempty(f.key_path),
            fingerprint: nonempty(f.fingerprint),
            addresses: f.addresses,
            jump: nonempty(f.jump),
            host,
        })
    }

    /// 这台机器的名字（`label`，空 ⇒ `host`）—— 与 monitor 的 origin 同一个串。
    pub fn name(&self) -> &str {
        if self.label.is_empty() {
            &self.host
        } else {
            &self.label
        }
    }

    /// 全部拨号地址：`host` 排首、`addresses` 追加，去重保序；解析不了的那一行丢掉（配置笔误不静默成默认端口）。
    pub fn endpoints(&self) -> Vec<(String, u16)> {
        let mut out: Vec<(String, u16)> = vec![(self.host.clone(), self.port)];
        for line in &self.addresses {
            if let Some(ep) = parse_address_line(line, self.port) {
                if !out.contains(&ep) {
                    out.push(ep);
                }
            }
        }
        out
    }
}

/// 一行地址 → `(host, port)`。四形态：`host` · `host:port` · `[v6]:port` · `[v6]` / 裸 v6（「>1 个冒号」判裸 v6）。
/// 空白 ⇒ `None`；端口非法 ⇒ `None`（拒绝而非静默默认）。
pub fn parse_address_line(line: &str, default_port: u16) -> Option<(String, u16)> {
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
        return Some((host.to_string(), port));
    }
    if s.matches(':').count() > 1 {
        return Some((s.to_string(), default_port));
    }
    match s.split_once(':') {
        Some((h, p)) if !h.is_empty() => Some((h.to_string(), p.parse().ok()?)),
        Some(_) => None,
        None => Some((s.to_string(), default_port)),
    }
}

fn hop(m: &Machine) -> Value {
    json!({
        "host": m.host,
        "port": m.port,
        "user": m.user,
        "key_path": m.key_path,
        "host_key_fingerprint": m.fingerprint,
        "label": m.name(),
    })
}

/// 界面交来的那几样（`machine` 必有；`saved` = 已保存的那一份；`jump` = 跳板那一台）→ 拨号请求（蛇形键，`dial::DialRequest` 的形状）。
/// `use_` 与 `extra` 由调用方补（`forward` 的规格 · 探针的命令与阶段行）。
pub fn request(
    machine: &Machine,
    saved: Option<&Machine>,
    jump: Option<&Machine>,
    prefer: Option<&(String, u16)>,
    use_: &str,
    extra: Value,
) -> Result<Value, (&'static str, String)> {
    // 指纹：表单那一格优先；没有 ⇒ 已保存那一份、且同一个 host（换了 host 不许继承旧指纹）。
    let fingerprint = machine.fingerprint.clone().or_else(|| {
        saved
            .filter(|s| s.host == machine.host)
            .and_then(|s| s.fingerprint.clone())
    });
    let mut order = machine.endpoints();
    // 上次赢的那条排首（它仍在这台的地址里才算；配置改过就失效）。
    if let Some(i) = prefer.and_then(|p| order.iter().position(|e| e == p)) {
        let won = order.remove(i);
        order.insert(0, won);
    }
    let endpoints: Vec<Value> = order
        .into_iter()
        .map(|(host, port)| json!({ "host": host, "port": port }))
        .collect();
    let mut req = json!({
        "host": machine.host,
        "port": machine.port,
        "user": machine.user,
        "key_path": machine.key_path,
        "host_key_fingerprint": fingerprint,
        "endpoints": endpoints,
        "use": use_,
    });
    if let Some(jump_name) = machine.jump.as_deref() {
        if jump_name == machine.name() {
            return Err(("bad_jump", copy_text("beMachine.jump.loop", &[])));
        }
        let j = jump.filter(|j| j.name() == jump_name).ok_or_else(|| {
            (
                "bad_jump",
                copy_text("beMachine.jump.notFound", &[("jumpLabel", jump_name)]),
            )
        })?;
        // v1 单跳：跳板自身的 jump 忽略（防链式递归 / 环）。
        req["jump"] = hop(j);
    }
    if let (Some(obj), Value::Object(more)) = (req.as_object_mut(), extra) {
        obj.extend(more);
    }
    Ok(req)
}

/// 线上交来的一份拨号 ⇒ 拨号请求：`{machine, saved?, jump?, prefer?: {host, port}, use?, …}`，其余格（`command` · `capture` ·
/// `forward` · `stages` · `probe` · `agent_sock`）原样进请求。`use` 缺席 ⇒ `stream`。
pub fn resolve(v: &Value) -> Result<crate::DialRequest, (&'static str, String)> {
    let bad = |d: &str| ("bad_args", crate::malformed(d));
    let obj = v.as_object().ok_or_else(|| bad("dial is not an object"))?;
    let (machine, saved, jump) = from_args(v)?;
    let prefer = match obj.get("prefer") {
        None | Some(Value::Null) => None,
        Some(p) => Some((
            p.get("host")
                .and_then(Value::as_str)
                .ok_or_else(|| bad("`prefer` without `host`"))?
                .to_string(),
            p.get("port")
                .and_then(Value::as_u64)
                .and_then(|n| u16::try_from(n).ok())
                .ok_or_else(|| bad("`prefer` without a port number"))?,
        )),
    };
    let use_ = obj.get("use").and_then(Value::as_str).unwrap_or("stream");
    let mut extra = obj.clone();
    for k in ["machine", "saved", "jump", "prefer", "use"] {
        extra.remove(k);
    }
    dial_request(
        &machine,
        saved.as_ref(),
        jump.as_ref(),
        prefer.as_ref(),
        use_,
        Value::Object(extra),
    )
}

/// 同 [`request`]，组好之后读成拨号请求（线上那一份 [`resolve`] 与按类型收的帧命令入参都经这里）。
pub fn dial_request(
    machine: &Machine,
    saved: Option<&Machine>,
    jump: Option<&Machine>,
    prefer: Option<&(String, u16)>,
    use_: &str,
    extra: Value,
) -> Result<crate::DialRequest, (&'static str, String)> {
    let req = request(machine, saved, jump, prefer, use_, extra)?;
    <crate::DialRequest as serde::Deserialize>::deserialize(&req).map_err(|e| {
        (
            "bad_args",
            crate::malformed(&format!("dial request unreadable: {e}")),
        )
    })
}

/// 帧命令入参里那三格（`machine` · `saved?` · `jump?`）一次读齐。
pub fn from_args(
    args: &Value,
) -> Result<(Machine, Option<Machine>, Option<Machine>), (&'static str, String)> {
    let m = args
        .get("machine")
        .ok_or_else(|| ("bad_args", crate::malformed("missing `machine`")))?;
    let opt = |k: &str| -> Result<Option<Machine>, (&'static str, String)> {
        match args.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => Machine::parse(v).map(Some),
        }
    };
    Ok((Machine::parse(m)?, opt("saved")?, opt("jump")?))
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_machine_tests.rs"]
mod tests;
