#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R3-C1 · `D4①` 全表：**仓里有多少条判据「读源码文本再下断言」**。

住址（唯一）：<工作树>/evidence/K-R3-C1-textguard-census.py
被测对象：本文件所在工作树（`WT` 由 `__file__` 推出 ⇒ 复跑不会指到另一棵树）
量于：输出头部打印 `git rev-parse HEAD`。

# 人群怎么切（先定义，再数 —— 这一格 `D4①` 逐字要求）

**单位 = 一个 `#[test]` 函数**（不是「一处 `production_code(` 调用」：
一条判据里可以调三次，而 `K-H2b §2a` 那个 `21` 数的是调用处，两个分母不是一回事）。

**入群判据**：该 `#[test]` 的**函数体**里出现下面任一「读源码文本」的原语，
或它调用了**同文件内**一个体里出现这些原语的辅助函数（**只闭一层**，这条边界写出来）：

    production_code( · production_source( · test_source(   ← 剥过的
    include_str!(  · read_to_string(                       ← 生料
    scan_tree!(    · scan_tree_excluding_self(             ← 生料（整棵树）
    files_by_extension( · shell_scripts( · assert_tree_strips_clean(

⚠ **不在群里**的（写出来，免得读大）：读**非源码**文本的判据（读 JSON / 读 md / 读日志）
本表按上面那张原语表机械地切，`read_to_string(` 那一支**必然混进**一些读非源码的
（量法即口径，别把它读成「一定是源码」）。

# 形怎么标

按函数体里出现的断言形状打标（一条判据可以同时带几形，报的是**多重集**）：

  不存在型  `assert!(!` + contains/find/matches/is_some
  等号型    `assert_eq!` / `assert_ne!`
  计数型    `.count()` / `.len()` 与 `>=` / `<=` / `>` / `<` 同现
  存在型    其余的 `assert!(` + contains/find_pinned/pin_line/contains_word/is_some

# 洞的方向（`D9` 09-01 订正过一次，照订正后的写）

  存在型 / 计数型 ⇒ 行尾注释能喂饱它 ⇒ **假绿**
  不存在型       ⇒ 行尾注释里的字面量把它打红 ⇒ **假红**（方向相反）
  等号型         ⇒ **没有这个洞**（多一段注释文本，等号先不成立）
"""
from __future__ import annotations

import collections
import hashlib
import pathlib
import re
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve()
WT = HERE.parent.parent

READERS = [
    ("production_code", "production_code("),
    ("production_source", "production_source("),
    ("test_source", "test_source("),
    ("include_str!", "include_str!("),
    ("read_to_string", "read_to_string("),
    ("scan_tree!", "scan_tree!("),
    ("scan_tree_excluding_self", "scan_tree_excluding_self("),
    ("files_by_extension", "files_by_extension("),
    ("shell_scripts", "shell_scripts("),
    ("assert_tree_strips_clean", "assert_tree_strips_clean("),
]
# 我这一刀（`production_code` 补行尾剥法）盖得到的，只有「剥过的」那三个原语
COVERED = {"production_code", "production_source", "test_source"}

FN_RE = re.compile(r"^([ \t]*)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)", re.M)


def fn_bodies(src: str):
    """粗切每个 `fn` 的体：从签名行的第一个 `{` 起按花括号配平。

    ⚠ 不解析字符串 / 注释里的花括号 —— 这是**粗刀**，量法写出来：
    个别函数的体可能被切长或切短，误差只影响归属，不影响「这条判据读不读源码」这一位
    （原语调用几乎总在体内靠前）。
    """
    out = {}
    for m in FN_RE.finditer(src):
        name = m.group(2)
        i = src.find("{", m.end())
        if i < 0:
            continue
        depth, end = 0, len(src)
        for k in range(i, len(src)):
            if src[k] == "{":
                depth += 1
            elif src[k] == "}":
                depth -= 1
                if depth == 0:
                    end = k + 1
                    break
        out.setdefault(name, []).append((m.start(), end, src[i:end]))
    return out


def tests_in(src: str):
    """返回 [(名字, 体)]，`#[test]` 与 `#[tokio::test]` 标注的函数。"""
    out = []
    for m in re.finditer(r"#\[(?:tokio::)?test\]", src):
        mm = FN_RE.search(src, m.end())
        if not mm:
            continue
        i = src.find("{", mm.end())
        if i < 0:
            continue
        depth, end = 0, len(src)
        for k in range(i, len(src)):
            if src[k] == "{":
                depth += 1
            elif src[k] == "}":
                depth -= 1
                if depth == 0:
                    end = k + 1
                    break
        out.append((mm.group(2), src[i:end]))
    return out


def readers_in(text: str):
    return {name for name, pat in READERS if pat in text}


def forms_in(body: str):
    f = set()
    probes = ("contains(", "contains_word(", "find_pinned(", "pin_line(",
              ".find(", ".matches(", ".is_some(")
    if re.search(r"assert!\(\s*!", body) and any(p in body for p in probes):
        f.add("不存在型")
    if "assert_eq!" in body or "assert_ne!" in body:
        f.add("等号型")
    if re.search(r"\.(count|len)\(\)", body) and re.search(r"[<>]=?", body):
        f.add("计数型")
    if "assert!(" in body and any(p in body for p in probes):
        f.add("存在型")
    return f or {"未分类"}


def rs_files():
    for root in ("src-tauri/src", "src-tauri/crates", "remote-daemon-proto/src"):
        d = WT / root
        if not d.is_dir():
            continue
        for p in sorted(d.rglob("*.rs")):
            yield p


def main() -> int:
    head = subprocess.run(["git", "-C", str(WT), "rev-parse", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    print(f"# 工作树 = {WT}")
    print(f"# HEAD = {head}")
    total_tests = 0
    pop = []          # (文件, 判据名, 原语集合, 形集合)
    for p in rs_files():
        src = p.read_text(encoding="utf-8", errors="replace")
        helpers = fn_bodies(src)
        helper_reads = {n: readers_in("".join(b for _, _, b in v))
                        for n, v in helpers.items()}
        for name, body in tests_in(src):
            total_tests += 1
            r = readers_in(body)
            # 只闭一层：体里点名了同文件里的辅助函数，且那个辅助函数读源码
            for hn, hr in helper_reads.items():
                if hn == name or not hr:
                    continue
                if re.search(r"\b" + re.escape(hn) + r"\s*\(", body):
                    r |= hr
            if r:
                looks_src = any(e in body for e in
                                (".rs\"", ".ts\"", ".tsx\"", ".sh\"", ".py\"",
                                 "\"rs\"", "\"ts\"", "\"sh\"", "\"py\""))
                pop.append((str(p.relative_to(WT)), name, r, forms_in(body), looks_src))

    print(f"# 全仓判据总数（分母）= {total_tests}"
          f"（量法：`#[test]` / `#[tokio::test]` 后紧跟的 fn，扫 src-tauri/src + src-tauri/crates"
          f" + remote-daemon-proto/src 下全部 .rs；**静态计数**，与门禁跑出来的数不是一回事："
          f"cfg 关掉的 / `#[ignore]` 的 / 别处的集成测试都影响后者）")
    print(f"# 其中「读源码文本再下断言」= {len(pop)}")
    covered = [x for x in pop if x[2] & COVERED]
    raw_only = [x for x in pop if not (x[2] & COVERED)]
    print(f"#   · 经**剥过的**原语读（production_code/source/test_source）= {len(covered)}"
          f"  ← 本拍这一刀盖得到")
    print(f"#   · **只**走生料原语读（include_str!/read_to_string/scan_tree! …）= {len(raw_only)}"
          f"  ← 盖不到，洞照旧开着")
    SRC_EXT = (".rs\"", ".ts\"", ".tsx\"", ".sh\"", ".py\"", "\"rs\"", "\"ts\"", "\"sh\"", "\"py\"")
    raw_src = [x for x in raw_only if x[4]]
    print(f"#     其中体里点名了源码扩展名（{SRC_EXT}）的 = {len(raw_src)}"
          f"；其余 {len(raw_only) - len(raw_src)} 条**判不了**读的是不是源码（量法只到这一层）")
    print()
    print("## 按原语分（一条判据可占多格 ⇒ 合计 > 判据数）")
    c = collections.Counter()
    for _, _, r, _, _ in pop:
        for x in r:
            c[x] += 1
    for k, v in c.most_common():
        print(f"  {k:26s} {v:4d}   {'（本刀盖得到）' if k in COVERED else '（本刀盖不到）'}")
    print()
    print("## 按形分（多重集；一条判据可带几形）")
    cf = collections.Counter()
    for _, _, _, f, _ in pop:
        for x in f:
            cf[x] += 1
    hole = {"存在型": "假绿", "计数型": "假绿", "不存在型": "假红", "等号型": "没有这个洞", "未分类": "判不了"}
    for k, v in cf.most_common():
        print(f"  {k:8s} {v:4d}   洞的方向 = {hole[k]}")
    print()
    if "--list" in sys.argv:
        print("## 逐条点名")
        for f, n, r, fm, _ in pop:
            print(f"  {f}::{n}\n      原语={sorted(r)} 形={sorted(fm)}")
    else:
        print("（逐条点名：加 --list）")
    print(f"# md5(本量具) = {hashlib.md5(HERE.read_bytes()).hexdigest()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
