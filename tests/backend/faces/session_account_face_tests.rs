//! `session-account`：一条会话的号那一份成品。号照「中转记的在用号 ＞ 进程环境判出的号 ＞ 上次的号 ＞ 不知道」取；
//! 用量 · 悬停卡 · 与默认号比 · 徽章都在这里写好。临时家目录、假凭据、不起中转、不读真家。

use super::*;
use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::quota::rotation::{self, RotationStore};
use crate::accounts::upstream_select::rotate::Hop;
use crate::agents::{QuotaReading, QuotaStatus, QuotaWindow};
use std::sync::Arc;

const NOW: u64 = 4_000_000_000;
/// 判据里那条会话那一家（替身家；号名照它问）。
const AGENT: &str = crate::agents::fake::AGENT_KIND;

/// 号名表里 `AGENT` 那一家的叫法（同 `product` 那一处）。
fn label(account: &str) -> String {
    crate::accounts::quota::name_words::account_text_of(AGENT, account)
}

fn home(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("ccm-sessacct-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for id in ["work", "personal"] {
        let d = root.join("accts").join(id);
        std::fs::create_dir_all(&d).expect("mkdir");
        std::fs::write(
            d.join(".credentials.json"),
            "{\"claudeAiOauth\":{\"accessToken\":\"fake\",\"refreshToken\":\"fake-r\",\"expiresAt\":9999999999999}}",
        )
        .expect("creds");
    }
    root
}

/// 账号库：work · personal（都登录了）；额度账上 work 5 小时用了 63%，personal 没出过数。
fn quota(tag: &str) -> QuotaRead {
    let root = home(tag);
    let accts = root.join("accts");
    let ctx = crate::faces::rotation_face::Ctx {
        setup: Arc::new(crate::accounts::quota::setup::uses::Setup::of(Arc::new(
            RotationStore::at(Some(root.join(rotation::FILE_NAME))),
        ))),
        hop: Hop::new(
            Arc::new(RotationStore::at(Some(root.join(rotation::FILE_NAME)))),
            &crate::accounts::quota::adoptions_at(),
            Arc::new(Ledger::at(Some(root.join(ledger::FILE_NAME)))),
            Some(root.to_path_buf()),
            Arc::new(move |_| {
                Some(acct_core::AccountBook {
                    accounts: ["work", "personal"]
                        .iter()
                        .map(|id| {
                            crate::accounts::upstream_select::rotate::test_support::book_row(
                                id,
                                Some(accts.join(id)),
                                false,
                            )
                        })
                        .collect(),
                })
            }),
            crate::accounts::upstream_select::rotate::HistoryPorts {
                live: Arc::new(Default::default),
                ring: Arc::new(|| {}),
            },
            None,
        ),
        rows: Box::new(|_, _| None),
    };
    let own = crate::platform::fs::Own::issue_data_at(&root);
    ledger::record_seen(
        &ctx.hop.quota,
        &own,
        None,
        "claude-code",
        "work",
        QuotaReading {
            status: Some(QuotaStatus::Allowed),
            refused: false,
            limiting: Some("five_hour".into()),
            resets_at: Some(NOW + 3_600),
            windows: vec![QuotaWindow {
                name: "five_hour".into(),
                used: Some(0.63),
                resets_at: Some(NOW + 3_600),
                warned_at: None,
            }],
            overage: None,
        },
        NOW,
    );
    crate::faces::rotation_face::quota_read_with(&ctx, NOW, &crate::common::time::Tz::default())
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn texts(rows: &cmd_wire::quota::Rows) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| r.iter().map(|c| c.text.0.clone()).collect())
        .collect()
}

/// ★ 走中转、轮换从 personal 换到 work：在用的是 work（此刻），从 personal 换来；用量照额度账 work 那一条；
/// 悬停卡是 work 那一段 ＋ 末行「从 personal 换过来」；默认号是 personal ⇒ 不一致、出徽章（不是幽灵）。
#[test]
fn relayed_swapped_session_answers_current_account_with_usage_and_badge() {
    let q = quota("swapped");
    let f = Facts {
        agent: AGENT.into(),
        rot: Some(("personal".into(), "work".into())),
        alive: true,
        live: s("personal"),
        last: s("personal"),
        default: s("personal"),
        queryable: true,
        own_upstream: false,
    };
    let got = product(&f, &q);
    assert_eq!(got.account.as_deref(), Some("work"));
    assert_eq!(got.name, label("work"));
    assert_eq!(got.when, SessionAccountWhen::Now);
    assert_eq!(got.from.as_deref(), Some("personal"));
    let led = q
        .accounts
        .iter()
        .find(|a| a.head.account == "work")
        .expect("work 在账上");
    assert_eq!(got.usage.as_ref(), Some(&led.head.show.usage));
    assert_eq!(got.stale, led.head.show.stale);
    let mut want = texts(&led.rows);
    want.push(vec![copy_text(
        "rot.hover.from",
        &[("account", &label("personal"))],
    )]);
    assert_eq!(texts(&got.hover), want);
    assert_eq!(
        got.hover[..led.rows.len()],
        led.rows[..],
        "额度账那一段的格连语气原样带过去（悬停卡照语气上色）"
    );
    assert_eq!(
        got.sampled,
        Some(led.sampled.0.clone()),
        "几点采的照额度账那一条写好的那一句（不成行）"
    );
    assert!(got.sampled.is_some());
    assert!(got.differs);
    assert_eq!(
        got.badge,
        Some(SessionAccountBadge {
            ghost: false,
            title: copy_text("tabBar.hover.account", &[("name", "work")]),
        })
    );
    assert_eq!(
        got.account_line,
        Some(copy_text("tabBar.hover.account", &[("name", "work")]))
    );
    assert_eq!(
        got.from_line,
        Some(copy_text(
            "rot.hover.from",
            &[("account", &label("personal"))]
        ))
    );
}

/// ★ 走中转、在用号额度账上没出过数（personal）：用量空着、悬停照没出过数那一段、不接采样；与默认号相同 ⇒ 不出徽章。
#[test]
fn relayed_unseen_account_has_no_usage_and_matches_default() {
    let q = quota("unseen");
    let f = Facts {
        agent: AGENT.into(),
        rot: Some(("personal".into(), "personal".into())),
        alive: true,
        default: s("personal"),
        queryable: true,
        ..Facts::default()
    };
    let got = product(&f, &q);
    assert_eq!(got.account.as_deref(), Some("personal"));
    assert_eq!(got.from, None);
    assert_eq!(got.from_line, None);
    assert_eq!(got.usage, None);
    let un = q
        .unseen
        .iter()
        .find(|u| u.head.account == "personal")
        .expect("personal 没出过数");
    assert_eq!(texts(&got.hover), texts(&un.rows));
    assert_eq!(got.sampled, None);
    assert!(!got.differs);
    assert_eq!(got.badge, None);
}

/// ★ 没走中转、活着：号照进程环境判的那一个（此刻），不画用量；悬停三行（名 · 类型 / 用量 — 未实时显示 / 切换 仅重启切换）。
#[test]
fn unrelayed_live_session_uses_process_account_without_usage() {
    let q = quota("live");
    let f = Facts {
        agent: AGENT.into(),
        alive: true,
        live: s("work"),
        last: s("personal"),
        default: s("work"),
        queryable: true,
        ..Facts::default()
    };
    let got = product(&f, &q);
    assert_eq!(got.account.as_deref(), Some("work"));
    assert_eq!(got.when, SessionAccountWhen::Now);
    assert_eq!(got.usage, None);
    let led = q
        .accounts
        .iter()
        .find(|a| a.head.account == "work")
        .expect("work");
    let mut head = vec![label("work")];
    head.extend(led.rows[0].get(1).map(|c| c.text.0.clone()));
    assert_eq!(
        texts(&got.hover),
        vec![
            head,
            vec![
                copy_text("acct.hover.usage", &[]),
                copy_text("acct.val.none", &[]),
                copy_text("acct.hover.noRelay", &[]),
            ],
            vec![
                copy_text("acct.hover.switch", &[]),
                copy_text("acct.hover.restartOnly", &[])
            ],
        ]
    );
    assert!(!got.differs);
    assert_eq!(got.badge, None);
}

/// ★ 结束了、只记着上次的号：画成「上次」；与默认号不同 ⇒ 徽章是幽灵的。
#[test]
fn ended_session_falls_back_to_last_account_as_ghost() {
    let q = quota("last");
    let f = Facts {
        agent: AGENT.into(),
        last: s("personal"),
        default: s("work"),
        queryable: true,
        ..Facts::default()
    };
    let got = product(&f, &q);
    assert_eq!(got.account.as_deref(), Some("personal"));
    assert_eq!(got.when, SessionAccountWhen::Last);
    assert!(got.differs);
    assert_eq!(got.badge.map(|b| b.ghost), Some(true));
}

/// ★ 一样都不知道：号空着、名字是「未实时显示」、悬停两行；默认号再有也不比（说不出「不同」）。
#[test]
fn nothing_known_says_so_and_never_differs() {
    let q = quota("none");
    let f = Facts {
        agent: AGENT.into(),
        default: s("work"),
        queryable: true,
        ..Facts::default()
    };
    let got = product(&f, &q);
    assert_eq!(got.account, None);
    assert_eq!(got.account_line, None);
    assert_eq!(got.when, SessionAccountWhen::Unknown);
    assert_eq!(got.name, copy_text("acct.hover.noRelay", &[]));
    assert_eq!(
        texts(&got.hover),
        vec![
            vec![copy_text("acct.hover.noRelay", &[])],
            vec![
                copy_text("acct.hover.switch", &[]),
                copy_text("acct.hover.restartOnly", &[])
            ],
        ]
    );
    assert!(!got.differs);
    assert_eq!(got.badge, None);
}

/// ★ 默认号不知道 ⇒ 不比；那台的号查不了 ⇒ 不一致也不出徽章。
#[test]
fn unknown_default_or_unqueryable_host_draws_no_badge() {
    let q = quota("gate");
    let no_default = Facts {
        agent: AGENT.into(),
        alive: true,
        live: s("work"),
        queryable: true,
        ..Facts::default()
    };
    let got = product(&no_default, &q);
    assert!(!got.differs);
    assert_eq!(got.badge, None);
    let closed = Facts {
        agent: AGENT.into(),
        alive: true,
        live: s("work"),
        default: s("personal"),
        queryable: false,
        ..Facts::default()
    };
    let got = product(&closed, &q);
    assert!(got.differs);
    assert_eq!(got.badge, None, "号查不了 ⇒ 不出徽章");
}

/// ★ 上游是自设的（用户自己的网关）：那一格写上；不画那个号的用量（额度账上有也不画）；悬停写看不到 · 不适用；
/// 不与默认号比、不出徽章；标签页悬停卡那一行照它写。
#[test]
fn own_upstream_session_says_quota_unseen_and_switching_not_applicable() {
    let q = quota("own");
    let f = Facts {
        agent: AGENT.into(),
        alive: true,
        live: s("work"),
        default: s("personal"),
        queryable: true,
        own_upstream: true,
        ..Facts::default()
    };
    let got = product(&f, &q);
    let line = copy_text("acct.own.line", &[]);
    assert_eq!(got.own_upstream.as_deref(), Some(line.as_str()));
    assert_eq!(got.usage, None, "画了那个号的用量");
    assert_eq!(
        texts(&got.hover),
        vec![
            vec![label("work")],
            vec![
                copy_text("acct.hover.usage", &[]),
                copy_text("acct.own.usage", &[]),
            ],
            vec![
                copy_text("acct.hover.switch", &[]),
                copy_text("acct.own.switch", &[]),
            ],
        ]
    );
    assert_eq!(got.sampled, None);
    assert!(!got.differs);
    assert_eq!(got.badge, None);
    assert_eq!(got.account_line.as_deref(), Some(line.as_str()));
    // 不是自设的 ⇒ 那一格不在。
    let plain = product(
        &Facts {
            own_upstream: false,
            ..f
        },
        &q,
    );
    assert_eq!(plain.own_upstream, None);
}

/// 拿来比的默认号只认这条会话那一家的：默认号是另一家的号 ⇒ 当没有（不拿别家的号比、不画徽章）；同家、选得了照取；
/// 同家却选不了 ⇒ 没有。两家都是替身家（alpha · beta）。
#[test]
fn the_default_compared_is_only_one_of_the_sessions_own_family() {
    use crate::accounts::manage::library::test_support::{list, row_with};
    let of = |name: &str, kind: &str, default: bool| {
        row_with(name, |a| {
            a.agent = Some(cmd_wire::AgentKind::new(kind));
            a.is_default = default;
        })
    };
    let alpha = cmd_wire::AgentKind::new("alpha");
    let beta = cmd_wire::AgentKind::new("beta");
    let mixed = list(vec![of("a1", "alpha", false), of("work", "beta", true)]);
    assert_eq!(default_of(&mixed, &alpha), None, "别家的默认号不取");
    assert_eq!(default_of(&mixed, &beta), s("work"), "同家照取");
    let unusable = list(vec![row_with("work", |a| {
        a.agent = Some(beta.clone());
        a.is_default = true;
        a.exists = false;
    })]);
    assert_eq!(default_of(&unusable, &beta), None, "选不了的不算");
}

/// 入参多一格 ⇒ 收不下（`deny_unknown_fields`）。
#[test]
fn unknown_request_field_is_refused() {
    let bad = serde_json::from_value::<SessionAccountReq>(serde_json::json!({"sid": "s1", "x": 1}));
    assert!(bad.is_err());
    let ok = serde_json::from_value::<SessionAccountReq>(serde_json::json!({"sid": "s1"}))
        .expect("收得下");
    assert_eq!(ok.sid, "s1");
}

/// 号名照这条会话那一家问：账号 0 叫那一家默认家目录的短形：默认那一家有本机布局、写得出短形，替身家不在注册表里、说不出 ⇒ 空串；两家叫法不同，传错家就对不上。
#[test]
fn the_account_name_follows_the_sessions_family() {
    let q = quota("name-by-family");
    let name_of = |agent: &str| {
        product(
            &Facts {
                agent: agent.into(),
                last: s("0"),
                ..Facts::default()
            },
            &q,
        )
        .name
    };
    let default = crate::agents::default_kind();
    use crate::accounts::quota::name_words::account_text_of;
    assert_ne!(
        account_text_of(default, "0"),
        account_text_of(AGENT, "0"),
        "两家账号 0 的叫法一样 ⇒ 下面两条分不出是不是照家问的"
    );
    assert_eq!(name_of(default), account_text_of(default, "0"));
    assert_eq!(name_of(AGENT), account_text_of(AGENT, "0"));
}

/// ★ 会话归哪一家照按 sid 找到它的那一家认：会话落在 `other` 的记录根下、照路径认不出 ⇒ 仍是 `other`（号清单 · 号名照它问），
/// 不退到默认那一家；这台找不到它的记录（别家刚起、还没写）⇒ 照活会话表里写它那份 pidfile 的那一家；活会话表也认不出 ⇒
/// 不知道是哪一家，不落到默认那一家。
#[test]
fn the_account_family_follows_the_family_the_session_was_found_in() {
    let (reg, at) = crate::agents::fake::found_in_the_other("account", |f| f);
    assert_eq!(agent_among(&reg, Some(&at), None), (Some("other"), "other"));
    assert_eq!(
        agent_among(&reg, None, Some("other")),
        (Some("other"), "other"),
        "没写记录 ⇒ 照活会话表那一家"
    );
    assert_eq!(
        agent_among(&reg, None, None),
        (None, ""),
        "认不出 ⇒ 不退到默认那一家"
    );
}

/// 活没活照脸上那一判：启动时刻写成另一种记法（Windows 上 pidfile 的记法与这台读的恒不等 ⇒ 身份核不上）、进程像那一家
/// ⇒ 活会话表说活着 ⇒ 归号说「现在」那一侧；身份没核上 ⇒ 不读它的环境、号说不出。
#[cfg(target_os = "linux")]
#[test]
fn a_session_whose_start_time_cannot_be_checked_is_still_live_with_no_process_account() {
    use std::os::unix::process::CommandExt;
    let home = test_home::scratch("sessacct-live");
    let sessions = crate::agents::IntoHomes::into_homes(&home)[0]
        .pidfile_dir()
        .unwrap();
    std::fs::create_dir_all(&sessions).unwrap();
    let mut kid = crate::platform::proc::test_support::spawn_settled_sleep(|c| {
        c.arg0("claude");
    });
    std::fs::write(
        sessions.join(format!("{}.json", kid.id())),
        r#"{"sessionId":"s-live","cwd":"/w","procStart":"638000000000000000"}"#,
    )
    .unwrap();
    let got = standing_at(&home, "s-live");
    let other = standing_at(&home, "s-other");
    let _ = kid.kill();
    let _ = kid.wait();
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(got, (true, None), "脸上活着、身份没核上 ⇒ 活着、号说不出");
    assert_eq!(other, (false, None));
}
