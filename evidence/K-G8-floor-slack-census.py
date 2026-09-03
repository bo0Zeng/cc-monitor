#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-G8 的尺子：把「断言数地板」这把量具的**三处登记**与**实得**摆在一张表上，
算出每一套的**余量 = 实得 − 地板**（＝今天可以被静默删掉的断言数）。

# 为什么要有这个文件

`K-R19` 的写区里有 `evidence/` 而它一个尺子没留 ⇒ 它报的三个数今天无法复算。
`K-R20` 顶回来立了规矩：**摸底拍留下的读数必须可复算**。本文件就是那把尺子。

# 它量什么、分母是什么（三处登记 —— 别混）

一套 e2e 的地板在本仓有**三处**独立登记，改地板要三处一起改（`ci.yml:527` 那条纪律）：

  A. `.github/workflows/ci.yml` 的**调用行** `run: bash e2e/assert-pass-floor.sh <套> <地板>`
     —— 这是 CI 真正执行的那一份。**分母 = 这些行的条数。**
  B. `.github/workflows/ci.yml` 的**覆盖面自检清单**（那个 `for pair in "<套> <地板>" …` 循环）
     —— 它只做「调用行里存不存在这个 `<套> <地板>` 字面量」的反向对拍，本身不跑套件。
  C. `scripts/gate.sh` 的 `run_e2e <套> <地板>` 行 —— **出货门禁**真正执行的那一份（子集）。

⚠ 三处的**人群不同**：A ⊇ C，B 应与 A 逐字同集。本脚本把三处分别数出来再对拍，
  **不假设它们一致** —— 不一致本身就是一条读数。

# 实得从哪来

`e2e/assert-pass-floor.sh` 只认收尾那行 `合计 PASS=<n> FAIL=<m>`（正则逐字照抄它的
`grep -oE '合计 PASS=[0-9]+'` + `tail -1`）。本脚本读**同一条正则**去解析套件运行日志，
所以「本脚本算的实得」与「门禁看见的实得」是同一个数，不是另一把尺子。

  ⚠ 抓不到那行 ⇒ 记 `None`，**不当 0、也不当通过** —— 与 `assert-pass-floor.sh`
    fail-closed 第 2 条同口径。

# 用法

  # ① 只看登记（不跑任何套件，秒回）
  python3 evidence/K-G8-floor-slack-census.py floors

  # ② 有了运行日志之后算余量；<日志目录> 下每套一个 `<套名>.log`
  python3 evidence/K-G8-floor-slack-census.py slack --runs <日志目录>

  # ③ 机读
  python3 evidence/K-G8-floor-slack-census.py floors --json

`--repo` 默认取本文件所在仓的根（`evidence/` 的上一级）。
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys

# `assert-pass-floor.sh:52` 逐字：grep -oE '合计 PASS=[0-9]+' … | tail -1
RE_TOTAL_PASS = re.compile(r"合计 PASS=(\d+)")
RE_TOTAL_FAIL = re.compile(r"合计 PASS=\d+ FAIL=(\d+)")

# A：ci.yml 的调用行。`ci.yml:516` 自己数的也是这个前缀（`run: bash e2e/assert-pass-floor\.sh`），
#    刻意**不用**宽模式：宽模式会把那几行 grep 自己算进来，数出一个比真值更大的假数。
RE_CI_CALL = re.compile(
    r"^\s*run:\s*bash\s+e2e/assert-pass-floor\.sh\s+(\S+)\s+(\d+)\s*$"
)
# B：ci.yml 覆盖面自检里的 `"<套> <地板>"` 字面量。只在那个 `for pair in` 块内取。
RE_CI_PAIR_ITEM = re.compile(r'"([A-Za-z0-9][\w.-]*) (\d+)"')
# C：scripts/gate.sh 的 run_e2e 行（**行首非注释**）。
RE_GATE_CALL = re.compile(r"^\s*run_e2e\s+(\S+)\s+(\d+)\s*$")


def repo_root(explicit: str | None) -> str:
    if explicit:
        return os.path.abspath(explicit)
    return os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))


def read_lines(path: str) -> list[str]:
    with open(path, encoding="utf-8") as fh:
        return fh.read().splitlines()


def scan_ci_calls(lines: list[str]) -> list[tuple[int, str, int]]:
    out = []
    for i, line in enumerate(lines, 1):
        m = RE_CI_CALL.match(line)
        if m:
            out.append((i, m.group(1), int(m.group(2))))
    return out


def scan_ci_pairs(lines: list[str]) -> list[tuple[int, str, int]]:
    """只扫覆盖面自检那个 `for pair in … ; do` 块，别把散文里的引号对当成登记。"""
    out: list[tuple[int, str, int]] = []
    inside = False
    for i, line in enumerate(lines, 1):
        stripped = line.strip()
        if stripped.startswith("for pair in"):
            inside = True
        if inside:
            for m in RE_CI_PAIR_ITEM.finditer(line):
                out.append((i, m.group(1), int(m.group(2))))
            if "; do" in line or stripped.endswith("do"):
                inside = False
    return out


def scan_gate_calls(lines: list[str]) -> list[tuple[int, str, int]]:
    out = []
    for i, line in enumerate(lines, 1):
        if line.lstrip().startswith("#"):
            continue
        m = RE_GATE_CALL.match(line)
        if m:
            out.append((i, m.group(1), int(m.group(2))))
    return out


def parse_log(path: str) -> tuple[int | None, int | None]:
    """返回 (实得 PASS, FAIL)。抓不到 `合计 PASS=` ⇒ (None, None)，不当 0。"""
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except OSError:
        return (None, None)
    passes = RE_TOTAL_PASS.findall(text)
    if not passes:
        return (None, None)
    fails = RE_TOTAL_FAIL.findall(text)
    return (int(passes[-1]), int(fails[-1]) if fails else None)


def collect(root: str) -> dict:
    ci_path = os.path.join(root, ".github", "workflows", "ci.yml")
    gate_path = os.path.join(root, "scripts", "gate.sh")
    ci_lines = read_lines(ci_path)
    gate_lines = read_lines(gate_path)

    calls = scan_ci_calls(ci_lines)
    pairs = scan_ci_pairs(ci_lines)
    gates = scan_gate_calls(gate_lines)

    return {
        "repo": root,
        "ci_calls": [{"line": ln, "suite": s, "floor": f} for ln, s, f in calls],
        "ci_pairs": [{"line": ln, "suite": s, "floor": f} for ln, s, f in pairs],
        "gate_calls": [{"line": ln, "suite": s, "floor": f} for ln, s, f in gates],
    }


def cross_check(data: dict) -> list[str]:
    """三处登记对拍。不一致本身就是读数，逐条吐出来。"""
    notes: list[str] = []
    ci = {d["suite"]: d["floor"] for d in data["ci_calls"]}
    pr = {d["suite"]: d["floor"] for d in data["ci_pairs"]}
    gt = {d["suite"]: d["floor"] for d in data["gate_calls"]}

    if len(ci) != len(data["ci_calls"]):
        notes.append("⚠ ci.yml 调用行里有同名套件重复出现 —— 本脚本按最后一条取值")
    for suite in sorted(set(ci) | set(pr)):
        a, b = ci.get(suite), pr.get(suite)
        if a is None:
            notes.append(f"✗ `{suite} {b}` 只在覆盖面清单里，**没有调用行** ⇒ CI 不跑它")
        elif b is None:
            notes.append(f"✗ `{suite} {a}` 有调用行，但**不在覆盖面清单**里 ⇒ 那条自检盖不住它")
        elif a != b:
            notes.append(f"✗ `{suite}`：调用行地板 {a} ≠ 覆盖面清单 {b}")
    for suite, g in sorted(gt.items()):
        a = ci.get(suite)
        if a is None:
            notes.append(f"✗ `{suite}` 在 scripts/gate.sh 上有地板 {g}，但 ci.yml 没有调用行")
        elif a != g:
            notes.append(f"✗ `{suite}`：ci.yml 地板 {a} ≠ scripts/gate.sh 地板 {g}")
    if not notes:
        notes.append("✓ 三处登记逐套同值")
    return notes


def cmd_floors(args) -> int:
    root = repo_root(args.repo)
    data = collect(root)
    if args.json:
        data["cross_check"] = cross_check(data)
        json.dump(data, sys.stdout, ensure_ascii=False, indent=2)
        print()
        return 0

    print(f"# 仓：{root}")
    print()
    print(f"## A · ci.yml 调用行（CI 真跑的那一份）：{len(data['ci_calls'])} 条")
    for d in data["ci_calls"]:
        print(f"  ci.yml:{d['line']:<5} {d['suite']:<26} 地板 {d['floor']}")
    print()
    print(f"## B · ci.yml 覆盖面自检清单（只做字面量对拍，不跑）：{len(data['ci_pairs'])} 条")
    for d in data["ci_pairs"]:
        print(f"  ci.yml:{d['line']:<5} {d['suite']:<26} 地板 {d['floor']}")
    print()
    print(f"## C · scripts/gate.sh 的 run_e2e（出货门禁真跑的那一份）：{len(data['gate_calls'])} 条")
    for d in data["gate_calls"]:
        print(f"  gate.sh:{d['line']:<4} {d['suite']:<26} 地板 {d['floor']}")
    print()
    print("## 三处对拍")
    for n in cross_check(data):
        print(f"  {n}")
    return 0


def cmd_slack(args) -> int:
    root = repo_root(args.repo)
    data = collect(root)
    ci = {d["suite"]: d["floor"] for d in data["ci_calls"]}
    gt = {d["suite"]: d["floor"] for d in data["gate_calls"]}

    rows = []
    for suite in sorted(set(ci) | set(gt)):
        log = os.path.join(args.runs, f"{suite}.log")
        got, failed = parse_log(log) if os.path.exists(log) else (None, None)
        floor_ci = ci.get(suite)
        floor_gate = gt.get(suite)
        floor = floor_gate if floor_gate is not None else floor_ci
        slack = None if (got is None or floor is None) else got - floor
        rows.append(
            {
                "suite": suite,
                "floor_ci": floor_ci,
                "floor_gate": floor_gate,
                "got": got,
                "fail": failed,
                "slack": slack,
                "log": log if os.path.exists(log) else None,
            }
        )

    if args.json:
        json.dump({"repo": root, "runs": args.runs, "rows": rows},
                  sys.stdout, ensure_ascii=False, indent=2)
        print()
        return 0

    print(f"# 仓：{root}   日志目录：{args.runs}")
    print("# 余量 = 实得 − 地板 = **今天可以被静默删掉的断言数**（`n -lt FLOOR` 只挡缩水）")
    print("# 实得 = None ⇒ 抓不到「合计 PASS=」，**不当 0、不当通过**（同 assert-pass-floor 第 2 条）")
    print()
    print(f"{'套件':<26} {'CI地板':>7} {'gate地板':>8} {'实得':>6} {'FAIL':>5} {'余量':>6}")
    print("-" * 66)
    measured = 0
    total_slack = 0
    for r in rows:
        g = "—" if r["got"] is None else str(r["got"])
        f = "—" if r["fail"] is None else str(r["fail"])
        s = "—" if r["slack"] is None else str(r["slack"])
        fg = "—" if r["floor_gate"] is None else str(r["floor_gate"])
        fc = "—" if r["floor_ci"] is None else str(r["floor_ci"])
        print(f"{r['suite']:<26} {fc:>7} {fg:>8} {g:>6} {f:>5} {s:>6}")
        if r["slack"] is not None:
            measured += 1
            total_slack += r["slack"]
    print("-" * 66)
    print(f"量到实得的：{measured}/{len(rows)} 套    这几套的余量合计：{total_slack} 条断言")
    print("⚠ 合计只对「量到的那几套」成立 —— 没量到的那几套余量**未知**，不是 0。")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="K-G8：断言数地板的登记与余量普查")
    ap.add_argument("--repo", default=None, help="仓根（默认：本文件的上一级）")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p1 = sub.add_parser("floors", help="只列三处登记并对拍（不跑套件）")
    p1.add_argument("--json", action="store_true")
    p1.set_defaults(func=cmd_floors)

    p2 = sub.add_parser("slack", help="结合运行日志算余量")
    p2.add_argument("--runs", required=True, help="日志目录，每套一个 <套名>.log")
    p2.add_argument("--json", action="store_true")
    p2.set_defaults(func=cmd_slack)

    args = ap.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
