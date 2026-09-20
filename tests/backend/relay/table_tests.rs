use super::*;
use creds_core::store;

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
        &base("https://default.invalid"),
    );
    assert!(bad.is_empty(), "不该有装不进去的行：{:?}", bad.len());
    assert_eq!(t.len(), 2);

    let a = t.lookup("acct-a").expect("A 应当查得到");
    let b = t.lookup("acct-b").expect("B 应当查得到");
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
        &base("https://default.invalid"),
    );
    // 分母 = 我列出的这 4 个不存在的 id。
    for miss in ["acct-b", "default", "", "ACCT-A"] {
        assert!(
            t.lookup(miss).is_none(),
            "{miss:?} 不在表里，却查到了一行 —— 那就是回落"
        );
    }
    // ★ 非空对照承重：同一张表、同一把尺子，**存在**的那一条查得到。
    assert!(t.lookup("acct-a").is_some(), "这把尺子是瞎的");
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
        &base("https://default.invalid"),
    );
    // ★★ **承重的那条排最前**〔`D1` 一并修，08-28〕：
    //    先前它排在整条判据的**最后**，被前面 `assert_eq!(t.len(), 1)` 挡着
    //    ⇒ `D1-M6` 实测红在前面那条，**这一句一次都没被求值**。
    //    本件这是**第二次**同形（`KH2` 那两条已经修过一次）⇒ 全件扫过一遍。
    assert!(
        t.lookup("ok-id").is_none(),
        "打错的端点回落到了默认上游 —— 那比 404 坏得多"
    );
    // 同族的另一半：id 装不下的那条也不许在表里。
    assert!(t.lookup("bad.id").is_none(), "id 当不了路由段的那条进了表");

    assert_eq!(t.len(), 1, "只有 `good` 该进表");
    assert!(t.lookup("good").is_some());
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
        &base("https://api.example.invalid"),
    );
    // ★★ **承重的那条排最前**〔`D1` 一并修〕：它是「**有名字的行**，不是默认行」
    //    这句话的全部内容 —— 先前它排在最后。
    assert!(
        t.lookup("anything-else").is_none(),
        "别的账号段也查到了那一行 —— 那它就成了「默认行」，正是 `KH2` 禁的回落"
    );
    assert!(bad.is_empty());
    assert_eq!(t.len(), 1);
    let row = t
        .lookup(store::LEGACY_ACCOUNT_ID)
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
        &base("https://default.invalid"),
    );
    assert!(bad.is_empty());
    let row = t.lookup("sub-only").expect("行应当在");
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
        &base("https://default.invalid"),
    );
    assert!(bad.is_empty(), "不该有装不进去的行：{}", bad.len());
    assert_eq!(t.len(), 3);

    let x = t.lookup("as-xapikey").expect("x-api-key 那一行");
    let d = t.lookup("as-default").expect("没写的那一行");
    let l = t.lookup("as-local").expect("本地那一行");
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
        &base("https://default.invalid"),
    );
    // ★★ **承重的那两条排最前**：它们不许静默回落进表。
    assert!(
        t.lookup("typo").is_none(),
        "认不出的 auth_style 被回落成默认了 —— 症状是一条查不出来的 401"
    );
    assert!(
        t.lookup("contradiction").is_none(),
        "「不发任何头」与「配了 key」同时在，却被挑了一句执行 —— 那是替人猜意思"
    );
    // ★ 非空对照：好的那条进得去（不是恒拒）。
    assert_eq!(t.len(), 1, "只有 `good` 该进表");
    assert!(t.lookup("good").is_some(), "这把尺子是瞎的");

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
        &base("https://default.invalid"),
    );
    // ★★ 承重的排最前：明文 + 非回环的两条**一条都不许进表**。
    assert!(
        t.lookup("off-loopback").is_none(),
        "明文 http 打到公网地址进了表 —— 那一行的 key 会明着过网线"
    );
    assert!(
        t.lookup("private-lan").is_none(),
        "内网也不是回环：那把 key 仍然明着过网线，只是网线短一点"
    );
    // ★ 非空对照（两格，各自单断）：回环上的明文放行，非回环上的 TLS 也放行
    //   ⇒ 本条判的是「明文 **且** 非回环」这个合，不是其中任一个。
    assert!(
        t.lookup("on-loopback").is_some(),
        "回环上的明文被拒了 —— 那把本地部署这一格整个关掉了"
    );
    assert!(
        t.lookup("tls-anywhere").is_some(),
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
