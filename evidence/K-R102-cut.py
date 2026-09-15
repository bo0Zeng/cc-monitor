#!/usr/bin/env python3
"""K-R102 的**变异刀**（fail-closed）。一刀一趟，跑完必须 `--revert`。

被测对象（住址）
---------------
`<本文件所在工作树>/remote-daemon-proto/src/`。同 `K-R102-ruler.py`：按**自己的位置**推树，
不接受参数指树。备份落在 `<工作树>/.k-r102-cut-backup/`（只此一处，`--revert` 从它还原）。

用法
----
    python3 evidence/K-R102-cut.py --list
    python3 evidence/K-R102-cut.py <刀名>[,<刀名>…]   # 落刀（先断言锚点命中数，再改，再印「变异已落地」）
    python3 evidence/K-R102-cut.py --verify     # 只核「盘上现在是不是干净的」
    python3 evidence/K-R102-cut.py --revert     # 还原

fail-closed 的三处
------------------
1. **锚点命中数不等于登记的那个数 ⇒ 一个字节都不改**（brief 7）。
2. **上一刀没还原就不许落下一刀** —— 备份目录还在就拒。
3. **还原之后逐份核 md5** 与落刀前一致，对不上就吼；并把 mtime 推到现在
   （不推就会让下一趟不落刀的门禁复用上一刀的产物 —— 见 `do_revert` 里那段）。
"""

from __future__ import annotations

import hashlib
import os
import shutil
import sys
from pathlib import Path

WT = Path(__file__).resolve().parents[1]
SRC = WT / "remote-daemon-proto" / "src"
BACKUP = WT / ".k-r102-cut-backup"

MAIN = "main.rs"
HIST = "observe/history_query.rs"

# 每一刀：(被改的文件, 形状, 锚点, 锚点该命中几次)
#   形状 `line`  = 删掉**整行等于锚点**（trim 后）的那一行
#   形状 `span`  = 删掉从「trim 后等于锚点[0]」那一行到「trim 后等于锚点[1]」那一行（含两端）
CUTS: dict[str, tuple[str, str, object, int]] = {
    # D2 ①：摘掉一条子命令的分派臂，`SUBCOMMANDS` 那张表一个字不动
    "arm-relay": (
        MAIN,
        "line",
        "Some(\"--relay\") => relay::run(&agent_home, &args),",
        1,
    ),
    # D2 ②（反方向）：表里删一条，臂还在
    "table-relay": (MAIN, "line", '"--relay",', 1),
    # 覆盖「派生臂」那一路（共享一刀，走它的那几条同生共死）
    "arm-derived": (
        MAIN,
        "line",
        "Some(f) if control::cli_control::handles(f) => control::cli_control::run(&args).await,",
        1,
    ),
    # 覆盖「`_` 兜底臂」那一路：臂住在 history_query 自己那个 match 块里
    "arm-history-list-projects": (
        HIST,
        "line",
        "Some(\"--list-projects\") => list_projects(agent_home),",
        1,
    ),
    # D2 ③ 阴性对照的那一半：把本件新加的那条判据**整段**拿掉
    "unharden": (
        MAIN,
        "span",
        (
            "/// **同时**落在「字面量臂」与「派生臂」两条路上的那几条 —— `(token, 它为什么非要留自己那条臂)`。",
            "/// ★ 未知 `--flag` 不许把 daemon 踢出流模式。",
        ),
        1,
    ),
    # D2 ③ 阴性对照的另一半：`unharden` 只退判据本体，我加在隔壁那条判据**头注**里的交叉引用还留着
    #   ⇒ src-tauri 的 `structural_scan::every_dead_name_named_in_the_prose_is_declared_dead` 当场红
    #     （逐字「盘上 1 处，登记表写 0 处」）。那**不是牙，是刀不干净** ⇒ 同一趟连这段头注一起退。
    "unharden-doc": (
        MAIN,
        "span",
        (
            "/// # \u26a0 \u5b83\u5728\u300c**\u81c2\u5220\u4e86\u3001\u8868\u8fd8\u5728**\u300d\u8fd9\u4e00\u5f62\u4e0a**\u4e0d\u7ea2** —— \u8fd9\u4e0d\u662f\u7f3a\u9677\u767b\u8bb0\uff0c\u662f\u5b83\u7684\u6784\u9020",
            "/// \u21d2 \u90a3\u4e00\u5f62\u4eca\u5929\u7531 [`every_listed_subcommand_has_a_live_dispatch_route`] \u63a5\u4f4f\uff08`K-R102`\uff09\u3002",
        ),
        1,
    ),
    # D1 ①：把一条**已有伞**的判据摘掉 —— 尺子的分档读数必须跟着变
    "ruler-drop-capture-umbrella": (
        MAIN,
        "span",
        (
            "/// ★★ `KR86D1` 的**接线那一半**：`--capture-pane` 真的**够得到**那条原语。",
            "/// ★★ `KR87D1` 的**接线那一半**：`--oneshot-session` 真的**够得到**那条原语。",
        ),
        1,
    ),
}
# `span` 那一形：删到**第二个锚点之前**为止（第二个锚点自己留下）
SPAN_KEEPS_END = {"unharden", "ruler-drop-capture-umbrella"}


def sha(p: Path) -> str:
    return hashlib.md5(p.read_bytes()).hexdigest()


def hits_line(lines: list[str], anchor: str) -> list[int]:
    return [i for i, l in enumerate(lines) if l.strip() == anchor]


def cut_one(lines: list[str], name: str) -> tuple[list[str], int, str | None]:
    """算出这一刀之后的行；锚点数对不上就返回理由（**一个字节都不改**，brief 7）。"""
    _f, shape, anchor, want = CUTS[name]
    if shape == "line":
        at = hits_line(lines, anchor)  # type: ignore[arg-type]
        if len(at) != want:
            return lines, 0, f"锚点命中 {len(at)} 次（登记 {want}）：{anchor!r}"
        return [l for i, l in enumerate(lines) if i not in set(at)], len(at), None
    beg_a, end_a = anchor  # type: ignore[misc]
    b, e = hits_line(lines, beg_a), hits_line(lines, end_a)
    if len(b) != want or len(e) != want:
        return lines, 0, f"span 锚点命中 起 {len(b)} / 止 {len(e)}（各应 {want}）"
    i, j = b[0], e[0]
    if not i < j:
        return lines, 0, f"span 起点 {i} 不在止点 {j} 之前 —— 锚点次序不对"
    end = j if name in SPAN_KEEPS_END else j + 1
    return lines[:i] + lines[end:], end - i, None


def do_apply(names: list[str]) -> int:
    if BACKUP.exists():
        print(f"🔴 拒绝落刀：`{BACKUP}` 还在 —— 上一刀没还原。先 `--revert`。")
        return 3
    files = sorted({CUTS[n][0] for n in names})
    buf = {f: (SRC / f).read_text(encoding="utf-8").split("\n") for f in files}
    plan = []
    for n in names:
        f = CUTS[n][0]
        buf[f], removed, why = cut_one(buf[f], n)
        if why:
            print(f"🔴 {n}：{why} —— 一个字节都不改（整趟放弃）")
            return 3
        plan.append((n, f, removed))
    BACKUP.mkdir()
    lines_out = []
    for f in files:
        src = SRC / f
        dst = BACKUP / f.replace("/", "__")
        shutil.copy2(src, dst)
        lines_out.append(f"{f}\t{sha(src)}")
    (BACKUP / "MANIFEST").write_text(
        ",".join(names) + "\n" + "\n".join(lines_out) + "\n", encoding="utf-8"
    )
    for f in files:
        (SRC / f).write_text("\n".join(buf[f]), encoding="utf-8")
    for n, f, removed in plan:
        print(f"变异已落地：{n} · 文件 {f} · 删掉 {removed} 行")
    for f in files:
        print(f"  落刀后 md5：{f} = {sha(SRC / f)}")
    return 0


def do_revert() -> int:
    if not BACKUP.exists():
        print("🔴 没有备份可还原 —— 盘上要么本来就是干净的，要么有人手改过。自己核。")
        return 3
    rows = (BACKUP / "MANIFEST").read_text(encoding="utf-8").strip().split("\n")
    names, rows = rows[0], rows[1:]
    bad = []
    for r in rows:
        f, want_md5 = r.split("\t")
        # 🔴 **`copy2` 不许用在这一侧**：它连 mtime 一起还原 ⇒ 还原出来的文件比「上一趟带刀的
        #    构建」还旧 ⇒ **`cargo` 判它没变、直接复用带刀的产物**，下一趟不落刀的门禁跑的是
        #    **上一刀的二进制**，而输出长得和真跑了一模一样。
        #    〔09-13 实打踩过一次：`M6-final` 印 `daemon 755 passed`（本件那条判据在盘上、
        #     却一趟都没跑），与「判据被删掉」那一趟的读数**逐字相同**。〕
        #    ⇒ 用 `copyfile`（不带元数据）＋ 显式把 mtime 推到现在。
        shutil.copyfile(BACKUP / f.replace("/", "__"), SRC / f)
        os.utime(SRC / f, None)
        got = sha(SRC / f)
        print(f"已还原：{f} · md5 {got}")
        if got != want_md5:
            bad.append(f"{f}: {got} != {want_md5}")
    shutil.rmtree(BACKUP)
    if bad:
        print(f"🔴 还原之后 md5 对不上：{bad}")
        return 3
    print(f"（这一趟退掉的是：{names}）")
    return 0


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    a = sys.argv[1]
    if a == "--list":
        for k, (f, shape, _, want) in CUTS.items():
            print(f"{k:34s} {shape:5s} {f:26s} 锚点命中应 = {want}")
        return 0
    if a == "--revert":
        return do_revert()
    if a == "--verify":
        if BACKUP.exists():
            print("🔴 盘上不干净：备份目录还在 ⇒ 有一刀没还原")
            return 3
        print("干净：没有未还原的刀")
        return 0
    names = a.split(",")
    bad = [n for n in names if n not in CUTS]
    if bad:
        print(f"没有这几刀：{bad}（`--list` 看有哪几把）")
        return 2
    return do_apply(names)


if __name__ == "__main__":
    sys.exit(main())
