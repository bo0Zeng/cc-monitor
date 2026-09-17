#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R5 · 实现拍（C）的量具 ③：把头注里**每一条 `文件:行号` 引用**拿到盘上现打一次。

住址（唯一）：`<工作树>/evidence/K-R5-C-line-refs.py`
被测对象：`<工作树>/src-tauri/src/capability_registry.rs` 里的引用 → `<工作树>/` 下的那些文件。
          09-02 这一拍是 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r5`，分支 `track/k-r5`。

# 它治什么

🔴 **行号是快照**（`brief` 12：「带具体读数的与描述盘上现状的话都是那一刻的快照 —— 引用前重打，
别当常量」；`K-R9` `§3` 逐字记过同一个病：「谁再在上方加注释就会把它推馊」）。
`K-G3` 09-01 往 `scripts/gate.sh` 加了门六，把行号整体往下推 ⇒ 头注里引 `gate.sh` 的那几处当场全馊。

⚠ **本量具只判「那一行今天长什么样」，判不了「引用的意图对不对」** —— 它印出原文，
由人看一眼。它**不是**一条判据（没有对错的机器口径），是一把尺子。

⚠ **分母是「本文件里 `文件:行号` 这一形的引用」**，不含：只写文件名不写行号的引用 ·
写成 `(:82-101)` 这类**跟在别的词后面**的行段（本量具单列一类 `裸行段`，靠上下文认不出文件）。

用法：
  python3 evidence/K-R5-C-line-refs.py            # 全表
  python3 evidence/K-R5-C-line-refs.py --only gate.sh
"""
from __future__ import annotations

import argparse
import pathlib
import re
import sys

# `路径:行号`（路径要带扩展名或斜杠，免得把 `D3:` 这种代号也算进来）
REF = re.compile(r"(?<![\w/.-])([A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*\.[A-Za-z0-9]+):(\d+)")
# 同一条引用后面**紧接着**的 `/`:NNN``（`release.yml:42`/`:152`/`:271` 那一形）。
# 🔴 必须锚在 `\A`：第一版没锚，于是在
# 「`tmux-guarded-acceptance.sh:21` · `usage-probe-acceptance.sh:23`/`:129`」这一行上
# 把 `:129` 也挂到了**前一个**文件头上，印出一条假的「越界」。
# 〔09-02 自查现打逮到；同族就是 MEMORY 里那条「量具的作用域对不上事实」。〕
TAIL = re.compile(r"\A`?/`?:(\d+)`?")
BARE_RANGE = re.compile(r"\(:(\d+)-(\d+)\)")

# 引用里写的是仓根相对路径，但有几个只写了文件名 —— 这张表给它们补住址。
# ⚠ 补不出来的一律标 `找不到那份文件`，**不许猜**。
HINTS = {
    "gate.sh": "scripts/gate.sh",
    "ci.yml": ".github/workflows/ci.yml",
    "release.yml": ".github/workflows/release.yml",
    "run.ps1": "scripts/run.ps1",
    "build.rs": "src-tauri/build.rs",
}


def resolve(wt: pathlib.Path, ref: str) -> pathlib.Path | None:
    for cand in (wt / ref, wt / HINTS.get(ref, ref), wt / "e2e" / ref, wt / "scripts" / ref):
        if cand.is_file():
            return cand
    return None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--wt", default=str(pathlib.Path(__file__).resolve().parent.parent))
    ap.add_argument("--only", default="")
    a = ap.parse_args()

    wt = pathlib.Path(a.wt).resolve()
    f = wt / "src-tauri" / "src" / "capability_registry.rs"
    src = f.read_text(encoding="utf-8").splitlines()

    print(f"# 引用现打 —— 被测对象 {f}")
    print(f"# 工作树 {wt}")
    print()
    total = miss = 0
    for i, line in enumerate(src, 1):
        for m in REF.finditer(line):
            ref, no = m.group(1), int(m.group(2))
            nos, rest = [no], line[m.end():]
            while True:  # `a.yml:42`/`:152`/`:271` —— 一条一条往后咬，中间不许隔别的字
                t = TAIL.match(rest.lstrip("`"))
                if not t:
                    break
                nos.append(int(t.group(1)))
                rest = rest.lstrip("`")[t.end():]
            if a.only and a.only not in ref:
                continue
            p = resolve(wt, ref)
            for n in nos:
                total += 1
                if p is None:
                    print(f"  本文件 :{i}  引 `{ref}:{n}`  🔴 找不到那份文件")
                    miss += 1
                    continue
                body = p.read_text(encoding="utf-8", errors="replace").splitlines()
                got = body[n - 1].strip() if 1 <= n <= len(body) else "<越界>"
                flag = "🔴 注释" if got.startswith(("#", "//")) else ("🔴 空行" if not got else "  ")
                if flag.startswith("🔴"):
                    miss += 1
                print(f"  本文件 :{i}  引 `{ref}:{n}` {flag} → {got[:96]}")
        for m in BARE_RANGE.finditer(line):
            print(f"  本文件 :{i}  裸行段 `(:{m.group(1)}-{m.group(2)})`（靠上下文认文件，本量具不判）")
    print()
    print(f"命中引用 {total} 条；其中**指到注释行 / 空行 / 找不到文件** {miss} 条。")
    print("⚠ 「指到注释行」不等于「错」—— 有的引用本来就该指注释；这一栏只是让人看一眼。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
