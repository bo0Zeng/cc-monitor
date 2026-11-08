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

// ---- 鉴权方式这一维 ----

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
/// 否则一个手抄错的 manifest 会静默把订阅号变成「可选但断开」。
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
            "{raw:?} 被认成了 {got} —— 保守方向是订阅（缺凭据看得见），不是 api-key（断开看不见）"
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

/// 正常字符不许被误杀 —— 否则合法账号名会被拒。
#[test]
fn ordinary_characters_are_not_rejected() {
    for c in ['a', 'Z', '0', '_', '-', '.', '/', ' ', '中', 'é'] {
        assert!(!is_deceptive_char(c), "{c:?} 被误判成欺骗字符");
    }
}

/// 两条 apikey 规则搬进来之后的行为钉（逐格照搬前 monitor `history.rs` 那一份的口径）。
///
/// 要求：「**一个判定只有一个家**」—— 这两条规则此后由 monitor（起会话那一侧）与
/// 后端（`accounts-list` 出成品时并表）两个调用方共用，住 `acct-core` 一处。
#[test]
fn the_account_id_is_the_last_path_segment_and_the_subset_is_per_agent() {
    assert_eq!(
        apikey_account_id_of_dir("  /home/u/.claude-alt/acct-a/  ").as_deref(),
        Some("acct-a"),
        "带空白与尾斜杠的写法要落到同一个 id 上"
    );
    assert_eq!(apikey_account_id_of_dir(""), None, "说不出 id 就不表态");
    assert_eq!(apikey_account_id_of_dir("/"), None);
    let rows = vec!["acct-a".to_string()];
    let dirs = vec![
        "/home/u/.claude-alt/acct-a".to_string(),
        "/home/u/.claude-alt/acct-b".to_string(),
    ];
    assert_eq!(
        apikey_routed_subset(&dirs, &rows, "claude-code", "claude-code"),
        vec!["/home/u/.claude-alt/acct-a".to_string()],
        "表里有行的那一个没被挑出来"
    );
    assert!(
        apikey_routed_subset(&dirs, &rows, "codex", "claude-code").is_empty(),
        "别家的号不许借这张表（有行说的是 (agent, 账号) 这一对）"
    );
    assert!(apikey_routed_subset(&dirs, &[], "claude-code", "claude-code").is_empty());
}

/// 〔`INVARIANTS §47` ②〕账号配置目录的全表（全仓唯一一份）：POSIX 形与任一平台形，**正反各一格**。
/// 要求住址：`INVARIANTS §47` ②「本仓自管的值（配置目录 · 后端落点）……走全表」—— 形式（绝对 · 无 `..` 段）＋ 拒绝集（控制符 · 元字符 · 欺骗字符）。
#[test]
fn a_config_dir_passes_the_full_table_only_when_it_is_plainly_absolute() {
    for good in ["/home/u/.claude-alt/z", "/home/用户/带 空格/z", "/a..b/c"] {
        assert!(config_dir_posix_ok(good), "POSIX 形好值被拒了：{good:?}");
        assert!(config_dir_ok(good), "任一平台形好值被拒了：{good:?}");
    }
    for win in [
        r"C:\Users\z\.claude-alt\z",
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

fn row(name: &str, dir: Option<&str>, default: bool, auth: Option<&str>) -> ManifestAccount {
    ManifestAccount {
        name: name.into(),
        config_dir: dir.map(str::to_string),
        is_default: default,
        auth_kind: auth.map(str::to_string),
        ..Default::default()
    }
}

/// ★ 号表只有一张：换号认的路由段 · 按量号 · 按名字找，都是同一行上现算的。
/// 账号 0（没写号目录的那一条，不认名字）路由段恒是 [`BARE_ACCOUNT`]；其余是号目录末段（同 [`apikey_account_id_of_dir`]）。
#[test]
fn one_account_book_answers_routes_and_kinds() {
    let book = AccountBook {
        accounts: vec![
            row("work", Some("/h/accts/acct-w"), false, None),
            row("zero", None, false, None),
            row("key", Some("/h/accts/k1/"), true, Some(AUTH_KIND_API_KEY)),
        ],
    };
    assert_eq!(
        book.accounts
            .iter()
            .map(ManifestAccount::route_id)
            .collect::<Vec<_>>(),
        [
            Some("acct-w".into()),
            Some(BARE_ACCOUNT.into()),
            Some("k1".into())
        ]
    );
    assert_eq!(book.by_route("k1").map(|a| a.name.as_str()), Some("key"));
    assert_eq!(
        book.by_route(BARE_ACCOUNT).map(|a| a.name.as_str()),
        Some("zero")
    );
    assert!(book.by_route("nope").is_none());
    assert!(book.by_route("k1").is_some_and(ManifestAccount::is_api_key));
    assert!(!book
        .by_route("acct-w")
        .is_some_and(ManifestAccount::is_api_key));
    assert_eq!(
        book.by_name("work").and_then(|a| a.config_dir.as_deref()),
        Some("/h/accts/acct-w")
    );
    assert_eq!(
        book.named().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["work", "key"],
        "具名号 = 写了号目录的那几个，照清单顺序"
    );
}

/// ★ 换号那一侧账号 0 恒在、排第一个：清单里写了那一条就把它挪到前面，没写就补一条（名字是 [`BARE_ACCOUNT`]）。
#[test]
fn the_bare_account_is_always_first_on_the_rotation_side() {
    let without = AccountBook {
        accounts: vec![row("work", Some("/h/a/w"), false, None)],
    }
    .with_bare_first();
    assert_eq!(
        without
            .accounts
            .iter()
            .map(|a| (a.name.as_str(), a.config_dir.is_none()))
            .collect::<Vec<_>>(),
        [(BARE_ACCOUNT, true), ("work", false)]
    );
    let with = AccountBook {
        accounts: vec![
            row("work", Some("/h/a/w"), false, None),
            row("zero", None, false, None),
        ],
    }
    .with_bare_first();
    assert_eq!(
        with.accounts
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["zero", "work"],
        "清单里那一条照用，不另补一条"
    );
}
