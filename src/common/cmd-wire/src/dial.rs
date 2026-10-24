//! **拨号的线上形状**：拨号核心（`ssh-dial`）写、读它的那一侧（壳的 `ssh_link` · 手机 Kotlin）读的那几样，两头同一份。
//!
//! 一条链路上 ack 之前只有一种行 `{"stage":{…}}`（[`StageLine`]：拨号阶段 [`Stage`] 或「过了一步」），然后恰好一行 ack（[`DialAck`]），
//! 之后按用法（`capture` 那一形是一行 [`Captured`]）。纯数据：怎么拨、详情怎么写住拨号核心。

use serde::{Deserialize, Serialize};

use crate::machine::Why;

/// ack 里的协议版本：v2 = 竞速 · 跳板 · agent · 几种用法 · 阶段行。多一种用法不 bump 它（新界面凭 `uses` 认出老后端），
/// 版本号只在同一种用法的字节形状变了时才动。
pub const ACK_V: u32 = 2;

/// 拨号代理认得的用法 —— ack 的 `uses` 原样回这张表，读的那一侧据它判「代理够不够新」。
pub const USES: &[&str] = &["stream", "capture", "forward", "shell", "hold"];

/// capture 那一形的收工记号：不认一次性子命令的老后端会把它当未知旗标、直接进流模式、第一行发 hello ——
/// 收全的一侧见到它就提前收（不让它装 hook、不挂住），读的一侧当「太旧、要更新」。后端问远端 · 桌面与手机的重连循环同读这一份。
pub const OLD_BACKEND_MARKER: &str = "\"kind\":\"hello\"";

/// 一个拨号地址。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

/// ack 之前的一行（`{"stage":…}`）。阶段取 russh 能干净观测的粒度 —— 不含 KEX（`HostKey` 触发即隐含 TCP ＋ KEX 已过）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Stage {
    /// 某地址开始拨号（TCP ＋ 握手）。
    Dialing { endpoint: String },
    /// 某地址握手到 host key 校验（带指纹）。
    HostKey {
        endpoint: String,
        fingerprint: String,
    },
    /// 某地址连接失败（reason = 粗分类 ＋ 原始错误）。
    Failed { endpoint: String, reason: String },
    /// 竞速胜出地址（其余在飞的已丢弃）。
    Won { endpoint: String },
    /// 鉴权结果。没登上时 `detail` 是那一句（已带原因词；原话随 ack 走）。
    Auth { ok: bool, detail: Option<String> },
    /// 连接就绪（握手 ＋ 鉴权全过）。
    Established,
}

/// ack 之前那一行 `{"stage":…}` 的值：拨号又过了一步，或者一条拨号阶段（[`Stage`]）。同一种行、同一个 `kind` 标签。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StageLine {
    /// 拨号又过了一步（`tcp` · `banner` · `hostkey` · `auth` · `tunnel` · `channel`，经跳板时每一台各走一遍）。
    /// 当场就写，不攒；读的那一侧只拿它重新计时，不按名字分支。
    Step { step: String },
    /// 一条拨号阶段（`kind` 是它自己那几个）。
    #[serde(untagged)]
    Stage(Stage),
}

/// 拨号代理回的那一行 ack。读的那一侧对老代理宽容：除 `ok` 之外的格缺席都按缺省读。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialAck {
    pub ok: bool,
    /// `ok=false` 时的那一句（只带原因词，不带下层原话）。
    #[serde(default)]
    pub error: Option<String>,
    /// `ok=false` 时的复制详情（时刻 · 机器 · 命令 · 下层原话）。成功 ⇒ 不出这一格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 实际观察到的 host key 指纹（展示用；失败时也尽量带上）。
    #[serde(default)]
    pub fingerprint: Option<String>,
    /// 竞速胜出的地址（`host:port`，给人看）。
    #[serde(default)]
    pub endpoint: Option<String>,
    /// 同一条胜者，结构化 —— 记 last-good、下一趟当 `prefer` 交回来用它（地址不在读的那一侧解析）。失败 ⇒ `None`。
    #[serde(default)]
    pub winner: Option<Endpoint>,
    /// 没拨成时的原因码；拨成了 / 说不清 ⇒ `None`。
    #[serde(default)]
    pub reason: Option<Why>,
    /// 目标那一台握手时给的 host key 类型。没握到那一步 ⇒ 不出这一格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_type: Option<String>,
    /// 「钥匙变了」时库里同类型那一把的指纹（没有 ⇒ 空串）。没因钥匙被拒 ⇒ 不出这一格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_fingerprint: Option<String>,
    /// 协议版本（[`ACK_V`]；v1 代理没有这一格 ⇒ 0）。
    #[serde(default)]
    pub v: u32,
    /// 这个代理认得的用法（[`USES`]）。
    #[serde(default)]
    pub uses: Vec<String>,
}

impl DialAck {
    /// 开成了：看到的指纹 · 胜出的地址（给人看的一串 ＋ 结构化的一份）。
    pub fn opened(fingerprint: Option<String>, endpoint: String, winner: Endpoint) -> Self {
        DialAck {
            ok: true,
            error: None,
            detail: None,
            fingerprint,
            endpoint: Some(endpoint),
            winner: Some(winner),
            reason: None,
            key_type: None,
            expected_fingerprint: None,
            v: ACK_V,
            uses: uses(),
        }
    }

    /// 没开成的那一份写成线上那一行。
    pub fn of_fail(f: DialFail) -> Self {
        DialAck {
            ok: false,
            error: Some(f.error),
            detail: f.detail,
            fingerprint: f.fingerprint,
            endpoint: None,
            winner: None,
            reason: f.reason,
            key_type: f.key_type,
            expected_fingerprint: f.expected_fingerprint,
            v: ACK_V,
            uses: uses(),
        }
    }
}

fn uses() -> Vec<String> {
    USES.iter().map(|u| (*u).to_string()).collect()
}

/// 没开成：线上就是一行 `ok:false` 的 ack（[`DialAck::of_fail`]），进程内的调用方直接拿这一份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialFail {
    /// 那一句（只带原因词，不带下层原话）。
    pub error: String,
    /// 复制详情（时刻 · 机器 · 命令 · 下层原话）。
    pub detail: Option<String>,
    /// 握手时看到的 host key 指纹（失败时也尽量带上）。
    pub fingerprint: Option<String>,
    /// 原因码；说不清 ⇒ `None`。
    pub reason: Option<Why>,
    /// 目标那一台握手时给的 host key 类型（没握到那一步 ⇒ `None`）。
    pub key_type: Option<String>,
    /// 「钥匙变了」时库里同类型那一把的指纹（没有 ⇒ 空串；没因钥匙被拒 ⇒ `None`）。
    pub expected_fingerprint: Option<String>,
}

/// `use: capture` 的结果行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Captured {
    pub stdout: String,
    pub stderr: String,
    /// `None` = 远端没送 exit-status（连接被掐 / 服务端不守规矩）。**不许把 `None` 当成 0**。
    pub exit_status: Option<u32>,
}
