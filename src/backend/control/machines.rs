//! **机器表**：这一端连各台用的那张表（每台的名字 · 地址 · 端口 · 用户 · 钥匙路径 · 指纹 · 备用地址 · 跳板 · 连不连）。
//! 住拨号那一端的本机后端（`~/.cc-monitor/machines.json`，[`relay_route_core::MACHINES_REL`]）：钥匙路径是这台上的文件，
//! 手机有自己的一张（同一形、不共用一份）。
//!
//! - 谁写：只有本模块的 [`answer_set_in`]（帧面 `machines-set` ＋ 派生的 CLI 面）与第 7 批换形那一次（[`super::prefs_migration`]，下一版删）。
//!   补丁全按键认元素（[`MachineEdit`]），锁内现读现判 ⇒ 设置页改一格、拨号时固化指纹、另一扇窗口同写都不会互相盖掉。
//! - 判准只此一处（[`check_table`]）：名字不重 · 端口 1–65535 · 地址与用户非空。`machines-try` 与写口同一份。
//! - 「指纹记于哪一天」只由写口盖（[`stamp_pinned_day`]），不认补丁里带来的那一格。
//! - 读不懂：照空表办，`machines-read` 照实报 `unreadable`；`machines-set` 拒（不拿空的盖掉原文）。
//! - 每次写完响一下（[`changes`]），流连接推 `changed {machines}`，monitor 照它重问、对齐远端那几条流。

use crate::common::said::Said;
use crate::platform::Own;
use copy_core::copy_text;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// 那份文件名（字面量只住契约 crate）。
pub const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::MACHINES_REL);

/// 盘上那一版。
const VERSION: u32 = 1;

/// 读盘的上限。
const MAX_BYTES: u64 = 1024 * 1024;

/// 一张表至多几台。
pub(crate) const HOSTS_MAX: usize = 256;

/// 一台（盘上那一形，也是线上那一形）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Host {
    /// 这台的名字（origin）；空 ⇒ 用 `host`。
    #[serde(default)]
    pub(crate) label: String,
    /// 首选地址。
    #[serde(default)]
    pub(crate) host: String,
    /// 端口（缺 ＝ 22）。
    #[serde(default = "port_22")]
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) port: u64,
    #[serde(default)]
    pub(crate) user: String,
    /// 钥匙文件路径（这台上的；空 ＝ 照 ssh 的缺省找）。
    #[serde(default)]
    pub(crate) key_path: String,
    /// 固化的主机指纹（空 ＝ 没固化）。
    #[serde(default)]
    pub(crate) host_key_fingerprint: String,
    /// 指纹记下的那一天（`YYYY-MM-DD`，写口盖；空 ＝ 没记过）。
    #[serde(default)]
    pub(crate) host_key_pinned_at: String,
    /// 备用地址（竞发）。
    #[serde(default)]
    pub(crate) addresses: Vec<String>,
    /// 跳板：另一台的名字（空 ＝ 直连）。
    #[serde(default)]
    pub(crate) jump: String,
    /// 连不连这台。
    #[serde(default = "yes")]
    pub(crate) connect: bool,
}

fn port_22() -> u64 {
    22
}
fn yes() -> bool {
    true
}

impl Host {
    /// 这台的名字：`label` 去首尾空白后非空取它，否则 `host`。
    pub(crate) fn name(&self) -> &str {
        match self.label.trim() {
            "" => self.host.trim(),
            l => l,
        }
    }
}

/// 盘上整份。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OnDisk {
    version: u32,
    hosts: Vec<Host>,
}

/// 现读一次的结果。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Loaded {
    Absent,
    Present(Vec<Host>),
    /// 文件在、读不懂（那一句 ＋ 原话）。
    Unreadable(Said),
}

impl Loaded {
    /// 照用的那一张：读不懂 / 没有 ⇒ 空表。
    pub(crate) fn hosts(&self) -> Vec<Host> {
        match self {
            Loaded::Present(h) => h.clone(),
            Loaded::Absent | Loaded::Unreadable(_) => Vec::new(),
        }
    }
    fn state(&self) -> &'static str {
        match self {
            Loaded::Absent => "absent",
            Loaded::Present(_) => "ok",
            Loaded::Unreadable(_) => "unreadable",
        }
    }
}

/// 某个家目录底下那份文件。
pub(crate) fn path_in(home: &Path) -> PathBuf {
    home.join(relay_route_core::MACHINES_REL)
}

fn not_ours(path: &Path, why: String) -> Said {
    Said::with_raw(
        copy_text(
            "beOwnState.read.notJson",
            &[("path", &path.display().to_string())],
        ),
        why,
    )
}

/// 读一次 `path`。每次都真去读盘。
pub(crate) fn read_at(path: &Path) -> Loaded {
    use crate::common::own_state::{read_json, Read};
    match read_json::<OnDisk>(path, MAX_BYTES) {
        Read::Absent => Loaded::Absent,
        Read::Unreadable(why) => Loaded::Unreadable(why),
        Read::Present(d) if d.version != VERSION => Loaded::Unreadable(not_ours(
            path,
            format!("version {} (this backend reads {VERSION})", d.version),
        )),
        Read::Present(d) if d.hosts.len() > HOSTS_MAX => Loaded::Unreadable(not_ours(
            path,
            format!("{} hosts (at most {HOSTS_MAX})", d.hosts.len()),
        )),
        Read::Present(d) => Loaded::Present(d.hosts),
    }
}

fn body_of(path: &Path, hosts: &[Host]) -> Result<Vec<u8>, Said> {
    crate::common::own_state::json_body(
        path,
        &OnDisk {
            version: VERSION,
            hosts: hosts.to_vec(),
        },
    )
}

fn mkdir_said(dir: &Path, e: std::io::Error) -> Said {
    Said::with_raw(
        copy_text(
            "reason.fs.writeFailed",
            &[
                ("path", &dir.display().to_string()),
                ("why", &copy_core::io_reason(e.kind())),
            ],
        ),
        &e,
    )
}

/// 整份写（自己拿 `~/.cc-monitor` 那把目录锁）：换形那一次用。写完响一下。
pub(crate) fn write(own: &Own, hosts: &[Host]) -> Result<(), Said> {
    let dir = own.home_dir()?;
    own.ensure_dir(&dir)
        .map_err(|e| mkdir_said(dir.as_path(), e))?;
    let held = own.lock(&dir)?;
    let path = dir.join(FILE_NAME)?;
    own.write(&path, &held, &body_of(path.as_path(), hosts)?)?;
    drop(held);
    ring();
    Ok(())
}

// ───────────────────────────── 判准 ─────────────────────────────

/// 机器表那一道没过的那一处（闭集码 ＋ 那台的名字 ＋ 那一格下面的那一句）；界面照 `code` 挑落在哪一格、照抄 `said`。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct MachineFault {
    /// `no_host` 地址空 · `no_user` 用户空 · `port` 端口不在 1–65535 · `name_taken` 与另一台同名。
    pub(crate) code: MachineFaultCode,
    /// 出问题的那台的名字（`label` 非空取它、否则 `host`）。
    pub(crate) name: String,
    /// 那一格下面的那一句（添加机器框 · 机器卡同读）。
    pub(crate) said: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub(crate) enum MachineFaultCode {
    NoHost,
    NoUser,
    Port,
    NameTaken,
}

impl MachineFault {
    /// 一处错（那一格下面的那一句照码写）。
    pub(crate) fn new(code: MachineFaultCode, name: String) -> Self {
        let said = match code {
            MachineFaultCode::NoHost => copy_text("addMachine.err.host", &[]),
            MachineFaultCode::NoUser => copy_text("addMachine.err.user", &[]),
            MachineFaultCode::Port => copy_text("addMachine.err.port", &[]),
            MachineFaultCode::NameTaken => copy_text("addMachine.err.taken", &[("name", &name)]),
        };
        Self { code, name, said }
    }

    /// 写口拒绝时的那一句（带那台的名字与「未保存」）。
    pub(crate) fn refused_said(&self) -> String {
        let name = self.name.as_str();
        match self.code {
            MachineFaultCode::NoHost => copy_text("rsConfig.machine.noHost", &[("name", name)]),
            MachineFaultCode::NoUser => copy_text("rsConfig.machine.noUser", &[("name", name)]),
            MachineFaultCode::Port => copy_text("rsConfig.machine.port", &[("name", name)]),
            MachineFaultCode::NameTaken => {
                copy_text("rsConfig.machine.nameTaken", &[("name", name)])
            }
        }
    }
}

/// 机器表成不成立：名字（`label` 非空取它、否则 `host`，去首尾空白、大小写敏感）不重 · 端口 1–65535 · 地址与用户非空。
/// 不成立 ⇒ 第一处；成立 ⇒ `None`。**纯函数**。
pub(crate) fn check_table(hosts: &[Host]) -> Option<MachineFault> {
    let mut seen = std::collections::HashSet::new();
    for h in hosts {
        let fault = |code| Some(MachineFault::new(code, h.name().to_string()));
        if h.host.trim().is_empty() {
            return fault(MachineFaultCode::NoHost);
        }
        if h.user.trim().is_empty() {
            return fault(MachineFaultCode::NoUser);
        }
        if !(1..=65535).contains(&h.port) {
            return fault(MachineFaultCode::Port);
        }
        if !seen.insert(h.name().to_string()) {
            return fault(MachineFaultCode::NameTaken);
        }
    }
    None
}

// ───────────────────────────── 补丁 ─────────────────────────────

/// 认一台的一条：那一台里 `fields` 按序取**第一个非空字符串**，它 == `equals`
/// （名字就是 `["label", "host"]`：`label` 非空取它、否则 `host`）。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct ElemKey {
    pub(crate) fields: Vec<String>,
    pub(crate) equals: String,
}

/// 一条补丁：全按键认一台（`where` 每一条都对得上的**恰好一台**），锁内现读现判。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "op", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum MachineEdit {
    /// 只改认出的那一台的 `field` 那一格。`ifEmpty` ＝ 那一格已有值（非空串 / 非 null）就不动（拨号时自动固化指纹用它）。
    #[serde(rename = "setin", rename_all = "camelCase")]
    SetIn {
        r#where: Vec<ElemKey>,
        field: String,
        #[cfg_attr(test, ts(type = "unknown"))]
        value: Value,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        if_empty: Option<bool>,
    },
    /// 加一台：已有满足 `where` 的 ⇒ 整批拒；否则追加到末尾。
    #[serde(rename = "insertin")]
    InsertIn {
        r#where: Vec<ElemKey>,
        #[cfg_attr(test, ts(type = "unknown"))]
        value: Value,
    },
    /// 删一台：认出恰好一台才删。
    #[serde(rename = "removein")]
    RemoveIn { r#where: Vec<ElemKey> },
}

impl MachineEdit {
    fn r#where(&self) -> &[ElemKey] {
        match self {
            MachineEdit::SetIn { r#where, .. }
            | MachineEdit::InsertIn { r#where, .. }
            | MachineEdit::RemoveIn { r#where } => r#where,
        }
    }

    /// 动了认人的那几格（加一台 · 改名称 / 地址 / 端口 / 用户）⇒ 改完的整张表要过 [`check_table`]。固化指纹这类格不查。
    fn moves_identity(&self) -> bool {
        match self {
            MachineEdit::InsertIn { .. } => true,
            MachineEdit::SetIn { field, .. } => {
                ["label", "host", "port", "user"].contains(&field.as_str())
            }
            MachineEdit::RemoveIn { .. } => false,
        }
    }
}

/// 一条补丁落下去的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Applied {
    Done,
    NoMatch,
    Ambiguous,
    /// `ifEmpty`：那一格已有值。
    Kept,
    /// `insertin`：已有满足 `where` 的。
    Exists,
}

/// 一台的「指纹记于哪一天」盖成了什么（`day` 空 ＝ 指纹清了、日子一起清）。`origin` ＝ 补丁认那台用的键。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PinnedDay {
    pub(crate) origin: cmd_wire::Machine,
    pub(crate) day: String,
}

crate::wire::typed::codes! {
    /// `machines-set` 的码：盘上那份读不懂（不拿空的盖掉它）· 认不出那一台（`ambiguous` ＝ 认出了不止一台）·
    /// 要加的那一台已在 · 改完的表不成立（那一处）· 写没成。
    pub(crate) enum MachinesSetErr {
        Unreadable = "unreadable",
        ElementGone = "element_gone" => Gone,
        ElementExists = "element_exists",
        BadMachine = "bad_machine" => MachineFault,
        IoFailed = "io_failed",
    }
}

/// 认不出那一台时多带的那一格。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Gone {
    /// 认出了不止一台（不猜是哪台）。
    pub(crate) ambiguous: bool,
}

/// 元素里 `fields` 按序第一个非空字符串。
fn elem_key<'a>(elem: &'a Map<String, Value>, fields: &[String]) -> Option<&'a str> {
    fields.iter().find_map(|f| {
        elem.get(f)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    })
}

fn elem_matches(o: &Map<String, Value>, r#where: &[ElemKey]) -> bool {
    r#where
        .iter()
        .all(|k| elem_key(o, &k.fields) == Some(k.equals.as_str()))
}

fn hits(rows: &[Map<String, Value>], r#where: &[ElemKey]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, o)| elem_matches(o, r#where))
        .map(|(i, _)| i)
        .collect()
}

fn apply(rows: &mut Vec<Map<String, Value>>, e: &MachineEdit) -> Applied {
    let found = hits(rows, e.r#where());
    match e {
        MachineEdit::InsertIn { value, .. } => {
            if !found.is_empty() {
                return Applied::Exists;
            }
            let mut v = value.as_object().cloned().unwrap_or_default();
            // 日子只由写口盖（下面 `stamp_pinned_day`），补丁里带来的那一格不认。
            v.remove("hostKeyPinnedAt");
            rows.push(v);
            Applied::Done
        }
        MachineEdit::RemoveIn { .. } => match found.as_slice() {
            [i] => {
                rows.remove(*i);
                Applied::Done
            }
            [] => Applied::NoMatch,
            _ => Applied::Ambiguous,
        },
        MachineEdit::SetIn {
            field,
            value,
            if_empty,
            ..
        } => {
            let i = match found.as_slice() {
                [i] => *i,
                [] => return Applied::NoMatch,
                _ => return Applied::Ambiguous,
            };
            let elem = &mut rows[i];
            let taken = elem
                .get(field)
                .is_some_and(|v| !v.is_null() && v.as_str().is_none_or(|s| !s.trim().is_empty()));
            if if_empty.unwrap_or(false) && taken {
                return Applied::Kept;
            }
            elem.insert(field.clone(), value.clone());
            Applied::Done
        }
    }
}

/// 写进了指纹的那一条（改那一格 · 新增一台带着指纹）⇒ 同一台的 `hostKeyPinnedAt` 盖上写的那一天；指纹写成空 ⇒ 日子清空。
fn stamp_pinned_day(
    rows: &mut [Map<String, Value>],
    e: &MachineEdit,
    day: &str,
) -> Option<PinnedDay> {
    let (r#where, fp) = match e {
        MachineEdit::SetIn {
            r#where,
            field,
            value,
            ..
        } if field == "hostKeyFingerprint" => (r#where, value.clone()),
        MachineEdit::InsertIn { r#where, value } => (
            r#where,
            value
                .get("hostKeyFingerprint")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        _ => return None,
    };
    let given = fp.as_str().is_some_and(|s| !s.trim().is_empty());
    if !given && matches!(e, MachineEdit::InsertIn { .. }) {
        return None;
    }
    let day = if given {
        day.to_string()
    } else {
        String::new()
    };
    let [i] = hits(rows, r#where)[..] else {
        return None;
    };
    rows[i].insert("hostKeyPinnedAt".into(), Value::String(day.clone()));
    Some(PinnedDay {
        origin: cmd_wire::Machine(
            r#where
                .first()
                .map(|k| k.equals.clone())
                .unwrap_or_default(),
        ),
        day,
    })
}

/// 写的那一天（UTC 的 `YYYY-MM-DD`）。
pub(crate) fn pinned_day(t: std::time::SystemTime) -> String {
    let days = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    let (y, m, d) = copy_core::civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// 一串补丁打完的样子。
#[derive(Debug)]
pub(crate) struct Patched {
    pub(crate) hosts: Vec<Host>,
    pub(crate) pinned: Vec<PinnedDay>,
    /// 带 `ifEmpty`、那一格已有值而没动的条数。
    pub(crate) kept: usize,
    /// 有没有一条真动了。
    pub(crate) changed: bool,
}

/// 把一串补丁逐条打到 `base` 上，再过机器表那一道（动了认人的格才过）。写口与试算口同一份。**纯函数**（日子是参数）。
pub(crate) fn patched(
    base: &[Host],
    edits: &[MachineEdit],
    day: &str,
) -> Result<Patched, MachinesSetErr> {
    let bad = |m: String| MachinesSetErr::BadArgs(crate::common::contract::malformed(&m));
    if edits.iter().any(|e| e.r#where().is_empty()) {
        return Err(bad("empty `where`".into()));
    }
    for e in edits {
        if let MachineEdit::SetIn { field, .. } = e {
            if field == "hostKeyPinnedAt" {
                return Err(bad("`hostKeyPinnedAt` is stamped by the writer".into()));
            }
        }
    }
    let mut rows: Vec<Map<String, Value>> = base
        .iter()
        .map(|h| match serde_json::to_value(h) {
            Ok(Value::Object(m)) => m,
            _ => Map::new(),
        })
        .collect();
    let mut pinned = Vec::new();
    let mut kept = 0;
    let mut changed = false;
    for e in edits {
        match apply(&mut rows, e) {
            Applied::Done => {
                changed = true;
                pinned.extend(stamp_pinned_day(&mut rows, e, day));
            }
            Applied::Kept => kept += 1,
            Applied::NoMatch => {
                return Err(MachinesSetErr::ElementGone(
                    copy_text("rsConfig.write.elementGone", &[]),
                    Gone { ambiguous: false },
                ))
            }
            Applied::Ambiguous => {
                return Err(MachinesSetErr::ElementGone(
                    copy_text("rsConfig.write.elementGone", &[]),
                    Gone { ambiguous: true },
                ))
            }
            Applied::Exists => {
                return Err(MachinesSetErr::ElementExists(copy_text(
                    "rsConfig.write.elementExists",
                    &[],
                )))
            }
        }
    }
    if rows.len() > HOSTS_MAX {
        return Err(bad(format!("{} hosts (at most {HOSTS_MAX})", rows.len())));
    }
    let hosts = rows
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            serde_json::from_value::<Host>(Value::Object(m))
                .map_err(|e| bad(format!("hosts[{i}]: {e}")))
        })
        .collect::<Result<Vec<Host>, _>>()?;
    if changed && edits.iter().any(MachineEdit::moves_identity) {
        if let Some(f) = check_table(&hosts) {
            return Err(MachinesSetErr::BadMachine(f.refused_said(), f));
        }
    }
    Ok(Patched {
        hosts,
        pinned,
        kept,
        changed,
    })
}

// ───────────────────────────── 推送 ─────────────────────────────

/// 进程里那条「机器表变了」的铃（流连接订它推 `changed {machines}`）。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<()> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<()>> = std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<()>(16).0)
}

fn ring() {
    let _ = changes().send(());
}

// ───────────────────────────── 三条命令 ─────────────────────────────

/// `machines-read` / `machines-set` 读回的那一份：状态 ＋ 照用的那一张（读不懂 ⇒ 空表）。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct MachinesReply {
    /// `absent`（没有这份）· `ok` · `unreadable`（读不懂：也照答，不是失败）。
    #[cfg_attr(test, ts(type = "\"absent\" | \"ok\" | \"unreadable\""))]
    pub(crate) state: &'static str,
    /// `unreadable` 时的原因（那一句不带原话）。
    pub(crate) reason: Option<String>,
    /// `unreadable` 时的复制详情。
    pub(crate) detail: Option<String>,
    /// 那份文件的路径（家目录说不出 ⇒ `null`）。
    pub(crate) path: Option<String>,
    /// 照用的那一张。
    pub(crate) hosts: Vec<Host>,
}

fn reply(l: &Loaded, path: Option<&Path>) -> MachinesReply {
    let why = match l {
        Loaded::Unreadable(w) => Some(w),
        _ => None,
    };
    let (reason, detail) = crate::wire::detail::unreadable("machines-read", why);
    MachinesReply {
        state: l.state(),
        reason,
        detail,
        path: path.map(|p| p.display().to_string()),
        hosts: l.hosts(),
    }
}

/// `machines-read` 的本体（家目录是参数）。
pub(crate) fn answer_read_in(home: Option<&Path>) -> MachinesReply {
    let path = home.map(path_in);
    let l = path.as_deref().map_or(Loaded::Absent, read_at);
    reply(&l, path.as_deref())
}

/// `machines-try` / `machines-set` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct MachinesEdits {
    pub(crate) edits: Vec<MachineEdit>,
}

/// `machines-try` 的回包。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct MachinesTried {
    /// 这串补丁若现在落盘，机器表那一道没过的那一处；过 ⇒ `null`（补丁本身认不出那一台 · 不成形 ⇒ 也是 `null`，留给写口去拒）。
    pub(crate) fault: Option<MachineFault>,
}

/// `machines-try` 的本体：盘上一个字节不动。
pub(crate) fn answer_try_in(home: Option<&Path>, req: &MachinesEdits) -> MachinesTried {
    let base = home
        .map(path_in)
        .map_or(Loaded::Absent, |p| read_at(&p))
        .hosts();
    let day = pinned_day(crate::common::time::now());
    let fault = match patched(&base, &req.edits, &day) {
        Err(MachinesSetErr::BadMachine(_, f)) => Some(f),
        // 往表里加一台、而同名的已经在了 ⇒ 这一道说成同名。
        Err(MachinesSetErr::ElementExists(_)) => req.edits.iter().find_map(|e| match e {
            MachineEdit::InsertIn { r#where, .. } => Some(MachineFault::new(
                MachineFaultCode::NameTaken,
                r#where
                    .first()
                    .map(|k| k.equals.clone())
                    .unwrap_or_default(),
            )),
            _ => None,
        }),
        _ => None,
    };
    MachinesTried { fault }
}

/// `machines-set` 的回包：写完读回的那一张 ＋ 写口替那几台盖上的日子 ＋ 带 `ifEmpty` 没动的条数。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct MachinesSet {
    pub(crate) machines: MachinesReply,
    pub(crate) pinned: Vec<PinnedDay>,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) kept: usize,
}

/// `machines-set` 的本体（家目录 · 日子是参数）：锁内现读 → 逐条打 → 判 → 整份写 → 读回 → 响一下。一条都没动 ⇒ 不写、不响。
pub(crate) fn answer_set_in(
    home: Option<&Path>,
    own: &Own,
    req: &MachinesEdits,
    day: &str,
) -> Result<MachinesSet, MachinesSetErr> {
    let io = |e: Said| MachinesSetErr::IoFailed(crate::common::contract::joined(e.said, e.raw));
    let home = home
        .ok_or_else(|| MachinesSetErr::IoFailed(copy_text("beExitPolicy.answerSet.noHome", &[])))?;
    let path = path_in(home);
    let dir = own.home_dir().map_err(io)?;
    own.ensure_dir(&dir)
        .map_err(|e| io(mkdir_said(dir.as_path(), e)))?;
    let held = own.lock(&dir).map_err(|e| io(e.into()))?;
    let base = match read_at(&path) {
        Loaded::Unreadable(why) => {
            return Err(MachinesSetErr::Unreadable(crate::common::contract::joined(
                why.said, why.raw,
            )))
        }
        l => l.hosts(),
    };
    let p = patched(&base, &req.edits, day)?;
    if p.changed {
        let at = dir.join(FILE_NAME).map_err(io)?;
        let body = body_of(at.as_path(), &p.hosts).map_err(io)?;
        own.write(&at, &held, &body).map_err(io)?;
    }
    drop(held);
    if p.changed {
        ring();
    }
    Ok(MachinesSet {
        machines: reply(&read_at(&path), Some(&path)),
        pinned: p.pinned,
        kept: p.kept,
    })
}

crate::wire::typed::codes! {
    /// `machines-read` 没有别的码：读不懂是一个状态，不是一次失败。
    pub(crate) enum MachinesReadErr {}
}

crate::wire::typed::codes! {
    /// `machines-try` 没有别的码：说不出的一律当「这一道没话说」。
    pub(crate) enum MachinesTryErr {}
}

/// `machines-read`：这一端的机器表（同步文件 I/O ⇒ 阻塞档）。
pub(crate) struct MachinesRead;

impl crate::wire::typed::Command for MachinesRead {
    const NAME: &'static str = "machines-read";
    const TAKES_INPUT: bool = false;
    type Req = crate::wire::typed::NoArgs;
    type Reply = MachinesReply;
    type Err = MachinesReadErr;
}

impl crate::wire::typed::Handle for MachinesRead {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<MachinesReply, MachinesReadErr>> + Send + 'static
    {
        async move {
            Ok(answer_read_in(
                crate::platform::paths::home_dir().as_deref(),
            ))
        }
    }
}

/// `machines-try`：这串补丁若现在落盘，机器表那一道过不过（盘上不动）。
pub(crate) struct MachinesTry;

impl crate::wire::typed::Command for MachinesTry {
    const NAME: &'static str = "machines-try";
    type Req = MachinesEdits;
    type Reply = MachinesTried;
    type Err = MachinesTryErr;
}

impl crate::wire::typed::Handle for MachinesTry {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: MachinesEdits,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<MachinesTried, MachinesTryErr>> + Send + 'static
    {
        async move {
            Ok(answer_try_in(
                crate::platform::paths::home_dir().as_deref(),
                &req,
            ))
        }
    }
}

/// `machines-set`：改机器表，写完读回（写自家状态 ⇒ 只递写句柄）。
pub(crate) struct MachinesSetCmd;

impl crate::wire::typed::Command for MachinesSetCmd {
    const NAME: &'static str = "machines-set";
    type Req = MachinesEdits;
    type Reply = MachinesSet;
    type Err = MachinesSetErr;
}

impl crate::wire::typed::Handle for MachinesSetCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: MachinesEdits,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<MachinesSet, MachinesSetErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let own = &needs;

            answer_set_in(
                crate::platform::paths::home_dir().as_deref(),
                own,
                &req,
                &pinned_day(crate::common::time::now()),
            )
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/machines_tests.rs"]
mod tests;
