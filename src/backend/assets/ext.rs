//! 设置里「扩展」那一页的后端：skill 与 MCP 跨机器的一张表（`ext-list`）· 装到一台的确认卡（枢纽 `ext-hub-*`，住 [`super::hub`]）·
//! 从一台卸掉（`ext-uninstall-*`，在被卸的那一台上判、写）。
//!
//! # 判定只在这里
//!
//! 界面只画格子：每格的态（`same` · `differs` · `missing` · `project`）、那一格唯一的那个按钮做什么、从哪台拿哪一版、
//! 为什么没有按钮，全由 [`table`] 答；摘要不出后端（线上一个 `digest` 都没有），界面没有东西可比。
//!
//! - `same` / `differs` 只拿**用户级**那几份比：持有人最多的那一版算「这一版」（打平时本机那一份优先），和它一样 ⇒ `same`。
//! - 用户级没有、只在项目里有 ⇒ `project`；哪儿都没有 ⇒ `missing`。
//! - 用户级 MCP 住 agent 自己的热状态文件（它自己一直在重写）⇒ 只读：能当来源装进别的机器的项目 `.mcp.json`，这里不往它里面写、也不从它里面删。
//! - 「新见到」：这台目录第一次见到这个条目的时刻晚于上一次来看这一页（`asset_catalog::Visits`）。
//!
//! # 卸
//!
//! 装记录里有 ⇒ 只撤装时写进去的（skill：`skill_install` 那一份逐文件判；MCP：那一条的摘要对得上才不问）；
//! 没有 ⇒ 不是 cc-monitor 装的：先挪 / 抄进 `~/.cc-monitor/backups/`，再删，预览里明说。

use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::asset_catalog::{self, Asset, Catalog, KIND_MCP, KIND_SKILL};
use super::door::{self, Door, Refused};
use super::skill_flow::Record;

type Answer = Result<Value, (&'static str, String)>;

// ───────────────────────── 线上形状（界面那一份由这里生成） ─────────────────────────

/// 一个扩展住哪一级。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "level", rename_all = "lowercase")]
pub enum ExtLoc {
    User,
    Project { dir: String },
}

/// 条目种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub enum ExtKind {
    Skill,
    Mcp,
}

/// 一格的态（闭集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub enum ExtState {
    /// 用户级有，且是「这一版」。
    Same,
    /// 用户级有，但不是「这一版」。
    Differs,
    /// 哪儿都没有。
    Missing,
    /// 用户级没有，只在项目里有。
    Project,
}

/// 装到一台时两头各在哪一级。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ExtScope {
    pub from: ExtLoc,
    pub to: ExtLoc,
}

/// 一格上那唯一的按钮。`from` = 来源那台的键（`null` = 本机后端自己，同枢纽的 `from` / `to`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(
    tag = "verb",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum ExtAction {
    Install {
        from: Option<String>,
        from_name: String,
        scope: ExtScope,
    },
    Replace {
        from: Option<String>,
        from_name: String,
        scope: ExtScope,
    },
    Uninstall {
        at: ExtLoc,
    },
}

/// 表里一台机器（一列）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ExtMachine {
    /// 枢纽认它的键：本机后端自己 = `null`；别的台 = 可达表里的名字；没连上 = `null` 且 `reachable = false`。
    pub key: Option<String>,
    /// 就是本机后端这一台。
    pub here: bool,
    pub reachable: bool,
    /// 给人看的名字（可达表里的名字；没有就是那台自报的称呼）。
    pub name: String,
    /// 那台上开过会话的项目目录（装到项目时给人选）。
    pub projects: Vec<String>,
}

/// 一格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ExtCell {
    pub state: ExtState,
    /// 在那台上住哪几处（用户级在前）。
    pub places: Vec<ExtLoc>,
    /// skill：那台上的目录（「在文件窗口里打开」用）。
    pub dir: Option<String>,
    pub action: Option<ExtAction>,
    /// 没有按钮时为什么（现状 ＋ 能做什么）。
    pub note: Option<String>,
}

/// 抽屉里「看内容」那几行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ExtDetail {
    pub label: String,
    pub value: String,
}

/// 表里一行（一个条目）。`cells` 与 [`ExtList::machines`] 同序。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ExtRow {
    pub kind: ExtKind,
    pub name: String,
    pub about: Option<String>,
    pub detail: Vec<ExtDetail>,
    pub new: bool,
    pub cells: Vec<ExtCell>,
}

/// `ext-list` 的成品。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ExtList {
    pub machines: Vec<ExtMachine>,
    pub rows: Vec<ExtRow>,
    pub problems: Vec<String>,
}

/// 确认卡上一格待填（或沿用目标机已有的值）的值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ExtSlot {
    /// `env` 或 `headers`。
    pub field: String,
    pub key: String,
    /// 目标机那一条里已有这个键的值（可以沿用；不填就沿用）。
    pub kept: bool,
}

/// 看过的那一份的记号：应用时原样交回，枢纽拿它判「看过之后变了没有」（界面不解读）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ExtTokens {
    pub source: String,
    pub target: Option<String>,
}

/// `ext-hub-preview` 的成品：确认卡。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ExtCard {
    pub kind: ExtKind,
    pub name: String,
    /// 被写那台上的落点（skill 目录 / MCP 配置文件）。
    pub path: String,
    /// 要写的那几个（skill：目录里的相对路径；MCP：那份配置文件）。
    pub writes: Vec<String>,
    /// 装上之后和现在一样（没什么可写）。
    pub unchanged: bool,
    /// 要留意的几件（说人话）。
    pub suspects: Vec<String>,
    /// 装不了的原因（有它就不给确认）。
    pub stop: Option<String>,
    /// MCP：装上之后那一条（待填的值是 `null`）。
    pub config: Option<String>,
    pub slots: Vec<ExtSlot>,
    pub tokens: ExtTokens,
}

/// `ext-uninstall-preview` 的成品：卸之前那张卡。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ExtUninstallCard {
    pub kind: ExtKind,
    pub name: String,
    pub path: String,
    /// cc-monitor 装的（装记录里有）。
    pub recorded: bool,
    /// 要删的那几个（skill：相对路径；MCP：那份配置文件里的那一条）。
    pub files: Vec<String>,
    /// 删之前先放到哪（不是 cc-monitor 装的才有）。
    pub backup: Option<String>,
    /// 这一趟会做什么（说人话：装的 / 不是装的 · 改过没有 · 删了回不回得去）。
    pub said: String,
    pub token: String,
}

/// 应用 / 卸完之后的成品。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ExtDone {
    pub path: String,
    /// 写了 / 删了的那几个。
    pub changed: Vec<String>,
    /// 做成了，但有一件要知道的（执行位没改成 · 没记下来 · 空目录没收掉 …）。
    pub note: Option<String>,
}

impl ExtLoc {
    /// 线上 `{level, dir?}` → 位置；项目目录须是那台上的绝对路径。
    pub(crate) fn from_arg(v: Option<&Value>, key: &str) -> Result<ExtLoc, (&'static str, String)> {
        let v = v.ok_or((
            "bad_args",
            crate::common::contract::malformed(&format!("missing `{key}`")),
        ))?;
        let loc: ExtLoc = serde_json::from_value(v.clone()).map_err(|e| {
            (
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "`{key}` must be {{level: user}} or {{level: project, dir}}: {e}"
                )),
            )
        })?;
        match loc {
            ExtLoc::User => Ok(ExtLoc::User),
            ExtLoc::Project { dir } => Ok(ExtLoc::Project {
                dir: super::mcp_edit::project_root(&dir)?,
            }),
        }
    }

    pub(crate) fn project(&self) -> Option<&str> {
        match self {
            ExtLoc::User => None,
            ExtLoc::Project { dir } => Some(dir),
        }
    }

    fn of(asset: &Asset) -> ExtLoc {
        match &asset.project {
            None => ExtLoc::User,
            Some(d) => ExtLoc::Project { dir: d.clone() },
        }
    }
}

impl ExtKind {
    pub(crate) fn from_arg(args: &Value) -> Result<ExtKind, (&'static str, String)> {
        serde_json::from_value(args.get("kind").cloned().unwrap_or(Value::Null)).map_err(|_| {
            (
                "bad_args",
                crate::common::contract::malformed("`kind` must be skill or mcp"),
            )
        })
    }
}

// ───────────────────────── 表（纯） ─────────────────────────

/// 一台机器在表里的样子（内部：带目录里的 id）。
struct Column<'a> {
    id: &'a str,
    m: ExtMachine,
}

fn columns<'a>(cat: &'a Catalog, reach: &[(String, Option<String>)]) -> Vec<Column<'a>> {
    let mut cols: Vec<Column> = cat
        .machines
        .iter()
        .map(|(id, snap)| {
            let here = *id == cat.self_id;
            let key = if here {
                None
            } else {
                reach
                    .iter()
                    .find(|(_, peer)| peer.as_deref() == Some(id.as_str()))
                    .map(|(o, _)| o.clone())
            };
            Column {
                id: id.as_str(),
                m: ExtMachine {
                    reachable: here || key.is_some(),
                    name: key.clone().unwrap_or_else(|| snap.label.clone()),
                    key,
                    here,
                    projects: snap.project_dirs.clone(),
                },
            }
        })
        .collect();
    cols.sort_by(|a, b| (!a.m.here, &a.m.name, a.id).cmp(&(!b.m.here, &b.m.name, b.id)));
    cols
}

/// 「这一版」：用户级那几份里持有人最多的摘要；打平时本机那一份优先，再按列序。
fn canonical<'a>(held: &[Option<&'a Asset>], cols: &[Column]) -> Option<&'a str> {
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    for a in held.iter().flatten() {
        *count.entry(a.digest.as_str()).or_default() += 1;
    }
    let best = *count.values().max()?;
    let tied: Vec<&str> = count
        .iter()
        .filter(|(_, n)| **n == best)
        .map(|(d, _)| *d)
        .collect();
    cols.iter()
        .zip(held)
        .filter(|(c, _)| c.m.here)
        .chain(cols.iter().zip(held))
        .find_map(|(_, a)| a.map(|a| a.digest.as_str()).filter(|d| tied.contains(d)))
}

/// 装到项目时替那台先挑一个：和来源同一个目录（那台也有这个项目）⇒ 它；否则那台的第一个；那台没有项目 ⇒ `None`。
fn pick_project(m: &ExtMachine, like: Option<&str>) -> Option<String> {
    like.filter(|d| m.projects.iter().any(|p| p == d))
        .map(str::to_string)
        .or_else(|| m.projects.first().cloned())
}

fn about_of(a: &Asset) -> Option<String> {
    let s = &a.summary;
    if a.kind == KIND_SKILL {
        return s
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if let Some(cmd) = s.get("command").and_then(Value::as_str) {
        let args: Vec<&str> = s
            .get("args")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        return Some(
            std::iter::once(cmd)
                .chain(args)
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    s.get("url").and_then(Value::as_str).map(str::to_string)
}

fn detail_of(a: &Asset) -> Vec<ExtDetail> {
    let s = &a.summary;
    let line = |label: String, value: String| ExtDetail { label, value };
    if a.kind == KIND_SKILL {
        let n = |k: &str| s.get(k).and_then(Value::as_u64).unwrap_or(0).to_string();
        let mut out = vec![line(
            copy_text("beExt.detail.files", &[]),
            copy_text(
                "beExt.detail.filesValue",
                &[("files", &n("files")), ("bytes", &n("bytes"))],
            ),
        )];
        if s.get("binary").and_then(Value::as_bool) == Some(true) {
            out.push(line(
                copy_text("beExt.detail.binary", &[]),
                copy_text("beExt.detail.binaryValue", &[]),
            ));
        }
        return out;
    }
    // MCP：目录里那一份本来就不带密钥值（`asset_catalog::mcp_summary`），字段名是用户自己写在配置里的那几个。
    let mut out = Vec::new();
    if let Some(o) = s.as_object() {
        for (k, v) in o {
            let value = match v {
                Value::String(t) => t.clone(),
                Value::Array(items) => items
                    .iter()
                    .map(|x| {
                        x.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| x.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
                other => other.to_string(),
            };
            out.push(line(k.clone(), value));
        }
    }
    out
}

/// 各台目录 ＋ 可达表（`origin` → 那台目录的 id）⇒ 条目 × 机器一张表。**判定只在这里。**
pub fn table(cat: &Catalog, reach: &[(String, Option<String>)]) -> ExtList {
    let cols = columns(cat, reach);
    let mut by_key: BTreeMap<(String, String), Vec<Vec<&Asset>>> = BTreeMap::new();
    for (i, c) in cols.iter().enumerate() {
        for a in &cat.machines[c.id].assets {
            by_key
                .entry((a.kind.clone(), a.name.clone()))
                .or_insert_with(|| vec![Vec::new(); cols.len()])[i]
                .push(a);
        }
    }
    let rows = by_key
        .into_iter()
        .map(|((kind, name), per)| row(cat, &cols, &kind, &name, &per))
        .collect();
    ExtList {
        machines: cols.into_iter().map(|c| c.m).collect(),
        rows,
        problems: Vec::new(),
    }
}

fn row(cat: &Catalog, cols: &[Column], kind: &str, name: &str, per: &[Vec<&Asset>]) -> ExtRow {
    let is_mcp = kind == KIND_MCP;
    let user: Vec<Option<&Asset>> = per
        .iter()
        .map(|v| v.iter().copied().find(|a| a.project.is_none()))
        .collect();
    let canon = canonical(&user, cols);
    // 来源：持有「这一版」的那几台里本机优先、再按列序、要连得上；用户级谁都没有 ⇒ 项目里有的那几台里同样挑。
    let held: Vec<Option<&Asset>> = match canon {
        Some(d) => user.iter().map(|a| a.filter(|a| a.digest == d)).collect(),
        None => per.iter().map(|v| v.first().copied()).collect(),
    };
    let source = pick_source(cols, &held);
    let shown = source
        .map(|(_, a)| a)
        .or_else(|| per.iter().flatten().next().copied());
    let first_seen = cat.known.get(&asset_catalog::entry_key(kind, name));
    let new = cat.visits.prev > 0 && first_seen.is_some_and(|t| *t > cat.visits.prev);
    let cells = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut places: Vec<ExtLoc> = per[i].iter().map(|a| ExtLoc::of(a)).collect();
            places.sort_by_key(|l| l.project().map(str::to_string));
            let state = match (user[i], canon) {
                (Some(a), Some(d)) if a.digest == d => ExtState::Same,
                (Some(_), _) => ExtState::Differs,
                (None, _) if !per[i].is_empty() => ExtState::Project,
                (None, _) => ExtState::Missing,
            };
            let dir = if is_mcp {
                None
            } else {
                user[i]
                    .or_else(|| per[i].first().copied())
                    .and_then(|a| a.dir.clone())
            };
            let (action, note) = action_for(c, state, &places, is_mcp, source);
            ExtCell {
                state,
                places,
                dir,
                action,
                note,
            }
        })
        .collect();
    ExtRow {
        kind: if is_mcp { ExtKind::Mcp } else { ExtKind::Skill },
        name: name.to_string(),
        about: shown.and_then(about_of),
        detail: shown.map(detail_of).unwrap_or_default(),
        new,
        cells,
    }
}

/// 有这一版、连得上的那几台里挑一台：本机优先，再按列序。
fn pick_source<'c, 'a>(
    cols: &'c [Column<'a>],
    held: &[Option<&'a Asset>],
) -> Option<(&'c Column<'a>, &'a Asset)> {
    cols.iter()
        .zip(held)
        .filter_map(|(c, a)| a.filter(|_| c.m.reachable).map(|a| (c, a)))
        .min_by_key(|(c, _)| !c.m.here)
}

fn action_for(
    c: &Column,
    state: ExtState,
    places: &[ExtLoc],
    is_mcp: bool,
    source: Option<(&Column, &Asset)>,
) -> (Option<ExtAction>, Option<String>) {
    if !c.m.reachable {
        return (None, Some(copy_text("beExt.note.offline", &[])));
    }
    let uninstall = |at: ExtLoc| (Some(ExtAction::Uninstall { at }), None);
    let bring = |replace: bool| -> (Option<ExtAction>, Option<String>) {
        let Some((src, a)) = source else {
            return (None, Some(copy_text("beExt.note.noSource", &[])));
        };
        let from_loc = ExtLoc::of(a);
        let to = if is_mcp || (!replace && from_loc != ExtLoc::User) {
            match pick_project(&c.m, from_loc.project()) {
                Some(dir) => ExtLoc::Project { dir },
                None if is_mcp => return (None, Some(copy_text("beExt.note.noProject", &[]))),
                None => ExtLoc::User,
            }
        } else {
            ExtLoc::User
        };
        let scope = ExtScope { from: from_loc, to };
        let (from, from_name) = (src.m.key.clone(), src.m.name.clone());
        let act = if replace {
            ExtAction::Replace {
                from,
                from_name,
                scope,
            }
        } else {
            ExtAction::Install {
                from,
                from_name,
                scope,
            }
        };
        (Some(act), None)
    };
    match state {
        ExtState::Same if is_mcp => (None, Some(copy_text("beExt.note.userMcpReadOnly", &[]))),
        ExtState::Same => uninstall(ExtLoc::User),
        ExtState::Differs if is_mcp => (None, Some(copy_text("beExt.note.userMcpReadOnly", &[]))),
        ExtState::Differs => bring(true),
        ExtState::Missing => bring(false),
        ExtState::Project => uninstall(
            places
                .iter()
                .find(|l| **l != ExtLoc::User)
                .cloned()
                .unwrap_or(ExtLoc::User),
        ),
    }
}

/// 目录的写口（现扫 ＋ 记下 ＋ 交回整份与读不出来的那几份）：生产 = `asset_catalog::answer_current`。
pub(crate) type Current<'a> =
    &'a dyn Fn(bool) -> Result<(Catalog, Vec<String>), (&'static str, String)>;

/// `ext-list {visit}`：这台现扫一次、记下（`current` = 目录的写口，由 `inbound.rs` 递进来），出表。
pub(crate) fn answer_list(
    args: &Value,
    current: Current,
    reach: &[(String, Option<String>)],
) -> Answer {
    let visit = match args.get("visit") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("`visit` must be a boolean"),
            ))
        }
    };
    let (cat, problems) = current(visit)?;
    let mut list = table(&cat, reach);
    list.problems = problems;
    serde_json::to_value(list).map_err(|e| ("io_failed", e.to_string()))
}

/// 可达表的一份快照：`(origin, 那台目录的 id)`。
pub(crate) fn reach_of(table: &crate::stream::remote_ask::Table) -> Vec<(String, Option<String>)> {
    crate::stream::remote_ask::lock(table)
        .iter()
        .map(|(o, r)| (o.clone(), r.peer.clone()))
        .collect()
}

// ───────────────────────── 这台上的那几处（可喂夹具） ─────────────────────────

/// 一台机器上扩展住哪（生产 = 这台的环境现解；判据交临时目录）。
pub(crate) struct Env {
    /// 用户级 skill 的根。
    pub skills: Option<PathBuf>,
    /// 用户级 MCP 那份文件（只读）。
    pub user_mcp: Option<PathBuf>,
    /// 装记录那份文件。
    pub ledger: Option<PathBuf>,
    /// 家目录（备份落在它下面的 `.cc-monitor/backups`）。
    pub home: Option<PathBuf>,
}

impl Env {
    pub(crate) fn here() -> Env {
        Env {
            skills: crate::agents::skills_root(),
            user_mcp: crate::agents::user_mcp_file(),
            ledger: super::skill_ledger::ledger_path(),
            home: crate::platform::paths::home_dir(),
        }
    }

    /// 那一级 skill 的根。
    pub(crate) fn skill_root(&self, at: &ExtLoc) -> Option<PathBuf> {
        match at {
            ExtLoc::User => self.skills.clone(),
            ExtLoc::Project { dir } => crate::agents::skill_root_at(Some(Path::new(dir))),
        }
    }
}

/// 这台上缺了家目录那一格（装记录 · 备份 · skill 根都从它来）。
fn need<T>(v: Option<T>) -> Result<T, (&'static str, String)> {
    v.ok_or(("io_failed", copy_text("beExt.uninstall.noHome", &[])))
}

fn str_arg<'a>(args: &'a Value, k: &str) -> Result<&'a str, (&'static str, String)> {
    args.get(k).and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!("missing `{k}` (a string)")),
    ))
}

/// 一段值的记号（看过的那一份 · 应用时再算一次比）。
pub(crate) fn token_of(v: &Value) -> String {
    super::skill_ledger::digest_of(&asset_catalog::canonical(v))
}

/// 备份落点：`<家>/.cc-monitor/backups/<毫秒>-<种类>-<名字>`（名字里别的字符换成 `_`）。回（家下相对段, 绝对路径）。
fn backup_slot(home: &Path, kind: &str, name: &str, now_ms: u128) -> (String, String) {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let rel = format!(
        "{}/{now_ms}-{kind}-{safe}",
        relay_route_core::EXT_BACKUPS_DIR_REL
    );
    let abs = door::join_under(&home.display().to_string(), &rel);
    (rel, abs)
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

// ───────────────────────── 卸（在被卸的那一台上） ─────────────────────────

/// 一个 skill 的卸法：装记录里有 ⇒ 那一份逐文件判（`seen` 当 CAS 期望）；没有 ⇒ 整个目录的摘要当记号。
enum SkillPlan {
    Recorded { plan: Value },
    Foreign { digest: String, files: Vec<String> },
}

fn skill_plan(env: &Env, name: &str, dir: &Path) -> Result<SkillPlan, (&'static str, String)> {
    let ledger = need(env.ledger.clone())?;
    let key = dir.display().to_string();
    let recorded = super::skill_ledger::load_at(&ledger)?
        .installs
        .contains_key(&key);
    if recorded {
        let plan = super::skill_install::answer_uninstall_plan_at(&ledger, &json!({ "dir": key }))?;
        return Ok(SkillPlan::Recorded { plan });
    }
    let files: Vec<String> = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .min_depth(1)
        .sort_by_file_name()
        .into_iter()
        .flatten()
        .filter(|e| !e.file_type().is_dir())
        .map(|e| {
            e.path()
                .strip_prefix(dir)
                .unwrap_or(e.path())
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    Ok(SkillPlan::Foreign {
        digest: asset_catalog::skill_asset(None, name, dir, None).digest,
        files,
    })
}

fn skill_dir_of(env: &Env, name: &str, at: &ExtLoc) -> Result<PathBuf, (&'static str, String)> {
    if !super::skill_install::valid_name(name) {
        return Err((
            "bad_args",
            copy_text("beSkillInstall.read.badName", &[("name", name)]),
        ));
    }
    let dir = need(env.skill_root(at))?.join(name);
    if !dir.is_dir() {
        return Err((
            "not_found",
            copy_text(
                "beExt.uninstall.notThere",
                &[("path", &dir.display().to_string())],
            ),
        ));
    }
    Ok(dir)
}

/// MCP 那一条在这台上：（配置文件, 原文, 那一条, 装记录里记的摘要）。
struct McpHere {
    root: String,
    path: String,
    text: String,
    def: Value,
    recorded: Option<String>,
}

fn mcp_here(
    d: &dyn Door,
    env: &Env,
    name: &str,
    at: &ExtLoc,
) -> Result<McpHere, (&'static str, String)> {
    let Some(dir) = at.project() else {
        return Err(("refused", copy_text("beExt.note.userMcpReadOnly", &[])));
    };
    let got = door::peek(d, dir, super::mcp_edit::MCP_JSON).map_err(|m| ("refused", m))?;
    let text = got.text.ok_or((
        "not_found",
        copy_text("beExt.uninstall.notThere", &[("path", &got.path)]),
    ))?;
    let servers = super::mcp_sync::servers_of(
        Some(&text),
        &copy_text("beMcpSync.answerWith.targetSide", &[]),
    )?;
    let def = servers.get(name).cloned().ok_or((
        "not_found",
        copy_text(
            "beExt.uninstall.noEntry",
            &[("name", name), ("path", &got.path)],
        ),
    ))?;
    let ledger = need(env.ledger.clone())?;
    let recorded = super::skill_ledger::load_at(&ledger)?
        .mcp
        .get(&got.path)
        .and_then(|m| m.get(name))
        .cloned();
    Ok(McpHere {
        root: dir.to_string(),
        path: got.path,
        text,
        def,
        recorded,
    })
}

/// `ext-uninstall-preview {kind, name, at}`：卸之前那张卡（只读）。
pub(crate) fn answer_uninstall_preview(d: &dyn Door, env: &Env, args: &Value) -> Answer {
    let kind = ExtKind::from_arg(args)?;
    let name = str_arg(args, "name")?;
    let at = ExtLoc::from_arg(args.get("at"), "at")?;
    let home = need(env.home.clone())?;
    let card = match kind {
        ExtKind::Skill => {
            let dir = skill_dir_of(env, name, &at)?;
            match skill_plan(env, name, &dir)? {
                SkillPlan::Recorded { plan } => {
                    let rows = plan["rows"].as_array().cloned().unwrap_or_default();
                    let files: Vec<String> = rows
                        .iter()
                        .filter(|r| r["deletable"] == true)
                        .filter_map(|r| r["path"].as_str().map(str::to_string))
                        .collect();
                    let ask = rows.iter().any(|r| r["ask"] == true);
                    ExtUninstallCard {
                        kind,
                        name: name.to_string(),
                        path: dir.display().to_string(),
                        recorded: true,
                        said: if ask {
                            copy_text(
                                "beExt.uninstall.ownedAsk",
                                &[("n", &files.len().to_string())],
                            )
                        } else {
                            copy_text("beExt.uninstall.owned", &[("n", &files.len().to_string())])
                        },
                        files,
                        backup: None,
                        token: token_of(&json!({ "rows": plan["rows"], "seen": plan["seen"] })),
                    }
                }
                SkillPlan::Foreign { digest, files } => {
                    let (_, abs) = backup_slot(&home, KIND_SKILL, name, 0);
                    let at_dir = Path::new(&abs)
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or(abs);
                    ExtUninstallCard {
                        kind,
                        name: name.to_string(),
                        path: dir.display().to_string(),
                        recorded: false,
                        said: copy_text("beExt.uninstall.foreign", &[("backup", &at_dir)]),
                        files,
                        backup: Some(at_dir),
                        token: digest,
                    }
                }
            }
        }
        ExtKind::Mcp => {
            let m = mcp_here(d, env, name, &at)?;
            let intact = m.recorded.as_deref() == Some(&super::skill_ledger::mcp_digest(&m.def));
            let (said, backup) = match (&m.recorded, intact) {
                (Some(_), true) => (copy_text("beExt.uninstall.mcpOwned", &[]), None),
                (Some(_), false) => (copy_text("beExt.uninstall.mcpModified", &[]), None),
                (None, _) => {
                    let (_, abs) = backup_slot(&home, KIND_MCP, name, 0);
                    let at_dir = Path::new(&abs)
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or(abs);
                    (
                        copy_text("beExt.uninstall.foreign", &[("backup", &at_dir)]),
                        Some(at_dir),
                    )
                }
            };
            ExtUninstallCard {
                kind,
                name: name.to_string(),
                path: m.path.clone(),
                recorded: m.recorded.is_some(),
                files: vec![name.to_string()],
                backup,
                said,
                token: super::skill_ledger::digest_of(&m.text),
            }
        }
    };
    serde_json::to_value(card).map_err(|e| ("io_failed", e.to_string()))
}

fn stale() -> (&'static str, String) {
    ("stale", copy_text("beExt.uninstall.changed", &[]))
}

/// `ext-uninstall-apply {kind, name, at, token}`：看卡时那一份（`token`）与现在不同 ⇒ `stale`、一个字节不动。
/// 用户在卡上点了确认 = 同意（装完改过的 · 装时盖掉原有的 · 不是装的，卡上都说过）。
pub(crate) fn answer_uninstall_apply(
    d: &dyn Door,
    env: &Env,
    record: Record,
    args: &Value,
) -> Answer {
    let kind = ExtKind::from_arg(args)?;
    let name = str_arg(args, "name")?;
    let at = ExtLoc::from_arg(args.get("at"), "at")?;
    let token = str_arg(args, "token")?;
    let home = need(env.home.clone())?;
    let done = match kind {
        ExtKind::Skill => {
            let dir = skill_dir_of(env, name, &at)?;
            match skill_plan(env, name, &dir)? {
                SkillPlan::Recorded { plan } => {
                    if token_of(&json!({ "rows": plan["rows"], "seen": plan["seen"] })) != token {
                        return Err(stale());
                    }
                    let rows = plan["rows"].as_array().cloned().unwrap_or_default();
                    let pick = |k: &str| -> Vec<Value> {
                        rows.iter()
                            .filter(|r| r["deletable"] == true && (k == "take" || r["ask"] == true))
                            .map(|r| r["path"].clone())
                            .collect()
                    };
                    let ledger = need(env.ledger.clone())?;
                    let out = super::skill_flow::answer_uninstall(
                        d,
                        &ledger,
                        record,
                        &json!({ "dir": dir.display().to_string(), "seen": plan["seen"], "take": pick("take"), "confirm": pick("confirm") }),
                    )?;
                    let changed: Vec<String> = out["deleted"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|p| p.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    let note = [&out["recordFailed"], &out["dirFailed"]]
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect::<Vec<_>>();
                    ExtDone {
                        path: dir.display().to_string(),
                        changed,
                        note: (!note.is_empty()).then(|| note.join(" ")),
                    }
                }
                SkillPlan::Foreign { digest, files } => {
                    if digest != token {
                        return Err(stale());
                    }
                    let home_s = home.display().to_string();
                    let from =
                        door::rel_under(&home_s, &dir.display().to_string()).map_err(|_| {
                            (
                                "refused",
                                copy_text(
                                    "beExt.uninstall.backupOutsideHome",
                                    &[("path", &dir.display().to_string())],
                                ),
                            )
                        })?;
                    let (rel, abs) = backup_slot(&home, KIND_SKILL, name, now_ms());
                    ensure_backups_dir(d, &home_s)?;
                    door::rename(d, &home_s, &from, &rel).map_err(|e| {
                        (
                            "refused",
                            copy_text("beExt.uninstall.backupFailed", &[("e", &e)]),
                        )
                    })?;
                    ExtDone {
                        path: dir.display().to_string(),
                        changed: files,
                        note: Some(copy_text("beExt.uninstall.movedTo", &[("path", &abs)])),
                    }
                }
            }
        }
        ExtKind::Mcp => {
            let m = mcp_here(d, env, name, &at)?;
            if super::skill_ledger::digest_of(&m.text) != token {
                return Err(stale());
            }
            let mut note = None;
            if m.recorded.is_none() {
                let (rel, abs) = backup_slot(&home, KIND_MCP, name, now_ms());
                let home_s = home.display().to_string();
                door::put(
                    d,
                    &home_s,
                    &format!("{rel}/{}", super::mcp_edit::MCP_JSON),
                    &m.text,
                    None,
                    false,
                    true,
                )
                .map_err(|e| {
                    (
                        "refused",
                        copy_text("beExt.uninstall.backupFailed", &[("e", &e.said())]),
                    )
                })?;
                note = Some(copy_text("beExt.uninstall.copiedTo", &[("path", &abs)]));
            }
            let next = super::mcp_edit::plan_project_mcp(&m.path, Some(&m.text), &mut |v| {
                super::mcp_edit::remove_mcp_server_value(v, name)
            })
            .map_err(|e| ("refused", e))?;
            if let Some(next) = next {
                match door::put(
                    d,
                    &m.root,
                    super::mcp_edit::MCP_JSON,
                    &next,
                    Some(&m.text),
                    false,
                    false,
                ) {
                    Ok(_) => {}
                    Err(Refused::Stale(_)) => return Err(stale()),
                    Err(e) => return Err(("refused", e.said())),
                }
            }
            if m.recorded.is_some() {
                if let Err((_, e)) =
                    record(&json!({ "op": "mcp-drop", "file": m.path, "name": name }))
                {
                    note = Some(copy_text("beExt.uninstall.dropFailed", &[("e", &e)]));
                }
            }
            ExtDone {
                path: m.path,
                changed: vec![name.to_string()],
                note,
            }
        }
    };
    serde_json::to_value(done).map_err(|e| ("io_failed", e.to_string()))
}

/// `~/.cc-monitor/backups` 不在就建（它的上一层是后端自己的家，一定在）。
fn ensure_backups_dir(d: &dyn Door, home: &str) -> Result<(), (&'static str, String)> {
    let abs = door::join_under(home, relay_route_core::EXT_BACKUPS_DIR_REL);
    match door::stat_kind(d, &abs) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => door::mkdir(d, home, relay_route_core::EXT_BACKUPS_DIR_REL).map_err(|e| {
            (
                "refused",
                copy_text("beExt.uninstall.backupFailed", &[("e", &e)]),
            )
        }),
        Err(e) => Err((
            "refused",
            copy_text("beExt.uninstall.backupFailed", &[("e", &e)]),
        )),
    }
}

// ───────────────────────── 可疑项的那句话（枢纽拼卡时用） ─────────────────────────

/// 一条可疑项 → 那句话（`machine` = 被写那台给人看的名字）。种类闭集：MCP 与 skill 两边的并集。
pub(crate) fn suspect_said(s: &Value, machine: &str) -> String {
    let get = |k: &str| s.get(k).and_then(Value::as_str).unwrap_or_default();
    let (field, value, there) = (get("field"), get("value"), get("there"));
    match (get("kind"), there) {
        ("abs-path", "present") => copy_text(
            "beExt.suspect.absPresent",
            &[("field", field), ("value", value), ("machine", machine)],
        ),
        ("abs-path", "absent") => copy_text(
            "beExt.suspect.absAbsent",
            &[("field", field), ("value", value), ("machine", machine)],
        ),
        ("abs-path", "foreign") => copy_text(
            "beExt.suspect.absForeign",
            &[("field", field), ("value", value), ("machine", machine)],
        ),
        ("abs-path", _) => copy_text(
            "beExt.suspect.absUnknown",
            &[("field", field), ("value", value), ("machine", machine)],
        ),
        ("command-missing", "unknown") => copy_text(
            "beExt.suspect.commandUnknown",
            &[("value", value), ("machine", machine)],
        ),
        ("command-missing", _) => copy_text(
            "beExt.suspect.commandMissing",
            &[("value", value), ("machine", machine)],
        ),
        ("command-relative", _) => copy_text(
            "beExt.suspect.commandRelative",
            &[("value", value), ("machine", machine)],
        ),
        ("executable", _) => copy_text("beExt.suspect.executable", &[]),
        ("binary", _) => copy_text("beExt.suspect.binary", &[]),
        _ => copy_text("beExt.suspect.other", &[("field", field), ("value", value)]),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/ext_tests.rs"]
mod tests;
