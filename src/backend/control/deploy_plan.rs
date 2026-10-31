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
//! # 帧命令 `deploy-uninstall`（同一家）
//!
//! 入参同上；SFTP 只写暂存区 ⇒ 传一份暂存件上去、由它删落点那一份（`deploy::uninstall`），回 `{removed, said}`。
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

use crate::platform::fs::Ro;

/// 装 · 卸那两条的入参与回包住 cmd-wire（名字与回包挂在入参上，monitor 照同一份发、照同一份解）。
pub(crate) use cmd_wire::deploy::{
    DeployInstallReq, DeployUninstallReq, Installed, SlotRow, Uninstalled,
};
use copy_core::copy_text;
use deploy_contract::{DeployAction, Key, Refusal, RemoteIdentity, Route};
use serde::{Deserialize, Serialize};

/// 判定与计划的本体住 `deploy` crate（桌面与手机同一份）；本文件只是帧面那一层（入参 · 回包形状 · 码）。
pub use deploy::{identity_decision, judge, DialFacing, Plan, MARKS};

use crate::common::said::Said;

/// `deploy` crate 的失败 ⇒ 帧面的失败（码 · 那一句 · 原话原样）。
fn fail_of(f: deploy::Fail) -> Said {
    crate::wire::spec::fail(f.code, f.said).and_raw(f.raw.as_deref())
}

/// 一形拒绝 ⇒ `refused`（句子是 `Refusal::say`，那台答的原话随失败交出去）。
fn refused(r: &Refusal, machine: &str) -> Said {
    fail_of(deploy::refused(r, machine))
}

/// 出计划（[`deploy::plan`]，一个字节都不写）：判据经它问判定那一半。
#[cfg(test)]
pub async fn plan(
    facing: &dyn deploy::Facing,
    carried: &[(deploy_contract::Key, String)],
    machine: &str,
    now_secs: u64,
) -> Result<Plan, Said> {
    deploy::plan(facing, carried, machine, now_secs)
        .await
        .map_err(fail_of)
}

/// `slots` 那几行 ⇒ `deploy::Slot`。键认不出 / 身份空 / 路径不是绝对 ⇒ `bad_args`（monitor 交的是它自己表里的词，认不出就是两侧漂了）。
pub(crate) fn slots_of(rows: &[SlotRow]) -> Result<Vec<deploy::Slot>, Said> {
    let bad = |m: &str| crate::wire::spec::fail("bad_args", crate::common::contract::malformed(m));
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

/// monitor 落在盘上的那几格（本机后端落点旁边的 `slots/<os>-<arch>`，monitor 起来时落）⇒ 带着的那几格（各台的常驻流装后端那一步读它）。
/// 那一格的身份照字节自报的那一个（[`deploy_contract::identity_of_bytes`]，同装那一趟自检的读法）；名字认不出 · 字节里没有恰好一个身份 ⇒ 不算带着。
/// 家目录问不到 · 目录不在 ⇒ 那一句（不装，照样接已有的后端）。
pub(crate) fn carried_slots() -> Result<Vec<SlotRow>, String> {
    let home = crate::platform::paths::home_dir().ok_or("no home directory")?;
    carried_slots_in(&home)
}

/// [`carried_slots`] 的本体（家目录由调用方给）。
pub(crate) fn carried_slots_in(home: &std::path::Path) -> Result<Vec<SlotRow>, String> {
    let (landing_dir, _) = relay_route_core::BACKEND_LANDING_REL
        .rsplit_once('/')
        .ok_or("landing has no directory")?;
    let dir = landing_dir
        .split('/')
        .fold(home.to_path_buf(), |p, seg| p.join(seg))
        .join("slots");
    let entries = Ro::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut rows = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some((os, arch)) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.split_once('-'))
        else {
            continue;
        };
        let Ok(key) = deploy_contract::key_of(os, arch) else {
            continue;
        };
        let Ok(bytes) = Ro::read(&path) else {
            continue;
        };
        if let RemoteIdentity::Stamp(id) = deploy_contract::identity_of_bytes(&bytes, MARKS) {
            rows.push(SlotRow {
                os: key.os.label().to_string(),
                arch: key.arch.label().to_string(),
                id,
                path: path.display().to_string(),
            });
        }
    }
    rows.sort_by(|a, b| (&a.os, &a.arch).cmp(&(&b.os, &b.arch)));
    Ok(rows)
}

/// 装完的那份计划 → 应答。
pub(crate) fn installed_of(p: &Plan) -> Installed {
    let build = match &p.action {
        DeployAction::Keep { theirs, .. } => theirs.clone(),
        DeployAction::Skip | DeployAction::Deploy(_) => p.expected.clone(),
    };
    Installed {
        action: p.action.tag(),
        said: p.said(),
        build,
        os: p.key.os.label().to_string(),
    }
}

/// 入参那几行 ⇒ `deploy::Slot`；`machine` 空 ⇒ `bad_args`。
fn checked(rows: &[SlotRow], machine: &str) -> Result<Vec<deploy::Slot>, Said> {
    let slots = slots_of(rows)?;
    if machine.trim().is_empty() {
        return Err(crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("missing `machine` (string)"),
        ));
    }
    Ok(slots)
}

/// 读一格字节（本机文件）。
fn load(p: &std::path::Path) -> std::io::Result<Vec<u8>> {
    Ro::read(p)
}

/// `deploy-install` 的本体（对面由调用方给）：每走一步交 `on`，结局是应答。
pub async fn install(
    req: &DeployInstallReq,
    host: &dyn deploy::Place,
    on: &(dyn Fn(deploy::Step) + Send + Sync),
) -> Result<Installed, Said> {
    let slots = checked(&req.slots, &req.machine)?;
    let now = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
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
    crate::wire::typed::by_call!(DeployInstallReq);
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

/// 真拨那台的对面（装 · 卸同一个）：拨号核心那一套（钥匙 · known_hosts · 写规矩）先装上；一次性命令走全后端那一处
/// （`remote_ask::capture_full`，池里那条连接上多开一个 exec）。机器表里没有那台 · 组不成请求 ⇒ 那一句（`bad_args`）。
fn facing_of(machine: &str) -> Result<DialFacing, String> {
    let hosts = crate::control::machines::here();
    let (dial, to) =
        crate::dial::named_both(&hosts, machine.trim(), "capture", serde_json::json!({}))
            .map_err(|f| f.said)?;
    crate::dial::host();
    let exec: deploy::Exec = Box::new(move |command| {
        let dial = dial.clone();
        Box::pin(async move { crate::dial::remote_ask::capture_full(&dial, command).await })
    });
    Ok(DialFacing::new(to, exec))
}

/// 生产的 `deploy-install`：真拨号；每一步与收尾记进那台的状态成品。
pub(crate) async fn answer_install(req: DeployInstallReq) -> Result<Installed, DeployInstallErr> {
    let facing = facing_of(&req.machine).map_err(DeployInstallErr::BadArgs)?;
    let machine = req.machine.clone();
    let got = install(&req, &facing, &move |s| {
        crate::control::machine_state::note(&machine, deploy::machine::Fact::Step(s));
    })
    .await;
    if !req.machine.trim().is_empty() {
        let os = got.as_ref().ok().map(|i| i.os.clone());
        crate::control::machine_state::note(&req.machine, deploy::machine::Fact::Deployed { os });
        // 没换上那一句记进那台的成品（机器条那一行照它说，不另弹 toast）。
        if let Err(f) = &got {
            crate::control::machine_state::note(
                &req.machine,
                deploy::machine::Fact::UpdateFailed {
                    said: f.said.clone(),
                },
            );
        }
    }
    got.map_err(|f| crate::wire::typed::of_fail(f, DeployInstallErr::IoFailed))
}

// ═══ 卸载（帧命令 `deploy-uninstall`）═══════════════════════════════════════════════════════
//
// SFTP 只写暂存区 ⇒ 卸载也传一份暂存件上去，由它删落点那一份（`deploy::uninstall`）。入参那两格同 `deploy-install`。

/// `deploy-uninstall` 的本体（对面由调用方给）。
pub async fn uninstall(
    req: &DeployUninstallReq,
    host: &dyn deploy::Place,
) -> Result<Uninstalled, Said> {
    let slots = checked(&req.slots, &req.machine)?;
    deploy::uninstall(host, &slots, &load, &req.machine)
        .await
        .map(|r| Uninstalled {
            removed: r.removed,
            said: r.said,
        })
        .map_err(fail_of)
}

crate::wire::typed::codes! {
    /// `deploy-uninstall` 的码：读写那台没成 · 那一格这一版不承诺 / 没带 · 够不到那台 · 推不上去（围栏拒）· 那一份没删成。
    pub(crate) enum DeployUninstallErr {
        IoFailed = "io_failed",
        Refused = "refused",
        Unreachable = "unreachable",
        NotCarried = "not_carried",
        Fenced = "fenced",
        PlaceFailed = "place_failed",
    }
}

/// `deploy-uninstall`：删那台的后端（真异步：拨号 / 等远端）。
pub(crate) struct DeployUninstall;

impl crate::wire::typed::Command for DeployUninstall {
    crate::wire::typed::by_call!(DeployUninstallReq);
    type Err = DeployUninstallErr;
}

impl crate::wire::typed::Handle for DeployUninstall {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: DeployUninstallReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Uninstalled, DeployUninstallErr>> + Send + 'static
    {
        async move {
            let facing = facing_of(&req.machine).map_err(DeployUninstallErr::BadArgs)?;
            uninstall(&req, &facing)
                .await
                .map_err(|f| crate::wire::typed::of_fail(f, DeployUninstallErr::IoFailed))
        }
    }
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
) -> Result<Placed, Said> {
    judge(Route::Local, me).map_err(|r| refused(&r, machine))?;
    let id = match disk {
        Ok(None) => RemoteIdentity::Missing,
        Ok(Some(b)) => deploy_contract::identity_of_bytes(&b, MARKS),
        Err(e) => RemoteIdentity::Unreadable(e),
    };
    match identity_decision(&id, mine, machine, dest).map_err(|e| crate::wire::spec::fail("undecidable", e))? {
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
pub fn answer_place(req: &PlaceReq) -> Result<PlaceAnswer, Said> {
    let bad = |m: &str| crate::wire::spec::fail("bad_args", crate::common::contract::malformed(m));
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
