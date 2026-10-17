//! 继承表（[`super::INHERIT`]）与判它的那几个函数两向对拍：表里每一格点名的函数在源码里有、头注带着表的标记；
//! 源码里带那个标记的函数，每一个都在表里。表是「新会话从哪拿了什么」唯一的出处，判还是那几个函数判。

use super::*;

/// 标记：判继承表某一格的函数，头注里带这一句（表只认它，不认名字）。
const MARK: &str = "〔继承表〕";

/// `crate::a::b::c::f` / `crate::a::b::T::f` ⇒ （候选文件，函数名）。模块路径最后一段大写开头的是类型，去掉。
fn locate(judge: &str) -> (Vec<std::path::PathBuf>, String) {
    let parts: Vec<&str> = judge.split("::").collect();
    assert_eq!(
        parts.first(),
        Some(&"crate"),
        "判它的函数要写成 crate:: 开头的全路径：{judge}"
    );
    let name = parts.last().expect("有函数名").to_string();
    let mut module: Vec<&str> = parts[1..parts.len() - 1].to_vec();
    if module
        .last()
        .is_some_and(|m| m.starts_with(|c: char| c.is_ascii_uppercase()))
    {
        module.pop();
    }
    let rel = module.join("/");
    let root = guard_core::addr::backend_src_root();
    (
        vec![
            root.join(format!("{rel}.rs")),
            root.join(&rel).join("mod.rs"),
        ],
        name,
    )
}

/// 一份源文件里头注带 [`MARK`] 的函数名（标记之后、下一个 `fn` 之前只许是头注与属性）。
/// 按词认：头注行的第一个词以 `///` 打头；`fn` 是一个完整的词，函数名是它后面那个词的标识符那一截。
fn marked_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut armed = false;
    for src_line in text.lines() {
        let words: Vec<&str> = src_line.split_whitespace().collect();
        let first = words.first().copied().unwrap_or("");
        if first.get(..3) == Some("///") {
            if guard_core::contains_word(src_line, MARK) {
                armed = true;
            }
            continue;
        }
        if !armed || first.is_empty() || first.get(..2) == Some("#[") {
            continue;
        }
        armed = false;
        let Some(at) = words.iter().position(|w| *w == "fn") else {
            continue;
        };
        let name: String = words
            .get(at + 1)
            .copied()
            .unwrap_or("")
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            out.push(name);
        }
    }
    out
}

/// 每个生产源文件里带标记的函数：`crate::<模块路径>::<函数名>`（模块路径照文件位置写，不带类型）。
fn marked_everywhere() -> std::collections::BTreeSet<(String, String)> {
    let root = guard_core::addr::backend_src_root();
    let mut out = std::collections::BTreeSet::new();
    for (path, text) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        for name in marked_in(&text) {
            out.insert((rel.clone(), name));
        }
    }
    out
}

#[test]
fn every_inherit_row_names_a_judge_and_every_marked_judge_is_a_row() {
    assert!(!INHERIT.is_empty(), "继承表是空的 —— 下面两向都会空真");
    let mut from_table = std::collections::BTreeSet::new();
    for row in INHERIT {
        let (files, name) = locate(row.judge);
        let hit = files.iter().find(|f| f.is_file()).unwrap_or_else(|| {
            panic!(
                "{:?} 那一格点名的 {} 找不到文件（{:?}）",
                row.item, row.judge, files
            )
        });
        let text = std::fs::read_to_string(hit).expect("读得出");
        assert!(
            marked_in(&text).contains(&name),
            "{:?} 那一格点名的 {} 在 {} 里没有、或它的头注没带「{MARK}」",
            row.item,
            row.judge,
            hit.display()
        );
        let rel = hit
            .strip_prefix(guard_core::addr::backend_src_root())
            .expect("在源码树里")
            .to_string_lossy()
            .replace('\\', "/");
        from_table.insert((rel, name));
    }
    let marked = marked_everywhere();
    assert!(
        marked.len() >= INHERIT.len(),
        "扫描器只认出 {} 个带标记的函数，比表里的 {} 格还少 —— 扫描器坏了",
        marked.len(),
        INHERIT.len()
    );
    let stray: Vec<_> = marked.difference(&from_table).collect();
    assert!(
        stray.is_empty(),
        "这几个函数头注带着「{MARK}」、表里却没有它判的那一格：{stray:?}"
    );
}

#[test]
fn each_item_is_one_row_and_says_both_paths() {
    let mut seen = Vec::new();
    for row in INHERIT {
        assert!(!seen.contains(&row.item), "{:?} 在表里出现了两次", row.item);
        seen.push(row.item);
    }
    // 直路与 tmux 路这一格就是不一样：表里照实写，不假装一样。
    let rest = INHERIT
        .iter()
        .find(|r| r.item == Item::OtherEnv)
        .expect("其余环境变量那一格在");
    assert_eq!((rest.direct, rest.tmux), (Pass::Caller, Pass::Pane));
    // 密钥不靠继承走：我们的中转那一形每一发重新问，tmux 路不搬调用方整份环境。
    let relay = INHERIT
        .iter()
        .find(|r| r.item == Item::Relay)
        .expect("中转地址那一格在");
    assert_eq!(relay.direct, Pass::Fresh);
}

// ── `--ccm-print` 的「来自」一节 ─────────────────────────────────────────────

fn env() -> Env {
    Env {
        home: "/home/pi".into(),
        pwd: "/here".into(),
        accts_manifest: "/nonexistent/accounts.json".into(),
        account_env: "CLAUDE_CONFIG_DIR".into(),
        self_argv: vec!["/usr/local/bin/ccm".into()],
        ..Default::default()
    }
}

/// `layers` 是别名那几层（名字 ＋ ccm 那半的词），`args` 是命令行当场给的（`[交给 agent 的…] -- [ccm 的…]`）。
fn rows_of(layers: &[(&str, &[&str])], args: &[&str], e: &Env) -> Vec<(String, String, Source)> {
    let layers: Vec<crate::ccm_line::argv::Layer> = layers
        .iter()
        .map(|(name, ccm)| crate::ccm_line::argv::Layer {
            name: name.to_string(),
            agent: Vec::new(),
            ccm: ccm.iter().map(|s| s.to_string()).collect(),
        })
        .collect();
    let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (parsed, origins) = crate::ccm_line::argv::parse_layered(&layers, &a).expect("解析得动");
    let crate::ccm_line::argv::Parsed::Opts(o) = parsed else {
        panic!("该是起会话")
    };
    let t = AccountTable::default();
    let plan = build(&o, e, &t, None).expect("算得出计划");
    from_rows(&o, &origins, e, &t, &plan)
        .into_iter()
        .map(|r| (r.label, r.value, r.from))
        .collect()
}

fn row<'a>(rows: &'a [(String, String, Source)], label_key: &str) -> &'a (String, String, Source) {
    let label = copy_core::copy_text(label_key, &[]);
    rows.iter()
        .find(|r| r.0 == label)
        .unwrap_or_else(|| panic!("没有「{label}」那一行：{rows:?}"))
}

#[test]
fn the_from_section_says_flag_alias_inherit_or_default_for_each_item() {
    // 什么都没给：目录是调用方当前目录 ⇒ 继承；启动器 · 在哪起 ⇒ 默认；没有账号库也没继承号 ⇒ 不指定 · 默认。
    let bare = rows_of(&[], &["--"], &env());
    assert_eq!(row(&bare, "beCcm.item.cwd").1, "/here");
    assert_eq!(row(&bare, "beCcm.item.cwd").2, Source::Inherit);
    assert_eq!(row(&bare, "beProfile.slot.launcher").2, Source::Default);
    assert_eq!(
        row(&bare, "beProfile.slot.tmux").1,
        copy_core::copy_text("beCcm.value.here", &[])
    );
    assert_eq!(row(&bare, "beProfile.slot.tmux").2, Source::Default);
    assert_eq!(
        row(&bare, "beProfile.slot.account").1,
        copy_core::copy_text("beCcm.value.noAccount", &[])
    );
    assert_eq!(row(&bare, "beProfile.slot.account").2, Source::Default);
    // 命令行给的 ⇒ 命令行。
    let flags = rows_of(
        &[],
        &[
            "--",
            "--cwd",
            "/p",
            "--launcher",
            "mycc",
            "--account-dir",
            "/acct/x",
        ],
        &env(),
    );
    assert_eq!(row(&flags, "beCcm.item.cwd").2, Source::Flag);
    assert_eq!(row(&flags, "beProfile.slot.launcher").1, "mycc");
    assert_eq!(row(&flags, "beProfile.slot.launcher").2, Source::Flag);
    assert_eq!(row(&flags, "beProfile.slot.account").1, "/acct/x");
    assert_eq!(row(&flags, "beProfile.slot.account").2, Source::Flag);
    // 别名里写的 ⇒ 别名 <那一段>；命令行当场给的盖过它 ⇒ 命令行。
    let alias = rows_of(
        &[("cct", &["--cwd", "/a", "--ccm-tmux=t1"])],
        &["--", "--launcher", "x"],
        &env(),
    );
    assert_eq!(row(&alias, "beCcm.item.cwd").1, "/a");
    assert_eq!(row(&alias, "beCcm.item.cwd").2, Source::Alias("cct".into()));
    assert_eq!(row(&alias, "beProfile.slot.tmux").1, "t1");
    assert_eq!(
        row(&alias, "beProfile.slot.tmux").2,
        Source::Alias("cct".into())
    );
    let over = rows_of(&[("cct", &["--cwd", "/a"])], &["--", "--cwd", "/b"], &env());
    assert_eq!(
        (
            row(&over, "beCcm.item.cwd").1.as_str(),
            &row(&over, "beCcm.item.cwd").2
        ),
        ("/b", &Source::Flag)
    );
    // 继承来的号 ⇒ 继承；调用方是某条会话的 shell ⇒ 主会话那一行 · 继承。
    let mut e = env();
    e.inherited_config_dir = Some("/acct/b".into());
    e.inherited_parent = Some(("CLAUDE_CODE_SESSION_ID".into(), "s-parent".into()));
    let kid = rows_of(&[], &["--"], &e);
    assert_eq!(row(&kid, "beProfile.slot.account").1, "/acct/b");
    assert_eq!(row(&kid, "beProfile.slot.account").2, Source::Inherit);
    assert_eq!(row(&kid, "beCcm.item.parent").1, "s-parent");
    assert_eq!(row(&kid, "beCcm.item.parent").2, Source::Inherit);
    assert!(
        bare.iter()
            .all(|r| r.0 != copy_core::copy_text("beCcm.item.parent", &[])),
        "不在任何会话里也写了主会话那一行"
    );
}

#[test]
fn the_from_section_is_comment_lines_only() {
    let a: Vec<String> = ["--", "--cwd", "/p"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let (parsed, origins) = crate::ccm_line::argv::parse_layered(&[], &a).expect("解析得动");
    let crate::ccm_line::argv::Parsed::Opts(o) = parsed else {
        panic!("该是起会话")
    };
    let t = AccountTable::default();
    let plan = build(&o, &env(), &t, None).expect("算得出计划");
    let text = render_from(&from_rows(&o, &origins, &env(), &t, &plan));
    // 每一行都是注释（混进那条命令交给 shell 就会被当命令跑）：标题一行 ＋ 四项各一行（值 · 从哪来）。
    let c = |k: &str| copy_core::copy_text(k, &[]);
    let row = |item: String, value: &str, from: &str| {
        let r = copy_core::copy_text(
            "beCcm.print.row",
            &[("item", &item), ("value", value), ("from", &c(from))],
        );
        format!("# {r}\n")
    };
    let want = [
        format!("# {}\n", c("beCcm.print.from")),
        row(c("beCcm.item.cwd"), "/p", "beCcm.from.flag"),
        row(
            c("beProfile.slot.account"),
            &c("beCcm.value.noAccount"),
            "beCcm.from.default",
        ),
        row(c("beProfile.slot.launcher"), "claude", "beCcm.from.default"),
        row(
            c("beProfile.slot.tmux"),
            &c("beCcm.value.here"),
            "beCcm.from.default",
        ),
    ]
    .concat();
    assert_eq!(text, want, "「来自」那一节");
    // 接回一个会话不起新会话 ⇒ 一行都不写。
    let attach: Vec<String> = ["--", "--attach", "x"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let (p, g) = crate::ccm_line::argv::parse_layered(&[], &attach).expect("解析得动");
    let crate::ccm_line::argv::Parsed::Opts(o) = p else {
        panic!()
    };
    let plan = build(&o, &env(), &t, None).expect("算得出计划");
    assert_eq!(render_from(&from_rows(&o, &g, &env(), &t, &plan)), "");
}

/// 额度预热不再自己剥一遍环境（那是继承表的第二份判法）：交给 ccm 的就是它自己的环境，只多它自己要的两格。
/// 它那一发自己声明不轮换（ccm 的 `--no-rotation`）：「新会话用」设成会换号的那一份时也落在它点名的号上，不在脚本里另写判法。
/// 前提核过（`relay::observe_retry_tests::a_session_started_inside_a_swapped_one_keeps_the_account_it_names`）：
/// 在别的会话里起、点名一个号的那一发照那个号发，不跟着主会话换。
#[test]
fn quota_warm_hands_ccm_its_own_environment_plus_its_two_switches() {
    let path = guard_core::addr::repo_root().join("src/shared/quota-warm/scripts/quota-warm");
    let text = std::fs::read_to_string(&path).expect("预热脚本在");
    let code: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    for own in [
        "MAX_THINKING_TOKENS",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
    ] {
        assert!(
            code.contains(&format!("\"{own}\"")),
            "预热自己要的 {own} 那一格不在了"
        );
    }
    assert!(
        code.contains(&format!("\"{}\"", crate::ccm_line::argv::flag::NO_ROTATION)),
        "预热那一发没交 ccm 的「不轮换」"
    );
    for stripped in [
        "\"CCM_\"",
        "\"ANTHROPIC_\"",
        "\"CLAUDE_CONFIG_DIR\"",
        "\"TMUX\"",
        "\"TMUX_PANE\"",
        "\"CLAUDE_CODE_\"",
    ] {
        assert!(
            !code.contains(stripped),
            "预热脚本又按名字剥了一格 {stripped}：新会话从调用方拿什么只由 ccm 的继承表定"
        );
    }
}

/// 登记上 cc-bus 时这一发知道会话编号（`--ccm-sid`）⇒ 一并交给 cc-register（第 5 列，敲门按它经后端说一句）；不知道 ⇒ 不写那一格。
#[test]
fn bus_registration_carries_the_session_id_when_the_launch_knows_it() {
    let tail = |sid: &[&str]| {
        let mut a: Vec<String> = [
            "--",
            "--ccm-tmux=bus-sid",
            "--detach",
            "--bus-register",
            "--cwd",
            "/p",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        a.extend(sid.iter().map(|s| s.to_string()));
        let crate::ccm_line::argv::Parsed::Opts(o) =
            crate::ccm_line::argv::parse(&a).expect("解析得动")
        else {
            panic!()
        };
        let mut e = env();
        e.bus_scripts = Some("/b".into());
        let Plan::Container(c) = build(&o, &e, &AccountTable::default(), None).expect("计划")
        else {
            panic!("该是容器路")
        };
        render_container_tail(&c)
    };
    let with = tail(&["--ccm-sid", "s-1"]);
    assert!(
        with.contains("'/b/cc-register' --sid 's-1' 'bus-sid'"),
        "{with}"
    );
    let without = tail(&[]);
    assert!(
        without.contains("'/b/cc-register' 'bus-sid'") && !without.contains("--sid"),
        "{without}"
    );
}

/// 标签页那一格（继承表 `Item::Tab`）：不随调用方 —— 没写 `--tab` ⇒ 这一发不带（照这台偏好，观测侧判）；
/// 命令行写了 ⇒ 便条带它、来自「命令行」；别名写了 ⇒ 带上别名的名字、来自「别名 <名>」；容器路把它（连同别名名）交给窗格那一趟。
#[test]
fn the_tab_slot_goes_with_the_launch_and_names_its_alias() {
    use crate::control::launch_account::LaunchTab;
    use cmd_wire::session::Tab;
    let row = INHERIT
        .iter()
        .find(|r| r.item == Item::Tab)
        .expect("标签页那一格在继承表里");
    assert_eq!((row.direct, row.tmux), (Pass::Machine, Pass::Machine));
    let plan_of = |layers: &[(&str, &[&str])], args: &[&str]| {
        let layers: Vec<crate::ccm_line::argv::Layer> = layers
            .iter()
            .map(|(name, ccm)| crate::ccm_line::argv::Layer {
                name: name.to_string(),
                agent: Vec::new(),
                ccm: ccm.iter().map(|s| s.to_string()).collect(),
            })
            .collect();
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let (parsed, origins) =
            crate::ccm_line::argv::parse_layered(&layers, &a).expect("解析得动");
        let crate::ccm_line::argv::Parsed::Opts(o) = parsed else {
            panic!("该是起会话")
        };
        let t = AccountTable::default();
        let plan = build(&o, &env(), &t, None).expect("算得出计划");
        let rows = from_rows(&o, &origins, &env(), &t, &plan);
        (plan, rows)
    };
    let tab_of = |p: &Plan| match p {
        Plan::Direct(d) => d.tab.clone(),
        _ => panic!("该是直路"),
    };
    let label = copy_core::copy_text("beProfile.slot.tab", &[]);
    // 没写：不带、「来自」也不写这一行（由这台偏好判，不是这一发的事）。
    let (p, rows) = plan_of(&[], &["--"]);
    assert_eq!(tab_of(&p), None);
    assert!(rows.iter().all(|r| r.label != label));
    // 命令行。
    let (p, rows) = plan_of(&[], &["--", "--tab", "hide"]);
    assert_eq!(
        tab_of(&p),
        Some(LaunchTab {
            tab: Tab::Hide,
            alias: None
        })
    );
    let r = rows
        .iter()
        .find(|r| r.label == label)
        .expect("标签页那一行");
    assert_eq!(r.from, Source::Flag);
    // 别名；命令行当场写回 show 盖过它（单值后写的盖前面）。
    let (p, rows) = plan_of(&[("cct", &["--tab", "hide"])], &["--"]);
    assert_eq!(
        tab_of(&p),
        Some(LaunchTab {
            tab: Tab::Hide,
            alias: Some("cct".into())
        })
    );
    assert_eq!(
        rows.iter()
            .find(|r| r.label == label)
            .map(|r| r.from.clone()),
        Some(Source::Alias("cct".into()))
    );
    let (p, _) = plan_of(&[("cct", &["--tab", "hide"])], &["--", "--tab", "show"]);
    assert_eq!(
        tab_of(&p),
        Some(LaunchTab {
            tab: Tab::Show,
            alias: None
        })
    );
    // 容器路：窗格那一趟的命令行里带 `--tab hide --tab-from cct`。
    let (p, _) = plan_of(
        &[("cct", &["--tab", "hide"])],
        &["--", "--ccm-tmux=t", "--detach"],
    );
    let Plan::Container(c) = p else {
        panic!("该是容器路")
    };
    assert!(
        c.payload.contains("'--tab' 'hide' '--tab-from' 'cct'"),
        "{}",
        c.payload
    );
}

/// `--tab` 只认 show · hide；`--tab-from` 要配 `--tab`、值要是能当别名的名字。
#[test]
fn the_tab_flag_takes_only_show_or_hide() {
    let parse = |a: &[&str]| {
        crate::ccm_line::argv::parse(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    };
    assert!(parse(&["--", "--tab", "hide"]).is_ok());
    assert!(parse(&["--", "--tab=show"]).is_ok());
    let Err(crate::ccm_line::argv::Die(m)) = parse(&["--", "--tab", "maybe"]) else {
        panic!("认了一个不是 show / hide 的值")
    };
    assert_eq!(
        m,
        copy_core::copy_text("beArgv.validate.badTab", &[("v", "\"maybe\"")])
    );
    assert!(parse(&["--", "--tab-from", "cct"]).is_err());
    assert!(parse(&["--", "--tab", "hide", "--tab-from", "a b"]).is_err());
    assert!(parse(&["--", "--tab", "hide", "--tab-from", "cct"]).is_ok());
}

/// `--no-rotation`：这一发不轮换（同 `session-new` 选了「不轮换」）。计划里带上要先记「不轮换」的那个会话ID，
/// 交给那一家的那一串里带着同一个先定会话ID的旗标：透传里写了 ⇒ 用那个值、不另加；没写 ⇒ 铸一个，插在那一家自己的 `--` 之前。
/// 接着的会话（`--resume`）· 接回（`--attach`）· 起的时候定不了会话ID的那一家 ⇒ 拒，不静默吃掉。
/// 不带它 ⇒ 计划里没有这一格（照「新会话用」那一份）。tmux 路把它带进窗格里那一条 ccm，记由那一趟做。
#[test]
fn no_rotation_presets_the_session_id_it_hands_the_agent() {
    let plan_of = |args: &[&str]| -> Result<Plan, String> {
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let o = match crate::ccm_line::argv::parse(&a).map_err(|e| e.0)? {
            crate::ccm_line::argv::Parsed::Opts(o) => o,
            _ => panic!("该是起会话"),
        };
        build(&o, &env(), &AccountTable::default(), None).map_err(|e| e.0)
    };
    let direct = |args: &[&str]| match plan_of(args).expect("算得出计划") {
        Plan::Direct(d) => d,
        other => panic!("该是直路：{other:?}"),
    };
    let flag = crate::agents::claudecode::resume::SESSION_ID_FLAG;
    let after = |argv: &[String]| {
        let at = argv
            .iter()
            .position(|a| a == flag)
            .expect("交出去的那一串里没有先定会话ID的旗标");
        argv[at + 1].clone()
    };
    // 没写 ⇒ 铸一个，插在那一家自己的 `--` 之前。
    let d = direct(&["-p", "--", "-x", "--", "--no-rotation"]);
    let nr = d
        .no_rotation
        .clone()
        .expect("计划里没有要记「不轮换」的那一格");
    assert_eq!(nr.agent, "claude");
    let sid = nr.sid;
    assert!(shell_quote_core::session_id_ok(&sid), "{sid}");
    assert_eq!(after(&d.argv), sid);
    let end = d
        .argv
        .iter()
        .position(|a| a == "--")
        .expect("那一家自己的 --");
    assert!(
        d.argv.iter().position(|a| a == flag) < Some(end),
        "{:?}",
        d.argv
    );
    // 写了 ⇒ 用那个值，不另加。
    let given = "0f0e0d0c-0b0a-4908-8706-050403020100";
    let d = direct(&["-p", flag, given, "--", "--no-rotation"]);
    assert_eq!(d.no_rotation.map(|n| n.sid).as_deref(), Some(given));
    assert_eq!(
        d.argv.iter().filter(|a| *a == flag).count(),
        1,
        "{:?}",
        d.argv
    );
    // 不带 ⇒ 没有这一格，交出去的那一串里也没多出旗标。
    let d = direct(&["-p", "--", "--cwd", "/p"]);
    assert_eq!(d.no_rotation, None);
    assert!(!d.argv.iter().any(|a| a == flag), "{:?}", d.argv);
    // tmux 路：带进窗格里那一条 ccm（记由那一趟做）。
    match plan_of(&["-p", "--", "--ccm-tmux=nr", "--detach", "--no-rotation"]).expect("算得出") {
        Plan::Container(c) => assert!(
            c.payload.contains(crate::ccm_line::argv::flag::NO_ROTATION),
            "{}",
            c.payload
        ),
        other => panic!("该是容器路：{other:?}"),
    }
    // 拒的几形。
    for bad in [
        &["--resume", "abc", "--", "--no-rotation"][..],
        &["--", "--attach", "x", "--no-rotation"][..],
        &["--", "--ccm-agent", "codex", "--no-rotation"][..],
    ] {
        assert!(plan_of(bad).is_err(), "{bad:?} 该拒");
    }
}
