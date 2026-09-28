//! 〔RM1a · 第四波〕「足迹」的**远端那一栏**：问那台机器的后端要路径事实，判定走本机同一份 `build_rows`。
//! 〔C5 · TAIL〕本机那一栏也走这里（[`local_report_of`]）：事实问本机后端，只有 `HostScope::Client`
//! 那一族（monitor 自己那台的东西）由 monitor 进程自己查（`config_surface::with_monitor_probe`）。
//!
//! # 形状
//!
//! 用户裁「远端机器页『足迹』栏：补后端读口，远端也有真栏」。判定（哪一行属于哪个工具、存在 / 缺失 /
//! 查不动怎么分）**只住 `config_surface.rs` 一处**；本模块只把那台的事实喂进去：
//!
//! 1. 空问一趟 `footprint-probe` ⇒ 那台后端进程看到的 `HOME` · `PATH` · agent 家目录（解 `~/…` 要用）；
//! 2. 用一个**记账的探针**把 `build_rows` / `build_settings_scopes` 跑一遍（探针一律答「不在」），
//!    记下它们想问的每一条路径（`meta` / `list` / 钩子字样）；
//! 3. 把那些路径**一趟**问完；
//! 4. 用**答题的探针**（查第 3 步的答案）再跑一遍 —— 那一份就是报告。
//!
//! 第 2 步里探针答「不在」会让「是目录才去列」那一支走不到 —— 那不漏：后端对每个目录**顺带**交一层文件名。
//! 第 4 步若问到一条第 3 步没问过的路径 ⇒ 按「不在」答不许发生：[`Answers`] 对没问过的路径答 `None`
//! 与「查了、不在」同形，所以第 2、4 两步必须走**同一份**判定、同一组输入（判据
//! `the_second_pass_asks_nothing_the_first_pass_did_not_record` 钉着）。
//!
//! # 远端路径的分隔符
//!
//! 判定那一侧用 `PathBuf::join` 拼路径 —— monitor 跑在 Windows 上时拼出来的是 `\`。远端是 POSIX，
//! ⇒ 过线前（与查答案时）把**本机分隔符** `\` 换成 `/`（[`wire_path`]）。只在分隔符本来就是 `\` 的平台换：
//! Linux / macOS 上的 monitor 拼不出 `\`，那里出现的 `\` 只能是远端文件名里真带的那个，原样过线
//! 〔W5-UI · `设计/70 §10 #6`〕。⚠ 余下的代价：**Windows 上的 monitor** 仍分不开「拼进来的 `\`」与「名字里的 `\`」
//! （POSIX 允许、极少见）—— 要判定那一侧不用本机 `PathBuf` 拼远端路径，如实登记。
//! `PATH` 同理：远端按 `:` 切，再按本机规矩合回去，`resolves_on_path` 才切得对。

use crate::backend::control::backend_route::{no_channel, route_call_error, Routed};
use crate::backend::control::inbound_client;
use crate::config_surface::{
    build_rows, build_settings_scopes, with_monitor_probe, ConfigSurfaceReport, FsProbe,
    SurfaceEnv, Vantage, HOOK_PROGRAMS,
};
use crate::copy_table::copy_text;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// 后端那条命令的名字（与 `src/backend/inbound.rs::REGISTRY` 同名，跨半边由判据现抠对拍）。
pub(crate) const CMD: &str = "footprint-probe";

/// 一趟往返的上限。
const BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

/// 过线的路径形：本机分隔符是 `\` 时换成 `/`；别的平台原样（见模块头注）。
pub(crate) fn wire_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    if std::path::MAIN_SEPARATOR == '\\' {
        s.replace('\\', "/")
    } else {
        s.into_owned()
    }
}

/// 那台后端进程看到的环境。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteEnv {
    pub(crate) home: PathBuf,
    /// 本机规矩合好的 `PATH`（远端按 `:` 切的）。`None` = 取不到 / 合不回去 ⇒ 那一族「查不动」。
    pub(crate) path: Option<String>,
    pub(crate) agent_home: PathBuf,
    pub(crate) agent_home_is_dir: bool,
}

/// 第 3 步的答案：路径 → `(是否目录, 大小)` · 目录 → 一层文件名 · 文件 → 有没有钩子字样。
#[derive(Debug, Default, Clone)]
pub(crate) struct Answers {
    pub(crate) meta: BTreeMap<String, (bool, u64)>,
    pub(crate) list: BTreeMap<String, Vec<String>>,
    pub(crate) hooks: BTreeMap<String, bool>,
}

/// 第 2 步记下的问题。
#[derive(Debug, Default)]
pub(crate) struct Asked {
    pub(crate) stat: BTreeSet<String>,
    pub(crate) hooks: BTreeSet<String>,
}

/// 那台机器的报告。
pub(crate) async fn report_of(host: &str) -> Result<ConfigSurfaceReport, String> {
    let env = env_from_wire(host, &call(host, json!({})).await?)?;
    let asked = ask(&env);
    let answers = answers_from_wire(host, &call(host, stat_args(&asked)).await?)?;
    Ok(ConfigSurfaceReport {
        origin: crate::origin::Origin(host.to_string()),
        ..build(&env, &answers).0
    })
}

/// 〔C5〕本机那一栏：同一套两趟问法问本机后端；`HostScope::Client` 那一族由 monitor 自己的探针答。
/// 本机后端与 monitor 同一个平台 ⇒ `PATH` 原样用、路径不换分隔符。
pub(crate) async fn local_report_of() -> Result<ConfigSurfaceReport, String> {
    let host = inbound_client::LOCAL_ORIGIN;
    let env = local_env_from_wire(host, &call(host, json!({})).await?)?;
    let env2 = env.clone();
    let asked = tokio::task::spawn_blocking(move || {
        with_monitor_probe(|m| {
            let (asked, _) = run_recording(&env2, None, Some(m));
            asked
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))??;
    let answers = answers_from_wire(host, &call(host, stat_args(&asked)).await?)?;
    tokio::task::spawn_blocking(move || {
        with_monitor_probe(|m| ConfigSurfaceReport {
            origin: crate::origin::Origin::local(),
            ..run_recording(&env, Some(&answers), Some(m)).1
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?
}

fn stat_args(asked: &Asked) -> Value {
    json!({
        "stat": asked.stat.iter().collect::<Vec<_>>(),
        "hooks": { "paths": asked.hooks.iter().collect::<Vec<_>>(), "needles": HOOK_PROGRAMS },
    })
}

/// 第 2 步：记账的探针（一律答「不在」）跑一遍，记下想问的路径。
pub(crate) fn ask(env: &RemoteEnv) -> Asked {
    run_recording(env, None, None).0
}

/// 第 4 步：答题的探针跑一遍。第二项是这一趟**问到**的路径（给判据核「没问过的一条都没有」）。
pub(crate) fn build(env: &RemoteEnv, a: &Answers) -> (ConfigSurfaceReport, Asked) {
    let (asked, report) = run_recording(env, Some(a), None);
    (report, asked)
}

/// 两步共用：`answers` 为 `None` ＝ 第 2 步（一律答「不在」）；`client` 有 ＝ 本机那一栏（见 [`local_report_of`]）。
fn run_recording(
    env: &RemoteEnv,
    answers: Option<&Answers>,
    client: Option<&SurfaceEnv>,
) -> (Asked, ConfigSurfaceReport) {
    let asked = RefCell::new(Asked::default());
    let meta = |p: &Path| {
        let k = wire_path(p);
        asked.borrow_mut().stat.insert(k.clone());
        answers.and_then(|a| a.meta.get(&k).copied())
    };
    let list = |p: &Path| {
        let k = wire_path(p);
        asked.borrow_mut().stat.insert(k.clone());
        answers.and_then(|a| a.list.get(&k).cloned())
    };
    let hooks = |p: &Path| {
        let k = wire_path(p);
        asked.borrow_mut().hooks.insert(k.clone());
        answers.and_then(|a| a.hooks.get(&k).copied())
    };
    let report = run(env, &meta, &list, &hooks, client);
    (asked.into_inner(), report)
}

/// 两步共用的那一趟：**同一份判定、同一组输入**，只有探针不同。
fn run(
    env: &RemoteEnv,
    meta: &dyn Fn(&Path) -> Option<(bool, u64)>,
    list: &dyn Fn(&Path) -> Option<Vec<String>>,
    hooks: &dyn Fn(&Path) -> Option<bool>,
    client: Option<&SurfaceEnv>,
) -> ConfigSurfaceReport {
    // 本机那一栏：视角是 monitor 那台（远端落点照旧「本页不连 SSH」），路径原样不换分隔符。
    let local = client.is_some();
    let agent_home = env.agent_home.clone();
    let agent_home_is_dir = env.agent_home_is_dir;
    let is_dir = move |p: &Path| agent_home_is_dir && wire_path(p) == wire_path(&agent_home);
    let fs = FsProbe { meta, list };
    let surface_env = SurfaceEnv {
        home: &env.home,
        cfg_dir_env: Some(&env.agent_home),
        is_dir: &is_dir,
        fs: &fs,
        path_env: env.path.as_deref(),
        vantage: if local {
            Vantage::Monitor
        } else {
            Vantage::Remote
        },
    };
    let cfg_dir =
        crate::config_surface::claude_config_dir(Some(&env.agent_home), &env.home, &is_dir);
    let mut rows = build_rows(&surface_env, client);
    let mut settings_scopes =
        build_settings_scopes(&env.home, Some(&env.agent_home), &is_dir, hooks, &fs);
    let shown = |p: &Path| {
        if local {
            p.to_string_lossy().into_owned()
        } else {
            wire_path(p)
        }
    };
    if !local {
        for r in &mut rows {
            if let Some(p) = r.path_resolved.as_mut() {
                *p = p.replace('\\', "/");
            }
        }
        for s in &mut settings_scopes {
            s.path = s.path.replace('\\', "/");
        }
    }
    ConfigSurfaceReport {
        rows,
        settings_scopes,
        claude_config_dir: shown(&cfg_dir),
        home: shown(&env.home),
        // 由 [`report_of`] 填实（那台的名字）；这一趟单跑（判据）时它就是一个空名。
        origin: crate::origin::Origin(String::new()),
    }
}

/// 第 1 步的应答 → 环境。`home` 缺 ⇒ 报错（没有家目录就解不了任何 `~/…`，**不猜**）。
pub(crate) fn env_from_wire(host: &str, d: &Value) -> Result<RemoteEnv, String> {
    env_from_wire_as(host, d, native_path_list)
}

/// 〔C5〕本机后端的应答：与 monitor 同一个平台 ⇒ `PATH` 原样（不按 `:` 切）。
fn local_env_from_wire(host: &str, d: &Value) -> Result<RemoteEnv, String> {
    env_from_wire_as(host, d, |p| Some(p.to_string()))
}

fn env_from_wire_as(
    host: &str,
    d: &Value,
    path_list: fn(&str) -> Option<String>,
) -> Result<RemoteEnv, String> {
    let e = d.get("env").ok_or_else(|| {
        copy_text(
            "rsFootprintRemote.wire.noEnv",
            &[("host", &host.to_string())],
        )
    })?;
    let s = |k: &str| e.get(k).and_then(Value::as_str).map(str::to_string);
    let home = s("home").ok_or_else(|| {
        copy_text(
            "rsFootprintRemote.wire.noHome",
            &[("host", &host.to_string())],
        )
    })?;
    let agent_home = s("agentHome").ok_or_else(|| {
        copy_text(
            "rsFootprintRemote.wire.noAgentHome",
            &[("host", &host.to_string())],
        )
    })?;
    let agent_home_is_dir = e
        .get("agentHomeIsDir")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            copy_text(
                "rsFootprintRemote.wire.noAgentHomeIsDir",
                &[("host", &host.to_string())],
            )
        })?;
    Ok(RemoteEnv {
        home: PathBuf::from(home),
        path: s("path").and_then(|p| path_list(&p)),
        agent_home: PathBuf::from(agent_home),
        agent_home_is_dir,
    })
}

/// 远端（POSIX）那条 `PATH` 按 `:` 切、按**本机**规矩合回去 —— `resolves_on_path` 用本机的 `split_paths` 切它。
/// 合不回去（某一段里带着本机的分隔符）⇒ `None`（那一族「查不动」，不是「没有」）。
pub(crate) fn native_path_list(remote: &str) -> Option<String> {
    let parts: Vec<&str> = remote.split(':').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }
    std::env::join_paths(parts)
        .ok()
        .map(|s| s.to_string_lossy().into_owned())
}

/// 第 3 步的应答 → 答案。`null` 那几格（不在 / 读不动）不进表 ⇒ 探针答 `None`。
pub(crate) fn answers_from_wire(host: &str, d: &Value) -> Result<Answers, String> {
    let bad = |what: &str| {
        copy_text(
            "rsFootprintRemote.wire.badShape",
            &[("host", &host.to_string()), ("what", &what.to_string())],
        )
    };
    let mut a = Answers::default();
    let stat = d
        .get("stat")
        .and_then(Value::as_object)
        .ok_or_else(|| bad(&copy_text("rsFootprintRemote.wire.statNotObject", &[])))?;
    for (p, v) in stat {
        if v.is_null() {
            continue;
        }
        let kind = v
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| bad(&copy_text("rsFootprintRemote.wire.kindMissing", &[])))?;
        let size = v.get("size").and_then(Value::as_u64).unwrap_or(0);
        let is_dir = match kind {
            "dir" => true,
            "file" => false,
            other => {
                return Err(bad(&copy_text(
                    "rsFootprintRemote.wire.kindOther",
                    &[("other", &format!("{:?}", other))],
                )))
            }
        };
        a.meta.insert(p.clone(), (is_dir, size));
        if let Some(names) = v.get("entries").and_then(Value::as_array) {
            let names: Option<Vec<String>> = names
                .iter()
                .map(|n| n.as_str().map(str::to_string))
                .collect();
            a.list.insert(
                p.clone(),
                names
                    .ok_or_else(|| bad(&copy_text("rsFootprintRemote.wire.entryNotString", &[])))?,
            );
        }
    }
    let hooks = d
        .get("hooks")
        .and_then(Value::as_object)
        .ok_or_else(|| bad(&copy_text("rsFootprintRemote.wire.hooksNotObject", &[])))?;
    for (p, v) in hooks {
        if let Some(b) = v.as_bool() {
            a.hooks.insert(p.clone(), b);
        }
    }
    Ok(a)
}

/// 发送口（形状照 `apikey_remote::call`）。
async fn call(host: &str, args: Value) -> Result<Value, String> {
    let Some(client) = inbound_client::client_for(host) else {
        return Err(said(no_channel(host)));
    };
    if !client.accepts(CMD) {
        return Err(copy_text(
            "rsFootprintRemote.call.tooOld",
            &[("host", &host.to_string())],
        ));
    }
    let data = client.call(CMD, args, BUDGET).await.map_err(|e| {
        said(route_call_error(&e, |_code, message| {
            copy_text(
                "rsFootprintRemote.call.failed",
                &[
                    ("host", &host.to_string()),
                    ("message", &message.to_string()),
                ],
            )
        }))
    })?;
    data.ok_or_else(|| {
        copy_text(
            "rsFootprintRemote.call.noData",
            &[("host", &host.to_string())],
        )
    })
}

/// 三态里给人看的那句话。`Done` 在本族走不到。
fn said(r: Routed) -> String {
    match r {
        Routed::NoChannel(s) | Routed::Refused(s) => s,
        Routed::Done => copy_text("rsFootprintRemote.call.internal", &[]),
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/footprint_remote_tests.rs"]
mod tests;
