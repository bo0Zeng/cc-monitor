//! tmux 单发期限只一处：后端生产源码里起 tmux 的那几份，`Deadline` 字面量只许 `host/tmux.rs` 的 [`super::TMUX_WITHIN`]，
//! 外加下面登记的整条命令总预算（那是一条命令里几发 tmux 加起来的上限，不是一发的期限）。

/// 起 tmux 的认法：经 `host::tmux::child` 造命令 · 经 `via_shell` 跑观测脚本 · 经 `run_tmux_via` 装钩子；`host/tmux.rs` 自己恒算。
const STARTS_TMUX: [&str; 3] = ["tmux::child(", "tmux::via_shell(", "run_tmux_via("];

/// 单发期限的家。
const HOME: (&str, &str) = ("src/backend/host/tmux.rs", "TMUX_WITHIN");

/// 起 tmux 的文件里另有的整条命令总预算：（文件 · 常量 · 为什么不是单发期限）。
const BUDGETS: [(&str, &str, &str); 2] = [
    (
        "src/backend/control/terminals.rs",
        "TERMINALS_LIST_CAP",
        "终端名单整条命令的总预算（两种宿主各问一遍）",
    ),
    (
        "src/backend/control/terminals.rs",
        "TERMINAL_PREVIEW_CAP",
        "终端预览整条命令的总预算",
    ),
];

/// 一行里有没有「`Deadline::<构造>(<数字>`」那样的字面量期限。
fn literal_deadline(line: &str) -> bool {
    line.match_indices("Deadline::").any(|(i, m)| {
        let rest = &line[i + m.len()..];
        let Some(open) = rest.find('(') else {
            return false;
        };
        let ctor = &rest[..open];
        !ctor.is_empty()
            && ctor.chars().all(|c| c.is_ascii_lowercase() || c == '_')
            && rest[open + 1..]
                .trim_start()
                .starts_with(|c: char| c.is_ascii_digit())
    })
}

/// 第 `i` 行（或往上三行内）声明的常量名；不是常量声明 ⇒ 那一行原文。
fn named_at(lines: &[&str], i: usize) -> String {
    for back in 0..=3.min(i) {
        let l = guard_core::strip_visibility(lines[i - back].trim_start());
        if let Some(rest) = l.strip_prefix("const ") {
            return rest.split(':').next().unwrap_or("").trim().to_string();
        }
    }
    lines[i].trim().to_string()
}

#[test]
fn every_tmux_shot_takes_the_one_deadline() {
    let files = guard_core::addr::tracked_under("src/backend", "rs");
    let root = guard_core::addr::repo_root();
    let mut tmux_files = 0usize;
    let mut homes = 0usize;
    let mut seen_budgets: Vec<(String, String)> = Vec::new();
    let mut stray: Vec<String> = Vec::new();
    for rel in &files {
        let src =
            std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("读不了 {rel}：{e}"));
        let prod = guard_core::production_code(&src);
        let lines: Vec<&str> = prod.lines().collect();
        for l in &lines {
            if guard_core::strip_visibility(l.trim_start())
                .starts_with(&format!("const {}:", HOME.1))
            {
                assert_eq!(
                    rel, HOME.0,
                    "`{}` 只许定在 {}，{rel} 又定了一份",
                    HOME.1, HOME.0
                );
                homes += 1;
            }
        }
        let starts = rel == HOME.0 || STARTS_TMUX.iter().any(|p| prod.contains(p));
        if !starts {
            continue;
        }
        tmux_files += 1;
        for (i, l) in lines.iter().enumerate() {
            if !literal_deadline(l) {
                continue;
            }
            let name = named_at(&lines, i);
            if (rel.as_str(), name.as_str()) == HOME {
                continue;
            }
            if BUDGETS
                .iter()
                .any(|(f, c, _)| (*f, *c) == (rel.as_str(), name.as_str()))
            {
                seen_budgets.push((rel.clone(), name));
                continue;
            }
            stray.push(format!("{rel}：{name}"));
        }
    }
    assert!(
        tmux_files >= 10,
        "起 tmux 的源码只认出 {tmux_files} 份 —— 认法（{STARTS_TMUX:?}）多半跟不上代码了，扫空了不算过"
    );
    assert!(
        stray.is_empty(),
        "起 tmux 的源码里另写了期限字面量（单发一律取 `host::tmux::{}`；整条命令的总预算登进 BUDGETS 写清理由）：\n{}",
        HOME.1,
        stray.join("\n")
    );
    assert_eq!(homes, 1, "`{}` 要在 {} 定且只定一次", HOME.1, HOME.0);
    for (f, c, _) in BUDGETS {
        assert!(
            seen_budgets
                .iter()
                .any(|(sf, sc)| (sf.as_str(), sc.as_str()) == (f, c)),
            "登记的总预算 {f}：{c} 找不到了（删了 · 改名 · 那份不再起 tmux）⇒ 从 BUDGETS 拿掉"
        );
    }
}

#[test]
fn literal_deadline_reads_the_shapes_in_use() {
    assert!(literal_deadline("const X: Deadline = Deadline::secs(5);"));
    assert!(literal_deadline(
        "const X: crate::platform::child::Deadline = crate::platform::child::Deadline::secs(5);"
    ));
    assert!(literal_deadline("    .run(Deadline::millis( 200))"));
    assert!(!literal_deadline("Deadline::millis(until.left_ms())"));
    assert!(!literal_deadline(
        "use crate::platform::child::{Child, Deadline};"
    ));
    assert!(!literal_deadline(".run(TMUX_WITHIN)"));
}
