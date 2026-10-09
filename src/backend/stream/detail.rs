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

fn now() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let off = crate::platform::local_tz::offset_secs(t).unwrap_or(0);
    stamp(i64::try_from(t).unwrap_or(i64::MAX), off)
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
