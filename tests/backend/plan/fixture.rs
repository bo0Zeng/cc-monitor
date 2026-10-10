//! 计划读面的夹具：一份合成的 `pb dump`（形状 1）· 一个假 pb 插件（入口脚本把夹具原样吐出来，并记下自己被怎么起的）。
//! 名字与正文全是合成的（片名 `alpha` · 格题「甲功能」之类），不引 pb 仓的任何文件。

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 主会话 id · 子 agent id · 对不上的 id。
pub(crate) const MAIN: &str = "11111111-2222-3333-4444-555555555555";
pub(crate) const SUB: &str = "a0b1c2d3e4f5a6b7c";
pub(crate) const STRANGER: &str = "99999999-0000-0000-0000-000000000000";

/// 一份合成的 dump：一片 `alpha`（软件），四格：顶层 A1（两个子 A1-1 · A1-2）· 顶层 A2（不做了）；
/// A1-2 有一条 with 指着 A1-1，两格声明同一份文件；A1-1 签过两次（后签的作数）；派出去一块 `B` 归子 agent。
pub(crate) fn dump(workspace: &str) -> Value {
    json!({
        "pb": "0.2.0", "shape": 1, "workspace": workspace, "repo": "alpha", "auto": true,
        "slices": [{
            "name": "alpha", "domain": "软件",
            "kinds": [
                {"name": "能力", "edge_words": {"to": "指向", "with": "连着", "after": "排在后面", "replaces": "顶掉"}},
                {"name": "模块", "edge_words": {"to": "指向", "with": "实现", "after": "排在后面", "replaces": "顶掉"}}
            ],
            "phases": [
                {"name": "定架构", "does": "写图", "marks": []},
                {"name": "回看", "does": "看全局", "marks": ["看全局"]}
            ],
            "done": false, "top": ["A1", "A2"],
            "blocks": [
                {"id": "project", "cells": ["project"], "dir": ".planned-build/alpha", "owner": MAIN, "phase": "回看", "at": null, "row": true},
                {"id": "B", "cells": ["A1"], "dir": ".planned-build/alpha/B", "owner": SUB, "phase": "定架构", "at": "A1-2", "row": true}
            ],
            "check": {"unreadable": [], "red": [{"rule": "悬空", "what": "A1-2 指着 A9", "block": "B", "fix": "改成一个在的编号"}], "undecidable": []},
            "cells": [
                {"id": "A1", "title": "甲功能", "kind": "能力", "body": "照 A2 的公约。", "parent": null, "children": ["A1-1", "A1-2"],
                 "edges": {"to": [], "with": [], "after": [], "replaces": []}, "files": [],
                 "status": "没做完", "why": "里面 1/2 做完了", "signs": [], "owner": SUB,
                 "refs": {"title": [], "body": [{"start": 2, "end": 4, "ids": ["A2"]}]},
                 "agent_view": "── A1 甲功能（能力 · 没做完）\n> id: A1"},
                {"id": "A1-1", "title": "甲的读入", "kind": "模块", "body": "读入。", "parent": "A1", "children": [],
                 "edges": {"to": [], "with": [], "after": [], "replaces": []},
                 "files": [{"path": "src/read.txt", "state": "在", "note": "12 字"}],
                 "status": "做完了", "why": null,
                 "signs": [
                    {"at": "2026-01-01T00:00:00Z", "by": SUB, "reason": "第一次", "refs": []},
                    {"at": "2026-01-02T00:00:00Z", "by": STRANGER, "reason": "再签", "refs": []}
                 ],
                 "owner": SUB, "refs": {"title": [], "body": []}, "agent_view": "── A1-1 甲的读入"},
                {"id": "A1-2", "title": "甲的写出", "kind": "模块", "body": "写出。", "parent": "A1", "children": [],
                 "edges": {"to": [], "with": ["A1-1"], "after": ["A1-1"], "replaces": []},
                 "files": [{"path": "src/read.txt", "state": "在", "note": "12 字"}, {"path": "src/write.txt", "state": "缺", "note": null}],
                 "status": "没做完", "why": "没签", "signs": [], "owner": SUB,
                 "refs": {"title": [], "body": []}, "agent_view": "── A1-2 甲的写出"},
                {"id": "A2", "title": "乙功能", "kind": "能力", "body": "不做了。", "parent": null, "children": [],
                 "edges": {"to": [], "with": [], "after": [], "replaces": []}, "files": [],
                 "status": "不做了", "why": null, "signs": [], "owner": MAIN,
                 "refs": {"title": [], "body": []}, "agent_view": "── A2 乙功能"}
            ],
            "archived": [{"id": "A3", "title": "丙功能", "kind": "能力", "replaced_by": "A2"}]
        }]
    })
}

/// 同一份，只是那一片这一刻读不成（pb 给 `error`，别的格都没有）。
pub(crate) fn dump_broken(workspace: &str) -> Value {
    json!({
        "pb": "0.2.0", "shape": 1, "workspace": workspace, "repo": "alpha", "auto": true,
        "slices": [{"name": "alpha", "domain": "软件", "error": "图.md 第 3 行：元行缺 id"}]
    })
}

/// 夹具的身份表：MAIN 是一个活着、在等你批准的会话；SUB 是 MAIN 底下的子 agent；别的都对不上。
pub(crate) fn who(id: &str) -> crate::plan::Whose {
    use crate::plan::{Live, Whose};
    let live = Some(Live {
        activity: Some(crate::agents::SessionActivity::NeedsYou),
        needs: Some(crate::observe::facts_query::NeedsKind::Approve),
    });
    match id {
        MAIN => Whose::Session {
            sid: MAIN.into(),
            live,
        },
        SUB => Whose::Subagent {
            parent: MAIN.into(),
            live,
        },
        _ => Whose::Unknown,
    }
}

/// 一次性目录。
pub(crate) fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("plan-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 在 `root` 底下造一个假 pb 插件：清单名字是 pb，入口脚本
/// ① 把自己的 argv · 当前目录 · 环境里有没有 `PB_ID` 记进 `<root>/ran.json`；
/// ② 当前目录（往上找）有 `.planned-build/` ⇒ 印出 `<那个目录>/dump.json`、退 0；没有 ⇒ stderr 一句、退 3。
/// `rc` 给了就不管上面，直接以它退出（造「没有 dump」「别的失败」）。
pub(crate) fn fake_pb(root: &Path, name: &str, rc: Option<i32>) -> PathBuf {
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
forced = {forced}
if forced is not None:
    sys.stderr.write("forced\n"); sys.exit(forced)
d = os.getcwd()
while True:
    if os.path.isdir(os.path.join(d, ".planned-build")):
        sys.stdout.write(open(os.path.join(d, "dump.json")).read()); sys.exit(0)
    up = os.path.dirname(d)
    if up == d:
        sys.stderr.write("读不成：这里不是一个 pb 工作区\n"); sys.exit(3)
    d = up
"#,
            ran = ran.to_string_lossy(),
        ),
    )
    .unwrap();
    tools.join("entry.py")
}

/// 造一个工作区：`.planned-build/alpha/` ＋ 工作区根上的 `.env` ＋ 一份 `dump.json`（假 pb 吐它）。
pub(crate) fn workspace(root: &Path, doc: &Value) -> PathBuf {
    std::fs::create_dir_all(root.join(".planned-build/alpha")).unwrap();
    std::fs::create_dir_all(root.join("alpha/src")).unwrap();
    std::fs::write(root.join(".env"), r#"{"仓库": "alpha"}"#).unwrap();
    std::fs::write(root.join("dump.json"), doc.to_string()).unwrap();
    root.to_path_buf()
}
