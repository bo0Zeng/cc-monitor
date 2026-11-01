//! 后端这一端写的「复制详情」（失败应答的 `detail` 那一格）：句子下面那几行 —— 时刻 · 机器 · 命令 · 码 · 原话。
//! 排法只住 `copy_core::detail::failure` 一份（壳排它自己的错也调它）；这里只管后端知道的那几样事实从哪来。
//!
//! 机器那一项不写主机名（贴出去的那段不带这台的名字；界面上对这台的称呼由本机壳在「本机」那一行旁边知道）。

use copy_core::copy_text;
use copy_core::detail::{failure, os_word, Host};
use copy_core::discard::{Discard, Why};

/// 这台后端此刻的一份详情。`cmd` 缺 ＝ 协议级失败（还没落到哪条命令上）；`raw` 是下层原话（子进程 stderr · 系统报错）。
pub(crate) fn of(cmd: Option<&str>, code: &str, raw: Option<&str>) -> String {
    failure(&now(), Host::Machine(&machine()), cmd, Some(code), raw)
}

/// 一趟跑起来之后才停下的事（传输收场帧的 `failed` 那一格：没有命令级的码）：时刻 · 机器 · 命令 · 原话。
pub(crate) fn of_run(cmd: &str, raw: Option<&str>) -> String {
    failure(&now(), Host::Machine(&machine()), Some(cmd), None, raw)
}

/// **一次失败投出去的那一份**（[`cmd_wire::failed::Failed`]）：帧面的失败应答与 CLI 面的失败信封都从它出（两个面只是装运不同：一帧 `reply` · stderr 一行）。
pub(crate) use cmd_wire::failed::Failed;

/// 拼一份投出去的失败：`cmd` 缺 ＝ 协议级失败（还没落到哪条命令上）；`raw` 是下层原话（进详情、不进句子）。
pub(crate) fn failed(
    cmd: Option<&str>,
    code: &str,
    said: String,
    raw: Option<&str>,
    data: Option<serde_json::Value>,
) -> Failed {
    // 契约错的诊断跟在句子后面（`contract::malformed`）⇒ 拆开：句子留给人看，诊断进原话那一行。
    let (said, raw) = crate::common::contract::parts(&said, raw);
    Failed {
        code: code.to_string(),
        detail: of(cmd, code, raw.as_deref()),
        said,
        data,
    }
}

/// 给人看那一形（CLI 的 `--text`）：那一句 ＋ 下面原样接复制详情（同界面［复制详情］复制出去的那一段）。
/// `tz` ＝ 看的那一台的时区：详情里那一行时刻按它填。
pub(crate) fn text_of(f: &Failed, tz: &crate::common::time::Tz) -> String {
    crate::common::time::fill_at(&copy_core::detail::one(&f.said, &f.detail), tz).into_owned()
}

/// 投到 CLI 面（**唯一一处**：CLI 控制面与常驻后端起停那几条都经这里）：
/// stderr 一行 `{code, said, detail, data?}`，退出 2；`text`（[`crate::wire::cli_words::TEXT_FLAG`]）⇒ 那一句 ＋ 下面原样接复制详情。
/// `tz` ＝ 看的那一台的时区（`--tz`）：详情里那一行时刻按它填。
pub(crate) fn emit_to(
    f: &Failed,
    err: &mut dyn std::io::Write,
    text: bool,
    tz: &crate::common::time::Tz,
) -> i32 {
    let line = if text {
        text_of(f, tz)
    } else {
        serde_json::to_string(f)
            .map(|l| crate::common::time::fill_at(&l, tz).into_owned())
            .unwrap_or_else(|_| text_of(f, tz))
    };
    writeln!(err, "{line}").discard(Why::Diag); // CLI 面的失败信封写 stderr（各 CLI 出口都经 `Failed::emit_to` 这一处）：写不进去就没有第二个地方可以说（退出码 2 照样回）
    2
}

/// 读不出来那一形在成功应答里的两格：`reason` 是那一句（不带原话）· `detail` 是复制详情（排法同失败应答，码 `unreadable`）。
/// 读得到 / 不在 ⇒ 两格都是 `null`。各读答（额度账 · 轮换 · 退出策略 · 额度探针）都经这里，不各拼一份。
pub(crate) fn unreadable(
    cmd: &str,
    why: Option<&crate::common::said::Said>,
) -> (Option<String>, Option<String>) {
    match why {
        None => (None, None),
        Some(s) => (
            Some(s.said.clone()),
            Some(of(Some(cmd), "unreadable", s.raw.as_deref())),
        ),
    }
}

/// 此刻那一行：先写成空位（[`crate::common::time::at_slot`]），出去那一下按看的那一台的时区填（带偏移）。
fn now() -> String {
    crate::common::time::at_slot(crate::common::time::now_secs())
}

/// `Linux x86_64 · 后端 p9k-…`。
fn machine() -> String {
    format!(
        "{} {} · {}",
        os_word(std::env::consts::OS),
        std::env::consts::ARCH,
        copy_text("detail.value.backend", &[("build", crate::BUILD_ID)])
    )
}

/// 一次性 CLI 面的失败出口（CLI 面分派 `stream::cli` 与 `control/resident.rs` 那两条子命令都走这一份，不另立第 N 份信封，`readonly_guard::error_envelope_registry`）。
/// `cmd` 进复制详情的「命令」那一项；`said` 可带下层原话（`Said.raw`），原话进复制详情、不上句子。
pub fn emit_err(cmd: &str, code: &str, said: impl Into<copy_core::said::Said>) -> i32 {
    // 这几条（常驻起停那两问）不收 `--tz` ⇒ 详情那一行时刻按 UTC 填（同「没带按 UTC」）。
    emit_to(
        &failed_of(cmd, code, said.into()),
        &mut std::io::stderr(),
        false,
        &crate::Tz::default(),
    )
}

/// [`emit_err`] 交出去的那一份：那一句进 `said`，下层原话进复制详情（[`failed`] 的 `raw`）。
pub(crate) fn failed_of(cmd: &str, code: &str, s: copy_core::said::Said) -> Failed {
    failed(Some(cmd), code, s.said, s.raw.as_deref(), None)
}
