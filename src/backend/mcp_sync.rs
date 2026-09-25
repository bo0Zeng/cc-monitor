//! 〔AS1 · 第四波 4B〕**MCP 资产同步的判定**（`设计/96` 的 B）：帧面 `mcp-sync-plan` —— 只读。
//!
//! # 用户裁决（2026-09-24，`99 §1` V111 · V112）
//!
//! - **同步什么**：从 MCP 起步。
//! - **谁是真相源**：「各管各的，只有显式推 / 拉」—— 推 / 拉之前先给看差异，对面有不同就问盖不盖；
//!   **不做自动同步、不做冲突合并**。
//! - **内容还是引用**：「内容，原样拷过去并标出可疑项」—— 带本机绝对路径 / 对面未必有的命令的条目标出来，
//!   **不替用户改写**。
//!
//! # 为什么判定住这里（`设计/01 §1.1`「一切判定都在后端」）
//!
//! 可疑项里「对面有没有这个路径 / 这个命令」是**写入那一台机器上的事实**，只有那台机器的后端答得了。
//! ⇒ 这条命令由**要被写的那一台**的后端跑（推 ⇒ 远端那台；拉 ⇒ 本机那台）；本机远端同一条命令，只差 origin。
//! monitor 那一侧（`src/bridge/src/mcp_sync.rs`）只编排 I/O：经两台各自的后端读两份原文、把原文交到这里、
//! 按这里答的「写哪几条」去改对面那份、经那台后端 `files-put` 写（CAS）。
//!
//! # 交什么（一份判定，三件事）
//!
//! 1. **差异**：每个条目名一行，四态闭集 [`STATES`]（`new` · `same` · `differs` · `only-there`）。
//!    相等 = JSON 值相等（对象键序无关）。
//! 2. **可疑项**：只看 stdio 那四个字段（`command` · `args[i]` · `env.<键>` · `cwd`），种类闭集 [`SUSPECT_KINDS`]，
//!    每条带一格**这台机器上的事实**（[`THERE`]）。规则住 [`candidates`]（纯）与 [`judge`]（纯，事实是参数）。
//! 3. **写哪几条**（给了 `take` 才答）：`differs` 的必须在 `overwrite` 里点名，否则**整趟拒**（`needs_consent`）——
//!    「对面有不同就问盖不盖」这一问不许被界面静默跳过；`only-there` 永远不碰（没有「镜像删除」）。
//!
//! # 它**不**做什么
//!
//! 不读用户的文件（两份原文由调用方经文件管理那一面 `files-peek` 读来 —— 读的那一份与 CAS 期望的那一份
//! 必须是同一个读法）· 不写盘 · 不改写任何一条配置（写的内容由 monitor 从原文里原样取）。
//! 事实只有两种：`stat` 一条绝对路径 · 在这个后端进程的 `PATH` 上找一个名字。

use copy_core::copy_text;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;

/// 差异的四态（线上名）。**闭集**：界面文案表按它逐键给字（跨半边两向相等）。
#[cfg_attr(not(test), allow(dead_code))] // 只被判据读（后端逐态对拍 ＋ 跨半边对界面文案表），同 `inbound::CommandSpec`
pub(crate) const STATES: &[&str] = &["new", "same", "differs", "only-there"];
/// 可疑项的三种（线上名）。闭集，同上。
#[cfg_attr(not(test), allow(dead_code))] // 只被判据读（后端逐态对拍 ＋ 跨半边对界面文案表），同 `inbound::CommandSpec`
pub(crate) const SUSPECT_KINDS: &[&str] = &["abs-path", "command-missing", "command-relative"];
/// 可疑项在这台机器上的事实（线上名）。闭集，同上。`command-relative` 那一种不探，这一格是 `null`。
#[cfg_attr(not(test), allow(dead_code))] // 只被判据读（后端逐态对拍 ＋ 跨半边对界面文案表），同 `inbound::CommandSpec`
pub(crate) const THERE: &[&str] = &["present", "absent", "foreign", "unknown"];

/// 本族的应答。
pub(crate) type SyncAnswer = Result<Value, (&'static str, String)>;

/// 一个条目名在两边的样子。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    /// 拷出来的那一份有、要写进去的那一份没有。
    New,
    /// 两边都有，值相等。
    Same,
    /// 两边都有，值不等 —— 要写就得用户显式说盖。
    Differs,
    /// 只在要写进去的那一份里 —— 永远不碰。
    OnlyThere,
}

impl State {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            State::New => "new",
            State::Same => "same",
            State::Differs => "differs",
            State::OnlyThere => "only-there",
        }
    }
}

/// 一条配置里**值得看一眼**的地方（纯规则的产物，还没问过这台机器）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Candidate {
    /// 某个字段里有一条绝对路径（`field` 是 `command` / `args[i]` / `env.<键>` / `cwd`）。
    AbsPath { field: String, path: String },
    /// `command` 是一个裸名字（要靠 `PATH` 找）。
    BareCommand { name: String },
    /// `command` 带分隔符却不是绝对路径（跟着项目目录走）。
    RelativeCommand { command: String },
}

/// 一条路径 / 一个命令在这台机器上的事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum There {
    Present,
    Absent,
    /// 不是这台机器那种系统的绝对路径写法（Windows 路径到了 Linux，或反过来）。
    Foreign,
    /// 查不动（权限等）/ 这个后端进程没有 `PATH`。
    Unknown,
}

impl There {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            There::Present => "present",
            There::Absent => "absent",
            There::Foreign => "foreign",
            There::Unknown => "unknown",
        }
    }
}

/// 标出来给人看的一条。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Suspect {
    pub kind: &'static str,
    pub field: String,
    pub value: String,
    pub there: Option<There>,
}

/// 这台机器上的事实从哪来。生产是 [`Live`]；判据拿临时目录 ＋ 给定的 `PATH` 喂同一个实现。
pub(crate) trait Facts {
    /// 一条（调用方认为是绝对的）路径在这台机器上的样子。
    fn path(&self, p: &str) -> There;
    /// 一个裸名字在这个后端进程的 `PATH` 上找不找得到。`None` = 没有 `PATH` 可查。
    fn command(&self, name: &str) -> Option<bool>;
}

/// 生产那一份：真 `stat` ＋ 这个进程的 `PATH`（与 `PATHEXT`，只有 Windows 上才有这个变量）。
pub(crate) struct Live {
    pub path_var: Option<OsString>,
    pub pathext: Option<OsString>,
}

impl Live {
    pub(crate) fn from_env() -> Self {
        Live {
            path_var: std::env::var_os("PATH"),
            pathext: std::env::var_os("PATHEXT"),
        }
    }
}

impl Facts for Live {
    fn path(&self, p: &str) -> There {
        // 「是不是绝对路径」按**这台机器**的写法判：`C:\x` 在 Linux 上、`/x` 在 Windows 上都不是 ——
        // 那种路径在这里不可能存在，说成「没有」会让人以为只是没装（`foreign` 与 `absent` 分开）。
        if !Path::new(p).is_absolute() {
            return There::Foreign;
        }
        match std::fs::metadata(p) {
            Ok(_) => There::Present,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => There::Absent,
            Err(_) => There::Unknown,
        }
    }

    fn command(&self, name: &str) -> Option<bool> {
        let dirs: Vec<_> = std::env::split_paths(self.path_var.as_ref()?)
            .filter(|d| !d.as_os_str().is_empty())
            .collect();
        if dirs.is_empty() {
            return None;
        }
        // `PATHEXT` 只在 Windows 上有（`.COM;.EXE;.BAT;.CMD` …）：`npx` 在那边真装了的叫 `npx.cmd`。
        // 没有这个变量 ⇒ 只找原名。⚠ 不看执行位（只问「有没有这个名字的文件」，见模块头注「买不到」）。
        let exts: Vec<String> = self
            .pathext
            .as_ref()
            .map(|v| {
                v.to_string_lossy()
                    .split(';')
                    .filter(|e| !e.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Some(dirs.iter().any(|d| {
            d.join(name).is_file() || exts.iter().any(|e| d.join(format!("{name}{e}")).is_file())
        }))
    }
}

/// 两种系统的绝对路径写法都认：POSIX `/…` · Windows `X:\…` / `X:/…` / `\\…`。**纯**。
///
/// ⚠ 不认 `~/…`：那是「各台机器各自的家目录」，拷过去照样解得开，不是「本机绝对路径」。
pub(crate) fn is_abs_any(s: &str) -> bool {
    let b = s.as_bytes();
    s.starts_with('/')
        || s.starts_with("\\\\")
        || (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'\\' || b[2] == b'/'))
}

/// 一个字段值里的那条绝对路径：整串就是，或 `--opt=<绝对路径>` 的 `=` 右边是。**纯**。
fn abs_token(s: &str) -> Option<&str> {
    if is_abs_any(s) {
        return Some(s);
    }
    s.split_once('=').map(|(_, r)| r).filter(|r| is_abs_any(r))
}

/// 一条 server 配置里值得看一眼的地方。**纯**：只看 stdio 那四个字段（`url` / `headers` 不看）。
pub(crate) fn candidates(server: &Value) -> Vec<Candidate> {
    let mut out = Vec::new();
    let Some(obj) = server.as_object() else {
        return out;
    };
    if let Some(cmd) = obj.get("command").and_then(Value::as_str) {
        if is_abs_any(cmd) {
            out.push(Candidate::AbsPath {
                field: "command".into(),
                path: cmd.into(),
            });
        } else if cmd.contains(['/', '\\']) {
            out.push(Candidate::RelativeCommand {
                command: cmd.into(),
            });
        } else if !cmd.trim().is_empty() {
            out.push(Candidate::BareCommand { name: cmd.into() });
        }
    }
    if let Some(args) = obj.get("args").and_then(Value::as_array) {
        for (i, a) in args.iter().enumerate() {
            if let Some(p) = a.as_str().and_then(abs_token) {
                out.push(Candidate::AbsPath {
                    field: format!("args[{i}]"),
                    path: p.into(),
                });
            }
        }
    }
    if let Some(env) = obj.get("env").and_then(Value::as_object) {
        for (k, v) in env {
            if let Some(p) = v.as_str().and_then(abs_token) {
                out.push(Candidate::AbsPath {
                    field: format!("env.{k}"),
                    path: p.into(),
                });
            }
        }
    }
    if let Some(p) = obj.get("cwd").and_then(Value::as_str).and_then(abs_token) {
        out.push(Candidate::AbsPath {
            field: "cwd".into(),
            path: p.into(),
        });
    }
    out
}

/// 候选 ＋ 这台机器的事实 ⇒ 标给人看的那几条。**纯**（事实是参数）。
///
/// 裸名字在 `PATH` 上找得到 ⇒ **不标**（那是「对面有这个命令」）；其余候选一律标出（绝对路径即使对面也有，
/// 也是「带本机绝对路径」—— 用户逐字要看的就是这一族；`there` 那一格告诉他对面有没有）。
pub(crate) fn judge(cands: &[Candidate], facts: &dyn Facts) -> Vec<Suspect> {
    let mut out = Vec::new();
    for c in cands {
        match c {
            Candidate::AbsPath { field, path } => out.push(Suspect {
                kind: "abs-path",
                field: field.clone(),
                value: path.clone(),
                there: Some(facts.path(path)),
            }),
            Candidate::BareCommand { name } => {
                let there = match facts.command(name) {
                    Some(true) => continue,
                    Some(false) => There::Absent,
                    None => There::Unknown,
                };
                out.push(Suspect {
                    kind: "command-missing",
                    field: "command".into(),
                    value: name.clone(),
                    there: Some(there),
                });
            }
            Candidate::RelativeCommand { command } => out.push(Suspect {
                kind: "command-relative",
                field: "command".into(),
                value: command.clone(),
                there: None,
            }),
        }
    }
    out
}

/// 一份原文里的 `mcpServers`。`None`（那份文件不存在）⇒ 空表。
///
/// 解析不了 / 根不是对象 / `mcpServers` 不是对象 ⇒ 拒（`bad_file`）—— **不拿骨架比、更不拿骨架盖**
/// （与单条写 `mcp.rs::edit_project_mcp` 同一句承诺）。`side` 是给人看的「哪一份」。
pub(crate) fn servers_of(
    text: Option<&str>,
    side: &str,
) -> Result<Map<String, Value>, (&'static str, String)> {
    let Some(t) = text else {
        return Ok(Map::new());
    };
    let v: Value = serde_json::from_str(t.trim_start_matches('\u{feff}')).map_err(|e| {
        (
            "bad_file",
            copy_text(
                "beMcpSync.serversOf.badJson",
                &[("side", &side.to_string()), ("e", &e.to_string())],
            ),
        )
    })?;
    let root = v.as_object().ok_or((
        "bad_file",
        copy_text(
            "beMcpSync.serversOf.notObject",
            &[("side", &side.to_string())],
        ),
    ))?;
    match root.get("mcpServers") {
        None => Ok(Map::new()),
        Some(Value::Object(m)) => Ok(m.clone()),
        Some(_) => Err((
            "bad_file",
            copy_text(
                "beMcpSync.serversOf.serversNotObject",
                &[("side", &side.to_string())],
            ),
        )),
    }
}

/// 差异：每个条目名一行（按名字排序；`BTreeMap` 迭代本身有序）。**纯**。
pub(crate) fn diff(
    source: &Map<String, Value>,
    target: &Map<String, Value>,
) -> Vec<(String, State)> {
    let mut rows: Vec<(String, State)> = source
        .iter()
        .map(|(n, v)| {
            let s = match target.get(n) {
                None => State::New,
                Some(t) if t == v => State::Same,
                Some(_) => State::Differs,
            };
            (n.clone(), s)
        })
        .collect();
    rows.extend(
        target
            .keys()
            .filter(|n| !source.contains_key(*n))
            .map(|n| (n.clone(), State::OnlyThere)),
    );
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

/// 用户勾的 ⇒ 真要写的那几条。**纯**。
///
/// - `same` ⇒ 不写（没什么可写）；`new` ⇒ 写；
/// - `differs` ⇒ 必须也在 `overwrite` 里，否则**整趟拒**（`needs_consent`）—— 不静默跳过：跳过的话
///   用户以为「推过去了」，对面其实还是旧的；
/// - 不在拷出来的那一份里（含 `only-there`）⇒ `bad_args`；`overwrite` 里点了没勾的 ⇒ `bad_args`（两张单子对不上）。
pub(crate) fn plan(
    rows: &[(String, State)],
    take: &BTreeSet<String>,
    overwrite: &BTreeSet<String>,
) -> Result<Vec<String>, (&'static str, String)> {
    if let Some(n) = overwrite.iter().find(|n| !take.contains(*n)) {
        return Err((
            "bad_args",
            copy_text(
                "beMcpSync.plan.overwriteUnchecked",
                &[("n", &n.to_string())],
            ),
        ));
    }
    let state_of = |n: &str| rows.iter().find(|(r, _)| r == n).map(|(_, s)| *s);
    let mut write = Vec::new();
    for n in take {
        match state_of(n) {
            None | Some(State::OnlyThere) => {
                return Err((
                    "bad_args",
                    copy_text("beMcpSync.plan.notInSource", &[("n", &n.to_string())]),
                ))
            }
            Some(State::Same) => {}
            Some(State::New) => write.push(n.clone()),
            Some(State::Differs) if overwrite.contains(n) => write.push(n.clone()),
            Some(State::Differs) => {
                return Err((
                    "needs_consent",
                    copy_text(
                        "beMcpSync.plan.conflictUnconfirmed",
                        &[("n", &n.to_string())],
                    ),
                ))
            }
        }
    }
    Ok(write)
}

/// 一组条目名（可缺席）。给了就必须是字符串数组。〔AS2〕skill 装那一条复用它（`pub(crate)`，语义不变）。
pub(crate) fn names_arg(
    v: Option<&Value>,
    key: &str,
) -> Result<Option<BTreeSet<String>>, (&'static str, String)> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| {
                x.as_str().map(str::to_string).ok_or((
                    "bad_args",
                    crate::common::contract::malformed(&format!("`{key}` accepts strings only")),
                ))
            })
            .collect::<Result<BTreeSet<_>, _>>()
            .map(Some),
        Some(_) => Err((
            "bad_args",
            crate::common::contract::malformed(&format!("`{key}` must be an array of strings")),
        )),
    }
}

/// 帧面入口。
pub(crate) fn answer(args: &Value) -> SyncAnswer {
    answer_with(&Live::from_env(), args)
}

/// [`answer`] 的本体：事实是参数（判据拿临时目录与给定的 `PATH` 喂，不去改进程环境）。
pub(crate) fn answer_with(facts: &dyn Facts, args: &Value) -> SyncAnswer {
    let source = args.get("source").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed(
            "missing `source` (the copied text), or it is not a string",
        ),
    ))?;
    let target =
        match args.get("target") {
            None => return Err((
                "bad_args",
                crate::common::contract::malformed(
                    "missing `target` (the text to write into; `null` if that file does not exist)",
                ),
            )),
            Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.as_str()),
            Some(_) => {
                return Err((
                    "bad_args",
                    crate::common::contract::malformed("`target` must be a string or `null`"),
                ))
            }
        };
    let take = names_arg(args.get("take"), "take")?;
    let overwrite = names_arg(args.get("overwrite"), "overwrite")?;
    if take.is_none() && overwrite.is_some() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`overwrite` given without `take`"),
        ));
    }
    let src = servers_of(
        Some(source),
        &copy_text("beMcpSync.answerWith.sourceSide", &[]),
    )?;
    let tgt = servers_of(target, &copy_text("beMcpSync.answerWith.targetSide", &[]))?;
    let rows = diff(&src, &tgt);
    let rows_json: Vec<Value> = rows
        .iter()
        .map(|(name, state)| {
            // 只有会被写过去的两态才值得看可疑项（`same` 写不写都一样，`only-there` 不是这一趟拷的东西）。
            let suspects: Vec<Value> = match state {
                State::New | State::Differs => judge(&candidates(&src[name]), facts)
                    .into_iter()
                    .map(|s| {
                        json!({
                            "kind": s.kind,
                            "field": s.field,
                            "value": s.value,
                            "there": s.there.map(There::wire),
                        })
                    })
                    .collect(),
                State::Same | State::OnlyThere => Vec::new(),
            };
            json!({ "name": name, "state": state.wire(), "suspects": suspects })
        })
        .collect();
    let write = match take {
        Some(t) => Some(plan(&rows, &t, &overwrite.unwrap_or_default())?),
        None => None,
    };
    Ok(json!({ "rows": rows_json, "write": write }))
}

#[cfg(test)]
#[path = "../../tests/backend/mcp_sync_tests.rs"]
mod tests;
