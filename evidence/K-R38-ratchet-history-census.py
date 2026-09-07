#!/usr/bin/env python3
"""K-R38 `D1` 的量具：那个「只许降」的棘轮，历史上到底降过几次、抬过几次。

用法（**被测对象必须显式给**，别让它猜）：
    python3 evidence/K-R38-ratchet-history-census.py <工作树绝对路径>

它答三问：
  ① 工作树此刻的 `PENDING` 条数与 `PENDING_CEILING`（两把尺子对拍）；
  ② 逐个**触碰过本文件的提交**上这两个值各是多少 —— 于是「降过几次 / 抬过几次」
     是**数出来的**，不是从一句话里抄来的；
  ③ 三把 git 尺子（`-L` / `-G` / `-S`）各给几个提交 —— `-S` 是 pickaxe，
     数的是**字符串出现次数**，`31 -> 30` 不改变出现次数 ⇒ 它看不见。

⚠ 分母怎么数的，逐条写在输出里。
⚠ 本量具**只读**，一个 git 写命令都不跑。
"""

import re
import subprocess
import sys
from pathlib import Path

REL = "src-tauri/src/scanning_guard_registry.rs"

CEIL_RE = re.compile(r"const\s+PENDING_CEILING\s*:\s*usize\s*=\s*(\d+)\s*;")
PEND_OPEN_RE = re.compile(r"const\s+PENDING\s*:\s*&\[&str\]\s*=\s*&\[")


def git(root: Path, *args: str) -> str:
    out = subprocess.run(
        ["git", *args], cwd=root, capture_output=True, text=True, check=False
    )
    if out.returncode != 0:
        raise SystemExit(f"git {' '.join(args)} 退出码 {out.returncode}：{out.stderr}")
    return out.stdout


def ceiling_of(src: str):
    """`PENDING_CEILING` 的值。找不到 ⇒ None（**不许默默当 0**）。"""
    m = CEIL_RE.search(src)
    return int(m.group(1)) if m else None


def pending_of(src: str):
    """`PENDING` 里的条数。

    口径 = 从 `const PENDING: &[&str] = &[` 那一行起、到同层 `];` 为止，
    数**以引号打头**的行（那正是判据自己 `PENDING.iter().count()` 数的东西）。
    找不到那张表 ⇒ None。
    """
    lines = src.splitlines()
    start = None
    for i, ln in enumerate(lines):
        if PEND_OPEN_RE.search(ln):
            start = i
            break
    if start is None:
        return None
    n = 0
    for ln in lines[start + 1 :]:
        t = ln.strip()
        if t.startswith("];"):
            return n
        if t.startswith('"'):
            n += 1
    return None


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    root = Path(sys.argv[1]).resolve()
    path = root / REL

    print(f"【被测对象】{root}")
    print(f"【被测文件】{REL}")
    print(f"【本树的 HEAD】{git(root, 'rev-parse', 'HEAD').strip()}")
    print(f"【本树的分支】{git(root, 'rev-parse', '--abbrev-ref', 'HEAD').strip()}")
    print()

    src = path.read_text(encoding="utf-8")
    wt_ceil, wt_pend = ceiling_of(src), pending_of(src)
    print("── ① 工作树此刻 ──────────────────────────────")
    print(f"  PENDING_CEILING = {wt_ceil}")
    print(f"  PENDING 条数     = {wt_pend}   （口径：表内以引号打头的行）")
    # 第二把尺子：**同一个作用域**（那张表的行区间内），换一个正则
    # —— 要求「引号包着、`.rs` 结尾、带尾逗号」。两把尺子答案该相同。
    lines = src.splitlines()
    lo = next(i for i, ln in enumerate(lines) if PEND_OPEN_RE.search(ln))
    hi = next(i for i, ln in enumerate(lines[lo:], lo) if ln.strip().startswith("];"))
    rs_line = re.compile(r'^\s*"(src-tauri|remote-daemon-proto)/[^"]*\.rs",$')
    ruler2 = len([ln for ln in lines[lo + 1 : hi] if rs_line.match(ln)])
    print(f"  PENDING 条数(尺子2) = {ruler2}   （**同一区间**内 `\"…/*.rs\",` 行）")
    print(f"  两把尺子{'一致' if ruler2 == wt_pend else '🔴 不一致'}")
    # 🔴 一把**作用域错了**的尺子，留在这里当活体（本仓最高频的那一族病）：
    # 不切区间、直接数整份文件 ⇒ 多点 1 条，多出来的是 `MUST_BE_IN_REACH`
    # 那个见证住址（`K-R37` 落的，与 `PENDING` 毫无关系）。
    # **它与真值只差 1，而差的那 1 条长得和真条目一模一样** —— 这就是为什么
    # 「换一把尺子对拍」要连**作用域**一起对，不是只对正则。
    wrong_scope = len([ln for ln in lines if rs_line.match(ln)])
    print(
        f"  ⚠ 作用域错了的尺子(整份文件) = {wrong_scope}"
        f"   ⇒ 多 {wrong_scope - wt_pend} 条，多的是 `MUST_BE_IN_REACH` 的见证住址"
    )
    print(f"  余量 = {None if wt_ceil is None or wt_pend is None else wt_ceil - wt_pend}")
    print()

    print("── ② 逐个提交（分母 = 所有触碰过本文件的提交）──")
    shas = git(root, "log", "--format=%H", "--", REL).split()
    print(f"  分母 = {len(shas)} 个提交\n")
    rows = []
    for sha in shas:
        blob = git(root, "show", f"{sha}:{REL}")
        meta = git(root, "log", "-1", "--format=%h\t%ad\t%s", "--date=short", sha)
        short, date, subject = meta.rstrip("\n").split("\t", 2)
        rows.append((short, date, ceiling_of(blob), pending_of(blob), subject))
    print(f"  {'sha':9} {'日期':11} {'CEILING':>8} {'PENDING':>8}  余量  提交标题")
    for short, date, c, p, subject in rows:
        gap = "?" if c is None or p is None else str(c - p)
        print(f"  {short:9} {date:11} {str(c):>8} {str(p):>8}  {gap:>4}  {subject[:44]}")
    print()

    # 按时间正序（最老在前）看棘轮怎么转的
    chrono = list(reversed(rows))
    ups, downs = [], []
    prev = None
    for short, date, c, _p, _s in chrono:
        if c is None:
            continue
        if prev is not None and c != prev:
            (ups if c > prev else downs).append((short, date, prev, c))
        prev = c
    print("── ② 结论（数出来的，不是抄的）──────────────")
    print(f"  立起来那一刻：{chrono[0][0]} @ {chrono[0][1]} ⇒ CEILING = {chrono[0][2]}")
    print(f"  **降过 {len(downs)} 次**：")
    for short, date, a, b in downs:
        print(f"    {short} @ {date}  {a} -> {b}")
    print(f"  **抬过 {len(ups)} 次**：")
    for short, date, a, b in ups:
        print(f"    {short} @ {date}  {a} -> {b}")
    if not ups:
        print("    （一次都没有）")
    same = [r for r in rows if r[2] is not None and r[3] is not None and r[2] == r[3]]
    print(f"  每个提交上 PENDING == CEILING 的：{len(same)}/{len(rows)}")
    print()

    print("── ③ 三把 git 尺子（同一个问题，答案差 3 倍）──")
    for label, args in [
        ("-L  那一行的每一次改动", ["log", "-L", f"/const PENDING_CEILING/,+1:{REL}", "--format=%h"]),
        ("-G  diff 文本匹配得上", ["log", "-G", "PENDING_CEILING: usize", "--format=%h", "--", REL]),
        ("-S  pickaxe：出现次数变没变", ["log", "-S", "PENDING_CEILING", "--format=%h", "--", REL]),
    ]:
        out = git(root, *args)
        hits = sorted({ln.strip() for ln in out.splitlines() if re.fullmatch(r"[0-9a-f]{7,}", ln.strip())})
        print(f"  {label:28} ⇒ {len(hits)} 个提交  {hits}")
    print()
    print("  🔴 `-S` 少数 2 个 —— 它数的是**字符串出现次数**，而 `31 -> 30`")
    print("     不改变 `PENDING_CEILING` 的出现次数 ⇒ 它看不见。问「这一行改过几次」时它是错的尺子。")


if __name__ == "__main__":
    main()
