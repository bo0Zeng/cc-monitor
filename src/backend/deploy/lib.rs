//! **装 / 升级远端后端的判定与编排**：桌面与手机同一份。
//!
//! 照桌面五步走：① 出计划（[`plan`]）· ② 取字节 ＋ 自检 · ③ 放进暂存区 ＋ 读回（`ssh_dial::sftp::put_verified`）→ 那一份自己换进落点（`--place-self`）· ④ 判读回 ＋ 收拾残件 ·
//! ⑤ 起常驻 / 换旧的（远端 ccm 自己的 `--resident-ensure [--replace]`，[`resident_verdict`] 判换不换）。
//!
//! 两端都调 [`install`]（①–④ 一趟，字节是宿主交来的一份文件，每一步那一句是 [`Step::said`]、结局那一句是 [`Plan::said`]）；⑤ 由宿主跑 `--resident-ensure --replace`
//! （那一步那一句也是 [`Step::said`]）。卸载是 [`uninstall`]：同样只写暂存区，由传上去的那一份删落点那一份。
//! - 桌面：本机常驻后端答帧命令 `deploy-install`（`control/deploy_plan.rs`，薄薄一层：入参 · 进度帧 · 回包形状 · 码），字节是 monitor 落成的文件；
//! - 手机：mobcore 直接调，字节是 App 从包里解出来的那一份文件。
//!
//! 契约那一半（表 A 的键与行 · 戳格式 · 答话形状 · 落点路径）住 `deploy_contract` / `relay_route_core`；本 crate 只放判定与编排。
//!
//! # 买不到的
//!
//! - 判定与放字节之间有窗（TOCTOU）：计划出来之后那台上的文件被人换了，照计划放。

use std::future::Future;
use std::pin::Pin;

use copy_core::copy_text;
use deploy_contract::{
    build_relation, Arch, BuildRelation, DeployAction, Key, Marks, Os, Refusal, RemoteIdentity,
    Route, LINES,
};
use serde_json::Value;
use ssh_dial::{Captured, DialRequest};

mod install;
pub use install::{install, uninstall, Load, Place, Removed, Slot, Step, STAGED_STEM};

/// 身份戳的两个界标（住契约 crate）。
pub const MARKS: Marks<'static> = Marks {
    open: deploy_contract::STAMP_OPEN,
    close: deploy_contract::STAMP_CLOSE,
};

pub type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 这一串的失败：码 ＋ 那一句 ＋ 下层原话（那台答 `uname` 的原话进复制详情，不上句子）。后端把它换成帧面的失败，手机原样交 Kotlin。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fail {
    pub code: &'static str,
    pub said: String,
    pub raw: Option<String>,
}

impl Fail {
    pub fn new(code: &'static str, said: String) -> Fail {
        Fail {
            code,
            said,
            raw: None,
        }
    }

    pub fn with_raw(mut self, raw: Option<&str>) -> Fail {
        self.raw = raw
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_string);
        self
    }
}

/// 对面：在那台上跑一条一次性命令（收全三样）· 问那台落点上一个文件（SFTP）。生产 = [`DialFacing`]；判据用替身。
pub trait Facing: Send + Sync {
    /// 收全三样 ＋ 那一趟拨号的 ack（`DialAck` 原样；桌面那一侧逐地址指纹要交 monitor 固化）。
    fn exec(&self, command: String) -> Fut<'_, Result<(Captured, Value), String>>;
    /// `(metadata 的 size, 补问的 exists)` —— 与 [`interpret_target_probe`] 入参同形。
    fn stat<'a>(
        &'a self,
        rel: &'a str,
    ) -> Fut<'a, Result<(Option<Option<u64>>, Option<bool>), String>>;
    /// 列一个目录：`(名字, 修改时间秒)`；列不出 ⇒ `None`。
    fn list<'a>(&'a self, rel: &'a str) -> Fut<'a, Option<Vec<(String, Option<u64>)>>>;
}

// ═══ 部署判定 ══════════════

/// 表 B：这个 origin 承诺哪几种机器（本机 Windows x86_64 · 本机 Linux x86_64 · 远端 Linux 两个 arch）。
///
/// 本机 (Linux, aarch64) 不承诺（[`Refusal::NotPromisedHere`]；OS 适配以后单独做）⇒ 本机那两行都钉到 x86_64，远端 Linux 两个 arch 照旧。
/// 承诺面的唯一住址是 `tests/evidence/K-G4-platform-ledger.py` 的 `PROMISE_FACE`；本函数与它两向相等
/// 由后端 `deploy_plan_tests.rs::the_promise_face_in_the_ledger_equals_the_code` 钉着。
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
    /// stat 说它在，但是 0 字节 —— 原子上传（`ssh-dial/sftp.rs::put_atomic`）里「绝不 set_metadata」防的就是把后端截成 0 字节、
    /// 不可 exec 的那一形。`try_exists` 会把它算成「在」。
    Empty,
    /// 问不出来（无权限 / 传输失败 / 服务器不给属性）—— 不许读成上面任何一个。
    Unknown,
}

/// stat 那两次取样的解释（纯函数，可单测）。
///
/// 形状：**吃两次调用各自的结果，不吃会话**。拆出来的理由是一个具体缺陷，不是行数：解释这一半原先焊在 async 体里，
/// 四个状态的映射规则因此一条判据都没有。
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

/// 要不要（重）部署 —— **对照物是手上那份字节自报的身份**，不是源码常量。**纯函数**。
///
/// `Err` = 显式失败、**一个字节都不写**（出路交给用户：机器页「卸载后端」删掉那个文件，就是明确授权覆盖）。
///
/// 「另一版」那一格按 [`deploy_contract::build_relation`] 拆两格：那台上的**比这一版旧** ⇒ 换；**不比这一版旧** ⇒ [`DeployAction::Keep`]
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
        RemoteIdentity::Stamp(s) if build_relation(s, expected) == BuildRelation::Older => {
            Ok(DeployAction::Deploy(copy_text(
                "rsSftp.identity.other",
                &[("s", &s.to_string()), ("expected", &expected.to_string())],
            )))
        }
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

/// 残件多久没动过才算没人要：远大于等一次 `put` 的上限（monitor `dial_host::FILES_PUT_DEADLINE`，600 秒）
/// ⇒ 另一个部署者正在写的那一份（修改时间随写不断刷新）不会被当成残件；也容得下两台机器之间一些钟差。
pub const LEFTOVER_STALE_SECS: u64 = 3600;

/// `dir` 里哪几份是一趟部署留下、已经没人要的件（`dir/名字`，排序）。
/// 只认那个形状（`ssh_dial::sftp::is_trip_leftover`）；修改时间缺 / 比 `now` 新 ⇒ 不算（宁可多留一轮）。
pub fn stale_leftovers(dir: &str, entries: &[(String, Option<u64>)], now_secs: u64) -> Vec<String> {
    let mut out: Vec<String> = entries
        .iter()
        .filter(|(name, mtime)| {
            ssh_dial::sftp::is_trip_leftover(name)
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
    /// 暂存区里上一趟部署没人要的件（家目录相对）；照删。列不出那个目录 ⇒ 空（下次再问）。
    pub leftovers: Vec<String>,
    /// 问 `uname` 那一趟的 ack：桌面那一侧原样交回 monitor，按它固化指纹（`dial_host::settle_host_key`）；手机不用（拨号时已经回调落进 Room）。
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
                    &[("machine", machine), ("why", &e)],
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
pub fn refused(r: &Refusal, machine: &str) -> Fail {
    Fail::new("refused", r.say(machine)).with_raw(r.raw())
}

/// 出计划。`machine` = 那台的名字（只用来说话）；`now_secs` = 此刻（判残件新旧）。一个字节都不写。
pub async fn plan(
    facing: &dyn Facing,
    carried: &[(Key, String)],
    machine: &str,
    now_secs: u64,
) -> Result<Plan, Fail> {
    plan_and_landing(facing, carried, machine, now_secs)
        .await
        .map(|(p, _)| p)
}

/// [`plan`] ＋ 落点上原来有没有一份（没有 / 0 字节 ⇒ `false`：这一趟是装，不是更新）。
pub(crate) async fn plan_and_landing(
    facing: &dyn Facing,
    carried: &[(Key, String)],
    machine: &str,
    now_secs: u64,
) -> Result<(Plan, bool), Fail> {
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
    let had_one = !matches!(id, RemoteIdentity::Missing | RemoteIdentity::Empty);
    let action = identity_decision(&id, &expected, machine, &format!("~/{landing}"))
        .map_err(|e| Fail::new("undecidable", e))?;
    let staging = ssh_dial::sftp::STAGING_ROOT;
    let leftovers = facing
        .list(staging)
        .await
        .map(|entries| stale_leftovers(staging, &entries, now_secs))
        .unwrap_or_default();
    Ok((
        Plan {
            key,
            expected,
            action,
            leftovers,
            ack,
        },
        had_one,
    ))
}

/// 落点所在的目录（家目录相对）。
pub fn landing_dir() -> &'static str {
    relay_route_core::BACKEND_LANDING_REL
        .rsplit_once('/')
        .map_or(".", |(d, _)| d)
}

// ═══ 生产那一个对面 ═══════════════════════════════════════════════════════════════

/// 在那台上跑一条一次性命令、收全三样（退出码 · stdout · stderr，不替调用方判退出码：`grep` 退出 1 是「一个都没有」）＋ 那一趟拨号的 ack。
/// 宿主给（池里那条连接上多开一个 exec，全后端只在 `dial/remote_ask.rs` 一处）：后端 = `remote_ask::capture_full` · 手机 = 常驻流那条链路的 capture（ack 交 `null`）。
pub type Exec =
    Box<dyn Fn(String) -> Fut<'static, Result<(Captured, Value), String>> + Send + Sync>;

/// 生产那一个对面：一次性命令走宿主给的 [`Exec`] · 文件走池里那条连接上的一条 SFTP 会话（第一问时才开）。
/// 拨号请求是组好的那一形（后端 `ssh_dial::machine::resolve` 出 · 手机 Kotlin 交）；宿主那一套（钥匙 · host key · 写规矩）由宿主先装好。
pub struct DialFacing {
    req: DialRequest,
    exec: Exec,
    fs: tokio::sync::OnceCell<Result<ssh_dial::sftp::Session, ssh_dial::Said>>,
}

impl DialFacing {
    pub fn new(req: DialRequest, exec: Exec) -> DialFacing {
        DialFacing {
            req,
            exec,
            fs: tokio::sync::OnceCell::new(),
        }
    }

    /// 这条连接上那条 SFTP 会话（计划那几问只读；手机那一趟装也在它上面放字节）。
    pub async fn session(&self) -> Result<&ssh_dial::sftp::Session, &ssh_dial::Said> {
        self.fs
            .get_or_init(|| async {
                let d = ssh_dial::sftp::Dial::of(self.req.clone());
                ssh_dial::sftp::open_for_transfer(&d).await
            })
            .await
            .as_ref()
    }
}

impl Facing for DialFacing {
    fn exec(&self, command: String) -> Fut<'_, Result<(Captured, Value), String>> {
        (self.exec)(command)
    }

    fn stat<'a>(
        &'a self,
        rel: &'a str,
    ) -> Fut<'a, Result<(Option<Option<u64>>, Option<bool>), String>> {
        Box::pin(async move {
            // 部署这一问只带一句话（`Facing::stat` 回 `String`）：SFTP 那一下的原话进日志。
            let s = self.session().await.map_err(|s| {
                if let Some(raw) = &s.raw {
                    tracing::warn!("deploy: opening sftp failed: {raw}");
                }
                s.said.clone()
            })?;
            let size = ssh_dial::sftp::metadata_size(s, rel).await;
            let exists = match size {
                Some(_) => None,
                None => ssh_dial::sftp::exists(s, rel).await,
            };
            Ok((size, exists))
        })
    }

    fn list<'a>(&'a self, rel: &'a str) -> Fut<'a, Option<Vec<(String, Option<u64>)>>> {
        Box::pin(async move {
            let s = self.session().await.ok()?;
            ssh_dial::sftp::list_dir(s, rel).await
        })
    }
}

// ═══ 远端常驻后端 hello 的新旧 ═══════════════════════════

/// hello 那一问的答：`replace` = 换掉再接（只升不降，且只换一次）；`older` = 那台比手上这一版旧（版本那句话按它挑）。
#[derive(Debug, PartialEq, Eq)]
pub struct Verdict {
    pub replace: bool,
    pub older: bool,
}

/// **纯函数**：`mine` = 手上这一版自报的身份 · `theirs` = 那台 hello 报的 · `replaced` = 这一趟已经换过一次。
pub fn resident_verdict(mine: &str, theirs: &str, replaced: bool) -> Verdict {
    let older = build_relation(theirs, mine) == BuildRelation::Older;
    Verdict {
        replace: older && !replaced,
        older,
    }
}

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

#[cfg(test)]
#[path = "../../../tests/backend/deploy/install_tests.rs"]
mod install_tests;
