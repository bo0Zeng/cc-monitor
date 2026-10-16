//! **这台的偏好**：对这台做事时怎么做 —— 每一家用哪条启动命令 · 恢复时进不进 tmux · 每个号默认用哪个模型 · 上下文上限表 · pb 在哪 ·
//! 子会话 / 分身会话进不进标签页（隐藏的判定读它，`control/session_hidden.rs`）。
//! 住这台后端的家（`~/.cc-monitor/prefs.json`，[`relay_route_core::PREFS_REL`]），不住各个前端：手机直连这台，
//! 从来不经过桌面那一份设置，偏好住在这里两端才拿到同一份。
//!
//! - 谁写：只有本模块的 [`answer_set`]（帧面 `prefs-set` ＋ 派生的 CLI 面）与第 7 批换形那一次（[`super::prefs_migration`]，下一版删）；前端从不碰这个文件。
//! - 谁读：起会话那几条（启动命令 · 模型表 · 进不进 tmux）· `history-facts`（上限表）在要用的那一刻现读（[`read_now`]），不缓存。
//! - 读不懂：照缺省办（空表 · 不进 tmux），`prefs-read` 照实报 `unreadable`；不写回，`prefs-set` 也拒（不拿空的盖掉原文）。
//! - 没人设过（文件不在）：照缺省办，报 `absent`。

use crate::common::said::Said;
use crate::platform::Own;
use copy_core::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 那份文件名（字面量只住契约 crate：数据位置页按它列出）。
pub const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::PREFS_REL);

/// 盘上那一版。
const VERSION: u32 = 1;

/// 读盘的上限：几张小表。
const MAX_BYTES: u64 = 256 * 1024;

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
        let rules = crate::control::session_hidden::Rules::default();
        Prefs {
            launchers: BTreeMap::new(),
            resume_in_tmux: false,
            models: BTreeMap::new(),
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
struct OnDisk {
    version: u32,
    #[serde(flatten)]
    prefs: Prefs,
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
    pub(crate) fn rules(&self) -> crate::control::session_hidden::Rules {
        crate::control::session_hidden::Rules {
            child_tab: self.child_tab,
            bg_tab: self.bg_tab,
        }
    }

    /// 上限表那一形（`facts_query::ContextLimits`：子串小写）。
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
    fn state(&self) -> PrefsState {
        match self {
            Loaded::Absent => PrefsState::Absent,
            Loaded::Present(_) => PrefsState::Ok,
            Loaded::Unreadable(_) => PrefsState::Unreadable,
        }
    }
}

/// 某个家目录底下那份文件。
pub(crate) fn path_in(home: &Path) -> PathBuf {
    home.join(relay_route_core::PREFS_REL)
}

/// 这台那份文件（家目录说不出 ⇒ `None`）。
pub(crate) fn path_now() -> Option<PathBuf> {
    crate::platform::paths::home_dir().map(|h| path_in(&h))
}

/// 读一次 `path`。每次都真去读盘。
pub(crate) fn read_at(path: &Path) -> Loaded {
    use crate::common::own_state::{read_json, Read};
    match read_json::<OnDisk>(path, MAX_BYTES) {
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

/// 值域：启动命令过全仓那一张命令片段白名单 · 模型名过 `model_name_ok` · 上限是正数 · 每张表有界。
fn check(p: &Prefs) -> Result<(), String> {
    for (what, n) in [
        ("launchers", p.launchers.len()),
        ("models", p.models.len()),
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

/// 这台设了的各家数据目录（读不懂 / 没设 ⇒ 空）。
pub(crate) fn agent_homes_in(l: &Loaded) -> BTreeMap<String, PathBuf> {
    match l {
        Loaded::Present(p) => p
            .agent_homes
            .iter()
            .map(|(k, d)| (k.clone(), PathBuf::from(d.trim())))
            .collect(),
        Loaded::Absent | Loaded::Unreadable(_) => BTreeMap::new(),
    }
}

/// 流模式起来时调一次（`main.rs`，在解析 agent 家目录、起任何线程之前）：这台给某一家设了数据目录 ⇒ 交成本进程里
/// 那一家的目录环境变量（变量名问注册表那一格 `account_env_of`；解析只在 `agents::home_at` 一处，起的会话照旧继承）；
/// 没设 / 那一家没有这个变量 ⇒ 不动。改了要重启后端才生效。
/// `own` 只用来认出换形之前那一次（[`super::prefs_migration::agent_homes_before_move`]，下一版删）。
pub fn apply_agent_homes(own: &Own) {
    let l = read_now();
    let homes = match l {
        Loaded::Absent => super::prefs_migration::agent_homes_before_move(own),
        _ => agent_homes_in(&l),
    };
    for (env, d) in agent_home_envs(homes) {
        tracing::info!("这台设了数据目录：{env}={}", d.display());
        std::env::set_var(env, d);
    }
}

/// 各家的数据目录 ⇒ `(那一家的目录环境变量, 路径)`（变量名问注册表 `account_env_of`）；认不出 / 没有那个变量的那一家不交、说一句。
pub(crate) fn agent_home_envs(homes: BTreeMap<String, PathBuf>) -> Vec<(&'static str, PathBuf)> {
    homes
        .into_iter()
        .filter_map(|(kind, d)| {
            let env = crate::agents::account_env_of(&kind).filter(|e| !e.is_empty());
            if env.is_none() {
                tracing::warn!("这台给 {kind} 设了数据目录，那一家没有目录环境变量，不用");
            }
            Some((env?, d))
        })
        .collect()
}

/// 整份写（经 `Own` 原子写、拿 `~/.cc-monitor` 那把目录锁）。
pub(crate) fn write(own: &Own, p: &Prefs) -> Result<(), Said> {
    let dir = own.home_dir()?;
    own.ensure_dir(&dir).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beExitPolicy.writeAt.mkdirFailed",
                &[
                    ("dir", &dir.as_path().display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    let held = own.lock(&dir)?;
    let path = dir.join(FILE_NAME)?;
    let body = crate::common::own_state::json_body(
        path.as_path(),
        &OnDisk {
            version: VERSION,
            prefs: p.clone(),
        },
    )?;
    own.write(&path, &held, &body)
}

/// 那份文件的状态：没设过 · 读得懂 · 读不懂（读不懂也照答，不是失败）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PrefsState {
    Absent,
    Ok,
    Unreadable,
}

/// `prefs-read` / `prefs-set` 的回包：状态 ＋ 照用的那一份（读不懂 ⇒ 缺省那一份）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrefsReply {
    #[cfg_attr(test, ts(type = "\"absent\" | \"ok\" | \"unreadable\""))]
    pub(crate) state: PrefsState,
    /// `unreadable` 时的原因（那一句不带原话）。
    pub(crate) reason: Option<String>,
    /// `unreadable` 时的复制详情。
    pub(crate) detail: Option<String>,
    /// 那份文件的路径（家目录说不出 ⇒ `null`）。
    pub(crate) path: Option<String>,
    /// 照用的那一份。
    pub(crate) prefs: Prefs,
}

fn reply(l: &Loaded, path: Option<&Path>) -> PrefsReply {
    let why = match l {
        Loaded::Unreadable(w) => Some(w),
        _ => None,
    };
    let (reason, detail) = crate::wire::detail::unreadable("prefs-read", why);
    PrefsReply {
        state: l.state(),
        reason,
        detail,
        path: path.map(|p| p.display().to_string()),
        prefs: l.prefs(),
    }
}

/// `prefs-read` 的本体（家目录是参数）。
pub(crate) fn answer_read_in(home: Option<&Path>) -> PrefsReply {
    let path = home.map(path_in);
    let l = path.as_deref().map_or(Loaded::Absent, read_at);
    reply(&l, path.as_deref())
}

/// `prefs-set` 的入参：给了的那一节整节换掉，没给的不动。`models` 里值为 `null` 的那一条删掉、别的号不动。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrefsSet {
    /// 某一家的启动命令：`{kind: 命令}`，空串 ⇒ 删掉那一家（回到它的默认）。只动点到的那几家。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) launchers: Option<BTreeMap<String, String>>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) resume_in_tmux: Option<bool>,
    /// 某几个号的模型：`{号: 模型 | null}`，`null` ⇒ 删掉那一条。只动点到的那几个号。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) models: Option<BTreeMap<String, Option<String>>>,
    /// 整张上限表（整张换掉）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, as = "Option<BTreeMap<String, u32>>"))]
    pub(crate) context_limits: Option<BTreeMap<String, u64>>,
    /// 某一家的数据目录：`{kind: 路径}`，空串 ⇒ 删掉那一家（回到它自己的解析）。只动点到的那几家；改了要重启后端才生效。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) agent_homes: Option<BTreeMap<String, String>>,
    /// 子会话进不进标签页。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) child_tab: Option<cmd_wire::session::Tab>,
    /// 分身会话进不进标签页。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) bg_tab: Option<cmd_wire::session::Tab>,
    /// pb 的插件根目录：空串 ⇒ 删掉（回到认装着的插件）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) pb_dir: Option<String>,
}

/// 整份当补丁（交给一台还没设过的：每一节都给）。
impl From<&Prefs> for PrefsSet {
    fn from(p: &Prefs) -> Self {
        PrefsSet {
            launchers: Some(p.launchers.clone()),
            resume_in_tmux: Some(p.resume_in_tmux),
            models: Some(
                p.models
                    .iter()
                    .map(|(k, v)| (k.clone(), Some(v.clone())))
                    .collect(),
            ),
            context_limits: Some(p.context_limits.clone()),
            agent_homes: Some(p.agent_homes.clone()),
            child_tab: Some(p.child_tab),
            bg_tab: Some(p.bg_tab),
            pb_dir: p.pb_dir.clone(),
        }
    }
}

/// 补丁打到 `p` 上（纯）。
pub(crate) fn patched(mut p: Prefs, s: &PrefsSet) -> Prefs {
    for (k, v) in s.launchers.iter().flatten() {
        match v.trim() {
            "" => p.launchers.remove(k),
            t => p.launchers.insert(k.clone(), t.to_string()),
        };
    }
    if let Some(t) = s.resume_in_tmux {
        p.resume_in_tmux = t;
    }
    for (k, v) in s.models.iter().flatten() {
        match v.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
            None => p.models.remove(k),
            Some(m) => p.models.insert(k.clone(), m.to_string()),
        };
    }
    if let Some(l) = &s.context_limits {
        p.context_limits = l.clone();
    }
    for (k, v) in s.agent_homes.iter().flatten() {
        match v.trim() {
            "" => p.agent_homes.remove(k),
            t => p.agent_homes.insert(k.clone(), t.to_string()),
        };
    }
    if let Some(t) = s.child_tab {
        p.child_tab = t;
    }
    if let Some(t) = s.bg_tab {
        p.bg_tab = t;
    }
    if let Some(d) = &s.pb_dir {
        p.pb_dir = Some(d.trim().to_string()).filter(|d| !d.is_empty());
    }
    p
}

/// 这台设了的 pb 路径（读不懂 / 没设 ⇒ `None`）。
pub(crate) fn pb_dir_now() -> Option<PathBuf> {
    match read_now() {
        Loaded::Present(p) => p.pb_dir.map(PathBuf::from),
        Loaded::Absent | Loaded::Unreadable(_) => None,
    }
}

/// 一句 ＋ 原话交给只装一句的那一层（投出去时拆回来）。
fn joined(s: Said) -> String {
    crate::common::contract::joined(s.said, s.raw)
}

crate::wire::typed::codes! {
    /// `prefs-set` 的码：值不对 · 盘上那份读不懂（不拿空的盖掉它）· 写没成。
    pub(crate) enum PrefsSetErr {
        Unreadable = "unreadable",
        IoFailed = "io_failed",
    }
}

/// `prefs-set` 的本体（家目录是参数）：读 → 打补丁 → 校验 → 整份写 → 读回。
pub(crate) fn answer_set_in(
    home: Option<&Path>,
    own: &Own,
    req: &PrefsSet,
) -> Result<PrefsReply, PrefsSetErr> {
    let path = home
        .map(path_in)
        .ok_or_else(|| PrefsSetErr::IoFailed(copy_text("beExitPolicy.answerSet.noHome", &[])))?;
    let base = match read_at(&path) {
        Loaded::Unreadable(why) => return Err(PrefsSetErr::Unreadable(joined(why))),
        l => l.prefs(),
    };
    let next = patched(base, req);
    check(&next).map_err(|e| PrefsSetErr::BadArgs(crate::common::contract::malformed(&e)))?;
    write(own, &next).map_err(|e| PrefsSetErr::IoFailed(joined(e)))?;
    Ok(reply(&read_at(&path), Some(&path)))
}

crate::wire::typed::codes! {
    /// `prefs-read` 没有别的码：读不懂是一个状态（`state: "unreadable"`），不是一次失败。
    pub(crate) enum PrefsReadErr {}
}

/// `prefs-read`：这台的偏好（同步文件 I/O ⇒ 阻塞档）。
pub(crate) struct PrefsRead;

impl crate::wire::typed::Command for PrefsRead {
    const NAME: &'static str = "prefs-read";
    const TAKES_INPUT: bool = false;
    type Req = crate::wire::typed::NoArgs;
    type Reply = PrefsReply;
    type Err = PrefsReadErr;
}

impl crate::wire::typed::Handle for PrefsRead {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<PrefsReply, PrefsReadErr>> + Send + 'static {
        async move {
            Ok(answer_read_in(
                crate::platform::paths::home_dir().as_deref(),
            ))
        }
    }
}

/// `prefs-set`：改这台的偏好，写完读回（写自家状态 ⇒ 只递写句柄）。
pub(crate) struct PrefsSetCmd;

impl crate::wire::typed::Command for PrefsSetCmd {
    const NAME: &'static str = "prefs-set";
    type Req = PrefsSet;
    type Reply = PrefsReply;
    type Err = PrefsSetErr;
}

impl crate::wire::typed::Handle for PrefsSetCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: PrefsSet,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<PrefsReply, PrefsSetErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let own = &needs;

            answer_set_in(crate::platform::paths::home_dir().as_deref(), own, &req)
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/machine_prefs_tests.rs"]
mod tests;
