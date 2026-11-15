//! 核对是纯的：同一份手搭的快照，改一格就该多一条致命。
use super::*;
use crate::accounts::manage::scan::{DirState, Roots};
use std::collections::BTreeMap;

fn snap(z: BTreeMap<String, Item>) -> Snapshot {
    let text = r#"{"version":1,"sharedStore":"/h/.claude","accounts":[{"name":"z","email":"","configDir":"/h/.cc-monitor/accounts/z","isDefault":true}]}"#;
    let mut dirs = BTreeMap::new();
    dirs.insert(
        "/h/.cc-monitor/accounts/z".to_string(),
        DirState {
            me: Some(Item::Dir { mode: Some(0o700) }),
            entries: z,
        },
    );
    let mut shared = BTreeMap::new();
    for n in ["skills", "agents", "plans", "projects", "sessions"] {
        shared.insert(n.to_string(), Item::Dir { mode: Some(0o755) });
    }
    Snapshot {
        roots: Some(Roots {
            home: "/h".into(),
            shared: "/h/.claude".into(),
            accts: "/h/.cc-monitor/accounts".into(),
        }),
        manifest: Some(super::super::model::Manifest::parse(text)),
        shared: DirState {
            me: Some(Item::Dir { mode: Some(0o700) }),
            entries: shared,
        },
        dirs,
        ..Snapshot::default()
    }
}

fn fails(r: &VerifyReport) -> Vec<&str> {
    r.checks
        .iter()
        .filter(|c| c.level == CheckLevel::Fail)
        .map(|c| c.text.as_str())
        .collect()
}

/// 号 z 里链好共享库那几项（[`snap`] 的共享库有的那几样）。
fn all_linked() -> BTreeMap<String, Item> {
    ["skills", "agents", "plans", "projects", "sessions"]
        .iter()
        .map(|n| {
            (
                n.to_string(),
                Item::Link {
                    target: format!("/h/.claude/{n}"),
                    dangling: false,
                },
            )
        })
        .collect()
}

#[test]
fn a_correct_link_passes_and_a_wrong_one_fails() {
    let ok = all_linked();
    let good = verify(&snap(ok), &[]);
    assert!(good.pass, "{:?}", good.checks);
    let mut bad = all_linked();
    bad.insert(
        "skills".to_string(),
        Item::Link {
            target: "/tmp/x".into(),
            dangling: false,
        },
    );
    let r = verify(&snap(bad), &[]);
    assert!(!r.pass);
    assert_eq!(r.fails, 1);
    assert!(fails(&r)[0].contains("/tmp/x"), "{:?}", fails(&r));
}

#[test]
fn no_manifest_is_one_fail_that_says_where() {
    let mut s = snap(BTreeMap::new());
    s.manifest = None;
    let r = verify(&s, &[]);
    assert_eq!(r.fails, 1);
    assert!(fails(&r)[0].contains("/h/.cc-monitor/accounts/accounts.json"));
}

#[test]
fn an_unknown_name_to_verify_is_a_fail_not_a_pass() {
    let ok = all_linked();
    assert!(!verify(&snap(ok), &["nope".into()]).pass);
}

/// 一个号目录的顶层结构（只有名字与类型，几个号的并集）：`l` 链回共享库 · `f` 文件 · `d` 目录。
const ACCOUNT_DIR_SHAPE: &[(&str, char)] = &[
    (".claude.json", 'f'),
    (".credentials.json", 'f'),
    (".last-cleanup", 'f'),
    (".last-update-result.json", 'f'),
    ("CLAUDE.md", 'l'),
    ("agents", 'l'),
    ("backups", 'd'),
    ("cache", 'l'),
    ("downloads", 'l'),
    ("feedback", 'd'),
    ("file-history", 'l'),
    ("history.jsonl", 'l'),
    ("jobs", 'l'),
    ("mcp-needs-auth-cache.json", 'f'),
    ("paste-cache", 'l'),
    ("plans", 'l'),
    ("plugins", 'l'),
    ("policy-limits.json", 'f'),
    ("policy-limits.json.stamp.json", 'f'),
    ("projects", 'l'),
    ("remote-settings.json", 'f'),
    ("remote-settings.json.synced-at", 'f'),
    ("session-env", 'l'),
    ("sessions", 'l'),
    ("settings.json", 'l'),
    ("shell-snapshots", 'l'),
    ("skills", 'l'),
    ("stats-cache.json", 'f'),
    ("state", 'd'),
    ("telemetry", 'd'),
];

fn own_file(mode: u32) -> Item {
    Item::File {
        mode: Some(mode),
        size: 2,
        mtime: None,
    }
}

/// 共享库里有每一项链接的源头，**也有**每一份非身份本体的每号文件（共享库自己那份，最容易被当成「该共享」的那一形）；
/// 号 z 的目录照 [`ACCOUNT_DIR_SHAPE`] 摆（`extra` 再多放几份自己的文件，共享库里也放一份）。
fn shaped(extra: &[&str]) -> Snapshot {
    let mut s = snap(BTreeMap::new());
    let (mut shared, mut z) = (BTreeMap::new(), BTreeMap::new());
    let secret = |n: &str| {
        super::super::layout::identity()
            .iter()
            .any(|(i, _, sec)| *i == n && *sec)
    };
    for (name, kind) in ACCOUNT_DIR_SHAPE {
        let it = match kind {
            'l' => Item::Link {
                target: format!("/h/.claude/{name}"),
                dangling: false,
            },
            'd' => Item::Dir { mode: Some(0o700) },
            _ => own_file(0o600),
        };
        if *kind != 'l' && secret(name) {
            z.insert(name.to_string(), it);
            continue;
        }
        let src = if *kind == 'd' || *kind == 'l' {
            Item::Dir { mode: Some(0o755) }
        } else {
            own_file(0o644)
        };
        shared.insert(name.to_string(), src);
        z.insert(name.to_string(), it);
    }
    for e in extra {
        shared.insert(e.to_string(), own_file(0o644));
        z.insert(e.to_string(), own_file(0o600));
    }
    s.shared.entries = shared;
    s.dirs.get_mut("/h/.cc-monitor/accounts/z").unwrap().entries = z;
    s
}

#[test]
fn every_per_account_file_in_an_account_dir_is_on_the_list_and_never_called_a_missing_link() {
    // ① 每号清单 == 号目录里不是链接的那几项（两向）。
    let mut want: Vec<&str> = ACCOUNT_DIR_SHAPE
        .iter()
        .filter(|(_, k)| *k != 'l')
        .map(|(n, _)| *n)
        .collect();
    want.sort_unstable();
    let mut got: Vec<&str> = super::super::layout::identity()
        .iter()
        .map(|(n, _, _)| *n)
        .collect();
    got.sort_unstable();
    assert_eq!(got, want);

    // ② 共享库里也有一份时，核对不说「应当是共享链接」，也不提示别的；正控：一项已知共享的在号里是自己的实体 ⇒ 恰好那一条致命。
    let r = verify(&shaped(&[]), &[]);
    let loud: Vec<&VerifyCheck> = r
        .checks
        .iter()
        .filter(|c| matches!(c.level, CheckLevel::Fail | CheckLevel::Warn))
        .collect();
    assert!(loud.is_empty(), "{loud:?}");
    let mut own = shaped(&[]);
    own.dirs
        .get_mut("/h/.cc-monitor/accounts/z")
        .unwrap()
        .entries
        .insert("history.jsonl".into(), own_file(0o600));
    let r = verify(&own, &[]);
    assert_eq!(r.fails, 1, "{:?}", r.checks);
    assert!(fails(&r)[0].contains("history.jsonl"), "{:?}", fails(&r));

    // ③ 修复只补链接那几项：号目录空着时，计划里新建的链接 == 形状里的链接（每号那几份一份都不链）。
    let mut empty = shaped(&[]);
    empty
        .dirs
        .get_mut("/h/.cc-monitor/accounts/z")
        .unwrap()
        .entries
        .clear();
    let plan = crate::accounts::manage::layout::plan_repair(&empty).unwrap();
    let mut linked: Vec<String> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            crate::accounts::manage::layout::Op::Link { at, .. } => {
                at.rsplit('/').next().map(str::to_string)
            }
            _ => None,
        })
        .collect();
    linked.sort_unstable();
    let mut links: Vec<String> = ACCOUNT_DIR_SHAPE
        .iter()
        .filter(|(_, k)| *k == 'l')
        .map(|(n, _)| n.to_string())
        .collect();
    links.sort_unstable();
    assert_eq!(linked, links);
}

/// 远端托管设置的「上次同步时刻」跟着号走：共享库里有一份、号里没有 ⇒ 不报缺共享链接；
/// 号里那份被链回共享库 ⇒ 跟别的身份文件被链错一样报致命，点名是它。
#[test]
fn the_remote_settings_sync_stamp_is_per_account_not_a_missing_link() {
    const SYNCED_AT: &str = "remote-settings.json.synced-at";
    let mut s = shaped(&[]);
    s.shared
        .entries
        .insert(SYNCED_AT.to_string(), own_file(0o644));
    let z = "/h/.cc-monitor/accounts/z";
    s.dirs.get_mut(z).unwrap().entries.remove(SYNCED_AT);
    let r = verify(&s, &[]);
    let loud: Vec<&VerifyCheck> = r
        .checks
        .iter()
        .filter(|c| matches!(c.level, CheckLevel::Fail | CheckLevel::Warn))
        .collect();
    assert!(loud.is_empty(), "{loud:?}");

    s.dirs.get_mut(z).unwrap().entries.insert(
        SYNCED_AT.to_string(),
        Item::Link {
            target: format!("/h/.claude/{SYNCED_AT}"),
            dangling: false,
        },
    );
    let r = verify(&s, &[]);
    assert_eq!(r.fails, 1, "{:?}", r.checks);
    assert!(fails(&r)[0].contains(SYNCED_AT), "{:?}", fails(&r));
}

/// 按需才建的共用目录在号里是自己的一份（起来时那一步不替人合并的那一形）：不报致命，一条提示点名是哪一项；
/// 别的共享项在号里是实体照旧致命。
#[test]
fn an_own_copy_of_an_on_demand_shared_dir_is_a_note_not_a_fail() {
    let mut z = all_linked();
    z.insert("agents".into(), Item::Dir { mode: Some(0o700) });
    let r = verify(&snap(z), &[]);
    assert!(r.pass, "{:?}", r.checks);
    let warns: Vec<&str> = r
        .checks
        .iter()
        .filter(|c| c.level == CheckLevel::Warn && c.text.contains("agents"))
        .map(|c| c.text.as_str())
        .collect();
    assert_eq!(warns.len(), 1, "{:?}", r.checks);
    assert!(warns[0].contains('z'), "{warns:?}");
    let mut z = all_linked();
    z.insert("skills".into(), Item::Dir { mode: Some(0o700) });
    assert!(!verify(&snap(z), &[]).pass);
}

fn warns(r: &VerifyReport) -> Vec<&str> {
    r.checks
        .iter()
        .filter(|c| c.level == CheckLevel::Warn)
        .map(|c| c.text.as_str())
        .collect()
}

/// 认不得名字的（号里的实体 `bridge-spawn` · 原子写的旁名 · 共享库里 `0600` 的文件）不出 Warn：全报告只有一条 `Ok` 级的
/// 「没归类」，列出共享库顶层与各号里认不得的名字（去重、排序；号里的链接不列）。
#[test]
fn unknown_names_are_one_quiet_unclassified_line_not_warnings() {
    let own = Item::File {
        mode: Some(0o600),
        size: 2,
        mtime: None,
    };
    let mut z = all_linked();
    z.insert("bridge-spawn".into(), Item::Dir { mode: Some(0o700) });
    z.insert("settings.json.ccm-backup-1-0".into(), own.clone());
    z.insert(
        "elsewhere".into(),
        Item::Link {
            target: "/x/elsewhere".into(),
            dangling: false,
        },
    );
    let mut s = snap(z);
    s.shared.entries.insert("x.json".into(), own);
    s.shared
        .entries
        .insert("bridge-spawn".into(), Item::Dir { mode: Some(0o700) });
    let r = verify(&s, &[]);
    assert!(r.pass, "{:?}", r.checks);
    let names = ["bridge-spawn", "settings.json.ccm-backup-1-0", "x.json"];
    let named: Vec<&str> = warns(&r)
        .into_iter()
        .filter(|w| names.iter().any(|n| w.contains(n)))
        .collect();
    assert!(named.is_empty(), "{named:?}");
    let want = copy_text(
        "beAcctVerify.global.unclassified",
        &[
            ("n", &names.len().to_string()),
            ("items", &names.join(&copy_text("kit.text.sep", &[]))),
        ],
    );
    let said: Vec<&VerifyCheck> = r.checks.iter().filter(|c| c.text == want).collect();
    assert!(
        said.len() == 1 && said[0].level == CheckLevel::Ok && said[0].account.is_none(),
        "{:?}",
        r.checks
    );
}

/// 核对条那一句由后端写好（`headline`）：第一条对不上的那句（带号名 ⇒ 「号：…」，全局的 ⇒ 「账号库：…」）
/// 再接「另 ×N」；一条对不上的都没有 ⇒ 缺。前端照摆，不数、不挑、不拼。
#[test]
fn the_bar_sentence_is_written_here_first_fail_plus_how_many_more() {
    let line = |account: Option<&str>, level: CheckLevel, text: &str| VerifyCheck {
        level,
        account: account.map(str::to_string),
        text: text.to_string(),
    };
    let r = finish(Checks {
        checks: vec![
            line(None, CheckLevel::Warn, "先一条提示"),
            line(Some("z"), CheckLevel::Fail, "身份文件是链接"),
            line(None, CheckLevel::Fail, "共享库里留着身份"),
            line(Some("y"), CheckLevel::Fail, "权限不对"),
        ],
    });
    let first = copy_text(
        "acctPage.verify.bar",
        &[("name", "z"), ("what", "身份文件是链接")],
    );
    let more = copy_text("kit.dialog.more", &[("n", "2")]);
    let sep = copy_text("kit.text.sep", &[]);
    assert_eq!(
        r.headline.as_deref(),
        Some(format!("{first}{sep}{more}").as_str())
    );

    let global = finish(Checks {
        checks: vec![line(None, CheckLevel::Fail, "共享库里留着身份")],
    });
    assert_eq!(
        global.headline.as_deref(),
        Some(copy_text("acctPage.verify.barGlobal", &[("what", "共享库里留着身份")]).as_str()),
        "只一条 ⇒ 不接「另 ×N」；全局的 ⇒ 账号库那一式"
    );

    let none = finish(Checks {
        checks: vec![line(Some("z"), CheckLevel::Warn, "还没登录")],
    });
    assert_eq!(none.headline, None, "没有对不上的 ⇒ 缺");
    assert_eq!(
        serde_json::to_value(&none).unwrap().get("headline"),
        None,
        "缺 ⇒ 不上线"
    );
}

/// 「未登录」照那个号那一家声明的凭据文件判（号表判「已登录」问的同一格）：缺省那一家的号缺凭据 ⇒ 说未登录；
/// 那一家没声明凭据文件的号（不在 app 里登的那一家）⇒ 不拿缺省那一家的凭据名去判它。
#[test]
fn not_logged_in_follows_the_credentials_file_of_that_accounts_family() {
    let said = copy_text("beAcctVerify.acct.notLoggedIn", &[]);
    let mine = verify(&snap(BTreeMap::new()), &[]);
    assert!(
        warns(&mine).contains(&said.as_str()),
        "缺省那一家的号缺凭据"
    );
    // 替身注册表：缺省那一家声明了凭据文件；另一家（`no-login`）身份表同形，却没声明凭据文件。
    let [one, two] = two_families();
    let unsaid = crate::agents::Adapter {
        kind: "no-login",
        accounts: Some(crate::agents::AccountsFace {
            login: None,
            ..two.accounts.expect("替身家有账号面")
        }),
        ..two
    };
    let reg = [one, unsaid];
    let mut s = two_family_snap(false, false);
    let text = r#"{"version":1,"sharedStore":"/h/.claude","accounts":[
        {"name":"z","email":"","configDir":"/h/.cc-monitor/accounts/z","isDefault":true},
        {"name":"t","email":"","configDir":"/h/.cc-monitor/accounts/t","agent":"no-login"}]}"#;
    s.manifest = Some(super::super::model::Manifest::parse(text));
    let who: Vec<String> = verify_among(&reg, &s, &[])
        .checks
        .into_iter()
        .filter(|c| c.text == said)
        .filter_map(|c| c.account)
        .collect();
    assert_eq!(
        who,
        ["z"],
        "no-login 没声明凭据文件，不该拿缺省那一家的凭据名判它未登录"
    );
}

/// 另一家替身的凭据文件名：跟缺省那一家的不同名，身份表各是各的（拿缺省那一家的表核它就认不出它的凭据）。
const TWO_CREDS: &str = "st-login.ndjson";

/// 两家替身：缺省那一家（共享库是它的）申报了共享 `skills`、身份本体是它的凭据文件；另一家只申报身份本体（另一个名字）、不共享。
fn two_families() -> [crate::agents::Adapter; 2] {
    use crate::agents::{IdentityClass, IdentityRoot};
    let face = |row: &crate::agents::Adapter| row.accounts.expect("替身家有账号面");
    let one = crate::agents::fake::family_row("stand-in", "CCM_STAND_IN_DIR", "si", true, || None);
    let two =
        crate::agents::fake::family_row("stand-two", "CCM_STAND_TWO_DIR", "st", false, || None);
    let login = face(&one).login.expect("替身家声明了凭据文件");
    assert_eq!(
        login.creds_file, "fake-login.ndjson",
        "身份表那一项照凭据文件名写"
    );
    [
        crate::agents::Adapter {
            accounts: Some(crate::agents::AccountsFace {
                identity: &[(
                    "fake-login.ndjson",
                    IdentityRoot::ConfigDir,
                    IdentityClass::Secret,
                )],
                shared: &["skills"],
                ..face(&one)
            }),
            ..one
        },
        crate::agents::Adapter {
            accounts: Some(crate::agents::AccountsFace {
                identity: &[(TWO_CREDS, IdentityRoot::ConfigDir, IdentityClass::Secret)],
                shared: &[],
                login: Some(crate::agents::LoginFace {
                    creds_file: TWO_CREDS,
                    ..login
                }),
                ..face(&two)
            }),
            ..two
        },
    ]
}

/// 两家替身各一个号：z 是缺省那一家的（链共享库的 `skills`），t 是另一家的（不链共享库，照它那一家的表只有自己的凭据）。
fn two_family_snap(z_creds: bool, t_creds: bool) -> Snapshot {
    let text = r#"{"version":1,"sharedStore":"/h/.claude","accounts":[
        {"name":"z","email":"","configDir":"/h/.cc-monitor/accounts/z","isDefault":true},
        {"name":"t","email":"","configDir":"/h/.cc-monitor/accounts/t","agent":"stand-two"}]}"#;
    let mut s = snap(BTreeMap::new());
    s.manifest = Some(super::super::model::Manifest::parse(text));
    s.shared.entries = [("skills".to_string(), Item::Dir { mode: Some(0o755) })].into();
    let mut z: BTreeMap<String, Item> = [(
        "skills".to_string(),
        Item::Link {
            target: "/h/.claude/skills".into(),
            dangling: false,
        },
    )]
    .into();
    let mut t = BTreeMap::new();
    if z_creds {
        z.insert("fake-login.ndjson".into(), own_file(0o600));
    }
    if t_creds {
        t.insert(TWO_CREDS.into(), own_file(0o600));
    }
    for (n, e) in [("z", z), ("t", t)] {
        s.dirs.insert(
            format!("/h/.cc-monitor/accounts/{n}"),
            DirState {
                me: Some(Item::Dir { mode: Some(0o700) }),
                entries: e,
            },
        );
    }
    s
}

/// 核对照每个号那一家的身份表 · 共享表核：另一家的号不链共享库，不报缺链；两家的凭据各照各家的表认（不报「没归类」）。
#[test]
fn each_account_is_checked_by_its_own_familys_tables() {
    let reg = two_families();
    let r = verify_among(&reg, &two_family_snap(true, true), &[]);
    assert!(r.pass, "{:?}", fails(&r));
    for (n, item) in [("z", "fake-login.ndjson"), ("t", TWO_CREDS)] {
        let cred_ok = copy_text("beAcctVerify.acct.credOk", &[("item", item)]);
        assert!(
            r.checks
                .iter()
                .any(|c| c.account.as_deref() == Some(n) && c.text == cred_ok),
            "{n} 的凭据没照它那一家的身份表认：{:?}",
            r.checks
        );
    }
    let linked = copy_text("beAcctVerify.acct.linksOk", &[("n", "1")]);
    assert!(
        r.checks
            .iter()
            .any(|c| c.account.as_deref() == Some("z") && c.text == linked),
        "缺省那一家的号照它的共享表核：{:?}",
        r.checks
    );
    assert!(
        !r.checks
            .iter()
            .any(|c| c.account.as_deref() == Some("t") && c.text.contains("skills")),
        "另一家的号被拿共享库的表核：{:?}",
        r.checks
    );
    assert!(
        !r.checks.iter().any(|c| c.account.is_none()
            && (c.text.contains("fake-login.ndjson") || c.text.contains(TWO_CREDS))),
        "凭据被当成没归类：{:?}",
        r.checks
    );
}

/// 全局那几条（就地 / 隔离 · 共享库残留 · 家目录留配置 · 身份本体哪儿都没有）只数共享库那一家的号：
/// 账号库里只有另一家的号 ⇒ 一条都不出，也不照缺省那一家的身份表去别家号的目录里找。
#[test]
fn the_global_lines_count_only_accounts_of_the_store_family() {
    let reg = two_families();
    let mut s = two_family_snap(false, true);
    let text = r#"{"version":1,"sharedStore":"/h/.claude","accounts":[
        {"name":"t","email":"","configDir":"/h/.cc-monitor/accounts/t","agent":"stand-two"}]}"#;
    s.manifest = Some(super::super::model::Manifest::parse(text));
    s.home_items
        .insert("fake-config.json".into(), own_file(0o600));
    let r = verify_among(&reg, &s, &[]);
    let global: Vec<&VerifyCheck> = r
        .checks
        .iter()
        .filter(|c| c.account.is_none() && c.level != CheckLevel::Skip)
        .collect();
    assert!(
        global.is_empty(),
        "只有别家的号也出了全局那几条：{global:?}"
    );

    // 正控：同一份快照多一个缺省那一家的隔离号 ⇒ 家目录留配置那一条照出。
    let text = r#"{"version":1,"sharedStore":"/h/.claude","accounts":[
        {"name":"z","email":"","configDir":"/h/.cc-monitor/accounts/z","isDefault":true},
        {"name":"t","email":"","configDir":"/h/.cc-monitor/accounts/t","agent":"stand-two"}]}"#;
    s.manifest = Some(super::super::model::Manifest::parse(text));
    let home_config = copy_text(
        "beAcctVerify.global.homeConfig",
        &[("path", "/h/fake-config.json")],
    );
    assert!(
        warns(&verify_among(&reg, &s, &[])).contains(&home_config.as_str()),
        "缺省那一家的隔离号在 ⇒ 家目录留配置照报"
    );
}

/// 邮箱重复只在同一家的号之间判：两家各一个号用同一个邮箱（同一个人两家各登一个，常态）⇒ 不报重复；
/// 同一家两个号用同一个邮箱 ⇒ 照报一条。
#[test]
fn the_same_email_is_a_duplicate_only_within_one_family() {
    let reg = two_families();
    let dup = |r: &VerifyReport| -> usize {
        r.checks
            .iter()
            .filter(|c| c.level == CheckLevel::Fail && c.text.contains("same@x"))
            .count()
    };
    let mut s = two_family_snap(true, true);
    for n in ["z", "t"] {
        s.emails
            .insert(format!("/h/.cc-monitor/accounts/{n}"), "same@x".into());
    }
    let r = verify_among(&reg, &s, &[]);
    assert_eq!(dup(&r), 0, "两家各一个号同邮箱被报成重复：{:?}", r.checks);
    assert!(r.pass, "{:?}", fails(&r));

    let text = r#"{"version":1,"sharedStore":"/h/.claude","accounts":[
        {"name":"z","email":"","configDir":"/h/.cc-monitor/accounts/z","isDefault":true},
        {"name":"t","email":"","configDir":"/h/.cc-monitor/accounts/t"}]}"#;
    s.manifest = Some(super::super::model::Manifest::parse(text));
    let r = verify_among(&reg, &s, &[]);
    assert_eq!(dup(&r), 1, "同一家两个号同邮箱没报：{:?}", r.checks);
}

/// 未登录各照各家：两家的号都缺凭据 ⇒ 各报一条；那一家的凭据在 ⇒ 不报它。
#[test]
fn not_logged_in_is_said_per_family() {
    let reg = two_families();
    let said = copy_text("beAcctVerify.acct.notLoggedIn", &[]);
    let who = |r: &VerifyReport| -> Vec<String> {
        r.checks
            .iter()
            .filter(|c| c.text == said)
            .filter_map(|c| c.account.clone())
            .collect()
    };
    assert_eq!(
        who(&verify_among(&reg, &two_family_snap(false, false), &[])),
        ["z", "t"]
    );
    assert_eq!(
        who(&verify_among(&reg, &two_family_snap(true, false), &[])),
        ["t"]
    );
}
