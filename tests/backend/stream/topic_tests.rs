//! 主题表：线上名 ＝ serde 名；每个主题恰好一行；可丢的只从 tap 那条发、不可丢的只从 watcher 的出方向发；
//! 生产段造 `changed` 帧只经 `Frame::changed`。

use super::*;

#[test]
fn the_wire_name_is_the_serde_name() {
    for t in Topic::ALL {
        let v = serde_json::to_value(t).unwrap();
        assert_eq!(v.as_str(), Some(t.name()), "{t:?}");
    }
    let mut names: Vec<&str> = Topic::ALL.iter().map(|t| t.name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), Topic::ALL.len(), "主题名重了");
}

/// 带 `body` 的主题有上限、不带的上限是 0（表里两格不许说两样话）。
#[test]
fn a_body_has_a_cap_and_only_then() {
    for t in Topic::ALL {
        let s = t.spec();
        assert_eq!(s.body, s.body_cap > 0, "{t:?}：body 与 body_cap 对不上");
    }
}

/// 生产段里每个 `Topic::X` 出现在哪几份文件（只数发端那两份与本表）。
fn emitters(topic: &str) -> Vec<String> {
    let root = crate::guard_support::src_root();
    let needle = format!("Topic::{topic},");
    let mut out = Vec::new();
    for rel in ["stream/tap.rs", "observe/watcher.rs"] {
        let src = std::fs::read_to_string(root.join(rel)).expect("读源码");
        if crate::guard_support::production_code(&src).contains(&needle) {
            out.push(rel.to_string());
        }
    }
    out
}

/// 可丢性跟着发的那条路走：可丢的从 tap 那条（`stream/tap.rs`）发、不可丢的从 watcher 的出方向（`observe/watcher.rs`）发。
/// 表里改了可丢性却没换发端（或反过来）⇒ 红。
#[test]
fn lossy_topics_go_out_on_the_tap_and_the_rest_on_the_watcher() {
    for t in Topic::ALL {
        let variant = format!("{t:?}");
        let want = if t.spec().lossy {
            "stream/tap.rs"
        } else {
            "observe/watcher.rs"
        };
        assert_eq!(
            emitters(&variant),
            vec![want.to_string()],
            "{t:?} 的发端与可丢性对不上"
        );
    }
}

/// 生产段造 `changed` 帧只经 `Frame::changed`（上限在那一处判）：别处写 `Frame::Changed {` 字面量 ⇒ 红。
#[test]
fn changed_frames_are_built_in_one_place() {
    let root = crate::guard_support::src_root();
    let needle = format!("{}::{} {{", "Frame", "Changed");
    let mut hits = Vec::new();
    for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
        let code = crate::guard_support::production_code(&src);
        if code.contains(&needle) {
            hits.push(
                path.strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    assert_eq!(hits, vec!["stream/wire.rs".to_string()]);
}

/// IPC-PROTOCOL 的「`changed` 主题表」逐行 ＝ 本表（手写那一份不许与代码说两样话）。
#[test]
fn the_protocol_topic_table_is_this_table() {
    let doc =
        std::fs::read_to_string(crate::guard_support::repo_root().join("src/doc/IPC-PROTOCOL.md"))
            .expect("读 IPC-PROTOCOL.md");
    let (_, rest) = doc
        .split_once("<!-- topic-table:begin -->\n")
        .expect("没有主题表起点");
    let (table, _) = rest
        .split_once("<!-- topic-table:end -->")
        .expect("没有主题表终点");
    let yes = |x: bool| if x { "带" } else { "—" };
    let mut want =
        String::from("| topic | 可丢 | key | rev | body | 重问 |\n|---|---|---|---|---|---|\n");
    for t in Topic::ALL {
        let s = t.spec();
        let body = if s.body {
            format!("带（≤ {} B）", s.body_cap)
        } else {
            "—".to_string()
        };
        want.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | `{}` |\n",
            s.name,
            if s.lossy { "是" } else { "否" },
            yes(s.key),
            yes(s.rev),
            body,
            s.reask
        ));
    }
    assert_eq!(
        table, want,
        "IPC-PROTOCOL 的主题表与 stream/topic.rs 对不上"
    );
}

/// 每个主题的「重问」是一条登记了的命令。
#[test]
fn every_reask_is_a_registered_command() {
    for t in Topic::ALL {
        let r = t.spec().reask;
        assert!(
            crate::stream::inbound::REGISTRY.iter().any(|c| c.name == r),
            "{t:?} 的重问 `{r}` 不是登记了的命令"
        );
    }
}

/// 现算小成品的那几样 ⇒ 表里写着带 `body`（反过来不必：计划的 `body` 由发端给）。
#[test]
fn a_computed_body_is_a_declared_body() {
    for t in Topic::ALL {
        let s = t.spec();
        if s.ask != Ask::None {
            assert!(s.body, "{t:?} 现算小成品，表里却写不带 body");
        }
    }
}

/// 帧里的小成品就是重问那条命令的应答（同一个处理器跑出来的，实现只一处）：规则表 · 一个会话的任务清单 · 额度账；
/// 不现算的主题（账号清单要带 agent · 配置文件大 · 计划由发端给）⇒ 不带。沙箱家目录里那几份都是空的 / 不在。
#[test]
fn a_computed_body_is_the_reask_reply() {
    assert_eq!(
        crate::stream::topic_body::body_now(Topic::RotationRules, None, &crate::Tz::default()),
        crate::faces::rotation_face::answer_rules_read().ok(),
        "规则表的小成品不是 rotation-rules-read 的应答"
    );
    assert_eq!(
        crate::stream::topic_body::body_now(
            Topic::Tasks,
            Some("no-such-sid"),
            &crate::Tz::default()
        ),
        crate::faces::feature_face::answer(
            "tasks-list",
            &serde_json::json!({"sid": "no-such-sid"}),
            &crate::Tz::default()
        )
        .ok(),
        "任务清单的小成品不是 tasks-list 的应答"
    );
    assert_eq!(
        crate::stream::topic_body::body_now(Topic::Tasks, None, &crate::Tz::default()),
        None,
        "没 key 的任务清单没得问"
    );
    for t in [Topic::Accounts, Topic::Profiles, Topic::Plan] {
        assert_eq!(
            crate::stream::topic_body::body_now(t, Some("x"), &crate::Tz::default()),
            None,
            "{t:?} 不该现算"
        );
    }
}

/// 生产进程真把现算装上了：`main` 恰好一处装 `topic_body::body_now`（没装 ⇒ 帧里永远不带小成品、客户端照旧重问，不红任何别的判据）；
/// 发端只经那一层调，不直接引命令表那一份。
#[test]
fn main_installs_the_body_and_emitters_go_through_the_hook() {
    let root = crate::guard_support::src_root();
    let main = std::fs::read_to_string(root.join("main.rs")).expect("读 main.rs");
    let prod = crate::guard_support::production_code(&main);
    let install = format!("{}::install({})", "topic_hook", "topic_body::body_now");
    assert_eq!(
        prod.matches(&install).count(),
        1,
        "main.rs 不是恰好一处装上现算"
    );
    for rel in ["stream/tap.rs", "observe/watcher.rs"] {
        let src = std::fs::read_to_string(root.join(rel)).expect("读发端");
        let code = crate::guard_support::production_code(&src);
        assert!(
            !code.contains("topic_body"),
            "{rel} 直接引了现算那一份（该经 topic_hook）"
        );
        assert!(
            code.contains("topic_hook::body"),
            "{rel} 没经 topic_hook 现算"
        );
    }
}
