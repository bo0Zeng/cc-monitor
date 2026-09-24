use super::*;
use creds_core::store;

/// 判据里这些行挂在谁名下。**名字刻意不是任何一家真 agent** —— 本文件量的是
/// 「键有两段」这件事本身，与哪家是登记过的无关（那一格在下面单独量）。
const AGENT: &str = "agent-under-test";

fn base(u: &str) -> Base {
    Base::parse(u).expect("夹具的 base 应当解析得了")
}

fn entries(raw: &str) -> Vec<AccountEntry> {
    store::read_accounts(&store::parse(raw).expect("夹具应当可解析"))
}

/// ★★ 装表这一步**认得两条账号，各带各的上游与各自的 key**。
#[test]
fn two_accounts_land_in_two_rows_each_with_its_own_upstream_and_key() {
    let (t, bad, _) = build(
        entries(
            r#"{"accounts":{
                    "acct-a":{"api_key":"KEY-A","base_url":"https://a.invalid"},
                    "acct-b":{"api_key":"KEY-B"}
                }}"#,
        ),
        AGENT,
        &base("https://default.invalid"),
    );
    assert!(bad.is_empty(), "不该有装不进去的行：{:?}", bad.len());
    assert_eq!(t.len(), 2);

    let a = t.lookup(AGENT, "acct-a").expect("A 应当查得到");
    let b = t.lookup(AGENT, "acct-b").expect("B 应当查得到");
    // ★★ **反空真排最前**〔`D1` 一并修〕：两行的 key 必须**不同**，
    //    否则下面「各用各的」整族断言恒真。先前它排在最后。
    assert_ne!(
        a.key().expect("A").expose_for_auth_header(),
        b.key().expect("B").expose_for_auth_header(),
        "两行的 key 一样 —— 下面整族断言在空转"
    );
    // ★ 同理：两行的**端点**也必须不同（本件题目就是每行端点不一样）。
    assert_ne!(
        a.base().host_header(),
        b.base().host_header(),
        "两行的端点一样 —— 「端点跟着行走」那一向量不到"
    );
    // 期望值全是**手写字面量**。
    assert_eq!(a.base().host_header(), "a.invalid");
    assert_eq!(a.key().expect("A 有 key").expose_for_auth_header(), "KEY-A");
    // 没写 `base_url` 的那条用**默认上游** —— 这不是回落，是这一行的字段取默认值。
    assert_eq!(b.base().host_header(), "default.invalid");
    assert_eq!(b.key().expect("B 有 key").expose_for_auth_header(), "KEY-B");
}

/// ★★★ **`KH2` 在这一层的那一半**：查不到就是查不到，**没有任何一行会被顶上来**。
#[test]
fn an_account_that_is_not_in_the_table_resolves_to_nothing() {
    let (t, _, _) = build(
        entries(r#"{"accounts":{"acct-a":{"api_key":"KEY-A"}}}"#),
        AGENT,
        &base("https://default.invalid"),
    );
    // 分母 = 我列出的这 4 个不存在的 id。
    for miss in ["acct-b", "default", "", "ACCT-A"] {
        assert!(
            t.lookup(AGENT, miss).is_none(),
            "{miss:?} 不在表里，却查到了一行 —— 那就是回落"
        );
    }
    // ★ 非空对照承重：同一张表、同一把尺子，**存在**的那一条查得到。
    assert!(t.lookup(AGENT, "acct-a").is_some(), "这把尺子是瞎的");
}

/// **进不了表的两形，都要出声**，而且不许静默回落。
#[test]
fn a_row_that_cannot_be_reached_or_cannot_be_parsed_is_rejected_out_loud() {
    let (t, bad, _) = build(
        entries(
            r#"{"accounts":{
                    "bad.id":{"api_key":"K1"},
                    "ok-id":{"api_key":"K2","base_url":"ftp://nope"},
                    "good":{"api_key":"K3"}
                }}"#,
        ),
        AGENT,
        &base("https://default.invalid"),
    );
    // ★★ **承重的那条排最前**〔`D1` 一并修，08-28〕：
    //    先前它排在整条判据的**最后**，被前面 `assert_eq!(t.len(), 1)` 挡着
    //    ⇒ `D1-M6` 实测红在前面那条，**这一句一次都没被求值**。
    //    本件这是**第二次**同形（`KH2` 那两条已经修过一次）⇒ 全件扫过一遍。
    assert!(
        t.lookup(AGENT, "ok-id").is_none(),
        "打错的端点回落到了默认上游 —— 那比 404 坏得多"
    );
    // 同族的另一半：id 装不下的那条也不许在表里。
    assert!(
        t.lookup(AGENT, "bad.id").is_none(),
        "id 当不了路由段的那条进了表"
    );

    assert_eq!(t.len(), 1, "只有 `good` 该进表");
    assert!(t.lookup(AGENT, "good").is_some());
    assert_eq!(bad.len(), 2, "两条都该被说出去：{}", bad.len());
    let ids: Vec<&str> = bad.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"bad.id"), "id 装不下那条没被说出去");
    assert!(ids.contains(&"ok-id"), "base_url 坏那条没被说出去");
}

/// `K-H2a` 那份**旧文件**（顶层一把 key）装出来是一条 id 逐字是 `default` 的行，
/// 用默认上游 —— **别的 id 一律查不到**。
#[test]
fn the_legacy_single_key_file_becomes_exactly_one_reachable_row() {
    let (t, bad, _) = build(
        entries(r#"{"api_key":"LEGACY"}"#),
        AGENT,
        &base("https://api.example.invalid"),
    );
    // ★★ **承重的那条排最前**〔`D1` 一并修〕：它是「**有名字的行**，不是默认行」
    //    这句话的全部内容 —— 先前它排在最后。
    assert!(
        t.lookup(AGENT, "anything-else").is_none(),
        "别的账号段也查到了那一行 —— 那它就成了「默认行」，正是 `KH2` 禁的回落"
    );
    assert!(bad.is_empty());
    assert_eq!(t.len(), 1);
    let row = t
        .lookup(AGENT, store::LEGACY_ACCOUNT_ID)
        .expect("default 应当查得到");
    assert_eq!(row.base().host_header(), "api.example.invalid");
    assert_eq!(
        row.key().expect("有 key").expose_for_auth_header(),
        "LEGACY"
    );
}

/// **行在、但这一行没配 key** —— 合法状态（订阅那一档），`lookup` 查得到、`key()` 是 `None`。
///
/// 这一条与上面那条 `an_account_that_is_not_in_the_table_resolves_to_nothing`
/// **是两件事**，件计划逐字要求它们分开。
#[test]
fn a_row_can_exist_and_still_have_no_key_of_its_own() {
    let (t, bad, _) = build(
        entries(r#"{"accounts":{"sub-only":{"base_url":"https://sub.invalid"}}}"#),
        AGENT,
        &base("https://default.invalid"),
    );
    assert!(bad.is_empty());
    let row = t.lookup(AGENT, "sub-only").expect("行应当在");
    assert!(row.key().is_none(), "这一行不该有 key");
    assert_eq!(row.base().host_header(), "sub.invalid");
    // ★ `K-R1`：没写 `auth_style` 的那一行落到默认 ⇒ 一份旧文件的行为一个字节不变。
    assert_eq!(row.auth_style(), AuthStyle::DEFAULT);
}

/// ★★★ **`K-R1` 的正主之一**：鉴权头形状**跟着行走**，两行各是各的。
///
/// # 反空真排最前
///
/// 两行的形状必须**不同** —— 一样的话「跟着行走」这一向根本量不到。
#[test]
fn each_row_carries_its_own_auth_style_and_a_missing_one_means_the_default() {
    let (t, bad, _) = build(
        entries(
            r#"{"accounts":{
                    "as-xapikey":{"api_key":"K1","auth_style":"x-api-key"},
                    "as-default":{"api_key":"K2"},
                    "as-local":{"base_url":"http://127.0.0.1:11434/v1","auth_style":"none"}
                }}"#,
        ),
        AGENT,
        &base("https://default.invalid"),
    );
    assert!(bad.is_empty(), "不该有装不进去的行：{}", bad.len());
    assert_eq!(t.len(), 3);

    let x = t.lookup(AGENT, "as-xapikey").expect("x-api-key 那一行");
    let d = t.lookup(AGENT, "as-default").expect("没写的那一行");
    let l = t.lookup(AGENT, "as-local").expect("本地那一行");
    // ★★ 反空真：三行的形状两两不同（否则下面整族断言恒真）。
    assert_ne!(x.auth_style(), d.auth_style());
    assert_ne!(x.auth_style(), l.auth_style());
    assert_ne!(d.auth_style(), l.auth_style());
    // 期望值是**手写字面量**。
    assert_eq!(x.auth_style(), AuthStyle::XApiKey);
    assert_eq!(d.auth_style(), AuthStyle::DEFAULT);
    assert_eq!(l.auth_style(), AuthStyle::NoAuth);
    // ★ 本地那一行的三样**同时**成立：无鉴权 · 自定义 host:port/path · 明文回环。
    assert!(l.key().is_none(), "本地那一行不该有 key");
    assert_eq!(l.base().host_header(), "127.0.0.1:11434");
    assert_eq!(l.base().upstream_target("/v1/messages"), "/v1/v1/messages");
}

/// ★★★ `K-R1`：**认不出的 `auth_style` 不许被悄悄当成默认**，
/// 「说不发头又配了 key」这条自相矛盾也一样 —— 两条各自一格，各自有话说。
///
/// # 单断在这里是承重的
///
/// 三条判断（认不出 / 矛盾 / 明文非回环）各有一格**只有它红**的夹具行，
/// 而不是一格「全都坏」的行 —— 后者只买得到目录级塌陷。〔`brief` 9〕
#[test]
fn a_row_whose_auth_style_makes_no_sense_is_rejected_out_loud() {
    let (t, bad, _) = build(
        entries(
            r#"{"accounts":{
                    "typo":{"api_key":"K1","auth_style":"bearerr"},
                    "contradiction":{"api_key":"K2","auth_style":"none"},
                    "good":{"api_key":"K3","auth_style":"bearer"}
                }}"#,
        ),
        AGENT,
        &base("https://default.invalid"),
    );
    // ★★ **承重的那两条排最前**：它们不许静默回落进表。
    assert!(
        t.lookup(AGENT, "typo").is_none(),
        "认不出的 auth_style 被回落成默认了 —— 症状是一条查不出来的 401"
    );
    assert!(
        t.lookup(AGENT, "contradiction").is_none(),
        "「不发任何头」与「配了 key」同时在，却被挑了一句执行 —— 那是替人猜意思"
    );
    // ★ 非空对照：好的那条进得去（不是恒拒）。
    assert_eq!(t.len(), 1, "只有 `good` 该进表");
    assert!(t.lookup(AGENT, "good").is_some(), "这把尺子是瞎的");

    // 两条的**理由不同** —— 合成一句的话，其中一句在另一形上是假的指引。
    let why = |id: &str| {
        bad.iter()
            .find(|r| r.id == id)
            .map(|r| r.why)
            .unwrap_or_else(|| panic!("{id} 没被说出去"))
    };
    assert_eq!(why("typo"), WHY_AUTH_STYLE_UNKNOWN);
    assert_eq!(why("contradiction"), WHY_NO_AUTH_WITH_KEY);
    assert_ne!(why("typo"), why("contradiction"));
}

/// ★★★ `K-R1`：**明文 http 只许连回环**。
///
/// ⚠ 分母 = 我列出的这 4 形（2 形该拒、2 形该放）。它**不是**「所有明文写法」。
#[test]
fn a_plaintext_upstream_is_only_allowed_on_loopback() {
    let (t, bad, _) = build(
        entries(
            r#"{"accounts":{
                    "off-loopback":{"api_key":"K1","base_url":"http://1.2.3.4/v1"},
                    "private-lan":{"api_key":"K2","base_url":"http://10.0.0.1:8000/v1"},
                    "on-loopback":{"api_key":"K3","base_url":"http://127.0.0.1:11434/v1"},
                    "tls-anywhere":{"api_key":"K4","base_url":"https://1.2.3.4/v1"}
                }}"#,
        ),
        AGENT,
        &base("https://default.invalid"),
    );
    // ★★ 承重的排最前：明文 + 非回环的两条**一条都不许进表**。
    assert!(
        t.lookup(AGENT, "off-loopback").is_none(),
        "明文 http 打到公网地址进了表 —— 那一行的 key 会明着过网线"
    );
    assert!(
        t.lookup(AGENT, "private-lan").is_none(),
        "内网也不是回环：那把 key 仍然明着过网线，只是网线短一点"
    );
    // ★ 非空对照（两格，各自单断）：回环上的明文放行，非回环上的 TLS 也放行
    //   ⇒ 本条判的是「明文 **且** 非回环」这个合，不是其中任一个。
    assert!(
        t.lookup(AGENT, "on-loopback").is_some(),
        "回环上的明文被拒了 —— 那把本地部署这一格整个关掉了"
    );
    assert!(
        t.lookup(AGENT, "tls-anywhere").is_some(),
        "TLS 打到公网被拒了 —— 本条把 `裁-1` 反过来读了"
    );
    assert_eq!(t.len(), 2);
    for id in ["off-loopback", "private-lan"] {
        let r = bad
            .iter()
            .find(|r| r.id == id)
            .unwrap_or_else(|| panic!("{id} 没被说出去"));
        assert_eq!(r.why, WHY_PLAINTEXT_OFF_LOOPBACK);
    }
}

/// ★★ `K-R1`：**能用但行为与默认不同的行要出声** —— 那是 [`Note`] 这个类型的全部意义。
///
/// # 它治的是本件最阴的那一格
///
/// 路径前缀是**承重的、会改变字节**，而它错了的时候（前缀与客户端路径头一段重了）
/// 上游给的是一个 404 ——「配错了」与「上游挂了」同形。
/// ⇒ 装表那一刻就把这件事说出来，别等它变成一条查不出来的 404。
#[test]
fn a_row_that_still_works_but_behaves_differently_gets_a_note() {
    let (t, bad, notes) = build(
        entries(
            r#"{"accounts":{
                    "plain":{"api_key":"K1"},
                    "prefixed":{"api_key":"K2","base_url":"https://gw.invalid/anthropic"},
                    "xapikey":{"api_key":"K3","auth_style":"x-api-key"}
                }}"#,
        ),
        AGENT,
        &base("https://default.invalid"),
    );
    assert!(bad.is_empty(), "这三条都该进表：{}", bad.len());
    assert_eq!(t.len(), 3);

    let of = |id: &str| -> Vec<&'static str> {
        notes
            .iter()
            .filter(|n| n.id == id)
            .map(|n| n.what)
            .collect()
    };
    // ★★ **反空真排最前**：什么都没配特别的那一行**一条 note 都不该有**
    //    —— 没有这一格，「出声了」可能只是因为它对每一行都出声。
    assert!(
        of("plain").is_empty(),
        "默认那一行也出声了 ⇒ 全是噪音：{:?}",
        of("plain")
    );
    assert_eq!(of("prefixed"), vec![NOTE_PATH_PREFIX]);
    assert_eq!(of("xapikey"), vec![NOTE_AUTH_STYLE_X_API_KEY]);
    assert_eq!(notes.len(), 2, "note 的条数不对：{}", notes.len());
}

/// ★★ `K-R1`：**非默认的每一个鉴权头形状都有话说**，默认那个不说。
///
/// 它把「哪个是默认」这件事的住址钉在 `AuthStyle::DEFAULT` 上：
/// 默认值哪天换了，本条会去核新的那个不出声、旧的那个开始出声。
/// ⚠ 它**不**证明那几句话是对的（散文没人机检），只证明「一个都没漏、也没有两个共用一句」。
#[test]
fn every_auth_style_other_than_the_default_gets_announced() {
    assert!(
        note_for_auth_style(AuthStyle::DEFAULT).is_none(),
        "默认那个形状也出声 ⇒ 每一行都会印一句，那是噪音不是信号"
    );
    let mut said: Vec<&'static str> = Vec::new();
    for s in AuthStyle::ALL.iter().copied() {
        if s == AuthStyle::DEFAULT {
            continue;
        }
        let what =
            note_for_auth_style(s).unwrap_or_else(|| panic!("{s:?} 是非默认形状，却一个字都不说"));
        said.push(what);
    }
    // 反空真：真的走过至少一个非默认成员。
    assert!(!said.is_empty(), "闭集里只有默认那一个 —— 本条在空转");
    let n = said.len();
    said.sort();
    said.dedup();
    assert_eq!(said.len(), n, "有两个形状共用了同一句话：{said:?}");
}

// ════════════════════════════════════════════════════════════════════════════
//  条 49 / 条 59 / 条 60：表的键是 agent ＋ 账号 · 默认上游每 agent 一行 · 未登记直接拒
// ════════════════════════════════════════════════════════════════════════════
//
// 期望值一律**手写字面量**（`"claude-code"` · `"CCM_RELAY_UPSTREAM"` · 那条官方 URL 的主机名），
// 不拿被测的 `CREDENTIALS_FILE_AGENT` / `AGENT_UPSTREAMS` 去算 —— 拿被测常量写期望值再拿它去读，
// 两侧同源，恒真。

use super::super::{decide, Upstreams, AGENT_UPSTREAMS};
use crate::relay::{Destination, Mode, RouteKey};

/// 问一次生产段那张决策表，把答案压成一个好比对的形状。
#[derive(Debug, PartialEq, Eq)]
enum Said {
    Refuse(&'static str),
    Passthrough(String),
    /// `(上游主机, 要写的那个头值)`。
    Substitute(String, Option<String>),
}

fn ask(t: &RoutingTable, u: &Upstreams, mode: Mode, seg1: &str, seg2: &str) -> Said {
    let key = RouteKey {
        seg1: seg1.to_string(),
        seg2: seg2.to_string(),
    };
    let mut out = None;
    decide(t, u, mode, &key, &mut |d| {
        out = Some(match d {
            Destination::Refuse { status, .. } => Said::Refuse(status),
            Destination::Passthrough { upstream } => Said::Passthrough(upstream.host_header()),
            Destination::Substitute { upstream, auth } => Said::Substitute(
                upstream.host_header(),
                auth.write.map(|(_, v)| v.to_string()),
            ),
        });
    });
    out.expect("层 2 一次都没答")
}

/// 没有任何环境变量时那一份（每家取内置默认）。
fn upstreams_without_env() -> Upstreams {
    Upstreams::from_env(&|_| None).expect("内置默认必须解析得了")
}

/// ★★★ **条 49 的正主**：同一个账号 id 挂在两家名下 = **两行**，各带各的上游与 key；
/// 第三家拿同一个 id 来查 ⇒ **查不到**（`/s/` 回 404，一个字节不发上游）。
///
/// 先前 `lookup` 只收账号 ⇒ 这里的三问会拿到**同一行**：claude 的 3 号与 codex 的 3 号是一个东西。
#[test]
fn the_same_account_id_under_two_agents_is_two_rows_and_a_third_agent_finds_neither() {
    let a = base("https://a.invalid");
    let b = base("https://b.invalid");
    let t = RoutingTable::build([
        (
            "agent-one".to_string(),
            "3".to_string(),
            a,
            Some(creds_core::SecretKey::new("KEY-ONE")),
            AuthStyle::DEFAULT,
        ),
        (
            "agent-two".to_string(),
            "3".to_string(),
            b,
            Some(creds_core::SecretKey::new("KEY-TWO")),
            AuthStyle::DEFAULT,
        ),
    ]);
    // 反空真排最前：两行真的是两行（不是后一行把前一行盖掉了）。
    assert_eq!(
        t.len(),
        2,
        "同一个账号 id 在两家名下只剩一行 ⇒ 键里没有 agent"
    );

    let u = upstreams_without_env();
    assert_eq!(
        ask(&t, &u, Mode::Substitute, "agent-one", "3"),
        Said::Substitute("a.invalid".into(), Some("Bearer KEY-ONE".into()))
    );
    assert_eq!(
        ask(&t, &u, Mode::Substitute, "agent-two", "3"),
        Said::Substitute("b.invalid".into(), Some("Bearer KEY-TWO".into())),
        "第二家拿到的不是它自己那一行 ⇒ 表仍然只按账号查"
    );
    // ★ 最坏失效形态的那一格：第三家（今天它就是 codex 的位置）拿同一个 id ⇒ 404，不是「借一行」。
    assert_eq!(
        ask(&t, &u, Mode::Substitute, "agent-three", "3"),
        Said::Refuse("404 Not Found"),
        "第三家查到了别家的 3 号 ⇒ 那就是「拿 A 的 key 发 B 的请求」"
    );
}

/// ★★ 凭据文件装表那条真路（`build`）：每一行挂在**交进来的那一家**名下，别家查不到。
#[test]
fn rows_loaded_from_the_credentials_file_belong_to_the_agent_they_were_loaded_under() {
    let (t, bad, _) = build(
        entries(r#"{"accounts":{"acct-a":{"api_key":"KEY-A"}}}"#),
        "claude-code",
        &base("https://default.invalid"),
    );
    assert!(bad.is_empty());
    assert!(
        t.lookup("claude-code", "acct-a").is_some(),
        "这把尺子是瞎的（装进去的那一行自己都查不到）"
    );
    assert!(
        t.lookup("codex", "acct-a").is_none(),
        "codex 名下查到了 claude-code 的那一行 ⇒ 键里没有 agent"
    );
}

/// ★★★ **条 59 ＋ 「未登记直接拒」**：`/t/` ＋ 表里无行 ⇒ **登记过的那家**发到它**自己那一行**，
/// **未登记的**回 502。两格用同一把尺子量，互为对照。
///
/// 🔴 那个 502 **不许**变成「透传到某一家」：那正是 `设计/20 §3.1` 第 4 行那条 🔴 禁的回落。
#[test]
fn passthrough_without_a_row_goes_to_that_agents_own_upstream_and_an_unregistered_agent_is_refused()
{
    let t = RoutingTable::build(std::iter::empty());
    let u = upstreams_without_env();
    assert_eq!(
        ask(&t, &u, Mode::Passthrough, "claude-code", "acct-anything"),
        Said::Passthrough("api.anthropic.com".into()),
        "登记过的那家在 `/t/` 无行时没有发到它自己的默认上游"
    );
    assert_eq!(
        ask(&t, &u, Mode::Passthrough, "codex", "acct-anything"),
        Said::Refuse("502 Bad Gateway"),
        "🔴 未登记的 agent 没被拒 ⇒ 它的请求被发到了某一家的上游"
    );
    // `/s/` 无行仍是 404（`§3.1` 第 2 行，一字不改），与 agent 登没登记无关。
    for agent in ["claude-code", "codex"] {
        assert_eq!(
            ask(&t, &u, Mode::Substitute, agent, "acct-anything"),
            Said::Refuse("404 Not Found"),
            "{agent}：`/s/` 无行该是 404"
        );
    }
}

/// ★★ **条 60：每家一个环境旋钮**，而且它**只盖那一家**。
///
/// 取值器按**手写的变量名**答话 ⇒ 名字写错 / 读错了家，下面当场对不上。
#[test]
fn each_agents_env_knob_overrides_only_that_agents_default() {
    let got = Upstreams::from_env(&|k| {
        (k == "CCM_RELAY_UPSTREAM").then(|| "http://127.0.0.1:1/pfx".to_string())
    })
    .expect("回环明文是合法上游");
    let cc = got.of("claude-code").expect("claude-code 该登记着");
    assert_eq!(cc.host_header(), "127.0.0.1:1");
    assert_eq!(cc.path, "/pfx", "旋钮里那段路径前缀被丢掉了");
    // 非空对照：不设旋钮时是内置默认 —— 证明上面那格不是「恒等于取值器的值」。
    let dflt = upstreams_without_env();
    assert_eq!(
        dflt.of("claude-code").expect("登记着").host_header(),
        "api.anthropic.com"
    );
    assert_ne!(got, dflt);
    // 认不出 ⇒ `None`（调用方出声退 2），不跳过、不回落。
    assert!(Upstreams::from_env(&|_| Some("ftp://x".to_string())).is_none());
    // 未登记的那家：查不到，不是拿别家的顶上。
    assert!(got.of("codex").is_none());
}

/// ★ 那张每家一行的表**自己的形状**：家名不重 · 旋钮不重 · 凭据文件那一家在表里。
///
/// ⚠ 这是一条**构造期**断言（表是一个 `const`），它买的是「加第二家时不会把旋钮抄成同一个」。
#[test]
fn the_per_agent_upstream_table_has_no_duplicate_agent_or_knob_and_holds_the_credentials_file_agent(
) {
    let agents: std::collections::BTreeSet<&str> =
        AGENT_UPSTREAMS.iter().map(|a| a.agent).collect();
    let knobs: std::collections::BTreeSet<&str> = AGENT_UPSTREAMS.iter().map(|a| a.env).collect();
    assert!(
        !AGENT_UPSTREAMS.is_empty(),
        "表是空的 ⇒ 每一家都未登记，下面几条空转"
    );
    assert_eq!(agents.len(), AGENT_UPSTREAMS.len(), "同一家登记了两行");
    assert_eq!(
        knobs.len(),
        AGENT_UPSTREAMS.len(),
        "两家共用一个环境旋钮 ⇒ 盖一家等于盖两家"
    );
    assert!(
        agents.contains("claude-code"),
        "凭据文件那一家（手写 `claude-code`）不在表里 ⇒ 文件里的行没有默认上游可取"
    );
}
