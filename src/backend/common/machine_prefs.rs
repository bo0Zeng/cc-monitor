//! **这台的偏好**那份文件的形状、值域与读：每一家用哪条启动命令 · 恢复时放在哪 · 每个号默认用哪个模型 · 上下文上限表 · pb 在哪 ·
//! 子会话 / 分身会话进不进标签页（隐藏的判定读它，`history/session_hidden.rs`）。
//! 计划页手加的目录也住这里（`plan_dirs`）。
//! 住这台后端的家（`~/.cc-monitor/prefs.json`，[`relay_route_core::PREFS_REL`]），不住各个前端：手机直连这台，
//! 从来不经过桌面那一份设置，偏好住在这里两端才拿到同一份。写与帧面那两条命令住 `control/machine_prefs.rs`。
//!
//! - 谁读：起会话那几条（启动命令 · 模型表 · 恢复时放在哪）· `history-facts`（上限表）· 隐藏的判定在要用的那一刻现读（[`read_now`]），不缓存。
//! - 读不懂：照缺省办（空表 · 恢复落托管），`prefs-read` 照实报 `unreadable`。
//! - 没人设过（文件不在）：照缺省办，报 `absent`。
//! - 格式比下限老（共用读口当没有）：照缺省办，报 `absent` 并带那一句（[`Loaded::Older`]），不说成没设过；`prefs-set` 照写、原文留 `.v<N>`。

use crate::common::said::Said;
use cmd_wire::FileState;
use copy_core::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 这台偏好：状态（读不懂不覆盖）。第 1 版经 [`v1_to_v2`] 读成这一版；盘上那一份由 [`bring_forward`] 在常驻起来时写回成这一版。
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
pub const PREFS_ROWS_MAX: usize = 256;

/// 状态行模板至多几个字。
pub const STATUS_LINE_MAX: usize = 512;

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
    /// 计划页手加的目录（绝对路径）：`plan-list` 与需手动的计划那一截除了活会话的工作目录，还问这几个。用的那一刻现读。
    pub plan_dirs: Vec<String>,
    /// 子会话（在会话里用 ccm 起的）进不进标签页；缺省隐藏。
    pub child_tab: cmd_wire::session::Tab,
    /// 分身会话（那一家在后台起的主会话副本）进不进标签页；缺省显示。
    pub bg_tab: cmd_wire::session::Tab,
    /// 托管终端的状态行模板：没设 ⇒ 随包的缺省（盘上不写这一格）；空串 ⇒ 不留那一行。照用的那一条只经
    /// `host::managed::status::template_of` 取（缺省住字段表那一处）。瘦客户端接上时 · 偏好变了时现读。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub status_line: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        let rules = cmd_wire::session::Rules::default();
        Prefs {
            launchers: BTreeMap::new(),
            // 恢复不说放在哪 ⇒ 托管（缺省只住这一处）。
            resume_where: cmd_wire::session::ResumeWhere::Managed,
            models: BTreeMap::new(),
            permission_modes: BTreeMap::new(),
            context_limits: BTreeMap::new(),
            agent_homes: BTreeMap::new(),
            pb_dir: None,
            plan_dirs: Vec::new(),
            child_tab: rules.child_tab,
            bg_tab: rules.bg_tab,
            status_line: None,
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
    /// 文件在、格式比下限老（共用读口当没有）：照缺省用、写照写（原文留 `.v<N>`），但不说成「没设过」——带那一句。
    Older(Said),
}

impl Loaded {
    /// 照用的那一份：读不懂 / 比下限老 / 没有 ⇒ 缺省。
    pub fn prefs(&self) -> Prefs {
        match self {
            Loaded::Present(p) => p.clone(),
            Loaded::Absent | Loaded::Unreadable(_) | Loaded::Older(_) => Prefs::default(),
        }
    }

    /// 那份文件的状态（`prefs-read` 回包那一格）：比下限老照共用读口算没有（写不拒），那一句另在回包里。
    pub fn state(&self) -> FileState {
        match self {
            Loaded::Absent | Loaded::Older(_) => FileState::Absent,
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
    match DOC.read_noting_older(path) {
        // 读口说「比下限老、当没有」⇒ 照缺省用，但照实带一句（盘上第几版在原话里）。
        (Read::Absent, Some(o)) => Loaded::Older(Said::with_raw(
            copy_text(
                "beMachinePrefs.read.older",
                &[
                    ("path", &path.display().to_string()),
                    ("mine", &o.readable()),
                ],
            ),
            o,
        )),
        (got, _) => match got {
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
        },
    }
}

/// `path` 那一份是不是比下限还老（读口当没有的那一种）：是 ⇒ 盘上第几版与此后端认的那一段；不在 · 读得懂 · 读不懂 ⇒ `None`。
pub fn older_at(path: &Path) -> Option<crate::common::own_state::Older> {
    DOC.read_noting_older(path).1
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
        ("planDirs", p.plan_dirs.len()),
    ] {
        if n > PREFS_ROWS_MAX {
            return Err(format!(
                "`{what}` holds {n} rows (at most {PREFS_ROWS_MAX})"
            ));
        }
    }
    for (kind, l) in &p.launchers {
        // 偏好存在这一台、只在这一台起会话时用 ⇒ 照这一台的平台认。
        if let Some(c) = shell_quote_core::launcher_refused_char(
            l,
            crate::platform::shell::LOCAL_TERMINAL_IS_POWERSHELL,
        ) {
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
    if let Some(n) = p
        .status_line
        .as_deref()
        .map(|t| t.chars().count())
        .filter(|n| *n > STATUS_LINE_MAX)
    {
        return Err(format!(
            "`statusLine` holds {n} characters (at most {STATUS_LINE_MAX})"
        ));
    }
    if let Some(d) = p.plan_dirs.iter().find(|d| !agent_home_ok(d)) {
        return Err(format!(
            "`planDirs` holds a path that is not absolute: {d:?}"
        ));
    }
    Ok(())
}

/// 整份写（经 `Own` 原子写、拿 `~/.cc-monitor` 那把目录锁）。只有 `prefs-set` 写。
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

/// 常驻起来那一下调一次（升级只此一处，读口本身不写盘）：盘上是读得懂的旧版 ⇒ 照升级后的那一份写回成这一版（原文留 `.v<N>`），回 `true`；
/// 已是这一版 · 没有 · 读不懂 · 比下限还老 ⇒ 不动盘，回 `false`。
pub fn bring_forward(own: &crate::platform::Own) -> Result<bool, Said> {
    let dir = own.home_dir()?;
    let at = dir.join(relay_route_core::file_name_of(relay_route_core::PREFS_REL))?;
    let path = at.as_path();
    if !DOC.behind(path) {
        return Ok(false);
    }
    match read_at(path) {
        Loaded::Present(p) => write(own, &p).map(|()| true),
        Loaded::Absent | Loaded::Unreadable(_) | Loaded::Older(_) => Ok(false),
    }
}

/// 数据目录 · pb 路径那两格的值域：非空、绝对路径。
pub(crate) fn agent_home_ok(d: &str) -> bool {
    !d.trim().is_empty() && Path::new(d.trim()).is_absolute()
}
