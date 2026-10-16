//! **第 7 批的一次换形**：「对那台做事时怎么做」那几格（每一家的启动命令 · 恢复进不进 tmux · 每号模型 · 上下文上限表）从本机
//! `config.json`（monitor 的文件）搬到各台后端自己的 `prefs.json`（[`super::machine_prefs`]）。一次换形、不留旧兼容；**下一版整份删**。
//!
//! 照轮换换形那次的做法与事故教训（只由常驻的那一个写、读不懂不当空的搬、一个文件一个写者）：
//!
//! 1. **谁换**：本机常驻后端（拿到常驻门牌的那一个进程，[`migrate_here`] 只在那里调）；一次性进程一律不换、不交。
//! 2. **本机那一份**：`prefs.json` 不在 ⇒ 在 `config.json` 所在目录那把跨进程锁里**只读** `config.json`（与 monitor 同一种锁），
//!    本机那几格写成 `prefs.json`，远端每台那几格写进「待交」（`prefs-handoff.json`）。`config.json` 一个字节不动 ——
//!    删那几个键是 monitor 的事（它是 `config.json` 唯一的写者），它读到本机 `prefs-read` 说 `ok` 才删、原文另存 `config.json.pre-7`。
//!    `config.json` 读不懂 / 不在 ⇒ 一个字节不写（读不懂的不当空的搬）。
//! 3. **远端那几份**：monitor 每台远端流握手时交 `remote-reach`，常驻那一位顺手 [`hand_over`]：那台 `prefs-read` 说 `absent`
//!    ⇒ 交一次 `prefs-set`（那一台整份），交成了删掉那一条；那台已经有（读得懂 · 读不懂都算）⇒ 不盖、也删掉；问不到 ⇒ 留着，下次连上再交，不出声。
//!
//! 4. **机器表**（`config.json` 的 `remote` ⇒ 本机 `machines.json`，[`super::machines`]）：同一位、同一把锁里只读，`machines.json` 不在才换；
//!    每台残留的 `resumeCommand` 等旧格不带过去；`remote` 在而认不出（没有 `hosts` 数组）⇒ 不写、留原样。monitor 读到本机 `machines-read`
//!    说 `ok` 才删 `remote`。各家旧的那一格数据目录（键名问注册表 `old_home_pref_of`）并进本机那一份 `prefs.json` 的 `agentHomes`。
//!
//! 旧形状（今天各写者的那几格）：通用页 `resumeCommand` · 本机 `localResumeCommand` · 机器表每台 `remote.hosts[].resumeCommand`
//! （空 ⇒ 通用页那一格）· `resumeInTmux`（各台同一格）· `contextLimits`（各台同一张）· `accounts.byMachine.<origin>.modelByAccount`
//! （本机是 `<local>`）。启动命令只记给默认那一家（今天界面就只对它用这一格）。坏值那一条丢掉、不拖垮整份（写口那几条规矩同 `prefs-set`）。

use super::machine_prefs::{self, write as write_prefs, Prefs, PREFS_ROWS_MAX};
use super::machines;
use crate::dial::remote_ask::{ask_json, Remote, Table};
use crate::platform::Own;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// monitor 那份设置文件的名字（在数据目录里；monitor `config.rs::resolve_config_path` 同一处）。
pub(crate) const CONFIG_NAME: &str = "config.json";

/// 本机在 `accounts.byMachine` 里的键（界面的 `LOCAL_ORIGIN`）。
const LOCAL_ORIGIN: &str = "<local>";

/// 「待交」那一版。
const HANDOFF_VERSION: u32 = 1;

/// 读盘的上限（同 `prefs.json`：几张小表；机器表封顶 256 台）。
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// 拆出来的那几份：本机一份 · 远端每台一份（按 origin）。
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Plan {
    pub(crate) local: Prefs,
    pub(crate) remote: BTreeMap<String, Prefs>,
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).map_or("", str::trim)
}

/// 照旧形状拆（纯）。
pub(crate) fn plan(cfg: &Value) -> Plan {
    let global = text(cfg, "resumeCommand");
    let tmux = cfg
        .get("resumeInTmux")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let limits: BTreeMap<String, u64> = cfg
        .get("contextLimits")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(k, _)| !k.trim().is_empty())
        .filter_map(|(k, v)| v.as_u64().filter(|n| *n > 0).map(|n| (k.clone(), n)))
        .take(PREFS_ROWS_MAX)
        .collect();
    let models = |at: &cmd_wire::Machine| -> BTreeMap<String, String> {
        cfg.pointer(&format!(
            "/accounts/byMachine/{}/modelByAccount",
            at.as_wire_str().replace('~', "~0").replace('/', "~1")
        ))
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| v.as_str().map(|m| (k.clone(), m.trim().to_string())))
        .filter(|(k, m)| !k.is_empty() && shell_quote_core::model_name_ok(m))
        .take(PREFS_ROWS_MAX)
        .collect()
    };
    let one = |at: &cmd_wire::Machine, own: &str| -> Prefs {
        let launcher = if own.is_empty() { global } else { own };
        let mut launchers = BTreeMap::new();
        if !launcher.is_empty() && shell_quote_core::launcher_refused_char(launcher).is_none() {
            launchers.insert(
                crate::agents::default_kind().to_string(),
                launcher.to_string(),
            );
        }
        Prefs {
            launchers,
            resume_in_tmux: tmux,
            models: models(at),
            context_limits: limits.clone(),
            agent_homes: BTreeMap::new(),
            ..Prefs::default()
        }
    };
    let mut local = one(
        &cmd_wire::Machine(LOCAL_ORIGIN.to_string()),
        text(cfg, "localResumeCommand"),
    );
    // 数据目录只说本机（设置页那一格从来只管这一台）。
    local.agent_homes = old_homes(cfg);
    let mut remote = BTreeMap::new();
    let hosts = cfg.pointer("/remote/hosts").and_then(Value::as_array);
    for h in hosts.into_iter().flatten() {
        let origin = match text(h, "label") {
            "" => text(h, "host"),
            l => l,
        };
        if origin.is_empty() || remote.len() >= PREFS_ROWS_MAX {
            continue;
        }
        let p = one(
            &cmd_wire::Machine(origin.to_string()),
            text(h, "resumeCommand"),
        );
        if p != Prefs::default() {
            remote.insert(origin.to_string(), p);
        }
    }
    Plan { local, remote }
}

/// 起来那一次的结局（判据按它说；生产只记一行日志）。
#[derive(Debug, PartialEq)]
pub(crate) enum Migrated {
    /// `prefs.json` 已经在（换过了，或这台从没有旧设置后来设过）：什么都不做。
    AlreadyThere,
    /// 没有 `config.json`（不是桌面那台 · 新装）：什么都不做。
    NoConfig,
    /// `config.json` 读不懂：一个字节不写（那一句）。
    Unreadable(String),
    /// 写成了本机那一份，`pending` 台远端待交。
    Done { pending: usize },
    /// 写没成（那一句）：下次起来再试。
    WriteFailed(String),
}

/// 「待交」盘上那一形。
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Handoff {
    version: u32,
    machines: BTreeMap<String, Prefs>,
}

fn handoff_path(own: &Own) -> Option<std::path::PathBuf> {
    own.at_home(relay_route_core::PREFS_HANDOFF_REL)
        .ok()
        .map(|p| p.as_path().to_path_buf())
}

/// 还待交的那几台（读不懂 / 不在 ⇒ 空：读不懂的那一份不再交，也不写回）。
pub(crate) fn read_pending(own: &Own) -> BTreeMap<String, Prefs> {
    use crate::common::own_state::{read_json, Read};
    match handoff_path(own).map(|p| read_json::<Handoff>(&p, MAX_BYTES)) {
        Some(Read::Present(h)) if h.version == HANDOFF_VERSION => h.machines,
        _ => BTreeMap::new(),
    }
}

/// 整份写「待交」；一台都不剩 ⇒ 删掉那份文件。
fn write_pending(own: &Own, machines: BTreeMap<String, Prefs>) -> Result<(), String> {
    let dir = own.home_dir().map_err(|e| e.said)?;
    let held = own
        .lock(&dir)
        .map_err(|e| crate::common::said::Said::from(e).said)?;
    let path = dir
        .join(relay_route_core::file_name_of(
            relay_route_core::PREFS_HANDOFF_REL,
        ))
        .map_err(|e| e.said)?;
    if machines.is_empty() {
        return match own.delete_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    let body = crate::common::own_state::json_body(
        path.as_path(),
        &Handoff {
            version: HANDOFF_VERSION,
            machines,
        },
    )
    .map_err(|e| e.said)?;
    own.write(&path, &held, &body).map_err(|e| e.said)
}

/// 换形那一次的本体（句柄是参数）。
pub(crate) fn migrate_in(own: &Own) -> Migrated {
    use crate::common::own_state::{read_json, Read};
    let Some(prefs_at) = own.at_home(relay_route_core::PREFS_REL).ok() else {
        return Migrated::NoConfig;
    };
    if machine_prefs::read_at(prefs_at.as_path()) != machine_prefs::Loaded::Absent {
        return Migrated::AlreadyThere;
    }
    let Some(dir) = own.root().filter(|d| d.as_path().is_dir()) else {
        return Migrated::NoConfig;
    };
    let Ok(cfg_at) = dir.join(CONFIG_NAME) else {
        return Migrated::NoConfig;
    };
    // 与 monitor 写 `config.json` 同一把目录锁里读：读到的是一份整的。读完就放（写 `prefs.json` 要拿家目录那把，可能就是同一把）。
    let read = {
        let _held = match own.lock(&dir) {
            Ok(h) => h,
            Err(e) => return Migrated::WriteFailed(crate::common::said::Said::from(e).said),
        };
        read_json::<Value>(cfg_at.as_path(), MAX_BYTES)
    };
    let cfg = match read {
        Read::Absent => return Migrated::NoConfig,
        Read::Unreadable(why) => return Migrated::Unreadable(why.said),
        Read::Present(v) if v.is_object() => v,
        Read::Present(_) => return Migrated::Unreadable(format!("{CONFIG_NAME} is not an object")),
    };
    let p = plan(&cfg);
    let pending = p.remote.len();
    // 先记待交、再写本机那一份：`prefs.json` 在 ＝ 换过了（中途断了 ⇒ 下次起来整趟重来，同一份结果）。
    if let Err(why) = write_pending(own, p.remote) {
        return Migrated::WriteFailed(why);
    }
    match write_prefs(own, &p.local) {
        Ok(()) => Migrated::Done { pending },
        Err(e) => Migrated::WriteFailed(e.said),
    }
}

/// 机器表那一次的结局（判据按它说；生产只记一行日志）。
#[derive(Debug, PartialEq)]
pub(crate) enum MachinesMoved {
    /// `machines.json` 已经在：什么都不做。
    AlreadyThere,
    /// 没有 `config.json`：什么都不做。
    NoConfig,
    /// `config.json` 读不懂，或 `remote` 在而认不出（没有 `hosts` 数组）：一个字节不写（那一句）。
    Unreadable(String),
    /// 写成了（几台）。
    Done(usize),
    /// 写没成（那一句）：下次起来再试。
    WriteFailed(String),
}

/// 旧形状的一台 ⇒ 新表那一形（宽读：类型不对的格当缺，`addresses` 认数组或多行文本；`resumeCommand` 那几格不带过去）。**纯函数**。
pub(crate) fn host_of(v: &Value) -> Option<machines::Host> {
    let o = v.as_object()?;
    let s = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let addresses = match o.get("addresses") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(str::to_string)
            .collect(),
        Some(Value::String(t)) => t
            .lines()
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };
    Some(machines::Host {
        label: s("label"),
        host: s("host"),
        port: o.get("port").and_then(Value::as_u64).unwrap_or(22),
        user: s("user"),
        key_path: s("keyPath"),
        host_key_fingerprint: s("hostKeyFingerprint"),
        host_key_pinned_at: s("hostKeyPinnedAt"),
        addresses,
        jump: s("jump"),
        connect: o.get("connect").and_then(Value::as_bool) != Some(false),
    })
}

/// 旧 `config.json` 的 `remote` ⇒ 新表（没有 `remote` ⇒ 空表；`remote` 在而没有 `hosts` 数组 ⇒ 认不出）。**纯函数**。
pub(crate) fn machines_of(cfg: &Value) -> Result<Vec<machines::Host>, String> {
    let Some(remote) = cfg.get("remote").filter(|r| !r.is_null()) else {
        return Ok(Vec::new());
    };
    let Some(hosts) = remote.get("hosts").and_then(Value::as_array) else {
        return Err("`remote` has no `hosts` array".into());
    };
    Ok(hosts
        .iter()
        .filter_map(host_of)
        .take(machines::HOSTS_MAX)
        .collect())
}

/// 机器表那一次的本体：`machines.json` 不在 ⇒ 锁里只读 `config.json`，`remote` 写成 `machines.json`（`config.json` 不动）。
pub(crate) fn migrate_machines_in(own: &Own) -> MachinesMoved {
    use crate::common::own_state::{read_json, Read};
    let Some(at) = own.at_home(relay_route_core::MACHINES_REL).ok() else {
        return MachinesMoved::NoConfig;
    };
    if machines::read_at(at.as_path()) != machines::Loaded::Absent {
        return MachinesMoved::AlreadyThere;
    }
    let Some(dir) = own.root().filter(|d| d.as_path().is_dir()) else {
        return MachinesMoved::NoConfig;
    };
    let Ok(cfg_at) = dir.join(CONFIG_NAME) else {
        return MachinesMoved::NoConfig;
    };
    let read = {
        let _held = match own.lock(&dir) {
            Ok(h) => h,
            Err(e) => return MachinesMoved::WriteFailed(crate::common::said::Said::from(e).said),
        };
        read_json::<Value>(cfg_at.as_path(), MAX_BYTES)
    };
    let cfg = match read {
        Read::Absent => return MachinesMoved::NoConfig,
        Read::Unreadable(why) => return MachinesMoved::Unreadable(why.said),
        Read::Present(v) if v.is_object() => v,
        Read::Present(_) => {
            return MachinesMoved::Unreadable(format!("{CONFIG_NAME} is not an object"))
        }
    };
    let hosts = match machines_of(&cfg) {
        Ok(h) => h,
        Err(why) => return MachinesMoved::Unreadable(why),
    };
    match machines::write(own, &hosts) {
        Ok(()) => MachinesMoved::Done(hosts.len()),
        Err(e) => MachinesMoved::WriteFailed(e.said),
    }
}

/// `config.json` 里各家旧的那一格数据目录 ⇒ `{kind: 路径}`：键名问注册表（[`crate::agents::old_home_pref_of`]，
/// 各家自己的旧键，只给换形用、下一版随本文件删）；空 / 不是绝对路径 ⇒ 那一家没有。
fn old_homes(cfg: &Value) -> BTreeMap<String, String> {
    crate::agents::registered_kinds()
        .filter_map(|kind| {
            let key = crate::agents::old_home_pref_of(kind)?;
            Some(text(cfg, key).trim().to_string())
                .filter(|d| machine_prefs::agent_home_ok(d))
                .map(|d| (kind.to_string(), d))
        })
        .collect()
}

/// 换形之前那一次（`prefs.json` 还不在）：数据目录还在 `config.json` 那一格旧键里 ⇒ 照它（只读）。
/// 起来时 [`super::machine_prefs::apply_agent_homes`] 在换形之前就要用到它，下一版随本文件删。
pub(crate) fn agent_homes_before_move(own: &Own) -> BTreeMap<String, std::path::PathBuf> {
    use crate::common::own_state::{read_json, Read};
    let Some(at) = own.root().and_then(|r| r.join(CONFIG_NAME).ok()) else {
        return BTreeMap::new();
    };
    match read_json::<Value>(at.as_path(), MAX_BYTES) {
        Read::Present(v) => old_homes(&v)
            .into_iter()
            .map(|(k, d)| (k, std::path::PathBuf::from(d)))
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// 常驻那一位的写句柄（[`migrate_here`] 填）：填了 ＝ 这个进程是常驻那一位，`remote-reach` 之后才交。
static RESIDENT_OWN: OnceLock<Own> = OnceLock::new();

/// 本机常驻后端起来时调一次（只在拿到常驻门牌的那个进程里）：换形 ＋ 记下「之后连上的那几台由我交」。
pub fn migrate_here(own: &Own) {
    if RESIDENT_OWN.set(own.clone()).is_err() {
        tracing::warn!("换形那一段被叫了第二次（只该在常驻那一位起来时叫一次）：这一次不换");
        return;
    }
    match migrate_in(own) {
        Migrated::AlreadyThere | Migrated::NoConfig => {}
        Migrated::Done { pending } => {
            tracing::info!("这台的偏好从 config.json 搬进了 prefs.json（远端待交 {pending} 台）")
        }
        Migrated::Unreadable(why) => tracing::warn!("config.json 读不懂，这台的偏好没搬：{why}"),
        Migrated::WriteFailed(why) => tracing::warn!("这台的偏好没搬成，下次起来再试：{why}"),
    }
    match migrate_machines_in(own) {
        MachinesMoved::AlreadyThere | MachinesMoved::NoConfig => {}
        MachinesMoved::Done(n) => {
            tracing::info!("机器表从 config.json 搬进了 machines.json（{n} 台）")
        }
        MachinesMoved::Unreadable(why) => tracing::warn!("config.json 的机器表认不出，没搬：{why}"),
        MachinesMoved::WriteFailed(why) => tracing::warn!("机器表没搬成，下次起来再试：{why}"),
    }
}

/// 交给一台的结局。
#[derive(Debug, PartialEq)]
pub(crate) enum HandedOver {
    /// 待交里没有这一台：一句都不问。
    Nothing,
    /// 那台还没有偏好 ⇒ 交过去了。
    Given,
    /// 那台已经有（读得懂 · 读不懂）⇒ 没盖。
    AlreadyThere,
    /// 问不到 / 交没成 ⇒ 留着，下次连上再交（那一句）。
    Kept(String),
}

/// 交给 `machine` 那一台（经可达表问它后端：先 `prefs-read`，没设过 ⇒ 整份 `prefs-set`）。
pub(crate) async fn hand_over(
    own: &Own,
    machine: &cmd_wire::Machine,
    tz: &crate::Tz,
    table: &Table,
    remote: &dyn Remote,
) -> HandedOver {
    use machine_prefs::{PrefsRead, PrefsSetCmd, PrefsState};
    let origin = machine.as_wire_str();
    let mut pending = read_pending(own);
    let Some(share) = pending.get(origin).cloned() else {
        return HandedOver::Nothing;
    };
    let read = ask_json::<PrefsRead>(origin, &Default::default(), tz, table, remote).await;
    let got = match read.map(|r| r.state) {
        Err(e) => return HandedOver::Kept(e.message),
        Ok(PrefsState::Absent) => {
            let set = machine_prefs::PrefsSet::from(&share);
            match ask_json::<PrefsSetCmd>(origin, &set, tz, table, remote).await {
                Ok(_) => HandedOver::Given,
                Err(e) => return HandedOver::Kept(e.message),
            }
        }
        Ok(PrefsState::Ok | PrefsState::Unreadable) => HandedOver::AlreadyThere,
    };
    pending.remove(origin);
    if let Err(why) = write_pending(own, pending) {
        tracing::warn!("待交那一份没改成（{origin} 下次连上会再问一次）：{why}");
    }
    got
}

/// `remote-reach` 登记完那一台之后调：常驻那一位、待交里有它 ⇒ 后台交一次（不挡那一答）。
pub(crate) fn after_reach(machine: &cmd_wire::Machine, tz: crate::Tz) {
    let Some(own) = RESIDENT_OWN.get().cloned() else {
        return;
    };
    if !read_pending(&own).contains_key(machine.as_wire_str()) {
        return;
    }
    let machine = machine.clone();
    tokio::spawn(async move {
        let remote = crate::dial::remote_ask::DialRemote;
        let got = hand_over(
            &own,
            &machine,
            &tz,
            &crate::dial::remote_ask::REACH,
            &remote,
        )
        .await;
        match got {
            HandedOver::Given => {
                tracing::info!("从旧设置搬来的偏好交给了 {}", machine.as_wire_str())
            }
            HandedOver::Kept(why) => tracing::info!(
                "偏好还没交给 {}，下次连上再交：{why}",
                machine.as_wire_str()
            ),
            HandedOver::Nothing | HandedOver::AlreadyThere => {}
        }
    });
}

#[cfg(test)]
#[path = "../../../tests/backend/control/prefs_migration_tests.rs"]
mod tests;
