//! **这台的偏好**那份文件的形状、值域与读：每一家用哪条启动命令 · 恢复时进不进 tmux · 每个号默认用哪个模型 · 上下文上限表 · pb 在哪 ·
//! 子会话 / 分身会话进不进标签页（隐藏的判定读它，`history/session_hidden.rs`）。
//! 住这台后端的家（`~/.cc-monitor/prefs.json`，[`relay_route_core::PREFS_REL`]），不住各个前端：手机直连这台，
//! 从来不经过桌面那一份设置，偏好住在这里两端才拿到同一份。写与帧面那两条命令住 `control/machine_prefs.rs`。
//!
//! - 谁读：起会话那几条（启动命令 · 模型表 · 进不进 tmux）· `history-facts`（上限表）· 隐藏的判定在要用的那一刻现读（[`read_now`]），不缓存。
//! - 读不懂：照缺省办（空表 · 不进 tmux），`prefs-read` 照实报 `unreadable`。
//! - 没人设过（文件不在）：照缺省办，报 `absent`。

use crate::common::said::Said;
use cmd_wire::FileState;
use copy_core::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 那份文件名（字面量只住契约 crate：数据位置页按它列出）。
pub const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::PREFS_REL);

/// 盘上那一版。
pub(crate) const VERSION: u32 = 1;

/// 读盘的上限：几张小表。
const MAX_BYTES: u64 = 256 * 1024;

/// 这台偏好：状态（读不懂不覆盖）。
pub(crate) const DOC: crate::common::own_state::Doc<OnDisk> =
    crate::common::own_state::Doc::state(relay_route_core::PREFS_REL, MAX_BYTES);

/// 一张表至多几条（启动命令按家 · 模型按号 · 上限按子串）。
pub(crate) const PREFS_ROWS_MAX: usize = 256;

/// 这台的偏好（盘上那一形，也是线上那几格）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub(crate) struct Prefs {
    /// 每一家（线上的 kind）用哪条启动命令（原值）；没有那一家 ⇒ 那一家自己的默认启动器。
    pub(crate) launchers: BTreeMap<String, String>,
    /// 恢复会话默认进这台的 tmux。
    pub(crate) resume_in_tmux: bool,
    /// 这台每个号默认用哪个模型（号 → 模型，原值）。
    pub(crate) models: BTreeMap<String, String>,
    /// 从这台起新会话默认用哪个权限档（哪一家 → 那一家声明的档的原值）；恢复不读它。
    pub(crate) permission_modes: BTreeMap<String, String>,
    /// 上下文上限表：模型名子串（大小写不敏感，最长匹配胜）→ 上限 tokens。
    #[cfg_attr(test, ts(as = "BTreeMap<String, u32>"))]
    pub(crate) context_limits: BTreeMap<String, u64>,
    /// 每一家（线上的 kind）的数据目录（绝对路径）；没有那一家 ⇒ 照那一家自己的解析（环境变量 → 默认目录）。
    /// 后端起来时读一次（[`apply_agent_homes`]），改了要重启后端才生效。
    pub(crate) agent_homes: BTreeMap<String, String>,
    /// planned-build 的插件根目录（绝对路径）；设了 ⇒ 计划那一族只认它，没设 ⇒ 认这台装着的插件。用的那一刻现读。
    pub(crate) pb_dir: Option<String>,
    /// 子会话（在会话里用 ccm 起的）进不进标签页；缺省隐藏。
    pub(crate) child_tab: cmd_wire::session::Tab,
    /// 分身会话（那一家在后台起的主会话副本）进不进标签页；缺省显示。
    pub(crate) bg_tab: cmd_wire::session::Tab,
}

impl Default for Prefs {
    fn default() -> Self {
        let rules = cmd_wire::session::Rules::default();
        Prefs {
            launchers: BTreeMap::new(),
            resume_in_tmux: false,
            models: BTreeMap::new(),
            permission_modes: BTreeMap::new(),
            context_limits: BTreeMap::new(),
            agent_homes: BTreeMap::new(),
            pb_dir: None,
            child_tab: rules.child_tab,
            bg_tab: rules.bg_tab,
        }
    }
}

/// 盘上整份（多一格版本）。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OnDisk {
    pub(crate) version: u32,
    #[serde(flatten)]
    pub(crate) prefs: Prefs,
}

impl Prefs {
    /// 那一家用哪条启动命令：设过、非空 ⇒ 那一条；否则那一家的默认。
    pub(crate) fn launcher_for(&self, kind: &str, default: &str) -> String {
        self.launchers
            .get(kind)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(default)
            .to_string()
    }

    /// 隐藏的判定读的那两格。
    pub(crate) fn rules(&self) -> cmd_wire::session::Rules {
        cmd_wire::session::Rules {
            child_tab: self.child_tab,
            bg_tab: self.bg_tab,
        }
    }

    /// 上限表那一形（`recognize::facts::ContextLimits`：子串小写）。
    pub(crate) fn limits(&self) -> Vec<(String, u64)> {
        self.context_limits
            .iter()
            .map(|(k, v)| (k.trim().to_lowercase(), *v))
            .collect()
    }
}

/// 现读一次的结果。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Loaded {
    Absent,
    Present(Prefs),
    /// 文件在、读不懂（那一句 ＋ 原话）。
    Unreadable(Said),
}

impl Loaded {
    /// 照用的那一份：读不懂 / 没有 ⇒ 缺省。
    pub(crate) fn prefs(&self) -> Prefs {
        match self {
            Loaded::Present(p) => p.clone(),
            Loaded::Absent | Loaded::Unreadable(_) => Prefs::default(),
        }
    }

    /// 那份文件的状态（`prefs-read` 回包那一格）。
    pub(crate) fn state(&self) -> FileState {
        match self {
            Loaded::Absent => FileState::Absent,
            Loaded::Present(_) => FileState::Ok,
            Loaded::Unreadable(_) => FileState::Unreadable,
        }
    }
}

/// 某个家目录底下那份文件。
pub(crate) fn path_in(home: &Path) -> PathBuf {
    home.join(DOC.rel)
}

/// 这台那份文件（家目录说不出 ⇒ `None`）。
pub(crate) fn path_now() -> Option<PathBuf> {
    crate::platform::paths::home_dir().map(|h| path_in(&h))
}

/// 读一次 `path`。每次都真去读盘。
pub(crate) fn read_at(path: &Path) -> Loaded {
    use crate::common::own_state::Read;
    match DOC.read(path) {
        Read::Absent => Loaded::Absent,
        Read::Unreadable(why) => Loaded::Unreadable(why),
        Read::Present(d) if d.version != VERSION => Loaded::Unreadable(Said::with_raw(
            copy_text(
                "beOwnState.read.notJson",
                &[("path", &path.display().to_string())],
            ),
            format!("version {} (this backend reads {VERSION})", d.version),
        )),
        Read::Present(d) => match check(&d.prefs) {
            Ok(()) => Loaded::Present(d.prefs),
            Err(why) => Loaded::Unreadable(Said::with_raw(
                copy_text(
                    "beOwnState.read.notJson",
                    &[("path", &path.display().to_string())],
                ),
                why,
            )),
        },
    }
}

/// 要用的那一刻调它：现解路径、现读盘。家目录说不出 ⇒ 当没有。
pub(crate) fn read_now() -> Loaded {
    path_now().map_or(Loaded::Absent, |p| read_at(&p))
}

/// 值域：启动命令过全仓那一张命令片段白名单 · 模型名过 `model_name_ok` · 上限是正数 · 每张表有界。读的时候判、写之前也判（`prefs-set`）。
pub(crate) fn check(p: &Prefs) -> Result<(), String> {
    for (what, n) in [
        ("launchers", p.launchers.len()),
        ("models", p.models.len()),
        ("permissionModes", p.permission_modes.len()),
        ("contextLimits", p.context_limits.len()),
        ("agentHomes", p.agent_homes.len()),
    ] {
        if n > PREFS_ROWS_MAX {
            return Err(format!(
                "`{what}` holds {n} rows (at most {PREFS_ROWS_MAX})"
            ));
        }
    }
    for (kind, l) in &p.launchers {
        if let Some(c) = shell_quote_core::launcher_refused_char(l) {
            return Err(format!("`launchers.{kind}` has {c:?}"));
        }
    }
    for (acct, m) in &p.models {
        if acct.is_empty() || !shell_quote_core::model_name_ok(m) {
            return Err(format!("`models.{acct}` is not a model name: {m:?}"));
        }
    }
    for (k, v) in &p.context_limits {
        if k.trim().is_empty() || *v == 0 {
            return Err(format!("`contextLimits.{k}` must be a positive number"));
        }
    }
    for (kind, d) in &p.agent_homes {
        if !agent_home_ok(d) {
            return Err(format!(
                "`agentHomes.{kind}` is not an absolute path: {d:?}"
            ));
        }
    }
    if let Some(d) = p.pb_dir.as_deref().filter(|d| !agent_home_ok(d)) {
        return Err(format!("`pbDir` is not an absolute path: {d:?}"));
    }
    Ok(())
}

/// 数据目录 · pb 路径那两格的值域：非空、绝对路径。
pub(crate) fn agent_home_ok(d: &str) -> bool {
    !d.trim().is_empty() && Path::new(d.trim()).is_absolute()
}
