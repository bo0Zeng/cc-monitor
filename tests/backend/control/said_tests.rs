//! 那几条命令被拒时的那一句（各条命令自己的「码 → 句」表）：金样里的每个码各一句、两两不同、句子里没有原话；
//! 后端知道的对象（会话名 · 收件人）在句子里（处理器原来那句进复制详情的原话：`kill_tests.rs` 那条）。

use super::{cc_bus_wire, kill, terminal_follow, terminals_wire};

fn golden(file: &str) -> serde_json::Value {
    let p = crate::guard_support::repo_root()
        .join("tests/__fixtures__")
        .join(file);
    serde_json::from_str(
        &std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}")),
    )
    .unwrap()
}

fn codes(g: &serde_json::Value, cmd: &str) -> Vec<String> {
    g[cmd]["codes"]
        .as_array()
        .unwrap_or_else(|| panic!("金样里 {cmd} 没有 codes"))
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn every_golden_code_has_its_own_sentence_without_raw_words() {
    let bus = golden("cc-bus-control.golden.json");
    let tmux = golden("tmux-control.golden.json");
    let term = golden("terminals.golden.json");
    type Table = Box<dyn Fn(&str) -> String>;
    let cases: Vec<(&str, Table, Vec<String>, Option<&str>)> = vec![
        (
            "kill",
            Box::new(|c| kill::said("demo-cc", c)),
            codes(&tmux, "kill"),
            Some("demo-cc"),
        ),
        (
            "terminal-preview",
            Box::new(terminals_wire::preview_said),
            codes(&term, "terminal-preview"),
            None,
        ),
        (
            "bus-send",
            Box::new(|c| cc_bus_wire::send_said("agent-a", c)),
            codes(&bus, "bus-send"),
            Some("agent-a"),
        ),
        (
            "bus-kill",
            Box::new(|c| cc_bus_wire::kill_said("agent-a", c)),
            codes(&bus, "bus-kill"),
            None,
        ),
        (
            "bus-spawn",
            Box::new(cc_bus_wire::spawn_said),
            codes(&bus, "bus-spawn"),
            None,
        ),
        (
            "bus-broadcast",
            Box::new(cc_bus_wire::broadcast_said),
            codes(&bus, "bus-broadcast"),
            None,
        ),
    ];
    for (cmd, table, codes, object) in cases {
        assert!(!codes.is_empty(), "{cmd}：金样里一个码都没有");
        let said: Vec<String> = codes.iter().map(|c| table(c)).collect();
        for (c, s) in codes.iter().zip(&said) {
            assert!(!s.trim().is_empty() && !s.contains('{'), "{cmd}/{c}：{s:?}");
            assert!(!s.contains(c.as_str()), "{cmd}/{c}：码上了句子：{s}");
            // `bad_id` 说的是 id 的形状不对（可能是发件人那一格），不点名收件人。
            if let (Some(o), false) = (object, c == "bad_id") {
                assert!(s.contains(o), "{cmd}/{c}：句子里没有对象 {o}：{s}");
            }
        }
        let distinct: std::collections::BTreeSet<&String> = said.iter().collect();
        // `bad_args` 与 `bad_target` 在读画面那一条是同一句（都是目标不对）；别的两两不同。
        let expect = if cmd == "terminal-preview"
            && codes.iter().any(|c| c == "bad_args")
            && codes.iter().any(|c| c == "bad_target")
        {
            said.len() - 1
        } else {
            said.len()
        };
        assert_eq!(
            distinct.len(),
            expect,
            "{cmd}：有两档被压成了同一句：{said:?}"
        );
    }
}

/// 没有表的命令照处理器那句：投出去那一层（`Fail::settle`）不按命令名换句子。
#[test]
fn the_wire_layer_keeps_the_handlers_sentence() {
    for (cmd, code) in [
        ("kill", "kill_failed"),
        ("bus-send", "rejected"),
        ("accounts-add", "bad_args"),
    ] {
        let f = crate::wire::spec::Fail::new(code, "处理器那句".to_string());
        let out = f.settle(cmd);
        assert_eq!(out.message, "处理器那句", "{cmd}/{code}");
    }
}

/// 订实时画面被拒：带版本 / 带上限的那两码处理器那句就是成品（照原样）；别的码各一句、没有原话。
#[test]
fn terminal_follow_refusals_are_said_by_the_backend() {
    for code in ["tmux_too_old", "too_many_follows"] {
        assert_eq!(terminal_follow::said(code), None, "{code}");
    }
    let said: Vec<String> = ["no_tmux", "not_known", "ambiguous", "unobservable"]
        .iter()
        .map(|c| terminal_follow::said(c).unwrap_or_else(|| panic!("{c} 没有句子")))
        .collect();
    let mut uniq = said.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), said.len(), "两码同句：{said:?}");
    assert_eq!(
        terminal_follow::said("bad_args"),
        terminal_follow::said("unobservable"),
        "认不出的码落「接不上」那一句"
    );
}
