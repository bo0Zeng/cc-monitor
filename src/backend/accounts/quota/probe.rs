//! 用某个号查一次额度的本体（帧命令 `quota-probe` 的那一半，命令登记 · 入参闸 · 回包形状住 `faces/quota_probe_face.rs`）：
//! 起那一家的官方客户端报用量（不发模型请求、不花额度）、读成窗口、记进**同一本**额度账（来源 `usage`，按窗口合并）。
//!
//! - 怎么起（程序 · 参数 · 配置目录交给哪一格 · 摘 / 设哪几格环境）· 往 stdin 喂哪几行 · 读到哪一行就够 · 怎么读，
//!   都住适配层（[`crate::agents::usage_of`]）；这里只认「这一家能不能查、查回什么窗口」，不认哪一家。
//! - 声明了「读到哪一行就够」⇒ 读到那一行就收掉进程（它自己不退也不等）；没声明 ⇒ 等它退。期限都是 [`PROBE_WITHIN`]。
//! - 工作目录是这台家里不出会话的那一个（[`hidden_work_dir`]，开窗那一发同用）：起的这一次不进历史页与会话列表。
//! - 读不懂 ⇒ 照实回「读不懂 ＋ 哪一处」，不猜、不写账。

use crate::accounts::quota::ledger;
use crate::agents::UsageFace;
use crate::common::said::Said;
use crate::platform::child::{Child, ChildFail, Deadline, Heard, Until};
use crate::platform::fs::{Own, OwnPath};
use copy_core::copy_text;
use std::sync::mpsc;

/// 起官方客户端报一次用量的期限（它要起进程、读登录、问一次上游）。
const PROBE_WITHIN: Deadline = Deadline::secs(30);

/// 报错原话里带官方客户端的输出至多这么多个字（截了接「…」）。
const SAID_CHARS: usize = 200;

/// ★ 家里那个不出会话的工作目录（数据目录底下一层，没有就经写句柄建）：查额度与开窗那一发都在这里起。
/// 家推不出来 · 建不了 ⇒ `io_failed` 那一句。
pub(crate) fn hidden_work_dir(own: &Own) -> Result<OwnPath, Said> {
    let cwd = own
        .path(crate::common::own_dir::HIDDEN_WORK_DIR)
        .map_err(|_| Said::new("io_failed", copy_text("beQuotaLedger.read.noHome", &[])))?;
    own.ensure_dir(&cwd).map_err(|e| {
        crate::wire::spec::fail(
            "io_failed",
            copy_text(
                "beQuotaProbe.workDir.failed",
                &[("why", &copy_core::io_reason(e.kind()))],
            ),
        )
        .and_raw(Some(&e.to_string()))
    })?;
    Ok(cwd)
}

/// 查回来的：读得懂（已记进额度账）· 读不懂（哪一处，英文诊断；不写账）。
pub enum Probe {
    Read(Vec<crate::agents::QuotaWindow>),
    Unreadable(String),
}

/// 查哪一个：那一家的路由 · 线上的 kind（只进那一句）· 号 · 此刻 · 起会话那个 shell 的 `PATH`
/// （判据交一个放着假客户端的目录；生产 `None` ＝ 现问，见 `platform::shell::agent_launch`）。
pub struct Ask<'a> {
    pub route: crate::agents::RouteId,
    pub kind: &'a str,
    pub account: &'a str,
    pub now: u64,
    pub path: Option<&'a str>,
}

/// `kind` 那一家没有报用量那一形时的那一句：照 `registry` 说那一家的叫法（认不出 ⇒ 照实那一句，不露 kind）。
pub(crate) fn unsupported_among(registry: &[crate::agents::Adapter], kind: &str) -> Said {
    Said::new(
        "unsupported",
        copy_text(
            "beQuotaProbe.agent.unsupported",
            &[("agent", &crate::agents::family_said_among(registry, kind))],
        ),
    )
}

/// ★ 用 `ask.account` 查一次额度：号照这台的账号库（`lib`）找，读到的记进额度账 `quota`。
pub fn probe(
    lib: &acct_core::AccountBook,
    quota: &ledger::Ledger,
    own: &Own,
    ask: &Ask<'_>,
) -> Result<Probe, Said> {
    let face = crate::agents::usage_of(ask.route)
        .ok_or_else(|| unsupported_among(crate::agents::REGISTRY, ask.kind))?;
    probe_by(
        crate::agents::REGISTRY,
        lib,
        quota,
        own,
        face,
        ask.route.id(),
        ask,
    )
}

/// 它跑完（或读够）时交回的：成没成 · 退出码（读够就收的 ⇒ 没有）· 两条输出。
struct Ran {
    ok: bool,
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// 照 `face` 那一形查一次，读到的记在 `agent` 名下（`ask.route` 不看：那一家已由 `face` · `agent` 说定）。
/// 号是号表里 `agent` 那一家的 `ask.account`（号按（家, 号名）认，号是哪一家照注册表 `reg`）。
/// 判据拿不进注册表的那一家直接交它那一形与一张替身注册表。
pub(crate) fn probe_by(
    reg: &[crate::agents::Adapter],
    lib: &acct_core::AccountBook,
    quota: &ledger::Ledger,
    own: &Own,
    face: UsageFace,
    agent: &str,
    ask: &Ask<'_>,
) -> Result<Probe, Said> {
    let (account, now, path) = (ask.account, ask.now, ask.path);
    let found = crate::accounts::quota::rules::decide::in_family_among(reg, lib, agent, account)
        .ok_or_else(|| {
            Said::new(
                "not_found",
                copy_text("beQuotaProbe.account.notFound", &[("account", account)]),
            )
        })?;
    if found.is_api_key() {
        return Err(Said::new(
            "unsupported",
            copy_text("beQuotaProbe.account.apiKey", &[("account", account)]),
        ));
    }
    if quota.path().is_none() {
        return Err(Said::new(
            "io_failed",
            copy_text("beQuotaLedger.read.noHome", &[]),
        ));
    }
    let cwd = hidden_work_dir(own)?;
    let failed = |why: &str| {
        copy_text(
            "beQuotaProbe.run.failed",
            &[("program", face.program), ("why", why)],
        )
    };
    let child_failed = |e: ChildFail| {
        let (code, said) = e.into_cmd_said("failed", failed);
        crate::wire::spec::fail(code, said)
    };
    let at = crate::platform::shell::agent_launch(face.program, path)
        .map_err(|why| crate::wire::spec::fail("failed", failed(&why)))?;
    #[expect(
        clippy::disallowed_methods,
        reason = "起那一家官方客户端报一次某个号的用量（适配层登记的那一形，argv 不过 shell、stdin 只喂适配层登记的那几行、期限 30 秒）：它照自己的样子用自己的配置目录，后端不写；不许加第二种用途"
    )]
    let mut child = Child::new(at.program)
        .args(at.lead)
        .args(face.args)
        .current_dir(cwd.as_path());
    child = match &found.config_dir {
        Some(d) => child.env(face.dir_env, d),
        None => child.env_remove(face.dir_env),
    };
    for k in face.env_remove {
        child = child.env_remove(k);
    }
    for (k, v) in face.env_set {
        child = child.env(k, v);
    }
    let ran = match face.enough {
        Some(enough) => until_enough(child, &face, enough).map_err(child_failed)?,
        None => {
            if !face.stdin.is_empty() {
                child = child.stdin_bytes(fed_bytes(face.stdin));
            }
            let out = child.run(PROBE_WITHIN).map_err(child_failed)?;
            Ran {
                ok: out.status.success(),
                code: out.status.code(),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            }
        }
    };
    if !ran.ok {
        let stderr = copy_core::clip(
            copy_core::first_line(&ran.stderr)
                .or_else(|| copy_core::first_line(&ran.stdout))
                .unwrap_or(""),
            SAID_CHARS,
        );
        let code = ran.code.map_or_else(|| "-".to_string(), |c| c.to_string());
        // 退出码与它说的那一行是原话：进复制详情，不上句子。
        return Err(crate::wire::spec::fail(
            "failed",
            copy_text("beQuotaProbe.run.exit", &[("program", face.program)]),
        )
        .and_raw(Some(&format!("exit {code}: {stderr}"))));
    }
    match (face.read)(&ran.stdout, now) {
        Err(why) => Ok(Probe::Unreadable(why)),
        Ok(windows) => {
            ledger::record_probe(quota, own, agent, account, windows.clone(), now)
                .map_err(|e| crate::wire::spec::fail("io_failed", e))?;
            Ok(Probe::Read(windows))
        }
    }
}

/// 那几行喂进 stdin 的字节（每行自带换行）。
fn fed_bytes(lines: &[&str]) -> Vec<u8> {
    lines
        .iter()
        .flat_map(|l| [*l, "\n"])
        .collect::<String>()
        .into_bytes()
}

/// 声明了「读到哪一行就够」的那一种：起它、依次喂那几行（stdin 留着不关）、逐行读标准输出，
/// 读到 `enough` 认的那一行 ⇒ 收掉它（放手即杀组）、交回读到的那几行；它先退了 ⇒ 照它退出的结局交。
/// 过了 [`PROBE_WITHIN`] 还没够 ⇒ 收掉它、回超时。
fn until_enough(
    child: Child,
    face: &UsageFace,
    enough: fn(&str) -> bool,
) -> Result<Ran, ChildFail> {
    let until = Until::after(PROBE_WITHIN);
    let mut conv = child.converse()?;
    let heard = conv
        .take_heard()
        .ok_or_else(|| ChildFail::Io(std::io::Error::from(std::io::ErrorKind::BrokenPipe)))?;
    for line in face.stdin {
        if let Err(e) = conv.say(line) {
            // 它先退了 · 不读 stdin ⇒ 照它退出的结局交，不当起不来（同 `Child::run` 喂 stdin 那一处）。
            tracing::debug!(
                "{} 没读完喂给它的 stdin（{e}）⇒ 照它退出的结局交",
                face.program
            );
            break;
        }
    }
    let (mut stdout, mut stderr) = (String::new(), String::new());
    let (ok, code) = loop {
        match next_heard(&heard, until) {
            Ok(Heard::Out(l)) => {
                stdout.push_str(&l);
                stdout.push('\n');
                if enough(&l) {
                    break (true, None);
                }
            }
            Ok(Heard::Err(l)) => {
                stderr.push_str(&l);
                stderr.push('\n');
            }
            Ok(Heard::Exited(code)) => break (code == Some(0), code),
            // 两条输出都到头、它退了的那一件却没来（收尸那一侧没了）⇒ 说不出退出码，当没成
            Err(mpsc::RecvTimeoutError::Disconnected) => break (false, None),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(ChildFail::TimedOut {
                    program: face.program.to_string(),
                    after: PROBE_WITHIN,
                })
            }
        }
    };
    // 放手 `conv` ⇒ 收掉它（够了时它还没退；已退的不再杀）。
    Ok(Ran {
        ok,
        code,
        stdout,
        stderr,
    })
}

/// 它说的下一件，至多等到 `until`（[`PROBE_WITHIN`] 从起它那一刻算；钟面装不下 ⇒ 按整段期限等）。
#[expect(
    clippy::disallowed_methods,
    reason = "报用量读够就收的那一种：等它说到够的有界等待（期限即 `PROBE_WITHIN`，从起它那一刻算）；它卡住不说时没有它这条命令就一直挂着"
)]
fn next_heard(
    heard: &mpsc::Receiver<Heard>,
    until: Option<Until>,
) -> Result<Heard, mpsc::RecvTimeoutError> {
    let left = until.map_or_else(|| PROBE_WITHIN.as_ms(), Until::left_ms);
    heard.recv_timeout(std::time::Duration::from_millis(left))
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/probe_tests.rs"]
mod tests;
