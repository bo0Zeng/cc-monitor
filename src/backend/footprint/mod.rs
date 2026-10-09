//! 「足迹」由那台后端出整份成品：帧面 `footprint-report` —— 只读。申报表（[`registry`]）与判定（[`rows`]：哪一行属于哪个工具、
//! 存在 / 缺失 / 查不动怎么分）都住后端，这台 stat 这台自己的盘。
//!
//! # 两种问法（一问出整份报告）
//!
//! - 远端那一栏（`{}`）：视角 [`rows::Vantage::Remote`]：住 monitor 那台的那一族（`HostScope::Client`）不进人群。
//! - 本机那一栏（`{client: {home, path?}}`）：视角 [`rows::Vantage::Monitor`]。本机后端与 monitor 同一台、同一用户 ⇒ `HostScope::Client` 那一族也由这里 stat；
//!   monitor 只交它独有的那几条事实（它自己进程的家目录 · `PATH`），那一族按它们解；agent 家用这台后端自己解析的那一个。
//!
//! # 上限
//!
//! 目录最多列 [`MAX_ENTRIES`] 个名字（超了 ⇒ 列不动，不截断：截断的清单会被当成完整的去数 glob）·
//! 查钩子字样的文件最多 [`MAX_HOOK_FILE_BYTES`] 字节（内容一个字节都不回）。

pub(crate) mod agent_home_check;
pub(crate) mod chores;
pub(crate) mod data;
pub(crate) mod last_seen;
pub(crate) mod readiness;
pub(crate) mod registry;
pub(crate) mod rows;

use copy_core::copy_text;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use rows::{build_rows, build_settings_scopes, ConfigSurfaceReport, FsProbe, SurfaceEnv, Vantage};

/// 一个目录最多交几个名字。
pub(crate) const MAX_ENTRIES: usize = 4096;
/// 查字样的文件最大多少字节（`settings.json` 那一类，远小于它）。
pub(crate) const MAX_HOOK_FILE_BYTES: u64 = 1 << 20;

/// 本族的应答。
pub(crate) type FootprintAnswer = Result<Value, (&'static str, String)>;

/// 帧面入口。
pub(crate) fn answer(args: &Value) -> FootprintAnswer {
    let get = |k: &str| std::env::var(k).ok();
    answer_with(
        &get,
        &crate::platform::shell::session_shell_path,
        &crate::observe::history_query::agent_home(),
        args,
    )
}

/// monitor 交来的它自己那台独有的事实。
struct Client {
    home: PathBuf,
    path: Option<String>,
}

/// [`answer`] 的本体：环境取值器、起会话那个 shell 的 `PATH` 与 agent 家目录都是参数（判据拿夹具喂，不去改进程环境、不起登录 shell）。
pub(crate) fn answer_with(
    get: &dyn Fn(&str) -> Option<String>,
    session_path: &dyn Fn() -> Option<String>,
    agent_home: &Path,
    args: &Value,
) -> FootprintAnswer {
    Ok(json!(report_with(get, session_path, agent_home, args)?))
}

/// 「文件与数据」那一份成品（帧面 `data-report`）：同一份足迹按「改过你的文件 · 待办 · 有没有 tmux」重排（[`data`] · [`chores`]）。
/// `door` 是这台的文件管理面（别名块的候选经它读）。
pub(crate) fn data_answer(door: &dyn crate::assets::door::Door, args: &Value) -> FootprintAnswer {
    let get = |k: &str| std::env::var(k).ok();
    let report = report_with(
        &get,
        &crate::platform::shell::session_shell_path,
        &crate::observe::history_query::agent_home(),
        args,
    )?;
    let todo = chores::chores(&chores::gather::facts(door, data::needs_install(&report)));
    let tmux = tmux_here();
    let own = crate::platform::paths::home_dir()
        .map(|h| data::own_rows(&h))
        .unwrap_or_default();
    Ok(data::shape(&report, todo, tmux, own))
}

/// 这台有没有 tmux（查不动 ⇒ `None`）：「文件与数据」那一份 · 别名页表单 · 起新会话框同一个判法（`control::terminals::rows_here`）。
pub(crate) fn tmux_here() -> Option<bool> {
    match crate::control::terminals::rows_here() {
        Ok(Some(_)) => Some(true),
        Ok(None) => Some(false),
        Err(_) => None,
    }
}

/// 整份足迹（两种问法共用这一份）。
fn report_with(
    get: &dyn Fn(&str) -> Option<String>,
    session_path: &dyn Fn() -> Option<String>,
    agent_home: &Path,
    args: &Value,
) -> Result<ConfigSurfaceReport, (&'static str, String)> {
    let client = client_arg(args.get("client"))?;
    let home = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into))
        .ok_or(("failed", copy_text("beFootprint.env.noHome", &[])))?;
    let path_env = get("PATH");
    let system_root = get("SystemRoot");
    let session_path = session_path();
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
        session_path: session_path.as_deref(),
        system_root: system_root.as_deref(),
        windows: cfg!(windows),
        vantage: if client.is_some() {
            Vantage::Monitor
        } else {
            Vantage::Remote
        },
    };
    // monitor 那一族：按 monitor 交来的环境解，stat 仍是这台自己（同一台、同一用户）。
    let c_env = client.as_ref().map(|c| SurfaceEnv {
        home: &c.home,
        agent_home,
        fs: &own_fs,
        path_env: c.path.as_deref(),
        session_path: session_path.as_deref(),
        system_root: system_root.as_deref(),
        windows: cfg!(windows),
        vantage: Vantage::Monitor,
    });
    let rows = build_rows(&own, c_env.as_ref());
    let hooks = |p: &Path| hooks_in(p);
    let settings_scopes = build_settings_scopes(agent_home, &hooks, &own_fs);
    Ok(ConfigSurfaceReport {
        rows,
        settings_scopes,
        claude_config_dir: agent_home.display().to_string(),
        home: home.display().to_string(),
    })
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

/// `client` 入参：`{home, path?}`（monitor 自己进程的那几条，原样）。
fn client_arg(v: Option<&Value>) -> Result<Option<Client>, (&'static str, String)> {
    let bad = |m: &str| ("bad_args", crate::common::contract::malformed(m));
    let Some(c) = v.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let env = c
        .as_object()
        .ok_or_else(|| bad("`client` must be an object"))?;
    let abs = |k: &str| -> Result<PathBuf, (&'static str, String)> {
        let s = env
            .get(k)
            .and_then(Value::as_str)
            .ok_or_else(|| bad(&format!("`client.{k}` must be a string")))?;
        let p = PathBuf::from(s);
        if p.is_absolute() {
            Ok(p)
        } else {
            Err(bad(&format!("`client.{k}`: {s:?} is not an absolute path")))
        }
    };
    let home = abs("home")?;
    let path = env.get("path").and_then(Value::as_str).map(str::to_string);
    Ok(Some(Client { home, path }))
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/face_tests.rs"]
mod tests;
