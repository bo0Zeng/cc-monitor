//! 列全部命令的那个口（`ccm -- --commands`）的判据，与读它的 skill 说明书命令段的生成器。
//!
//! 三条：口列的命令集 ＝ 分派表（帧面 `REGISTRY` · CLI 面 `SUBCOMMANDS`）两向相等；
//! 每份带命令段的 `src/shared/*/SKILL.md` 与照口现打的逐字相等（金样那一套：不等 ⇒ 现打的落 `.build/golden/`、红，
//! 对过后 `bash tests/scripts/bless-golden.sh` 拷回）；命令段以外不手抄命令（提到的子命令都得在本份命令段里、没有以子命令打头的列表行）。

use super::*;
use crate::stream::inbound::REGISTRY;
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// 命令段的开头一行：`<!-- 命令：<选哪几条> -->`；选的是帧命令名或子命令名（不带 `--`），`x-*` ＝ 名字以 `x-` 打头的全部。
const OPEN: &str = "<!-- 命令：";
/// 命令段的收尾一行。
const CLOSE: &str = "<!-- /命令 -->";
/// 命令段头一行里写给读的人的那半句（生成物、怎么重打）。
const NOTE: &str = "生成物：照 `ccm -- --commands` 现打，勿手改；重打见 `tests/backend/stream/catalog_tests.rs`";

fn name_of(e: &Entry) -> String {
    e.cmd
        .map(str::to_string)
        .or_else(|| e.cli.as_deref().map(|f| f.trim_start_matches("--").to_string()))
        .unwrap_or_default()
}

#[test]
fn the_catalog_is_the_dispatch_table() {
    let rows = catalog();
    let cmds: Vec<&str> = rows.iter().filter_map(|e| e.cmd).collect();
    let cmd_set: BTreeSet<&str> = cmds.iter().copied().collect();
    assert_eq!(cmds.len(), cmd_set.len(), "口里有重名的帧命令");
    let registry: BTreeSet<&str> = REGISTRY.iter().map(|s| s.name).collect();
    let (extra, missing): (Vec<_>, Vec<_>) = (
        cmd_set.difference(&registry).collect(),
        registry.difference(&cmd_set).collect(),
    );
    assert!(
        extra.is_empty() && missing.is_empty(),
        "口列的帧命令与 `REGISTRY` 不两向相等：多 {extra:?} · 少 {missing:?}"
    );

    let clis: Vec<&str> = rows.iter().filter_map(|e| e.cli.as_deref()).collect();
    let cli_set: BTreeSet<&str> = clis.iter().copied().collect();
    assert_eq!(clis.len(), cli_set.len(), "口里有重名的子命令");
    let gate: BTreeSet<&str> = crate::SUBCOMMANDS.iter().copied().collect();
    let (extra, missing): (Vec<_>, Vec<_>) = (
        cli_set.difference(&gate).collect(),
        gate.difference(&cli_set).collect(),
    );
    assert!(
        extra.is_empty() && missing.is_empty(),
        "口列的子命令与 `SUBCOMMANDS` 不两向相等：多 {extra:?}（闸门不认、分派不到）· 少 {missing:?}（`CLI_ONLY_DOCS` 没写说明）"
    );
    assert!(rows.iter().all(|e| e.cmd.is_some() || e.cli.is_some()));
    assert!(rows.iter().all(|e| !e.what.is_empty()), "有一条没有一句话");
    assert!(cmd_set.len() >= 50 && cli_set.len() >= 50, "口里不足 50 条 —— 本断言在空转");
}

#[test]
fn the_subcommand_prints_the_catalog() {
    let mut out = Vec::new();
    assert_eq!(run_to(&mut out), 0);
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.matches('\n').count(), 1, "一行 JSON");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v, serde_json::json!({ "commands": catalog() }));
    let own = v["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["cli"] == "--commands")
        .expect("口里要有它自己");
    assert!(own.get("cmd").is_none(), "`--commands` 是 CLI 独有的");
}

/// 一段选择照口展开成那几条（照选的先后；`x-*` 照口的先后）。选到口里没有的 ⇒ 红。
fn pick(sel: &str, rows: &[Entry]) -> Vec<Entry> {
    let mut got: Vec<Entry> = Vec::new();
    for tok in sel.split_whitespace() {
        let hits: Vec<&Entry> = match tok.strip_suffix('*') {
            Some(pre) => rows.iter().filter(|e| name_of(e).starts_with(pre)).collect(),
            None => rows.iter().filter(|e| name_of(e) == tok).collect(),
        };
        assert!(!hits.is_empty(), "命令段选了 `{tok}`，口里没有这一条");
        for h in hits {
            if !got.contains(h) {
                got.push(h.clone());
            }
        }
    }
    got
}

/// 入参类型那一份 TS 声明：入参本身 ＋ 它递归引到的具名类型（回包与码不进命令段）。
fn req_decls(name: &str, out: &mut Vec<(String, String)>) -> Option<String> {
    let spec = REGISTRY.iter().find(|s| s.name == name)?;
    if !spec.takes_input {
        return None;
    }
    let shape = (spec.run.shape)();
    let mut todo = vec![shape.req.clone()];
    while let Some(t) = todo.pop() {
        if out.iter().any(|(n, _)| *n == t) {
            continue;
        }
        let Some((_, decl)) = shape.decls.iter().find(|(n, _)| *n == t) else {
            continue;
        };
        for (n, _) in &shape.decls {
            if *n != t && guard_core::find_pinned(decl, n).is_ok() {
                todo.push(n.clone());
            }
        }
        out.push((t, decl.clone()));
    }
    Some(shape.req)
}

/// 一个命令段的正文（不含头尾两行）。
fn section(sel: &str, rows: &[Entry]) -> String {
    let mut s = String::new();
    let mut decls = Vec::new();
    for e in pick(sel, rows) {
        let head = match (&e.cli, e.cmd) {
            (Some(f), _) => format!("`ccm -- {f}`"),
            (None, Some(c)) => format!("`{c}`（只在流上）"),
            (None, None) => unreachable!(),
        };
        let usage = e.usage.map(|u| format!(" `{u}`")).unwrap_or_default();
        let args = match e.cmd.and_then(|c| req_decls(c, &mut decls)) {
            Some(req) => format!("参数 `{req}`"),
            None if e.args == Some(false) => "不收参数".to_string(),
            None => String::new(),
        };
        let tail = if args.is_empty() {
            String::new()
        } else {
            format!("。{args}")
        };
        let _ = writeln!(s, "- {head}{usage}：{}{tail}", e.what);
    }
    if !decls.is_empty() {
        decls.sort();
        s.push_str("\n```ts\n");
        for (_, d) in &decls {
            let _ = writeln!(s, "{d}");
        }
        s.push_str("```\n");
    }
    s
}

/// 一份 SKILL.md 照口重打：每个命令段换成现打的，段外一字不动。交回 (重打的整份, 段外的那几行)。
fn regenerate(text: &str, rows: &[Entry]) -> (String, Vec<String>, BTreeSet<String>) {
    let mut out = String::new();
    let mut outside = Vec::new();
    let mut listed = BTreeSet::new();
    let mut lines = text.split_inclusive('\n');
    while let Some(line) = lines.next() {
        let Some(rest) = line.trim_end().strip_prefix(OPEN) else {
            outside.push(line.trim_end().to_string());
            out.push_str(line);
            continue;
        };
        let sel = rest
            .strip_suffix(" -->")
            .and_then(|r| r.split('（').next())
            .unwrap_or_else(|| panic!("命令段的头一行写坏了：{line}"))
            .trim()
            .to_string();
        let _ = writeln!(out, "{OPEN}{sel}（{NOTE}） -->");
        for e in pick(&sel, rows) {
            if let Some(f) = &e.cli {
                listed.insert(f.clone());
            }
        }
        out.push_str(&section(&sel, rows));
        assert!(
            lines.by_ref().any(|l| l.trim_end() == CLOSE),
            "命令段 `{sel}` 没有收尾的 `{CLOSE}`"
        );
        let _ = writeln!(out, "{CLOSE}");
    }
    (out, outside, listed)
}

/// 命令段以外的手抄还没清、在别的格子里改的那几份：(仓相对路径, 为什么不在这里改)。清完一份就删一行；文件不在了 ⇒ 红（删行）。
const ELSEWHERE: &[(&str, &str)] = &[(
    "src/shared/cc-bus/SKILL.md",
    "cc-bus 的 skill 另一格在改（命令那一段跟着那一格换成命令段）",
)];

fn skill_files() -> Vec<(String, String)> {
    let root = guard_core::addr::repo_root();
    let mut got = Vec::new();
    for d in std::fs::read_dir(root.join("src/shared")).unwrap() {
        let p = d.unwrap().path().join("SKILL.md");
        if let Ok(text) = std::fs::read_to_string(&p) {
            let rel = p.strip_prefix(&root).unwrap().to_string_lossy().into_owned();
            got.push((rel, text));
        }
    }
    got.sort();
    assert!(got.len() >= 3, "src/shared 底下的 SKILL.md 不足 3 份 —— 本断言在空转");
    got
}

/// 一行里提到的子命令（整词）。
fn flags_in(line: &str) -> Vec<&'static str> {
    crate::SUBCOMMANDS
        .iter()
        .copied()
        .filter(|f| {
            line.match_indices(f).any(|(at, _)| {
                let after = line[at + f.len()..].chars().next();
                let before = line[..at].chars().next_back();
                !after.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
                    && !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
            })
        })
        .collect()
}

#[test]
fn the_skill_command_sections_are_what_the_catalog_generates() {
    let rows = catalog();
    let mut red = Vec::new();
    let mut sections = 0;
    let files = skill_files();
    for (rel, _) in ELSEWHERE {
        assert!(
            files.iter().any(|(r, _)| r == rel),
            "`ELSEWHERE` 里的 {rel} 已经不在 —— 删那一行"
        );
    }
    for (rel, text) in files {
        let (fresh, outside, listed) = regenerate(&text, &rows);
        sections += text.matches(OPEN).count();
        if let Some(e) = crate::golden::mismatch(&rel, &fresh) {
            red.push(format!("命令段与口现打的不一致：{e}"));
        }
        if ELSEWHERE.iter().any(|(r, _)| *r == rel) {
            continue;
        }
        for line in &outside {
            for f in flags_in(line) {
                if !listed.contains(f) {
                    red.push(format!("{rel}：命令段以外提到 `{f}`，本份命令段里没有它 —— 选进命令段：{line}"));
                }
            }
            let t = line.trim_start().trim_start_matches(['-', '*', '|', ' ']);
            let t = t.trim_start_matches('`').trim_start_matches("ccm -- ");
            if crate::SUBCOMMANDS.iter().any(|f| t.starts_with(f)) {
                red.push(format!("{rel}：命令段以外手抄了一行命令 —— 删掉，命令段里有：{line}"));
            }
        }
    }
    assert!(sections >= 2, "带命令段的 SKILL.md 不足 2 段 —— 本断言在空转");
    assert!(red.is_empty(), "{}", red.join("\n"));
}

/// 量具自检：重打不碰段外、段内照口写、手抄的那一行认得出。
#[test]
fn the_section_generator_rewrites_only_inside_the_markers() {
    let rows = catalog();
    let text = format!("头\n{OPEN}quota-read -->\n旧的\n{CLOSE}\n- `--quota-read`：手抄的\n尾\n");
    let (fresh, outside, listed) = regenerate(&text, &rows);
    assert!(fresh.starts_with("头\n"));
    assert!(fresh.ends_with(&format!("{CLOSE}\n- `--quota-read`：手抄的\n尾\n")));
    assert!(fresh.contains("- `ccm -- --quota-read`："), "{fresh}");
    assert!(!fresh.contains("旧的"));
    assert!(listed.contains("--quota-read"));
    assert_eq!(flags_in(&outside[1]), vec!["--quota-read"]);
    assert!(flags_in("`--quota-reader`").is_empty(), "整词才算");
    let (again, _, _) = regenerate(&fresh, &rows);
    assert_eq!(again, fresh, "重打一遍不变");
}
