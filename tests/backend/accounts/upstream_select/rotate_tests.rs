//! 备料（换到别的号时这一发怎么走）：照模块头逐条钉 —— 起会话的号照原样（[`Go::Start`]）；订阅号换上那个号的访问令牌、请求体里账号身份那一格
//! 一起换；按量号照它在 key 表里那一行发、身份格换成空串；拿不到登录 / key / 认不准身份格只回原因码。
//! 还有：认出 `_` 那一发在用哪个号（[`Hop::recognize`]）。
//! 临时家目录、假凭据、假身份、续令牌发到本机一个绑着不听的口；不起中转。

use super::test_support::book_row;
use super::*;
use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::quota::rotation;
use crate::relay::Mode;
use creds_core::store::AuthStyle;

const UUID_A: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
const UUID_B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
const AGENT: &str = "claude-code";

fn now() -> u64 {
    crate::common::time::now_secs()
}

/// 续令牌一律发到本机一个绑着不听的口（拨了必被拒），绝不发到那一家的真端点。
fn dead_token_endpoint() -> TokenEndpoint {
    let port = crate::refusing_port::refusing_port();
    TokenEndpoint {
        base: crate::relay::base_of(&format!("http://127.0.0.1:{port}")).expect("base"),
        rest: "/v1/oauth/token".into(),
        deadline: std::time::Duration::from_millis(2_000),
    }
}

struct Home {
    root: PathBuf,
    tx: crate::accounts::quota::nodes::Nodes,
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Home {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ccm-rotate-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("accts")).expect("mkdir");
        Self {
            root,
            tx: tokio::sync::broadcast::channel(256).0,
        }
    }

    fn dir(&self, id: &str) -> PathBuf {
        self.root.join("accts").join(id)
    }

    /// 一个订阅号：假凭据（`expires_in` 秒后过期）· 假身份（`uuid`，`None` ＝ 没有身份文件）。
    fn sub(&self, id: &str, expires_in: i64, uuid: Option<&str>) {
        let dir = self.dir(id);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let at = (i64::try_from(now()).expect("now") + expires_in) * 1000;
        std::fs::write(
            dir.join(".credentials.json"),
            format!("{{\"claudeAiOauth\":{{\"accessToken\":\"fake-access-of-{id}\",\"refreshToken\":\"fake-refresh\",\"expiresAt\":{at}}}}}"),
        )
        .expect("creds");
        if let Some(u) = uuid {
            std::fs::write(
                dir.join(".claude.json"),
                format!("{{\"oauthAccount\":{{\"accountUuid\":\"{u}\"}}}}"),
            )
            .expect("identity");
        }
    }

    fn own(&self) -> Own {
        Own::issue_data_at(&self.root)
    }

    /// 账号库：`subs` 是订阅号、`apis` 是按量号（都在 `accts/<id>`）。
    fn hop(&self, subs: &[&str], apis: &[&str]) -> Hop {
        let accts = self.root.join("accts");
        let row = |id: &str, api: bool| acct_core::ManifestAccount {
            name: id.to_string(),
            config_dir: Some(accts.join(id).to_string_lossy().into_owned()),
            auth_kind: api.then(|| acct_core::AUTH_KIND_API_KEY.to_string()),
            ..Default::default()
        };
        self.hop_of(acct_core::AccountBook {
            accounts: subs
                .iter()
                .map(|id| row(id, false))
                .chain(apis.iter().map(|id| row(id, true)))
                .collect(),
        })
    }

    /// 照这一张号表。
    fn hop_of(&self, book: acct_core::AccountBook) -> Hop {
        Hop::new(
            Arc::new(RotationStore::with_nodes(
                Some(self.root.join(rotation::FILE_NAME)),
                self.tx.clone(),
            )),
            &crate::accounts::quota::adoptions_at(),
            Arc::new(Ledger::open(
                Some(self.root.join(ledger::FILE_NAME)),
                self.tx.clone(),
                Default::default(),
            )),
            Some(self.root.clone()),
            Arc::new(move |_| Some(book.clone())),
            crate::accounts::upstream_select::rotate::HistoryPorts {
                live: Arc::new(std::collections::BTreeSet::new),
                ring: Arc::new(|| {}),
            },
            Some(dead_token_endpoint()),
        )
    }
}

/// 一份带账号身份那一格的请求体（`metadata.user_id` 这段 JSON 串里的 `account_uuid`）；`echo` ⇒ 消息里再原样带一份那段串（那几个字节不止一处）。
fn body_of(uuid: &str, echo: bool) -> Vec<u8> {
    let user_id = format!(
        "{{\"device_id\":\"{}\",\"account_uuid\":\"{uuid}\",\"session_id\":\"s\"}}",
        "d".repeat(64)
    );
    let messages = if echo {
        serde_json::json!([{"role": "user", "content": user_id}])
    } else {
        serde_json::json!([])
    };
    serde_json::to_vec(
        &serde_json::json!({"model": "m", "messages": messages, "metadata": {"user_id": user_id}}),
    )
    .expect("json")
}

fn identity_in(body: &[u8]) -> Option<String> {
    crate::agents::route_named(AGENT)
        .and_then(crate::agents::login_of)
        .and_then(|f| (f.identity_of_body)(body))
}

fn prep(
    hop: &Hop,
    own: &Own,
    body: &[u8],
    row: &dyn Fn(&str) -> Option<bool>,
    to: &str,
) -> Result<Go, Unready> {
    let lib = hop.book();
    hop.prepare(
        own,
        &Turn {
            agent: AGENT,
            start: "a",
            sid: "s",
            body,
            row,
            now: now(),
        },
        &lib,
        to,
    )
}

fn no_row(_: &str) -> Option<bool> {
    None
}

/// ★ 起会话的号照原样：换回 `a` ⇒ [`Go::Start`]（那张决策表照走），不看它有没有登录、请求体一个字节不动。
#[test]
fn the_start_account_goes_as_it_came() {
    let home = Home::new("start");
    let hop = home.hop(&["a", "b"], &[]);
    let go = prep(&hop, &home.own(), &body_of(UUID_A, false), &no_row, "a");
    assert!(matches!(go, Ok(Go::Start)), "起会话的号该照原样走");
}

/// ★ 订阅号：鉴权换成那个号的访问令牌、请求体里账号身份那一格换成那个号的身份（别的字节不动）；请求体里没有那一格 ⇒ 原样发（`body: None`）。
#[test]
fn a_subscription_account_brings_its_token_and_identity() {
    let home = Home::new("sub");
    home.sub("b", 3_600, Some(UUID_B));
    let hop = home.hop(&["a", "b"], &[]);
    let own = home.own();
    let sent = body_of(UUID_A, false);
    let Ok(Go::Sub {
        account,
        heads,
        body,
    }) = prep(&hop, &own, &sent, &no_row, "b")
    else {
        panic!("b 该备成订阅号那一发");
    };
    assert_eq!(account, "b");
    match heads.as_slice() {
        [(
            "Authorization",
            HeadValue::Token {
                prefix: "Bearer ",
                token,
            },
        )] => assert!(
            token.same_secret(&SecretKey::new("fake-access-of-b")),
            "令牌不是 b 那一份"
        ),
        _ => panic!("Claude 那一家换号只写 Authorization 一个头"),
    }
    let body = body.expect("请求体里有身份格，该换");
    assert_eq!(identity_in(&body).as_deref(), Some(UUID_B));
    assert_eq!(
        String::from_utf8(body)
            .expect("utf8")
            .replace(UUID_B, UUID_A)
            .into_bytes(),
        sent,
        "除了身份格别的字节动了"
    );

    let bare =
        serde_json::to_vec(&serde_json::json!({"model": "m", "messages": []})).expect("json");
    let Ok(Go::Sub { body, .. }) = prep(&hop, &own, &bare, &no_row, "b") else {
        panic!("没有身份格也该备成订阅号那一发");
    };
    assert!(body.is_none(), "没有身份格该原样发");
}

/// ★ 按量号照它在 key 表里那一行：行接得上 ⇒ [`Go::Api`]、身份格换成空串；没有行 / 行接不上 ⇒ 需 key（账号库说它是按量号、或 key 表里有它那一行，都算按量号）。
#[test]
fn an_api_account_goes_by_its_key_row_with_an_empty_identity() {
    let home = Home::new("api");
    let hop = home.hop(&["a"], &["k"]);
    let own = home.own();
    let ok = |id: &str| (id == "k" || id == "t").then_some(true);
    let Ok(Go::Api { account, body }) = prep(&hop, &own, &body_of(UUID_A, false), &ok, "k") else {
        panic!("k 该备成按量号那一发");
    };
    assert_eq!(account, "k");
    let body = body.expect("请求体里有身份格，该换");
    assert!(
        String::from_utf8_lossy(&body).contains(r#"\"account_uuid\":\"\""#),
        "身份格该换成空串"
    );
    // key 表里有行、账号库里不是按量号 ⇒ 也照那一行。
    assert!(matches!(
        prep(&hop, &own, &body_of(UUID_A, false), &ok, "t"),
        Ok(Go::Api { .. })
    ));
    let dead = |_: &str| Some(false);
    assert_eq!(
        prep(&hop, &own, &body_of(UUID_A, false), &dead, "k").err(),
        Some(Unready::NeedsKey)
    );
    assert_eq!(
        prep(&hop, &own, &body_of(UUID_A, false), &no_row, "k").err(),
        Some(Unready::NeedsKey)
    );
}

/// ★ 接不上只回原因码：订阅号没有凭据 / 没有身份 / 续期失败 ⇒ 需登录（续期失败还发一个「登录」节点）；不在账号库里 ⇒ 需登录；
/// 请求体里身份那几个字节不是恰好一处 ⇒ 认不准，不换。
#[test]
fn an_account_that_cannot_be_reached_says_why_and_nothing_more() {
    let home = Home::new("unready");
    home.sub("nocreds", 3_600, Some(UUID_B));
    std::fs::remove_file(home.dir("nocreds").join(".credentials.json")).expect("rm");
    home.sub("noid", 3_600, None);
    home.sub("stale", -60, Some(UUID_B));
    home.sub("b", 3_600, Some(UUID_B));
    let hop = home.hop(&["a", "nocreds", "noid", "stale", "b"], &[]);
    let own = home.own();
    let mut rx = home.tx.subscribe();
    let body = body_of(UUID_A, false);
    for who in ["nocreds", "noid", "nobody"] {
        assert_eq!(
            prep(&hop, &own, &body, &no_row, who).err(),
            Some(Unready::NeedsLogin),
            "{who}"
        );
    }
    assert!(rx.try_recv().is_err(), "没去续期就发了节点");
    assert_eq!(
        prep(&hop, &own, &body, &no_row, "stale").err(),
        Some(Unready::NeedsLogin),
        "续期失败该是需登录"
    );
    assert!(
        std::iter::from_fn(|| rx.try_recv().ok())
            .any(|n| n.source == crate::accounts::quota::nodes::Source::Login),
        "续期失败没发「登录」节点"
    );
    assert_eq!(
        prep(&hop, &own, &body_of(UUID_A, true), &no_row, "b").err(),
        Some(Unready::UnsureBody)
    );
}

/// ★ 中转照备好的去处发：订阅号 ⇒ 这一家的默认上游、鉴权头写那个号的令牌、带换好的请求体、记那个号；按量号 ⇒ key 表里那一行（上游与 key 都是那一行的）；
/// 按量号在表里没有行 · 起会话的号 ⇒ 都照那张决策表。
#[test]
fn the_relay_sends_by_what_was_prepared() {
    let table = RoutingTable::build([(
        AGENT.to_string(),
        "k".to_string(),
        crate::relay::base_of("https://k.invalid").expect("base"),
        Some(SecretKey::new("KEY-K")),
        AuthStyle::DEFAULT,
    )]);
    let ups = super::super::Upstreams::from_env(&|_| None).expect("内置默认");
    let key = RouteKey {
        seg1: AGENT.to_string(),
        seg2: "a".to_string(),
    };
    #[derive(Debug, PartialEq)]
    enum Said {
        Sub(String, Option<String>, Option<Vec<u8>>, String),
        Other(String),
    }
    let said = |go: Go| {
        let mut out = None;
        dispatch_go(&table, &ups, Mode::Substitute, &key, &[], go, &mut |d| {
            out = Some(match d {
                Destination::Substitute {
                    upstream,
                    auth,
                    body,
                    tag,
                } => Said::Sub(
                    upstream.host_header(),
                    auth.write.first().map(|(_, v)| v.to_string()),
                    body.map(<[u8]>::to_vec),
                    tag.to_string(),
                ),
                Destination::Refuse { status, reason, .. } => {
                    Said::Other(format!("refuse {status} {reason}"))
                }
                Destination::Passthrough { upstream, .. } => {
                    Said::Other(format!("pass {}", upstream.host_header()))
                }
                Destination::Reply { status, .. } => Said::Other(format!("reply {status}")),
            });
        });
        out.expect("一次都没答")
    };
    let start = {
        let mut out = None;
        super::super::decide(&table, &ups, Mode::Substitute, &key, &[], &mut |d| {
            out = Some(format!("{:?}", std::mem::discriminant(&d)));
        });
        out.expect("决策表没答")
    };
    let default_host = ups.of(AGENT, &[]).expect("默认上游").host_header();

    assert_eq!(
        said(Go::Sub {
            account: "b".into(),
            heads: vec![(
                "Authorization",
                HeadValue::Token {
                    prefix: "Bearer ",
                    token: SecretKey::new("TOKEN-B"),
                },
            )],
            body: Some(b"swapped".to_vec()),
        }),
        Said::Sub(
            default_host,
            Some("Bearer TOKEN-B".into()),
            Some(b"swapped".to_vec()),
            "b".into()
        )
    );
    assert_eq!(
        said(Go::Api {
            account: "k".into(),
            body: None,
        }),
        Said::Sub(
            "k.invalid".into(),
            Some("Bearer KEY-K".into()),
            None,
            "k".into()
        )
    );
    for go in [
        Go::Start,
        Go::Api {
            account: "gone".into(),
            body: None,
        },
    ] {
        let mut kind = None;
        dispatch_go(&table, &ups, Mode::Substitute, &key, &[], go, &mut |d| {
            kind = Some(format!("{:?}", std::mem::discriminant(&d)));
        });
        assert_eq!(kind.as_deref(), Some(start.as_str()), "该照决策表");
    }
}

/// ★ 起会话时没说是哪个号（`_`）：请求体里的身份恰好对上账号库里一个订阅号 ⇒ 那个号；一个都没对上 · 对上不止一个 · 没有身份格 ⇒ 不猜。按量号不参与。
#[test]
fn an_undeclared_send_is_recognized_only_by_a_single_identity_match() {
    let home = Home::new("recognize");
    home.sub("a", 3_600, Some(UUID_A));
    home.sub("b", 3_600, Some(UUID_B));
    home.sub("twin", 3_600, Some(UUID_B));
    home.sub("k", 3_600, Some(UUID_A));
    let one = home.hop(&["a", "b"], &["k"]);
    assert_eq!(
        one.recognize(AGENT, &body_of(UUID_A, false)).as_deref(),
        Some("a")
    );
    assert_eq!(
        one.recognize(AGENT, &body_of(UUID_B, false)).as_deref(),
        Some("b")
    );
    assert_eq!(
        one.recognize(
            AGENT,
            &body_of("cccccccc-3333-4333-8333-cccccccccccc", false)
        ),
        None
    );
    let two = home.hop(&["a", "b", "twin"], &[]);
    assert_eq!(
        two.recognize(AGENT, &body_of(UUID_B, false)),
        None,
        "对上两个号不该猜"
    );
    let bare = serde_json::to_vec(&serde_json::json!({"model": "m"})).expect("json");
    assert_eq!(one.recognize(AGENT, &bare), None);
}

/// ★ 账号 0（清单里那一条不设配置目录的 `~/.claude`：账号库每次写清单都合成它）不经选号 ⇒ 不是轮换里的号：
/// 事实里没有它；会话恰好起在账号 0 上时，开会话的号那一条照样拼得出。
#[test]
fn account_zero_is_not_a_rotation_account() {
    let home = Home::new("zero");
    home.sub("a", 3_600, Some(UUID_A));
    home.sub("b", 3_600, Some(UUID_B));
    let row = |id: &str| book_row(id, Some(home.dir(id)), false);
    let hop = home.hop_of(acct_core::AccountBook {
        accounts: vec![
            book_row(acct_core::BARE_ACCOUNT, None, false),
            row("a"),
            row("b"),
        ],
    });
    let ids: Vec<String> = hop
        .facts(AGENT, &no_row, now(), &crate::common::time::Tz::default())
        .into_iter()
        .map(|f| f.account)
        .collect();
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(
        hop.fact_one(
            AGENT,
            &hop.book(),
            acct_core::BARE_ACCOUNT,
            &no_row,
            now(),
            &crate::common::time::Tz::default()
        )
        .account,
        acct_core::BARE_ACCOUNT
    );
}

// ── 替身家（`agents::fake::upstream`）：换号头 · 用满回包 · 不声明换号头 ──

/// 中转对一个去处做的那一下：去处的上游 · 剥的名单 · 写的头 · 回包（各取一份自有值）。
#[derive(Debug, PartialEq)]
enum Did {
    Swap {
        clear: Vec<String>,
        write: Vec<(String, String)>,
        tag: String,
    },
    Reply {
        status: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    Other,
}

fn did(go: Go) -> Did {
    let table = RoutingTable::build(std::iter::empty());
    let ups = super::super::Upstreams::from_env(&|_| None).expect("内置默认");
    let key = RouteKey {
        seg1: AGENT.to_string(),
        seg2: "a".to_string(),
    };
    let mut out = Did::Other;
    dispatch_go(&table, &ups, Mode::Passthrough, &key, &[], go, &mut |d| {
        out = match d {
            Destination::Substitute { auth, tag, .. } => Did::Swap {
                clear: auth.clear.iter().map(|n| n.to_ascii_lowercase()).collect(),
                write: auth
                    .write
                    .iter()
                    .map(|(n, v)| (n.to_string(), v.to_string()))
                    .collect(),
                tag: tag.to_string(),
            },
            Destination::Reply {
                status,
                headers,
                body,
                ..
            } => Did::Reply {
                status: status.to_string(),
                headers: headers.to_vec(),
                body: body.to_vec(),
            },
            _ => Did::Other,
        };
    });
    out
}

/// ★ 替身家声明换两个头、策略选了第二个号 ⇒ 转发那一发恰带第二个号的那两个头（令牌那一格带前缀、凭据里点名的那一格原样）；
/// 剥的名单是鉴权头全集并上这两个头名 ⇒ 下游原来带的鉴权头（连同与这两个同名的）一条不剩。
#[test]
fn a_family_that_declares_two_swap_heads_gets_both_of_the_second_account() {
    let field = |f: &str| (f == "org").then(|| "org-of-second".to_string());
    let go = swap_go(
        "second",
        crate::agents::fake::upstream::SWAP_HEADS,
        &mut || Ok((SecretKey::new("TOKEN-OF-SECOND"), None)),
        &field,
    )
    .expect("两个头都取得到");
    let Did::Swap { clear, write, tag } = did(go) else {
        panic!("该是换头那一发");
    };
    assert_eq!(
        write,
        vec![
            (
                "X-Fake-Pass".to_string(),
                "Seat TOKEN-OF-SECOND".to_string()
            ),
            ("X-Fake-Org".to_string(), "org-of-second".to_string()),
        ]
    );
    assert_eq!(tag, "second");
    for gone in ["authorization", "x-api-key", "x-fake-pass", "x-fake-org"] {
        assert!(
            clear.iter().any(|c| c == gone),
            "{gone} 不在剥的名单里：{clear:?}"
        );
    }
}

/// 点名的那一格凭据里没有 ⇒ 接不上（要登录），不发一个缺头的请求。
#[test]
fn a_swap_head_whose_cell_is_missing_does_not_go() {
    let go = swap_go(
        "second",
        crate::agents::fake::upstream::SWAP_HEADS,
        &mut || Ok((SecretKey::new("T"), None)),
        &|_| None,
    );
    assert!(matches!(go, Err(Unready::NeedsLogin)));
}

/// ★ 硬上限停发：回的字节恰是替身家自己写的那一份用满回包（状态 · 头 · 体一个字节不差），中转不改。
#[test]
fn a_hold_replies_exactly_the_familys_own_limit_reply() {
    let reply = crate::agents::fake::upstream::limit_reply(1_800_003_600, 1_800_000_000, None);
    let want = Did::Reply {
        status: reply.status.to_string(),
        headers: reply.headers.clone(),
        body: reply.body.clone(),
    };
    assert_eq!(did(Go::Hold { reply }), want);
}

/// ★ 替身家不声明换号头 ⇒ 不换：照起会话那个号走（[`Go::Start`]）。
#[test]
fn a_family_without_swap_heads_stays_on_the_start_account() {
    let mut login = || -> Result<(SecretKey, Option<Vec<u8>>), Unready> {
        panic!("不换号就不该登录、续令牌")
    };
    let go = swap_go("second", &[], &mut login, &|_| None);
    assert!(matches!(go, Ok(Go::Start)));
}

// ── 替身家（`agents::fake`）的账号 0：声明不续不写 ⇒ 令牌到期的那一态说得出「不由 cc-monitor 续」 ──

/// 替身家的登录面，只把账号身份那一格换成读得出（替身家自己那一格恒读不出，判不到身份之后那几步）。
fn fake_face(base_login: crate::agents::BaseLogin) -> LoginFace {
    LoginFace {
        identity_in: |p| p.is_file().then(|| "who".to_string()),
        base_login,
        ..crate::agents::fake::full_row(|| None)
            .accounts
            .and_then(|f| f.login)
            .expect("替身家声明了登录面")
    }
}

/// 替身家那一形的凭据（`stamped` ＝ 上次续的那一秒，过期 ＝ 它 ＋ 一小时）写进 `dir`。
fn fake_creds_in(dir: &Path, stamped_s: u64) {
    std::fs::create_dir_all(dir).expect("mkdir");
    let doc = serde_json::json!({
        "session": {"bearer": "acc", "renewal": "ref", "app": "a", "grants": ["g"]},
        "stamped": stamped_s,
    });
    std::fs::write(dir.join("fake-login.ndjson"), doc.to_string()).expect("creds");
}

/// 一个号在替身家的这一形下的登录态：`base` ⇒ 账号 0（家目录下那份配置）；否则号载体。
fn fake_login(
    home: &Home,
    base_login: crate::agents::BaseLogin,
    base: bool,
    stamped_s: u64,
) -> LoginState {
    let face = fake_face(base_login);
    let (dir, at) = if base {
        (
            (crate::agents::fake::full_row(|| None)
                .accounts
                .expect("替身家有账号面")
                .shared_root)(&home.root),
            Some(home.root.clone()),
        )
    } else {
        (home.dir("c"), None)
    };
    fake_creds_in(&dir, stamped_s);
    let when = oauth::When {
        renew: crate::agents::Renew::Anytime,
        in_use: |_| false,
        base,
    };
    home.hop(&[], &[])
        .sub_login(&face, &dir, at.as_deref(), when, now())
        .login
}

/// ★ 声明账号 0 不续不写的那一家：账号 0 令牌过期 ⇒ 登录态是「不由 cc-monitor 续」那一态（不是正常、也不是要重新登录）；
/// 没过期 ⇒ 正常；同一家的号载体过期 ⇒ 照续，正常；不声明的那一家账号 0 过期 ⇒ 照续，正常。
#[test]
fn an_expired_account_zero_left_to_its_family_says_so() {
    use crate::agents::BaseLogin::{Renew, Untouched};
    let stale = now() - 7_200;
    let fresh = now() - 60;
    let home = Home::new("base-untouched");
    assert_eq!(
        fake_login(&home, Untouched, true, stale),
        LoginState::BaseUntouched
    );
    let home = Home::new("base-fresh");
    assert_eq!(fake_login(&home, Untouched, true, fresh), LoginState::Ok);
    let home = Home::new("carrier-stale");
    assert_eq!(fake_login(&home, Untouched, false, stale), LoginState::Ok);
    let home = Home::new("renewed-base");
    assert_eq!(fake_login(&home, Renew, true, stale), LoginState::Ok);
}

// ── 替身家注册表喂进上游选择（[`Hop::among`]）：换号那一路对账号 0 只读 ──

/// 替身家一家的账号库：账号 0（家目录下那份配置）与号载体 `c`；号表照替身家注册表认家。
fn fake_hop(home: &Home) -> Hop {
    home.hop_of(acct_core::AccountBook {
        // 号表照实写是替身家的号（没说家只认默认那一家，替身家不是默认）。
        accounts: [
            book_row("0", None, false),
            book_row("c", Some(home.dir("c")), false),
        ]
        .into_iter()
        .map(|a| acct_core::ManifestAccount {
            agent: Some(crate::agents::fake::AGENT_KIND.to_string()),
            ..a
        })
        .collect(),
    })
    .among(super::test_support::fake_registry())
}

fn fake_turn<'a>(body: &'a [u8], row: &'a dyn Fn(&str) -> Option<bool>) -> Turn<'a> {
    Turn {
        agent: crate::agents::fake::AGENT_KIND,
        start: "a",
        sid: "s",
        body,
        row,
        now: now(),
    }
}

/// ★ 声明账号 0 不续不写的那一家：账号 0 令牌过期 ⇒ 换号那一路判接不上、原因是「不由 cc-monitor 续」（不是要重新登录），
/// 凭据文件一字不变、不拿续期锁；同一家的号载体过期了照常判接得上（归我方续），没过期的照常换上它的两个头。
#[test]
fn a_family_fed_in_reports_an_expired_account_zero_as_left_to_it() {
    use super::test_support::fake_creds_in;
    let home = Home::new("fake-base");
    let hop = fake_hop(&home);
    let own = home.own();
    let base = home.root.join("shared");
    fake_creds_in(&base, now() - 7_200);
    let before = std::fs::read(base.join("fake-login.ndjson")).expect("read");
    let lib = hop.book();
    let body = b"{}".to_vec();
    let a = fake_turn(&body, &no_row);
    assert_eq!(
        hop.reach(&a, &lib, "0").err(),
        Some(Unready::BaseUntouched),
        "过期的账号 0 该判接不上、不由 cc-monitor 续"
    );
    assert_eq!(
        hop.prepare(&own, &a, &lib, "0").err(),
        Some(Unready::BaseUntouched),
        "换到过期的账号 0 该报不由 cc-monitor 续"
    );
    assert_eq!(
        std::fs::read(base.join("fake-login.ndjson")).expect("read"),
        before,
        "账号 0 的凭据被动了"
    );
    let mut left: Vec<String> = crate::platform::fs::Ro::read_dir(&base)
        .expect("read_dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        ["fake-login.ndjson"],
        "账号 0 的号目录里多出了凭据文件以外的东西（锁 · 旁稿）"
    );

    fake_creds_in(&home.dir("c"), now() - 7_200);
    assert!(
        matches!(hop.reach(&a, &lib, "c"), Ok(Reach::Sub)),
        "过期的号载体归我方续，该判接得上"
    );
    fake_creds_in(&home.dir("c"), now() - 60);
    match hop.prepare(&own, &a, &lib, "c") {
        Ok(Go::Sub { account, heads, .. }) => {
            assert_eq!(account, "c");
            let names: Vec<&str> = heads.iter().map(|(n, _)| *n).collect();
            assert_eq!(names, ["X-Fake-Pass", "X-Fake-Org"]);
        }
        Ok(_) => panic!("号载体没换上它的头"),
        Err(e) => panic!("号载体接不上：{e:?}"),
    }
}

/// ★ 续令牌一时没成、等下次再试的号（[`oauth::missed::Unrenewed::RetryAt`]）：「此刻能不能用」只由 [`Hop::sub_login`] 一处给 ——
/// 帧面（[`Hop::show_facts`]）与换号那一路（[`Hop::reach`]）同一个说法：都说此刻用不了，帧面带着几点再试。
#[test]
fn an_account_waiting_to_retry_its_renewal_is_told_the_same_on_both_sides() {
    let home = Home::new("fake-retry");
    let hop = fake_hop(&home);
    let own = home.own();
    let lib = hop.book();
    let body = b"{}".to_vec();
    let a = fake_turn(&body, &no_row);
    // 号载体过期：换到它那一下去续，续期端点不听 ⇒ 记成一时没续上、到点再试。
    fake_creds_in(&home.dir("c"), now() - 7_200);
    assert!(
        hop.prepare(&own, &a, &lib, "c").is_err(),
        "续期端点不听，该接不上"
    );
    let face = hop
        .login_face(crate::agents::fake::AGENT_KIND)
        .expect("替身家声明了登录面");
    assert!(
        matches!(
            oauth::missed::missed(&home.dir("c"), &face, now()),
            Some(oauth::missed::Unrenewed::RetryAt(_))
        ),
        "前提：那个号记成一时没续上"
    );
    let shown = hop.show_facts(crate::agents::fake::AGENT_KIND, &lib, "c", &no_row, now());
    let reached = hop.reach(&a, &lib, "c");
    assert_eq!(
        shown.login == LoginState::Ok && shown.retry_at.is_none(),
        reached.is_ok(),
        "帧面说登录 {:?}、几点再试 {:?}，换号那一路说 {:?}：两边说法不一",
        shown.login,
        shown.retry_at,
        reached.as_ref().err()
    );
    assert_eq!(
        reached.err(),
        Some(Unready::NeedsLogin),
        "等再试的号此刻该接不上"
    );
    assert!(
        shown.retry_at.is_some_and(|t| t > now()),
        "帧面该带着几点再试"
    );
    let tz = crate::common::time::Tz::default();
    let fact = hop.fact_one(
        crate::agents::fake::AGENT_KIND,
        &lib,
        "c",
        &no_row,
        now(),
        &tz,
    );
    assert_eq!(
        fact.blocked.and_then(|b| b.until),
        shown.retry_at,
        "事实那一格的「几点回来」该照同一个再试时刻"
    );
}
