//! `session-account`：一条会话的号那一份成品。号照「中转记的在用号 ＞ 进程环境判出的号 ＞ 上次的号 ＞ 不知道」取；
//! 用量 · 悬停卡 · 与默认号比 · 徽章都在这里写好。临时家目录、假凭据、不起中转、不读真家。

use super::*;
use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::quota::rotation::{self, RotationStore};
use crate::accounts::upstream_select::rotate::Hop;
use crate::agents::{QuotaReading, QuotaStatus, QuotaWindow};
use std::sync::Arc;

const NOW: u64 = 4_000_000_000;

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
    assert_eq!(got.hover, want);
    assert_eq!(
        got.sampled,
        Some(led.rows.len() - 1),
        "额度账那一段的末行接采样那一台（换来那一行在它后面）"
    );
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
    assert_eq!(got.hover, texts(&un.rows));
    assert_eq!(got.sampled, None);
    assert!(!got.differs);
    assert_eq!(got.badge, None);
}

/// ★ 没走中转、活着：号照进程环境判的那一个（此刻），不画用量；悬停三行（名 · 类型 / 用量 — 未实时显示 / 切换 仅重启切换）。
#[test]
fn unrelayed_live_session_uses_process_account_without_usage() {
    let q = quota("live");
    let f = Facts {
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
        got.hover,
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
        got.hover,
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
        alive: true,
        live: s("work"),
        queryable: true,
        ..Facts::default()
    };
    let got = product(&no_default, &q);
    assert!(!got.differs);
    assert_eq!(got.badge, None);
    let closed = Facts {
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
        got.hover,
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

/// 入参多一格 ⇒ 收不下（`deny_unknown_fields`）。
#[test]
fn unknown_request_field_is_refused() {
    let bad = serde_json::from_value::<SessionAccountReq>(serde_json::json!({"sid": "s1", "x": 1}));
    assert!(bad.is_err());
    let ok = serde_json::from_value::<SessionAccountReq>(serde_json::json!({"sid": "s1"}))
        .expect("收得下");
    assert_eq!(ok.sid, "s1");
}
