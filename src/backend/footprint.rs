//! 〔RM1a · 第四波〕**「足迹」的这台机器那一半**：帧面 `footprint-probe` —— 只读。
//!
//! # 它补的是哪一格
//!
//! 设置里「足迹」那一块（cc-monitor 在这台机器上碰过哪些文件）先前**只答本机**：
//! monitor 进程自己去 stat 自己那台的盘，远端那一族一律「本页不连 SSH」。用户裁：
//! 「补后端读口，远端也有真栏」。⇒ 远端那台的事实由那台的后端答，**判定仍只住 monitor 一处**
//! （`src/bridge/src/config_surface.rs` 的 `build_rows`：哪一行属于哪个工具、存在 / 缺失 / 查不动怎么分）。
//! 本模块只交**路径事实**，一个工具名、一条判定规则都不认识。
//!
//! # 交什么
//!
//! - `env`：这个后端进程看到的 `HOME`（没有再退 `USERPROFILE`）· `PATH` · agent 的家目录
//!   （`agentHome`，与帧面其余几条同一个出处 `observe::history_query::agent_home`）及它是不是目录。
//!   ⚠ 都是**后端进程的**环境 —— 用户交互 shell 里的 rc 可能改过它们，这里看不见（如实登记）。
//! - `stat`：一批**绝对**路径各自的 `kind`（`file` / `dir`）· 大小 · 目录的一层文件名；不在 / 读不动 ⇒ `null`。
//! - `notices`：`hooks` 里答 `null` 的那几条各自为什么（不在 / 太大 / 读不动）；`stat` 里列不动的目录自带 `notice`。
//! - `hooks`：一批文件里**有没有**给定的几个字样（布尔；读不动 ⇒ `null`）。**文件内容一个字节都不回** ——
//!   monitor 那边要的只是「有没有 cc-bus 钩子字样」，把整份 `settings.json` 搬过线没有理由。
//!
//! # 上限（一帧装得下，且不让一次问话变成一次全盘扫）
//!
//! [`MAX_PATHS`] 条路径 · 每个目录最多列 [`MAX_ENTRIES`] 个名字（超了 ⇒ 那一格 `entries: null` = 列不动，
//! **不截断**：截断的清单会被当成完整的去数 glob）· 查字样的文件最多 [`MAX_HOOK_FILE_BYTES`] 字节。
//!
//! # 它**不**做什么
//!
//! 不写盘、不起进程、不递归、不跟 glob（glob 由 monitor 拿一层文件名自己筛）。

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// 一趟最多问几条路径（`stat` 与 `hooks.paths` 各自）。
pub(crate) const MAX_PATHS: usize = 256;
/// 一个目录最多交几个名字。
pub(crate) const MAX_ENTRIES: usize = 4096;
/// 查字样的文件最大多少字节（`settings.json` 那一类，远小于它）。
pub(crate) const MAX_HOOK_FILE_BYTES: u64 = 1 << 20;
/// 一趟最多几个字样。
pub(crate) const MAX_NEEDLES: usize = 16;

/// 本族的应答。
pub(crate) type FootprintAnswer = Result<Value, (&'static str, String)>;

/// 帧面入口。
pub(crate) fn answer(args: &Value) -> FootprintAnswer {
    let get = |k: &str| std::env::var(k).ok();
    answer_with(&get, &crate::observe::history_query::agent_home(), args)
}

/// [`answer`] 的本体：环境取值器与 agent 家目录都是参数（判据拿夹具喂，不去改进程环境）。
pub(crate) fn answer_with(
    get: &dyn Fn(&str) -> Option<String>,
    agent_home: &Path,
    args: &Value,
) -> FootprintAnswer {
    let stat_paths = paths_arg(args.get("stat"), "stat")?;
    let hooks = args.get("hooks");
    let hook_paths = paths_arg(hooks.and_then(|h| h.get("paths")), "hooks.paths")?;
    let needles = needles_arg(hooks.and_then(|h| h.get("needles")))?;
    if !hook_paths.is_empty() && needles.is_empty() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`hooks.paths` given but `hooks.needles` is empty"),
        ));
    }
    let home = get("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| get("USERPROFILE").filter(|h| !h.is_empty()));
    let mut stat = Map::new();
    for p in &stat_paths {
        stat.insert(p.display().to_string(), stat_of(p));
    }
    let mut hook_hits = Map::new();
    let mut notices = Map::new();
    for p in &hook_paths {
        let (hit, notice) = hooks_in(p, &needles);
        hook_hits.insert(p.display().to_string(), hit);
        if let Some(n) = notice {
            notices.insert(p.display().to_string(), Value::String(n));
        }
    }
    Ok(json!({
        "env": {
            "home": home,
            "path": get("PATH"),
            "agentHome": agent_home.display().to_string(),
            "agentHomeIsDir": agent_home.is_dir(),
        },
        "stat": stat,
        "hooks": hook_hits,
        "notices": notices,
    }))
}

/// 一条路径的事实：不在 / 读不动 ⇒ `null`。
fn stat_of(p: &Path) -> Value {
    let Ok(md) = std::fs::metadata(p) else {
        return Value::Null;
    };
    if md.is_dir() {
        // 列不动 / 太多 ⇒ `entries: null`（≠ 空目录）。
        let Ok(it) = std::fs::read_dir(p) else {
            return json!({ "kind": "dir", "size": 0, "entries": null, "notice": "列不动（没权限？）" });
        };
        let names: Vec<String> = it
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .take(MAX_ENTRIES + 1)
            .collect();
        if names.len() > MAX_ENTRIES {
            let notice =
                format!("名字超过 {MAX_ENTRIES} 个，没列（不截断：截断的清单会被当成完整的）");
            return json!({ "kind": "dir", "size": 0, "entries": null, "notice": notice });
        }
        json!({ "kind": "dir", "size": 0, "entries": names })
    } else {
        json!({ "kind": "file", "size": md.len() })
    }
}

/// 一份文件里有没有任何一个字样。读不动 / 太大 / 不是文件 ⇒ `null`（**不猜**），并带一句为什么。
fn hooks_in(p: &Path, needles: &[String]) -> (Value, Option<String>) {
    match std::fs::metadata(p) {
        Ok(md) if !md.is_file() => return (Value::Null, Some("不是一份普通文件".into())),
        Ok(md) if md.len() > MAX_HOOK_FILE_BYTES => {
            let notice = format!("{} 字节，超过 {MAX_HOOK_FILE_BYTES}，没读", md.len());
            return (Value::Null, Some(notice));
        }
        Ok(_) => {}
        Err(e) => return (Value::Null, Some(format!("读不到：{:?}", e.kind()))),
    }
    match std::fs::read_to_string(p) {
        Ok(s) => (
            Value::Bool(needles.iter().any(|n| s.contains(n.as_str()))),
            None,
        ),
        Err(e) => (Value::Null, Some(format!("读不动：{:?}", e.kind()))),
    }
}

/// 一组绝对路径（缺席 = 空）。相对路径拒：它按后端进程的 cwd 解，那不是任何人的意思。
fn paths_arg(v: Option<&Value>, name: &str) -> Result<Vec<PathBuf>, (&'static str, String)> {
    let Some(v) = v else {
        return Ok(Vec::new());
    };
    let arr = v.as_array().ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!("`{name}` must be an array of strings")),
    ))?;
    if arr.len() > MAX_PATHS {
        return Err((
            "too_large",
            crate::common::contract::malformed(&format!(
                "`{name}` has {arr_count} entries, at most {MAX_PATHS} per call",
                arr_count = arr.len()
            )),
        ));
    }
    arr.iter()
        .map(|x| {
            let s = x.as_str().ok_or((
                "bad_args",
                crate::common::contract::malformed(&format!("`{name}` has a non-string entry")),
            ))?;
            let p = PathBuf::from(s);
            if p.is_absolute() {
                Ok(p)
            } else {
                Err((
                    "bad_args",
                    crate::common::contract::malformed(&format!(
                        "`{name}`: {s:?} is not an absolute path"
                    )),
                ))
            }
        })
        .collect()
}

/// 一组非空字样（缺席 = 空）。
fn needles_arg(v: Option<&Value>) -> Result<Vec<String>, (&'static str, String)> {
    let Some(v) = v else {
        return Ok(Vec::new());
    };
    let arr = v.as_array().ok_or((
        "bad_args",
        crate::common::contract::malformed("`hooks.needles` must be an array of strings"),
    ))?;
    if arr.len() > MAX_NEEDLES {
        return Err((
            "too_large",
            crate::common::contract::malformed(&format!(
                "`hooks.needles` has {arr_count} entries, at most {MAX_NEEDLES} per call",
                arr_count = arr.len()
            )),
        ));
    }
    arr.iter()
        .map(|x| match x.as_str() {
            Some(s) if !s.is_empty() => Ok(s.to_string()),
            _ => Err((
                "bad_args",
                crate::common::contract::malformed(
                    "`hooks.needles` has an entry that is not a non-empty string",
                ),
            )),
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/backend/footprint_tests.rs"]
mod tests;
