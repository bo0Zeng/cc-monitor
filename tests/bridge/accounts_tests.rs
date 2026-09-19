use super::*;

#[test]
fn parses_meta_and_accounts() {
    let lines: Vec<String> = vec![
        r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/h/.claude-accts","manifestPath":"/h/.claude-accts/accounts.json","updatedAt":"2026-07-23T18:00:00Z","sharedStore":"/h/.claude","count":2,"error":null}"#.into(),
        r#"{"name":"z","email":"z@x.edu","configDir":"/h/.claude-accts/z","isDefault":true,"mode":"isolated","exists":true,"loggedIn":true}"#.into(),
        r#"{"name":"b","email":"","configDir":"/h/.claude-accts/b","isDefault":false,"mode":"isolated","exists":true,"loggedIn":false}"#.into(),
    ];
    let (meta, accts) = parse_accounts_lines(&lines);
    let m = meta.expect("meta 应解析出来");
    assert!(m.enabled);
    assert_eq!(m.count, 2);
    assert_eq!(m.shared_store.as_deref(), Some("/h/.claude"));
    assert_eq!(accts.len(), 2);
    assert_eq!(accts[0].name, "z");
    assert!(accts[0].is_default);
    assert!(accts[0].logged_in);
    assert!(!accts[1].logged_in);
    assert_eq!(accts[1].email, "");
}

#[test]
fn parses_disabled_meta_with_reason() {
    let lines: Vec<String> = vec![
        r#"{"kind":"accounts-meta","enabled":false,"acctsDir":"/h/.claude-accts","manifestPath":"/h/.claude-accts/accounts.json","updatedAt":null,"sharedStore":null,"count":0,"error":"manifest 不可读"}"#.into(),
    ];
    let (meta, accts) = parse_accounts_lines(&lines);
    let m = meta.expect("meta 应解析出来");
    assert!(!m.enabled);
    assert_eq!(m.error.as_deref(), Some("manifest 不可读"));
    assert!(accts.is_empty());
}

#[test]
fn skips_unparsable_lines_instead_of_failing() {
    let lines: Vec<String> = vec![
        r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":1,"error":null}"#.into(),
        "not json at all".into(),
        r#"{"kind":"some-future-kind","x":1}"#.into(),
        r#"{"name":"z","configDir":"/a/z"}"#.into(),
    ];
    let (meta, accts) = parse_accounts_lines(&lines);
    assert!(meta.is_some());
    assert_eq!(accts.len(), 1, "未知 kind 与坏行应被跳过而非整体失败");
    assert_eq!(accts[0].name, "z");
    assert!(!accts[0].logged_in, "缺字段走 default");
}

#[test]
fn session_account_row_parses() {
    let row: SessionAccount = serde_json::from_str(
        r#"{"pid":66936,"sessionId":"9d66c46d","cwd":"/w","configDir":null,"account":null,"bare":true,"alive":true}"#,
    )
    .unwrap();
    assert_eq!(row.pid, 66936);
    assert!(row.bare);
    assert!(row.alive);
    assert!(row.account.is_none());
    // ★ `K-P5f` `KP5FD4` 的 **additive 那一半**：上面这一行是**老 daemon 的出参**
    // （逐字节没有 `launchId` 键）。它必须照样解析成功、读成 `None`。
    // 生产上翻掉它的症状是**整台远端的会话账号映射一条都不剩**（每行解析失败被 warn
    // 跳过），徽章整片消失且没人说得出为什么。
    // ⚠ **翻掉它的那一刀不是删 `#[serde(default)]`**（`Option<T>` 缺键 serde 本来就给
    // `None`，实打过：删掉本条照样绿）——是**加一个非 `Option` 又没有 `default` 的字段**
    // （实打：临时加 `pub probe_required: bool` ⇒ 本条与下一条当场红
    // `missing field \`probeRequired\``）。这一格记在 `SessionAccount::launch_id` 头注里。
    assert!(
        row.launch_id.is_none(),
        "老 daemon 的行缺 launchId 应读成 None，不是报错"
    );
}

/// `K-P5f`：新 daemon 那一侧 —— `launchId` 有值时逐字带回来，`null` 读成 `None`。
///
/// ⚠ 这里刻意**不**判「什么时候该是 null」：那是 daemon 侧
/// `accounts_query::suppress_inherited_launch_ids` 与它的活体夹具的活。
/// 本条只买「这条线在 monitor 侧接得住」——一个性质两个量法就是本区最贵那族病。
#[test]
fn session_account_row_carries_the_launch_identity() {
    let with: SessionAccount = serde_json::from_str(
        r#"{"pid":1,"sessionId":"s","cwd":"/w","configDir":null,"account":null,"bare":true,"alive":true,"launchId":"tok-1"}"#,
    )
    .unwrap();
    assert_eq!(with.launch_id.as_deref(), Some("tok-1"));
    let nulled: SessionAccount = serde_json::from_str(
        r#"{"pid":1,"sessionId":"s","cwd":"/w","configDir":null,"account":null,"bare":true,"alive":true,"launchId":null}"#,
    )
    .unwrap();
    assert!(
        nulled.launch_id.is_none(),
        "daemon 判「不作数」时发的是 null，monitor 侧必须读成 None"
    );
}

// ---- Z01：账号 0（configDir 缺席）----

const META_AWARE: &str = r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/h/.claude-accts","manifestPath":"/h/.claude-accts/accounts.json","updatedAt":null,"sharedStore":"/h/.claude","count":2,"error":null,"accountZeroAware":true}"#;
const ACCT_Z: &str =
    r#"{"name":"z","configDir":"/h/.claude-accts/z","mode":"isolated","exists":true}"#;
const ACCT_ZERO: &str = r#"{"name":"0","email":"me@x.edu","mode":"bare","exists":true,"loggedIn":true,"configDir":null}"#;

#[test]
fn account_zero_parses_with_null_config_dir() {
    let lines: Vec<String> = vec![META_AWARE.into(), ACCT_Z.into(), ACCT_ZERO.into()];
    let (meta, accts) = parse_accounts_lines(&lines);
    let m = meta.unwrap();
    assert!(m.account_zero_aware);
    assert_eq!(accts.len(), 2, "账号 0 必须解析出来，不能被当坏行跳过");
    let zero = &accts[1];
    assert_eq!(zero.name, "0");
    assert!(zero.config_dir.is_none(), "必须是 None，不是 Some(\"\")");
    assert_eq!(zero.mode, "bare");
    assert!(zero.logged_in);
    assert!(
        degraded_notice(&m, &accts).is_none(),
        "一切正常时不该有 notice"
    );
}

/// configDir 这个键**整个不出现**（而不是显式 null）也一样。
/// bash 侧写的就是这种形状——两种都得认。
#[test]
fn account_zero_parses_with_absent_config_dir_key() {
    let lines: Vec<String> = vec![
        META_AWARE.into(),
        r#"{"name":"0","mode":"bare","exists":true}"#.into(),
    ];
    let (_, accts) = parse_accounts_lines(&lines);
    assert_eq!(accts.len(), 1);
    assert!(accts[0].config_dir.is_none());
}

/// 旧 daemon：不出 `accountZeroAware` ⇒ 必须**明说**它会少一行，绝不静默。
#[test]
fn old_daemon_gets_an_explicit_notice() {
    let lines: Vec<String> = vec![
        r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":1,"error":null}"#.into(),
        ACCT_Z.into(),
    ];
    let (meta, accts) = parse_accounts_lines(&lines);
    let m = meta.unwrap();
    assert!(!m.account_zero_aware, "缺键 ⇒ default false");
    let n = degraded_notice(&m, &accts).expect("必须给出人话说明");
    assert!(n.contains("daemon"), "要指明是 daemon 旧：{n}");
}

/// 新 daemon + 旧 cc-acct-iso：manifest 里根本没有账号 0 ⇒ 也要明说，
/// 且要指向**另一个**动作（跑 sync / 重新部署 cc-acct-iso），不是更新 daemon。
#[test]
fn old_cc_acct_iso_gets_a_different_notice() {
    let lines: Vec<String> = vec![META_AWARE.into(), ACCT_Z.into()];
    let (meta, accts) = parse_accounts_lines(&lines);
    let m = meta.unwrap();
    let n = degraded_notice(&m, &accts).expect("必须给出人话说明");
    assert!(n.contains("cc-acct-iso"), "要指明是 cc-acct-iso 旧：{n}");
    assert!(
        !n.contains("更新远端 daemon"),
        "别把用户指向错误的动作：{n}"
    );
}

/// 没启用多账号时不该冒出「缺账号 0」的噪音。
#[test]
fn disabled_manifest_has_no_account_zero_notice() {
    let lines: Vec<String> = vec![
        r#"{"kind":"accounts-meta","enabled":false,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":0,"error":"没有 manifest"}"#.into(),
    ];
    let (meta, accts) = parse_accounts_lines(&lines);
    assert!(degraded_notice(&meta.unwrap(), &accts).is_none());
}

/// ★ 账号 0 的 trust 查询**不传路径**——传空串会被 daemon 判不安全路径拒掉，
/// 用户看到一句莫名其妙的错。这条钉住命令行拼法。
#[test]
fn trust_args_for_account_zero_passes_no_path() {
    let a = trust_args(None, "/w/proj");
    assert_eq!(a, "--account-trust-zero '/w/proj'");
    assert!(!a.contains("''"), "绝不能出现空串路径参数：{a}");
    let b = trust_args(Some("/h/.claude-accts/z"), "/w/proj");
    assert_eq!(b, "--account-trust '/h/.claude-accts/z' '/w/proj'");
}

/// cwd 仍然要被 posix 引用（账号 0 这条新路径不能把注入面漏出来）。
#[test]
fn trust_args_quotes_cwd_on_the_account_zero_path() {
    let a = trust_args(None, "/w/it's here; rm -rf /");
    assert!(!a.contains("; rm -rf /'") || a.starts_with("--account-trust-zero '"));
    assert_eq!(
        a,
        format!(
            "--account-trust-zero {}",
            ssh_source::shell_quote("/w/it's here; rm -rf /")
        )
    );
}

// ---- K-A1：鉴权方式这一维（这一侧是**中继**，不是生产者） ----

/// ★ 枚举的 **serde 名**必须逐字等于 `acct-core` 的那两个契约常量。
///
/// 这是本枚举与共享 crate 之间**唯一**的双写点（`kebab-case` 是 derive 算出来的，
/// 不是我写的字面量）⇒ 改了任一侧，本条红。生成物 `src/generated/AuthKind.ts`
/// 也是从这两个名字来的，所以 TS 那个字面量联合一并被钉住。
#[test]
fn the_wire_names_match_the_shared_contract() {
    assert_eq!(
        serde_json::to_string(&AuthKind::Subscription).unwrap(),
        format!("\"{}\"", acct_core::AUTH_KIND_SUBSCRIPTION)
    );
    assert_eq!(
        serde_json::to_string(&AuthKind::ApiKey).unwrap(),
        format!("\"{}\"", acct_core::AUTH_KIND_API_KEY)
    );
    // 反向：契约里的每个字面量都要能反序列化回来（闭集两头都得通）。
    for k in acct_core::AUTH_KINDS {
        let v: AuthKind = serde_json::from_str(&format!("\"{k}\"")).unwrap();
        assert_eq!(v.as_contract_str(), k);
    }
    // 默认档是订阅（`KA6d`：缺席 ⇒ 订阅，不是 api-key）。
    assert_eq!(AuthKind::default(), AuthKind::Subscription);
    assert_eq!(AuthKind::from_manifest(None), AuthKind::Subscription);
    assert_eq!(
        AuthKind::from_manifest(Some(acct_core::AUTH_KIND_API_KEY)),
        AuthKind::ApiKey
    );
}

/// ★ **这一侧是中继，不是第三个生产者。**
///
/// 件计划 §0 把它写成「三个生产者」，Bx 实测订正：`parse_accounts_lines` 一个字段都不算
/// （通体只有一句 `from_value::<RemoteAccount>`）。⇒ 它该断的性质不是「产出与另两家相同」
/// （那会写成一条循环自证），而是**原样透传**：daemon 说什么就是什么，一个字都不改。
#[test]
fn the_relay_passes_the_auth_dimension_through_verbatim() {
    let lines: Vec<String> = vec![
        r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":2,"error":null,"accountZeroAware":true}"#.into(),
        r#"{"name":"sub","configDir":"/a/sub","mode":"isolated","exists":true,"loggedIn":true,"authKind":"subscription","authReady":true}"#.into(),
        r#"{"name":"api","configDir":"/a/api","mode":"isolated","exists":true,"loggedIn":false,"authKind":"api-key","authReady":true}"#.into(),
    ];
    let (_, accts) = parse_accounts_lines(&lines);
    assert_eq!(accts.len(), 2);
    assert_eq!(accts[0].auth_kind, Some(AuthKind::Subscription));
    assert_eq!(accts[0].auth_ready, Some(true));
    // ★ KAY2 在中继这一侧的那一格：api-key 号 `loggedIn:false` 但 `authReady:true`，
    // 中继不许「顺手修正」成 false。
    assert_eq!(accts[1].auth_kind, Some(AuthKind::ApiKey));
    assert_eq!(accts[1].auth_ready, Some(true));
    assert!(
        !accts[1].logged_in,
        "loggedIn 也得原样透传，不许被 authReady 带着改"
    );
}

/// ★ **旧 daemon（两个键都不出）⇒ 中继必须回 `None`，不许悄悄编一个值出来。**
///
/// 这一格是整条降级链的起点：`None` 才让前端能**回落到逐字节旧行为**
/// （`authReady ?? loggedIn`）。若这里 `#[serde(default)]` 被改成
/// 「缺键 ⇒ `Some(Subscription)` / `Some(false)`」，前端就分不出
/// 「对面说它没就绪」和「对面压根没说」——而这两件事该走不同的路。
#[test]
fn an_old_daemon_yields_none_not_a_made_up_value() {
    let lines: Vec<String> = vec![
        r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":1,"error":null}"#.into(),
        r#"{"name":"z","configDir":"/a/z","mode":"isolated","exists":true,"loggedIn":true}"#.into(),
    ];
    let (_, accts) = parse_accounts_lines(&lines);
    assert_eq!(accts.len(), 1);
    assert_eq!(accts[0].auth_kind, None, "缺键必须是 None（「对面没说」）");
    assert_eq!(accts[0].auth_ready, None);
    // 而且**序列化回前端时那两个键也不许出现**（`skip_serializing_if`）——
    // 出一个 `null` 会让 TS 那个 `?? ` 回落判据从「缺席」变成「显式空」。
    let json = serde_json::to_string(&accts[0]).unwrap();
    assert!(!json.contains("authKind"), "缺席被序列化出来了：{json}");
    assert!(!json.contains("authReady"), "缺席被序列化出来了：{json}");
    assert!(
        json.contains("\"loggedIn\":true"),
        "别的字段不该跟着掉：{json}"
    );
}

/// 认不出的 `authKind` 值（写侧比读侧新）⇒ 整行**不许**被丢掉。
///
/// `serde` 对未知 enum variant 是**硬错**，而 `parse_accounts_lines` 的策略是「坏行跳过」
/// ⇒ 若不当心，一个 `"authKind":"bedrock"` 会让那个账号**整行消失**（列表静默少一行，
/// 比判错 kind 更坏）。本条钉住实际行为，别让它变成静默丢账号。
#[test]
fn an_unrecognized_auth_kind_does_not_silently_drop_the_account() {
    let lines: Vec<String> = vec![
        r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":2,"error":null}"#.into(),
        r#"{"name":"z","configDir":"/a/z","mode":"isolated","exists":true,"loggedIn":true,"authKind":"bedrock","authReady":true}"#.into(),
        r#"{"name":"b","configDir":"/a/b","mode":"isolated","exists":true,"loggedIn":true}"#.into(),
    ];
    let (_, accts) = parse_accounts_lines(&lines);
    assert_eq!(
        accts.len(),
        2,
        "认不出的 authKind 让整个账号消失了 —— 静默少一行比判错 kind 更坏"
    );
    assert_eq!(accts[0].name, "z");
    assert_eq!(
        accts[0].auth_kind, None,
        "认不出的值该退化成「对面没说」（⇒ 前端当订阅、保留缺凭据保护），不是当 api-key"
    );
}

/// ★ `authReady` 那一格**同职同治**（自审补的）：形状不对也不许丢账号。
///
/// 与上一条是同一个失效模式（serde 硬错 + 「坏行跳过」= 静默少一行）。
/// 先只治撞到的那一处、放着同职的另一处，正是本仓反复栽的那个形。
#[test]
fn an_unrecognized_auth_ready_shape_does_not_silently_drop_the_account() {
    let lines: Vec<String> = vec![
        r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":2,"error":null}"#.into(),
        r#"{"name":"z","configDir":"/a/z","mode":"isolated","exists":true,"loggedIn":true,"authKind":"api-key","authReady":"partial"}"#.into(),
        r#"{"name":"b","configDir":"/a/b","mode":"isolated","exists":true,"loggedIn":true}"#.into(),
    ];
    let (_, accts) = parse_accounts_lines(&lines);
    assert_eq!(accts.len(), 2, "authReady 形状不对让整个账号消失了");
    assert_eq!(
        accts[0].auth_kind,
        Some(AuthKind::ApiKey),
        "kind 该照样透传"
    );
    assert_eq!(
        accts[0].auth_ready, None,
        "形状不对 ⇒「对面没说」⇒ 前端回落 loggedIn，而不是硬错丢行"
    );
    // 正常 bool 仍然要认出来 —— 否则上面那条是空真。
    let ok: Vec<String> = vec![
        r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/x.json","updatedAt":null,"sharedStore":null,"count":1,"error":null}"#.into(),
        r#"{"name":"z","configDir":"/a/z","mode":"isolated","exists":true,"loggedIn":false,"authKind":"api-key","authReady":true}"#.into(),
    ];
    let (_, ok_accts) = parse_accounts_lines(&ok);
    assert_eq!(ok_accts[0].auth_ready, Some(true));
}

#[test]
fn unavailable_helper_sets_flag_and_reason() {
    let r: AccountsResult = unavailable("daemon 太旧");
    assert!(!r.available);
    assert_eq!(r.error.as_deref(), Some("daemon 太旧"));
    assert!(r.accounts.is_empty());
    assert!(r.meta.is_none());
    let s: SessionAccountsResult = unavailable("x");
    assert!(!s.available);
    let t: AccountTrustResult = unavailable("y");
    assert!(!t.available);
    assert!(!t.trusted, "不可用时不得默认判为已信任");
}
