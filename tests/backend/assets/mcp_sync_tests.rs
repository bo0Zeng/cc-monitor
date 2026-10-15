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
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-mcp-sync-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

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
    let d = temp_dir("live");
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
        plan(&rows, &set(&["a", "c"]), &set(&[])).unwrap_err().0,
        "needs_consent"
    );
    assert_eq!(
        plan(&rows, &set(&["d"]), &set(&[])).unwrap_err().0,
        "bad_args"
    );
    assert_eq!(
        plan(&rows, &set(&["zzz"]), &set(&[])).unwrap_err().0,
        "bad_args"
    );
    assert_eq!(
        plan(&rows, &set(&["a"]), &set(&["c"])).unwrap_err().0,
        "bad_args"
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
        "File::open",
        "OpenOptions",
        "read_dir",
    ];
    let count = |src: &str| {
        let prod = crate::guard_support::production_code(src);
        needles
            .iter()
            .map(|n| prod.matches(n).count())
            .sum::<usize>()
    };
    let src = include_str!("../../../src/backend/assets/mcp_sync.rs");
    assert_eq!(count(src), 0, "mcp_sync.rs 生产段出现了读文件内容的原语");
    let planted = src.replacen(
        "match Ro::metadata(p) {",
        "let _ = Ro::read_to_string(p); match Ro::metadata(p) {",
        1,
    );
    assert_ne!(planted, src, "正控的锚没打中");
    assert_eq!(count(&planted), 1);
}
