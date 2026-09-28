//! 〔MIG-3b 续 · 主会话 09-28 裁①〕「足迹」**由那台后端出整份成品**：帧面 `footprint-report` —— 只读。
//!
//! 用户 09-27「一处后端」压过 `设计/96 §4`「「哪一行属于哪个工具」仍只住 monitor 的 `tool_registry`」⇒ 申报表（[`registry`]）
//! 与判定（[`rows`]：哪一行属于哪个工具、存在 / 缺失 / 查不动怎么分）都住后端，这台 stat 这台自己的盘。
//! 〔墓碑 —— RM1a 那一版这里是 `footprint-probe`：只交路径事实，判定住 monitor `config_surface.rs`，monitor 问两趟。〕
//!
//! # 两种问法
//!
//! - **远端那一栏**（`{}`）：一问即得。视角 [`rows::Vantage::Remote`]：住 monitor 那台的那一族（`HostScope::Client`）不进人群。
//! - **本机那一栏**（`{client: {env, stat?}}`）：视角 [`rows::Vantage::Monitor`]。`HostScope::Client` 那一族的**事实**
//!   照旧由 monitor 答（`05 §14.3` E 组「足迹里 monitor 自己那几行」），判定仍在这里：
//!   没给 `stat` ⇒ 只回那一族要 stat 哪些路径（`clientAsks`，`report: null`）；界面拿去问 monitor，
//!   再带着 `stat` 问一次 ⇒ 整份报告。第二趟问到第一趟没记下的路径 ⇒ `bad_args`（没问过的不许当「不在」答）。
//!
//! # 上限
//!
//! 目录最多列 [`MAX_ENTRIES`] 个名字（超了 ⇒ 列不动，**不截断**：截断的清单会被当成完整的去数 glob）·
//! 查钩子字样的文件最多 [`MAX_HOOK_FILE_BYTES`] 字节（内容一个字节都不回）· `client.stat` 最多 [`MAX_CLIENT_PATHS`] 条。

pub(crate) mod registry;
pub(crate) mod rows;

use copy_core::copy_text;
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rows::{build_rows, build_settings_scopes, ConfigSurfaceReport, FsProbe, SurfaceEnv, Vantage};

/// 一个目录最多交几个名字。
pub(crate) const MAX_ENTRIES: usize = 4096;
/// 查字样的文件最大多少字节（`settings.json` 那一类，远小于它）。
pub(crate) const MAX_HOOK_FILE_BYTES: u64 = 1 << 20;
/// monitor 交来的 `client.stat` 最多几条。
pub(crate) const MAX_CLIENT_PATHS: usize = 1024;

/// 本族的应答。
pub(crate) type FootprintAnswer = Result<Value, (&'static str, String)>;

/// 帧面入口。
pub(crate) fn answer(args: &Value) -> FootprintAnswer {
    let get = |k: &str| std::env::var(k).ok();
    answer_with(&get, &crate::observe::history_query::agent_home(), args)
}

/// monitor 交来的它自己那台的事实。
struct Client {
    home: PathBuf,
    agent_home: PathBuf,
    path: Option<String>,
    /// `None` = 第一趟（只要 `clientAsks`）。
    stat: Option<ClientStat>,
}

/// `client.stat` 解出来的样子：路径 → `(是否目录, 大小)` · 目录 → 一层名字；答 `null` 的也记着（问过、不在）。
#[derive(Default)]
struct ClientStat {
    asked: BTreeSet<String>,
    meta: BTreeMap<String, (bool, u64)>,
    list: BTreeMap<String, Vec<String>>,
}

/// [`answer`] 的本体：环境取值器与 agent 家目录都是参数（判据拿夹具喂，不去改进程环境）。
pub(crate) fn answer_with(
    get: &dyn Fn(&str) -> Option<String>,
    agent_home: &Path,
    args: &Value,
) -> FootprintAnswer {
    let client = client_arg(args.get("client"))?;
    let home = get("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| get("USERPROFILE").filter(|h| !h.is_empty()))
        .map(PathBuf::from)
        .ok_or(("failed", copy_text("beFootprint.env.noHome", &[])))?;
    let path_env = get("PATH");
    let own_meta = |p: &Path| meta_of(p);
    let own_list = |p: &Path| list_of(p);
    let own_fs = FsProbe {
        meta: &own_meta,
        list: &own_list,
    };
    let own = SurfaceEnv {
        home: &home,
        agent_home,
        fs: &own_fs,
        path_env: path_env.as_deref(),
        vantage: if client.is_some() {
            Vantage::Monitor
        } else {
            Vantage::Remote
        },
    };
    // monitor 那一族的探针：记下问了哪些；第二趟答 monitor 交来的事实，没交过的那一条记成「没问过」。
    let asked = RefCell::new(BTreeSet::<String>::new());
    let unasked = RefCell::new(BTreeSet::<String>::new());
    let answered = |k: &str| -> bool {
        let known = client
            .as_ref()
            .and_then(|c| c.stat.as_ref())
            .is_some_and(|s| s.asked.contains(k));
        if !known {
            unasked.borrow_mut().insert(k.to_string());
        }
        known
    };
    let c_meta = |p: &Path| {
        let k = p.display().to_string();
        asked.borrow_mut().insert(k.clone());
        let st = client.as_ref()?.stat.as_ref()?;
        answered(&k).then(|| st.meta.get(&k).copied()).flatten()
    };
    let c_list = |p: &Path| {
        let k = p.display().to_string();
        asked.borrow_mut().insert(k.clone());
        let st = client.as_ref()?.stat.as_ref()?;
        answered(&k).then(|| st.list.get(&k).cloned()).flatten()
    };
    let c_fs = FsProbe {
        meta: &c_meta,
        list: &c_list,
    };
    let c_env = client.as_ref().map(|c| SurfaceEnv {
        home: &c.home,
        agent_home: &c.agent_home,
        fs: &c_fs,
        path_env: c.path.as_deref(),
        vantage: Vantage::Monitor,
    });
    let rows = build_rows(&own, c_env.as_ref());
    let hooks = |p: &Path| hooks_in(p);
    let settings_scopes = build_settings_scopes(agent_home, &hooks, &own_fs);
    let client_asks: Vec<String> = asked.into_inner().into_iter().collect();
    let second_pass = client.as_ref().is_some_and(|c| c.stat.is_some());
    if client.is_some() && !second_pass {
        return Ok(json!({ "report": Value::Null, "clientAsks": client_asks }));
    }
    let unasked = unasked.into_inner();
    if second_pass && !unasked.is_empty() {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`client.stat` lacks {} path(s) this pass needs (ask again without `stat`): {:?}",
                unasked.len(),
                unasked.iter().take(4).collect::<Vec<_>>()
            )),
        ));
    }
    let report = ConfigSurfaceReport {
        rows,
        settings_scopes,
        claude_config_dir: agent_home.display().to_string(),
        home: home.display().to_string(),
    };
    Ok(json!({ "report": report, "clientAsks": client_asks }))
}

/// 这台的一条路径：`(是否目录, 大小)`；不在 / 读不动 ⇒ `None`。
fn meta_of(p: &Path) -> Option<(bool, u64)> {
    let md = std::fs::metadata(p).ok()?;
    Some((md.is_dir(), if md.is_dir() { 0 } else { md.len() }))
}

/// 这台的一个目录的一层名字；列不动 / 超过 [`MAX_ENTRIES`] ⇒ `None`（≠ 空目录，不截断；那一行按「列不动」报「未确定」）。
fn list_of(p: &Path) -> Option<Vec<String>> {
    let names: Vec<String> = std::fs::read_dir(p)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .take(MAX_ENTRIES + 1)
        .collect();
    if names.len() > MAX_ENTRIES {
        tracing::warn!(
            "足迹：{} 里名字太多（超过上限），没列（按列不动报，不截断）",
            p.display()
        );
        return None;
    }
    Some(names)
}

/// 一份文件里有没有 cc-bus 钩子那两个字样。读不动 / 太大 / 不是文件 ⇒ `None`（**不猜**）。
fn hooks_in(p: &Path) -> Option<bool> {
    let md = std::fs::metadata(p).ok()?;
    if !md.is_file() || md.len() > MAX_HOOK_FILE_BYTES {
        let size = md.len();
        tracing::warn!(
            "足迹：{} 不是普通文件或太大（{size} 字节），没读（钩子字样按「不知道」报）",
            p.display()
        );
        return None;
    }
    let s = std::fs::read_to_string(p).ok()?;
    Some(rows::HOOK_PROGRAMS.iter().any(|n| s.contains(n)))
}

/// `client` 入参：`{env: {home, agentHome, path?}, stat?: {<绝对路径>: {kind, size, entries?} | null}}`。
fn client_arg(v: Option<&Value>) -> Result<Option<Client>, (&'static str, String)> {
    let bad = |m: &str| ("bad_args", crate::common::contract::malformed(m));
    let Some(c) = v.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let env = c
        .get("env")
        .and_then(Value::as_object)
        .ok_or_else(|| bad("`client.env` must be an object"))?;
    let abs = |k: &str| -> Result<PathBuf, (&'static str, String)> {
        let s = env
            .get(k)
            .and_then(Value::as_str)
            .ok_or_else(|| bad(&format!("`client.env.{k}` must be a string")))?;
        let p = PathBuf::from(s);
        if p.is_absolute() {
            Ok(p)
        } else {
            Err(bad(&format!(
                "`client.env.{k}`: {s:?} is not an absolute path"
            )))
        }
    };
    let (home, agent_home) = (abs("home")?, abs("agentHome")?);
    let path = env.get("path").and_then(Value::as_str).map(str::to_string);
    let stat = match c.get("stat") {
        None | Some(Value::Null) => None,
        Some(s) => Some(stat_arg(s)?),
    };
    Ok(Some(Client {
        home,
        agent_home,
        path,
        stat,
    }))
}

fn stat_arg(v: &Value) -> Result<ClientStat, (&'static str, String)> {
    let bad = |m: &str| ("bad_args", crate::common::contract::malformed(m));
    let m: &Map<String, Value> = v
        .as_object()
        .ok_or_else(|| bad("`client.stat` must be an object"))?;
    if m.len() > MAX_CLIENT_PATHS {
        return Err((
            "too_large",
            crate::common::contract::malformed(&format!(
                "`client.stat` has {} entries, at most {MAX_CLIENT_PATHS}",
                m.len()
            )),
        ));
    }
    let mut out = ClientStat::default();
    for (p, f) in m {
        out.asked.insert(p.clone());
        if f.is_null() {
            continue;
        }
        let is_dir = match f.get("kind").and_then(Value::as_str) {
            Some("dir") => true,
            Some("file") => false,
            _ => return Err(bad("`client.stat` entry has no `kind` of `file` / `dir`")),
        };
        let size = f.get("size").and_then(Value::as_u64).unwrap_or(0);
        out.meta.insert(p.clone(), (is_dir, size));
        if let Some(names) = f.get("entries").and_then(Value::as_array) {
            let names: Option<Vec<String>> = names
                .iter()
                .map(|n| n.as_str().map(str::to_string))
                .collect();
            out.list.insert(
                p.clone(),
                names.ok_or_else(|| bad("`client.stat` entries must be strings"))?,
            );
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/face_tests.rs"]
mod tests;
