//! 部署那一族的帧面（入参 · 回包形状 · 码）：判定与编排的本体住 `deploy` crate（`src/backend/deploy/`，桌面与手机同一份）。
//!
//! # 帧命令 `deploy-install`（本机常驻后端答）
//!
//! monitor 交三样事实：怎么够到那台（`dial`）· 自己带着哪几格后端字节（`slots`，`[{os, arch, id, path}]`：那一格自报的身份 ·
//! 落在本机哪个文件，本后端从文件读，不经帧传字节）· 那台叫什么。`deploy::install` 沿池里那条 SSH 走完 ①–④
//! （计划 · 取字节 ＋ 自检 · 放进暂存区 ＋ 读回 · 那一份自己换进落点 · 收拾残件），同手机那一趟。
//! 每走一步、收尾那一下都记进那台的状态成品（`control/machine_state.rs`，推 `changed {machine}`），结局是应答。
//! ⑤ 换常驻不在这里：monitor 接那台时 `--resident-ensure --replace`，那台自己判换不换。
//!
//! # 帧命令 `place-verdict`（同一家：本机那一份放不放）
//!
//! monitor 放本机后端之前还没有常驻后端可问 ⇒ 问手上那份字节自己（写成暂存件、跑它的 CLI 面）：表 B 本机那一行 · 落点那一份
//! vs 自己的 `BUILD_ID`（[`place_verdict`]，只住本文件：它要这份字节自己的 `BUILD_ID`）。契约那一半在 `deploy_contract`。
//!
//! # 它归 `control/` 的理由
//!
//! 会改变那台（放字节）属于控制；判定与编排在 `deploy` crate，本文件只是帧面那一层。
//!
//! # 买不到的
//!
//! - 判定与放字节之间有窗（TOCTOU）：计划出来之后那台上的文件被人换了，照计划放。
//! - 第一次连一台没钉过指纹的机器：这一趟在本机后端里拨，ack 交 monitor 去钉（`dial_host::settle_host_key`）。

use crate::platform::fs::Ro;

use copy_core::copy_text;
use deploy_contract::{DeployAction, Key, Refusal, RemoteIdentity, Route};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 判定与计划的本体住 `deploy` crate（桌面与手机同一份）；本文件只是帧面那一层（入参 · 回包形状 · 码）。
pub use deploy::{
    identity_decision, interpret_target_probe, judge, promised, stale_leftovers, DialFacing,
    Facing, Fut, Plan, TargetBinary, LEFTOVER_STALE_SECS, MARKS,
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

/// 出计划（[`deploy::plan`]，一个字节都不写）：判据经它问判定那一半。
#[cfg(test)]
pub async fn plan(
    facing: &dyn Facing,
    carried: &[(deploy_contract::Key, String)],
    machine: &str,
    now_secs: u64,
) -> Result<Plan, Fail> {
    deploy::plan(facing, carried, machine, now_secs)
        .await
        .map_err(fail_of)
}

/// `slots` 的一行：monitor 带着的那一格（哪种机器 · 字节自报的身份 · 落在本机哪个文件）。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SlotRow {
    pub(crate) os: String,
    pub(crate) arch: String,
    pub(crate) id: String,
    /// 本机上那一格字节的文件（绝对路径；本后端读它，不经帧传字节）。
    pub(crate) path: String,
}

/// `slots` 那几行 ⇒ `deploy::Slot`。键认不出 / 身份空 / 路径不是绝对 ⇒ `bad_args`（monitor 交的是它自己表里的词，认不出就是两侧漂了）。
pub(crate) fn slots_of(rows: &[SlotRow]) -> Result<Vec<deploy::Slot>, Fail> {
    let bad = |m: &str| Fail::new("bad_args", crate::common::contract::malformed(m));
    rows.iter()
        .map(|r| {
            let key = deploy_contract::key_of(&r.os, &r.arch)
                .map_err(|_| bad("`slots` row names a machine outside table A"))?;
            if r.id.is_empty() {
                return Err(bad("`slots` row without `id`"));
            }
            if !std::path::Path::new(&r.path).is_absolute() {
                return Err(bad("`slots` row without an absolute `path`"));
            }
            Ok(deploy::Slot {
                key,
                id: r.id.clone(),
                path: r.path.clone().into(),
            })
        })
        .collect()
}

/// `deploy-install` 的入参：怎么够到那台 · monitor 带着的那几格 · 对人说话时那台叫什么。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct DeployInstallReq {
    pub(crate) dial: crate::dial::pick::MachineDial,
    pub(crate) slots: Vec<SlotRow>,
    /// 那台的名字（非空；进度帧 `deploy.machine` 也是它）。
    pub(crate) machine: String,
}

/// `deploy-install` 的应答：这一趟做了什么 · 那一句 · 落点上现在是哪一版 · 那台是什么 OS · 问 `uname` 那一趟拨号的 ack。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Installed {
    /// `skip`（已是这一版）· `deploy`（放上了这一版）· `keep`（那台不比这一版旧 ⇒ 一个字节没写）。
    #[cfg_attr(test, ts(type = "\"skip\" | \"deploy\" | \"keep\""))]
    pub(crate) action: &'static str,
    /// 这一趟做了什么那一句（`deploy::Plan::said`）。
    pub(crate) said: String,
    /// 落点上现在那一份自报的身份（`keep` ⇒ 那台上那一份的；否则这一版的）。
    pub(crate) build: String,
    /// 那台的 OS（表 A 的词）。
    pub(crate) os: &'static str,
    /// 问 `uname` 那一趟拨号的 ack 原样（逐地址指纹 · 严格与否，拨号核心 `DialAck` 那一形）：monitor 按它固化指纹。
    #[cfg_attr(test, ts(type = "unknown"))]
    pub(crate) ack: Value,
}

/// 装完的那份计划 → 应答。
pub(crate) fn installed_of(p: &Plan) -> Installed {
    let (action, build) = match &p.action {
        DeployAction::Skip => ("skip", p.expected.clone()),
        DeployAction::Deploy(_) => ("deploy", p.expected.clone()),
        DeployAction::Keep { theirs, .. } => ("keep", theirs.clone()),
    };
    Installed {
        action,
        said: p.said(),
        build,
        os: p.key.os.label(),
        ack: p.ack.clone(),
    }
}

/// `deploy-install` 的本体（对面由调用方给）：每走一步交 `on`，结局是应答。
pub async fn install(
    req: &DeployInstallReq,
    host: &dyn deploy::Place,
    on: &(dyn Fn(deploy::Step) + Send + Sync),
) -> Result<Installed, Fail> {
    let slots = slots_of(&req.slots)?;
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
    let load = |p: &std::path::Path| Ro::read(p);
    deploy::install(host, &slots, &load, &req.machine, now, on)
        .await
        .map(|p| installed_of(&p))
        .map_err(fail_of)
}

crate::wire::typed::codes! {
    /// `deploy-install` 的码：读写那台没成 · 那一格这一版不承诺 / 没带 · 够不到那台 · 那台那一份说不清自己是谁 ·
    /// 推不上去（围栏拒）· 那一份没换进落点。
    pub(crate) enum DeployInstallErr {
        IoFailed = "io_failed",
        Refused = "refused",
        Unreachable = "unreachable",
        Undecidable = "undecidable",
        NotCarried = "not_carried",
        Fenced = "fenced",
        PlaceFailed = "place_failed",
    }
}

/// `deploy-install`：给那台装 / 升级后端。真异步（拨号 / 等远端）。
pub(crate) struct DeployInstall;

impl crate::wire::typed::Command for DeployInstall {
    const NAME: &'static str = "deploy-install";
    type Req = DeployInstallReq;
    type Reply = Installed;
    type Err = DeployInstallErr;
}

impl crate::wire::typed::Handle for DeployInstall {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: DeployInstallReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Installed, DeployInstallErr>> + Send + 'static
    {
        answer_install(req)
    }
}

/// 生产的 `deploy-install`：真拨号；每一步与收尾记进那台的状态成品。
pub(crate) async fn answer_install(req: DeployInstallReq) -> Result<Installed, DeployInstallErr> {
    let dial = req
        .dial
        .stored()
        .map_err(|(_, said)| DeployInstallErr::BadArgs(said))?;
    let to = crate::dial::machine::resolve(&dial)
        .map_err(|(_, said)| DeployInstallErr::BadArgs(said))?;
    // 拨号核心那一套（钥匙 · known_hosts · 写规矩）先装上，再开那条连接。
    crate::dial::host();
    // 一次性命令走全后端那一处（`remote_ask::capture_full`，池里那条连接上多开一个 exec）。
    let exec: deploy::Exec = Box::new(move |command| {
        let dial = dial.clone();
        Box::pin(async move { crate::dial::remote_ask::capture_full(&dial, command).await })
    });
    let facing = DialFacing::new(to, exec);
    let machine = req.machine.clone();
    let got = install(&req, &facing, &move |s| {
        crate::control::machine_state::note(&machine, deploy::machine::Fact::Step(s));
    })
    .await;
    if !req.machine.trim().is_empty() {
        let os = got.as_ref().ok().map(|i| i.os.to_string());
        crate::control::machine_state::note(&req.machine, deploy::machine::Fact::Deployed { os });
    }
    got.map_err(|f| crate::wire::typed::of_fail(f, DeployInstallErr::IoFailed))
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
