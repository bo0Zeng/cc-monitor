//! 部署那一族的帧面（入参 · 回包形状 · 码）：判定与计划的本体住 `deploy` crate（`src/backend/deploy/`，桌面与手机同一份），monitor 只放字节。
//!
//! # 帧命令 `deploy-plan`（本机常驻后端答）
//!
//! monitor 交两样事实：怎么够到那台（`dial`，与 `files` 链路同一份拨号请求）· 自己带着哪几格后端字节、各自自报的身份
//! （`carried`，`[{os, arch, id}]`，放字节的一侧才知道）。`deploy::plan` 沿池里那条 SSH 问那台：
//!
//! 1. `uname -s -m` → 表 A / 表 B（[`judge`]）→ 这一版带没带那一格（`carried`）—— **换成什么**；拒绝点在写第一个字节之前；
//! 2. 落点那一份是谁：stat（没有 / 0 字节就不必再问）→ 扫它字节里的身份戳（一次 exec，不跑它）—— **身份判定**；
//! 3. **该不该换**（[`identity_decision`]：只升不降；不说自己是谁 ⇒ 显式失败、不动）；
//! 4. 落点那个目录里上一趟没收拾掉的临时件 / 备份件（[`stale_leftovers`]）—— 每次连上都问一次，交 monitor 删。
//!
//! 回计划，一个字节都不写：放字节（mkdir · 原子上传 ＋ 读回比对，`ssh_dial::sftp::put_verified`）与删残件是 monitor 经 `files` 链路做。
//!
//! # 帧命令 `resident-verdict`（同一家）
//!
//! 远端常驻后端 hello 报的 build 比 monitor 手上这一版旧 ⇒ 换一次（[`resident_verdict`]）；monitor 只照做（重连循环 `resident_client::feed` 换装那一步）。
//!
//! # 帧命令 `place-verdict`（同一家：本机那一份放不放）
//!
//! monitor 放本机后端之前还没有常驻后端可问 ⇒ 问手上那份字节自己（写成暂存件、跑它的 CLI 面）：表 B 本机那一行 · 落点那一份
//! vs 自己的 `BUILD_ID`（[`place_verdict`]，只住本文件：它要这份字节自己的 `BUILD_ID`）。契约那一半在 `deploy_contract`。
//!
//! # 它归 `control/` 的理由
//!
//! 同 [`super::resolve_query`]：产「要怎么改变世界」的计划属于控制的前半；它自己只读（stat · read · 两条只读 exec）。
//!
//! # 买不到的
//!
//! - 判定与放字节之间有窗（TOCTOU）：计划出来之后那台上的文件被人换了，monitor 照计划放。
//! - 第一次连一台没钉过指纹的机器：这一问在本机后端里拨，ack 里那枚指纹这一跳不交 monitor 去钉；
//!   紧接着 monitor 开 `files` 链路（池里同一条连接）那一跳照旧钉（`dial_host::settle_host_key`）。

use crate::platform::fs::Ro;

use copy_core::copy_text;
use deploy_contract::{DeployAction, Key, Refusal, RemoteIdentity, Route};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 判定与计划的本体住 `deploy` crate（桌面与手机同一份）；本文件只是帧面那一层（入参 · 回包形状 · 码）。
pub use deploy::{
    identity_decision, interpret_target_probe, judge, promised, resident_verdict, stale_leftovers,
    DialFacing, Facing, Fut, Plan, TargetBinary, Verdict, LEFTOVER_STALE_SECS, MARKS,
};

/// 这一串的失败：码 ＋ 那一句 ＋ 下层原话（那台答 `uname` 的原话进复制详情，不上句子）。
pub(crate) use crate::wire::spec::Fail;

/// `deploy` crate 的失败 ⇒ 帧面的失败（码 · 那一句 · 原话原样）。
fn fail_of(f: deploy::Fail) -> Fail {
    Fail::new(f.code, f.said).with_raw(f.raw.as_deref())
}

/// 一形拒绝 ⇒ `refused`（句子是 `Refusal::say`，那台答的原话随失败交出去）。
fn refused(r: &Refusal, machine: &str) -> Fail {
    fail_of(deploy::refused(r, machine))
}

/// 出计划（[`deploy::plan`]）。`machine` = monitor 交来的那台的名字（只用来说话）；`now_secs` = 此刻（判残件新旧）。
pub async fn plan(
    facing: &dyn Facing,
    carried: &[(Key, String)],
    machine: &str,
    now_secs: u64,
) -> Result<Plan, Fail> {
    deploy::plan(facing, carried, machine, now_secs)
        .await
        .map_err(fail_of)
}

/// `deploy-plan` 的应答：换成哪一格 · 落点那一份是谁 · 该不该换 · 旧落点那份删不删。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Planned {
    /// 那台是表 A 的哪一格（`label` 说给人听：`Linux / x86_64`）。
    pub(crate) os: &'static str,
    pub(crate) arch: &'static str,
    pub(crate) label: String,
    /// 那一格这一版带着的字节自报的身份（对照物）。
    pub(crate) expected: String,
    /// `skip`（已是这一版）· `deploy`（没装 / 0 字节 / 更旧）· `keep`（另一版、不比这一版旧 ⇒ 不动它）。
    #[cfg_attr(test, ts(type = "\"skip\" | \"deploy\" | \"keep\""))]
    pub(crate) action: &'static str,
    /// 人读原因（`skip` 时空串）。
    pub(crate) why: String,
    /// `keep` 时那台上那一份自报的身份，否则 `null`。
    pub(crate) theirs: Option<String>,
    /// 落点目录里没人要的上传残件（家目录相对，排序）；列不出那个目录 ⇒ `[]`（下次连上再问）。
    pub(crate) leftovers: Vec<String>,
    /// 问 `uname` 那一趟拨号的 ack 原样（逐地址指纹 · 严格与否，拨号核心 `DialAck` 那一形）：monitor 按它固化指纹。
    #[cfg_attr(test, ts(type = "unknown"))]
    pub(crate) ack: Value,
}

/// 计划 → 帧面那一格。
pub fn plan_json(p: &Plan) -> Planned {
    let (action, why, theirs) = match &p.action {
        DeployAction::Skip => ("skip", String::new(), None),
        DeployAction::Deploy(why) => ("deploy", why.clone(), None),
        DeployAction::Keep { theirs, why } => ("keep", why.clone(), Some(theirs.clone())),
    };
    Planned {
        os: p.key.os.label(),
        arch: p.key.arch.label(),
        label: p.key.label(),
        expected: p.expected.clone(),
        action,
        why,
        theirs,
        leftovers: p.leftovers.clone(),
        ack: p.ack.clone(),
    }
}

/// `carried` 的一行：monitor 手上带着的那一格字节自报的身份。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct Carried {
    pub(crate) os: String,
    pub(crate) arch: String,
    pub(crate) id: String,
}

/// `carried` 那几行 ⇒ 表 A 的键 ＋ 身份。键认不出 / 身份空 ⇒ `bad_args`（monitor 交的是它自己表里的词，认不出就是两侧漂了）。
pub fn carried_of(rows: &[Carried]) -> Result<Vec<(Key, String)>, Fail> {
    let bad = |m: &str| Fail::new("bad_args", crate::common::contract::malformed(m));
    rows.iter()
        .map(|r| {
            let key = deploy_contract::key_of(&r.os, &r.arch)
                .map_err(|_| bad("`carried` row names a machine outside table A"))?;
            match r.id.as_str() {
                "" => Err(bad("`carried` row without `id`")),
                id => Ok((key, id.to_string())),
            }
        })
        .collect()
}

/// `deploy-plan` 的入参：怎么够到那台 · monitor 手上带着的那几格 · 对人说话时那台叫什么。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct DeployPlanReq {
    pub(crate) dial: crate::dial::pick::MachineDial,
    pub(crate) carried: Vec<Carried>,
    /// 那台的名字（非空）。
    pub(crate) machine: String,
}

/// `deploy-plan` 的本体（对面由调用方给）。
pub async fn answer(req: &DeployPlanReq, facing: &dyn Facing) -> Result<Planned, Fail> {
    let carried = carried_of(&req.carried)?;
    if req.machine.trim().is_empty() {
        return Err(Fail::new(
            "bad_args",
            crate::common::contract::malformed("missing `machine` (string)"),
        ));
    }
    let now = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    plan(facing, &carried, &req.machine, now)
        .await
        .map(|p| plan_json(&p))
}

crate::wire::typed::codes! {
    /// `deploy-plan` 的码：读那台没读成 · 那一格这一版不承诺 · 够不到那台 · 那台那一份说不清自己是谁。
    pub(crate) enum DeployPlanErr {
        IoFailed = "io_failed",
        Refused = "refused",
        Unreachable = "unreachable",
        Undecidable = "undecidable",
    }
}

/// `deploy-plan`：那台的后端要不要换、换成哪一格。真异步（拨号 / 等远端）；一个字节都不写（放字节是 monitor 经 `files` 链路的事）。
pub(crate) struct DeployPlan;

impl crate::wire::typed::Command for DeployPlan {
    const NAME: &'static str = "deploy-plan";
    type Req = DeployPlanReq;
    type Reply = Planned;
    type Err = DeployPlanErr;
}

impl crate::wire::typed::Handle for DeployPlan {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: DeployPlanReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Planned, DeployPlanErr>> + Send + 'static {
        async move {
            let dial = req
                .dial
                .stored()
                .map_err(|(_, said)| DeployPlanErr::BadArgs(said))?;
            let to = crate::dial::machine::resolve(&dial)
                .map_err(|(_, said)| DeployPlanErr::BadArgs(said))?;
            // 拨号核心那一套（钥匙 · known_hosts · 写规矩）先装上，再开那条连接。
            crate::dial::host();
            // 一次性命令走全后端那一处（`remote_ask::capture_full`，池里那条连接上多开一个 exec）。
            let exec: deploy::Exec = Box::new(move |command| {
                let dial = dial.clone();
                Box::pin(async move { crate::dial::remote_ask::capture_full(&dial, command).await })
            });
            let facing = DialFacing::new(to, exec);
            answer(&req, &facing)
                .await
                .map_err(|f| crate::wire::typed::of_fail(f, DeployPlanErr::IoFailed))
        }
    }
}

// ═══ 远端常驻后端 hello 的新旧（帧命令 `resident-verdict`）═══════════════════════════
//
// monitor 接远端常驻后端时读到 hello，「那台比手上这一版旧 ⇒ 换一次」在这里判，monitor 只照做。

/// `resident-verdict` 的入参：手上这一版 · 那台 hello 报的 · 这一趟换过没有（只有事实）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct VerdictReq {
    /// monitor 手上这一版后端自报的身份（非空）。
    pub(crate) mine: String,
    /// 那台 hello 报的 `build_id`（缺 ⇒ 空串）。
    pub(crate) theirs: String,
    /// 这一趟是不是已经换过一次。
    pub(crate) replaced: bool,
}

/// `resident-verdict` 的回包。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct VerdictReply {
    /// 换掉再接 · 接上它。
    pub(crate) action: VerdictAction,
    /// 那台是不是比手上这一版旧（与 `replaced` 无关；版本提示那句话按它挑「会换掉」还是「不会换回去」）。
    pub(crate) older: bool,
}

/// [`VerdictReply::action`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub(crate) enum VerdictAction {
    Replace,
    Attach,
}

crate::wire::typed::codes! {
    /// `resident-verdict` 的码：`mine` 是空串。
    pub(crate) enum VerdictErr {}
}

/// `resident-verdict`：远端常驻后端 hello 的新旧（纯判定，不碰盘不拨号 ⇒ 不进阻塞档）。
pub(crate) struct ResidentVerdict;

impl crate::wire::typed::Command for ResidentVerdict {
    const NAME: &'static str = "resident-verdict";
    type Req = VerdictReq;
    type Reply = VerdictReply;
    type Err = VerdictErr;
}

impl crate::wire::typed::Handle for ResidentVerdict {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: VerdictReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<VerdictReply, VerdictErr>> + Send + 'static {
        std::future::ready(answer_resident_verdict(&req))
    }
}

/// [`ResidentVerdict`] 的本体：`mine` 空 ⇒ `bad_args`。
pub(crate) fn answer_resident_verdict(req: &VerdictReq) -> Result<VerdictReply, VerdictErr> {
    if req.mine.is_empty() {
        return Err(VerdictErr::BadArgs(crate::common::contract::malformed(
            "missing `mine` (non-empty string)",
        )));
    }
    let v = resident_verdict(&req.mine, &req.theirs, req.replaced);
    Ok(VerdictReply {
        action: if v.replace {
            VerdictAction::Replace
        } else {
            VerdictAction::Attach
        },
        older: v.older,
    })
}

// ═══ 本机那一份放不放（帧命令 `place-verdict`）═══════════════════════════════════════════
//
// 本机常驻后端放下去之前没有后端可问 —— 可「`ccm` 就是后端本体」：monitor 手上那份字节就是一个后端。
// monitor 把它写成暂存件、跑 `<暂存件> -- --place-verdict` 问一次（CLI 面自动派生），照答放或不放（`local_backend::extract_embedded_to`）；
// 判定（表 B 本机那一行 · 落点那一份 vs 自己的 `BUILD_ID`，只升不降）只在这里。
// 本机 (Linux, aarch64) 的「不承诺」落在写暂存件之后（不是写第一个字节之前）：问完即删、净足迹零。

/// 本机那一份的去向。
#[derive(Debug, PartialEq, Eq)]
pub enum Placed {
    /// 放（换）上去；`why` = 人读原因。
    Place(String),
    /// 不动、用盘上那一份（它不比这一份旧）；`why` 点名两边各是哪一版。
    Keep(String),
}

/// **纯函数**：`me` = 这份字节自己的键（就是这台）· `disk` = 落点那个文件（`Ok(None)` = 不在 · `Err` = 读不了）· `mine` = 自己的 `BUILD_ID`。
/// 只在「盘上与手上逐字节不同」时被问（相同那一形 monitor 直接用、不问）⇒ 同一版那一格也是放（开发树重编：同 id、不同字节）。
/// 拒：`refused`（表 A / 表 B，那句话是 `Refusal::say`）· `undecidable`（落点那一份不说自己是谁 / 身份不唯一 / 读不了）。
pub fn place_verdict(
    me: Result<Key, Refusal>,
    disk: Result<Option<Vec<u8>>, String>,
    mine: &str,
    machine: &str,
    dest: &str,
) -> Result<Placed, Fail> {
    judge(Route::Local, me).map_err(|r| refused(&r, machine))?;
    let id = match disk {
        Ok(None) => RemoteIdentity::Missing,
        Ok(Some(b)) => deploy_contract::identity_of_bytes(&b, MARKS),
        Err(e) => RemoteIdentity::Unreadable(e),
    };
    match identity_decision(&id, mine, machine, dest).map_err(|e| Fail::new("undecidable", e))? {
        DeployAction::Deploy(why) => Ok(Placed::Place(why)),
        DeployAction::Skip => Ok(Placed::Place(copy_text("bePlaceVerdict.why.rebuilt", &[]))),
        DeployAction::Keep { why, .. } => Ok(Placed::Keep(why)),
    }
}

/// `place-verdict` 的入参：落点的绝对路径 · 对人说话时这台叫什么（monitor 交「本机」，不是机器名）。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PlaceReq {
    /// 不在 ⇒ 没装 ⇒ 放；读不了 ⇒ `undecidable`，不当成没装。
    pub(crate) dest: String,
    pub(crate) machine: String,
}

/// `place-verdict` 的应答。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlaceAnswer {
    /// `place`（放 / 换上去）· `keep`（盘上那一份不比这一份旧 ⇒ 不动、用它）。
    #[cfg_attr(test, ts(type = "\"place\" | \"keep\""))]
    pub(crate) action: &'static str,
    /// 人读原因（`keep` 时点名两边各是哪一版）。
    pub(crate) why: String,
}

/// `place-verdict` 的本体：`dest` 不是绝对路径 · `machine` 空 ⇒ `bad_args`。
/// 只读落点那一个文件（不在 ⇒ 没装）；这份字节自己的键与身份取自编译期（`Key::this_machine` · `crate::BUILD_ID`）。
pub fn answer_place(req: &PlaceReq) -> Result<PlaceAnswer, Fail> {
    let bad = |m: &str| Fail::new("bad_args", crate::common::contract::malformed(m));
    if !std::path::Path::new(&req.dest).is_absolute() {
        return Err(bad("missing `dest` (absolute path)"));
    }
    if req.machine.is_empty() {
        return Err(bad("missing `machine` (non-empty string)"));
    }
    let disk = match Ro::read(&req.dest) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    };
    Ok(
        match place_verdict(
            Key::this_machine(),
            disk,
            crate::BUILD_ID,
            &req.machine,
            &req.dest,
        )? {
            Placed::Place(why) => PlaceAnswer {
                action: "place",
                why,
            },
            Placed::Keep(why) => PlaceAnswer {
                action: "keep",
                why,
            },
        },
    )
}

crate::wire::typed::codes! {
    /// `place-verdict` 的码：这一格不承诺 · 落点那一份说不清自己是谁 / 读不了。
    pub(crate) enum PlaceErr {
        Refused = "refused",
        Undecidable = "undecidable",
    }
}

/// `place-verdict`：本机那一份放不放（只读落点那一个文件 ⇒ 阻塞档）。
pub(crate) struct PlaceVerdict;

impl crate::wire::typed::Command for PlaceVerdict {
    const NAME: &'static str = "place-verdict";
    type Req = PlaceReq;
    type Reply = PlaceAnswer;
    type Err = PlaceErr;
}

impl crate::wire::typed::Handle for PlaceVerdict {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: PlaceReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<PlaceAnswer, PlaceErr>> + Send + 'static {
        async move {
            answer_place(&req).map_err(|f| crate::wire::typed::of_fail(f, PlaceErr::Undecidable))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/deploy_plan_tests.rs"]
mod tests;
