#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R30 交回「改动面」那一栏的量具：**改前 / 改后逐函数 md5 对拍**。

⚠ 本文件是 **K-R30 自己的量具**（`brief` 5k：量具住址要能唯一定位到那一份 ——
   `K-R28` 那份把被测树写死成 `…/k-r28`，照它的住址复跑会跑到另一棵树上）。
   本份**不写死任何树**：两个版本都从 `git show <rev>:<路径>` 取，`rev` 由命令行给。

── 口径 ───────────────────────────────────────────────────────────
  · **分母** = 生产段（`#[cfg(test)]` 之前那一半）里**列 0 的函数签名**，
    正则 `^(pub |pub\\(crate\\) )?(unsafe )?fn <名>`。取体：从签名那一行起，
    切到**下一处列 0 的 `}`**（与本仓那几条函数体判据同一条切法）。
  · md5 打在**原样字节**上（含注释与空白）——「逐字不变」就是逐字。
  · 两版的分母只装**两边都有**的函数（`brief` 15a）；只有一边有的**单列**。

🔴 **非空对照**（本仓最贵的那族）：这份量具的正常结论是「生产段逐函数全同」——
   那是一个**恒同格**。⇒ 同一次输出里必须有**非零格**：整文件 md5 的差、
   以及测试段函数条数的差。两者若一起是「无差别」，说明我根本没比到东西。

用法：
    python3 evidence/K-R30-fn-body-md5.py <改前rev> <改后rev> [<文件路径>…]
    例：python3 evidence/K-R30-fn-body-md5.py 39ae7fd HEAD
"""
from __future__ import annotations

import hashlib
import re
import subprocess
import sys

DEFAULT_FILES = ["src-tauri/src/backend/control/local_backend.rs"]
SIG = re.compile(r"^(?:pub(?:\(crate\))? )?(?:unsafe )?fn ([A-Za-z0-9_]+)")


def show(rev: str, path: str) -> str:
    out = subprocess.run(
        ["git", "show", f"{rev}:{path}"], capture_output=True, check=True
    )
    return out.stdout.decode("utf-8")


def split_prod_test(src: str) -> tuple[str, str]:
    """按 `#[cfg(test)]` 那一行切成生产段 / 测试段。切不动就整份算生产段并出声。"""
    marker = "#[cfg" + "(test)]"
    at = src.find(marker)
    if at < 0:
        print(f"  ⚠ 这一份里找不到 `{marker}` —— 整份当生产段算，下面的分母要按这个读")
        return src, ""
    return src[:at], src[at:]


def bodies(section: str) -> dict[str, str]:
    """`{函数名: 体的 md5}`。名字重复 ⇒ 直接退非零（不许静默取第一处）。"""
    lines = section.splitlines(keepends=True)
    starts: list[tuple[int, str]] = []
    for i, ln in enumerate(lines):
        m = SIG.match(ln)
        if m:
            starts.append((i, m.group(1)))
    out: dict[str, str] = {}
    for i, name in starts:
        j = i + 1
        while j < len(lines) and lines[j].rstrip("\n") != "}":
            j += 1
        blob = "".join(lines[i : j + 1]).encode("utf-8")
        if name in out:
            print(f"  🔴 函数名重复：{name} —— 分母不干净，停")
            raise SystemExit(3)
        out[name] = hashlib.md5(blob).hexdigest()
    return out


def count_test_fns(section: str) -> int:
    return len(re.findall(r"^\s+fn [A-Za-z0-9_]+", section, re.M))


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    before, after = sys.argv[1], sys.argv[2]
    files = sys.argv[3:] or DEFAULT_FILES
    print("=" * 78)
    print(f"K-R30 · 改动面对拍：{before} → {after}")
    print("=" * 78)
    diff_cells = 0
    for path in files:
        a, b = show(before, path), show(after, path)
        pa, ta = split_prod_test(a)
        pb_, tb = split_prod_test(b)
        fa, fb = bodies(pa), bodies(pb_)
        both = sorted(set(fa) & set(fb))
        only_a = sorted(set(fa) - set(fb))
        only_b = sorted(set(fb) - set(fa))
        changed = [n for n in both if fa[n] != fb[n]]
        print(f"\n── {path} ──")
        print(f"· 分母（两边都有的生产段函数）：{len(both)} 个"
              f"（改前 {len(fa)} · 改后 {len(fb)}）")
        print(f"· 只在改前有：{only_a or '（无）'}")
        print(f"· 只在改后有：{only_b or '（无）'}")
        print(f"· **体变了的**：{len(changed)} 个 —— {changed or '（一个都没有）'}")
        wa = hashlib.md5(a.encode()).hexdigest()
        wb = hashlib.md5(b.encode()).hexdigest()
        print(f"· 🔴 非空对照① 整文件 md5：{wa[:12]} → {wb[:12]}"
              f"  {'**不同**' if wa != wb else '相同'}")
        na, nb = count_test_fns(ta), count_test_fns(tb)
        print(f"· 🔴 非空对照② 测试段函数条数：{na} → {nb}"
              f"  差 {nb - na:+d}  {'**非零**' if nb != na else '零'}")
        if wa != wb:
            diff_cells += 1
        if nb != na:
            diff_cells += 1
    print()
    if diff_cells == 0:
        print("🔴 一格非零对照都没有 —— 这一趟根本没比到东西，上面的「全同」不算读数。")
        return 4
    print(f"· 本次输出里的非零格：{diff_cells} 个 ⇒ 上面那些「全同」是真的比出来的。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
