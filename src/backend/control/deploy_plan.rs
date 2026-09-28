//! 要求住址：`调研/第四波记录/_施工/4d-lanes.md` MIG-3b 第 1 条 ——「`sftp.rs` 部署决策（该不该换 · 换成什么 · 身份判定）进后端；monitor 只放字节」。
//!
//! # 帧命令 `deploy-plan`（本机常驻后端答）
//!
//! monitor 交两样事实：怎么够到那台（`dial`，与 `files` 链路同一份拨号请求）· 自己带着哪几格后端字节、各自自报的身份
//! （`carried`，`[{os, arch, id}]`，放字节的一侧才知道）。本模块沿池里那条 SSH 问那台：
//!
//! 1. `uname -s -m`（[`deploy_core::key_from_uname`]）→ 表 A / 表 B（[`deploy_core::judge`]）→ 这一版带没带那一格（`carried`）——
//!    **换成什么**；拒绝点在写第一个字节之前（`设计/96 §7.1.4b`）；
//! 2. 落点那一份是谁：stat（没有 / 0 字节就不必再问）→ 扫它字节里的身份戳（一次 exec，不跑它）；
//!    不肯说自己是谁时读回来看是不是从前那份三行入口 —— **身份判定**（`96 §7.2`）；
//! 3. **该不该换**（[`deploy_core::landing_verdict`]：只升不降）；旧落点那份字节要不要删（[`deploy_core::legacy_verdict`]）。
//!
//! 回计划，一个字节都不写：放字节（mkdir · 原子上传 · 读回比对）与删旧落点仍是 monitor 经 `files` 链路做（SR1b 那条路不变）。
//!
//! # 它归 `control/` 的理由
//!
//! 同 [`super::resolve_query`]：产「要怎么改变世界」的计划属于控制的前半；它自己只读（stat · read · 两条只读 exec）。
//!
//! # 买不到的
//!
//! - 判定与放字节之间有窗（TOCTOU）：计划出来之后那台上的文件被人换了，monitor 照计划放 —— 与从前在 monitor 里判一样。
//! - 第一次连一台没钉过指纹的机器：这一问在本机后端里拨，ack 里那枚指纹这一跳不交 monitor 去钉；
//!   紧接着 monitor 开 `files` 链路（池里同一条连接）那一跳照旧钉（`dial_host::settle_host_key`）。

use std::future::Future;
use std::pin::Pin;

use copy_core::copy_text;
use deploy_core::{
    DeployAction, Key, LegacyVerdict, Marks, Product, Refusal, RemoteIdentity, Route,
};
use serde_json::{json, Value};

/// 认从前那份三行入口时最多读多少（它几十字节；比这大就不是那一形 ⇒ 不读，按「不说自己是谁」显式失败）。
const ENTRY_READ_MAX: u64 = 64 * 1024;

/// 身份戳的两个界标（唯一住址在 `lib.rs`）。
const MARKS: Marks<'static> = Marks {
    open: crate::BUILD_STAMP_OPEN,
    close: crate::BUILD_STAMP_CLOSE,
};

type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 对面：在那台上跑一条一次性命令（收全三样）· 问那台落点上一个文件（SFTP）。生产 = [`DialFacing`]；判据用替身。
pub trait Facing: Send + Sync {
    /// 收全三样 ＋ 那一趟拨号的 ack（`DialAck` 原样；逐地址指纹要交 monitor 固化）。
    fn exec(&self, command: String) -> Fut<'_, Result<(crate::dial::Captured, Value), String>>;
    /// `(metadata 的 size, 补问的 exists)` —— 与 [`deploy_core::interpret_target_probe`] 入参同形。
    fn stat<'a>(
        &'a self,
        rel: &'a str,
    ) -> Fut<'a, Result<(Option<Option<u64>>, Option<bool>), String>>;
    /// 整份读回来；读不出 · 比 `max` 大（先问大小，大了不读）⇒ `None`。
    fn read<'a>(&'a self, rel: &'a str, max: u64) -> Fut<'a, Option<Vec<u8>>>;
}

/// 一份计划。
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub key: Key,
    /// 这一版带着的那一格自报的身份（对照物，`96 §7.2.3`）。
    pub expected: String,
    pub action: DeployAction,
    pub legacy: LegacyVerdict,
    /// 〔MIG-3b 续 · VIS2〕问 `uname` 那一趟的 ack（本机后端里拨的号 —— 第一次连一台没钉过指纹的机器就在这一跳）：
    /// 原样交回，monitor 按它固化指纹（`dial_host::settle_host_key`，与自己拨号那几条同一个判定）。
    pub ack: Value,
}

/// 落点那一份是谁：先 stat（没有 / 0 字节就不必再问），在就扫它字节里的身份戳。
async fn identity_at(facing: &dyn Facing, rel: &str, word: &str) -> Result<RemoteIdentity, String> {
    let (size, exists) = facing.stat(rel).await?;
    Ok(match deploy_core::interpret_target_probe(size, exists) {
        deploy_core::TargetBinary::Missing => RemoteIdentity::Missing,
        deploy_core::TargetBinary::Empty => RemoteIdentity::Empty,
        deploy_core::TargetBinary::Present | deploy_core::TargetBinary::Unknown => {
            match facing.exec(deploy_core::stamp_scan_cmd(word, MARKS)).await {
                Ok((r, _)) => {
                    deploy_core::interpret_stamp_scan(r.exit_status, &r.stdout, &r.stderr, MARKS)
                }
                Err(e) => RemoteIdentity::Unreadable(e),
            }
        }
    })
}

/// 出计划。`machine` = monitor 交来的那台的名字（只用来说话）。失败 = `(code, 一句话)`。
pub async fn plan(
    facing: &dyn Facing,
    carried: &[(Key, String)],
    machine: &str,
) -> Result<Plan, (&'static str, String)> {
    let said = |r: Refusal| ("refused", r.say(Product::Backend, machine));
    let (got, ack) = facing
        .exec(deploy_core::UNAME_CMD.to_string())
        .await
        .map_err(|e| {
            (
                "unreachable",
                copy_text(
                    "rsSftp.deploy.unameFailed",
                    &[("machine", machine), ("e", &e)],
                ),
            )
        })?;
    let key = deploy_core::judge(
        Product::Backend,
        Route::Remote,
        deploy_core::key_from_uname(got.exit_status, &got.stdout, &got.stderr),
    )
    .map_err(said)?;
    let expected = carried
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, id)| id.clone())
        .ok_or_else(|| {
            said(Refusal::NotCarried {
                os: key.os.label().to_string(),
                arch: key.arch.label().to_string(),
            })
        })?;
    let landing = relay_route_core::BACKEND_LANDING_REL;
    let id = identity_at(facing, landing, relay_route_core::BACKEND_LANDING_SHELL)
        .await
        .map_err(|e| ("io_failed", e))?;
    let old = match id {
        RemoteIdentity::NoStamp => {
            let got = facing.read(landing, ENTRY_READ_MAX).await;
            if got.is_none() {
                tracing::warn!(
                    "部署计划 [{machine}]：~/{landing} 不说自己是谁，读不回来或比三行入口的上限大 —— 不当成从前那份三行入口"
                );
            }
            got
        }
        _ => None,
    };
    let action = deploy_core::landing_verdict(
        &id,
        old.as_deref(),
        &expected,
        machine,
        &format!("~/{landing}"),
    )
    .map_err(|e| ("undecidable", e))?;
    let legacy = deploy_core::legacy_verdict(
        identity_at(
            facing,
            deploy_core::LEGACY_BACKEND_REL,
            deploy_core::LEGACY_BACKEND_WORD,
        )
        .await,
    );
    Ok(Plan {
        key,
        expected,
        action,
        legacy,
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
    let (legacy, legacy_why) = match &p.legacy {
        LegacyVerdict::Absent => ("absent", None),
        LegacyVerdict::Remove => ("remove", None),
        LegacyVerdict::Keep => ("keep", None),
        LegacyVerdict::Unknown(e) => ("unknown", Some(e.clone())),
    };
    json!({
        "os": p.key.os.label(),
        "arch": p.key.arch.label(),
        "label": p.key.label(),
        "expected": p.expected,
        "action": action,
        "why": why,
        "theirs": theirs,
        "legacy": legacy,
        "legacy_why": legacy_why,
        "ack": p.ack,
    })
}

/// `carried` 那一格：`[{os, arch, id}]`。键认不出 / 身份空 ⇒ `bad_args`（monitor 交的是它自己表里的词，认不出就是两侧漂了）。
pub fn carried_of(args: &Value) -> Result<Vec<(Key, String)>, (&'static str, String)> {
    let bad = |m: &str| ("bad_args", crate::common::contract::malformed(m));
    let rows = args
        .get("carried")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("missing `carried` (array)"))?;
    rows.iter()
        .map(|r| {
            let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("");
            let key = deploy_core::key_of(s("os"), s("arch"))
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
                crate::dial::sftp::open_for_transfer(&d).await
            })
            .await
            .as_ref()
            .map_err(Clone::clone)
    }
}

impl Facing for DialFacing {
    fn exec(&self, command: String) -> Fut<'_, Result<(crate::dial::Captured, Value), String>> {
        Box::pin(crate::remote_ask::capture_full(&self.dial, command))
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

    fn read<'a>(&'a self, rel: &'a str, max: u64) -> Fut<'a, Option<Vec<u8>>> {
        Box::pin(async move {
            let s = self.session().await.ok()?;
            let size = crate::dial::sftp::metadata_size(s, rel).await.flatten()?;
            if size > max {
                return None;
            }
            crate::dial::sftp::read_all(s, rel)
                .await
                .filter(|b| b.len() as u64 <= max)
        })
    }
}

/// 帧面入口：`{dial, carried, machine?}` → 计划。
pub async fn answer(args: &Value, facing: &dyn Facing) -> Result<Value, (&'static str, String)> {
    let carried = carried_of(args)?;
    let machine = args
        .get("machine")
        .and_then(Value::as_str)
        .filter(|m| !m.trim().is_empty())
        .ok_or((
            "bad_args",
            crate::common::contract::malformed("missing `machine` (string)"),
        ))?;
    plan(facing, &carried, machine).await.map(|p| plan_json(&p))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/deploy_plan_tests.rs"]
mod tests;
