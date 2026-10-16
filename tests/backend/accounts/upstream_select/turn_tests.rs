//! 中转发之前那一判（第五版）：只在节点上判、节点之间一发都不叫策略（#28）；节点怎么作废缓存；429 同一发重判；停发回「用满」；
//! 推演借用速只借同一档的号、借不到不编数。临时家目录、假凭据、自己的节点通道（不跟并行的判据串）、不起中转。

use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::quota::nodes::Source;
use crate::accounts::quota::rotation::{self, RotationStore};
use crate::accounts::quota::rules::book::{self, Cause};
use crate::accounts::quota::rules::script;
use crate::accounts::upstream_select::rotate::{Go, Hop, LibAccount, Library, Turn};
use crate::agents::{QuotaReading, QuotaStatus, QuotaWindow};
use std::sync::Arc;

const UUID_A: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
const UUID_B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
const SID: &str = "s-turn";

fn now() -> u64 {
    crate::accounts::quota::now_unix()
}

/// 续令牌一律发到本机一个绑着不听的口（拨了必被拒），绝不发到那一家的真端点。
fn dead_token_endpoint() -> crate::accounts::oauth::TokenEndpoint {
    let port = crate::refusing_port::refusing_port();
    crate::accounts::oauth::TokenEndpoint {
        base: crate::relay::Base::parse(&format!("http://127.0.0.1:{port}")).expect("base"),
        rest: "/v1/oauth/token".into(),
        deadline: std::time::Duration::from_millis(2_000),
    }
}

struct Home {
    root: std::path::PathBuf,
    tx: crate::accounts::quota::nodes::Nodes,
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Home {
    /// a · b 两个订阅号（假凭据、假身份；订阅档各给 `tiers`，`None` ＝ 凭据里没有那一格）。
    fn new(tag: &str, tiers: [Option<&str>; 2]) -> Self {
        let root = std::env::temp_dir().join(format!("ccm-turn-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for ((id, uuid), tier) in [("a", UUID_A), ("b", UUID_B)].into_iter().zip(tiers) {
            let dir = root.join("accts").join(id);
            std::fs::create_dir_all(&dir).expect("mkdir");
            let far = (now() + 3600) * 1000;
            let tier = tier.map_or(String::new(), |t| format!(",\"rateLimitTier\":\"{t}\""));
            std::fs::write(
                dir.join(".credentials.json"),
                format!("{{\"claudeAiOauth\":{{\"accessToken\":\"fake-access-of-{id}\",\"refreshToken\":\"fake-refresh\",\"expiresAt\":{far}{tier}}}}}"),
            )
            .expect("creds");
            std::fs::write(
                dir.join(".claude.json"),
                format!("{{\"oauthAccount\":{{\"accountUuid\":\"{uuid}\"}}}}"),
            )
            .expect("identity");
        }
        Self {
            root,
            tx: tokio::sync::broadcast::channel(256).0,
        }
    }

    fn own(&self) -> crate::platform::fs::Own {
        crate::platform::fs::Own::issue_data_at(&self.root)
    }

    fn ledger(&self) -> Arc<Ledger> {
        Arc::new(Ledger::with(
            Some(self.root.join(ledger::FILE_NAME)),
            self.tx.clone(),
            Default::default(),
        ))
    }

    fn hop(&self) -> Hop {
        let accts = self.root.join("accts");
        Hop::new(
            Arc::new(RotationStore::with_nodes(
                Some(self.root.join(rotation::FILE_NAME)),
                self.tx.clone(),
            )),
            self.ledger(),
            Some(self.root.clone()),
            Arc::new(move |_| Library {
                enabled: true,
                accounts: ["a", "b"]
                    .iter()
                    .map(|id| LibAccount {
                        id: id.to_string(),
                        dir: Some(accts.join(id)),
                        api: false,
                    })
                    .collect(),
            }),
            Arc::new(std::collections::BTreeSet::new),
            Some(dead_token_endpoint()),
        )
    }

    /// 新会话用的那份：「照列表先后」的代码 ＋ 号列表 a · b、5 小时窗上限 `cap`（`None` ＝ 用满为止）、软 / 硬。
    fn strategy(&self, cap: Option<u8>, hard: bool) {
        let caps = match cap {
            Some(n) => serde_json::json!({"5h": [{"from": 0, "to": 1440, "v": n}]}),
            None => serde_json::json!({}),
        };
        let src = crate::accounts::quota::rules::upgrade::material(0);
        let mut head = script::parse(&src).expect("读得懂").head;
        head.settings = serde_json::json!({
            "caps": caps,
            "capKind": if hard { "hard" } else { "soft" },
            "accounts": [{"start": true}, {"id": "b"}],
        });
        head.tz = Some("UTC".into());
        rotation::strategy_file_write(
            &self.own(),
            "t",
            &script::with_head(&src, &head).expect("头"),
        )
        .expect("写");
        rotation::face_change(
            &RotationStore::at(Some(self.root.join(rotation::FILE_NAME))),
            &self.own(),
            |b| b.for_new = "t".into(),
        )
        .expect("w");
    }

    /// 额度账上记一条：`acct` 的 5 小时窗用到 `used`（0–1），一小时后重置；`sid` ＝ 哪个会话的回包带来的。
    fn seen(&self, l: &Ledger, sid: Option<&str>, acct: &str, used: f64, at: u64) {
        let r = QuotaReading {
            status: Some(QuotaStatus::Allowed),
            refused: false,
            limiting: Some("five_hour".into()),
            resets_at: Some(at + 3_600),
            windows: vec![QuotaWindow {
                name: "five_hour".into(),
                used: Some(used),
                resets_at: Some(at + 3_600),
                warned_at: None,
            }],
            overage: None,
        };
        ledger::record_seen(l, &self.own(), sid, "claude-code", acct, r, at);
    }

    fn session(&self) -> book::Session {
        RotationStore::at(Some(self.root.join(rotation::FILE_NAME)))
            .now()
            .sessions[SID]
            .clone()
    }
}

fn body() -> Vec<u8> {
    let user_id = format!(
        "{{\"device_id\":\"{}\",\"account_uuid\":\"{UUID_A}\",\"session_id\":\"{SID}\"}}",
        "d".repeat(64)
    );
    serde_json::to_vec(
        &serde_json::json!({"model": "m", "messages": [], "metadata": {"user_id": user_id}}),
    )
    .expect("json")
}

fn steer(hop: &Hop, own: &crate::platform::fs::Own, at: u64) -> Go {
    let b = body();
    let none = |_: &str| None;
    hop.steer(
        &Turn {
            agent: "claude-code",
            start: "a",
            sid: SID,
            body: &b,
            row: &none,
            now: at,
        },
        own,
    )
}

/// 这个进程里重跑过几次判。
fn runs(hop: &Hop) -> u64 {
    hop.cache.lock().unwrap_or_else(|e| e.into_inner()).runs
}

fn which(g: &Go) -> &str {
    match g {
        Go::Start => "a",
        Go::Sub { account, .. } | Go::Api { account, .. } => account,
        Go::Hold { .. } => "hold",
    }
}

/// ★ 新会话默认不轮换：没选策略 ⇒ 只用开会话的号（它用满了也不换），一次都不叫策略；会话照样记下（界面能给它选一份）。
#[test]
fn without_a_strategy_a_session_keeps_its_start_account() {
    let home = Home::new("none", [None, None]);
    let hop = home.hop();
    let own = home.own();
    let t = now();
    home.seen(&hop.quota, None, "a", 1.0, t);
    for i in 0..5 {
        assert_eq!(which(&steer(&hop, &own, t + i)), "a");
    }
    assert_eq!(runs(&hop), 0, "没选策略也叫了策略");
    assert_eq!(home.session().strategy.id, "", "会话该记下、不带策略");
}

/// ★ #28：两个节点之间 100 发，策略一次都不重跑（第一发判一次，之后照缓存），结论没变不写盘；过了下一个已知节点（a 的重置）⇒ 重跑。
#[test]
fn a_hundred_sends_between_two_nodes_run_the_strategy_once() {
    let home = Home::new("between", [None, None]);
    home.strategy(None, false);
    let hop = home.hop();
    let own = home.own();
    let t = now();
    home.seen(&hop.quota, None, "a", 0.5, t);
    for i in 0..100 {
        assert_eq!(which(&steer(&hop, &own, t + i)), "a");
    }
    assert_eq!(runs(&hop), 1, "节点之间重跑了");
    let stamp = || {
        std::fs::metadata(home.root.join(rotation::FILE_NAME))
            .and_then(|m| m.modified())
            .ok()
    };
    let before = stamp();
    for i in 100..110 {
        steer(&hop, &own, t + i);
    }
    assert_eq!(stamp(), before, "结论没变也写了盘");
    steer(&hop, &own, t + 3_601);
    assert_eq!(runs(&hop), 2, "过了 a 的重置（下一个已知节点）没重跑");
}

/// ★ 节点作废缓存：事实那几种（`quota.json` 被写）⇒ 重跑；会话那一条在别处改了（快照不同）⇒ 重跑。
#[test]
fn fact_nodes_and_a_changed_session_rerun() {
    let home = Home::new("nodes", [None, None]);
    home.strategy(None, false);
    let hop = home.hop();
    let own = home.own();
    let t = now();
    steer(&hop, &own, t);
    steer(&hop, &own, t + 1);
    assert_eq!(runs(&hop), 1);
    hop.quota.notify(Source::QuotaFile, None);
    steer(&hop, &own, t + 2);
    assert_eq!(runs(&hop), 2, "quota.json 那一件没作废缓存");
    rotation::face_change(&hop.store, &own, |b| {
        b.sessions.get_mut(SID).expect("有").temp = vec!["b".into()];
    })
    .expect("w");
    steer(&hop, &own, t + 3);
    assert_eq!(runs(&hop), 3, "会话那一条变了没重跑");
}

/// ★ 读数节点：这个会话自己的读数没越过它记着的 `wake.when`（上限 80）⇒ 不重跑；越过 ⇒ 重跑、换到 b；
/// 别的会话的读数 ⇒ 外来的事实，重跑。
#[test]
fn an_own_reading_reruns_only_past_its_wake_reading() {
    let home = Home::new("wake", [None, None]);
    home.strategy(Some(80), false);
    let hop = home.hop();
    let own = home.own();
    let t = now();
    home.seen(&hop.quota, Some(SID), "a", 0.5, t);
    assert_eq!(which(&steer(&hop, &own, t)), "a");
    home.seen(&hop.quota, Some(SID), "a", 0.7, t + 1);
    assert_eq!(which(&steer(&hop, &own, t + 2)), "a");
    assert_eq!(runs(&hop), 1, "自己的读数没越过 80 也重跑了");
    home.seen(&hop.quota, Some("s-other"), "b", 0.1, t + 3);
    steer(&hop, &own, t + 4);
    assert_eq!(runs(&hop), 2, "别的会话的读数没作废缓存");
    home.seen(&hop.quota, Some(SID), "a", 0.85, t + 5);
    assert_eq!(which(&steer(&hop, &own, t + 6)), "b", "越过 80 没换");
    assert_eq!(runs(&hop), 3);
    let s = home.session();
    assert_eq!(
        (s.current.as_str(), s.log.last().map(|l| &l.cause)),
        ("b", Some(&Cause::Order))
    );
}

/// ★ 这个会话自己的读数没越过 `wake.when`，但让在用的号事实上发不出去了（回包说已拒）⇒ 照样作废、重跑、换到 b。
#[test]
fn an_own_reading_that_bars_the_account_in_use_reruns() {
    let home = Home::new("ownbar", [None, None]);
    home.strategy(Some(80), false);
    let hop = home.hop();
    let own = home.own();
    let t = now();
    home.seen(&hop.quota, Some(SID), "a", 0.5, t);
    assert_eq!(which(&steer(&hop, &own, t)), "a");
    let r = QuotaReading {
        status: Some(QuotaStatus::Rejected),
        refused: true,
        limiting: Some("five_hour".into()),
        resets_at: Some(t + 3_600),
        windows: vec![QuotaWindow {
            name: "five_hour".into(),
            used: Some(0.5),
            resets_at: Some(t + 3_600),
            warned_at: None,
        }],
        overage: None,
    };
    ledger::record_seen(&hop.quota, &own, Some(SID), "claude-code", "a", r, t + 1);
    assert_eq!(
        which(&steer(&hop, &own, t + 2)),
        "b",
        "在用的号发不出去了还照缓存发"
    );
}

/// ★ #27：回包被拒 ⇒ 同一发里带着 `tried` 再判一次，换到 b、记「事实上发不出去」。
#[test]
fn a_refusal_rejudges_in_the_same_send() {
    let home = Home::new("refused", [None, None]);
    home.strategy(None, false);
    let hop = home.hop();
    let own = home.own();
    let t = now();
    steer(&hop, &own, t);
    let b = body();
    let none = |_: &str| None;
    let heard = QuotaReading {
        status: None,
        refused: true,
        limiting: None,
        resets_at: Some(t + 600),
        windows: Vec::new(),
        overage: None,
    };
    let go = hop
        .on_refused(
            &Turn {
                agent: "claude-code",
                start: "a",
                sid: SID,
                body: &b,
                row: &none,
                now: t + 1,
            },
            "a",
            &heard,
            &["a".to_string()],
            &own,
        )
        .expect("换得了");
    assert_eq!(which(&go), "b");
    let s = home.session();
    assert_eq!(
        (s.current.as_str(), &s.log[0].cause),
        ("b", &Cause::Unusable)
    );
}

/// ★ 硬上限两个号都到了 ⇒ 停发：回那一家的「用满」回包；记一条停发、同一个 `until` 不重记。
#[test]
fn hard_caps_on_every_account_hold_the_send() {
    let home = Home::new("hold", [None, None]);
    home.strategy(Some(50), true);
    let hop = home.hop();
    let own = home.own();
    let t = now();
    home.seen(&hop.quota, None, "a", 0.6, t);
    home.seen(&hop.quota, None, "b", 0.7, t);
    let g = steer(&hop, &own, t + 1);
    let Go::Hold { reply } = &g else {
        panic!("该停发：{}", which(&g));
    };
    assert!(reply.status.starts_with("429"), "{}", reply.status);
    hop.quota.notify(Source::QuotaFile, None);
    steer(&hop, &own, t + 2);
    let holds = home
        .session()
        .log
        .iter()
        .filter(|l| matches!(l.cause, Cause::Hold { .. }))
        .count();
    assert_eq!(holds, 1, "同一个 until 重记了");
}

/// ★ 同一刻同一换法只记一条：两发同一秒、都照换号前那一份会话判出「a → b」（两条请求并发、各自拿着旧的那一份），
/// 落账是在锁里对盘上那一条重做一遍 ⇒ 第二发看到已经在 b 上，不再记；换号记录里那一刻只有一条 a → b。
#[test]
fn two_sends_in_the_same_second_judging_the_same_switch_log_it_once() {
    let home = Home::new("same-second", [None, None]);
    home.strategy(Some(50), false);
    let hop = home.hop();
    let own = home.own();
    let t = now();
    home.seen(&hop.quota, None, "a", 0.6, t);
    let b = body();
    let none = |_: &str| None;
    let turn = Turn {
        agent: "claude-code",
        start: "a",
        sid: SID,
        body: &b,
        row: &none,
        now: t + 1,
    };
    let stale = hop.entry(&own, &turn).expect("会话记下了");
    assert_eq!(stale.current, "a");
    let loaded = hop.load(&stale.strategy);
    let facts = hop.accts(&stale, &loaded.settings, &none, t + 1);
    let d = hop.judge_session(&stale, &loaded, &facts, t + 1, &[]);
    assert_eq!(
        d.pick,
        crate::accounts::quota::rules::decide::Pick::Use("b".into()),
        "该判出换到 b"
    );
    for _ in 0..2 {
        hop.settle(&own, SID, &stale, &d, &facts, t + 1);
    }
    let log = home.session().log;
    assert_eq!(
        log.iter()
            .map(|l| (l.at, l.from.as_str(), l.to.as_str()))
            .collect::<Vec<_>>(),
        vec![(t + 1, "a", "b")],
        "同一刻同一换法记了不止一条：{log:?}"
    );
}

/// ★ 换到的号接不上（令牌过期、续期被拒）⇒ 那一号没换成、不记：记录里不出「a → b」再接一条（同一秒两条，其中一条是没发生的）。
/// 这份策略里 a 过了上限、b 接不上 ⇒ 这一发停下；记录里只有那一条停发、在用的号仍是 a。
#[test]
fn a_switch_whose_account_cannot_be_reached_is_not_logged() {
    let home = Home::new("unreached", [None, None]);
    home.strategy(Some(50), false);
    let far_past = (now() - 3_600) * 1000;
    std::fs::write(
        home.root.join("accts/b/.credentials.json"),
        format!("{{\"claudeAiOauth\":{{\"accessToken\":\"fake-access-of-b\",\"refreshToken\":\"fake-refresh\",\"expiresAt\":{far_past}}}}}"),
    )
    .expect("creds");
    let hop = home.hop();
    let own = home.own();
    let t = now();
    home.seen(&hop.quota, None, "a", 0.6, t);
    assert_ne!(which(&steer(&hop, &own, t + 1)), "b", "b 接不上还发给了它");
    let s = home.session();
    let log: Vec<(&str, &str)> = s
        .log
        .iter()
        .map(|l| (l.from.as_str(), l.to.as_str()))
        .collect();
    assert_eq!(
        (s.current.as_str(), log),
        ("a", vec![("a", "a")]),
        "没换成的那一号记进了换号记录：{:?}",
        s.log
    );
}

// ── 推演的用速：只量、不借、量不到不编数 ───────────────────────────────────────────────

fn acct(id: &str, used: f64, rate: Option<f64>, t: u64) -> crate::accounts::quota::rules::Acct {
    crate::accounts::quota::rules::Acct {
        id: id.into(),
        api: false,
        used: [used, 0.0],
        resets: [Some(t + 5 * 3600), None],
        refused_until: None,
        barred: None,
        rate: [rate, None],
        per_pct: [None, None],
    }
}

fn fixture_by_list() -> script::File {
    script::parse(crate::accounts::quota::rules::upgrade::material(0)).expect("读得懂")
}

/// ★ 推演：在用的号额度账里量不到用速 ⇒ 推演里不出现估出来的换号时刻（那一段标「用到上限的时刻还不知道」），也不拿别的号的数顶；
/// 量得到 ⇒ 照它换算（20 → 80 照 0.5 %/分是 120 分钟）。
#[test]
fn the_simulation_never_makes_up_a_rate() {
    use crate::accounts::quota::rules::{decide::Sigma, simulate};
    let t = now() - now() % 60;
    let file = fixture_by_list();
    let settings = serde_json::json!({
        "caps": {"5h": [{"from": 0, "to": 1440, "v": 80}]},
        "capKind": "soft",
        "accounts": [{"id": "b"}, {"id": "a"}],
    });
    let sigma = Sigma {
        start: "a".into(),
        current: "b".into(),
        ..Sigma::default()
    };
    let run = |rate: Option<f64>| {
        simulate::simulate(
            &sigma,
            &[
                crate::accounts::quota::rules::Acct {
                    // 7 天窗量得到、是 0（不涨）：只看 5 小时窗那一格。
                    rate: [rate, rate.map(|_| 0.0)],
                    ..acct("b", 20.0, rate, t)
                },
                // a 量得到用速、但窗口没开（没人在用）：不涨，也不该被拿去顶 b 的。
                crate::accounts::quota::rules::Acct {
                    resets: [None, None],
                    ..acct("a", 0.0, Some(0.9), t)
                },
            ],
            &settings,
            &file,
            &|_| 0,
            t,
            t + 4 * 3600,
        )
    };
    let blind = run(None);
    assert!(
        blind.switches().is_empty(),
        "编了一个换号时刻：{:?}",
        blind.switches()
    );
    assert!(blind.uses.iter().any(|u| u.cap_unknown), "没标「还不知道」");
    let measured = run(Some(0.5));
    assert_eq!(measured.switches(), [(t + 120 * 60, "a".to_string())]);
    assert!(!measured.uses[0].cap_unknown);
}

/// ★ 别的会话在用的号：它在额度账里量得到的用速照样进推演（照它涨、到 100 那一刻它发不出去）。
#[test]
fn an_account_used_by_another_session_grows_at_its_own_rate() {
    use crate::accounts::quota::rules::{decide::Sigma, simulate};
    let t = now() - now() % 60;
    let settings =
        serde_json::json!({"caps": {}, "capKind": "soft", "accounts": [{"id": "a"}, {"id": "b"}]});
    let sigma = Sigma {
        start: "a".into(),
        current: "a".into(),
        ..Sigma::default()
    };
    let tl = simulate::simulate(
        &sigma,
        &[acct("a", 10.0, None, t), acct("b", 70.0, Some(0.5), t)],
        &settings,
        &fixture_by_list(),
        &|_| 0,
        t,
        t + 4 * 3600,
    );
    let b = &tl
        .lanes
        .iter()
        .find(|(id, _)| id == "b")
        .expect("b 那一行")
        .1;
    assert!(
        b.iter().any(|s| s.from == t + 3600 && !s.sendable),
        "b 照它的用速一小时后用满：{b:?}"
    );
}
