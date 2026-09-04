#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-G8 第三把尺子：**同一套件跑多趟，实得稳不稳** —— 逐套对拍两趟（或多趟）的读数。

# 为什么要有它（PM 09-03 裁定的前置）

`KG8D2` 的方向定成了**乙 · 恒等**（实得 ≠ 地板就红）。而恒等把「涨了也红」加进来之后，
**一套 PASS 数随环境浮动的套件会双向都红**。⇒ 装闸之前必须先答一句：
**哪几套的实得是稳的？** 上一拍只在一个镜像、一趟上量过 ⇒ 那一格当时没有读数。

# 它量什么（分母写死，别读宽）

  **人群** = `ci.yml` 的**调用行** 23 条（与 `K-G8-floor-slack-census.py` 同一条正则、同一个人群）。
  **一个读数** = 一趟运行日志里的 `(实得 PASS, FAIL, rc)` 三元组。
    · 实得用 `assert-pass-floor.sh:52` 逐字同一条正则（`合计 PASS=[0-9]+` + `tail -1`）；
    · 抓不到 ⇒ 记 `None`，**不当 0、不当通过**（同该脚本 fail-closed 第 2 条）。
  **稳** = 所有趟的三元组**逐字同值**。⚠ 只比 PASS 不够：一套 `PASS` 相同而 `FAIL` 变了，
    那是**另一件事变了**，不许当成「稳」。

🔴 **它证不了什么**（射程，别读宽）：
  · N 趟同值 **不等于** 恒稳 —— 它只把「不稳」的**下界**抬高。一条 2/14 的 flaky
    在 2 趟里有 74% 的概率两趟都绿。⇒ 报的时候要带趟数，别写成「稳」。
  · 它只覆盖**跑过的那些环境**。没跑过的镜像 / CI runner / Windows，一律**未知**，不是「稳」。

# 用法

  python3 evidence/K-G8-stability-diff.py <标签>=<日志目录> <标签>=<日志目录> [...]
  python3 evidence/K-G8-stability-diff.py --json A=/x/runA B=/x/runB

`--repo` 默认取本文件所在仓的根（`evidence/` 的上一级）。
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys

RE_TOTAL_PASS = re.compile(r"合计 PASS=(\d+)")
RE_TOTAL_FAIL = re.compile(r"合计 PASS=\d+ FAIL=(\d+)")
RE_CI_CALL = re.compile(
    r"^\s*run:\s*bash\s+e2e/assert-pass-floor\.sh\s+(\S+)\s+(\d+)\s*$"
)
RE_GATE_CALL = re.compile(r"^\s*run_e2e\s+(\S+)\s+(\d+)\s*$")


def repo_root(explicit: str | None) -> str:
    if explicit:
        return os.path.abspath(explicit)
    return os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))


def scan(path: str, rx) -> dict[str, int]:
    out: dict[str, int] = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if line.lstrip().startswith("#"):
                continue
            m = rx.match(line.rstrip("\n"))
            if m:
                out[m.group(1)] = int(m.group(2))
    return out


def read_run(run_dir: str, suite: str) -> tuple:
    """返回 (实得, FAIL, rc)。日志缺席 ⇒ (None, None, '<无日志>')。"""
    log = os.path.join(run_dir, f"{suite}.log")
    rcf = os.path.join(run_dir, f"{suite}.rc")
    rc = "<无rc>"
    if os.path.exists(rcf):
        with open(rcf, encoding="utf-8") as fh:
            rc = fh.read().strip()
    if not os.path.exists(log):
        return (None, None, rc if rc != "<无rc>" else "<无日志>")
    with open(log, encoding="utf-8", errors="replace") as fh:
        text = fh.read()
    ps = RE_TOTAL_PASS.findall(text)
    if not ps:
        return (None, None, rc)
    fs = RE_TOTAL_FAIL.findall(text)
    return (int(ps[-1]), int(fs[-1]) if fs else None, rc)


def fmt(t: tuple) -> str:
    got, fail, rc = t
    g = "—" if got is None else str(got)
    f = "—" if fail is None else str(fail)
    return f"{g}/{f} rc={rc}"


def main() -> int:
    ap = argparse.ArgumentParser(description="K-G8：多趟之间逐套对拍实得")
    ap.add_argument("runs", nargs="+", help="<标签>=<日志目录>，至少两个")
    ap.add_argument("--repo", default=None)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    if len(args.runs) < 2:
        print("❌ 至少给两趟才谈得上「稳不稳」", file=sys.stderr)
        return 2

    labels, dirs = [], []
    for spec in args.runs:
        if "=" not in spec:
            print(f"❌ 参数要写成 <标签>=<目录>，实得：{spec}", file=sys.stderr)
            return 2
        lab, d = spec.split("=", 1)
        labels.append(lab)
        dirs.append(d)

    root = repo_root(args.repo)
    ci = scan(os.path.join(root, ".github", "workflows", "ci.yml"), RE_CI_CALL)
    gate = scan(os.path.join(root, "scripts", "gate.sh"), RE_GATE_CALL)

    rows = []
    for suite in sorted(ci):
        reads = [read_run(d, suite) for d in dirs]
        same = len(set(reads)) == 1
        rows.append({
            "suite": suite,
            "in_gate": suite in gate,
            "floor_ci": ci[suite],
            "floor_gate": gate.get(suite),
            "reads": [{"label": l, "got": r[0], "fail": r[1], "rc": r[2]}
                      for l, r in zip(labels, reads)],
            "same": same,
        })

    if args.json:
        json.dump({"repo": root, "labels": labels, "dirs": dirs, "rows": rows},
                  sys.stdout, ensure_ascii=False, indent=2)
        print()
        return 0

    print(f"# 仓：{root}")
    print(f"# 趟数 {len(labels)}：" + " · ".join(f"{l} = {d}" for l, d in zip(labels, dirs)))
    print("# 一个读数 = (实得 PASS / FAIL / rc)。**三个都同**才算同值。")
    print()
    w = max(len(s) for s in ci) + 2
    head = f"{'套件':<{w}} {'gate':>5} {'地板':>5} "
    head += " ".join(f"{l:>18}" for l in labels) + "   同值"
    print(head)
    print("-" * len(head))
    unstable, unstable_gate = [], []
    for r in rows:
        cells = " ".join(f"{fmt((c['got'], c['fail'], c['rc'])):>18}" for c in r["reads"])
        mark = "✓" if r["same"] else "🔴 不同"
        print(f"{r['suite']:<{w}} {'★' if r['in_gate'] else '':>5} "
              f"{r['floor_ci']:>5} {cells}   {mark}")
        if not r["same"]:
            unstable.append(r["suite"])
            if r["in_gate"]:
                unstable_gate.append(r["suite"])
    print("-" * len(head))
    print(f"人群 {len(rows)} 套（ci.yml 调用行）· 其中 gate.sh 真跑 {sum(1 for r in rows if r['in_gate'])} 套（★）")
    print(f"逐套两两同值：{len(rows) - len(unstable)}/{len(rows)}")
    if unstable:
        print(f"🔴 **不同值的套件（{len(unstable)}）**：" + " · ".join(unstable))
    else:
        print(f"✓ 这 {len(labels)} 趟里没有一套读数不同 —— ⚠ 这是「这几趟没抓到」，不是「恒稳」")
    if unstable_gate:
        print(f"🔴🔴 **其中落在 gate.sh 那 4 套里的**：" + " · ".join(unstable_gate)
              + "  ⇒ 恒等闸**不许装**（PM 单子：4 套里有任何一套不稳就当场停下）")
    else:
        print("✓ gate.sh 那几套在这几趟里全部同值")
    return 0


if __name__ == "__main__":
    sys.exit(main())
