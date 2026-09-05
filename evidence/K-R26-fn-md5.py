#!/usr/bin/env python3
"""K-R26 的**改动面量具** —— 逐「函数 / 具名常量」md5（基线 vs 工作树）。

住址：`<工作树>/evidence/K-R26-fn-md5.py`
被测对象**指向哪棵树**：`/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r26`
（写死，且下面会核一次分支名 —— 同名量具被别人覆盖成指向另一棵树的那一族病，
`brief` `5k` 记着；这里让它**指得出来**）。

抽取口径（写明白，别读大）：
  · `brief` 第四部分要的是「`ast` 逐函数 md5」，那条是**给 Python 写的**（`ast` 模块）。
    Rust 没有现成的 `ast` ⇒ 这里是**按缩进切块**的粗抽取器：从 `fn <名>` /
    `const <名>` 那一行起，到下一个同缩进的 `}` / `];` / `;` 为止。
  · 它切的是**块本身 + 紧挨其上的文档注释与属性**（`///` / `//` / `#[…]` 连续块）。
    ⇒ **只改文档注释也会让 md5 变**，这是刻意的：本件改了好几处头注，那要看得见。
  · 抽取器自检：两侧切出来的条数都要 >= 地板，切不到就报 CRASH，不是「没变化」。

跑法：
    python3 evidence/K-R26-fn-md5.py                # 基线 = HEAD
    python3 evidence/K-R26-fn-md5.py --rev <sha>
"""

import hashlib
import re
import subprocess
import sys
from pathlib import Path

WT = Path("/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r26")
WANT_BRANCH = "track/k-r26"
FILES = [
    ("remote-daemon-proto/src/plugin/invoke.rs", 10),
    ("remote-daemon-proto/src/plugin/mod.rs", 10),
    ("remote-daemon-proto/src/plugin_walk_fixture.rs", 40),
]

BLOCK_RE = re.compile(
    r"^(?P<ind>[ ]*)(?:pub(?:\([a-z]+\))? )?(?:async )?"
    r"(?:fn (?P<fn>[A-Za-z0-9_]+)|const (?P<c>[A-Z][A-Za-z0-9_]*))"
)


def split_blocks(src: str):
    lines = src.splitlines()
    out = {}
    i = 0
    while i < len(lines):
        m = BLOCK_RE.match(lines[i])
        if not m:
            i += 1
            continue
        name = m.group("fn") or m.group("c")
        ind = m.group("ind")
        start = i
        while start > 0:
            prev = lines[start - 1].strip()
            if prev.startswith("///") or prev.startswith("#[") or prev.startswith("//"):
                start -= 1
            else:
                break
        # 🔴 `const` 与 `fn` 的收尾**不是同一种事**，第一版把它们并成一条 ⇒
        #    多行的 `const NAME: &str =\n    "…";` 找不到 `}` 收尾，就一路吞到**下一个函数的**
        #    右大括号，把中间那几个块整个吃掉（自查现打：`fixture_vocabulary` 那一族少点 5 个名）。
        end = i
        if m.group("c") is not None:
            # 常量：往后找**第一条以 `;` 收尾**的行（含本行）。
            for j in range(i, len(lines)):
                if lines[j].rstrip().endswith(";"):
                    end = j
                    break
            else:
                end = len(lines) - 1
        else:
            for j in range(i + 1, len(lines)):
                t = lines[j].rstrip()
                if t in (ind + "}", ind + "];", ind + ");"):
                    end = j
                    break
            else:
                end = len(lines) - 1
        body = "\n".join(lines[start : end + 1])
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
    branch = subprocess.run(
        ["git", "-C", str(WT), "rev-parse", "--abbrev-ref", "HEAD"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    head = subprocess.run(
        ["git", "-C", str(WT), "rev-parse", "--short", rev],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    print(f"被测对象：{WT}（分支 {branch}） · 基线 {rev} = {head}")
    if branch != WANT_BRANCH:
        print(f"CRASH：这棵树的分支是 `{branch}`，本量具写死的是 `{WANT_BRANCH}` —— "
              f"它此刻在量另一棵树，下面的数一律不算数")
        return 2
    print()
    rc = 0
    for rel, floor in FILES:
        old = subprocess.run(
            ["git", "-C", str(WT), "show", f"{rev}:{rel}"],
            capture_output=True, text=True,
        )
        old_src = old.stdout if old.returncode == 0 else ""
        new_src = (WT / rel).read_text(encoding="utf-8")
        a, b = split_blocks(old_src), split_blocks(new_src)
        print(f"── {rel}")
        if old.returncode != 0:
            print(f"   （基线里没有这个文件 —— 整份是新增的，{len(b)} 个块）")
            continue
        for label, d in (("基线", a), ("工作树", b)):
            if len(d) < floor:
                print(f"   CRASH：{label}只切出 {len(d)} 个块（地板 {floor}）—— "
                      f"抽取器坏了，本文件这一段不算数")
                rc = 2
        added = [k for k in b if k not in a]
        removed = [k for k in a if k not in b]
        changed = [k for k in b if k in a and a[k] != b[k]]
        same = [k for k in b if k in a and a[k] == b[k]]
        print(f"   基线 {len(a)} 块 · 工作树 {len(b)} 块 ⇒ "
              f"新增 {len(added)} · 删除 {len(removed)} · 改了 {len(changed)} · "
              f"一个字节没动 {len(same)}")
        for k in added:
            print(f"     新增  {b[k]}  {k}")
        for k in removed:
            print(f"     删除  {a[k]}  {k}")
        for k in changed:
            print(f"     改了  {a[k]} -> {b[k]}  {k}")
        print()
    print("（未列出的即「一个字节没动」）")
    return rc


if __name__ == "__main__":
    sys.exit(main())
