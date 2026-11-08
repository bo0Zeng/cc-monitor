//! 计划读面的夹具：一份照 pb 0.7.2（`pb dump` 形状 8）写的合成 dump 与 `pb dump --node` · 一个假 pb 插件（入口脚本把夹具原样吐出来，并记下自己被怎么起的）。
//! 键与值的写法照 pb 0.7 的 `pb dump` 字段表（只对键的形状，不抄任何工作区的内容）。
//! 合成工作区：两个仓库 `alpha`（软件，登记在 `repos/alpha`）· `beta`（研究，只做计划、仓库还不在）；名字与正文全是合成的（「甲功能」之类），
//! 不引 pb 仓与任何真工作区的文件。工作区根在常量里写成占位 `/synthetic/ws`，[`dump`] 换成交进来的那个。
//!
//! alpha：顶层 A1 能力（A1-1 模块已签收 2 笔、依据它的是 A1-4 · A1-2 模块没签、`to: A9` 指着图上没有的节点（不通过）、
//! 它的文件是空的 · A1-3 测试（一份坏 · 一份没提交）· A1-4 验收记录（执行过，成了 with）· A1-5 验收记录 sets A1-2 已签收（还没执行，查都过了））·
//! A2 能力划成一个权力范围（负责人 SUB，在回看；A2-1 已签收 · A2-2 交回的那一条 sets A2 已签收，等上一级删掉划出那一项）·
//! A3 规矩 covers A1 · A4 规矩 covers project · A5 待定（还没定）· A6 待定（定了，A6-1 原话）· A7 能力划出了、还没建（pending_scopes）·
//! A8 元数据坏了（判不了的节点）· 排期只有划出（P3 划出 A2 · P5 划出 A7；先后是节点上的 after，见下面各节点的边）；`src/stray.txt` 无主；
//! 审图两份：人按的审 A1 整棵、审完报了 3 条 · SUB 不带编号按的、起不了。阶段五个（定架构 · 审架构 · 开发 · 审实现 · 回看）。
//! beta：B1 问题 · B1-1 来源（支持 1，依据它的是 B1-2）· B1-2 佐证；仓库不在 ⇒ `unowned.paths` 为 null、`error` 说为什么；
//! project 没人接着、不在当前仓库 ⇒ `handoff` 为 null、`handoff_error` 说为什么。研究那一张表没有加 · 去的值 ⇒ 不出进度。

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 主会话 id · 子 agent id · 对不上的 id（写进权力范围的负责人与节点的 signer）。
pub const MAIN: &str = "11111111-2222-3333-4444-555555555555";
pub const SUB: &str = "a0b1c2d3e4f5a6b7c";
pub const STRANGER: &str = "99999999-0000-0000-0000-000000000000";

/// 常量里工作区根的占位。
const WS: &str = "/synthetic/ws";

/// 软件那张表（插件那一份的形状；阶段说明 · 规则只留判据用得上的几条）。
fn software() -> Value {
    let labels = |w: &str| json!({"to": w, "with": w, "after": "排在后面", "covers": w, "file": "管文件", "replaces": "替换", "sets": "改状态"});
    let kind = |name: &str, w: &str, evidence: bool, values: Value| json!({"name": name, "graph": "架构", "stored_in": null, "evidence": evidence, "values": values, "edge_labels": labels(w)});
    json!({
        "name": "软件", "source": "plugin",
        "graphs": [
            {"name": "架构", "file": "图.md", "lifetime": "permanent", "prefix": null, "scope_by": null},
            {"name": "排期", "file": "排期.md", "lifetime": "temporary", "prefix": "P", "scope_by": "covers"}
        ],
        "kinds": [
            kind("能力", "连着", false, json!(["已签收", "decided"])),
            kind("模块", "实现", false, json!(["已签收", "decided"])),
            kind("测试", "测", false, json!(["decided"])),
            kind("验收记录", "签收", true, json!([])),
            kind("原话", "连着", true, json!([])),
            kind("规矩", "管", false, json!(["decided"])),
            kind("待定", "连着", false, json!(["decided"])),
            {"name": "排期", "graph": "排期", "stored_in": null, "evidence": false, "values": [], "edge_labels": labels("指向")}
        ],
        "phases": [
            {"name": "定架构", "description": "先定结构"},
            {"name": "审架构", "description": "审架构"},
            {"name": "开发", "description": "写代码"},
            {"name": "审实现", "description": "审实现"},
            {"name": "回看", "description": "回看整体，决定下一步"}
        ],
        "views": [{"name": "没签收的", "query": "subtree=this kind=能力,模块 state!=已签收"}],
        "rules": [
            {"moments": ["update"], "when": "when: state!=已签收 delegated=false | all_children: kind=能力,模块 state=已签收", "effect": "add", "arg": "已签收", "line": 33},
            {"moments": ["update"], "when": "when: state=已签收 delegated=false | any_child: kind=能力,模块 state!=已签收", "effect": "remove", "arg": "已签收", "line": 34},
            {"moments": ["update"], "when": "when: value=decided,-decided | source: kind!=原话", "effect": "deny", "arg": "decided 只由原话节点记", "line": 38},
            {"moments": ["write"], "when": "when: phase!=开发", "effect": "deny", "arg": "只在开发阶段改仓库、推远端、打 tag、发版", "line": 41},
            {"moments": ["claim"], "when": "", "effect": "show", "arg": ["where"], "line": 42},
            {"moments": ["move"], "when": "", "effect": "move_to", "arg": "next", "line": 47}
        ]
    })
}

/// 研究那张表（没有加 · 去状态的规则）。
fn research() -> Value {
    let labels = |w: &str| json!({"to": w, "with": w, "after": "排在后面", "covers": w, "file": "管文件", "replaces": "替换", "sets": "改状态"});
    let kind = |name: &str, w: &str, evidence: bool, values: Value| json!({"name": name, "graph": "知识", "stored_in": null, "evidence": evidence, "values": values, "edge_labels": labels(w)});
    json!({
        "name": "研究", "source": "plugin",
        "graphs": [
            {"name": "知识", "file": "图.md", "lifetime": "permanent", "prefix": null, "scope_by": null},
            {"name": "排期", "file": "排期.md", "lifetime": "temporary", "prefix": "P", "scope_by": "covers"}
        ],
        "kinds": [
            kind("问题", "连着", false, json!(["decided"])),
            kind("来源", "连着", false, json!(["支持", "反对", "decided"])),
            kind("佐证", "连着", true, json!([])),
            kind("待定", "连着", false, json!(["decided"]))
        ],
        "phases": [
            {"name": "定题", "description": "拆问题"},
            {"name": "搜集", "description": "找来源"}
        ],
        "views": [],
        "rules": [
            {"moments": ["update"], "when": "when: value=支持,-支持 | source: kind!=佐证", "effect": "deny", "arg": "支持只由佐证写", "line": 34},
            {"moments": ["move"], "when": "", "effect": "move_to", "arg": "next", "line": 46}
        ]
    })
}

fn edges(
    to: &[&str],
    with: &[&str],
    after: &[&str],
    covers: &[&str],
    file: &[&str],
    sets: Value,
) -> Value {
    json!({"to": to, "with": with, "after": after, "covers": covers, "file": file, "replaces": [], "sets": sets})
}

fn no_refs() -> Value {
    json!({"title": [], "body": []})
}

/// 一个节点（没写的键给空）。
#[expect(
    clippy::too_many_arguments,
    reason = "一个节点的几格照 pb dump 的键逐个给，判据读起来一行一格"
)]
fn node(
    id: &str,
    title: &str,
    kind: &str,
    graph: &str,
    body: &str,
    parent: Option<&str>,
    children: &[&str],
    edges: Value,
    scope: (&str, Option<&str>, Option<&str>),
    covered_by: &[&str],
) -> Value {
    let (children_in, assignee, signer) = scope;
    json!({
        "id": id, "title": title, "kind": kind, "graph": graph, "stored_in": ".planned-build/synthetic/图.md", "body": body,
        "parent": parent, "children": children, "edges": edges,
        "files": [], "state": [], "evidenced_by": [],
        "children_in": children_in, "assignee": assignee, "signer": signer,
        "covered_by": covered_by, "refs": no_refs()
    })
}

fn alpha() -> Value {
    let top = ("project", Some(MAIN), Some(MAIN));
    let a2 = ("A2", Some(SUB), Some(SUB));
    let none = json!([]);
    let in_a1 = ["A3", "A4"];
    let mut a1 = node(
        "A1",
        "甲功能",
        "能力",
        "架构",
        "照 A3 的公约；读写两半见 A1-1/2，`A2` 在代码里不算。",
        None,
        &["A1-1", "A1-2", "A1-3", "A1-4", "A1-5"],
        edges(&[], &[], &[], &[], &[], none.clone()),
        top,
        &in_a1,
    );
    // 「A3」在码位 2..4；「A1-1/2」在 15..21（简写两个）；行内代码里的 `A2` 不算。
    a1["refs"]["body"] = json!([{"start": 2, "end": 4, "ids": ["A3"]}, {"start": 15, "end": 21, "ids": ["A1-1", "A1-2"]}]);
    let mut a1_1 = node(
        "A1-1",
        "甲的读入",
        "模块",
        "架构",
        "读入。",
        Some("A1"),
        &[],
        edges(&[], &["A1"], &[], &[], &["src/read.txt"], none.clone()),
        top,
        &in_a1,
    );
    a1_1["files"] = json!([{"path": "src/read.txt", "file_check": "present", "note": "3 字", "chars": 3, "bytes": 3}]);
    a1_1["state"] = json!([{"value": "已签收", "count": 2}]);
    a1_1["evidenced_by"] = json!(["A1-4"]);
    let mut a1_2 = node(
        "A1-2",
        "甲的写出",
        "模块",
        "架构",
        "照 A1-1 读进来的写出去。",
        Some("A1"),
        &[],
        edges(
            &[],
            &["A1"],
            &["A1-1"],
            &[],
            &["src/write.txt"],
            none.clone(),
        ),
        top,
        &in_a1,
    );
    a1_2["files"] = json!([{"path": "src/write.txt", "file_check": "empty", "note": "0 字", "chars": 0, "bytes": 0}]);
    a1_2["refs"]["body"] = json!([{"start": 2, "end": 6, "ids": ["A1-1"]}]);
    let mut a1_3 = node(
        "A1-3",
        "测读入",
        "测试",
        "架构",
        "测三种情况。",
        Some("A1"),
        &[],
        edges(&[], &["A1-1"], &[], &[], &["tests/a.txt"], none.clone()),
        top,
        &in_a1,
    );
    a1_3["edges"]["file"] = json!(["tests/a.txt", "tests/b.txt"]);
    a1_3["files"] = json!([
        {"path": "tests/a.txt", "file_check": "invalid", "note": "是个目录", "chars": null, "bytes": null},
        {"path": "tests/b.txt", "file_check": "uncommitted", "note": "没跟踪 · 4 个字符", "chars": 4, "bytes": 4}
    ]);
    let a1_4 = node(
        "A1-4",
        "签收甲的读入",
        "验收记录",
        "架构",
        "跑了 tests/a.txt，过了。",
        Some("A1"),
        &[],
        edges(&[], &["A1-1"], &[], &[], &[], none.clone()),
        top,
        &in_a1,
    );
    let a1_5 = node(
        "A1-5",
        "签收甲的写出",
        "验收记录",
        "架构",
        "写出照 A1-2 写的做了。",
        Some("A1"),
        &[],
        edges(
            &[],
            &[],
            &[],
            &[],
            &[],
            json!([{"target": "A1-2", "value": "已签收", "op": "add"}]),
        ),
        top,
        &in_a1,
    );
    let mut a2n = node(
        "A2",
        "乙功能",
        "能力",
        "架构",
        "乙那一半。",
        None,
        &["A2-1", "A2-2"],
        edges(&[], &[], &[], &[], &["src/shared.txt"], none.clone()),
        ("A2", Some(SUB), Some(MAIN)),
        &["A4", "P3"],
    );
    a2n["files"] = json!([{"path": "src/shared.txt", "file_check": "present", "note": "6 字", "chars": 6, "bytes": 6}]);
    let mut a2_1 = node(
        "A2-1",
        "乙的一半",
        "模块",
        "架构",
        "乙的那一半。",
        Some("A2"),
        &[],
        edges(&[], &["A2"], &[], &[], &["src/shared.txt"], none.clone()),
        a2,
        &["A4", "P3"],
    );
    a2_1["files"] = a2n["files"].clone();
    a2_1["state"] = json!([{"value": "已签收", "count": 1}]);
    let a2_2 = node(
        "A2-2",
        "交回乙功能",
        "验收记录",
        "架构",
        "A2-1 已签收。",
        Some("A2"),
        &[],
        edges(
            &[],
            &[],
            &[],
            &[],
            &[],
            json!([{"target": "A2", "value": "已签收", "op": "add"}]),
        ),
        a2,
        &["A4", "P3"],
    );
    let a3 = node(
        "A3",
        "公约",
        "规矩",
        "架构",
        "守什么：甲那一块的公约。",
        None,
        &[],
        edges(&[], &[], &[], &["A1"], &[], none.clone()),
        top,
        &["A4"],
    );
    let a4 = node(
        "A4",
        "总则",
        "规矩",
        "架构",
        "守什么：全部。",
        None,
        &[],
        edges(&[], &[], &[], &["project"], &[], none.clone()),
        top,
        &["A4"],
    );
    let a5 = node(
        "A5",
        "要不要导出 csv",
        "待定",
        "架构",
        "问题：导出要不要 csv。",
        None,
        &[],
        edges(&[], &[], &[], &[], &[], none.clone()),
        top,
        &["A4"],
    );
    let mut a6 = node(
        "A6",
        "编码先只做 UTF-8",
        "待定",
        "架构",
        "问题：编码。",
        None,
        &["A6-1"],
        edges(&[], &[], &[], &[], &[], none.clone()),
        top,
        &["A4"],
    );
    a6["state"] = json!([{"value": "decided", "count": 1}]);
    a6["evidenced_by"] = json!(["A6-1"]);
    let a6_1 = node(
        "A6-1",
        "只做 UTF-8",
        "原话",
        "架构",
        "「先只做 UTF-8」",
        Some("A6"),
        &[],
        edges(&[], &["A6"], &[], &[], &[], none.clone()),
        top,
        &["A4"],
    );
    let mut a7 = node(
        "A7",
        "丙功能",
        "能力",
        "架构",
        "丙那一半。",
        None,
        &[],
        edges(&[], &[], &["A2"], &[], &["src/third.txt"], none.clone()),
        top,
        &["A4", "P5"],
    );
    a7["files"] = json!([{"path": "src/third.txt", "file_check": "missing", "note": "", "chars": null, "bytes": null}]);
    // 排期只剩划出（先后住节点上的 after）。
    let p = |id: &str, covers: &[&str]| {
        node(
            id,
            "划出",
            "排期",
            "排期",
            "",
            None,
            &[],
            edges(&[], &[], &[], covers, &[], json!([])),
            top,
            &[],
        )
    };
    json!({
        "name": "alpha", "domain": "软件", "path": "repos/alpha", "plan_only": false,
        "graphs": software()["graphs"], "kinds": software()["kinds"], "phases": software()["phases"],
        "views": software()["views"], "rules": software()["rules"],
        "top_level": ["A1", "A2", "A3", "A4", "A5", "A6", "A7"],
        "scopes": [
            {"id": "project", "root_nodes": ["project"], "dir": ".planned-build/alpha", "assignee": MAIN,
             "first_claimed_at": "2026-01-01T08:00:00+00:00", "created_at": "2026-01-01T07:00:00+00:00",
             "phase": "开发", "current_node": "A1-2", "listed": true,
             "handoff": "你负责 alpha 的 project：先 pb claim project <认领码>，再 pb where。认领码只在人登记这个仓库与重发时印出来；丢了请人敲 `pb reissue project`",
             "handoff_error": null},
            {"id": "A2", "root_nodes": ["A2"], "dir": ".planned-build/alpha/A2", "assignee": SUB,
             "first_claimed_at": "2026-01-01T10:00:00+00:00", "created_at": "2026-01-01T09:00:00+00:00",
             "phase": "回看", "current_node": "A2-1", "listed": true,
             "handoff": "你负责 alpha 的 A2 乙功能：先 pb claim A2 <认领码>，再 pb where。认领码只在划出与重发时印出来；丢了找上一级重发",
             "handoff_error": null}
        ],
        "pending_scopes": [{"id": "A7", "root_nodes": ["A7"]}],
        "unowned": {"paths": ["src/stray.txt"], "error": null},
        "check": {
            "read_errors": [{"file": ".planned-build/alpha/图.md", "line": 71, "message": "元数据那一行写不对：`with A1` 少了冒号"}],
            "unparsed_nodes": ["A8"],
            "failures": [{"rule": "边指着图上的节点", "message": "A1-2 --to--> A9：A9 不在图上", "scope": "project",
                          "fix": "改掉那条边，或者补上 A9", "refs": [{"start": 0, "end": 4, "ids": ["A1-2"]}]}],
            "unknown": [{"rule": "无主文件", "reason": "beta 的仓库不在"}]
        },
        "nodes": [a1, a1_1, a1_2, a1_3, a1_4, a1_5, a2n, a2_1, a2_2, a3, a4, a5, a6, a6_1, a7,
            p("P3", &["A2"]), p("P5", &["A7"])],
        "pending_sets": [
            {"source": "A1-5", "target": "A1-2", "value": "已签收", "op": "add", "blocked_reason": null, "blocked_code": null, "rule": null},
            {"source": "A2-2", "target": "A2", "value": "已签收", "op": "add",
             "blocked_reason": "源节点在 A2 那个权力范围（还划出着）：等上一级删掉划出那一项",
             "blocked_code": "source_delegated", "rule": null}
        ],
        "pending_replaces": [],
        "archived": [],
        "audits": [
            {"file": ".planned-build/.临时/审图/alpha/20260101-080000-人.md", "by": "person", "who": null,
             "started_at": "2026-01-01T08:00:00+00:00", "tree": "A1", "in_scopes": null, "due": 6,
             "nodes": ["A1", "A1-1", "A1-2", "A1-3", "A1-4", "A1-5"], "status": "done", "failed_reason": null, "check": 1, "findings": 3},
            {"file": ".planned-build/.临时/审图/alpha/20260101-100000-agent.md", "by": "agent", "who": SUB,
             "started_at": "2026-01-01T10:00:00+00:00", "tree": null, "in_scopes": ["A2"], "due": 2,
             "nodes": ["A2", "A2-1"], "status": "failed", "failed_reason": "审图 agent 起不来", "check": 0, "findings": null}
        ]
    })
}

fn beta() -> Value {
    let top = ("project", None, None);
    let mut b1_1 = node(
        "B1-1",
        "一份调查",
        "来源",
        "知识",
        "调查说要表头。",
        Some("B1"),
        &[],
        edges(&[], &["B1"], &[], &[], &[], json!([])),
        top,
        &[],
    );
    b1_1["state"] = json!([{"value": "支持", "count": 1}]);
    b1_1["evidenced_by"] = json!(["B1-2"]);
    json!({
        "name": "beta", "domain": "研究", "path": "beta", "plan_only": true,
        "graphs": research()["graphs"], "kinds": research()["kinds"], "phases": research()["phases"],
        "views": research()["views"], "rules": research()["rules"],
        "top_level": ["B1"],
        "scopes": [
            {"id": "project", "root_nodes": ["project"], "dir": ".planned-build/beta", "assignee": null,
             "first_claimed_at": null, "created_at": "2026-01-01T07:00:00+00:00",
             "phase": "定题", "current_node": null, "listed": true,
             "handoff": null, "handoff_error": "project 不在当前仓库：先请人敲 `pb repo beta`"}
        ],
        "pending_scopes": [],
        "unowned": {"paths": null, "error": "仓库 beta 不在盘上"},
        "check": {"read_errors": [], "unparsed_nodes": [], "failures": [], "unknown": []},
        "nodes": [
            node("B1", "导出该不该带表头", "问题", "知识", "问题：表头。", None, &["B1-1", "B1-2"],
                edges(&[], &[], &[], &[], &[], json!([])), top, &[]),
            b1_1,
            node("B1-2", "调查的佐证", "佐证", "知识", "见 B1-1。", Some("B1"), &[],
                edges(&[], &["B1-1"], &[], &[], &[], json!([])), top, &[])
        ],
        "pending_sets": [], "pending_replaces": [], "archived": [], "audits": []
    })
}

/// 一份 dump，工作区根是 `workspace`。
pub fn dump(workspace: &str) -> Value {
    json!({
        "pb": "0.7.2", "shape": 8, "workspace": workspace, "current_repo": "alpha", "auto_continue": false,
        "domains": [research(), software()],
        "repos": [alpha(), beta()]
    })
}

/// `pb dump --node alpha A1-2` 那一份。
pub fn node_view(workspace: &str) -> Value {
    json!({
        "pb": "0.7.2", "shape": 8, "workspace": workspace, "repo": "alpha", "id": "A1-2",
        "agent_view": "── A1-2 甲的写出（模块 · — · 元数据写在 .planned-build/alpha/图.md）\n> id: A1-2 | kind: 模块 | with: A1 | after: A1-1 | to: A9\n> file: src/write.txt\n\n照 A1-1 读进来的写出去。\n\nA1-2 的边 3 条\n  A1-2 --with--> A1 · A1 住 .planned-build/alpha/图.md\n待执行 1 条：\n  A1-5 sets A1-2 已签收\nA1-2 底下下一个可用编号：A1-2-1"
    })
}

/// `pb dump --node alpha A9` 那一份（图上没有的节点，带 `error`）。
pub fn node_view_missing(workspace: &str) -> Value {
    json!({"pb": "0.7.2", "shape": 8, "workspace": workspace, "repo": "alpha", "id": "A9", "error": "A9 不在 alpha 的图上"})
}

/// 同一份，只是 alpha 顶层算活的那几个（A1 · A2 · A7）都记成了已签收（顶层全签收 ⇒ 要人过目那一条）。
pub fn dump_top_signed(workspace: &str) -> Value {
    let mut d = dump(workspace);
    for c in d["repos"][0]["nodes"].as_array_mut().unwrap() {
        if ["A1", "A2", "A7"].iter().any(|i| c["id"] == *i) {
            c["state"] = json!([{"value": "已签收", "count": 1}]);
        }
    }
    d
}

/// 同一份，只是那个仓库登记在别处（`pb repo <路径>`：dump 每个仓库带 `path`，相对工作区根）。
pub fn dump_at(workspace: &str, path: &str) -> Value {
    let mut d = dump(workspace);
    d["repos"][0]["path"] = json!(path);
    d
}

/// 同一份，只是 alpha 这一刻读不成（pb 给 `error`，就只有上面那几样）。
pub fn dump_broken(workspace: &str) -> Value {
    let mut d = dump(workspace);
    d["repos"][0] = json!({"name": "alpha", "domain": "软件", "path": "repos/alpha", "plan_only": false, "error": "图.md 第 3 行：元数据缺 id"});
    d
}

/// 一份 dump 照给定的身份表加工成成品（摘要 `r0`、读到的时刻 0：判据里用不上它俩的那几处）。
pub fn made_with(
    doc: &Value,
    who: &dyn Fn(&str) -> crate::plan::Whose,
) -> cmd_wire::plan::PlanRead {
    crate::plan::product::make(doc, who, "r0".into(), 0)
}

/// 线上那一份（判据按键看成品时用：键名 · 有没有某一格都照线上说）。
pub fn as_json<T: serde::Serialize>(t: &T) -> Value {
    serde_json::to_value(t).unwrap()
}

/// 夹具的身份表：MAIN 是一个活着、在等你批准的会话（叫「甲会话」）；SUB 是 MAIN 底下的子 agent；别的都对不上。
pub fn who(id: &str) -> crate::plan::Whose {
    use crate::plan::{Live, Whose};
    let live = Some(Live {
        activity: Some(crate::agents::SessionActivity::NeedsYou),
        needs: Some(cmd_wire::needs::NeedsKind::Approve),
    });
    match id {
        MAIN => Whose::Session {
            sid: MAIN.into(),
            live,
        },
        SUB => Whose::Subagent {
            parent: MAIN.into(),
            parent_title: "甲会话".into(),
            live,
        },
        _ => Whose::Unknown,
    }
}

/// 身份表说这条会话活着、在等人。
pub fn waits_per(who: crate::plan::WhoPort, sid: &str) -> bool {
    use crate::plan::{Live, Whose};
    matches!(
        who(sid),
        Whose::Session {
            live: Some(Live {
                activity: Some(crate::agents::SessionActivity::NeedsYou),
                ..
            }),
            ..
        }
    )
}

/// 判据用的读入口（生产不用）：本包与帧面那几份判据都经它喂结局。
impl crate::plan::book::Book {
    /// [`ingest_with`](crate::plan::book::Book::ingest_with)，本子里没记过的「哪次进的那一步」就用这一次的 rev；
    /// 活会话表那一判照身份表说的讲（身份表说那条会话在等人 ⇒ 表也说在等）。
    pub fn ingest(
        &self,
        ran: crate::plan::dump::Ran,
        dir: &Path,
        who: crate::plan::WhoPort,
        now_ms: u64,
    ) -> Result<cmd_wire::plan::PlanRead, crate::plan::book::Miss> {
        self.ingest_with(
            ran,
            dir,
            who,
            &|sid| waits_per(who, sid),
            now_ms,
            &|_, _| None,
        )
    }
}

/// 进程环境里的 `PB_ID` 只许一条判据同时碰：设它的那条（起 pb 不传下去）与读它的那条（代敲认不认 agent）各持这把锁跑。
pub fn pb_id_env() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 在 `root` 底下造一个假 pb 插件：清单名字是 pb，入口脚本
/// ① 把自己的 argv · 当前目录 · 环境里有没有 `PB_ID` 记进 `<root>/ran.json`（最近那一次），argv 另追加一行进 `<root>/runs.log`（每一次）；
/// ② 当前目录（往上找）有 `.planned-build/` ⇒ `dump --node` 印 `<那个目录>/node.json`；`dump --refs` 把标准输入记进 `<root>/stdin.txt`、
/// 印 `<那个目录>/refs.json`；别的印 `<那个目录>/dump.json`；退 0。没有 ⇒ stderr 一句、退 3。
/// `dump --domains <目录>` 从那个目录（不在 ⇒ 往上第一个在的）往上找：找到 ⇒ 同上印 `dump.json`；找不到 ⇒ 印 `<root>/domains.json`（插件里那几档）、退 0。
/// 它认的 `--` 旗标就是真 pb 那边后端会发的那几个（`protocol_doc_guard` 的子进程旗标对拍读这份）。
/// `rc` 给了就不管上面，直接以它退出（造「没有 dump」「别的失败」）。
pub fn fake_pb(root: &Path, name: &str, rc: Option<i32>) -> PathBuf {
    let tools = root.join("skills/planned-build/tools");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::create_dir_all(root.join(".claude-plugin")).unwrap();
    std::fs::write(
        root.join(".claude-plugin/plugin.json"),
        json!({"name": name, "version": "0.0.0"}).to_string(),
    )
    .unwrap();
    let forced = rc.map_or("None".to_string(), |c| c.to_string());
    let ran = root.join("ran.json");
    std::fs::write(
        tools.join("entry.py"),
        format!(
            r#"import json, os, sys
open({ran:?}, "w").write(json.dumps({{"argv": sys.argv[1:], "cwd": os.getcwd(), "pb_id": "PB_ID" in os.environ}}))
open({runs:?}, "a").write(json.dumps(sys.argv[1:]) + "\n")
forced = {forced}
if forced is not None:
    sys.stderr.write("forced\n"); sys.exit(forced)
d = os.getcwd()
domains = sys.argv[1:3] == ["dump", "--domains"]
if domains and len(sys.argv) > 3:
    d = sys.argv[3]
    while not os.path.isdir(d) and os.path.dirname(d) != d:
        d = os.path.dirname(d)
while True:
    if os.path.isdir(os.path.join(d, ".planned-build")):
        how = sys.argv[1:3]
        if how == ["dump", "--refs"]:
            open({stdin:?}, "w", encoding="utf-8").write(sys.stdin.buffer.read().decode("utf-8"))
        f = "node.json" if how == ["dump", "--node"] else "refs.json" if how == ["dump", "--refs"] else "dump.json"
        sys.stdout.write(open(os.path.join(d, f)).read()); sys.exit(0)
    up = os.path.dirname(d)
    if up == d:
        if domains:
            sys.stdout.write(open({plugin_domains:?}).read()); sys.exit(0)
        sys.stderr.write("读不成：这里不是一个 pb 工作区\n"); sys.exit(3)
    d = up
"#,
            ran = ran.to_string_lossy(),
            runs = root.join("runs.log").to_string_lossy(),
            stdin = root.join("stdin.txt").to_string_lossy(),
            plugin_domains = root.join("domains.json").to_string_lossy(),
        ),
    )
    .unwrap();
    std::fs::write(
        root.join("domains.json"),
        json!({"pb": "0.7.2", "shape": 8, "workspace": null, "domains": [research(), software()]})
            .to_string(),
    )
    .unwrap();
    tools.join("entry.py")
}

/// 造一个工作区：`.planned-build/alpha/` ＋ 工作区配置 `.planned-build/.env` ＋ 一份 `dump.json` 与 `node.json`（假 pb 吐它们）。
/// 仓库目录照 dump 里头一个仓库的 `path`（没有 ⇒ 仓库名）。
pub fn workspace(root: &Path, doc: &Value) -> PathBuf {
    let repo = doc["repos"][0]["path"].as_str().unwrap_or("alpha");
    std::fs::create_dir_all(root.join(".planned-build/alpha")).unwrap();
    std::fs::create_dir_all(root.join(repo).join("src")).unwrap();
    std::fs::write(
        root.join(".planned-build/.env"),
        r#"{"current_repo": "alpha"}"#,
    )
    .unwrap();
    std::fs::write(root.join("dump.json"), doc.to_string()).unwrap();
    let ws = doc["workspace"].as_str().unwrap_or(WS);
    std::fs::write(root.join("node.json"), node_view(ws).to_string()).unwrap();
    root.to_path_buf()
}

/// 假 pb 一次一次被起时带的参数（`runs.log`，没起过 ⇒ 空）。
pub fn runs(root: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(root.join("runs.log"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
