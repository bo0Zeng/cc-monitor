//! 备料（换到别的号时这一发怎么走）：照模块头逐条钉 —— 起会话的号照原样（[`Go::Start`]）；订阅号换上那个号的访问令牌、请求体里账号身份那一格
//! 一起换；按量号照它在 key 表里那一行发、身份格换成空串；拿不到登录 / key / 认不准身份格只回原因码。
//! 还有：认出 `_` 那一发在用哪个号（[`Hop::recognize`]）· 「其余的订阅号」（[`rest_members`]）。
//! 临时家目录、假凭据、假身份、续令牌发到本机一个绑着不听的口；不起中转。

use super::*;
use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::quota::rotation;
use crate::relay::{Base, Mode};
use creds_core::store::AuthStyle;

const UUID_A: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
const UUID_B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
const AGENT: &str = "claude-code";

fn now() -> u64 {
    crate::accounts::quota::now_unix()
}

/// 续令牌一律发到本机一个绑着不听的口（拨了必被拒），绝不发到那一家的真端点。
fn dead_token_endpoint() -> TokenEndpoint {
    let port = crate::refusing_port::refusing_port();
    TokenEndpoint {
        base: Base::parse(&format!("http://127.0.0.1:{port}")).expect("base"),
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
        let mut accounts: Vec<LibAccount> = subs
            .iter()
            .map(|id| LibAccount {
                id: id.to_string(),
                dir: Some(accts.join(id)),
                api: false,
            })
            .collect();
        accounts.extend(apis.iter().map(|id| LibAccount {
            id: id.to_string(),
            dir: Some(accts.join(id)),
            api: true,
        }));
        Hop::new(
            Arc::new(RotationStore::with_nodes(
                Some(self.root.join(rotation::FILE_NAME)),
                self.tx.clone(),
            )),
            Arc::new(Ledger::open(
                Some(self.root.join(ledger::FILE_NAME)),
                self.tx.clone(),
                Default::default(),
            )),
            Some(self.root.clone()),
            Arc::new(move |_| Library {
                enabled: true,
                accounts: accounts.clone(),
            }),
            Arc::new(std::collections::BTreeSet::new),
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
    crate::agents::login_of(AGENT).and_then(|f| (f.identity_of_body)(body))
}

fn prep(
    hop: &Hop,
    own: &Own,
    body: &[u8],
    row: &dyn Fn(&str) -> Option<bool>,
    to: &str,
) -> Result<Go, Unready> {
    let lib = hop.library();
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
        token,
        body,
    }) = prep(&hop, &own, &sent, &no_row, "b")
    else {
        panic!("b 该备成订阅号那一发");
    };
    assert_eq!(account, "b");
    assert!(
        token.same_secret(&SecretKey::new("fake-access-of-b")),
        "令牌不是 b 那一份"
    );
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
        Base::parse("https://k.invalid").expect("base"),
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
                    auth.write.map(|(_, v)| v.to_string()),
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
            token: SecretKey::new("TOKEN-B"),
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

/// ★ 「其余的订阅号」：库里没点名的、名字过得了路由段的订阅号（`_` 不算），按名字排。
#[test]
fn the_rest_members_are_the_unnamed_subscriptions_by_name() {
    let ids = ["z", "b", "_", "a", "k", "bad/seg", "c"];
    let named = vec!["c".to_string()];
    assert_eq!(
        rest_members(ids, &named, |x| x != "k"),
        vec!["a".to_string(), "b".to_string(), "z".to_string()]
    );
}
