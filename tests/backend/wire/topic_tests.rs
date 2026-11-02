//! 主题表：线上名 ＝ serde 名；每个主题恰好一行；可丢的只从 tap 那条发、不可丢的只从 watcher 的出方向发；
//! 生产段造 `changed` 帧只经 `Frame::changed`。
//! 读命令表与各族的面（上层）⇒ 挂主 crate（`cross/wire.rs`）；全部主题那一份住 `wire::test_support::ALL`。

use crate::wire::test_support::ALL;
use crate::wire::topic::*;

#[test]
fn the_wire_name_is_the_serde_name() {
    for t in ALL {
        let v = serde_json::to_value(t).unwrap();
        assert_eq!(v.as_str(), Some(t.name()), "{t:?}");
    }
    let mut names: Vec<&str> = ALL.iter().map(|t| t.name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), ALL.len(), "主题名重了");
}

/// 带 `body` 的主题有上限、不带的上限是 0（表里两格不许说两样话）。
#[test]
fn a_body_has_a_cap_and_only_then() {
    for t in ALL {
        let s = t.spec();
        assert_eq!(s.body, s.body_cap > 0, "{t:?}：body 与 body_cap 对不上");
    }
}

/// 生产段里每个主题的发端在哪几份文件（造那一臂 `Changed::X {` · 或照主题现算 `Topic::X,`；只数发端那几份）。
fn emitters(topic: &str) -> Vec<String> {
    let root = guard_core::addr::backend_src_root();
    let needles = [format!("Changed::{topic} {{"), format!("Topic::{topic},")];
    let mut out = Vec::new();
    for rel in [
        "stream/tap.rs",
        "observe/watcher.rs",
        "stream/inbound/mod.rs",
    ] {
        let src = std::fs::read_to_string(root.join(rel)).expect("读源码");
        let code = guard_core::production_code(&src);
        if needles.iter().any(|n| code.contains(n.as_str())) {
            out.push(rel.to_string());
        }
    }
    out
}

/// 可丢性跟着发的那条路走：可丢的从 tap 那条（`stream/tap.rs`）发、不可丢的从 watcher 的出方向（`observe/watcher.rs`）发。
/// 表里改了可丢性却没换发端（或反过来）⇒ 红。
#[test]
fn lossy_topics_go_out_on_the_tap_and_the_rest_on_the_watcher() {
    for t in ALL {
        let variant = format!("{t:?}");
        // 只推给发起它的那条连接的那一种（登录作业收了）走那条连接的应答通道（不丢）：发端在分派那一层。
        let want = if t == Topic::Login {
            "stream/inbound/mod.rs"
        } else if t.spec().lossy {
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

/// 生产段把一臂装进 `changed` 帧只经 `Frame::changed`（上限在那一处判）：别处写 `Frame::Changed(<那一臂>)` 造帧 ⇒ 红
/// （`Frame::Changed(c) =>` · `if let Frame::Changed(c) =` 那种拆帧的不算）。
#[test]
fn changed_frames_are_built_in_one_place() {
    let root = guard_core::addr::backend_src_root();
    let needle = format!("Frame::{}(", "Changed");
    let literal = |code: &str| {
        code.lines().any(|line| {
            line.match_indices(&needle).any(|(at, _)| {
                let after = &line[at..];
                let close = after.find(')').map_or("", |i| &after[i + 1..]);
                let pattern = close.trim_start().starts_with("=>")
                    || close.trim_start().starts_with('=')
                    || close.trim_start().starts_with('|')
                    || line[..at].contains("let ")
                    || line[..at].contains("matches!");
                !pattern
            })
        })
    };
    let mut hits = Vec::new();
    for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
        let code = guard_core::production_code(&src);
        if literal(&code) {
            hits.push(
                path.strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    assert_eq!(hits, vec!["wire/mod.rs".to_string()]);
}

/// IPC-PROTOCOL 的「`changed` 主题表」逐行 ＝ 本表（手写那一份不许与代码说两样话）。
#[test]
fn the_protocol_topic_table_is_this_table() {
    let doc =
        std::fs::read_to_string(guard_core::addr::repo_root().join("src/doc/IPC-PROTOCOL.md"))
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
    for t in ALL {
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
    assert_eq!(table, want, "IPC-PROTOCOL 的主题表与 wire/topic.rs 对不上");
}

/// 每个主题的「重问」是一条登记了的命令。
#[test]
fn every_reask_is_a_registered_command() {
    for t in ALL {
        let r = t.spec().reask;
        assert!(
            crate::stream::inbound::REGISTRY.iter().any(|c| c.name == r),
            "{t:?} 的重问 `{r}` 不是登记了的命令"
        );
    }
}

/// 枚举的每一臂 ＝ 表里那一行：臂的 `topic` 就是那个主题；臂里有 `key` / `rev` / `body` 那几格恰好是表里写带的那几格（类型那一侧照生成的 TS 声明数）。
#[test]
fn each_arm_has_the_cells_the_table_says() {
    let decl = <crate::wire::Changed as ts_rs::TS>::decl(&ts_rs::Config::default());
    for t in ALL {
        let s = t.spec();
        let arm = crate::wire::Changed::bare(t, Some("k".into())).expect("每个主题都有那一臂");
        assert_eq!(arm.topic(), t, "{t:?} 那一臂说的是别的主题");
        let head = format!("{{ \"topic\": \"{}\",", s.name);
        let at = decl
            .find(&head)
            .unwrap_or_else(|| panic!("{t:?} 在 Changed 里没有那一臂：{decl}"));
        let cells = &decl[at + head.len()..];
        let cells = &cells[..cells.find('}').expect("那一臂没收尾")];
        for (cell, want) in [("key", s.key), ("rev", s.rev), ("body", s.body)] {
            let has = cells.contains(&format!(" {cell}:")) || cells.contains(&format!(" {cell}?:"));
            assert_eq!(has, want, "{t:?} 那一臂带不带 `{cell}` 与表对不上：{cells}");
        }
    }
}

/// 现算小成品的那几样 ⇒ 表里写着带 `body`；跑的就是表里「重问」那一条命令。
#[test]
fn a_computed_body_is_a_declared_body() {
    use crate::wire::typed::Command as _;
    for t in ALL {
        if crate::stream::topic_body::body_now(t, Some("x"), &crate::Tz::default()).is_some() {
            assert!(t.spec().body, "{t:?} 现算小成品，表里却写不带 body");
        }
    }
    for (t, cmd) in [
        (
            Topic::Quota,
            crate::faces::rotation_face::QuotaReadCmd::NAME,
        ),
        (
            Topic::RotationStrategies,
            crate::faces::rotation_view_face::RotationStrategies::NAME,
        ),
        (Topic::Tasks, crate::faces::feature_face::TasksListCmd::NAME),
    ] {
        assert_eq!(t.spec().reask, cmd, "{t:?} 现算跑的不是表里重问那一条");
    }
}

/// 帧里的小成品就是重问那条命令的应答（同一个处理器跑出来的，实现只一处）：策略目录 · 一个会话的任务清单 · 额度账；
/// 不现算的主题（账号清单要带 agent · 配置文件大 · 计划由发端给）⇒ 不带。沙箱家目录里那几份都是空的 / 不在。
#[test]
fn a_computed_body_is_the_reask_reply() {
    assert_eq!(
        crate::stream::topic_body::body_now(Topic::RotationStrategies, None, &crate::Tz::default()),
        {
            use crate::wire::typed::Handle;
            Some(crate::wire::Changed::RotationStrategies {
                body: crate::wire::spec::finish(
                    crate::faces::rotation_view_face::RotationStrategies::run(
                        Default::default(),
                        crate::wire::typed::Cx {
                            tz: crate::Tz::default(),
                            until: None,
                            needs: (),
                        },
                    ),
                )
                .ok(),
            })
        },
        "策略目录的小成品不是 rotation-strategies 的应答"
    );
    assert_eq!(
        crate::stream::topic_body::body_now(
            Topic::Tasks,
            Some("no-such-sid"),
            &crate::Tz::default()
        ),
        {
            use crate::wire::typed::Handle as _;
            Some(crate::wire::Changed::Tasks {
                key: "no-such-sid".to_string(),
                body: crate::wire::spec::finish(crate::faces::feature_face::TasksListCmd::run(
                    crate::faces::feature_face::TasksListReq {
                        sid: "no-such-sid".to_string(),
                    },
                    crate::wire::typed::Cx {
                        tz: crate::Tz::default(),
                        until: None,
                        needs: (),
                    },
                ))
                .ok(),
            })
        },
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

/// 生产进程真把现算装上了：进程入口（`stream/entry.rs::run`）恰好一处装 `topic_body::body_now`（没装 ⇒ 帧里永远不带小成品、客户端照旧重问，不红任何别的判据）；
/// 发端只经那一层调，不直接引命令表那一份。
#[test]
fn main_installs_the_body_and_emitters_go_through_the_hook() {
    let root = guard_core::addr::backend_src_root();
    let main = std::fs::read_to_string(root.join("stream/entry.rs")).expect("读 stream/entry.rs");
    let prod = guard_core::production_code(&main);
    let install = format!("{}::install({})", "topic_hook", "topic_body::body_now");
    assert_eq!(
        prod.matches(&install).count(),
        1,
        "stream/entry.rs 不是恰好一处装上现算"
    );
    // `login` 的小成品由发端自己给（作业怎么收的），不现算 ⇒ 分派那一层不在这里。
    for rel in ["stream/tap.rs", "observe/watcher.rs"] {
        let src = std::fs::read_to_string(root.join(rel)).expect("读发端");
        let code = guard_core::production_code(&src);
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
