//! 读屏驱动的 `settle`：屏还不是认得的样子 ⇒ 等一次重画再读，至多那几次；认到就停，次数用完 · 不再重画 ⇒ 回最后读到的那一屏。

use super::*;

/// 假屏：按一下之后要过 `lag` 次重画才换成下一样；`still` 为真 ⇒ 再也不重画。
struct Screen {
    shown: u32,
    pending: Option<(u32, usize)>,
    lag: usize,
    still: bool,
    waits: usize,
}

impl Screen {
    fn new(lag: usize) -> Screen {
        Screen {
            shown: 0,
            pending: None,
            lag,
            still: false,
            waits: 0,
        }
    }
    fn press(&mut self) {
        self.pending = Some((self.shown + 1, self.lag));
    }
    /// 等一次重画：变了 ⇒ 读到的那一样。
    fn next(&mut self) -> Result<Option<u32>, Said> {
        self.waits += 1;
        if self.still {
            return Ok(None);
        }
        if let Some((to, left)) = self.pending {
            if left <= 1 {
                self.shown = to;
                self.pending = None;
            } else {
                self.pending = Some((to, left - 1));
            }
        }
        Ok(Some(self.shown))
    }
}

fn run(s: &mut Screen, redraws: usize) -> u32 {
    let now = s.shown;
    settle(now, redraws, |&n| n != now, || s.next()).unwrap()
}

/// ★ 按一下、过三次重画才换样子：次数够 ⇒ 认到、只等到认到为止。
#[test]
fn it_waits_until_the_screen_reads_as_wanted() {
    let mut s = Screen::new(3);
    s.press();
    assert_eq!(run(&mut s, REDRAWS_PER_PRESS), 1);
    assert_eq!(s.waits, 3, "认到了还在等");
}

/// ★ 次数用完还没换样子 ⇒ 回最后读到的那一屏（不多等一次）。
#[test]
fn it_gives_up_after_the_redraw_budget() {
    let mut s = Screen::new(REDRAWS_PER_PRESS + 1);
    s.press();
    assert_eq!(run(&mut s, REDRAWS_PER_PRESS), 0);
    assert_eq!(s.waits, REDRAWS_PER_PRESS);
}

/// ★ 不再重画 ⇒ 当场停，回此刻那一屏。
#[test]
fn it_stops_when_the_screen_no_longer_redraws() {
    let mut s = Screen::new(1);
    s.press();
    s.still = true;
    assert_eq!(run(&mut s, REDRAWS_PER_PRESS), 0);
    assert_eq!(s.waits, 1);
}

/// ★ 一开始就是认得的样子 ⇒ 一次都不等。
#[test]
fn it_does_not_wait_when_already_there() {
    let mut s = Screen::new(1);
    assert_eq!(
        settle(7_u32, REDRAWS_PER_PRESS, |&n| n == 7, || s.next()).unwrap(),
        7
    );
    assert_eq!(s.waits, 0);
}

/// 托管终端那一份驱动（判据直接给托管口，不经名单认）。
fn hosted(sock: &std::path::Path) -> Drive<'static> {
    Drive {
        sid: "",
        client: None,
        watch: Some(Watch::hosted(sock).expect("连上托管口")),
        hosted: Some(sock.to_path_buf()),
    }
}

fn shows(s: &[String], what: &str) -> bool {
    s.iter().any(|l| l.trim_end() == what)
}

/// 等到屏上出现 `what`（子进程起得慢也等得到）。
fn wait_for(d: &mut Drive<'_>, what: &str) {
    let now = d.screen().expect("取屏");
    let got = settle(
        now,
        64,
        |s| shows(s, what),
        || d.redrawn().then(|| d.screen()).transpose(),
    )
    .expect("取屏");
    assert!(shows(&got, what), "一直没等到 {what}：{got:?}");
}

/// ★ 托管终端：送一行之后它先后推两帧、第二帧才是要的样子 ⇒ 每推一帧读一次，认到第二帧就回；
/// 看屏不起 tmux（托管口的推送当重画）。真起托管进程（假子进程 `sh`，沙箱数据目录）。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hosted_terminal_settles_on_the_frame_it_pushes() {
    use crate::host::managed::run::test_support::{sandbox_own, spec, start};
    let (own, _data) = sandbox_own("drive-two");
    let (sock, _task) = start(
        &own,
        spec(
            "t-drv2",
            "printf 'ready\\r\\n'; read x; printf 'one\\r\\n'; sleep 0.3; printf 'two\\r\\n'; read y",
        ),
    )
    .await;
    tokio::task::spawn_blocking(move || {
        let mut d = hosted(&sock);
        wait_for(&mut d, "ready");
        d.watch.as_mut().expect("看着").forget();
        crate::host::managed::client::say(&sock, b"x\r").expect("送进去");
        let now = d.screen().expect("取屏");
        let got = settle(
            now,
            REDRAWS_AFTER_SEND,
            |s| shows(s, "two"),
            || d.redrawn().then(|| d.screen()).transpose(),
        )
        .expect("取屏");
        assert!(
            shows(&got, "one") && shows(&got, "two"),
            "没认到第二帧：{got:?}"
        );
    })
    .await
    .expect("没崩");
}

/// ★ 托管终端一字不推 ⇒ 等到期限照实回原来那一屏（不当成变了）。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hosted_terminal_that_pushes_nothing_gives_back_the_same_screen() {
    use crate::host::managed::run::test_support::{sandbox_own, spec, start};
    let (own, _data) = sandbox_own("drive-still");
    let (sock, _task) = start(&own, spec("t-drv0", "printf 'ready\\r\\n'; read x")).await;
    tokio::task::spawn_blocking(move || {
        let mut d = hosted(&sock);
        wait_for(&mut d, "ready");
        d.watch.as_mut().expect("看着").forget();
        let now = d.screen().expect("取屏");
        let mut waits = 0;
        let got = settle(
            now.clone(),
            REDRAWS_PER_PRESS,
            |s| shows(s, "never"),
            || {
                waits += 1;
                d.redrawn().then(|| d.screen()).transpose()
            },
        )
        .expect("取屏");
        assert_eq!(got, now, "没推也换了一屏");
        assert_eq!(waits, 1, "到期没变还在等");
    })
    .await
    .expect("没崩");
}
