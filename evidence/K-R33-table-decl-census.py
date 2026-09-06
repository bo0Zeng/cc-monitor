#!/usr/bin/env python3
"""K-R33 · `TABLE_DECLS` 那个闭集的**分母普查**量具（09-06 实现方现打）。

# 它回答什么

`scanning_guard_registry::every_registry_guard_keeps_its_reverse_half` 靠一个
**按名字**的闭集（`TABLE_DECLS`）认「这份文件带登记表」。本量具把三件事量出来：

1. `§0a` 那三个读数分别在**哪个分母**上才复得出来（PM 立件时没写清分母，两个读数用了
   两个不同的分母 —— 见 `--verbose` 的对照表）；
2. `const X: &[` 这个 **grep 口径**与「真的是一条 `const` 声明」之间差几条
   （差的那几条是**注释里/字符串里**提到一个声明的针，不是声明）；
3. 那条元判据**真正的**扫描面（只有一棵树）上，今天采到了谁。

# 🔴 被测对象是谁：由 `--root` 给，默认 = 本文件所在工作树的仓根

⚠ 报读数时**连 `--root` 一起写**：同一条命令在不同工作树上跑，答案不同，
而两次输出长得一模一样（`brief` 12：量具住址要能唯一定位到那一份 + 它指向哪棵树）。

# 跑法

    python3 evidence/K-R33-table-decl-census.py                # 量工作树上此刻的盘面
    python3 evidence/K-R33-table-decl-census.py --at ba35a8c   # 量某个提交（D1① 的分母就该这么钉）
    python3 evidence/K-R33-table-decl-census.py --list         # 逐条列出所有声明（给 D1② 逐条判词用）

🔴 **`--at` 不是可选的讲究**：本量具的语料里有 `scanning_guard_registry.rs` 自己，
而本件往它里面加了几行**长得像声明的合成夹具串** ⇒ 不带 `--at` 在改动之后跑，
「含本文件」那几行会比立件时**多几条**，而输出格式一模一样。
（★ 这正是本模块治的那一族：量具的语料里装着量具自己。）
"""

from __future__ import annotations

import argparse
import pathlib
import re
import subprocess
import sys

# `const X: &[` 的两种口径。差集就是「不是声明的那几条」。
GREP = re.compile(r"const\s+[A-Za-z0-9_]+\s*:\s*&\[")
DECL = re.compile(r"^\s*(?:pub(?:\([a-z]+\))? )?const\s+([A-Za-z0-9_]+)\s*:\s*(&\[.*)$")

# 那条元判据今天的闭集。🔴 **这里是第二份字面量** —— 本量具是一次性的普查工具，
# 唯一的住址仍是 `scanning_guard_registry.rs` 的 `TABLE_DECLS`；
# 拿本文件的数说事之前，先对一眼那边有没有变（`brief` 13b）。
CLOSED_SET = ("REGISTERED", "SITES", "SCHEDULING_SITES", "FORMS")

# 那条元判据**真正**扫的那一棵树（`scan_tree!` 的实参逐字）。
GUARD_SCAN_SUBTREE = "src-tauri/src"
# 它按构造摘掉的那一份（`scan_tree!` 用 `file!()` 摘除调用者自己）。
GUARD_SELF = "src-tauri/src/scanning_guard_registry.rs"

THREE_TREES = ("src-tauri/src", "remote-daemon-proto/src", "src-tauri/crates")
TWO_TREES = ("src-tauri/src", "remote-daemon-proto/src")


class Tree:
    """语料来源：工作树上的盘面（`at=None`），或某个提交（`at=<ref>`）。

    两者的读法必须**同一个口径**，否则「盘上现在」与「立件那一刻」两个数不可比。
    """

    def __init__(self, root: pathlib.Path, at: str | None):
        self.root, self.at = root, at
        if at:
            r = subprocess.run(
                ["git", "-C", str(root), "ls-tree", "-r", "--name-only", at],
                capture_output=True, text=True, check=True,
            )
            self._names = [x for x in r.stdout.split("\n") if x.endswith(".rs")]

    def label(self) -> str:
        return f"{self.root}  @ {self.at or '工作树盘面（未提交的改动也算）'}"

    def list_rs(self, subs, only_guard_names: bool):
        out = []
        if self.at:
            cand = [n for n in self._names if any(n.startswith(s + "/") for s in subs)]
        else:
            cand = []
            for sub in subs:
                d = self.root / sub
                if d.is_dir():
                    cand += [p.relative_to(self.root).as_posix() for p in d.rglob("*.rs")]
        for rel in sorted(cand):
            if only_guard_names and not (
                rel.endswith("_registry.rs") or rel.endswith("_guard.rs")
            ):
                continue
            out.append(rel)
        return out

    def read(self, rel: str) -> str:
        if self.at:
            return subprocess.run(
                ["git", "-C", str(self.root), "show", f"{self.at}:{rel}"],
                capture_output=True, text=True, check=True,
            ).stdout
        return (self.root / rel).read_text(encoding="utf-8", errors="replace")


def test_regions(src: str) -> str:
    """与 `scanning_guard_registry.rs` 的 `test_regions` **同形**（含它那个重叠语义）。

    ⚠ 那边一份文件里出现两次 `#[cfg(test)]` 就会把区段拼两份 ⇒ 文本里的出现次数翻倍。
    本量具只用 `in`（存在性），翻倍不影响判定；但拿它数「出现几次」会多一倍，别那么用。
    """
    out, i = [], 0
    while True:
        j = src.find("#[cfg(test)]", i)
        if j < 0:
            break
        e = src.find("\n}\n", j)
        out.append(src[j : (len(src) if e < 0 else e)])
        i = j + len("#[cfg(test)]")
    return "\n".join(out)


def census(tree: "Tree", rels):
    grep_hits = decls = 0
    rows = []
    for rel in rels:
        text = tree.read(rel)
        for i, line in enumerate(text.split("\n"), 1):
            if GREP.search(line):
                grep_hits += 1
            m = DECL.match(line)
            if m:
                decls += 1
                rows.append((rel, i, m.group(1), m.group(2)[:60]))
    return grep_hits, decls, rows


def main() -> int:
    here = pathlib.Path(__file__).resolve()
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(here.parent.parent), help="仓根（默认 = 本文件所在的那棵工作树）")
    ap.add_argument("--at", default=None, help="量哪个提交（不给 = 量工作树此刻的盘面）")
    ap.add_argument("--list", action="store_true", help="逐条列出声明（D1② 的输入）")
    a = ap.parse_args()
    tree = Tree(pathlib.Path(a.root).resolve(), a.at)
    print(f"# 被测对象：{tree.label()}")
    # 🔴 闭集是**本量具自己带的一份副本**，而它已经是 `K-R33` **改完之后**的那一份。
    #    ⇒ 「闭集认出」这一列拿去与立件前的读数对比时，先看清这一行印的是哪几个名字：
    #      少一个 `FORMS` 就少认一条（`local_backend.rs`），而两次输出格式一模一样。
    print(f"# 本趟用的闭集（{len(CLOSED_SET)} 个名字）：{' · '.join('const %s:' % n for n in CLOSED_SET)}")

    print("\n## 一 · 三个分母各自量出什么")
    print(f"{'分母':46s} {'份数':>5s} {'grep 口径':>9s} {'真声明':>7s} {'闭集认出':>8s}")
    cases = [
        ("三棵树 `*_registry.rs`/`*_guard.rs`（含本文件）", THREE_TREES, True, False),
        ("同上，剔掉元判据自己那一份", THREE_TREES, True, True),
        ("三棵树全部 `.rs`", THREE_TREES, False, False),
        ("两棵树全部 `.rs`（不含 src-tauri/crates）", TWO_TREES, False, False),
    ]
    for label, subs, guards_only, drop_self in cases:
        rels = tree.list_rs(subs, guards_only)
        if drop_self:
            rels = [r for r in rels if r != GUARD_SELF]
        g, d, rows = census(tree, rels)
        hit = sum(1 for r in rows if r[2] in CLOSED_SET)
        print(f"{label:46s} {len(rels):5d} {g:9d} {d:7d} {hit:8d}")

    print("\n## 二 · grep 口径里**不是声明**的那几条（注释 / 字符串里提到一个声明）")
    rels = [r for r in tree.list_rs(THREE_TREES, True) if r != GUARD_SELF]
    for rel in rels:
        for i, line in enumerate(tree.read(rel).split("\n"), 1):
            if GREP.search(line) and not DECL.match(line):
                print(f"  {rel}:{i}  {line.strip()[:110]}")
            # 行首锚定得住、但整行住在一个字符串字面量里的那一种（反斜杠转义是它的指纹）。
            elif DECL.match(line) and "\\n" in line:
                print(f"  {rel}:{i}  〔住在字符串里〕{line.strip()[:110]}")

    print(f"\n## 三 · 那条元判据**真正**的扫描面（只有 `{GUARD_SCAN_SUBTREE}`，且摘掉自己）")
    scan = [r for r in tree.list_rs((GUARD_SCAN_SUBTREE,), False) if r != GUARD_SELF]
    pop, cand = [], []
    for rel in scan:
        regs = test_regions(tree.read(rel))
        if GREP.search(regs):
            cand.append(rel)
        if any(f"const {n}:" in regs for n in CLOSED_SET):
            pop.append(rel)
    print(f"  扫描面 {len(scan)} 份 · 测试段里带 `const X: &[` 的 {len(cand)} 份 · 闭集采到 {len(pop)} 份")
    for p in pop:
        print(f"    {p}")

    if a.list:
        print("\n## 四 · 逐条声明（D1② 的输入）")
        _, _, rows = census(tree, rels)
        for k, (rel, ln, name, ty) in enumerate(rows, 1):
            print(f"[{k:03d}] {rel}:{ln} {name} : {ty}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
