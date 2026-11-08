//! 「足迹」由那台后端算整份（申报表 [`registry`] ＋ 判定 [`rows`]：哪一行属于哪个工具、存在 / 缺失 / 查不动怎么分），
//! 这台 stat 这台自己的盘；上线的是它重排成的那一份：帧面 `data-report`（[`data`] · [`chores`]）—— 只读。
//!
//! # 两种问法（一问出整份）
//!
//! - 远端那一栏（`{}`）：视角 [`rows::Vantage::Remote`]：住 monitor 那台的那一族（`HostScope::Client`）不进人群。
//! - 本机那一栏（`{client: {home, path?}}`）：视角 [`rows::Vantage::Monitor`]。本机后端与 monitor 同一台、同一用户 ⇒ `HostScope::Client` 那一族也由这里 stat；
//!   monitor 只交它独有的那几条事实（它自己进程的家目录 · `PATH`），那一族按它们解；agent 家用这台后端自己解析的那一个。
//!
//! # 上限
//!
//! 目录最多列 [`MAX_ENTRIES`] 个名字（超了 ⇒ 列不动，不截断：截断的清单会被当成完整的去数 glob）。

// 测试段可以直接问系统钟（量具、夹具的时刻）；生产段只许纯工具那一层的 `time::now`（`clippy.toml`）。
// 测试段收尾时丢结果不写理由；生产段丢一个 `Result` 写 `.discard(Why::…)`。
#![cfg_attr(
    test,
    allow(
        clippy::allow_attributes,
        clippy::allow_attributes_without_reason,
        clippy::disallowed_methods,
        clippy::let_underscore_must_use,
        clippy::unused_result_ok,
        reason = "测试段放开禁用名单与吞结果那两条：夹具自己定期限、读写盘、起进程、收尾时丢结果；用不用得到随判据增减，写成 `expect` 会在用不到时报 ⇒ 整包唯一许写 `allow` 的一行"
    )
)]

// 包里照旧写 `crate::footprint::…` · `crate::host::…` · `crate::assets::…` · `crate::accounts::…` ·
// `crate::agents::…` · `crate::history::…` · `crate::wire::…` · `crate::common::…` · `crate::platform::…`。
extern crate self as footprint;
use backend_accounts as accounts;
use backend_agents as agents;
use backend_assets as assets;
use backend_common as common;
use backend_history as history;
use backend_host as host;
use backend_platform as platform;
use backend_wire as wire;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本包的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

pub mod agent_home_check;
pub mod chores;
pub mod data;
pub mod last_seen;
pub mod readiness;
pub(crate) mod registry;
pub mod rows;

use crate::common::said::Said;
use crate::platform::fs::Ro;
use copy_core::copy_text;
use std::path::{Path, PathBuf};

use rows::{build_rows, ConfigSurfaceReport, FsProbe, SurfaceEnv, Vantage};

/// 一个目录最多交几个名字。
pub(crate) const MAX_ENTRIES: usize = 4096;

/// `data-report` 的入参：远端那一栏 `{}`；本机那一栏带 `client`（monitor 自己进程的那几条，原样）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct FootprintReq {
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub client: Option<FootprintClient>,
}

/// monitor 交来的它自己那台独有的事实：家目录（绝对路径）· `PATH`（取不到 ⇒ `null`）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct FootprintClient {
    pub(crate) home: String,
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub(crate) path: Option<String>,
}

/// 这台的整份足迹（[`data_answer`] 的原料）。
fn answer(req: &FootprintReq) -> Result<ConfigSurfaceReport, Said> {
    let get = |k: &str| std::env::var(k).ok();
    answer_with(
        &get,
        &crate::platform::shell::session_shell_path,
        &crate::history::listing::agent_home(),
        &user_path_here,
        req,
    )
}

/// `ccm` 所在目录（后端落点的上一级）在不在这台的用户级 `PATH` 上：只 Windows 那一台问得出（起 PowerShell ⇒ 阻塞档，
/// 只在闭集里有那一格时才问）；问不出 ⇒ `None`。
fn user_path_here() -> Option<bool> {
    let dir = crate::platform::paths::installed_ccm_dir()?;
    crate::platform::shell::powershell::user_path_holds(&dir)?.ok()
}

/// 解过的 `client`：家目录是绝对路径。
struct Client {
    home: PathBuf,
    path: Option<String>,
}

/// 「文件与数据」那一份成品（帧面 `data-report`）：同一份足迹按「改过你的文件 · 待办 · 有没有 tmux」重排（[`data`] · [`chores`]）。
/// `door` 是这台的文件管理面（别名块的候选经它读）。
pub fn data_answer(
    door: &dyn crate::common::door::Door,
    req: &FootprintReq,
) -> Result<data::DataReport, Said> {
    let report = answer(req)?;
    let todo = chores::chores(&chores::gather::facts(door, data::needs_install(&report)));
    let tmux = crate::host::tmux::tmux_here();
    let own = crate::platform::paths::home_dir()
        .map(|h| data::own_rows(&h))
        .unwrap_or_default();
    Ok(data::shape(&report, todo, tmux, own))
}

/// [`answer`] 的本体（整份足迹，两种问法共用这一份）：环境取值器、起会话那个 shell 的 `PATH` 与 agent 家目录都是参数
/// （判据拿夹具喂，不去改进程环境、不起登录 shell）。
pub(crate) fn answer_with(
    get: &dyn Fn(&str) -> Option<String>,
    session_path: &dyn Fn() -> Option<String>,
    agent_home: &Path,
    user_path: &dyn Fn() -> Option<bool>,
    req: &FootprintReq,
) -> Result<ConfigSurfaceReport, Said> {
    let client = req.client.as_ref().map(client_of).transpose()?;
    let home = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into))
        .ok_or_else(|| Said::new("failed", copy_text("beFootprint.env.noHome", &[])))?;
    let path_env = get("PATH");
    let system_root = get("SystemRoot");
    let session_path = session_path();
    let own_meta = |p: &Path| meta_of(p);
    let own_list = |p: &Path| list_of(p);
    let own_fs = FsProbe {
        meta: &own_meta,
        list: &own_list,
        user_path,
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
    Ok(ConfigSurfaceReport {
        rows: build_rows(&own, c_env.as_ref()),
        home: home.display().to_string(),
    })
}

/// 这台的一条路径：`(是否目录, 大小)`；不在 / 读不动 ⇒ `None`。
fn meta_of(p: &Path) -> Option<(bool, u64)> {
    let md = Ro::metadata(p).ok()?;
    Some((md.is_dir(), if md.is_dir() { 0 } else { md.len() }))
}

/// 这台的一个目录的一层名字；列不动 / 超过 [`MAX_ENTRIES`] ⇒ `None`（≠ 空目录，不截断；那一行按「列不动」报「未确定」）。
fn list_of(p: &Path) -> Option<Vec<String>> {
    let names: Vec<String> = Ro::read_dir(p)
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

/// `client` 入参：家目录要是绝对路径（解码器判不了的那一样）。
fn client_of(c: &FootprintClient) -> Result<Client, Said> {
    let home = PathBuf::from(&c.home);
    if !home.is_absolute() {
        return Err(Said::new(
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`client.home`: {:?} is not an absolute path",
                c.home
            )),
        ));
    }
    Ok(Client {
        home,
        path: c.path.clone(),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/face_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/footprint/small_wire_tests.rs"]
mod small_wire_tests;

#[cfg(test)]
#[path = "../../../tests/backend/footprint/report_wire_tests.rs"]
mod report_wire_tests;
