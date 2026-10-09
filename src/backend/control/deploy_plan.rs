//! 部署决策（该不该换 · 换成什么 · 身份判定）：monitor 只放字节，判定都在这里。
//!
//! # 帧命令 `deploy-plan`（本机常驻后端答）
//!
//! monitor 交两样事实：怎么够到那台（`dial`，与 `files` 链路同一份拨号请求）· 自己带着哪几格后端字节、各自自报的身份
//! （`carried`，`[{os, arch, id}]`，放字节的一侧才知道）。本模块沿池里那条 SSH 问那台：
//!
//! 1. `uname -s -m`（[`deploy_contract::key_from_uname`]）→ 表 A / 表 B（[`judge`]）→ 这一版带没带那一格（`carried`）——
//!    **换成什么**；拒绝点在写第一个字节之前；
//! 2. 落点那一份是谁：stat（没有 / 0 字节就不必再问）→ 扫它字节里的身份戳（一次 exec，不跑它）—— **身份判定**；
//! 3. **该不该换**（[`identity_decision`]：只升不降；不说自己是谁 ⇒ 显式失败、不动）；
//! 4. 落点那个目录里上一趟没收拾掉的临时件 / 备份件（[`stale_leftovers`]）—— 每次连上都问一次，交 monitor 删。
//!
//! 回计划，一个字节都不写：放字节（mkdir · 原子上传 · 读回比对）与删残件是 monitor 经 `files` 链路做。
//!
//! # 帧命令 `resident-verdict`（同一家）
//!
//! 远端常驻后端 hello 报的 build 比 monitor 手上这一版旧 ⇒ 换一次（[`resident_verdict`]）；monitor 只照做（`remote_resident::attach`）。
//!
//! # 帧命令 `place-verdict`（同一家：本机那一份放不放）
//!
//! monitor 放本机后端之前还没有常驻后端可问 ⇒ 问手上那份字节自己（写成暂存件、跑它的 CLI 面）：表 B 本机那一行 · 落点那一份
//! vs 自己的 `BUILD_ID`（[`place_verdict`]）。判定（承诺 · 换不换 · 认不认 · 取样解释）只住本文件；
//! 契约那一半在 `deploy_contract`。
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

use std::future::Future;
use std::pin::Pin;

use copy_core::copy_text;
use deploy_contract::{Arch, DeployAction, Key, Marks, Os, Refusal, RemoteIdentity, Route, LINES};
use serde_json::{json, Value};

/// 这一串的失败：码 ＋ 那一句 ＋ 下层原话（那台答 `uname` 的原话进复制详情，不上句子）。
pub(crate) use crate::stream::inbound::spec::Fail;

/// 身份戳的两个界标（住契约 crate）。
const MARKS: Marks<'static> = Marks {
    open: deploy_contract::STAMP_OPEN,
    close: deploy_contract::STAMP_CLOSE,
};

type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 对面：在那台上跑一条一次性命令（收全三样）· 问那台落点上一个文件（SFTP）。生产 = [`DialFacing`]；判据用替身。
pub trait Facing: Send + Sync {
    /// 收全三样 ＋ 那一趟拨号的 ack（`DialAck` 原样；逐地址指纹要交 monitor 固化）。
    fn exec(&self, command: String) -> Fut<'_, Result<(crate::dial::Captured, Value), String>>;
    /// `(metadata 的 size, 补问的 exists)` —— 与 [`interpret_target_probe`] 入参同形。
    fn stat<'a>(
        &'a self,
        rel: &'a str,
    ) -> Fut<'a, Result<(Option<Option<u64>>, Option<bool>), String>>;
    /// 列一个目录：`(名字, 修改时间秒)`；列不出 ⇒ `None`。
    fn list<'a>(&'a self, rel: &'a str) -> Fut<'a, Option<Vec<(String, Option<u64>)>>>;
}

// ═══ 部署判定（判定只在后端）══════════════
//
// 契约那一半（键 · 戳格式 · 答话形状 · 路径）住 `deploy_contract`，两侧同一份；下面这几条裁决只住这里，
// monitor 那两处自举（本机后端放下去之前）改问手上那份字节自己（[`answer_place`]，帧命令 `place-verdict`）。

/// 表 B：这个 origin 承诺哪几种机器（本机 Windows x86_64 · 本机 Linux x86_64 · 远端 Linux 两个 arch）。
///
/// 本机 (Linux, aarch64) 不承诺（[`Refusal::NotPromisedHere`]；OS 适配以后单独做）⇒ 本机那两行都钉到 x86_64，远端 Linux 两个 arch 照旧。
/// 承诺面的唯一住址是 `tests/evidence/K-G4-platform-ledger.py` 的 `PROMISE_FACE`；本函数与它两向相等
/// 由 `deploy_plan_tests.rs::the_promise_face_in_the_ledger_equals_the_code` 钉着。
pub fn promised(route: Route, key: Key) -> bool {
    matches!(
        (route, key.os, key.arch),
        (Route::Local, Os::Windows, Arch::X86_64)
            | (Route::Local, Os::Linux, Arch::X86_64)
            | (Route::Remote, Os::Linux, _)
    )
}

/// 拒绝点的前三步（在向目标机器写第一个字节之前）：键 → 产线 → 承诺。四形各在一步上，不合并；
/// 第五形「这一版带没带」看放字节的一侧交来的事实（`carried`，[`slot_of`]）。
pub fn judge(route: Route, key: Result<Key, Refusal>) -> Result<Key, Refusal> {
    let key = key?;
    let (os, arch) = (key.os.label().to_string(), key.arch.label().to_string());
    if !LINES.contains(&key) {
        return Err(Refusal::UnsupportedMachine { os, arch });
    }
    if !promised(route, key) {
        return Err(Refusal::NotPromisedHere { os, arch, route });
    }
    Ok(key)
}

/// 部署落点那个文件**本身**的取样结论（落点身份的第一步：没有 / 0 字节就不必再问它是谁）。
///
/// 纪律：**「问不出来」不许读成上面任何一个确定答案**
/// ——把无权限/传输失败当成「不在」会变成每次连接都重传（把版本门控拆了），
/// 当成「在」则退回本枚举要治的那个静默。**所以它不是 `bool`。**
/// ⚠ 成员就在下面，别在散文里复述一份基数 —— 那份字面量会在加成员那天变成假话。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TargetBinary {
    /// stat 说它在，且有字节。
    Present,
    /// stat **明确说**它不在。
    Missing,
    /// stat 说它在，但是 0 字节 —— 原子上传（`dial/sftp.rs::put_atomic`）里「绝不 set_metadata」防的就是把后端截成 0 字节、
    /// 不可 exec 的那一形。`try_exists` 会把它算成「在」。
    Empty,
    /// 问不出来（无权限 / 传输失败 / 服务器不给属性）—— 不许读成上面任何一个。
    Unknown,
}

/// stat 那两次取样的解释（纯函数，可单测）。
///
/// 形状：**吃两次调用各自的结果，不吃会话**。拆出来的理由是一个具体缺陷，不是行数：解释这一半原先焊在 async 体里，
/// 四个状态的映射规则因此一条判据都没有（`tests/evidence/K-W4b-readings.md`）。
///
/// 入参就是两次调用**降解之后**的结果：
/// - `metadata_size`：`None` = `metadata` 那次调用失败；`Some(inner)` = 成功，
///   `inner` 是服务器给的 size —— ⚠ `Some(None)` 是**服务器没给 size**，不是 0 字节。
/// - `exists`：`metadata` 失败时补问 `try_exists` 的结果（`None` = 它也答不出来）。
///   `metadata` 成功那一路根本不问它，那时它恒为 `None` 而本函数在那一路也不看它。
pub fn interpret_target_probe(
    metadata_size: Option<Option<u64>>,
    exists: Option<bool>,
) -> TargetBinary {
    match metadata_size {
        Some(Some(0)) => TargetBinary::Empty,
        // 服务器不给 size（`Some(None)`）≠ 0 字节：存在是确定的，别把「没说」读成「空」。
        Some(_) => TargetBinary::Present,
        None => match exists {
            Some(false) => TargetBinary::Missing,
            Some(true) => TargetBinary::Present,
            None => TargetBinary::Unknown,
        },
    }
}

/// 「手上这一版比那台上的新」—— 两边都解得出序键（[`deploy_contract::build_order`]）、且这一版的严格大。解不出任何一边 ⇒ `false`（不可比 ⇒ 不换）。
pub fn is_newer(mine: &str, theirs: &str) -> bool {
    matches!((deploy_contract::build_order(mine), deploy_contract::build_order(theirs)), (Some(m), Some(t)) if m > t)
}

/// 要不要（重）部署 —— **对照物是手上那份字节自报的身份**，不是源码常量。**纯函数**。
///
/// `Err` = 显式失败、**一个字节都不写**（出路交给用户：机器页「卸载后端」删掉那个文件，就是明确授权覆盖）。
///
/// 「另一版」那一格按 [`deploy_contract::build_order`] 拆两格：那台上的**比这一版旧** ⇒ 换；**不比这一版旧** ⇒ [`DeployAction::Keep`]
/// （两个不同版本的 monitor 连同一台远端，只升不降，不会每次连上互相换掉）。
pub fn identity_decision(
    id: &RemoteIdentity,
    expected: &str,
    machine: &str,
    path: &str,
) -> Result<DeployAction, String> {
    let hands_off = copy_text("rsSftp.identity.handsOff", &[]);
    match id {
        RemoteIdentity::Missing => Ok(DeployAction::Deploy(copy_text(
            "rsSftp.identity.missing",
            &[],
        ))),
        RemoteIdentity::Empty => Ok(DeployAction::Deploy(copy_text(
            "rsSftp.identity.empty",
            &[],
        ))),
        RemoteIdentity::Stamp(s) if s == expected => Ok(DeployAction::Skip),
        RemoteIdentity::Stamp(s) if is_newer(expected, s) => Ok(DeployAction::Deploy(copy_text(
            "rsSftp.identity.other",
            &[("s", &s.to_string()), ("expected", &expected.to_string())],
        ))),
        RemoteIdentity::Stamp(s) => Ok(DeployAction::Keep {
            theirs: s.clone(),
            why: copy_text(
                "rsSftp.identity.notOlder",
                &[
                    ("machine", &machine.to_string()),
                    ("s", &s.to_string()),
                    ("expected", &expected.to_string()),
                ],
            ),
        }),
        RemoteIdentity::NoStamp => Err(copy_text(
            "rsSftp.identity.unstamped",
            &[
                ("machine", &machine.to_string()),
                ("path", &path.to_string()),
                ("handsOff", &hands_off.to_string()),
            ],
        )),
        RemoteIdentity::Ambiguous(ids) => Err(copy_text(
            "rsSftp.identity.multiple",
            &[
                ("machine", &machine.to_string()),
                ("path", &path.to_string()),
                ("ids", &ids.join(&copy_text("rsSftp.identity.listSep", &[]))),
                ("handsOff", &hands_off.to_string()),
            ],
        )),
        RemoteIdentity::Unreadable(why) => Err(copy_text(
            "rsSftp.identity.undecidable",
            &[
                ("machine", &machine.to_string()),
                ("path", &path.to_string()),
                ("why", &why.to_string()),
            ],
        )),
    }
}

/// 残件多久没动过才算没人要：远大于 monitor 等一次 `put` 的上限（`dial_host::FILES_PUT_DEADLINE`，600 秒）
/// ⇒ 另一个部署者正在写的那一份（修改时间随写不断刷新）不会被当成残件；也容得下两台机器之间一些钟差。
pub const LEFTOVER_STALE_SECS: u64 = 3600;

/// `dir` 里哪几份是 [`crate::dial::sftp::put_atomic`] 留下、已经没人要的临时件 / 备份件（`dir/名字`，排序）。
/// 只认那个形状（`dial::sftp::is_trip_leftover`）；修改时间缺 / 比 `now` 新 ⇒ 不算（宁可多留一轮）。
pub fn stale_leftovers(dir: &str, entries: &[(String, Option<u64>)], now_secs: u64) -> Vec<String> {
    let mut out: Vec<String> = entries
        .iter()
        .filter(|(name, mtime)| {
            crate::dial::sftp::is_trip_leftover(name)
                && mtime.is_some_and(|t| now_secs.saturating_sub(t) > LEFTOVER_STALE_SECS)
        })
        .map(|(name, _)| format!("{dir}/{name}"))
        .collect();
    out.sort();
    out
}

/// 一份计划。
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub key: Key,
    /// 这一版带着的那一格自报的身份（对照物）。
    pub expected: String,
    pub action: DeployAction,
    /// 落点目录里没人要的临时件 / 备份件（家目录相对）；monitor 照删。列不出那个目录 ⇒ 空（下次再问）。
    pub leftovers: Vec<String>,
    /// 问 `uname` 那一趟的 ack（本机后端里拨的号 —— 第一次连一台没钉过指纹的机器就在这一跳）：
    /// 原样交回，monitor 按它固化指纹（`dial_host::settle_host_key`，与自己拨号那几条同一个判定）。
    pub ack: Value,
}

/// 落点那一份是谁：先 stat（没有 / 0 字节就不必再问），在就扫它字节里的身份戳。
async fn identity_at(facing: &dyn Facing, rel: &str, word: &str) -> Result<RemoteIdentity, String> {
    let (size, exists) = facing.stat(rel).await?;
    Ok(match interpret_target_probe(size, exists) {
        TargetBinary::Missing => RemoteIdentity::Missing,
        TargetBinary::Empty => RemoteIdentity::Empty,
        TargetBinary::Present | TargetBinary::Unknown => {
            match facing
                .exec(deploy_contract::stamp_scan_cmd(word, MARKS))
                .await
            {
                Ok((r, _)) => deploy_contract::interpret_stamp_scan(
                    r.exit_status,
                    &r.stdout,
                    &r.stderr,
                    MARKS,
                ),
                Err(e) => RemoteIdentity::Unreadable(e),
            }
        }
    })
}

/// **那台要哪一格字节** —— 部署计划的第 ① 步。
///
/// 问 `uname -s -m`（[`deploy_contract::key_from_uname`]）→ 表 A / 表 B（[`judge`]）→ 这一版带没带那一格（`carried`）。
/// 拒绝点在写第一个字节之前；回那一格与问 `uname` 那一趟拨号的 ack。
async fn slot_of(
    facing: &dyn Facing,
    carried: &[Key],
    machine: &str,
) -> Result<(Key, Value), Fail> {
    let said = |r: Refusal| refused(&r, machine);
    let (got, ack) = facing
        .exec(deploy_contract::UNAME_CMD.to_string())
        .await
        .map_err(|e| {
            Fail::new(
                "unreachable",
                copy_text(
                    "rsSftp.deploy.unameFailed",
                    &[("machine", machine), ("e", &e)],
                ),
            )
        })?;
    let raw = deploy_contract::key_from_uname(got.exit_status, &got.stdout, &got.stderr);
    let key = judge(Route::Remote, raw).map_err(said)?;
    if !carried.contains(&key) {
        return Err(said(Refusal::NotCarried {
            os: key.os.label().to_string(),
            arch: key.arch.label().to_string(),
        }));
    }
    Ok((key, ack))
}

/// 一形拒绝 ⇒ `refused`：句子是 [`Refusal::say`]，那台答的原话（[`Refusal::raw`]）随失败交出去。
fn refused(r: &Refusal, machine: &str) -> Fail {
    Fail::new("refused", r.say(machine)).with_raw(r.raw())
}

/// 出计划。`machine` = monitor 交来的那台的名字（只用来说话）；`now_secs` = 此刻（判残件新旧）。
pub async fn plan(
    facing: &dyn Facing,
    carried: &[(Key, String)],
    machine: &str,
    now_secs: u64,
) -> Result<Plan, Fail> {
    let keys: Vec<Key> = carried.iter().map(|(k, _)| *k).collect();
    let (key, ack) = slot_of(facing, &keys, machine).await?;
    let expected = carried
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, id)| id.clone())
        .expect("slot_of 只回带着的那一格");
    let landing = relay_route_core::BACKEND_LANDING_REL;
    let id = identity_at(facing, landing, relay_route_core::BACKEND_LANDING_SHELL)
        .await
        .map_err(|e| Fail::new("io_failed", e))?;
    let action = identity_decision(&id, &expected, machine, &format!("~/{landing}"))
        .map_err(|e| Fail::new("undecidable", e))?;
    let bin = landing.rsplit_once('/').map_or(".", |(d, _)| d);
    let leftovers = facing
        .list(bin)
        .await
        .map(|entries| stale_leftovers(bin, &entries, now_secs))
        .unwrap_or_default();
    Ok(Plan {
        key,
        expected,
        action,
        leftovers,
        ack,
    })
}

/// 计划 → 帧面那一格（线上形状住 `IPC-PROTOCOL.md` 的 `deploy-plan` 那一节）。
pub fn plan_json(p: &Plan) -> Value {
    let (action, why, theirs) = match &p.action {
        DeployAction::Skip => ("skip", String::new(), None),
        DeployAction::Deploy(why) => ("deploy", why.clone(), None),
        DeployAction::Keep { theirs, why } => ("keep", why.clone(), Some(theirs.clone())),
    };
    json!({
        "os": p.key.os.label(),
        "arch": p.key.arch.label(),
        "label": p.key.label(),
        "expected": p.expected,
        "action": action,
        "why": why,
        "theirs": theirs,
        "leftovers": p.leftovers,
        "ack": p.ack,
    })
}

/// `carried` 那一格：`[{os, arch, id}]`。键认不出 / 身份空 ⇒ `bad_args`（monitor 交的是它自己表里的词，认不出就是两侧漂了）。
pub fn carried_of(args: &Value) -> Result<Vec<(Key, String)>, Fail> {
    let bad = |m: &str| Fail::new("bad_args", crate::common::contract::malformed(m));
    let rows = args
        .get("carried")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("missing `carried` (array)"))?;
    rows.iter()
        .map(|r| {
            let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("");
            let key = deploy_contract::key_of(s("os"), s("arch"))
                .map_err(|_| bad("`carried` row names a machine outside table A"))?;
            match s("id") {
                "" => Err(bad("`carried` row without `id`")),
                id => Ok((key, id.to_string())),
            }
        })
        .collect()
}

/// 生产那一个对面：沿池里那条 SSH（`remote_ask::capture_full` 跑命令 · `dial::sftp` 开一条只读用的会话，第一问时才开）。
pub struct DialFacing {
    dial: Value,
    fs: tokio::sync::OnceCell<Result<crate::dial::sftp::Session, String>>,
}

impl DialFacing {
    pub fn new(dial: Value) -> DialFacing {
        DialFacing {
            dial,
            fs: tokio::sync::OnceCell::new(),
        }
    }

    async fn session(&self) -> Result<&crate::dial::sftp::Session, String> {
        self.fs
            .get_or_init(|| async {
                let d = crate::dial::sftp::Dial::parse(&self.dial)?;
                // 部署这一问只带一句话（`Facing::stat` 回 `String`）：SFTP 那一下的原话进后端日志。
                crate::dial::sftp::open_for_transfer(&d).await.map_err(|s| {
                    if let Some(raw) = &s.raw {
                        tracing::warn!("deploy: opening sftp failed: {raw}");
                    }
                    s.said
                })
            })
            .await
            .as_ref()
            .map_err(Clone::clone)
    }
}

impl Facing for DialFacing {
    fn exec(&self, command: String) -> Fut<'_, Result<(crate::dial::Captured, Value), String>> {
        Box::pin(crate::dial::remote_ask::capture_full(&self.dial, command))
    }

    fn stat<'a>(
        &'a self,
        rel: &'a str,
    ) -> Fut<'a, Result<(Option<Option<u64>>, Option<bool>), String>> {
        Box::pin(async move {
            let s = self.session().await?;
            let size = crate::dial::sftp::metadata_size(s, rel).await;
            let exists = match size {
                Some(_) => None,
                None => crate::dial::sftp::exists(s, rel).await,
            };
            Ok((size, exists))
        })
    }

    fn list<'a>(&'a self, rel: &'a str) -> Fut<'a, Option<Vec<(String, Option<u64>)>>> {
        Box::pin(async move {
            let s = self.session().await.ok()?;
            crate::dial::sftp::list_dir(s, rel).await
        })
    }
}

/// 帧面入口：`{dial, carried, machine?}` → 计划。
pub async fn answer(args: &Value, facing: &dyn Facing) -> Result<Value, Fail> {
    let carried = carried_of(args)?;
    let machine = args
        .get("machine")
        .and_then(Value::as_str)
        .filter(|m| !m.trim().is_empty())
        .ok_or_else(|| {
            Fail::new(
                "bad_args",
                crate::common::contract::malformed("missing `machine` (string)"),
            )
        })?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    plan(facing, &carried, machine, now)
        .await
        .map(|p| plan_json(&p))
}

// ═══ 远端常驻后端 hello 的新旧（帧命令 `resident-verdict`）═══════════════════════════
//
// monitor 接远端常驻后端时读到 hello，「那台比手上这一版旧 ⇒ 换一次」在这里判，monitor 只照做。

/// hello 那一问的答：`replace` = 换掉再接（只升不降，且只换一次）；`older` = 那台比手上这一版旧（版本那句话按它挑）。
#[derive(Debug, PartialEq, Eq)]
pub struct Verdict {
    pub replace: bool,
    pub older: bool,
}

/// **纯函数**：`mine` = monitor 手上这一版自报的身份 · `theirs` = 那台 hello 报的 · `replaced` = 这一趟已经换过一次。
pub fn resident_verdict(mine: &str, theirs: &str, replaced: bool) -> Verdict {
    let older = is_newer(mine, theirs);
    Verdict {
        replace: older && !replaced,
        older,
    }
}

/// 帧面入口：`{mine, theirs, replaced}` → `{action: "attach" | "replace", older}`。`mine` 空 / 缺 · 缺 `theirs` / `replaced` ⇒ `bad_args`。
pub fn answer_resident_verdict(args: &Value) -> Result<Value, (&'static str, String)> {
    let bad = |m: &str| ("bad_args", crate::common::contract::malformed(m));
    let mine = args
        .get("mine")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("missing `mine` (non-empty string)"))?;
    let theirs = args
        .get("theirs")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("missing `theirs` (string)"))?;
    let replaced = args
        .get("replaced")
        .and_then(Value::as_bool)
        .ok_or_else(|| bad("missing `replaced` (bool)"))?;
    let v = resident_verdict(mine, theirs, replaced);
    Ok(json!({
        "action": if v.replace { "replace" } else { "attach" },
        "older": v.older,
    }))
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

/// 帧面入口：`{dest, machine}` → `{action: "place" | "keep", why}`。`dest` 缺 / 不是绝对路径 · `machine` 缺 ⇒ `bad_args`。
/// 只读落点那一个文件（不在 ⇒ 没装）；这份字节自己的键与身份取自编译期（`Key::this_machine` · `crate::BUILD_ID`）。
pub fn answer_place(args: &Value) -> Result<Value, Fail> {
    let bad = |m: &str| Fail::new("bad_args", crate::common::contract::malformed(m));
    let dest = args
        .get("dest")
        .and_then(Value::as_str)
        .filter(|d| std::path::Path::new(d).is_absolute())
        .ok_or_else(|| bad("missing `dest` (absolute path)"))?;
    let machine = args
        .get("machine")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty())
        .ok_or_else(|| bad("missing `machine` (non-empty string)"))?;
    let disk = match std::fs::read(dest) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    };
    Ok(
        match place_verdict(Key::this_machine(), disk, crate::BUILD_ID, machine, dest)? {
            Placed::Place(why) => json!({ "action": "place", "why": why }),
            Placed::Keep(why) => json!({ "action": "keep", "why": why }),
        },
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/control/deploy_plan_tests.rs"]
mod tests;
