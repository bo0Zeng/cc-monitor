#!/usr/bin/env python3
"""N-G1 的第三个量具：`scripts/gate.sh` **逐函数整块 md5**，两个提交对拍。

它买的是一件事：**「五道门的判定口径一个字没动」不靠人读注释，靠哈希。**
`fails+=` 的条件、包数自检、`0 passed 不是绿` 全住在 `run_gate` / `run_gate_sum` /
`run_e2e` 三个函数里 ⇒ 这三块整块 md5 相等，那句话才算有读数。
（`brief` 第 11 条：判「动没动某个闭集」不许用 `grep` 数加行 —— 多行字面量会漏。）

⚠ 射程（别读宽）：
  · 它认的函数是 `^名字() {` 这一形。gate.sh 今天全用这一形；换成 `function f {` 就漏。
  · 它比的是**整块字节**，不去字面量、不去注释 ⇒ **只改注释也会判「变了」**（本文件本轮
    `gate_diag` 那一格就是「+1 行注释 ＋ 1 行改写」合起来判的「变了」，不是纯逻辑变更）。
  · 单行定义（`f() { …; }`）单独认 —— 初版没认，给一个 1 行的函数算出「29 行」的假读数
    （它一路扫到了下一个函数的收尾 `}`）。**那是本量具自己栽过的一跤，写在这里免得复发。**

用法：
  python3 evidence/N-G1-gate-fn-md5.py 333fcde            # 基点 vs 工作树
  python3 evidence/N-G1-gate-fn-md5.py 333fcde 2bbcb4e    # 两个提交对拍
"""
from __future__ import annotations

import hashlib
import pathlib
import re
import subprocess
import sys

DEF = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)\(\)\s*\{")
TARGET = "scripts/gate.sh"


def funcs(text: str) -> dict[str, tuple[str, int]]:
    out: dict[str, tuple[str, int]] = {}
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        m = DEF.match(lines[i])
        if m:
            if lines[i].rstrip().endswith("}"):          # 单行定义，别扫到下一个函数去
                body, j = lines[i], i
            else:
                j = i
                while j < len(lines) and lines[j] != "}":
                    j += 1
                body = "\n".join(lines[i:j + 1])
            out[m.group(1)] = (hashlib.md5(body.encode()).hexdigest()[:12], body.count("\n") + 1)
            i = j
        i += 1
    return out


def load(rev: str | None) -> str:
    if rev is None:
        return pathlib.Path(TARGET).read_text(encoding="utf-8")
    p = subprocess.run(["git", "show", f"{rev}:{TARGET}"], capture_output=True, text=True)
    if p.returncode != 0:
        raise SystemExit(f"取不到 {rev}:{TARGET} —— {p.stderr.strip()}")
    return p.stdout


def main() -> int:
    if len(sys.argv) not in (2, 3):
        raise SystemExit(__doc__)
    base = sys.argv[1]
    tip = sys.argv[2] if len(sys.argv) == 3 else None
    a, b = funcs(load(base)), funcs(load(tip))
    tipname = tip or "工作树"
    print(f"{'函数':16} {base:>20} {tipname:>20}  判读")
    changed = 0
    for k in sorted(set(a) | set(b)):
        x, y = a.get(k), b.get(k)
        xs = f"{x[0]}/{x[1]}行" if x else "（不存在）"
        ys = f"{y[0]}/{y[1]}行" if y else "（不存在）"
        verd = "新增" if not x else "删除" if not y else ("**变了**" if x[0] != y[0] else "逐字节相同")
        changed += verd != "逐字节相同"
        print(f"{k:16} {xs:>20} {ys:>20}  {verd}")
    print(f"\n分母：`^名字() {{` 认得出的函数，{base} {len(a)} 个 · {tipname} {len(b)} 个；"
          f"其中不「逐字节相同」的 {changed} 个")
    return 0


if __name__ == "__main__":
    sys.exit(main())
