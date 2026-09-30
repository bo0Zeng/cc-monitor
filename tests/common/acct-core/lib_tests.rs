use super::*;

/// ★ 并集必须**同时**覆盖两侧此前各自独有的那些码位。
///
/// 这条是集合漂移的回归钉：任一侧的集合被"还原"回去，本测试红。
///
/// ⚠ 但要分清**哪一半是真洞**：backend 缺的那些 `is_control()` 全是 `false`，
/// 它真的会放行；monitor 缺的 NEL 属 Cc 类、`is_control()` 本来就挡着 ——
/// U7-3 我把后者也当成安全洞报了，U7-4 实测证伪。集合差过，行为没差。
#[test]
fn the_union_covers_what_each_side_used_to_miss() {
    // NEL：集合里确实缺过，但**不是安全洞**（`is_control()` 覆盖）。
    // 留在集合里是为了让本集合自足，不是因为它此前漏防了什么。
    assert!(is_deceptive_char('\u{0085}'), "NEL 从集合里掉了");
    // backend 此前**真的**漏防的（这些 `is_control()` 全是 false）
    for c in [
        '\u{2060}', '\u{2064}', '\u{1680}', '\u{2000}', '\u{200A}', '\u{202F}', '\u{205F}',
        '\u{3000}',
    ] {
        assert!(
            is_deceptive_char(c),
            "U+{:04X} —— backend 侧此前真的会放行",
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

/// 〔C4c · 第四波 4B〕两条 apikey 规则搬进来之后的行为钉（逐格照搬前 monitor `history.rs` 那一份的口径）。
///
/// 要求住址：`设计/01 §5` D1「**一个判定只有一个家**」—— 这两条规则此后由 monitor（起会话那一侧）与
/// 后端（`accounts-list` 出成品时并表）两个调用方共用，住 `acct-core` 一处。
#[test]
fn the_account_id_is_the_last_path_segment_and_the_subset_is_per_agent() {
    assert_eq!(
        apikey_account_id_of_dir("  /home/u/.claude-accts/acct-a/  ").as_deref(),
        Some("acct-a"),
        "带空白与尾斜杠的写法要落到同一个 id 上"
    );
    assert_eq!(apikey_account_id_of_dir(""), None, "说不出 id 就不表态");
    assert_eq!(apikey_account_id_of_dir("/"), None);
    let rows = vec!["acct-a".to_string()];
    let dirs = vec![
        "/home/u/.claude-accts/acct-a".to_string(),
        "/home/u/.claude-accts/acct-b".to_string(),
    ];
    assert_eq!(
        apikey_routed_subset(&dirs, &rows, "claude-code", "claude-code"),
        vec!["/home/u/.claude-accts/acct-a".to_string()],
        "表里有行的那一个没被挑出来"
    );
    assert!(
        apikey_routed_subset(&dirs, &rows, "codex", "claude-code").is_empty(),
        "别家的号不许借这张表（条 49：有行说的是 (agent, 账号) 这一对）"
    );
    assert!(apikey_routed_subset(&dirs, &[], "claude-code", "claude-code").is_empty());
}

/// 〔DUP1 · `设计/90 §3` 判据 2 · `INVARIANTS §47` ②〕账号配置目录的全表（全仓唯一一份）：POSIX 形与任一平台形，**正反各一格**。
/// 要求住址：`INVARIANTS §47` ②「本仓自管的值（配置目录 · 后端落点）……走全表」—— 形式（绝对 · 无 `..` 段）＋ 拒绝集（控制符 · 元字符 · 欺骗字符）。
#[test]
fn a_config_dir_passes_the_full_table_only_when_it_is_plainly_absolute() {
    for good in ["/home/u/.claude-accts/z", "/home/用户/带 空格/z", "/a..b/c"] {
        assert!(config_dir_posix_ok(good), "POSIX 形好值被拒了：{good:?}");
        assert!(config_dir_ok(good), "任一平台形好值被拒了：{good:?}");
    }
    for win in [
        r"C:\Users\z\.claude-accts\z",
        "C:/Users/z",
        r"\\server\share\z",
    ] {
        assert!(config_dir_ok(win), "Windows 形好值被拒了：{win:?}");
        assert!(
            !config_dir_posix_ok(win),
            "POSIX 形不该认 Windows 形：{win:?}"
        );
    }
    for bad in [
        "",
        "/",
        "rel",
        "~/x",
        "/a/../b",
        "/a/..",
        "/a'b",
        "/a$b",
        "/a;b",
        "/a`b",
        "/a b\n",
        "/a\u{3000}b",
        "/a\u{202E}b",
        "/a\u{0085}b",
    ] {
        assert!(!config_dir_posix_ok(bad), "POSIX 形坏值放行了：{bad:?}");
        assert!(!config_dir_ok(bad), "任一平台形坏值放行了：{bad:?}");
    }
    assert!(!config_dir_ok(r"C:\Users\..\..\x"), "反斜杠下的上跳也要拒");
    assert!(!config_dir_posix_ok("/a\\b"), "POSIX 形拒反斜杠");
    assert!(
        config_dir_char_unsafe('\0')
            && config_dir_char_unsafe('!')
            && !config_dir_char_unsafe('中')
    );
}

/// 〔THIN〕要求住址：`设计/15 §2.5`（`acct-core` 测试夹具没有 `cfg(test)` 门 ⇒ 编进发布二进制）· `设计/00 §1.2`「契约 crate 不带测试夹具」。
/// 三项夹具（类型 · 表 · 渲染函数）各自紧跟在 `#[cfg(any(test, feature = "fixtures"))]` 后面：摘掉一处 ⇒ 那一项又进发布二进制，本条红。
/// 正控：同一把尺子在本 crate 的一个契约项（`AUTH_KINDS`）上认得出「没有门」。
#[test]
fn the_parity_fixtures_stay_behind_the_fixtures_gate() {
    let src = include_str!("../../../src/common/acct-core/src/lib.rs");
    let gate = "#[cfg(any(test, feature = \"fixtures\"))]";
    let gated = |decl: &str| -> bool {
        let at = src
            .find(decl)
            .unwrap_or_else(|| panic!("lib.rs 里找不到 `{decl}` —— 夹具改名了，先改本条"));
        src[..at].trim_end().ends_with(gate)
    };
    for decl in [
        "pub struct AuthKindParityCase",
        "pub const AUTH_KIND_PARITY_CASES",
        "pub fn auth_kind_parity_manifest",
    ] {
        assert!(
            gated(decl),
            "`{decl}` 没在 `{gate}` 后面 —— 测试夹具又进发布二进制了"
        );
    }
    assert!(
        !gated("pub const AUTH_KINDS:"),
        "正控：契约项 `AUTH_KINDS` 也被认成有门 —— 尺子恒真"
    );
}
