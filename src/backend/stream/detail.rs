//! 后端这一端写的「复制详情」（失败应答的 `detail` 那一格）：句子下面那几行 —— 时刻 · 机器 · 命令 · 码 · 原话。
//! 排法只住 `copy_core::detail` 一份；这里只管后端知道的那几样事实从哪来。
//!
//! 机器那一项不写主机名（贴出去的那段不带这台的名字；界面上对这台的称呼由本机壳在「本机」那一行旁边知道）。

use copy_core::copy_text;
use copy_core::detail::{os_word, stamp, Detail, Label};

/// 这台后端此刻的一份详情。`cmd` 缺 ＝ 协议级失败（还没落到哪条命令上）；`raw` 是下层原话（子进程 stderr · 系统报错）。
pub(crate) fn of(cmd: Option<&str>, code: &str, raw: Option<&str>) -> String {
    Detail::new()
        .item(Label::At, now())
        .item(Label::Machine, machine())
        .maybe(Label::Command, cmd)
        .item(Label::Code, code)
        .maybe(Label::Raw, raw)
        .render()
}

/// 一趟跑起来之后才停下的事（传输收场帧的 `failed` 那一格：没有命令级的码）：时刻 · 机器 · 命令 · 原话。
pub(crate) fn of_run(cmd: &str, raw: Option<&str>) -> String {
    Detail::new()
        .item(Label::At, now())
        .item(Label::Machine, machine())
        .item(Label::Command, cmd)
        .maybe(Label::Raw, raw)
        .render()
}

/// **一次失败投出去的那一份**：帧面的失败应答与 CLI 面的失败信封都从它出（两个面只是装运不同：一帧 `reply` · stderr 一行）。
/// `code` 给程序认；`message` 是给人看的那一句；`detail` 是复制详情那几行（恒在）；`data` 只给按码定了形的那几个码。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct Failed {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data: Option<serde_json::Value>,
}

impl Failed {
    /// `cmd` 缺 ＝ 协议级失败（还没落到哪条命令上）；`raw` 是下层原话（进详情、不进句子）。
    pub(crate) fn new(
        cmd: Option<&str>,
        code: &str,
        message: String,
        raw: Option<&str>,
        data: Option<serde_json::Value>,
    ) -> Failed {
        Failed {
            code: code.to_string(),
            detail: of(cmd, code, raw),
            message,
            data,
        }
    }

    /// 给人看那一形（CLI 的 `--text`）：那一句 ＋ 下面原样接复制详情（同界面［复制详情］复制出去的那一段）。
    /// `tz` ＝ 看的那一台的时区：详情里那一行时刻按它填。
    pub(crate) fn text(&self, tz: &crate::common::time::Tz) -> String {
        crate::common::time::fill_at(&copy_core::detail::one(&self.message, &self.detail), tz)
            .into_owned()
    }

    /// 投到 CLI 面（**唯一一处**：CLI 控制面 · `--list-projects` · `--fork-session` · `--account-trust*` 都经这里）：
    /// stderr 一行 `{code, message, detail, data?}`，退出 2；`text`（[`crate::TEXT_FLAG`]）⇒ 那一句 ＋ 下面原样接复制详情。
    /// `tz` ＝ 看的那一台的时区（`--tz`）：详情里那一行时刻按它填。
    pub(crate) fn emit_to(
        &self,
        err: &mut dyn std::io::Write,
        text: bool,
        tz: &crate::common::time::Tz,
    ) -> i32 {
        let line = if text {
            self.text(tz)
        } else {
            serde_json::to_string(self)
                .map(|l| crate::common::time::fill_at(&l, tz).into_owned())
                .unwrap_or_else(|_| self.text(tz))
        };
        let _ = writeln!(err, "{line}");
        2
    }

    /// [`Failed::emit_to`] 的 stderr · JSON 那一形（不收 `--text` 的那几条 CLI）。
    pub(crate) fn emit(&self, tz: &crate::common::time::Tz) -> i32 {
        self.emit_to(&mut std::io::stderr(), false, tz)
    }
}

/// 读不出来那一形在成功应答里的两格：`reason` 是那一句（不带原话）· `detail` 是复制详情（排法同失败应答，码 `unreadable`）。
/// 读得到 / 不在 ⇒ 两格都是 `null`。各读答（额度账 · 轮换 · 退出策略 · 额度探针）都经这里，不各拼一份。
pub(crate) fn unreadable(
    cmd: &str,
    why: Option<&crate::common::said::Said>,
) -> (serde_json::Value, serde_json::Value) {
    match why {
        None => (serde_json::Value::Null, serde_json::Value::Null),
        Some(s) => (
            serde_json::Value::String(s.said.clone()),
            serde_json::Value::String(of(Some(cmd), "unreadable", s.raw.as_deref())),
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
