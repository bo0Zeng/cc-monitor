#!/usr/bin/env python3
# ruff: noqa
"""K-R27 · `KR27D3` 的活体台子：一条 `parked` 理由**会不会自己出声说自己馊了**。

住址（唯一）：`evidence/K-R27-parked-staleness-probe.py`（名字带件号 `K-R27`，本件独占）。
被测对象：**本文件所在的那棵工作树**（`Path(__file__).resolve().parents[1]`）——
不写死路径、不指别的树；谁在哪棵树上跑它，量的就是那棵树。
`parked` 文本读的是 `<工作树>/../../planned-build/<ws>/.dispatch.json`（`--ws` 可换）。
⚠ 本脚本**只读**：不写盘、不跑 cargo / npm / e2e、不调 `pb`。

# 它在验什么（不是在做什么）

`KR27D3` 问的是「有没有一条**便宜**的做法，让『parked 理由过期』下次会出声」。
本脚本把两条候选**同时**跑在同一批 `parked` 上，各出各的读数，好让人拿
**真阳 / 出声总数** 去对铁律 18（真阳率压不住噪声就别加）。

  · 候选甲 **点名的符号还在不在** —— 把 `parked` 里反引号包着的**标识符**摘出来，
    在**生产段**（排掉 `evidence/` 与 `.md`）现打命中数；0 命中 ⇒ 出声。
  · 候选乙 **落笔之后有没有人动过它说的那片面** —— 把 `parked` 里反引号包着的
    **路径**摘出来（要在 `git ls-files` 里真的存在），从这条 `parked` 的**落笔锚点**
    到 `HEAD` 跑 `git log --oneline <锚点>..HEAD -- <那几条路径>`；非空 ⇒ 出声。

落笔锚点的取法（两级，**先文本后兜底**，每条都印出来是哪一级）：
  ①【文内 sha】`parked` 文本里第一个 7–40 位十六进制串，且 `git cat-file -e` 认得；
  ②【件号兜底】`git log --grep=<件号> --format=%H` 的**最新一条**（= 这一波它自己的落点）。
⚠ 两级都取不到 ⇒ 这一条**不判**，单列成 `锚点缺席`，不许并进「没出声」凑一个好看的数。

# 分母怎么数的

- **本脚本的分母 = `--only` 给的那几条**（默认 = `KR27D1` 的那 14 条非「等用户」）。
  那 6 条「等用户」的 (`K-P2` `K-P4` `K-R11` `K-R16` `K-R19` `K-R8`) **不进分母** ——
  它们等的是人不是代码，时态审计对它们没有意义（`KR27D2` 的射程）。
- **「出声」是按 `parked` 条目计的，不是按待办计**：一条 `parked` 里只要有一个符号死了、
  或有一条路径被动过，它就算「出声一次」。⇒ 这个数**不能**和手核那 52 条待办相加。
- **符号面**：反引号里形如 `[A-Za-z_][A-Za-z0-9_]{7,}` 的串（长度门槛 8，挡掉
  `exit` / `links` / `cargo` 这类词），再排掉一张**噪声名单**（下面 `NOT_SYMBOLS`）。
  ⚠ 这张名单就是本候选的成本所在：**它是手维护的**，漏一个就多一次假阳。
- **路径面**：反引号里含 `/` 且 `.` 的串，`git ls-files` 里存在才算；不存在的单列。

跑法：
    python3 evidence/K-R27-parked-staleness-probe.py            # 两条候选各一张表
    python3 evidence/K-R27-parked-staleness-probe.py --json     # 机读
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

TREE = Path(__file__).resolve().parents[1]

# `KR27D1` 的分母：14 条非「等用户」。闭集只有这一处住址，散文里不复述成员。
FOURTEEN = [
    "K-P3", "K-R1", "K-R12", "K-R2", "K-R22", "K-R23", "K-R24",
    "K-R25", "K-R9", "K-W1B", "K-W1C", "K-W2D", "K-W2E", "K-W4",
]

# 候选甲的噪声名单 —— **手维护，本身就是这条候选的代价**。
# 分三类：① 计划仓的词汇（不是代码里的符号）② 通用英文/工具名 ③ 本仓文件名以外的路径片段。
NOT_SYMBOLS = {
    "dispatch", "accept", "worktree", "worktrees", "features", "evidence",
    "audits", "parked", "inflight", "capabilities", "shellcheck", "readonly",
    "additive", "no-counterpart", "starred_count", "hidden_count", "has_live",
    "externalBin", "OUTPUT_CAPTURE", "coreutils",
}

HEX = re.compile(r"\b([0-9a-f]{7,40})\b")
TICK = re.compile(r"`([^`\n]{1,200})`")
IDENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]{7,}$")


def sh(args, cwd=TREE):
    p = subprocess.run(args, cwd=cwd, capture_output=True, text=True)
    return p.returncode, p.stdout


def tracked_files():
    rc, out = sh(["git", "ls-files", "-z"])
    # ⚠ `-z`：本仓路径全是中文，裸 `git ls-files` 会加引号转义（`KR27D4` 点名的那条）。
    return {x for x in out.split("\0") if x}


def anchor_for(item, text, files_ok):
    """落笔锚点：①文内 sha ②件号兜底。回 (sha, 哪一级) 或 (None, 原因)。"""
    for h in HEX.findall(text):
        if sh(["git", "cat-file", "-e", h + "^{commit}"])[0] == 0:
            return h, "①文内 sha"
    rc, out = sh(["git", "log", "--grep=" + item, "--format=%H", "-n", "1"])
    if rc == 0 and out.strip():
        return out.split()[0][:9], "②件号兜底"
    return None, "锚点缺席"


def symbols_in(text):
    out = []
    for t in TICK.findall(text):
        t = t.strip()
        if t in NOT_SYMBOLS:
            continue
        # `mod::name` / `name(` / `name` 三形都收一次末段
        base = t.split("::")[-1].split("(")[0].strip()
        if IDENT.match(base) and base not in NOT_SYMBOLS:
            out.append(base)
    return sorted(set(out))


def paths_in(text, files_ok):
    out = []
    for t in TICK.findall(text):
        t = t.strip().split(":")[0]
        if "/" in t and "." in t and t in files_ok:
            out.append(t)
    return sorted(set(out))


def prod_hits(sym):
    """生产段命中数：排掉 `evidence/` 与 `.md`（那是账，不是代码）。"""
    rc, out = sh(["git", "grep", "-c", "--", sym])
    n = 0
    for line in out.splitlines():
        f, _, c = line.rpartition(":")
        if f.startswith("evidence/") or f.endswith(".md"):
            continue
        n += int(c)
    return n


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ws", default="backend-consolidation")
    ap.add_argument("--only", default=",".join(FOURTEEN))
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()

    disp = TREE.parents[1] / "planned-build" / a.ws / ".dispatch.json"
    parked = json.loads(disp.read_text(encoding="utf-8"))["parked"]
    only = [x for x in a.only.split(",") if x]
    files_ok = tracked_files()
    head = sh(["git", "rev-parse", "HEAD"])[1].strip()

    rows = []
    for item in only:
        text = parked.get(item)
        if text is None:
            rows.append({"件": item, "错": "这条 parked 不存在"})
            continue
        sha, how = anchor_for(item, text, files_ok)
        syms = symbols_in(text)
        paths = paths_in(text, files_ok)
        dead = [s for s in syms if prod_hits(s) == 0]
        touched = []
        if sha:
            for p in paths:
                rc, out = sh(["git", "log", "--oneline", f"{sha}..HEAD", "--", p])
                if out.strip():
                    touched.append((p, len(out.strip().splitlines())))
        rows.append({
            "件": item, "锚点": sha, "锚点取法": how,
            "符号面": len(syms), "死名": dead,
            "路径面": len(paths), "被动过": touched,
            "甲出声": bool(dead),
            "乙出声": bool(touched) if sha else None,
        })

    if a.json:
        print(json.dumps({"tree": str(TREE), "head": head, "rows": rows},
                         ensure_ascii=False, indent=1))
        return

    print(f"# 被测对象 = {TREE}  @ {head}")
    print(f"# 分母 = {len(only)} 条 parked（`KR27D1` 那 14 条非「等用户」；6 条等用户不进分母）")
    print()
    print("| 件 | 锚点 | 取法 | 符号面 | 甲·死名 | 路径面 | 乙·落笔后被动过 |")
    print("|---|---|---|---|---|---|---|")
    jia = yi = anchorless = 0
    for r in rows:
        if "错" in r:
            print(f"| {r['件']} | — | — | — | — | — | {r['错']} |")
            continue
        if r["锚点"] is None:
            anchorless += 1
        if r["甲出声"]:
            jia += 1
        if r["乙出声"]:
            yi += 1
        dead = ", ".join(f"`{d}`" for d in r["死名"]) or "—"
        tou = ", ".join(f"{p}({n})" for p, n in r["被动过"]) or "—"
        print(f"| {r['件']} | {r['锚点'] or '缺席'} | {r['锚点取法']} | {r['符号面']} | "
              f"{dead} | {r['路径面']} | {tou} |")
    print()
    print(f"甲（点名的符号死了）出声：{jia} / {len(only)}")
    print(f"乙（落笔后那片面被动过）出声：{yi} / {len(only)}"
          f"（其中 {anchorless} 条锚点缺席、**不判**，不算没出声）")


if __name__ == "__main__":
    sys.exit(main())
