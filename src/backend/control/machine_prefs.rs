//! **这台的偏好**：对这台做事时怎么做 —— 每一家用哪条启动命令 · 恢复时进不进 tmux · 每个号默认用哪个模型 · 上下文上限表。
//! 住这台后端的家（`~/.cc-monitor/prefs.json`，[`relay_route_core::PREFS_REL`]），不住各个前端：手机直连这台，
//! 从来不经过桌面那一份设置，偏好住在这里两端才拿到同一份。
//!
//! - 谁写：只有本模块的 [`answer_set`]（帧面 `prefs-set` ＋ 派生的 CLI 面）与迁移那一次（[`migrate`]）；前端从不碰这个文件。
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
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    fn state(&self) -> &'static str {
        match self {
            Loaded::Absent => "absent",
            Loaded::Present(_) => "ok",
            Loaded::Unreadable(_) => "unreadable",
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
    Ok(())
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

/// `prefs-read` / `prefs-set` 的回包：状态 ＋ 照用的那一份（读不懂 ⇒ 缺省那一份）。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrefsReply {
    /// `absent`（没设过）· `ok` · `unreadable`（读不懂：也照答，不是失败）。
    #[cfg_attr(test, ts(type = "\"absent\" | \"ok\" | \"unreadable\""))]
    pub(crate) state: &'static str,
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
    let (reason, detail) = crate::stream::detail::unreadable("prefs-read", why);
    let text = |v: serde_json::Value| v.as_str().map(str::to_string);
    PrefsReply {
        state: l.state(),
        reason: text(reason),
        detail: text(detail),
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
#[derive(Debug, Clone, Default, serde::Deserialize)]
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
    p
}

/// 一句 ＋ 原话交给只装一句的那一层（投出去时拆回来）。
fn joined(s: Said) -> String {
    crate::common::contract::joined(s.said, s.raw)
}

crate::stream::inbound::typed::codes! {
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

crate::stream::inbound::typed::codes! {
    /// `prefs-read` 没有别的码：读不懂是一个状态（`state: "unreadable"`），不是一次失败。
    pub(crate) enum PrefsReadErr {}
}

/// `prefs-read`：这台的偏好（同步文件 I/O ⇒ 阻塞档）。
pub(crate) struct PrefsRead;

impl crate::stream::inbound::typed::Command for PrefsRead {
    const NAME: &'static str = "prefs-read";
    const TAKES_INPUT: bool = false;
    type Req = crate::stream::inbound::typed::NoArgs;
    type Reply = PrefsReply;
    type Err = PrefsReadErr;
}

impl crate::stream::inbound::typed::Blocking for PrefsRead {
    fn run(
        _req: crate::stream::inbound::typed::NoArgs,
        _cx: &crate::stream::inbound::typed::Cx,
    ) -> Result<PrefsReply, PrefsReadErr> {
        Ok(answer_read_in(
            crate::platform::paths::home_dir().as_deref(),
        ))
    }
}

/// `prefs-set`：改这台的偏好，写完读回（写自家状态 ⇒ 只递写句柄）。
pub(crate) struct PrefsSetCmd;

impl crate::stream::inbound::typed::Command for PrefsSetCmd {
    const NAME: &'static str = "prefs-set";
    type Req = PrefsSet;
    type Reply = PrefsReply;
    type Err = PrefsSetErr;
}

impl crate::stream::inbound::typed::Owning for PrefsSetCmd {
    fn run(
        req: PrefsSet,
        _cx: &crate::stream::inbound::typed::Cx,
        own: &Own,
    ) -> Result<PrefsReply, PrefsSetErr> {
        answer_set_in(crate::platform::paths::home_dir().as_deref(), own, &req)
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/machine_prefs_tests.rs"]
mod tests;
