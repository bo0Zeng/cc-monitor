//! 帧面：轮换配置的写（typed，配置编辑层）。改设置交一个意图（设一格 · 刷一段 · 拖号牌 · 加号 · 移出），后端照声明改完、校验形状（不认格的意思）；
//! 会话用哪份 · 改过的设置写进 `rotation-uses.json`，策略文件的写只重写头、代码一个字不动。另住 `session-open` 起之前那一笔。

use crate::accounts::quota::rotation;
use crate::accounts::quota::setup::uses::{self, Uses, UsesBook};
use crate::accounts::quota::setup::{form, script, shelf};
use crate::faces::rotation_face::Ctx;
use crate::faces::rotation_view_face::{checked, RotRefused};
use crate::wire::typed::Command;
pub(crate) use cmd_wire::rotation::{
    RotBadStrategy, RotConflict, RotDone, RotInUse, RotStrategyId, RotStrategyRev,
    RotationSaveReply, RotationSettingsReply,
};
use copy_core::copy_text;
pub(crate) use form::RotEdit;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

/// 写哪一份：一个会话（会话面板）· 一份策略（设置页）。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) enum RotWhere {
    Session { sid: String },
    Strategy { strategy: String },
}

fn gone(id: &str) -> String {
    copy_text("beRotation.strategy.gone", &[("name", id)])
}

fn no_session() -> String {
    copy_text("beRotation.session.gone", &[])
}

fn refused_said() -> String {
    copy_text("beRotation.edit.refused", &[])
}

fn note(e: crate::common::said::Said) -> String {
    crate::common::said::IntoNote::into_note(e)
}

/// 那一份（要读得懂）。
fn find(ctx: &Ctx, id: &str) -> Option<Arc<shelf::Shelved>> {
    shelf::find(&shelf::all(ctx.setup.dir().as_deref()), id).filter(|s| s.file.is_ok())
}

fn head_of(s: &shelf::Shelved) -> Option<&script::Head> {
    s.file.as_ref().ok().map(|f| &f.head)
}

fn session_of(book: &UsesBook, sid: &str) -> Option<Uses> {
    book.sessions.get(sid).cloned()
}

/// 名字（＝ 文件名 ＝ id ＝ 标题，§2.1）的写法过不了 ⇒ 那一句话。
fn name_ok(name: &str) -> Result<String, String> {
    let n = name.trim();
    if shelf::id_ok(n) {
        Ok(n.to_string())
    } else {
        Err(copy_text("beRotation.edit.noName", &[]))
    }
}

/// 一份新文件的名字：照写法，策略目录里不撞（不分大小写，有的盘不分）。
fn fresh_name(ctx: &Ctx, name: &str) -> Result<String, String> {
    let n = name_ok(name)?;
    let all = shelf::all(ctx.setup.dir().as_deref());
    if all.iter().any(|s| s.id.to_lowercase() == n.to_lowercase()) {
        return Err(copy_text("beRotation.edit.nameTaken", &[]));
    }
    Ok(n)
}

// ── rotation-settings ─────────────────────────────────────────────────────────────────────

/// `rotation-settings` 的入参。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationSettingsReq {
    pub(crate) target: RotWhere,
    /// 设置页：界面手上那份草稿（上一次回包的 `settings` 原样带回；缺 ⇒ 文件里的）。会话 ⇒ 不带（草稿在这台）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "unknown"))]
    pub(crate) settings: Option<Value>,
    pub(crate) edit: RotEdit,
    /// 改之前读到的那份策略文件的版本；对不上 ⇒ `conflict`。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) if_rev: Option<String>,
}

crate::wire::typed::codes! {
    /// `rotation-settings` 自己的码。
    pub(crate) enum RotationSettingsErr {
        NoSession = "no_session",
        NoStrategy = "no_strategy",
        Refused = "refused" => RotRefused,
        Conflict = "conflict" => RotConflict,
        IoFailed = "io_failed",
    }
}

/// `rotation-settings`：照意图改 ⇒ 会话写进 `rotation.json` 的草稿；策略只回改完的那份（设置页的草稿在界面手上）。
pub(crate) struct RotationSettings;

impl Command for RotationSettings {
    const NAME: &'static str = "rotation-settings";
    type Req = RotationSettingsReq;
    type Reply = RotationSettingsReply;
    type Err = RotationSettingsErr;
}

impl crate::wire::typed::Handle for RotationSettings {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: RotationSettingsReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotationSettingsReply, RotationSettingsErr>>
           + Send
           + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (_files, own) = (needs.0, &needs.1);

            settings_with(&Ctx::here(), own, &req)
        }
    }
}

pub(crate) fn settings_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationSettingsReq,
) -> Result<RotationSettingsReply, RotationSettingsErr> {
    let book = ctx.setup.uses.now();
    let (sid, id) = match &req.target {
        RotWhere::Session { sid } => {
            let s = session_of(&book, sid)
                .ok_or_else(|| RotationSettingsErr::NoSession(no_session()))?;
            (Some(sid.clone()), s.id)
        }
        RotWhere::Strategy { strategy } => (None, strategy.clone()),
    };
    let sh = find(ctx, &id).ok_or_else(|| RotationSettingsErr::NoStrategy(gone(&id)))?;
    let head = head_of(&sh);
    let base = match (&sid, &req.settings) {
        (Some(sid), _) => session_of(&book, sid).and_then(|s| s.draft),
        (None, given) => given.clone(),
    }
    .or_else(|| head.map(|h| h.settings.clone()))
    .unwrap_or_else(|| Value::Object(Default::default()));
    let form = head.map_or(&[][..], |h| h.form.as_slice());
    let settings = form::apply(form, &base, &req.edit)
        .map_err(|e| RotationSettingsErr::BadArgs(crate::common::contract::malformed(&e)))?;
    checked(sh.file.as_ref().ok(), &settings)
        .map_err(|r| RotationSettingsErr::Refused(refused_said(), r))?;
    if req.if_rev.as_deref().is_some_and(|r| r != sh.rev) {
        return Err(RotationSettingsErr::Conflict(
            copy_text("beRotation.edit.conflict", &[]),
            RotConflict {
                rev: sh.rev.clone(),
            },
        ));
    }
    let dirty = head.map(|h| &h.settings) != Some(&settings);
    if let Some(sid) = sid {
        let draft = dirty.then(|| settings.clone());
        let rev = sh.rev.clone();
        ctx.setup
            .change(own, |b| {
                if let Some(s) = b.sessions.get_mut(&sid) {
                    s.draft = draft;
                    s.rev = rev;
                }
            })
            .map_err(|e| RotationSettingsErr::IoFailed(note(e)))?;
    }
    Ok(RotationSettingsReply { dirty, settings })
}

// ── rotation-save ─────────────────────────────────────────────────────────────────────────

/// 另存成一份新的。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotSaveAs {
    pub(crate) name: String,
}

/// `rotation-save` 的入参。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationSaveReq {
    pub(crate) target: RotWhere,
    /// 要存的设置（会话缺 ⇒ 存它的草稿；策略缺 ⇒ `bad_args`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "unknown"))]
    pub(crate) settings: Option<Value>,
    /// 改之前读到的那份策略文件的版本。
    pub(crate) if_rev: String,
    /// 另存成一份新的（会话改用它）；缺 ⇒ 写回原来那份。
    #[serde(default, rename = "as")]
    #[cfg_attr(test, ts(optional, rename = "as"))]
    pub(crate) save_as: Option<RotSaveAs>,
}

crate::wire::typed::codes! {
    /// `rotation-save` 自己的码。
    pub(crate) enum RotationSaveErr {
        NoSession = "no_session",
        NoStrategy = "no_strategy",
        Refused = "refused" => RotRefused,
        Conflict = "conflict" => RotConflict,
        IoFailed = "io_failed",
    }
}

/// `rotation-save`：照那份策略的原文只重写头（设置 · 另存时的名字），代码原样；会话的草稿丢掉。
pub(crate) struct RotationSave;

impl Command for RotationSave {
    const NAME: &'static str = "rotation-save";
    type Req = RotationSaveReq;
    type Reply = RotationSaveReply;
    type Err = RotationSaveErr;
}

impl crate::wire::typed::Handle for RotationSave {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: RotationSaveReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotationSaveReply, RotationSaveErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (_files, own) = (needs.0, &needs.1);

            save_with(&Ctx::here(), own, &req)
        }
    }
}

pub(crate) fn save_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationSaveReq,
) -> Result<RotationSaveReply, RotationSaveErr> {
    let book = ctx.setup.uses.now();
    let (sid, id, draft) = match &req.target {
        RotWhere::Session { sid } => {
            let s =
                session_of(&book, sid).ok_or_else(|| RotationSaveErr::NoSession(no_session()))?;
            (Some(sid.clone()), s.id, s.draft)
        }
        RotWhere::Strategy { strategy } => (None, strategy.clone(), None),
    };
    let sh = find(ctx, &id).ok_or_else(|| RotationSaveErr::NoStrategy(gone(&id)))?;
    let Some(head) = head_of(&sh) else {
        return Err(RotationSaveErr::NoStrategy(gone(&id)));
    };
    let settings = req
        .settings
        .clone()
        .or(draft)
        .unwrap_or_else(|| head.settings.clone());
    checked(sh.file.as_ref().ok(), &settings)
        .map_err(|r| RotationSaveErr::Refused(refused_said(), r))?;
    let mut h = head.clone();
    h.settings = settings;
    let target = match &req.save_as {
        Some(a) => fresh_name(ctx, &a.name).map_err(RotationSaveErr::BadArgs)?,
        None => {
            if req.if_rev != sh.rev {
                return Err(RotationSaveErr::Conflict(
                    copy_text("beRotation.edit.conflict", &[]),
                    RotConflict {
                        rev: sh.rev.clone(),
                    },
                ));
            }
            id.clone()
        }
    };
    let text = script::with_head(&sh.source, &h).map_err(|f| RotationSaveErr::IoFailed(f.msg))?;
    shelf::write_file(own, &target, &text).map_err(|e| RotationSaveErr::IoFailed(note(e)))?;
    let rev = shelf::rev_of(&text);
    if let Some(sid) = sid {
        let (t, r) = (target.clone(), rev.clone());
        ctx.setup
            .change(own, |b| {
                if let Some(s) = b.sessions.get_mut(&sid) {
                    s.id = t;
                    s.rev = r;
                    s.draft = None;
                }
            })
            .map_err(|e| RotationSaveErr::IoFailed(note(e)))?;
    }
    Ok(RotationSaveReply { id: target, rev })
}

// ── rotation-revert ───────────────────────────────────────────────────────────────────────

/// `rotation-revert` 的入参。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationRevertReq {
    pub(crate) sid: String,
}

crate::wire::typed::codes! {
    /// `rotation-revert` 自己的码。
    pub(crate) enum RotationRevertErr {
        NoSession = "no_session",
        IoFailed = "io_failed",
    }
}

/// `rotation-revert`：会话里没存的设置丢掉（回到文件里的）。
pub(crate) struct RotationRevert;

impl Command for RotationRevert {
    const NAME: &'static str = "rotation-revert";
    type Req = RotationRevertReq;
    type Reply = RotDone;
    type Err = RotationRevertErr;
}

impl crate::wire::typed::Handle for RotationRevert {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: RotationRevertReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotDone, RotationRevertErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (_files, own) = (needs.0, &needs.1);

            revert_with(&Ctx::here(), own, &req)
        }
    }
}

pub(crate) fn revert_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationRevertReq,
) -> Result<RotDone, RotationRevertErr> {
    let found = ctx
        .setup
        .change(own, |b| {
            b.sessions
                .get_mut(&req.sid)
                .map(|s| s.draft = None)
                .is_some()
        })
        .map_err(|e| RotationRevertErr::IoFailed(note(e)))?;
    if found {
        Ok(RotDone {})
    } else {
        Err(RotationRevertErr::NoSession(no_session()))
    }
}

// ── rotation-use ──────────────────────────────────────────────────────────────────────────

/// `rotation-use` 的入参。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationUseReq {
    pub(crate) sid: String,
    /// `null` ⇒ 这个会话不再轮换（只用开会话的号）。
    #[cfg_attr(test, ts(type = "string | null"))]
    pub(crate) strategy: Option<String>,
    /// 有没存的设置也换（丢掉它）。
    #[serde(default)]
    pub(crate) discard: bool,
}

crate::wire::typed::codes! {
    /// `rotation-use` 自己的码。
    pub(crate) enum RotationUseErr {
        NoSession = "no_session",
        NoStrategy = "no_strategy",
        /// 会话里有没存的设置、又没给 `discard`。
        Dirty = "dirty",
        IoFailed = "io_failed",
    }
}

/// `rotation-use`：会话改用另一份策略（下拉）。memo 与叫醒清掉（那是上一份策略记的）；拖出来的顺序照留。
pub(crate) struct RotationUse;

impl Command for RotationUse {
    const NAME: &'static str = "rotation-use";
    type Req = RotationUseReq;
    type Reply = RotDone;
    type Err = RotationUseErr;
}

impl crate::wire::typed::Handle for RotationUse {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: RotationUseReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotDone, RotationUseErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (_files, own) = (needs.0, &needs.1);

            use_with(&Ctx::here(), own, &req)
        }
    }
}

pub(crate) fn use_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationUseReq,
) -> Result<RotDone, RotationUseErr> {
    let s = session_of(&ctx.setup.uses.now(), &req.sid)
        .ok_or_else(|| RotationUseErr::NoSession(no_session()))?;
    let want = req.strategy.clone().unwrap_or_default();
    let rev = match &req.strategy {
        Some(id) => find(ctx, id)
            .ok_or_else(|| RotationUseErr::NoStrategy(gone(id)))?
            .rev
            .clone(),
        None => String::new(),
    };
    if s.draft.is_some() && !req.discard && s.id != want {
        return Err(RotationUseErr::Dirty(copy_text(
            "beRotation.edit.dirty",
            &[],
        )));
    }
    if s.id == want && !req.discard {
        return Ok(RotDone {});
    }
    ctx.setup
        .change(own, |b| {
            if let Some(s) = b.sessions.get_mut(&req.sid) {
                s.id.clone_from(&want);
                s.rev = rev;
                s.draft = None;
            }
        })
        .map_err(|e| RotationUseErr::IoFailed(note(e)))?;
    // 上一份策略交的 memo 与叫醒不归新的这一份。
    rotation::face_change(&ctx.hop.store, own, |b| {
        if let Some(x) = b.sessions.get_mut(&req.sid) {
            x.memo = None;
            x.wake = Default::default();
        }
    })
    .map_err(|e| RotationUseErr::IoFailed(note(e)))?;
    Ok(RotDone {})
}

// ── rotation-for-new ──────────────────────────────────────────────────────────────────────

/// `rotation-for-new` 的入参。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationForNewReq {
    /// `null` ⇒ 新会话不轮换（只用开会话的号）。
    #[cfg_attr(test, ts(type = "string | null"))]
    pub(crate) strategy: Option<String>,
}

crate::wire::typed::codes! {
    /// `rotation-for-new` 自己的码。
    pub(crate) enum RotationForNewErr {
        NoStrategy = "no_strategy",
        IoFailed = "io_failed",
    }
}

/// `rotation-for-new`：新会话用哪份。
pub(crate) struct RotationForNew;

impl Command for RotationForNew {
    const NAME: &'static str = "rotation-for-new";
    type Req = RotationForNewReq;
    type Reply = RotDone;
    type Err = RotationForNewErr;
}

impl crate::wire::typed::Handle for RotationForNew {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: RotationForNewReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotDone, RotationForNewErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (_files, own) = (needs.0, &needs.1);

            for_new_with(&Ctx::here(), own, &req)
        }
    }
}

pub(crate) fn for_new_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationForNewReq,
) -> Result<RotDone, RotationForNewErr> {
    if let Some(id) = &req.strategy {
        find(ctx, id).ok_or_else(|| RotationForNewErr::NoStrategy(gone(id)))?;
    }
    let want = req.strategy.clone().unwrap_or_default();
    ctx.setup
        .change(own, |b| b.for_new = want)
        .map_err(|e| RotationForNewErr::IoFailed(note(e)))?;
    Ok(RotDone {})
}

// ── 策略文件：加 · 复制 · 改名 · 删 ─────────────────────────────────────────────────────────

/// `rotation-strategy-add` 的入参。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationStrategyAddReq {
    /// 一份策略文件的原文（说明 ＋ 头 ＋ 代码）。
    pub(crate) source: String,
    /// 存成的文件名（＝ 标题）。
    pub(crate) name: String,
}

crate::wire::typed::codes! {
    /// `rotation-strategy-add` 自己的码。
    pub(crate) enum RotationStrategyAddErr {
        /// 那份原文读不懂（头的形状 · 编不过 · 没有 `decide` · 设置不合声明）。
        BadStrategy = "bad_strategy" => RotBadStrategy,
        IoFailed = "io_failed",
    }
}

/// `rotation-strategy-add`：从文件添加。
pub(crate) struct RotationStrategyAdd;

impl Command for RotationStrategyAdd {
    const NAME: &'static str = "rotation-strategy-add";
    type Req = RotationStrategyAddReq;
    type Reply = RotStrategyId;
    type Err = RotationStrategyAddErr;
}

impl crate::wire::typed::Handle for RotationStrategyAdd {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: RotationStrategyAddReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotStrategyId, RotationStrategyAddErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (_files, own) = (needs.0, &needs.1);

            add_with(&Ctx::here(), own, &req)
        }
    }
}

pub(crate) fn add_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationStrategyAddReq,
) -> Result<RotStrategyId, RotationStrategyAddErr> {
    let bad = |f: crate::accounts::quota::strategy::Fail| {
        RotationStrategyAddErr::BadStrategy(
            crate::common::contract::malformed(&f.msg),
            RotBadStrategy { line: f.line },
        )
    };
    script::parse(&req.source).map_err(bad)?;
    let id = fresh_name(ctx, &req.name).map_err(RotationStrategyAddErr::BadArgs)?;
    shelf::write_file(own, &id, &req.source)
        .map_err(|e| RotationStrategyAddErr::IoFailed(note(e)))?;
    Ok(RotStrategyId { id })
}

/// `rotation-strategy-copy` 的入参。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationStrategyCopyReq {
    pub(crate) id: String,
    pub(crate) name: String,
}

crate::wire::typed::codes! {
    /// `rotation-strategy-copy` 自己的码。
    pub(crate) enum RotationStrategyCopyErr {
        NoStrategy = "no_strategy",
        IoFailed = "io_failed",
    }
}

/// `rotation-strategy-copy`：复制一份（只换名字）。
pub(crate) struct RotationStrategyCopy;

impl Command for RotationStrategyCopy {
    const NAME: &'static str = "rotation-strategy-copy";
    type Req = RotationStrategyCopyReq;
    type Reply = RotStrategyId;
    type Err = RotationStrategyCopyErr;
}

impl crate::wire::typed::Handle for RotationStrategyCopy {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: RotationStrategyCopyReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotStrategyId, RotationStrategyCopyErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (_files, own) = (needs.0, &needs.1);

            copy_with(&Ctx::here(), own, &req)
        }
    }
}

pub(crate) fn copy_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationStrategyCopyReq,
) -> Result<RotStrategyId, RotationStrategyCopyErr> {
    let sh =
        find(ctx, &req.id).ok_or_else(|| RotationStrategyCopyErr::NoStrategy(gone(&req.id)))?;
    let id = fresh_name(ctx, &req.name).map_err(RotationStrategyCopyErr::BadArgs)?;
    shelf::write_file(own, &id, &sh.source)
        .map_err(|e| RotationStrategyCopyErr::IoFailed(note(e)))?;
    Ok(RotStrategyId { id })
}

/// `rotation-strategy-rename` 的入参。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationStrategyRenameReq {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) if_rev: String,
}

crate::wire::typed::codes! {
    /// `rotation-strategy-rename` 自己的码。
    pub(crate) enum RotationStrategyRenameErr {
        NoStrategy = "no_strategy",
        Conflict = "conflict" => RotConflict,
        IoFailed = "io_failed",
    }
}

/// `rotation-strategy-rename`：改名 ＝ 换文件名（内容一个字不动）；指着它的会话与「新会话用」一起改过去。
pub(crate) struct RotationStrategyRename;

impl Command for RotationStrategyRename {
    const NAME: &'static str = "rotation-strategy-rename";
    type Req = RotationStrategyRenameReq;
    type Reply = RotStrategyId;
    type Err = RotationStrategyRenameErr;
}

impl crate::wire::typed::Handle for RotationStrategyRename {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: RotationStrategyRenameReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotStrategyId, RotationStrategyRenameErr>>
           + Send
           + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (_files, own) = (needs.0, &needs.1);

            rename_with(&Ctx::here(), own, &req)
        }
    }
}

pub(crate) fn rename_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationStrategyRenameReq,
) -> Result<RotStrategyId, RotationStrategyRenameErr> {
    let sh =
        find(ctx, &req.id).ok_or_else(|| RotationStrategyRenameErr::NoStrategy(gone(&req.id)))?;
    if req.if_rev != sh.rev {
        return Err(RotationStrategyRenameErr::Conflict(
            copy_text("beRotation.edit.conflict", &[]),
            RotConflict {
                rev: sh.rev.clone(),
            },
        ));
    }
    if name_ok(&req.name).map_err(RotationStrategyRenameErr::BadArgs)? == req.id {
        return Ok(RotStrategyId { id: req.id.clone() });
    }
    let id = fresh_name(ctx, &req.name).map_err(RotationStrategyRenameErr::BadArgs)?;
    shelf::write_file(own, &id, &sh.source)
        .map_err(|e| RotationStrategyRenameErr::IoFailed(note(e)))?;
    let (from, to) = (req.id.clone(), id.clone());
    ctx.setup
        .change(own, |b| {
            if b.for_new == from {
                b.for_new = to.clone();
            }
            for s in b.sessions.values_mut() {
                if s.id == from {
                    s.id = to.clone();
                }
            }
        })
        .map_err(|e| RotationStrategyRenameErr::IoFailed(note(e)))?;
    shelf::remove_file(own, &req.id).map_err(|e| RotationStrategyRenameErr::IoFailed(note(e)))?;
    Ok(RotStrategyId { id })
}

/// `rotation-strategy-tz` 的入参：那一份 · 设成哪个时区（IANA 名）· 改之前读到的版本。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationStrategyTzReq {
    pub(crate) id: String,
    pub(crate) tz: String,
    pub(crate) if_rev: String,
}

crate::wire::typed::codes! {
    /// `rotation-strategy-tz` 自己的码。
    pub(crate) enum RotationStrategyTzErr {
        NoStrategy = "no_strategy",
        Conflict = "conflict" => RotConflict,
        IoFailed = "io_failed",
    }
}

/// `rotation-strategy-tz`：只把设置块里的 `tz` 换掉（「时区不明」那一条也清掉），说明与代码一字不动。
pub(crate) struct RotationStrategyTz;

impl Command for RotationStrategyTz {
    const NAME: &'static str = "rotation-strategy-tz";
    type Req = RotationStrategyTzReq;
    type Reply = RotStrategyRev;
    type Err = RotationStrategyTzErr;
}

impl crate::wire::typed::Handle for RotationStrategyTz {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: RotationStrategyTzReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotStrategyRev, RotationStrategyTzErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (_files, own) = (needs.0, &needs.1);

            tz_with(&Ctx::here(), own, &req)
        }
    }
}

pub(crate) fn tz_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationStrategyTzReq,
) -> Result<RotStrategyRev, RotationStrategyTzErr> {
    let sh = find(ctx, &req.id).ok_or_else(|| RotationStrategyTzErr::NoStrategy(gone(&req.id)))?;
    let Some(head) = head_of(&sh) else {
        return Err(RotationStrategyTzErr::NoStrategy(gone(&req.id)));
    };
    if crate::Tz::iana(&req.tz).is_none() {
        return Err(RotationStrategyTzErr::BadArgs(copy_text(
            "beRotation.edit.badTz",
            &[("tz", &req.tz)],
        )));
    }
    if req.if_rev != sh.rev {
        return Err(RotationStrategyTzErr::Conflict(
            copy_text("beRotation.edit.conflict", &[]),
            RotConflict {
                rev: sh.rev.clone(),
            },
        ));
    }
    let mut h = head.clone();
    h.tz = Some(req.tz.clone());
    h.tz_unknown = false;
    let text =
        script::with_head(&sh.source, &h).map_err(|f| RotationStrategyTzErr::IoFailed(f.msg))?;
    shelf::write_file(own, &req.id, &text).map_err(|e| RotationStrategyTzErr::IoFailed(note(e)))?;
    Ok(RotStrategyRev {
        rev: shelf::rev_of(&text),
    })
}

/// `rotation-strategy-delete` 的入参。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationStrategyDeleteReq {
    pub(crate) id: String,
}

crate::wire::typed::codes! {
    /// `rotation-strategy-delete` 自己的码。
    pub(crate) enum RotationStrategyDeleteErr {
        NoStrategy = "no_strategy",
        /// 有活会话在用 · 是新会话用的那份。
        InUse = "in_use" => RotInUse,
        IoFailed = "io_failed",
    }
}

/// `rotation-strategy-delete`：删一份（读不懂的也能删）。
pub(crate) struct RotationStrategyDelete;

impl Command for RotationStrategyDelete {
    const NAME: &'static str = "rotation-strategy-delete";
    type Req = RotationStrategyDeleteReq;
    type Reply = RotDone;
    type Err = RotationStrategyDeleteErr;
}

impl crate::wire::typed::Handle for RotationStrategyDelete {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: RotationStrategyDeleteReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotDone, RotationStrategyDeleteErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (_files, own) = (needs.0, &needs.1);

            let ctx = Ctx::here();
            let live = (ctx.hop.live)();
            delete_with(&ctx, own, &req, &|sid| live.contains(sid))
        }
    }
}

pub(crate) fn delete_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationStrategyDeleteReq,
    live: &dyn Fn(&str) -> bool,
) -> Result<RotDone, RotationStrategyDeleteErr> {
    let all = shelf::all(ctx.setup.dir().as_deref());
    shelf::find(&all, &req.id)
        .ok_or_else(|| RotationStrategyDeleteErr::NoStrategy(gone(&req.id)))?;
    let book = ctx.setup.uses.now();
    let sids: Vec<String> = book
        .in_use(&req.id)
        .into_iter()
        .filter(|s| live(s))
        .collect();
    if !sids.is_empty() {
        return Err(RotationStrategyDeleteErr::InUse(
            copy_text(
                "beRotation.strategy.inUse",
                &[("n", &sids.len().to_string())],
            ),
            RotInUse { sids },
        ));
    }
    if book.for_new == req.id {
        return Err(RotationStrategyDeleteErr::InUse(
            copy_text("beRotation.edit.forNew", &[]),
            RotInUse { sids: Vec::new() },
        ));
    }
    shelf::remove_file(own, &req.id).map_err(|e| RotationStrategyDeleteErr::IoFailed(note(e)))?;
    Ok(RotDone {})
}

// ── session-open 起之前那一笔 ──────────────────────────────────────────────────────────────

/// `session-open` 带 `rotation: {strategy}`：起之前在这台的账本里给那个定好的 sid 记一条、用那一份（会话一报到就已经是它）。
/// 策略目录里没有那一份 ⇒ `no_strategy`。`kind` ＝ 线上的那一家（记成它的路由名；认不出 ⇒ `bad_args`）。
pub(crate) fn preset_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    sid: &str,
    kind: &str,
    strategy: &str,
    now: u64,
) -> Result<(), (&'static str, String)> {
    if ctx.hop.store.path().is_none() {
        return Err(("io_failed", copy_text("beRotation.read.noHome", &[])));
    }
    // 空 ⇒ 起成不轮换的会话（新会话框选了「不轮换」），不去目录里找。
    if !strategy.is_empty()
        && shelf::find(&shelf::all(ctx.setup.dir().as_deref()), strategy).is_none()
    {
        return Err(("no_strategy", gone(strategy)));
    }
    let route = crate::agents::pick_route(Some(&crate::agents::AgentKind::new(kind)))
        .map_err(|say| ("bad_args", say))?;
    uses::preset(&ctx.setup, own, sid, route, strategy, now).map_err(|e| ("io_failed", note(e)))
}

/// 起不成：撤掉 [`preset_with`] 记的那一条（只撤还没换过号的；写不成只出声）。
pub(crate) fn forget_preset_with(ctx: &Ctx, own: &crate::platform::Own, sid: &str) {
    uses::forget_preset(&ctx.setup, own, sid);
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_edit_face_tests.rs"]
mod tests;
