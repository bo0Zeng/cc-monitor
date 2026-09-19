use super::*;

/// ★ 并集必须**同时**覆盖两侧此前各自独有的那些码位。
///
/// 这条是集合漂移的回归钉：任一侧的集合被"还原"回去，本测试红。
///
/// ⚠ 但要分清**哪一半是真洞**：daemon 缺的那些 `is_control()` 全是 `false`，
/// 它真的会放行；monitor 缺的 NEL 属 Cc 类、`is_control()` 本来就挡着 ——
/// U7-3 我把后者也当成安全洞报了，U7-4 实测证伪。集合差过，行为没差。
#[test]
fn the_union_covers_what_each_side_used_to_miss() {
    // NEL：集合里确实缺过，但**不是安全洞**（`is_control()` 覆盖）。
    // 留在集合里是为了让本集合自足，不是因为它此前漏防了什么。
    assert!(is_deceptive_char('\u{0085}'), "NEL 从集合里掉了");
    // daemon 此前**真的**漏防的（这些 `is_control()` 全是 false）
    for c in [
        '\u{2060}', '\u{2064}', '\u{1680}', '\u{2000}', '\u{200A}', '\u{202F}', '\u{205F}',
        '\u{3000}',
    ] {
        assert!(
            is_deceptive_char(c),
            "U+{:04X} —— daemon 侧此前真的会放行",
            c as u32
        );
    }
}

/// 两侧本来就都有的那些，一个都不能丢。
#[test]
fn the_union_keeps_everything_both_sides_already_had() {
    for c in [
        '\u{00A0}', '\u{200B}', '\u{200F}', '\u{2028}', '\u{2029}', '\u{202A}', '\u{202E}',
        '\u{2066}', '\u{2069}', '\u{FEFF}',
    ] {
        assert!(is_deceptive_char(c), "U+{:04X} 丢了", c as u32);
    }
}

/// ★ 凭据文件名必须与 `cc-acct-iso` 的 `NATIVE_IDENTITY` 声明一致。
///
/// 这是**唯一还需要守卫的那一半**：bash 侧是另一门语言，共享不了常量。
/// Claude Code 哪天改了凭据文件名，改一边漏一边的表现是**静默错** ——
/// `loggedIn` 恒 false，UI 上看不出来。
///
/// 守卫搬到这里而不是留在两个调用方：常量住在这儿，检查就该住在这儿，
/// 否则又是两份。原先 daemon 侧那条还带了「本文件真的在用这个字面量」的第二半 ——
/// 常量共享之后那半**结构上不可能不成立**（只有一个定义处），已随之删掉。
#[test]
fn the_credential_filename_matches_the_cc_acct_iso_declaration() {
    let lib_sh = include_str!("../../../../src/bridge/vendor/cc-acct-iso/scripts/lib.sh");
    assert!(
        lib_sh.len() > 1000,
        "只读到 {} 字节的 lib.sh —— include_str! 没读到，本断言在空转",
        lib_sh.len()
    );
    // 声明里那一行的精确形状：`<项名>:<原生根>:<类别>`，凭据项必须是 secret。
    let expected = format!("{CREDENTIALS_NAME}:cfg:secret");
    assert!(
        lib_sh.contains(&expected),
        "Z06 双写点漂移：cc-acct-iso 的 NATIVE_IDENTITY 声明里找不到 {expected:?}。\n\
             两侧判「已登录」用的都是 {CREDENTIALS_NAME:?}（本 crate 的常量），\n\
             Claude Code 改了凭据文件名就要**同时**改这里与 bash 声明。"
    );
}

// ---- K-A1：鉴权方式这一维 ----

/// ★ **金样与规则互钉。** 改了 [`auth_ready`] 而没改金样、或反过来，这条红。
///
/// 金样里的 `expect_auth_ready` 是**手写字面量**（不是算出来的），所以本条不是恒真：
/// 把 `auth_ready` 的 api-key 那支改成 `credentials_present`，本条当场红两格。
#[test]
fn the_golden_matches_the_rule() {
    assert_eq!(
        AUTH_KIND_PARITY_CASES.len(),
        6,
        "金样格数变了 —— 先确认新格是不是也纳进了两侧的对拍"
    );
    for c in &AUTH_KIND_PARITY_CASES {
        assert_eq!(
            auth_kind_from_manifest(c.manifest_auth_kind),
            c.expect_auth_kind,
            "{}：分类规则与金样不一致",
            c.name
        );
        assert_eq!(
            auth_ready(c.expect_auth_kind, c.credentials_present),
            c.expect_auth_ready,
            "{}：就绪规则与金样不一致",
            c.name
        );
    }
}

/// 闭集里每一个字面量都要能被分类函数认出来 —— 否则「闭集」是装饰。
#[test]
fn every_declared_auth_kind_round_trips() {
    assert_eq!(
        AUTH_KINDS.len(),
        2,
        "加了第三档 ⇒ 先给它一条 auth_ready 规则"
    );
    for k in AUTH_KINDS {
        assert_eq!(
            auth_kind_from_manifest(Some(k)),
            k,
            "{k} 在闭集里却分类不出来"
        );
    }
}

/// ★ 缺席与认不出**都**落订阅，而且**不许**落 api-key。
///
/// 反向也断一次：只有逐字 `api-key` 才是 api-key（大小写/空格/近似写法一律不算），
/// 否则一个手抄错的 manifest 会静默把订阅号变成「可选但连不上」。
#[test]
fn unknown_and_absent_auth_kinds_fall_back_to_subscription_not_api_key() {
    for raw in [
        None,
        Some(""),
        Some("api_key"),
        Some("API-KEY"),
        Some(" api-key"),
        Some("apikey"),
        Some("bedrock"),
    ] {
        let got = auth_kind_from_manifest(raw);
        assert_eq!(
            got, AUTH_KIND_SUBSCRIPTION,
            "{raw:?} 被认成了 {got} —— 保守方向是订阅（缺凭据看得见），不是 api-key（连不上看不见）"
        );
    }
    assert_eq!(
        auth_kind_from_manifest(Some(AUTH_KIND_API_KEY)),
        AUTH_KIND_API_KEY,
        "逐字 api-key 反而没被认出来 —— 上面那批断言就成了空真"
    );
}

/// 订阅那一支**逐字节旧行为**：就绪 == 凭据文件在。
#[test]
fn subscription_readiness_is_still_exactly_the_credential_file() {
    assert!(auth_ready(AUTH_KIND_SUBSCRIPTION, true));
    assert!(!auth_ready(AUTH_KIND_SUBSCRIPTION, false));
    // api-key 那一支不看那个文件 —— 两种取值都真。
    assert!(auth_ready(AUTH_KIND_API_KEY, false));
    assert!(auth_ready(AUTH_KIND_API_KEY, true));
}

/// 夹具渲染器：路径真的被替换进去了，且 `manifest_auth_kind: None` 那格**没有**这个键
/// （不是写成 `"authKind":null` —— 缺席与 null 在 serde 上是两件事）。
#[test]
fn the_parity_manifest_renders_absent_keys_as_absent() {
    let m = auth_kind_parity_manifest("/tmp/fixture-root");
    assert!(m.starts_with("{\"version\":1,"), "schema 版本得是 1：{m}");
    assert!(
        m.contains("\"configDir\":\"/tmp/fixture-root/sub-cred\""),
        "{m}"
    );
    assert!(
        m.contains("\"sharedStore\":\"/tmp/fixture-root/shared\""),
        "{m}"
    );
    assert!(!m.contains("null"), "缺席的键不许渲染成 null：{m}");
    // 六格里有五格带 authKind（`legacy-nokind` 那格不带）。
    assert_eq!(m.matches("\"authKind\"").count(), 5, "{m}");
    // 认不出的那格照原样写进去 —— 分类规则由读侧负责，夹具不许提前替它决定。
    assert!(m.contains("\"authKind\":\"oauth-device\""), "{m}");
}

/// 正常字符不许被误杀 —— 否则合法账号名会被拒。
#[test]
fn ordinary_characters_are_not_rejected() {
    for c in ['a', 'Z', '0', '_', '-', '.', '/', ' ', '中', 'é'] {
        assert!(!is_deceptive_char(c), "{c:?} 被误判成欺骗字符");
    }
}
