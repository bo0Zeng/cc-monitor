#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R113` 的刀具 —— 一族：**门禁刀**（`apply <刀名>` → 跑一趟沙箱门禁 → `restore`）。

被测对象全部是 **Rust 判据**，只有沙箱门禁的 `cargo` / `daemon` 两格判得了它们。
**一趟一刀。**

住址：本文件住 `<本工作树>/evidence/`；`ROOT` = **它所在的那棵树的仓根**
（`brief` 12：量具住址要唯一定位到那一份被测对象，不许拿 `cwd` 猜）。
⚠ 与 `K-R112-cut.py` / `K-R109-cut.py` 是**不同的量具**（被测对象是不同的树、不同的刀谱）。
跑之前先看它印的「被测对象」那一行。

🔴 本量具**不自己跑门禁**：跑门禁只有一条许可命令（从项目根起跑
`PB_WS=backend-consolidation .claude/devbox/gate <工作树> <tag>`），本文件只管**落刀与还原**。

纪律（每一条都是前面几件真踩出来的）：
- 🔴 **fail-closed**：每处编辑先断言锚点**恰好命中 N 次**，差一次 `exit 3`，**一个字节都不落地**。
- 🔴 同一份文件的多处替换在内存里做完再写一次盘，写完**回读逐处核对**。
- 🔴 **还原不许 `cp -a`**（连 mtime ⇒ cargo 判「没变」⇒ 读到上一刀的红）⇒ `restore` 是重写原文。
- 备份落 `evidence/.K-R113-cut-backup.json`（`apply` 写、`restore` 删）；
  已有备份还想 `apply` ⇒ 拒（上一刀没还原）。
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BACKUP = ROOT / "evidence" / ".K-R113-cut-backup.json"

CCBUS = "remote-daemon-proto/src/control/cc_bus.rs"
INBOUND = "remote-daemon-proto/src/inbound.rs"
MAIN = "remote-daemon-proto/src/main.rs"
BIDG = "remote-daemon-proto/src/build_id_guard.rs"

# 本件新加的判据（阴性对照要逐条停掉的那一份人群）。**这张名单只有一处住址**：
# 下面的 `D_JUDGES`；阴性对照刀与「退掉实现还有几条仍绿」那一栏用的是同一份。
D_JUDGES = [
    "every_shelled_out_command_carries_a_written_ruling",
    "bus_state_answers_both_halves_from_one_call",
    "parse_spawned_reads_the_table_and_keeps_the_three_states",
    "non_data_lines_never_become_spawned_rows",
    "the_three_spawned_states_stay_three_different_answers",
]

# ── 刀谱：{刀名: {文件: [(锚点, 替换, 该命中几次), …]}} ──────────────────────

# `KR113D1` ①：**表里加了、分派臂不加。**
#
# 这棵树上「分派臂」是**派生**的（`main` 那条 `cli_control::handles(f)`，
# 认哪几条由 `inbound::REGISTRY` 在运行期决定）⇒ 「不加分派臂」= 摘掉注册表那一条，
# 而 `--bus-state` 仍留在 `SUBCOMMANDS` 里。这正是 `K-R102` 那把总伞要罩的形。
D1_1 = {
    INBOUND: [
        (
            """    CommandSpec {
        name: "bus-state",
        doc_anchor: Some("#### `bus-state`"),
        codes: &["not_installed", "timed_out", "failed"],
        fields: &[
            "agents", "ccm_sid", "dir", "id", "live", "spawned", "target", "task", "unread",
        ],
        takes_input: false,
        run: Run::Blocking(|_r| crate::control::cc_bus::state_for_inbound().map(Some)),
    },
""",
            "",
            1,
        )
    ]
}

# `KR113D1` ②：**`BUILD_ID` 不 bump。**
#
# 「不 bump」的真实形状是「加了子命令、而 bump 那一拍整个没做」⇒ 两处一起退：
# `BUILD_ID` 退回上一版 ＋ `SUBCOMMAND_HISTORY` 那一行不追加。
# ⚠ 只退 `BUILD_ID` 一处**不会红**（护栏判的是「当前指纹在不在表里」，不是「等于最后一行」）
#   —— 那是它刻意的设计，别把它读成漏。
D1_2 = {
    MAIN: [('const BUILD_ID: &str = "p2j-bus-state";', 'const BUILD_ID: &str = "p2i-frame-tmux-primitives";', 1)],
    BIDG: [
        (
            """        (
            "p2j-bus-state",
            "--account-trust\\n--account-trust-zero\\n--bus-kill\\n--bus-list\\n--bus-send\\n--bus-state\\n--capture-pane\\n--daemon-probe\\n--dial\\n--fork-session\\n--kill\\n--launch\\n--list-accounts\\n--list-projects\\n--list-sessions\\n--list-subagents\\n--oneshot-session\\n--ping\\n--read-session\\n--read-session-from-offset\\n--read-session-tail\\n--relay\\n--resolve\\n--search\\n--session-accounts\\n--tmux-notify\\n--usage\\n#channel\\nch:bus-kill\\nch:bus-list\\nch:bus-send\\nch:bus-state\\nch:cancel\\nch:capture-pane\\nch:kill\\nch:launch\\nch:oneshot-session\\nch:ping\\nch:resolve",
        ),
""",
            "",
            1,
        )
    ],
}

# `KR113D1` ⓪：**「一次回全」只回一半。**
#
# 最小面：只动 `state_for_inbound` 的体，签名与返回类型一个字没改
#（`brief` 7：形状对、恒答其中一张脸 —— 不是 `return None` 那种把台子炸掉的换法）。
D1_0 = {
    CCBUS: [
        (
            """    Ok(serde_json::json!({
        "agents": agents_via_cc_list()?,
        "spawned": spawned_via_cc_agents()?
    }))""",
            """    Ok(serde_json::json!({
        "agents": agents_via_cc_list()?
    }))""",
            1,
        )
    ]
}

# 本件自加 ③：**三态压成两态** —— 把「核不了」并进「活着」。
# 这正是 `cc-agents` 头注逐字点名的那族事故的共同起点。
D1_4 = {
    CCBUS: [
        (
            "        SPAWNED_UNVERIFIED => Some(serde_json::Value::Null),",
            "        SPAWNED_UNVERIFIED => Some(serde_json::Value::Bool(true)),",
            1,
        )
    ]
}

# 本件自加 ④：**认不出的状态串不落选，猜一个具体答案。**
D1_5 = {
    CCBUS: [
        ("        _ => None,\n    }\n}", "        _ => Some(serde_json::Value::Bool(false)),\n    }\n}", 1)
    ]
}

# `KR113D2` 乙的死值验：**把那条登记删掉。**
# 删的是 `cc-agents` 那一行（本件新长出来的那条转调）——删掉之后盘上就再没有
# 「这条转调是裁过的」这句话，而转调本身还在。
D2_1 = {
    CCBUS: [
        (
            """        (
            "cc-agents",
            "只读",""",
            """        (
            "cc-agents-DELETED",
            "只读",""",
            1,
        )
    ]
}

# `KR113D2` 甲的**代价现打**：把 spawn 台账那一半改成 daemon 自己读文件。
#
# 这不是「验一条判据」，是**给「甲（收）」这条路量一个真实读数**：
# 收进后端的第一步就是绕到 cc-bus 背后读它的数据文件，而那条边界今天有人守着。
# ⚠ 形状对：签名与返回类型没动，仍回 `Result<Vec<Value>, (String, String)>`。
D2_2 = {
    CCBUS: [
        (
            """    let out = run("cc-agents", &[]).map_err(|(c, m)| (c.to_string(), m))?;""",
            """    let home = std::env::var("HOME").unwrap_or_default();
    let p = std::path::Path::new(&home).join(".cc-bus").join("spawned.tsv");
    let raw = std::fs::read_to_string(&p).unwrap_or_default();
    let _ = &raw;
    let out = run("cc-list", &[]).map_err(|(c, m)| (c.to_string(), m))?;""",
            1,
        )
    ]
}


def _ignore_cuts(path: str, names: list[str]) -> dict:
    return {
        path: [
            (f"    #[test]\n    fn {n}(", f'    #[test]\n    #[ignore = "阴性对照"]\n    fn {n}(', 1)
            for n in names
        ]
    }


def _merge(*ds: dict) -> dict:
    out: dict[str, list] = {}
    for d in ds:
        for k, v in d.items():
            out.setdefault(k, []).extend(v)
    return out


CUTS = {
    "d1-0": ("`KR113D1`：「一次回全」只回一半（最小面，只动 `state_for_inbound` 的体）", D1_0),
    "d1-1": ("`KR113D1` ①：`SUBCOMMANDS` 里加了 `--bus-state`、注册表里摘掉它（表里加了、分派臂不加）", D1_1),
    "d1-2": ("`KR113D1` ②：`BUILD_ID` 不 bump（连 `SUBCOMMAND_HISTORY` 那一行也不追加）", D1_2),
    "d1-3": (
        "`KR113D1` ③ 阴性对照：本件新加的判据整段停掉 ＋ 刀①",
        _merge(_ignore_cuts(CCBUS, D_JUDGES), D1_1),
    ),
    "d1-4": ("本件自加：三态压成两态（`活?` 并进 `活`）", D1_4),
    "d1-5": ("本件自加：认不出的状态串不落选，猜成 `已退`", D1_5),
    "d2-1": ("`KR113D2` 乙的死值验：把 `cc-agents` 那条**登记**删掉（转调还在）", D2_1),
    "d2-2": ("`KR113D2` 甲的**代价现打**：spawn 台账那一半改成 daemon 自己读 `.tsv`", D2_2),
}


def _apply_text(cuts: dict) -> dict:
    """先全量断言锚点，再写盘。返回 {文件: 原文}。"""
    backup: dict[str, str] = {}
    staged: dict[str, str] = {}
    for rel, edits in cuts.items():
        p = ROOT / rel
        src = p.read_text(encoding="utf-8")
        backup[rel] = src
        cur = src
        for old, new, want in edits:
            got = cur.count(old)
            if got != want:
                print(f"❌ {rel}: 锚点命中 {got} 次（要 {want}）\n   锚点前 60 字：{old[:60]!r}")
                sys.exit(3)
            cur = cur.replace(old, new, want)
        staged[rel] = cur
    for rel, cur in staged.items():
        (ROOT / rel).write_text(cur, encoding="utf-8")
    for rel, edits in cuts.items():
        back = (ROOT / rel).read_text(encoding="utf-8")
        for _old, new, _n in edits:
            if new and new not in back:
                print(f"❌ {rel}: 写完回读找不到替换文本 —— 落盘没成功")
                sys.exit(3)
    return backup


def cmd_apply(name: str) -> int:
    if BACKUP.exists():
        print(f"❌ 已有备份 {BACKUP.name} —— 上一刀没还原。先 `restore`。")
        return 3
    if name not in CUTS:
        print(f"❌ 没有这把刀：{name}（有 {sorted(CUTS)}）")
        return 3
    why, cuts = CUTS[name]
    backup = _apply_text(cuts)
    BACKUP.write_text(
        json.dumps({"cut": name, "files": backup}, ensure_ascii=False), encoding="utf-8"
    )
    n = sum(len(v) for v in cuts.values())
    print(f"变异已落地：{name} —— {why}（{len(cuts)} 份文件 / {n} 处锚点，逐处命中数已断言）")
    print(f"被测对象：{ROOT}")
    print("下一步：从项目根跑一趟沙箱门禁，读完再 `restore`。")
    return 0


def cmd_restore() -> int:
    if not BACKUP.exists():
        print("· 没有备份，无需还原")
        return 0
    d = json.loads(BACKUP.read_text(encoding="utf-8"))
    for rel, src in d["files"].items():
        (ROOT / rel).write_text(src, encoding="utf-8")  # 重写原文 ⇒ mtime 必变
    BACKUP.unlink()
    print(f"已还原（刀 {d['cut']}，{len(d['files'])} 份文件；重写原文，mtime 已变）")
    return 0


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        print("门禁刀：", " ".join(sorted(CUTS)))
        return 0
    verb = sys.argv[1]
    if verb == "list":
        for k, (why, cuts) in sorted(CUTS.items()):
            n = sum(len(v) for v in cuts.values())
            print(f"  {k:6} [{len(cuts)} 份 / {n} 处] {why}")
        return 0
    if verb == "apply":
        return cmd_apply(sys.argv[2])
    if verb == "restore":
        return cmd_restore()
    print(f"❌ 不认识的动作：{verb}")
    return 3


if __name__ == "__main__":
    raise SystemExit(main())
