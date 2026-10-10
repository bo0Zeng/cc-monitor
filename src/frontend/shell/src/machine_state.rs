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
/// `installing` 正在把 cc-monitor 装上去（那台还没有）· `updating` 正在换成这一版（那台有旧的）·
/// `down` 这一轮没连上（`reason` 说为什么）· `host_key_changed` 主机指纹与记下的不一样 · `needs_update` 连着但那台旧 ·
/// `newer` 连着但那台比这一版新 · `disabled` 「连接这台」关着 · `unsupported` 那台做不了 · `unknown` 还没有任何一轮的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub enum MachineStateKind {
    Up,
    Connecting,
    Installing,
    Updating,
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
    /// 没连上 / 做不了的原因码（`down`：拨号那一层的闭集码；`unsupported`：`not_unix`；本机没连上：`local`）。
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
    /// 指纹不对时那台这一次出示的主机指纹（`host_key_changed` 才有；与记下的那枚比对用）。
    pub seen_host_key: Option<String>,
    /// 这一轮没连上 / 做不了时的复制详情（`down` · `host_key_changed` · `unsupported` 才可能有）：时刻 · 机器 · 本机 · 命令 ·
    /// 断在（哪一步没成的那一句）· 码 · 原话。界面照原样跟在问题行那句后面（［复制详情］），自己不拼。说不出 ⇒ 空。
    pub detail: Option<String>,
}

/// 「做不了」的原因码。
pub const NOT_UNIX: &str = "not_unix";

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
    /// 指纹不对那一次那台出示的指纹（拨号那一层带回）。
    seen_key: Option<String>,
    /// 那台握手报的构建标识（只进日志与诊断信息）。
    build: Option<String>,
    /// 拨号那一层最近一次没拨成的那一句 ＋ 详情（这一轮收尾时用）。
    dial_said: Option<crate::detail::Said>,
    /// 这一轮没成的那一句 ＋ 详情（接常驻后端那一步记；比拨号那一层的更完整，收尾时先用它）。
    round_said: Option<crate::detail::Said>,
    /// 收尾时定下的那份详情（读点原样交出）。
    detail: Option<String>,
}

static TABLE: Mutex<BTreeMap<String, Entry>> = Mutex::new(BTreeMap::new());

/// 那台变了之后往外推一帧的出口（壳起来时装上：读那台的成品、发 `machine-state` 事件）。没装 ⇒ 不推。
type Out = Box<dyn Fn(&str) + Send + Sync>;
static OUT: std::sync::OnceLock<Out> = std::sync::OnceLock::new();

/// 装推送出口（只装一次）。
pub(crate) fn install_out(out: impl Fn(&str) + Send + Sync + 'static) {
    if OUT.set(Box::new(out)).is_err() {
        tracing::warn!("[machine] 推送出口装了第二次，沿用第一次那个");
    }
}

/// 那台变了 ⇒ 推一帧（锁外调）。
pub(crate) fn notify(origin: &str) {
    if let Some(out) = OUT.get() {
        out(origin);
    }
}

fn with<R>(origin: &str, f: impl FnOnce(&mut Entry) -> R) -> R {
    let r = {
        let mut g = TABLE.lock().unwrap_or_else(|e| e.into_inner());
        f(g.entry(origin.to_string()).or_default())
    };
    notify(origin);
    r
}

/// 这一轮开始拨 / 到了哪一步。
pub(crate) fn connecting(origin: &str, stage: &str) {
    with(origin, |e| {
        e.state = Some(MachineStateKind::Connecting);
        e.reason = None;
        e.stage = Some(stage.to_string());
        e.detail = None;
        if stage == "deploy" {
            // 新一轮从头起：上一轮记下的那几句不跟进来。
            e.round_said = None;
            e.dial_said = None;
        }
    });
}

/// 这一版的产品版本号（全产品一个源：monitor 那个包的版本，与安装包 · 界面同一个号）。
pub(crate) const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 与这一版同一份字节 ⇒ 这一版的号；别的报不出 ⇒ `None`。
fn product_version(relation: VersionRelation) -> Option<String> {
    (relation == VersionRelation::Same).then(|| PRODUCT_VERSION.to_string())
}

/// 正在把这一版放上去：`first` ＝ 那台还没有（装）；否则是换掉旧的（更新）。
pub(crate) fn deploying(origin: &str, first: bool) {
    with(origin, |e| {
        e.state = Some(if first {
            MachineStateKind::Installing
        } else {
            MachineStateKind::Updating
        });
        e.reason = None;
        e.stage = None;
        e.detail = None;
    });
}

/// 连上时自动放这一版：这次运行里那台握过手（报过版本关系）⇒ 是换旧的（更新）；没握过 ⇒ 当作第一次装。
pub(crate) fn deploying_auto(origin: &str) {
    let seen = TABLE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(origin)
        .is_some_and(|e| e.relation.is_some());
    deploying(origin, !seen);
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
        e.seen_key = None;
        e.build = Some(build.to_string());
        e.dial_said = None;
        e.round_said = None;
        e.detail = None;
    });
}

/// 那台最近一次握手报的构建标识（诊断信息用）；没握过手 ⇒ `None`。
pub(crate) fn build_of(origin: &str) -> Option<String> {
    TABLE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(origin)
        .and_then(|e| e.build.clone())
}

/// 拨号那一层没拨成（后端 ack 带回的原因码 · 那台出示的指纹 · 那一句与详情；老后端没给码 ⇒ 码与指纹不记，那一句照记）。
pub(crate) fn dial_failed(
    origin: &str,
    why: Option<&str>,
    fingerprint: Option<&str>,
    said: &crate::detail::Said,
) {
    with(origin, |e| {
        if let Some(w) = why {
            e.dial_why = Some(w.to_string());
            e.seen_key = fingerprint.map(str::to_string);
        }
        e.dial_said = Some(said.clone());
    });
}

/// 这一轮没成的那一句 ＋ 详情（接常驻后端那一步没成时记；收尾时 [`down`] / [`unsupported`] 交给读点）。
pub(crate) fn round_failed(origin: &crate::origin::Origin, said: &crate::detail::Said) {
    with(&origin.0, |e| e.round_said = Some(said.clone()));
}

/// 收尾那一份详情：这一轮记下的（没有 ⇒ 拨号那一层的）那一句进「断在」，其余照它的详情（拨号那一层是本机后端写的：
/// 「机器」是本机那一份后端）；连的是哪一台进「对象」，码是这一轮的原因码。
fn round_detail(target: &str, e: &mut Entry) -> Option<String> {
    let said = e.round_said.take().or_else(|| e.dial_said.take())?;
    use copy_core::detail::Label;
    let step = said.said.clone();
    Some(
        said.with_target(copy_core::detail::Target::Machine(target))
            .with_item(Label::Hop, &step)
            .with_item(Label::Code, e.reason.as_deref().unwrap_or(""))
            .detail,
    )
}

/// 这一轮没连上：原因取拨号那一层记下的码（没有 ⇒ `other`）；指纹不对单列一态。
pub(crate) fn down(origin: &str) {
    with(origin, |e| {
        let why = e.dial_why.take().unwrap_or_else(|| "other".to_string());
        e.state = Some(if why == "host_key" {
            MachineStateKind::HostKeyChanged
        } else {
            e.seen_key = None;
            MachineStateKind::Down
        });
        e.reason = Some(why);
        e.stage = None;
        e.detail = round_detail(origin, e);
    });
}

/// 那台做不了（这条流就此收工）。
pub(crate) fn unsupported(origin: &str, why: &str) {
    with(origin, |e| {
        e.state = Some(MachineStateKind::Unsupported);
        e.reason = Some(why.to_string());
        e.stage = None;
        e.detail = round_detail(origin, e);
    });
}

/// 部署计划问到的那台的系统（给人看的名字，`deploy_contract::Os::label`）。
pub(crate) fn note_os(origin: &str, name: &str) {
    with(origin, |e| e.os = Some(name.to_string()));
}

/// 机器从表里拿掉 ⇒ 那一格作废（读点照机器表说停用 / 没问到）。
pub(crate) fn forget(origin: &str) {
    TABLE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(origin);
    notify(origin);
}

/// 每一态给哪几颗修法（只在这里定）。
pub(crate) fn fixes_of(state: MachineStateKind, reason: Option<&str>) -> Vec<MachineFix> {
    use MachineFix as F;
    if reason == Some(LOCAL_DOWN) {
        return vec![F::Retry];
    }
    match state {
        MachineStateKind::Down => match reason {
            Some("auth") | Some("key_unreadable") => vec![F::PushKey, F::ConnSettings],
            Some("password") => vec![F::PushKey],
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
        seen_host_key: if state == MachineStateKind::HostKeyChanged {
            e.seen_key
        } else {
            None
        },
        detail: if enabled { e.detail } else { None },
    }
}

/// 本机那个口上占着的、构建与这一版不同的后端（终端里先敲 `ccm` 起的旧 / 新那一份）：它的构建标识。
/// 起本机后端那一趟探口时记（`local_backend_host::probe_listen_socket`）；是我们这一版 / 没人 ⇒ 清掉。
static LOCAL_FOREIGN: Mutex<Option<String>> = Mutex::new(None);

/// 记下（或清掉）本机口上那一份别的构建。
pub(crate) fn note_local_foreign(build: Option<&str>) {
    *LOCAL_FOREIGN.lock().unwrap_or_else(|e| e.into_inner()) = build.map(str::to_string);
    notify(crate::inbound_client::LOCAL_ORIGIN);
}

/// 本机那一台的成品：版本就是手上这一版（本机后端换装只认同一版，`local_backend_host::hello_verdict`；没带后端字节 ⇒ 开发构建、不可比）；
/// 通道在 ⇒ 连着；不在而口上占着别的构建（终端里先起的那一份）⇒ 按两边构建的序说「要更新」/「较新」（解不出序 ⇒ 不可比、照没连上说）；
/// 别的不在 ⇒ 没连上。本机没连上一律原因码 `local`，修法只给［重试］。
pub(crate) fn local_product(channel: bool, mine: Option<&str>) -> MachineState {
    let foreign = LOCAL_FOREIGN
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let relation = match (channel, mine, foreign.as_deref()) {
        (true, Some(_), _) => VersionRelation::Same,
        (false, Some(m), Some(theirs)) => {
            match (
                deploy_contract::build_order(theirs),
                deploy_contract::build_order(m),
            ) {
                (Some(t), Some(o)) if t < o => VersionRelation::Older,
                (Some(t), Some(o)) if t > o => VersionRelation::Newer,
                _ => VersionRelation::Incomparable,
            }
        }
        (false, Some(_), None) => VersionRelation::Same,
        (_, None, _) => VersionRelation::Incomparable,
    };
    let state = match (channel, relation) {
        (true, _) => MachineStateKind::Up,
        (false, VersionRelation::Older) => MachineStateKind::NeedsUpdate,
        (false, VersionRelation::Newer) => MachineStateKind::Newer,
        (false, _) => MachineStateKind::Down,
    };
    let reason = (!channel).then(|| LOCAL_DOWN.to_string());
    MachineState {
        state,
        fixes: fixes_of(state, reason.as_deref()),
        reason,
        stage: None,
        version: (relation == VersionRelation::Same).then(|| PRODUCT_VERSION.to_string()),
        version_relation: Some(relation),
        os: None,
        seen_host_key: None,
        detail: None,
    }
}

/// 本机那条连不上的原因码。
pub const LOCAL_DOWN: &str = "local";

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/machine_state_tests.rs"]
mod tests;
