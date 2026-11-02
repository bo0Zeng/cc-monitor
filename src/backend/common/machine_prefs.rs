//! **这台的偏好**那份文件的形状、值域与读：每一家用哪条启动命令 · 恢复时放在哪 · 每个号默认用哪个模型 · 上下文上限表 · pb 在哪 ·
//! 子会话 / 分身会话进不进标签页（隐藏的判定读它，`history/session_hidden.rs`）。
//! 住这台后端的家（`~/.cc-monitor/prefs.json`，[`relay_route_core::PREFS_REL`]），不住各个前端：手机直连这台，
//! 从来不经过桌面那一份设置，偏好住在这里两端才拿到同一份。写与帧面那两条命令住 `control/machine_prefs.rs`。
//!
//! - 谁读：起会话那几条（启动命令 · 模型表 · 恢复时放在哪）· `history-facts`（上限表）· 隐藏的判定在要用的那一刻现读（[`read_now`]），不缓存。
//! - 读不懂：照缺省办（空表 · 恢复落托管），`prefs-read` 照实报 `unreadable`。
//! - 没人设过（文件不在）：照缺省办，报 `absent`。

use crate::common::said::Said;
use cmd_wire::FileState;
use copy_core::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 这台偏好：状态（读不懂不覆盖）。第 1 版经 [`v1_to_v2`] 读成这一版。
pub(crate) const DOC: crate::common::own_state::Doc<Prefs> =
    crate::common::upgrade!(relay_route_core::PREFS_FILE, [v1_to_v2]);

/// 第 1 版 ⇒ 第 2 版：恢复那一格从开关 `resumeInTmux` 换成三选 `resumeWhere`（开 ⇒ tmux，关 / 缺 ⇒ 托管），别的几格原样。
fn v1_to_v2(mut v: serde_json::Value) -> Result<serde_json::Value, Said> {
    use cmd_wire::session::ResumeWhere;
    let Some(o) = v.as_object_mut() else {
        return Ok(v);
    };
    let tmux = o.remove("resumeInTmux").and_then(|b| b.as_bool()) == Some(true);
    let w = if tmux {
        ResumeWhere::Tmux
    } else {
        ResumeWhere::Managed
    };
    o.insert("resumeWhere".into(), serde_json::json!(w));
    Ok(v)
}

/// 一张表至多几条（启动命令按家 · 模型按号 · 上限按子串）。
pub(crate) const PREFS_ROWS_MAX: usize = 256;

/// 这台的偏好（盘上那一形，也是线上那几格）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Prefs {
    /// 每一家（线上的 kind）用哪条启动命令（原值）；没有那一家 ⇒ 那一家自己的默认启动器。
    pub launchers: BTreeMap<String, String>,
    /// 恢复会话不说放在哪时落在哪：托管 · tmux · 照那条会话起时那一种。
    pub resume_where: cmd_wire::session::ResumeWhere,
    /// 这台每个号默认用哪个模型（号 → 模型，原值）。
    pub models: BTreeMap<String, String>,
    /// 从这台起新会话默认用哪个权限档（哪一家 → 那一家声明的档的原值）；恢复不读它。
    pub permission_modes: BTreeMap<String, String>,
    /// 上下文上限表：模型名子串（大小写不敏感，最长匹配胜）→ 上限 tokens。
    #[cfg_attr(any(test, feature = "ts"), ts(as = "BTreeMap<String, u32>"))]
    pub context_limits: BTreeMap<String, u64>,
    /// 每一家（线上的 kind）的数据目录（绝对路径）；没有那一家 ⇒ 照那一家自己的解析（环境变量 → 默认目录）。
    /// 后端起来时读一次（[`apply_agent_homes`]），改了要重启后端才生效。
    pub agent_homes: BTreeMap<String, String>,
    /// planned-build 的插件根目录（绝对路径）；设了 ⇒ 计划那一族只认它，没设 ⇒ 认这台装着的插件。用的那一刻现读。
    pub pb_dir: Option<String>,
    /// 子会话（在会话里用 ccm 起的）进不进标签页；缺省隐藏。
    pub child_tab: cmd_wire::session::Tab,
    /// 分身会话（那一家在后台起的主会话副本）进不进标签页；缺省显示。
    pub bg_tab: cmd_wire::session::Tab,
}

impl Default for Prefs {
    fn default() -> Self {
        let rules = cmd_wire::session::Rules::default();
        Prefs {
            launchers: BTreeMap::new(),
            resume_where: cmd_wire::session::ResumeWhere::default(),
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

impl Prefs {
    /// 那一家用哪条启动命令：设过、非空 ⇒ 那一条；否则那一家的默认。
    pub fn launcher_for(&self, kind: &str, default: &str) -> String {
        self.launchers
            .get(kind)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(default)
            .to_string()
    }

    /// 恢复一条会话想不想进 tmux（规则住这一处，`session_open::default_run` 收它）：偏好 tmux；或偏好照起时那一种、
    /// 启动台账记着它起在 tmux 里。记着托管 / 记不着 ⇒ 不想（照托管缺省）。
    pub fn resume_wants_tmux(&self, started: Option<cmd_wire::frame::TerminalHost>) -> bool {
        use cmd_wire::session::ResumeWhere;
        match self.resume_where {
            ResumeWhere::Managed => false,
            ResumeWhere::Tmux => true,
            ResumeWhere::AsStarted => started == Some(cmd_wire::frame::TerminalHost::Tmux),
        }
    }

    /// 隐藏的判定读的那两格。
    pub fn rules(&self) -> cmd_wire::session::Rules {
        cmd_wire::session::Rules {
            child_tab: self.child_tab,
            bg_tab: self.bg_tab,
        }
    }

    /// 上限表那一形（`recognize::facts::ContextLimits`：子串小写）。
    pub fn limits(&self) -> Vec<(String, u64)> {
        self.context_limits
            .iter()
            .map(|(k, v)| (k.trim().to_lowercase(), *v))
            .collect()
    }
}

/// 现读一次的结果。
#[derive(Debug, Clone, PartialEq)]
pub enum Loaded {
    Absent,
    Present(Prefs),
    /// 文件在、读不懂（那一句 ＋ 原话）。
    Unreadable(Said),
}

impl Loaded {
    /// 照用的那一份：读不懂 / 没有 ⇒ 缺省。
    pub fn prefs(&self) -> Prefs {
        match self {
            Loaded::Present(p) => p.clone(),
            Loaded::Absent | Loaded::Unreadable(_) => Prefs::default(),
        }
    }

    /// 那份文件的状态（`prefs-read` 回包那一格）。
    pub fn state(&self) -> FileState {
        match self {
            Loaded::Absent => FileState::Absent,
            Loaded::Present(_) => FileState::Ok,
            Loaded::Unreadable(_) => FileState::Unreadable,
        }
    }
}

/// 某个家目录底下那份文件。
pub fn path_in(home: &Path) -> PathBuf {
    home.join(DOC.rel)
}

/// 这台那份文件（家目录说不出 ⇒ `None`）。
pub fn path_now() -> Option<PathBuf> {
    crate::platform::paths::home_dir().map(|h| path_in(&h))
}

/// 读一次 `path`。每次都真去读盘。
pub fn read_at(path: &Path) -> Loaded {
    use crate::common::own_state::Read;
    match DOC.read(path) {
        Read::Absent => Loaded::Absent,
        Read::Unreadable(why) => Loaded::Unreadable(why),
        Read::Present(p) => match check(&p) {
            Ok(()) => Loaded::Present(p),
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
pub fn read_now() -> Loaded {
    path_now().map_or(Loaded::Absent, |p| read_at(&p))
}

/// 值域：启动命令过全仓那一张命令片段白名单 · 模型名过 `model_name_ok` · 上限是正数 · 每张表有界。读的时候判、写之前也判（`prefs-set`）。
pub fn check(p: &Prefs) -> Result<(), String> {
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

/// 整份写（经 `Own` 原子写、拿 `~/.cc-monitor` 那把目录锁；盖第 1 版先把原文留成 `prefs.json.v1`）。只有 `prefs-set` 写。
pub fn write(own: &crate::platform::Own, p: &Prefs) -> Result<(), Said> {
    let dir = own.home_dir()?;
    own.ensure_dir(&dir).map_err(|e| {
        Said::with_raw(
            copy_text(
                "reason.fs.writeFailed",
                &[
                    ("path", &dir.as_path().display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    let held = own.lock(&dir)?;
    DOC.write(own, &dir, &held, p)
}

/// 数据目录 · pb 路径那两格的值域：非空、绝对路径。
pub(crate) fn agent_home_ok(d: &str) -> bool {
    !d.trim().is_empty() && Path::new(d.trim()).is_absolute()
}
