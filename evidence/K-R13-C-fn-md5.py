#!/usr/bin/env python3
"""K-R13 实现拍（C）的第三件量具 —— **改动面：逐函数 md5**（HEAD vs 工作树）。

住址：<工作树>/evidence/K-R13-C-fn-md5.py
被测对象：`<工作树>/src-tauri/src/skill_host.rs`（默认工作树 = `.claude/worktrees/k-r13`）。

⚠ 口径写明白，别读大：
  · brief 第四部分要的是「`ast` 逐函数 md5」，那条是**给 Python 写的**（`ast` 模块）。
    Rust 没有现成的 `ast`，这里用的是**按缩进切块**的粗抽取器：
    从 `    fn <名>` / `    pub fn <名>` 那一行起，到下一个同缩进的 `}` 为止。
  · ⇒ 它切的是**函数体 + 紧挨其上的文档注释**（`///` 连续块）。
    改文档注释也会让 md5 变 —— **这是刻意的**（本件就改了两处头注，那要看得见）。
  · 抽取器自检：两侧切出来的函数条数都要 >= 一个地板，切不到就报 CRASH 而不是「没变化」。

跑法：
    python3 evidence/K-R13-C-fn-md5.py                 # 对比 HEAD 与工作树
    python3 evidence/K-R13-C-fn-md5.py --rev <sha>     # 换一个基线
"""

import hashlib
import re
import subprocess
import sys
from pathlib import Path

PROJ = Path("/home/zbl/文档/claudecode-frontend")
WT = PROJ / ".claude" / "worktrees" / "k-r13"
REL = "src-tauri/src/skill_host.rs"
FLOOR = 20  # 抽取器自检地板：切不到这么多函数就说明切法坏了

FN_RE = re.compile(r"^(?P<ind>[ ]*)(?:pub )?(?:async )?fn (?P<name>[A-Za-z0-9_]+)")


def split_fns(src: str):
    lines = src.splitlines()
    out = {}
    i = 0
    while i < len(lines):
        m = FN_RE.match(lines[i])
        if not m:
            i += 1
            continue
        ind = m.group("ind")
        name = m.group("name")
        # 往上吞掉紧挨着的文档注释 / 属性（`#[test]` 这类）
        start = i
        while start > 0:
            prev = lines[start - 1].strip()
            if prev.startswith("///") or prev.startswith("#[") or prev.startswith("//"):
                start -= 1
            else:
                break
        end = i
        close = ind + "}"
        for j in range(i + 1, len(lines)):
            if lines[j] == close or lines[j].rstrip() == close:
                end = j
                break
        else:
            end = len(lines) - 1
        body = "\n".join(lines[start : end + 1])
        # 同名（不同 mod 里）时加序号，别静默覆盖
        key = name
        n = 2
        while key in out:
            key = f"{name}#{n}"
            n += 1
        out[key] = hashlib.md5(body.encode("utf-8")).hexdigest()[:12]
        i = end + 1
    return out


def main():
    rev = "HEAD"
    if "--rev" in sys.argv:
        rev = sys.argv[sys.argv.index("--rev") + 1]
    old = subprocess.run(
        ["git", "-C", str(WT), "show", f"{rev}:{REL}"],
        capture_output=True, text=True, check=True,
    ).stdout
    new = (WT / REL).read_text(encoding="utf-8")

    a, b = split_fns(old), split_fns(new)
    for label, d in (("基线", a), ("工作树", b)):
        if len(d) < FLOOR:
            print(f"CRASH：{label}只切出 {len(d)} 个函数（地板 {FLOOR}）—— 抽取器坏了，"
                  f"下面的对比此刻不算数")
            return 2

    print(f"基线 = {rev}（{len(a)} 个函数） · 工作树（{len(b)} 个函数）")
    print()
    added = [k for k in b if k not in a]
    removed = [k for k in a if k not in b]
    changed = [k for k in b if k in a and a[k] != b[k]]
    same = [k for k in b if k in a and a[k] == b[k]]
    print(f"新增 {len(added)} · 删除 {len(removed)} · 改了 {len(changed)} · 一个字节没动 {len(same)}")
    print()
    for k in added:
        print(f"  新增  {b[k]}  {k}")
    for k in removed:
        print(f"  删除  {a[k]}  {k}")
    for k in changed:
        print(f"  改了  {a[k]} -> {b[k]}  {k}")
    print()
    print("（未列出的即「一个字节没动」）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
