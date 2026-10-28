//! **这台的偏好**帧面那两条命令（`prefs-read` · `prefs-set`）与起会话要的那几样。那份文件的形状、值域、读与整份写住 `common/machine_prefs.rs`。
//!
//! - 谁决定写：只有本模块的 [`answer_set`]（帧面 `prefs-set` ＋ 派生的 CLI 面）与第 7 批换形那一次（[`super::prefs_migration`]，下一版删）；前端从不碰这个文件。
//! - 读不懂：不写回，`prefs-set` 也拒（不拿空的盖掉原文）。

use crate::common::machine_prefs::{check, path_in, read_at, read_now, write, Loaded, Prefs};
use crate::common::said::Said;
use crate::platform::Own;
use copy_core::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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

/// 那份文件的状态：没设过 · 读得懂 · 读不懂（两端同一个名，住契约）。
pub(crate) use cmd_wire::FileState;

/// `prefs-read` / `prefs-set` 的回包：状态 ＋ 照用的那一份（读不懂 ⇒ 缺省那一份）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrefsReply {
    /// `absent`（没设过）· `ok` · `unreadable`（读不懂：也照答，不是失败）。
    pub(crate) state: FileState,
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

/// 这一趟要写的权限档都在那一家的清单里（读的时候不核：适配层改过清单的那一档起会话时当没设，[`permission_mode_for`]）。
fn offered_modes(req: &PrefsSet) -> Result<(), String> {
    for (kind, m) in req.permission_modes.iter().flatten() {
        if let Some(m) = m.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
            if offered(kind, m).is_none() {
                return Err(format!(
                    "`permissionModes.{kind}` is not a mode that agent offers: {m:?}"
                ));
            }
        }
    }
    Ok(())
}

/// `kind` 那一家的清单里那一档（不在 / 那一家没有权限档 ⇒ `None`）。
pub(crate) fn offered(kind: &str, mode: &str) -> Option<crate::agents::Choice> {
    crate::agents::launch_face_among(crate::agents::REGISTRY, kind)?
        .permission?
        .modes
        .iter()
        .find(|c| c.value == mode)
        .copied()
}

/// 从这台起 `kind` 那一家的新会话默认的权限档：设了且还在那一家清单里 ⇒ 那一档；清单里没了 ⇒ 不带、记一行。
pub(crate) fn permission_mode_for(p: &Prefs, kind: &str) -> Option<String> {
    let m = p.permission_modes.get(kind)?;
    if offered(kind, m).is_none() {
        tracing::warn!("这台偏好里 {kind} 的新会话权限档 {m:?} 那一家已不认 ⇒ 不带");
        return None;
    }
    Some(m.clone())
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
    /// 恢复时放在哪（三选）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) resume_where: Option<cmd_wire::session::ResumeWhere>,
    /// 某几个号的模型：`{号: 模型 | null}`，`null` ⇒ 删掉那一条。只动点到的那几个号。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) models: Option<BTreeMap<String, Option<String>>>,
    /// 某几家新会话的权限档：`{kind: 档 | null}`，`null` ⇒ 删掉那一条。写的时候核在那一家的清单里。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) permission_modes: Option<BTreeMap<String, Option<String>>>,
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
            resume_where: Some(p.resume_where),
            models: Some(
                p.models
                    .iter()
                    .map(|(k, v)| (k.clone(), Some(v.clone())))
                    .collect(),
            ),
            permission_modes: Some(
                p.permission_modes
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
    if let Some(w) = s.resume_where {
        p.resume_where = w;
    }
    for (k, v) in s.models.iter().flatten() {
        match v.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
            None => p.models.remove(k),
            Some(m) => p.models.insert(k.clone(), m.to_string()),
        };
    }
    for (k, v) in s.permission_modes.iter().flatten() {
        match v.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
            None => p.permission_modes.remove(k),
            Some(m) => p.permission_modes.insert(k.clone(), m.to_string()),
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
    check(&next)
        .and_then(|()| offered_modes(req))
        .map_err(|e| PrefsSetErr::BadArgs(crate::common::contract::malformed(&e)))?;
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
