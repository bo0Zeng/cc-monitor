//! `mcp_sync.rs` 的判据 —— MCP 资产同步的判定（差异 · 可疑项 · 写哪几条）。
//!
//! # 买到的
//!
//! - 差异四态逐行**相等**（不是「数出来 ≥ N」）；对象键序不同不算不同。
//! - 可疑项的候选按规则逐条**相等**（四个字段、`--opt=/abs` 形、`~/…` 不标、`url` 不看）。
//! - 事实在临时目录与给定的 `PATH` 上真 `stat`：有 / 没有 / 别家系统的写法 / 没有 `PATH` 各一格；
//!   `PATHEXT` 补后缀那一支在 Linux 上也跑得到（它读的是入参，不是 `cfg`）。
//! - 「对面有不同就问盖不盖」由后端执行：`differs` 没点名要盖 ⇒ **整趟拒**，不静默跳过。
//! - 本模块生产段不读任何文件内容（零命中，带正控）。
//!
//! # 买不到的
//!
//! - 🔴 真远端 / 真 Windows：`PATH` 是那台**后端进程**的；Windows 的 `C:\` 路径在 Windows 上 `stat` 没现打。
//! - 不看执行位（有这个名字的文件就算「找得到」）。

use super::*;
use serde_json::json;

fn map(v: Value) -> Map<String, Value> {
    v.as_object().expect("夹具是对象").clone()
}

fn states(rows: &[(String, State)]) -> Vec<(&str, &str)> {
    rows.iter().map(|(n, s)| (n.as_str(), s.wire())).collect()
}

/// 替身事实：一张表说了算（给 `judge` 的纯性质用）。
struct Table {
    present: &'static [&'static str],
    on_path: Option<&'static [&'static str]>,
}

impl Facts for Table {
    fn path(&self, p: &str) -> There {
        if self.present.contains(&p) {
            There::Present
        } else {
            There::Absent
        }
    }
    fn command(&self, name: &str) -> Option<bool> {
        self.on_path.map(|l| l.contains(&name))
    }
}

#[test]
fn every_name_gets_exactly_one_of_the_four_states() {
    let src = map(json!({
        "fresh": {"command": "a"},
        "equal": {"command": "b", "args": ["x"]},
        "changed": {"command": "c"},
    }));
    let tgt = map(json!({
        // 键序与拷出来的那一份相反 —— 值相等，不许判成不同。
        "equal": {"args": ["x"], "command": "b"},
        "changed": {"command": "c", "env": {"K": "v"}},
        "theirs": {"url": "https://example.invalid/mcp", "type": "http"},
    }));
    assert_eq!(
        states(&diff(&src, &tgt)),
        vec![
            ("changed", "differs"),
            ("equal", "same"),
            ("fresh", "new"),
            ("theirs", "only-there"),
        ]
    );
    // 对面没有那份文件 ⇒ 全是 new。
    assert_eq!(
        states(&diff(&src, &Map::new())),
        vec![("changed", "new"), ("equal", "new"), ("fresh", "new")]
    );
}

/// 差异的四态 · 可疑项的三种 · 可疑项在这台机器上的事实（线上名）：界面文案表按它们逐键给字。
const STATES: &[&str] = &["new", "same", "differs", "only-there"];
const SUSPECT_KINDS: &[&str] = &["abs-path", "command-missing", "command-relative"];
const THERE: &[&str] = &["present", "absent", "foreign", "unknown"];

#[test]
fn the_wire_names_are_exactly_the_declared_closed_sets() {
    // 穷尽 match：加一个态 / 一种事实而忘了进闭集 ⇒ 这里编不过或两向不等。
    let all_states = [State::New, State::Same, State::Differs, State::OnlyThere];
    for s in all_states {
        match s {
            State::New | State::Same | State::Differs | State::OnlyThere => {}
        }
    }
    let got: BTreeSet<&str> = all_states.iter().map(|s| s.wire()).collect();
    assert_eq!(got, STATES.iter().copied().collect::<BTreeSet<_>>());
    let all_there = [
        There::Present,
        There::Absent,
        There::Foreign,
        There::Unknown,
    ];
    for t in all_there {
        match t {
            There::Present | There::Absent | There::Foreign | There::Unknown => {}
        }
    }
    let got: BTreeSet<&str> = all_there.iter().map(|t| t.wire()).collect();
    assert_eq!(got, THERE.iter().copied().collect::<BTreeSet<_>>());
    // 可疑项的种类：从 `judge` 真产出的那一侧收，与闭集两向相等。
    let cands = vec![
        Candidate::AbsPath {
            field: "command".into(),
            path: "/p".into(),
        },
        Candidate::BareCommand { name: "n".into() },
        Candidate::RelativeCommand {
            command: "./r".into(),
        },
    ];
    let facts = Table {
        present: &[],
        on_path: Some(&[]),
    };
    let kinds: BTreeSet<&str> = judge(&cands, &facts).iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        SUSPECT_KINDS.iter().copied().collect::<BTreeSet<_>>()
    );
}

#[test]
fn candidates_follow_the_written_rules_field_by_field() {
    let server = json!({
        "command": "/home/u/bin/server",
        "args": ["--port", "7", "/home/u/data", "--config=/etc/x.toml", "~/rel", "C:\\tools\\a.exe", "plain=value"],
        "env": {"ROOT": "D:/work", "TOKEN": "abc", "SHARE": "\\\\srv\\share"},
        "cwd": "/srv/proj",
        "url": "/not/looked/at",
    });
    let got = candidates(&server);
    let abs = |f: &str, p: &str| Candidate::AbsPath {
        field: f.into(),
        path: p.into(),
    };
    assert_eq!(
        got,
        vec![
            abs("command", "/home/u/bin/server"),
            abs("args[2]", "/home/u/data"),
            abs("args[3]", "/etc/x.toml"),
            abs("args[5]", "C:\\tools\\a.exe"),
            abs("env.ROOT", "D:/work"),
            abs("env.SHARE", "\\\\srv\\share"),
            abs("cwd", "/srv/proj"),
        ]
    );
    assert_eq!(
        candidates(&json!({"command": "npx", "args": ["-y", "@x/mcp"]})),
        vec![Candidate::BareCommand { name: "npx".into() }]
    );
    assert_eq!(
        candidates(&json!({"command": "node_modules/.bin/srv"})),
        vec![Candidate::RelativeCommand {
            command: "node_modules/.bin/srv".into()
        }]
    );
    // 远程型（只有 url）/ 不是对象 ⇒ 什么都不标。
    assert_eq!(
        candidates(&json!({"type": "http", "url": "http://127.0.0.1:9/mcp"})),
        vec![]
    );
    assert_eq!(candidates(&json!("not an object")), vec![]);
}

#[test]
fn judge_marks_found_commands_as_fine_and_everything_else_as_suspect() {
    let cands = vec![
        Candidate::AbsPath {
            field: "args[0]".into(),
            path: "/there".into(),
        },
        Candidate::AbsPath {
            field: "cwd".into(),
            path: "/gone".into(),
        },
        Candidate::BareCommand { name: "npx".into() },
        Candidate::BareCommand { name: "uvx".into() },
        Candidate::RelativeCommand {
            command: "./bin/s".into(),
        },
    ];
    let facts = Table {
        present: &["/there"],
        on_path: Some(&["npx"]),
    };
    let got: Vec<(&str, String, String, Option<There>)> = judge(&cands, &facts)
        .into_iter()
        .map(|s| (s.kind, s.field, s.value, s.there))
        .collect();
    assert_eq!(
        got,
        vec![
            (
                "abs-path",
                "args[0]".into(),
                "/there".into(),
                Some(There::Present)
            ),
            (
                "abs-path",
                "cwd".into(),
                "/gone".into(),
                Some(There::Absent)
            ),
            (
                "command-missing",
                "command".into(),
                "uvx".into(),
                Some(There::Absent)
            ),
            ("command-relative", "command".into(), "./bin/s".into(), None),
        ]
    );
    // 没有 `PATH` 可查 ⇒ 不说「没有」，说「查不动」。
    let blind = Table {
        present: &[],
        on_path: None,
    };
    let got: Vec<Option<There>> = judge(&[Candidate::BareCommand { name: "npx".into() }], &blind)
        .into_iter()
        .map(|s| s.there)
        .collect();
    assert_eq!(got, vec![Some(There::Unknown)]);
}

#[test]
fn the_live_facts_really_look_at_this_machine() {
    let d = test_home::scratch("live");
    let bin = d.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("hastool"), "").unwrap();
    std::fs::write(bin.join("wintool.cmd"), "").unwrap();
    let here = d.join("here.txt");
    std::fs::write(&here, "").unwrap();
    let live = Live {
        path_var: Some(std::env::join_paths([&bin]).unwrap()),
        windows: false,
    };
    assert_eq!(live.path(&here.display().to_string()), There::Present);
    assert_eq!(
        live.path(&d.join("nope").display().to_string()),
        There::Absent
    );
    // 别家系统的绝对写法在这台上不是绝对路径 ⇒ foreign（不是 absent）。
    let foreign = if cfg!(windows) {
        "/usr/bin/x"
    } else {
        "C:\\x\\y.exe"
    };
    assert_eq!(live.path(foreign), There::Foreign);
    assert_eq!(live.command("hastool"), Some(true));
    assert_eq!(live.command("wintool"), Some(false), "unix 只找原名");
    let win = Live {
        path_var: live.path_var.clone(),
        windows: true,
    };
    assert_eq!(
        win.command("wintool"),
        Some(true),
        "Windows 上 .cmd 那一形找得到"
    );
    assert_eq!(
        win.command("hastool"),
        Some(false),
        "Windows 上不带后缀的那份不算"
    );
    assert_eq!(win.command("absent-tool"), Some(false));
    let no_path = Live {
        path_var: None,
        windows: false,
    };
    assert_eq!(no_path.command("hastool"), None);
    let empty_path = Live {
        path_var: Some(OsString::new()),
        windows: false,
    };
    assert_eq!(
        empty_path.command("hastool"),
        None,
        "PATH 切不出一个目录 ⇒ 判不了"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn differing_entries_are_written_only_when_the_user_said_overwrite() {
    let rows = vec![
        ("a".to_string(), State::New),
        ("b".to_string(), State::Same),
        ("c".to_string(), State::Differs),
        ("d".to_string(), State::OnlyThere),
    ];
    let set = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
    assert_eq!(
        plan(&rows, &set(&["a", "b"]), &set(&[])),
        Ok(vec!["a".to_string()]),
        "same 不写"
    );
    assert_eq!(
        plan(&rows, &set(&["a", "c"]), &set(&["c"])),
        Ok(vec!["a".to_string(), "c".to_string()])
    );
    assert_eq!(
        plan(&rows, &set(&["a", "c"]), &set(&[]))
            .unwrap_err()
            .code
            .as_deref(),
        Some("needs_consent")
    );
    assert_eq!(
        plan(&rows, &set(&["d"]), &set(&[]))
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args")
    );
    assert_eq!(
        plan(&rows, &set(&["zzz"]), &set(&[]))
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args")
    );
    assert_eq!(
        plan(&rows, &set(&["a"]), &set(&["c"]))
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args")
    );
}

/// 🔴 头注「不读用户的文件（两份原文由调用方经 `Door::peek` 读来）」：读的那一份与 CAS 期望的那一份
/// 必须是同一个读法 —— 本模块自己再读一遍就是第二个读法。生产段读文件内容的原语零命中；正控：同一把尺子
/// 在一份塞了读原语的副本上数得到。
#[test]
fn this_module_reads_no_file_content() {
    let needles = [
        "read_to_string",
        "::read(",
        "read_capped",
        "read_string_capped",
        "read_json",
        "File::open",
        "OpenOptions",
        "read_dir",
    ];
    let count = |src: &str| {
        let prod = guard_core::production_code(src);
        needles
            .iter()
            .map(|n| prod.matches(n).count())
            .sum::<usize>()
    };
    let src = include_str!("../../../src/backend/assets/mcp_sync.rs");
    assert_eq!(count(src), 0, "mcp_sync.rs 生产段出现了读文件内容的原语");
    let planted = src.replacen(
        "match Ro::metadata(p) {",
        "let _ = Ro::read_string_capped(p, CAP); match Ro::metadata(p) {",
        1,
    );
    assert_ne!(planted, src, "正控的锚没打中");
    assert_eq!(count(&planted), 1);
}

/// 共享集合里服务器表那一格的键只判一处（账号域的 `store_key`）：两个替身家各在自己的号里写一个服务器、同步进共享集合，
/// 资产这一侧照同一格读回缺省那一家的那一条（另一家住 `agents.<家>`，不当顶层那张表读）；生产注册表上读的也是那一格。
/// 期望的键写字面（替身家资产面声明的 `servers` · Claude 的 `mcpServers`），不从被测的 `store_key` 取。
#[cfg(unix)]
#[test]
fn the_shared_set_is_read_under_the_one_key_the_account_side_writes() {
    use crate::accounts::manage::mcp_share_exec::sync_among;
    let home = test_home::scratch("assets-mcp-store-key");
    let h = home.display().to_string();
    let write = |rel: &str, body: &str| {
        let p = home.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    };
    let row = |name: &str, agent: &str| {
        json!({ "name": name, "email": format!("{name}@example.test"),
                "configDir": format!("{h}/.cc-monitor/accounts/{name}"),
                "isDefault": name == "a1", "agent": agent })
    };
    write(
        &format!(
            "{}/{}",
            relay_route_core::ACCOUNTS_DIR_REL,
            relay_route_core::ACCOUNTS_MANIFEST_NAME
        ),
        &json!({ "version": 1, "accounts": [row("a1", "fake"), row("b1", "fake2")] }).to_string(),
    );
    let conf = |acct: &str| {
        format!(
            ".cc-monitor/accounts/{acct}/{}",
            crate::agents::fake::MCP_FILE
        )
    };
    write(&conf("a1"), "server a-only {\"command\":\"a\"}\n");
    write(&conf("b1"), "server b-only {\"command\":\"b\"}\n");
    let fam = |kind, env, wrapper, default| {
        crate::agents::fake::family_row(kind, env, wrapper, default, || None)
    };
    let reg = [
        fam("fake", "CCM_FAKE_A", "fakea", true),
        fam("fake2", "CCM_FAKE_B", "fakeb", false),
    ];
    let door = crate::assets::aliases::test_support::HomeDoor(home.clone());
    sync_among(&door, &reg).expect("同步一趟");
    let store = std::fs::read_to_string(home.join(relay_route_core::ACCOUNTS_MCP_REL))
        .expect("共享集合落了盘");
    let names = |m: Map<String, Value>| m.keys().cloned().collect::<Vec<_>>();
    assert_eq!(
        names(servers_at(Some(&store), "s", "servers").expect("读得懂")),
        ["a-only"]
    );
    let prod = json!({ "mcpServers": { "x": {} } }).to_string();
    assert_eq!(names(servers_of(Some(&prod), "s").expect("读得懂")), ["x"]);
    let _ = std::fs::remove_dir_all(&home);
}
