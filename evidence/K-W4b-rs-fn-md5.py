#!/usr/bin/env python3
"""K-W4b 的量具：`src-tauri/src/sftp.rs` **逐顶层函数整块 md5**，基点 vs 工作树对拍。

它买的是 `KW4bD5②` 那一句：**「判定口径与两条部署路的行为一个字节没动」不靠人读 diff，靠哈希。**
`deploy_decision` / `deploy_decision_at` / `marker_path` / `remote_parent` 四块整块 md5
与基点逐个相同，那句话才算有读数。
（`brief` 第 11 条：判「动没动某个闭集」不许用 `grep` 数加行 —— 多行字面量会漏。
 `N-G1` 那条订正：别用 `git diff --stat`，「行数变没变」与「这一块变没变」是两件事。）

⚠ 射程（别读宽）：
  · 它认的函数是**顶层**（第 0 列）`[pub[(crate)] ][async ]fn 名字(` 这一形，块尾认第 0 列的 `}`。
    `impl` / `mod tests` 里缩进的函数**一律不在分母里** —— 本量具报的数只覆盖顶层函数。
  · 它比的是**整块字节（含函数体内的注释与空白，不含函数上方的 `///` 头注）**
    ⇒ 只改函数体里的注释也会判「变了」；只改头注不会。
  · 同名函数只留最后一个（本文件顶层无重名，09-06 现打已验：29 个名字 / 29 块）。

用法（住址即被测对象，前两个参数都要写全，别省）：
  python3 evidence/K-W4b-rs-fn-md5.py <基点 rev> <文件相对路径> [第二个 rev]
  例：python3 evidence/K-W4b-rs-fn-md5.py 77220e6 src-tauri/src/sftp.rs
      （第三个参数省略 = 拿**当前工作树**上那一份比）
  ⚠ 跑它的**当前目录决定被测对象是哪棵树** —— 交回读数时把「在哪棵树上跑的」一起写；
    本量具会把工作树那一份的绝对路径原样印出来，就是为了让那个分母跑不掉。
"""
from __future__ import annotations

import hashlib
import pathlib
import re
import subprocess
import sys

DEF = re.compile(r"^(?:pub(?:\([a-z]+\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*[(<]")


def funcs(text: str) -> dict[str, tuple[str, int]]:
    out: dict[str, tuple[str, int]] = {}
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        m = DEF.match(lines[i])
        if m:
            j = i
            while j < len(lines) and lines[j] != "}":
                j += 1
            body = "\n".join(lines[i:j + 1])
            out[m.group(1)] = (hashlib.md5(body.encode()).hexdigest()[:12], body.count("\n") + 1)
            i = j
        i += 1
    return out


def load(rev: str | None, target: str) -> str:
    if rev is None:
        return pathlib.Path(target).read_text(encoding="utf-8")
    p = subprocess.run(["git", "show", f"{rev}:{target}"], capture_output=True, text=True)
    if p.returncode != 0:
        raise SystemExit(f"取不到 {rev}:{target} —— {p.stderr.strip()}")
    return p.stdout


def main() -> int:
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    base_rev, target = sys.argv[1], sys.argv[2]
    other_rev = sys.argv[3] if len(sys.argv) > 3 else None
    a = funcs(load(base_rev, target))
    b = funcs(load(other_rev, target))
    where = other_rev or f"工作树 {pathlib.Path(target).resolve()}"
    print(f"基点 {base_rev}:{target} —— {len(a)} 个顶层函数")
    print(f"对拍 {where} —— {len(b)} 个顶层函数")
    changed, gone, added, same = [], [], [], []
    for name, (h, _n) in a.items():
        if name not in b:
            gone.append(name)
        elif b[name][0] != h:
            changed.append((name, h, a[name][1], b[name][0], b[name][1]))
        else:
            same.append(name)
    for name in b:
        if name not in a:
            added.append(name)
    print(f"  相同 {len(same)} · 变了 {len(changed)} · 没了 {len(gone)} · 新增 {len(added)}")
    for name, h1, n1, h2, n2 in changed:
        print(f"  变了  {name:34s} {h1}({n1} 行) -> {h2}({n2} 行)")
    for name in gone:
        print(f"  没了  {name}")
    for name in added:
        print(f"  新增  {name:34s} {b[name][0]}({b[name][1]} 行)")
    pinned = ["deploy_decision", "deploy_decision_at", "marker_path", "remote_parent"]
    print("KW4bD5② 钉住的四块：")
    bad = 0
    for name in pinned:
        ha = a.get(name, ("<不在基点里>", 0))
        hb = b.get(name, ("<不在对拍面里>", 0))
        ok = ha[0] == hb[0] and name in a and name in b
        bad += 0 if ok else 1
        print(f"  {'同' if ok else '异'}  {name:22s} 基点 {ha[0]}({ha[1]} 行)  对拍 {hb[0]}({hb[1]} 行)")
    print(f"KW4bD5②: {'OK —— 四块逐个 md5 相同' if bad == 0 else f'FAIL —— {bad} 块变了'}")
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
