//! 账号域判据里引到上层的那几条（登录盯防接上帧面宿主那一份号目录读法 · 额度显示态读成帧面那一行）：
//! 账号域是独立的包（`backend-accounts`）、够不到上层 ⇒ 挂在主 crate（`cross/accounts.rs`）。其余账号域的判据住各自模块底下。

use crate::accounts::quota::nodes::{LoginWatch, Node, Nodes, Source};
use std::path::{Path, PathBuf};

// ── 登录盯防（`quota::nodes`）接上 `faces::rotation_face::account_dirs` ─────────────────────────

fn temp_home(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-nodes-up-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("mkdir");
    p
}

/// 等一件这个来源的节点（别的来源的跳过）；到期没来 ⇒ `None`。
fn wait_for(rx: &mut tokio::sync::broadcast::Receiver<Node>, want: Source) -> Option<Node> {
    let deadline = std::time::Instant::now() + guard_core::wall::CEILING;
    while std::time::Instant::now() < deadline {
        match rx.try_recv() {
            Ok(n) if n.source == want && n.sid.is_none() => return Some(n),
            Ok(_) | Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
    None
}

/// 一个号 z 的账号库：清单 ＋ z 的配置目录（带登录文件）。
fn library_home(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let home = temp_home(tag);
    let accts = home.join(relay_route_core::ACCOUNTS_DIR_REL);
    let z = home.join("acct-z");
    std::fs::create_dir_all(&accts).expect("mkdir");
    std::fs::create_dir_all(&z).expect("mkdir");
    std::fs::write(z.join(".credentials.json"), "{}").expect("creds");
    let manifest = accts.join(relay_route_core::ACCOUNTS_MANIFEST_NAME);
    write_manifest(&manifest, &accts, &z, "");
    (home, manifest, z)
}

fn write_manifest(manifest: &Path, accts: &Path, z: &Path, extra: &str) {
    std::fs::write(
        manifest,
        format!(
            r#"{{"version":1,"updatedAt":"2026-10-12T00:00:00Z{extra}","sharedStore":"/s","acctsDir":"{a}","accounts":[
               {{"name":"z","email":"","configDir":"{z}","isDefault":true,"mode":"isolated"}}]}}"#,
            a = accts.display(),
            z = z.display()
        ),
    )
    .expect("manifest");
}

/// ★ 某个号的登录文件变了（登录 · 退出 · 别处续了令牌）⇒ 一件 `Accounts`；清单变了 ⇒ 也是一件，而且照新的清单重盯。
#[test]
fn a_login_file_or_the_manifest_changing_sends_a_logins_node() {
    let (home, manifest, z) = library_home("acct");
    let tx: Nodes = tokio::sync::broadcast::channel(64).0;
    let mut rx = tx.subscribe();
    let w =
        LoginWatch::start(&home, &tx, crate::faces::rotation_face::account_dirs()).expect("watch");
    assert!(w.armed());
    std::fs::write(z.join(".credentials.json"), "{\"x\":1}").expect("creds");
    assert!(
        wait_for(&mut rx, Source::Logins).is_some(),
        "登录文件变了 ⇒ 该来一件"
    );
    write_manifest(&manifest, manifest.parent().expect("dir"), &z, "1");
    assert!(
        wait_for(&mut rx, Source::Logins).is_some(),
        "清单变了 ⇒ 该来一件"
    );
    let _ = std::fs::remove_dir_all(&home);
}

// ── 额度显示态（`quota::show`）读成帧面那一行（`faces::quota_read`）─────────────────────────────

mod show_rows {
    use crate::accounts::quota::facts::Kind;
    use crate::accounts::quota::ledger::{Observed, PerPct};
    use crate::accounts::quota::show::*;
    use crate::agents::{QuotaReading, QuotaStatus, QuotaWindow};
    use crate::common::cells::{Tone, Words};

    const NOW: u64 = 1_800_000_000;
    const LATER: u64 = NOW + 3_600;
    const EARLIER: u64 = NOW - 60;

    fn slot(w: &str) -> Option<&'static str> {
        crate::agents::claudecode::quota::slot_of(w)
    }

    fn win(name: &str, used: f64, resets_at: u64) -> QuotaWindow {
        QuotaWindow {
            name: name.into(),
            used: Some(used),
            resets_at: Some(resets_at),
            warned_at: None,
        }
    }

    fn facts(kind: Kind) -> Facts {
        Facts {
            kind,
            login: LoginState::Ok,
            sub_id: None,
        }
    }

    /// 一份账：5h 用 30%、7d 两个窗口（`seven_day` 40% · `seven_day_opus` 85%），5h 与 opus 量过「1% ≈ 多少」。
    fn observed() -> Observed {
        let r = QuotaReading {
            status: Some(QuotaStatus::Allowed),
            refused: false,
            limiting: Some("five_hour".into()),
            resets_at: Some(LATER),
            windows: vec![
                win("five_hour", 0.3, LATER),
                win("seven_day", 0.4, LATER + 86_400),
                win("seven_day_opus", 0.85, LATER + 86_400),
            ],
            overage: None,
        };
        Observed {
            agent: "claude".into(),
            account: "a".into(),
            seen_at: NOW,
            reading: r,
            windows_seen: Default::default(),
            trend: Default::default(),
            per_pct: [
                (
                    "five_hour".to_string(),
                    PerPct {
                        tokens: 2_100_000,
                        at: NOW,
                    },
                ),
                (
                    "seven_day_opus".to_string(),
                    PerPct {
                        tokens: 25_000,
                        at: NOW,
                    },
                ),
            ]
            .into(),
            history: Default::default(),
        }
    }

    /// ★ 更细的窗口（窗口键带 `:`）在 7d 与超额之间各出一行（`rows`）：名字照窗口键（`7d:opus` ⇒「7d opus」，原名不上屏）·
    /// 值带语气（快满 warn · 用满 fail）· ↻ · 距今；重置已过 ⇒ 值「—」、↻.. 已过。5h · 7d · 不算用量的窗口不另出行。
    #[test]
    fn finer_windows_each_get_a_row_between_7d_and_overage() {
        use cmd_wire::quota::Cell;
        let mut o = observed();
        o.reading.windows.push(win("overage", 1.0, LATER));
        o.reading
            .windows
            .push(win("seven_day_sonnet", 0.5, EARLIER));
        let key = |w: &str| crate::agents::claudecode::quota::key_of(w);
        let clock = crate::common::time::TextClock::new(NOW, &crate::common::time::Tz::default());
        let mut sh = show(Some((&o.reading, o.seen_at)), facts(Kind::Sub), NOW, &slot);
        sh.windows = windows_of(&o, &key, NOW);
        let a = crate::faces::quota_read::account_of(
            crate::faces::quota_read::Seen {
                agent: o.agent.clone(),
                account: o.account.clone(),
                seen_at: o.seen_at,
                reading: o.reading.clone(),
                windows_seen: Default::default(),
                show: sh,
            },
            &clock,
        );
        let cell = |t: String, tone| Cell {
            text: Words(t),
            tone,
        };
        let plain = |t: String| cell(t, Tone::Plain);
        let tier =
            |t: &str| copy_core::copy_text("acct.window.tier", &[("slot", "7d"), ("tier", t)]);
        let at = LATER + 86_400;
        let rows: Vec<&Vec<Cell>> = a
            .rows
            .iter()
            .filter(|r| r[0].text.0.starts_with("7d "))
            .collect();
        assert_eq!(
            rows,
            vec![
                &vec![
                    plain(tier("opus")),
                    cell(
                        copy_core::copy_text("acct.val.pct", &[("pct", "85")]),
                        Tone::Warn
                    ),
                    plain(copy_core::copy_text(
                        "acct.reset.at",
                        &[("at", &clock.text(at).0)]
                    )),
                    plain(clock.rel(at).unwrap().0),
                ],
                &vec![
                    plain(tier("sonnet")),
                    plain(copy_core::copy_text("acct.val.none", &[])),
                    plain(copy_core::copy_text(
                        "acct.reset.past",
                        &[("at", &clock.text(EARLIER).0)]
                    )),
                ],
            ]
        );
        let labels: Vec<&str> = a.rows.iter().map(|r| r[0].text.0.as_str()).collect();
        let i7 = labels.iter().position(|l| *l == "7d").expect("7d 那一行");
        let over = copy_core::copy_text("acct.row.over", &[]);
        assert_eq!(
            labels[i7 + 1..=i7 + 3],
            [
                tier("opus").as_str(),
                tier("sonnet").as_str(),
                over.as_str()
            ],
            "更细的窗口紧跟 7d、在超额之前，各一行：{labels:?}"
        );
        assert!(
            a.rows
                .iter()
                .flatten()
                .all(|c| !c.text.0.contains("seven_day") && !c.text.0.contains("7d:")),
            "原名或窗口键上了屏：{labels:?}"
        );
    }
}
