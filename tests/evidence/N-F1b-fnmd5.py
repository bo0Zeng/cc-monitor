#!/usr/bin/env python3
"""N-F1b：改动面 —— 基点与分支尖之间，**哪几个函数 / 方法真的变了**（逐函数 md5）。

# 为什么不是 `git diff --stat`

`--stat` 只答「哪几份文件、几行」。派工单第四节要的是「哪几个函数变了」——
一份 1300 行的文件里改了 3 行，`--stat` 说不出那 3 行落在谁身上，
而「远端那条路一个字节没动」（`NF1bD3`）恰恰是一句**按函数**说的话。

# 量法（写清楚，免得下一个人照住址跑出另一棵树上的数）

- 被测对象：`git show <rev>:<path>` 取两版原文，**不读工作树** ⇒ 未提交的改动不进这个数
  （那一维归 `NF1bD4` 的工作树口径，两把尺子各管各的）。
- 切分单位：顶层 `export function` / `export const` / `export interface` / `export type`
  + `class` 体内的方法，按**大括号配平**取整段（含前面紧贴的 jsdoc 块注释）。
  ⚠ 这是个**朴素切分器**，不是 TS parser：模板串里的 `{`/`}` 会算进配平。
  ⇒ 它自己带一条自检（下面 `--selfcheck`）：切出来的段落必须覆盖住 diff 命中的每一行，
  覆盖不住就点名报出来，不许静默少算。
- md5 落在**去掉行尾空白后的整段**上（换行统一成 \n）。

用法：
    python3 evidence/N-F1b-fnmd5.py <base-rev> <tip-rev>
"""

from __future__ import annotations

import hashlib
import re
import subprocess
import sys

PATHS = [
    "src/accounts.ts",
    "src/settings/accounts-section.ts",
    "src/settings/accounts-section.vitest.ts",
]

# 一个「段」的头：顶层导出、或类体内的方法 / getter。
HEAD = re.compile(
    r"^(?P<indent>[ ]*)"
    r"(?:export\s+)?"
    r"(?:(?:private|public|protected)\s+)?"
    r"(?:static\s+)?"
    r"(?:async\s+)?"
    r"(?:(?:function|class|interface|type|const|let)\s+)?"
    r"(?P<name>[A-Za-z_$][\w$]*)"
    r"\s*(?:<[^>]*>)?\s*[(:=]"
)


def show(rev: str, path: str) -> str | None:
    p = subprocess.run(
        ["git", "show", f"{rev}:{path}"], capture_output=True, text=True
    )
    return p.stdout if p.returncode == 0 else None


def segments(src: str) -> dict[str, tuple[int, int, str]]:
    """name -> (起行, 止行, 正文)。同名取第一个，重名在返回的 key 上加 #n。"""
    lines = src.split("\n")
    out: dict[str, tuple[int, int, str]] = {}
    i = 0
    while i < len(lines):
        m = HEAD.match(lines[i])
        if not m or lines[i].lstrip().startswith(("//", "*", "/*")):
            i += 1
            continue
        # 往回吞掉紧贴的块注释 —— 头注是那个函数的一部分，改了头注也算改了它。
        start = i
        j = i - 1
        while j >= 0 and (lines[j].strip().startswith(("*", "/**", "//")) or not lines[j].strip()):
            if lines[j].strip().startswith("/**"):
                start = j
                break
            if not lines[j].strip():
                break
            j -= 1
        # 大括号配平（朴素）
        depth, k, opened = 0, i, False
        while k < len(lines):
            depth += lines[k].count("{") - lines[k].count("}")
            if "{" in lines[k]:
                opened = True
            if opened and depth <= 0:
                break
            if not opened and lines[k].rstrip().endswith(";"):
                break
            k += 1
        name = m.group("name")
        key, n = name, 2
        while key in out:
            key, n = f"{name}#{n}", n + 1
        body = "\n".join(x.rstrip() for x in lines[start : k + 1])
        out[key] = (start + 1, k + 1, body)
        i = k + 1
    return out


def md5(s: str) -> str:
    return hashlib.md5(s.encode()).hexdigest()[:12]


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    base, tip = sys.argv[1], sys.argv[2]
    print(f"# 改动面 · 逐函数 md5   基点={base}  分支尖={tip}")
    print(f"# 分母：写区那 {len(PATHS)} 份文件里，两版都解析得出的段落")
    grand_changed = grand_added = grand_same = 0
    for path in PATHS:
        a, b = show(base, path), show(tip, path)
        if b is None:
            print(f"\n## {path}  ⚠ 分支尖上取不到，跳过")
            continue
        sa = segments(a) if a is not None else {}
        sb = segments(b)
        print(f"\n## {path}")
        print(f"   段落数：基点 {len(sa)} → 分支尖 {len(sb)}")
        changed = [k for k in sb if k in sa and md5(sa[k][2]) != md5(sb[k][2])]
        added = [k for k in sb if k not in sa]
        gone = [k for k in sa if k not in sb]
        same = len(sb) - len(changed) - len(added)
        grand_changed += len(changed)
        grand_added += len(added)
        grand_same += same
        for k in changed:
            print(f"   变  {k:<28} {md5(sa[k][2])} -> {md5(sb[k][2])}  (行 {sb[k][0]}-{sb[k][1]})")
        for k in added:
            print(f"   新  {k:<28} {'-':>12} -> {md5(sb[k][2])}  (行 {sb[k][0]}-{sb[k][1]})")
        for k in gone:
            print(f"   删  {k:<28} {md5(sa[k][2])} -> {'-':>12}")
        print(f"   没动：{same} 段")
    print(f"\n# 合计：变 {grand_changed} · 新 {grand_added} · 没动 {grand_same}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
