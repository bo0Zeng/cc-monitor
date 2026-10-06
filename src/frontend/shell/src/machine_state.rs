//! 每台机器的状态成品：`{state, reason, stage, version, versionRelation, os, fixes}`。
//!
//! 界面按码取那一句与修法，自己不判（「那台连不上 / 要更新 / 版本较新 / 停用了」只在这里定）。
//! 写点：远端那条流的每一轮（`stream_source::run`：起拨 · 握手成了 · 这一轮没成）、拨号那一处
//! （`dial_host::open`：后端 ack 带回的原因码）、部署计划（那台的系统）；读点：`backend_status` 的 `machine` 一格。
//! 「连接这台」关着的那台由读点按机器表说「停用」（不进这张表）。

use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// 那台此刻是什么样（闭集）：`up` 连着（版本对得上或不可比）· `connecting` 正在连（`stage` 说到哪一步）·
/// `down` 这一轮没连上（`reason` 说为什么）· `host_key_changed` 主机指纹与记下的不一样 · `needs_update` 连着但那台旧 ·
/// `newer` 连着但那台比这一版新 · `disabled` 「连接这台」关着 · `unsupported` 那台做不了 · `unknown` 还没有任何一轮的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub enum MachineStateKind {
    Up,
    Connecting,
    Down,
    HostKeyChanged,
    NeedsUpdate,
    Newer,
    Disabled,
    Unsupported,
    Unknown,
}

/// 那台与这一版的版本关系（连上时判一次）；`incomparable` ＝ 手上没带后端字节（开发树），不判旧、不自动换装。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub enum VersionRelation {
    Same,
    Older,
    Newer,
    Incomparable,
}

/// 那一行上给的修法（闭集；界面按它摆按钮，按钮名在文案表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub enum MachineFix {
    Retry,
    ConnSettings,
    PushKey,
    CompareFingerprint,
    Update,
    Connect,
}

/// 一台的成品（`backend_status` 的 `machine` 一格）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct MachineState {
    /// 那台此刻是什么样。
    pub state: MachineStateKind,
    /// 没连上 / 做不了的原因码（`down`：拨号那一层的闭集码；`unsupported`：`not_unix` · `no_forwarding`；本机没连上：`local`）。
    pub reason: Option<String>,
    /// 正在连时到哪一步：`deploy`（部署预检 / 装）· `attach`（接那台的常驻后端）。
    pub stage: Option<String>,
    /// 那台 cc-monitor 的产品版本号（发版那个号，`CARGO_PKG_VERSION`）：与这一版同一份字节时就是这一版的号；
    /// 不同的那台报不出它的号（握手只带构建标识）⇒ 空；开发构建 ⇒ 空（关系是 `incomparable`）。构建标识只进日志与诊断信息。
    pub version: Option<String>,
    /// 与这一版的版本关系（握手过才有）。
    pub version_relation: Option<VersionRelation>,
    /// 那台的系统（`Linux` · `macOS` · `Windows`；部署计划问到过才有）。
    pub os: Option<String>,
    /// 那一行上给的修法（按 `state` 与 `reason` 定）。
    pub fixes: Vec<MachineFix>,
}

/// 「做不了」的两个原因码。
pub const NOT_UNIX: &str = "not_unix";
pub const NO_FORWARDING: &str = "no_forwarding";

#[derive(Debug, Clone, Default)]
struct Entry {
    state: Option<MachineStateKind>,
    reason: Option<String>,
    stage: Option<String>,
    version: Option<String>,
    relation: Option<VersionRelation>,
    os: Option<String>,
    /// 拨号那一层最近一次没拨成的原因码（这一轮收尾时用）。
    dial_why: Option<String>,
}

static TABLE: Mutex<BTreeMap<String, Entry>> = Mutex::new(BTreeMap::new());

fn with<R>(origin: &str, f: impl FnOnce(&mut Entry) -> R) -> R {
    let mut g = TABLE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.entry(origin.to_string()).or_default())
}

/// 这一轮开始拨 / 到了哪一步。
pub(crate) fn connecting(origin: &str, stage: &str) {
    with(origin, |e| {
        e.state = Some(MachineStateKind::Connecting);
        e.reason = None;
        e.stage = Some(stage.to_string());
    });
}

/// 这一版的产品版本号（全产品一个源：monitor 那个包的版本，与安装包 · 界面同一个号）。
pub(crate) const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 与这一版同一份字节 ⇒ 这一版的号；别的报不出 ⇒ `None`。
fn product_version(relation: VersionRelation) -> Option<String> {
    (relation == VersionRelation::Same).then(|| PRODUCT_VERSION.to_string())
}

/// 握手成了：那台报的构建标识（只进日志）＋ 与这一版的关系。
pub(crate) fn up(origin: &str, build: &str, relation: VersionRelation) {
    tracing::info!("[machine] {origin} 连上：构建 {build} · 与这一版 {relation:?}");
    with(origin, |e| {
        e.state = Some(match relation {
            VersionRelation::Older => MachineStateKind::NeedsUpdate,
            VersionRelation::Newer => MachineStateKind::Newer,
            VersionRelation::Same | VersionRelation::Incomparable => MachineStateKind::Up,
        });
        e.reason = None;
        e.stage = None;
        e.version = product_version(relation);
        e.relation = Some(relation);
        e.dial_why = None;
    });
}

/// 拨号那一层没拨成（后端 ack 带回的原因码；老后端没给 ⇒ 不记）。
pub(crate) fn dial_failed(origin: &str, why: Option<&str>) {
    if let Some(w) = why {
        with(origin, |e| e.dial_why = Some(w.to_string()));
    }
}

/// 这一轮没连上：原因取拨号那一层记下的码（没有 ⇒ `other`）；指纹不对单列一态。
pub(crate) fn down(origin: &str) {
    with(origin, |e| {
        let why = e.dial_why.take().unwrap_or_else(|| "other".to_string());
        e.state = Some(if why == "host_key" {
            MachineStateKind::HostKeyChanged
        } else {
            MachineStateKind::Down
        });
        e.reason = Some(why);
        e.stage = None;
    });
}

/// 那台做不了（这条流就此收工）。
pub(crate) fn unsupported(origin: &str, why: &str) {
    with(origin, |e| {
        e.state = Some(MachineStateKind::Unsupported);
        e.reason = Some(why.to_string());
        e.stage = None;
    });
}

/// 部署计划问到的那台的系统（给人看的名字，`deploy_contract::Os::label`）。
pub(crate) fn note_os(origin: &str, name: &str) {
    with(origin, |e| e.os = Some(name.to_string()));
}

/// 机器从表里拿掉 ⇒ 那一格作废。
pub(crate) fn forget(origin: &str) {
    TABLE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(origin);
}

/// 每一态给哪几颗修法（只在这里定）。
pub(crate) fn fixes_of(state: MachineStateKind, reason: Option<&str>) -> Vec<MachineFix> {
    use MachineFix as F;
    match state {
        MachineStateKind::Down => match reason {
            Some("auth") | Some("key_unreadable") => vec![F::PushKey, F::ConnSettings],
            Some("resolve") | Some("jump") => vec![F::ConnSettings],
            Some(LOCAL_DOWN) => vec![F::Retry],
            _ => vec![F::Retry, F::ConnSettings],
        },
        MachineStateKind::HostKeyChanged => vec![F::CompareFingerprint],
        MachineStateKind::NeedsUpdate => vec![F::Update],
        MachineStateKind::Disabled => vec![F::Connect],
        _ => Vec::new(),
    }
}

/// 读点：一台的成品。`enabled = false` ⇒ 停用（不看表）；表里没有 ⇒ `unknown`。
pub(crate) fn product(origin: &str, enabled: bool) -> MachineState {
    let e = TABLE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(origin)
        .cloned()
        .unwrap_or_default();
    let state = if enabled {
        e.state.unwrap_or(MachineStateKind::Unknown)
    } else {
        MachineStateKind::Disabled
    };
    let reason = if enabled { e.reason } else { None };
    MachineState {
        state,
        fixes: fixes_of(state, reason.as_deref()),
        reason,
        stage: if enabled { e.stage } else { None },
        version: e.version,
        version_relation: e.relation,
        os: e.os,
    }
}

/// 本机那一台的成品：版本就是手上这一版（本机后端换装只认同一版，`local_backend_host::hello_verdict`；没带后端字节 ⇒ 开发构建、不可比）；
/// 通道在 ⇒ 连着，不在 ⇒ 没连上（原因码 `local`，修法只给［重试］）。
pub(crate) fn local_product(channel: bool, mine: Option<&str>) -> MachineState {
    let state = if channel {
        MachineStateKind::Up
    } else {
        MachineStateKind::Down
    };
    let reason = (!channel).then(|| LOCAL_DOWN.to_string());
    MachineState {
        state,
        fixes: fixes_of(state, reason.as_deref()),
        reason,
        stage: None,
        version: mine.map(|_| PRODUCT_VERSION.to_string()),
        version_relation: Some(if mine.is_some() {
            VersionRelation::Same
        } else {
            VersionRelation::Incomparable
        }),
        os: None,
    }
}

/// 本机那条连不上的原因码。
pub const LOCAL_DOWN: &str = "local";

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/machine_state_tests.rs"]
mod tests;
