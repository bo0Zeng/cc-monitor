//! **第 7 批的一次换形**：「对那台做事时怎么做」那几格（每一家的启动命令 · 恢复进不进 tmux · 每号模型 · 上下文上限表）从本机
//! `config.json`（monitor 的文件）搬到各台后端自己的 `prefs.json`（[`super::machine_prefs`]）。一次换形、不留旧兼容；**下一版整份删**。
//!
//! 照轮换换形那次的做法与事故教训（只由常驻的那一个写、读不懂不当空的搬、一个文件一个写者）：
//!
//! 1. **谁换**：本机常驻后端（拿到常驻门牌的那一个进程，[`migrate_here`] 只在那里调）；一次性进程一律不换、不交。
//! 2. **本机那一份**：`prefs.json` 不在 ⇒ 在 `config.json` 所在目录那把跨进程锁里**只读** `config.json`（与 monitor 同一种锁），
//!    本机那几格写成 `prefs.json`。`config.json` 一个字节不动 ——
//!    删那几个键是 monitor 的事（它是 `config.json` 唯一的写者），它读到本机 `prefs-read` 说 `ok` 才删、原文另存 `config.json.pre-7`。
//!    `config.json` 读不懂 / 不在 ⇒ 一个字节不写（读不懂的不当空的搬）。远端每台那几格不搬（不为旧配置留兼容：在那台设置里重设）。
//! 3. **机器表**（`config.json` 的 `remote` ⇒ 本机 `machines.json`，[`super::machines`]）：同一位、同一把锁里只读，`machines.json` 不在才换；
//!    每台残留的 `resumeCommand` 等旧格不带过去；`remote` 在而认不出（没有 `hosts` 数组）⇒ 不写、留原样。monitor 读到本机 `machines-read`
//!    说 `ok` 才删 `remote`。各家旧的那一格数据目录（键名问注册表 `old_home_pref_of`）并进本机那一份 `prefs.json` 的 `agentHomes`。
//!
//! 4. **`prefs.json` 第 1 版 ⇒ 第 2 版**（[`reshape_prefs_in`]）：恢复那一格从开关 `resumeInTmux` 换成三选 `resumeWhere`（开 ⇒ tmux，关 ⇒ 托管）；
//!    同一位、起来时先于上面几步换；读不懂的一个字节不动。读口只认第 2 版（旧键不留兼容）。
//!
//! 旧形状（今天各写者的那几格）：通用页 `resumeCommand` · 本机 `localResumeCommand` · 机器表每台 `remote.hosts[].resumeCommand`
//! （空 ⇒ 通用页那一格）· `resumeInTmux`（各台同一格）· `contextLimits`（各台同一张）· `accounts.byMachine.<origin>.modelByAccount`
//! （本机是 `<local>`）。启动命令只记给默认那一家（今天界面就只对它用这一格）。坏值那一条丢掉、不拖垮整份（写口那几条规矩同 `prefs-set`）。

use super::machines;
use crate::common::machine_prefs::{self, write as write_prefs, Prefs, PREFS_ROWS_MAX};
use crate::common::own_state::Doc;
use crate::platform::Own;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// monitor 那份设置文件的名字（在数据目录里；monitor `config.rs::resolve_config_path` 同一处）。
pub(crate) const CONFIG_NAME: &str = "config.json";

/// 本机在 `accounts.byMachine` 里的键（界面的 `LOCAL_ORIGIN`）。
const LOCAL_ORIGIN: &str = "<local>";

/// monitor 那份设置文件（只读，换形的来源）：状态（读不懂 ⇒ 不搬；上限照声明表）。
const CONFIG: Doc<Value> = Doc::of(&relay_route_core::CONFIG_FILE);

/// 拆出来的那一份：本机的。
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Plan {
    pub(crate) local: Prefs,
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).map_or("", str::trim)
}

/// 照旧形状拆（纯）。
pub(crate) fn plan(cfg: &Value) -> Plan {
    let global = text(cfg, "resumeCommand");
    let resume_where = resume_where_of(cfg.get("resumeInTmux"));
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
            at.to_string().replace('~', "~0").replace('/', "~1")
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
            resume_where,
            models: models(at),
            context_limits: limits.clone(),
            agent_homes: BTreeMap::new(),
            ..Prefs::default()
        }
    };
    let mut local = one(
        &cmd_wire::Machine::named(LOCAL_ORIGIN.to_string()),
        text(cfg, "localResumeCommand"),
    );
    // 数据目录只说本机（设置页那一格从来只管这一台）。
    local.agent_homes = old_homes(cfg);
    Plan { local }
}

/// 旧开关 `resumeInTmux` 读成三选：开 ⇒ tmux；关 / 没有 ⇒ 托管（关着时今天就落托管）。`config.json` 那一格与第 1 版 `prefs.json` 同一条。
fn resume_where_of(old: Option<&Value>) -> cmd_wire::session::ResumeWhere {
    use cmd_wire::session::ResumeWhere;
    if old.and_then(Value::as_bool) == Some(true) {
        ResumeWhere::Tmux
    } else {
        ResumeWhere::Managed
    }
}

/// 换第 1 版 `prefs.json` 那一次的结局（判据按它说；生产只记一行日志）。
#[derive(Debug, PartialEq)]
pub(crate) enum Reshaped {
    /// 不在，或已不是第 1 版：什么都不做。
    Nothing,
    /// 第 1 版读不懂（那一句）：一个字节不写。
    Unreadable(String),
    /// 换成了第 2 版。
    Done,
    /// 写没成（那一句）：下次起来再试。
    WriteFailed(String),
}

/// 盘上第 1 版的 `prefs.json`（恢复那一格是开关 `resumeInTmux`）⇒ 第 2 版（三选 `resumeWhere`，[`resume_where_of`]），别的几格原样。
/// 换完照第 2 版的值域核一遍再写；核不过 ⇒ 一个字节不写（读不懂的不当空的换）。
pub(crate) fn reshape_prefs_in(own: &Own) -> Reshaped {
    use crate::common::own_state::Read;
    let Ok(at) = own.at_home(machine_prefs::DOC.rel) else {
        return Reshaped::Nothing;
    };
    // 同一份声明按原样读（换形要看旧键；读不懂 ⇒ 不动）。
    let mut v = match machine_prefs::DOC.read_as::<Value>(at.as_path()) {
        Read::Absent => return Reshaped::Nothing,
        Read::Unreadable(why) => return Reshaped::Unreadable(why.said),
        Read::Present(v) => v,
    };
    let Some(o) = v.as_object_mut() else {
        return Reshaped::Unreadable(format!("{} is not an object", machine_prefs::FILE_NAME));
    };
    if o.get("version").and_then(Value::as_u64) != Some(1) {
        return Reshaped::Nothing;
    }
    let old = o.remove("resumeInTmux");
    let w = resume_where_of(old.as_ref());
    o.insert("resumeWhere".into(), serde_json::json!(w));
    o.insert("version".into(), serde_json::json!(machine_prefs::VERSION));
    let p = match serde_json::from_value::<machine_prefs::OnDisk>(v) {
        Ok(d) => d.prefs,
        Err(e) => return Reshaped::Unreadable(e.to_string()),
    };
    if let Err(why) = machine_prefs::check(&p) {
        return Reshaped::Unreadable(why);
    }
    match write_prefs(own, &p) {
        Ok(()) => Reshaped::Done,
        Err(e) => Reshaped::WriteFailed(e.said),
    }
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
    /// 写成了本机那一份。
    Done,
    /// 写没成（那一句）：下次起来再试。
    WriteFailed(String),
}

/// 换形那一次的本体（句柄是参数）。
pub(crate) fn migrate_in(own: &Own) -> Migrated {
    use crate::common::own_state::Read;
    let Some(prefs_at) = own.at_home(machine_prefs::DOC.rel).ok() else {
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
        CONFIG.read(cfg_at.as_path())
    };
    let cfg = match read {
        Read::Absent => return Migrated::NoConfig,
        Read::Unreadable(why) => return Migrated::Unreadable(why.said),
        Read::Present(v) if v.is_object() => v,
        Read::Present(_) => return Migrated::Unreadable(format!("{CONFIG_NAME} is not an object")),
    };
    let p = plan(&cfg);
    // `prefs.json` 在 ＝ 换过了（中途断了 ⇒ 下次起来整趟重来，同一份结果）。
    match write_prefs(own, &p.local) {
        Ok(()) => Migrated::Done,
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
    use crate::common::own_state::Read;
    let Some(at) = own.at_home(machines::DOC.rel).ok() else {
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
        CONFIG.read(cfg_at.as_path())
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
    use crate::common::own_state::Read;
    let Some(at) = own.root().and_then(|r| r.join(CONFIG_NAME).ok()) else {
        return BTreeMap::new();
    };
    match CONFIG.read(at.as_path()) {
        Read::Present(v) => old_homes(&v)
            .into_iter()
            .map(|(k, d)| (k, std::path::PathBuf::from(d)))
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// 换过一次了（[`migrate_here`] 填）：一个进程只换一次。
static RAN: OnceLock<()> = OnceLock::new();

/// 本机常驻后端起来时调一次（只在拿到常驻门牌的那个进程里）。
pub fn migrate_here(own: &Own) {
    if RAN.set(()).is_err() {
        tracing::warn!("换形那一段被叫了第二次（只该在常驻那一位起来时叫一次）：这一次不换");
        return;
    }
    match reshape_prefs_in(own) {
        Reshaped::Nothing => {}
        Reshaped::Done => tracing::info!("prefs.json 换成了第 2 版（恢复时放在哪改三选）"),
        Reshaped::Unreadable(why) => tracing::warn!("prefs.json 第 1 版读不懂，没换：{why}"),
        Reshaped::WriteFailed(why) => tracing::warn!("prefs.json 没换成，下次起来再试：{why}"),
    }
    match migrate_in(own) {
        Migrated::AlreadyThere | Migrated::NoConfig => {}
        Migrated::Done => tracing::info!("这台的偏好从 config.json 搬进了 prefs.json"),
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

#[cfg(test)]
#[path = "../../../tests/backend/control/prefs_migration_tests.rs"]
mod tests;
