#!/usr/bin/env python3
"""N-F2：改动面 —— 基点与分支尖之间，**哪几个函数 / 方法真的变了**（逐函数 md5）。

住址（只属于本件）：`<n-f2 工作树>/evidence/N-F2-fnmd5.py`
被测对象：**本脚本所在工作树**（`git show <rev>:<path>` 取两版原文，**不读工作树**
⇒ 未提交的改动不进这个数；那一维归 `NF2D4` 的工作树口径，两把尺子各管各的）。

# 为什么不再抄一份切分器

切分逻辑（顶层导出 + 类体内方法，按大括号配平，含紧贴的 jsdoc）住 `N-F1b-fnmd5.py`，
本文件**只换分母**（写区那四份文件）并调它 —— 同一段规则不在第二处再写一遍，
否则哪天它被修好了，这一份还照着旧的算。⚠ 它是个**朴素切分器**不是 TS parser
（模板串里的 `{`/`}` 会算进配平），这条边界随它一起继承。

用法：
    python3 evidence/N-F2-fnmd5.py <base-rev> <tip-rev>
"""

from __future__ import annotations

import importlib.util
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent

PATHS = [
    "src/settings/accounts-section.ts",
    "src/settings/accounts-section.vitest.ts",
    "src/settings/readiness.vitest.ts",
    "src/settings/machine-status.vitest.ts",
]


def _borrow():
    """把 `N-F1b-fnmd5.py` 当模块加载（文件名带减号，import 不进来）。"""
    src = HERE / "N-F1b-fnmd5.py"
    spec = importlib.util.spec_from_file_location("nf1b_fnmd5", src)
    if spec is None or spec.loader is None:
        raise SystemExit(f"取不到切分器：{src}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    base, tip = sys.argv[1], sys.argv[2]
    m = _borrow()
    print(f"# 改动面 · 逐函数 md5   基点={base}  分支尖={tip}")
    print(f"# 切分器住址：{HERE / 'N-F1b-fnmd5.py'}（本件只换分母，不抄第二份）")
    print(f"# 分母：写区那 {len(PATHS)} 份 .ts 里，两版都解析得出的段落")
    g_changed = g_added = g_same = 0
    for path in PATHS:
        a, b = m.show(base, path), m.show(tip, path)
        if b is None:
            print(f"\n## {path}  分支尖上取不到，跳过")
            continue
        sa = m.segments(a) if a is not None else {}
        sb = m.segments(b)
        changed = [k for k in sb if k in sa and m.md5(sa[k][2]) != m.md5(sb[k][2])]
        added = [k for k in sb if k not in sa]
        gone = [k for k in sa if k not in sb]
        same = len(sb) - len(changed) - len(added)
        g_changed += len(changed)
        g_added += len(added)
        g_same += same
        print(f"\n## {path}")
        print(f"   段落数：基点 {len(sa)} → 分支尖 {len(sb)}")
        for k in changed:
            print(
                f"   变  {k:<34} {m.md5(sa[k][2])} -> {m.md5(sb[k][2])}"
                f"  (行 {sb[k][0]}-{sb[k][1]})"
            )
        for k in added:
            print(f"   新  {k:<34} {'-':>12} -> {m.md5(sb[k][2])}  (行 {sb[k][0]}-{sb[k][1]})")
        for k in gone:
            print(f"   删  {k:<34} {m.md5(sa[k][2])} -> {'-':>12}")
        print(f"   没动：{same} 段")
    print(f"\n# 合计：变 {g_changed} · 新 {g_added} · 没动 {g_same}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
