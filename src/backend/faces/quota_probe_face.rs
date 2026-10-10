//! 帧命令 `quota-probe`：用某个号查一次额度 —— 起那一家的官方客户端报用量（不发模型请求、不花额度）、读成窗口、
//! 记进**同一本**额度账（来源 `usage`，按窗口合并）。同一种事实的第二个来源，不是第二套额度。
//!
//! - 怎么起（程序 · 参数 · 配置目录交给哪一格 · 摘 / 设哪几格环境）与怎么读都住适配层（[`crate::agents::usage_of`]）；
//!   这里只认「这一家能不能查、查回什么窗口」。
//! - 工作目录是这台家里不出会话的那一个（[`crate::common::own_dir::HIDDEN_WORK_DIR`]，没有就建）：起的这一次不进历史页与会话列表。
//! - 读不懂 ⇒ 照实回 `unreadable` ＋ 哪一处读不懂，不猜、不写账。
//! - 动作、不定时：界面「刷新」· skill · AI 按需调。

use crate::accounts::quota::ledger;
use crate::accounts::upstream_select::rotate::account_ok;
use crate::faces::rotation_face::Ctx;
use crate::platform::child::{Child, Deadline};
use copy_core::copy_text;
use serde_json::{json, Value};
use std::ffi::OsStr;

/// 失败带码 ＋ 那一句 ＋ 原话（落账写不进那一形的原话进复制详情，[`Fail`]）。
type Answer = Result<Value, crate::stream::inbound::spec::Fail>;

/// 起官方客户端报一次用量的期限（它要起进程、读登录、问一次上游）。
const PROBE_WITHIN: Deadline = Deadline::secs(30);

/// 这条命令的总期限（分派那一层装；略长于那一次子进程）。
pub(crate) const PROBE_CAP: Deadline = Deadline::secs(32);

/// 报错原话里带官方客户端的输出至多这么多个字。
const SAID_CHARS: usize = 200;

fn bad(detail: &str) -> crate::stream::inbound::spec::Fail {
    crate::stream::inbound::spec::Fail::new("bad_args", crate::common::contract::malformed(detail))
}

/// `quota-probe`：`{agent, account}` ⇒ 读到的窗口（并已记进额度账）或「读不懂」。
pub(crate) fn answer_probe(args: &Value) -> Answer {
    answer_probe_with(&Ctx::here(), args, crate::accounts::quota::now_unix(), None)
}

/// 本体。`path` ＝ 子进程的 `PATH`（判据交一个放着假客户端的目录；生产 `None` ＝ 照宿主的）。
pub(crate) fn answer_probe_with(ctx: &Ctx, args: &Value, now: u64, path: Option<&OsStr>) -> Answer {
    let agent = args
        .get("agent")
        .and_then(Value::as_str)
        .filter(|a| crate::relay::segment_is_safe(a))
        .ok_or_else(|| bad("`agent` must be a route id such as \"claude-code\""))?;
    let account = args
        .get("account")
        .and_then(Value::as_str)
        .filter(|a| account_ok(a))
        .ok_or_else(|| bad("`account` must be an account id"))?;
    if let Some(k) = args.as_object().and_then(|o| {
        o.keys()
            .find(|k| !matches!(k.as_str(), "agent" | "account"))
    }) {
        return Err(bad(&format!("unknown field `{k}`")));
    }
    let face = crate::agents::usage_of(agent).ok_or_else(|| {
        (
            "unsupported",
            copy_text("beQuotaProbe.agent.unsupported", &[("agent", agent)]),
        )
    })?;
    let lib = ctx.hop.library();
    let found = lib
        .accounts
        .iter()
        .find(|a| a.id == account)
        .ok_or_else(|| {
            (
                "not_found",
                copy_text("beQuotaProbe.account.notFound", &[("account", account)]),
            )
        })?;
    if found.api {
        return Err((
            "unsupported",
            copy_text("beQuotaProbe.account.apiKey", &[("account", account)]),
        )
            .into());
    }
    let book = ctx.hop.quota.path();
    let home = book
        .and_then(std::path::Path::parent)
        .ok_or_else(|| ("io_failed", copy_text("beQuotaLedger.read.noHome", &[])))?;
    let cwd = crate::common::own_dir::ensure_hidden_work_dir(home).map_err(|e| {
        crate::stream::inbound::spec::Fail::new(
            "io_failed",
            copy_text(
                "beQuotaProbe.workDir.failed",
                &[("why", &copy_core::io_reason(e.kind()))],
            ),
        )
        .with_raw(Some(&e.to_string()))
    })?;
    let mut child = Child::new(face.program).args(face.args).current_dir(&cwd);
    child = match &found.dir {
        Some(d) => child.env(face.dir_env, d),
        None => child.env_remove(face.dir_env),
    };
    for k in face.env_remove {
        child = child.env_remove(k);
    }
    for (k, v) in face.env_set {
        child = child.env(k, v);
    }
    if let Some(p) = path {
        child = child.env("PATH", p);
    }
    let out = child.run(PROBE_WITHIN).map_err(|e| {
        crate::stream::inbound::spec::Fail::from(e.into_cmd_said("failed", |why| {
            copy_text(
                "beQuotaProbe.run.failed",
                &[("program", face.program), ("why", why)],
            )
        }))
    })?;
    let text = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() {
        let stderr: String = String::from_utf8_lossy(&out.stderr)
            .lines()
            .chain(text.lines())
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("")
            .chars()
            .take(SAID_CHARS)
            .collect();
        let code = out
            .status
            .code()
            .map_or_else(|| "-".to_string(), |c| c.to_string());
        // 退出码与它说的那一行是原话：进复制详情，不上句子。
        return Err(crate::stream::inbound::spec::Fail::new(
            "failed",
            copy_text("beQuotaProbe.run.exit", &[("program", face.program)]),
        )
        .with_raw(Some(&format!("exit {code}: {stderr}"))));
    }
    let shown = book.map(|p| p.display().to_string());
    match (face.read)(&text, now) {
        Err(why) => Ok(json!({
            "agent": agent, "account": account, "from": "usage", "now": now,
            "state": "unreadable", "reason": why, "windows": [], "path": shown,
        })),
        Ok(windows) => {
            ledger::record_probe(&ctx.hop.quota, agent, account, windows.clone(), now)
                .map_err(|e| ("io_failed", e))?;
            Ok(json!({
                "agent": agent, "account": account, "from": "usage", "now": now,
                "state": "read", "reason": null, "windows": windows, "path": shown,
            }))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/quota_probe_face_tests.rs"]
mod tests;
