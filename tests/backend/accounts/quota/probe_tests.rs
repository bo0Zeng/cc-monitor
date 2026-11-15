//! 报用量往 stdin 喂几行 · 读够就收：拿假 agent 那一形跑通用层（不起真 claude / codex）。
//! 假 agent 的报用量程序是一个回显 stdin 的小程序（`agents::fake::upstream::ECHO_PROGRAM`），放进本判据自己的 `PATH` 目录；
//! 它把喂进来的每一行记进旁边的 `fed.txt`、回一行 `echo: …`，stdin 不关就一直等着。

use super::*;
use crate::accounts::quota::ledger::{self, Ledger};
use crate::agents::fake::upstream;
use crate::agents::UsageFace;

/// 2026-03-05 12:00 UTC。
const NOW: u64 = 1_772_712_000;

/// 额度账里记在哪一家名下（假 agent 不进注册表，通用层只当不透明的键）。
const AGENT: &str = "fake";

struct Home {
    root: std::path::PathBuf,
    bin: std::path::PathBuf,
    quota: Ledger,
}

impl Home {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ccm-feed-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir");
        let prog = bin.join(upstream::PROGRAM);
        // 马上被起 ⇒ 经 `write_exe` 写（本进程写的写端会漏进别的判据线程 fork 出来的子进程 ⇒ ETXTBSY）。
        #[cfg(unix)]
        test_home::write_exe(&prog, upstream::ECHO_PROGRAM);
        #[cfg(not(unix))]
        std::fs::write(&prog, upstream::ECHO_PROGRAM).expect("echo program");
        std::fs::create_dir_all(root.join("accts").join("b")).expect("mkdir");
        let quota = Ledger::at(Some(root.join(ledger::FILE_NAME)));
        Self { root, bin, quota }
    }

    /// 用号 `b` 照 `face` 查一次（号表只有假 agent 那一家的 `b`）。
    fn probe(&self, face: UsageFace) -> Result<Probe, crate::common::said::Said> {
        let lib = acct_core::AccountBook {
            accounts: vec![self.row("b", Some(AGENT), false)],
        };
        let reg = [crate::agents::fake::full_row(|| None)];
        self.probe_in(&reg, &lib, face, AGENT, "b")
    }

    /// 号表里的一行：`agent` 那一家（`None` ＝ 清单没写，归缺省那一家）的号 `id`，号目录在本判据的家里。
    fn row(&self, id: &str, agent: Option<&str>, api: bool) -> acct_core::ManifestAccount {
        acct_core::ManifestAccount {
            agent: agent.map(str::to_string),
            ..crate::accounts::upstream_select::rotate::test_support::book_row(
                id,
                Some(self.root.join("accts").join(id)),
                api,
            )
        }
    }

    /// 在注册表 `reg` · 号表 `lib` 上用 `agent` 那一家的号 `account` 照 `face` 查一次。
    fn probe_in(
        &self,
        reg: &[crate::agents::Adapter],
        lib: &acct_core::AccountBook,
        face: UsageFace,
        agent: &str,
        account: &str,
    ) -> Result<Probe, crate::common::said::Said> {
        let path = format!("{}:/usr/bin:/bin", self.bin.display());
        let ask = Ask {
            route: crate::agents::pick_route(None).expect("默认那一家"),
            kind: agent,
            account,
            now: NOW,
            path: Some(path.as_str()),
        };
        probe_by(
            reg,
            lib,
            &self.quota,
            &crate::platform::fs::Own::issue_data_at(&self.root),
            face,
            agent,
            &ask,
        )
    }

    /// 回显程序收到的那几行（它一行都没收到 ⇒ 空）。
    fn fed(&self) -> Vec<String> {
        std::fs::read_to_string(self.bin.join("fed.txt"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// ★ 声明 stdin 两行、`enough` 认第二行的回显 ⇒ 起的进程恰收到那两行；读到第二行的回显就收掉它（它自己不退）；
/// 回包翻成两窗、记进额度账。
#[cfg(unix)]
#[test]
fn the_declared_lines_are_fed_and_the_echo_is_booked_as_two_windows() {
    let home = Home::new("two");
    let got = home.probe(upstream::USAGE).expect("查得成");
    let Probe::Read(windows) = got else {
        panic!("该读得懂");
    };
    let names: Vec<&str> = windows.iter().map(|w| w.name.as_str()).collect();
    assert_eq!(names, ["burst", "long"]);
    assert_eq!(windows[0].used, Some(0.25));
    assert_eq!(windows[0].resets_at, Some(NOW + 3600));
    assert_eq!(windows[1].resets_at, Some(NOW + 86400));
    assert_eq!(home.fed(), upstream::USAGE.stdin, "恰收到声明的那两行");
    let booked = home.quota.entry(AGENT, "b").expect("进了额度账");
    let booked: Vec<&str> = booked
        .reading
        .windows
        .iter()
        .map(|w| w.name.as_str())
        .collect();
    assert_eq!(booked, ["burst", "long"]);
}

/// ★ 不声明 stdin（也不声明 `enough`）⇒ 一行不喂，等它自己退（读到 stdin 头就退）；没有回显 ⇒ 读不懂、账不动。
#[cfg(unix)]
#[test]
fn without_declared_lines_nothing_is_fed() {
    let home = Home::new("none");
    let face = UsageFace {
        stdin: &[],
        enough: None,
        ..upstream::USAGE
    };
    let got = home.probe(face).expect("查得成");
    assert!(matches!(got, Probe::Unreadable(_)), "没有回显 ⇒ 读不懂");
    assert!(home.fed().is_empty(), "一行不喂：{:?}", home.fed());
    assert!(home.quota.entry(AGENT, "b").is_none(), "账不动");
}

/// ★ 回显读不懂 ⇒ 账不动、说出是哪一截。
#[cfg(unix)]
#[test]
fn an_unreadable_echo_leaves_the_ledger_alone_and_names_the_cell() {
    let home = Home::new("bad");
    let face = UsageFace {
        stdin: &["hello meter", "meter burst=lots"],
        ..upstream::USAGE
    };
    let got = home.probe(face).expect("查得成");
    let Probe::Unreadable(why) = got else {
        panic!("该读不懂");
    };
    assert!(why.contains("burst=lots"), "{why}");
    assert_eq!(home.fed(), ["hello meter", "meter burst=lots"]);
    assert!(home.quota.entry(AGENT, "b").is_none(), "账不动");
}

/// ★ 号按（家, 号名）认：两家替身各有一个 `b`（别家那个是按量号、排在前面）⇒ 查本家的 `b` 查得成、记在本家名下；
/// 查别家的 `b` 认出它是按量号；本家没有、只别家有的号名 ⇒ 找不到（不借别家那一个）。
#[cfg(unix)]
#[test]
fn the_probe_finds_the_account_by_family_and_name() {
    let home = Home::new("family");
    let (mine, other) = ("stand-in", "other");
    let reg = [
        crate::agents::fake::family_row(mine, "CCM_STAND_IN_DIR", "si", true, || None),
        crate::agents::fake::family_row(other, "CCM_OTHER_DIR", "ot", false, || None),
    ];
    std::fs::create_dir_all(home.root.join("accts").join("c")).expect("mkdir");
    let lib = acct_core::AccountBook {
        accounts: vec![
            home.row("b", Some(other), true),
            home.row("c", Some(other), false),
            home.row("b", Some(mine), false),
        ],
    };
    let got = home.probe_in(&reg, &lib, upstream::USAGE, mine, "b");
    match got {
        Ok(Probe::Read(_)) => {}
        Ok(Probe::Unreadable(why)) => panic!("本家的 b 该读得懂：{why}"),
        Err(e) => panic!("本家的 b 是订阅号、该查得成：{e:?}"),
    }
    assert!(home.quota.entry(mine, "b").is_some(), "记在本家名下");
    assert!(home.quota.entry(other, "b").is_none(), "别家的 b 账不动");

    let theirs = home.probe_in(&reg, &lib, upstream::USAGE, other, "b");
    let code = theirs.err().and_then(|e| e.code);
    assert_eq!(code.as_deref(), Some("unsupported"), "别家的 b 是按量号");

    let absent = home.probe_in(&reg, &lib, upstream::USAGE, mine, "c");
    let code = absent.err().and_then(|e| e.code);
    assert_eq!(code.as_deref(), Some("not_found"), "本家没有 c");

    // 账号 0（不设号目录）哪一家都算：清单没写是哪一家，别家查它也找得到。
    let mut lib = lib;
    lib.accounts.push(
        crate::accounts::upstream_select::rotate::test_support::book_row(
            acct_core::BARE_ACCOUNT,
            None,
            false,
        ),
    );
    let bare = home.probe_in(&reg, &lib, upstream::USAGE, other, acct_core::BARE_ACCOUNT);
    assert!(
        matches!(bare, Ok(Probe::Read(_))),
        "别家的账号 0 照它本来那一份登录查"
    );
}

/// ★ 这一家没有报用量那一形 ⇒ 那一句照传进来的注册表说那一家的叫法；认不出 ⇒ 照实那一句，不露 kind（替身家）。
#[test]
fn the_unsupported_refusal_names_that_family_not_its_kind() {
    use crate::agents::fake;
    let reg = [
        fake::family_row("stand-a", "CCM_STAND_A", "sa", true, || None),
        fake::family_row("stand-b", "CCM_STAND_B", "sb", false, || None),
    ];
    let said =
        |agent: &str| copy_core::copy_text("beQuotaProbe.agent.unsupported", &[("agent", agent)]);
    for (kind, want) in [
        ("stand-b", said(fake::said_of("stand-b"))),
        ("stand-a", said(fake::said_of("stand-a"))),
        (
            "nope",
            said(&copy_core::copy_text("rsAgents.family.unsaid", &[])),
        ),
    ] {
        let got = unsupported_among(&reg, kind);
        assert_eq!(got.code.as_deref(), Some("unsupported"), "{kind}");
        assert_eq!(got.said, want, "{kind}");
    }
}
