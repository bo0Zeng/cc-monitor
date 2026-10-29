//! **机器表**：这一端连各台用的那张表（每台的名字 · 地址 · 端口 · 用户 · 钥匙路径 · 备用地址 · 跳板 · 连不连）。
//! 住拨号那一端的本机后端（`~/.cc-monitor/machines.json`，[`relay_route_core::MACHINES_REL`]）：钥匙路径是这台上的文件，
//! 手机有自己的一张（同一形、不共用一份）。
//!
//! - 谁写：只有本模块的 [`answer_set_in`]（帧面 `machines-set` ＋ 派生的 CLI 面）与第 7 批换形那一次（[`super::prefs_migration`]，下一版删）。
//!   补丁全按键认元素（[`MachineEdit`]），锁内现读现判 ⇒ 设置页改一格、另一扇窗口同写都不会互相盖掉。
//! - 一台的那一形（[`Host`]）、名字、判准、组拨号请求都住拨号核心（`ssh_dial::machine`），手机核心链同一份；
//!   判准没过的那一处说成界面那一句也在那里（[`table_fault`]，手机编辑页同读）。`machines-try` 与写口同一份。
//! - 主机钥匙不在这张表里：在拨号核心那份库（`dial/known_hosts.rs`）。
//! - 读不懂：照空表办，`machines-read` 照实报 `unreadable`；`machines-set` 拒（不拿空的盖掉原文）。
//! - 每次写完响一下（[`changes`]），流连接推 `changed {machines}`，monitor 照它重问、对齐远端那几条流。

use crate::common::said::Said;
use crate::platform::Own;
use copy_core::copy_text;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// 那份文件名（字面量只住契约 crate）。
pub const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::MACHINES_REL);

/// 机器表：状态（读不懂 · 比这一版新 ⇒ 不覆盖；落点 · 上限 · 版本照声明表）。
pub(crate) const DOC: crate::common::own_state::Doc<OnDisk> =
    crate::common::own_state::Doc::of(&relay_route_core::MACHINES_FILE);

/// 一张表至多几台。
pub(crate) const HOSTS_MAX: usize = 256;

/// 一台（盘上那一形，也是线上那一形）：住拨号核心。
pub(crate) use ssh_dial::machine::{table_fault, Host, MachineFault, MachineFaultCode, MachineRow};

/// 盘上整份（顶层 `version` 归读写口）。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OnDisk {
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
    fn state(&self) -> cmd_wire::FileState {
        match self {
            Loaded::Absent => cmd_wire::FileState::Absent,
            Loaded::Present(_) => cmd_wire::FileState::Ok,
            Loaded::Unreadable(_) => cmd_wire::FileState::Unreadable,
        }
    }
}

/// 某个家目录底下那份文件。
pub(crate) fn path_in(home: &Path) -> PathBuf {
    home.join(DOC.rel)
}

/// 这一端此刻照用的那一张（现读；家目录说不出 · 没有 · 读不懂 ⇒ 空表）。拨号层经它按名字组请求（`dial::install_machines`）。
pub fn here() -> Vec<Host> {
    crate::platform::paths::home_dir()
        .map(|h| read_at(&path_in(&h)).hosts())
        .unwrap_or_default()
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
    use crate::common::own_state::Read;
    match DOC.read(path) {
        Read::Absent => Loaded::Absent,
        Read::Unreadable(why) => Loaded::Unreadable(why),
        Read::Present(d) if d.hosts.len() > HOSTS_MAX => Loaded::Unreadable(not_ours(
            path,
            format!("{} hosts (at most {HOSTS_MAX})", d.hosts.len()),
        )),
        Read::Present(d) => Loaded::Present(d.hosts),
    }
}

/// 锁里整份写进 `dir`（`~/.cc-monitor`）。
fn put(
    own: &Own,
    dir: &crate::platform::fs::OwnPath,
    held: &crate::platform::fs::OwnLock,
    hosts: &[Host],
) -> Result<(), Said> {
    DOC.write(
        own,
        dir,
        held,
        &OnDisk {
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
    put(own, &dir, &held, hosts)?;
    drop(held);
    ring();
    Ok(())
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
    /// 只改认出的那一台的 `field` 那一格。
    #[serde(rename = "setin", rename_all = "camelCase")]
    SetIn {
        r#where: Vec<ElemKey>,
        field: String,
        #[cfg_attr(test, ts(type = "unknown"))]
        value: Value,
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

    /// 动了认人的那几格（加一台 · 改名称 / 地址 / 端口 / 用户 / 备用地址）⇒ 改完的整张表要过 [`table_fault`]。
    fn moves_identity(&self) -> bool {
        match self {
            MachineEdit::InsertIn { .. } => true,
            MachineEdit::SetIn { field, .. } => {
                ["label", "host", "port", "user", "addresses"].contains(&field.as_str())
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
    /// `insertin`：已有满足 `where` 的。
    Exists,
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
            rows.push(value.as_object().cloned().unwrap_or_default());
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
        MachineEdit::SetIn { field, value, .. } => {
            let i = match found.as_slice() {
                [i] => *i,
                [] => return Applied::NoMatch,
                _ => return Applied::Ambiguous,
            };
            rows[i].insert(field.clone(), value.clone());
            Applied::Done
        }
    }
}

/// 一串补丁打完的样子。
#[derive(Debug)]
pub(crate) struct Patched {
    pub(crate) hosts: Vec<Host>,
    /// 有没有一条真动了。
    pub(crate) changed: bool,
}

/// 把一串补丁逐条打到 `base` 上，再过机器表那一道（动了认人的格才过）。写口与试算口同一份。**纯函数**。
pub(crate) fn patched(base: &[Host], edits: &[MachineEdit]) -> Result<Patched, MachinesSetErr> {
    let bad = |m: String| MachinesSetErr::BadArgs(crate::common::contract::malformed(&m));
    if edits.iter().any(|e| e.r#where().is_empty()) {
        return Err(bad("empty `where`".into()));
    }
    let mut rows: Vec<Map<String, Value>> = base
        .iter()
        .map(|h| match serde_json::to_value(h) {
            Ok(Value::Object(m)) => m,
            _ => Map::new(),
        })
        .collect();
    let mut changed = false;
    for e in edits {
        match apply(&mut rows, e) {
            Applied::Done => changed = true,
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
        if let Some(f) = table_fault(&hosts) {
            return Err(MachinesSetErr::BadMachine(f.refused_said(), f));
        }
    }
    Ok(Patched { hosts, changed })
}

// ───────────────────────────── 推送 ─────────────────────────────

/// 进程里那条「机器表变了」的铃（流连接订它推 `changed {machines}`）。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<()> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<()>> = std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<()>(16).0)
}

fn ring() {
    let _ = changes().send(());
    crate::control::machine_state::table_moved();
    // 各台的常驻流照新表对齐（只在常驻进程里有；一次性进程里改表不起流）。
    if crate::control::feeds::hosted() {
        crate::control::feeds::align();
    }
}

// ───────────────────────────── 三条命令 ─────────────────────────────

/// `machines-read` / `machines-set` 读回的那一份：状态 ＋ 照用的那一张（读不懂 ⇒ 空表）。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct MachinesReply {
    /// 没有这份 · 读得懂 · 读不懂（读不懂也照答，不是失败）。
    pub(crate) state: cmd_wire::FileState,
    /// `unreadable` 时的原因（那一句不带原话）。
    pub(crate) reason: Option<String>,
    /// `unreadable` 时的复制详情。
    pub(crate) detail: Option<String>,
    /// 那份文件的路径（家目录说不出 ⇒ `null`）。
    pub(crate) path: Option<String>,
    /// 照用的那一张：一台一行，带后端算好的名字（`{name, host}`；名字的算法只在拨号核心 `Host::name`，界面 · 壳都不再算）。
    pub(crate) hosts: Vec<MachineRow>,
    /// 每台记下的主机指纹（名字 → 指纹，库里没有的那台不出现；拨号核心那份库现读）。
    pub(crate) host_keys: std::collections::BTreeMap<String, Vec<String>>,
}

/// 库里每台记下的那几把的指纹（库读不出来 ⇒ 空：那一处另有拨号时的那一句）。
fn host_keys_of(home: &Path, hosts: &[Host]) -> std::collections::BTreeMap<String, Vec<String>> {
    let known = crate::dial::known_hosts::KNOWN_HOSTS;
    let text =
        crate::dial::known_hosts::read_at(&crate::platform::paths::home_join(home, known.rel))
            .unwrap_or_default();
    hosts
        .iter()
        .filter_map(|h| {
            let fps: Vec<String> = ssh_dial::known_hosts::known(&text, &h.endpoints())
                .iter()
                .map(ssh_dial::known_hosts::fingerprint)
                .collect();
            (!fps.is_empty()).then(|| (h.name().to_string(), fps))
        })
        .collect()
}

fn reply(l: &Loaded, path: Option<&Path>, home: Option<&Path>) -> MachinesReply {
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
        host_keys: home
            .map(|h| host_keys_of(h, &l.hosts()))
            .unwrap_or_default(),
        hosts: l.hosts().into_iter().map(MachineRow::of).collect(),
    }
}

/// `machines-read` 的本体（家目录是参数）。
pub(crate) fn answer_read_in(home: Option<&Path>) -> MachinesReply {
    let path = home.map(path_in);
    let l = path.as_deref().map_or(Loaded::Absent, read_at);
    reply(&l, path.as_deref(), home)
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
    let fault = match patched(&base, &req.edits) {
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

/// `machines-set` 的回包：写完读回的那一张。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct MachinesSet {
    pub(crate) machines: MachinesReply,
}

/// `machines-set` 的本体（家目录是参数）：锁内现读 → 逐条打 → 判 → 整份写 → 读回 → 响一下。一条都没动 ⇒ 不写、不响。
pub(crate) fn answer_set_in(
    home: Option<&Path>,
    own: &Own,
    req: &MachinesEdits,
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
    let p = patched(&base, &req.edits)?;
    if p.changed {
        put(own, &dir, &held, &p.hosts).map_err(io)?;
    }
    drop(held);
    if p.changed {
        ring();
    }
    Ok(MachinesSet {
        machines: reply(&read_at(&path), Some(&path), Some(home)),
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

            answer_set_in(crate::platform::paths::home_dir().as_deref(), own, &req)
        }
    }
}

// ───────────────────────────── 忘掉一台的主机指纹 ─────────────────────────────

/// `host-keys-forget` 的入参：哪一台（名字）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HostKeysForgetReq {
    pub(crate) machine: cmd_wire::Machine,
}

crate::wire::typed::codes! {
    /// `host-keys-forget` 的码：机器表读不懂 · 表里没有这一台 · 写没成。
    pub(crate) enum HostKeysForgetErr {
        Unreadable = "unreadable",
        ElementGone = "element_gone",
        IoFailed = "io_failed",
    }
}

/// `host-keys-forget` 的本体（家目录是参数）：库里删这台全部地址的行（拨号核心那条规矩），下回连上照首见记；读回机器表、响一下。
pub(crate) fn answer_forget_in(
    home: Option<&Path>,
    own: &Own,
    req: &HostKeysForgetReq,
) -> Result<MachinesReply, HostKeysForgetErr> {
    let home = home.ok_or_else(|| {
        HostKeysForgetErr::IoFailed(copy_text("beExitPolicy.answerSet.noHome", &[]))
    })?;
    let path = path_in(home);
    let hosts = match read_at(&path) {
        Loaded::Unreadable(why) => {
            return Err(HostKeysForgetErr::Unreadable(
                crate::common::contract::joined(why.said, why.raw),
            ))
        }
        l => l.hosts(),
    };
    let host = hosts
        .iter()
        .find(|h| req.machine.host_name() == Some(h.name()))
        .ok_or_else(|| {
            HostKeysForgetErr::ElementGone(copy_text("rsConfig.write.elementGone", &[]))
        })?;
    let addrs = host.endpoints();
    crate::dial::known_hosts::writer_at(own)
        .rewrite(&mut |t| ssh_dial::known_hosts::forgotten(t, &addrs))
        .map_err(|e| HostKeysForgetErr::IoFailed(crate::common::contract::joined(e.said, e.raw)))?;
    ring();
    Ok(reply(&read_at(&path), Some(&path), Some(home)))
}

/// `host-keys-forget`：忘掉这台记下的主机指纹（用户点了才调；写自家状态 ⇒ 只递写句柄）。
pub(crate) struct HostKeysForget;

impl crate::wire::typed::Command for HostKeysForget {
    const NAME: &'static str = "host-keys-forget";
    type Req = HostKeysForgetReq;
    type Reply = MachinesReply;
    type Err = HostKeysForgetErr;
}

impl crate::wire::typed::Handle for HostKeysForget {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: HostKeysForgetReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<MachinesReply, HostKeysForgetErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move { answer_forget_in(crate::platform::paths::home_dir().as_deref(), &needs, &req) }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/machines_tests.rs"]
mod tests;
