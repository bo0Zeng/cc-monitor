//! 〔HX1 · 4D〕**先请它自己收尾，等一段，还在才强杀** —— 机器页「停」本机后端那一下的形状。
//!
//! 主会话 D-a 逐字：「机器页『停』改 SIGTERM → 等 → 超时才 SIGKILL」。出处 E §E2：「停」此前直接 `SIGKILL`，
//! 后端正在写的那一条（存盘 · 递归删 · 起会话键入）被当场腰斩。后端那一侧收到 SIGTERM 会先把停不下来的那一档排空再退
//! （`src/backend/inbound.rs::exit_after_drain`）；而后端零定时器、自己不设上限 ⇒ **上限由叫它退的这一方执行**
//! （`设计/05 §3.3.2`「执行归调用方」）—— 就是本模块的 [`STOP_GRACE_TRIES`] × [`STOP_POLL`]。
//!
//! ⚠ 值住 monitor：`05 §3.3.2`「值归后端」这一格没做到（与 `dial_host::ACK_DEADLINE` 同形，如实登记）。
//! 取值：比后端自己的退出排空期限（`inbound::DRAIN_DEADLINE`，30 秒）多 5 秒 —— 正常情形后端自己先退（排空完或到期说清），
//! 这边的强杀只兜后端连那一步都走不到的形状。大文件同机复制（`files-copy`，没有进度、不可取消）可能超过它 ⇒ 被列进「没做完」。
//! ⚠ 本模块只管「怎么等」，**不认识任何平台原语**：发信号 · 看它还在不在 · 强杀，三样都由调用方注入
//! （自己起的那个 = `Child`；接管来的那个 = 按 pid 并核身份，见 `local_backend_host.rs`）。

use std::time::Duration;

/// 看一眼「它还在不在」的间隔。
pub(crate) const STOP_POLL: Duration = Duration::from_millis(100);
/// 最多看几眼（× [`STOP_POLL`] ≈ 35 秒）。**一次性条件，有次数上限** —— 登记在 `rust_timer_registry`（wait-for-condition）。
///
/// 〔HX1 · 主会话裁拍板项 1〕后端自己兜了一个退出排空期限（`src/backend/inbound.rs::DRAIN_DEADLINE`，30 秒）⇒
/// 这里要**比它久一点**：好让后端先把「哪几条没做完」说出来、自己退；这边的强杀只兜后端连那一步都走不到的情形。
/// 两边的关系由 `stop_grace_tests::s4_the_monitor_waits_a_little_longer_than_the_backend_drains` 跨半边钉住。
pub(crate) const STOP_GRACE_TRIES: u32 = 350;

/// 「停」的结局。**三态**：干净地停了 · 等不到、强杀了（在跑的写可能只做了一半）· 连强杀都没成。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StopEnd {
    /// 收到请求之后自己退了。
    Stopped,
    /// 等满了还在 ⇒ 强杀了，而且它确实没了。
    Forced,
    /// 没能让它没了：`Some(原因)` = 强杀发不出去；`None` = 强杀之后它还在。
    Stuck(Option<String>),
}

/// SIGTERM（`term`）→ 每 `poll` 看一眼 `gone`，最多 `tries` 眼 → 还在 ⇒ `kill` → 再看最多 `tries` 眼。
///
/// `term` 发不出去（例如 `kill(1)` 不在）⇒ 说一句、直接走强杀那一步 —— 发不出请求，等也等不来它自己退。
pub(crate) fn stop_gracefully(
    term: impl FnOnce() -> Result<(), String>,
    mut gone: impl FnMut() -> bool,
    kill: impl FnOnce() -> Result<(), String>,
    tries: u32,
    poll: Duration,
) -> StopEnd {
    let mut wait = |gone: &mut dyn FnMut() -> bool| -> bool {
        for _ in 0..tries {
            if gone() {
                return true;
            }
            std::thread::sleep(poll);
        }
        gone()
    };
    match term() {
        Ok(()) => {
            if wait(&mut gone) {
                return StopEnd::Stopped;
            }
        }
        Err(e) => tracing::warn!("发不出「请你收尾退出」的信号（{e}）⇒ 直接强杀"),
    }
    if let Err(e) = kill() {
        return StopEnd::Stuck(Some(e));
    }
    if wait(&mut gone) {
        StopEnd::Forced
    } else {
        StopEnd::Stuck(None)
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/stop_grace_tests.rs"]
mod tests;
