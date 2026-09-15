#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R43 量具：改动面 —— 哪几个 `fn` 变了，逐函数 md5。

住址（唯一）：`<代码仓工作树>/evidence/K-R43-changed-surface.py`
被测对象：`--tree`（缺省 = 本文件的上一级目录）· 基线 `--base`（缺省 `8b474e2`）。

分母怎么数的
------------
分母 = 三份写区文件里**每一个顶格 / 四格缩进的 `fn` 定义**（`fn 名(` 那一行起，
到与它同缩进的 `}` 行止）。`brief` 要的「`ast` 逐函数 md5」是 Python 的说法；
Rust 没有现成 `ast`，这里用**同一套括号切法**代替，并把切法写在这里。

⚠ 射程（写死别读宽）
--------------------
· 它按**缩进**认函数边界（本仓 rustfmt 之后成立），不是真解析器 ⇒
  宏体里 / 奇怪缩进里的 `fn` 它认不出来。**两侧函数名集合的差**会把这类漏说出来。
· md5 算的是**含注释与空白的原文** ⇒ 「只改了 docstring」也会算成「这个函数变了」。
  哪几个是**只动文字**，本量具答不了，交回时逐个说。
"""
import argparse
import hashlib
import pathlib
import re
import subprocess
import sys

FILES = [
    "src-tauri/src/local_daemon.rs",
    "src-tauri/src/backend/control/local_backend.rs",
    "src-tauri/src/write_site_registry.rs",
]
FN = re.compile(r"^(\s*)(?:pub(?:\([^)]*\))? )?(?:async )?fn ([A-Za-z_][A-Za-z0-9_]*)")


def fns(src: str):
    lines = src.split("\n")
    out = {}
    for i, l in enumerate(lines):
        m = FN.match(l)
        if not m:
            continue
        indent = m.group(1)
        if len(indent) not in (0, 4):
            continue
        closer = indent + "}"
        for j in range(i + 1, len(lines)):
            if lines[j] == closer:
                body = "\n".join(lines[i:j + 1])
                out.setdefault(m.group(2), []).append(hashlib.md5(body.encode()).hexdigest()[:10])
                break
    return {k: ",".join(v) for k, v in out.items()}


def main():
    ap = argparse.ArgumentParser()
    here = pathlib.Path(__file__).resolve().parent
    ap.add_argument("--tree", default=str(here.parent))
    ap.add_argument("--base", default="8b474e2")
    a = ap.parse_args()
    tree = pathlib.Path(a.tree).resolve()
    print(f"[K-R43 改动面] 被测对象那棵树 = {tree}")
    print(f"[K-R43 改动面] 量具自己住 = {pathlib.Path(__file__).resolve()}")
    print(f"[K-R43 改动面] 基线 = {a.base}\n")
    tot_same = tot_chg = tot_add = tot_del = 0
    for rel in FILES:
        base = subprocess.run(["git", "-C", str(tree), "show", f"{a.base}:{rel}"],
                              capture_output=True, text=True, check=True).stdout
        now = (tree / rel).read_text(encoding="utf-8")
        b, n = fns(base), fns(now)
        chg = sorted(k for k in b.keys() & n.keys() if b[k] != n[k])
        add = sorted(n.keys() - b.keys())
        rm = sorted(b.keys() - n.keys())
        same = len(b.keys() & n.keys()) - len(chg)
        tot_same += same; tot_chg += len(chg); tot_add += len(add); tot_del += len(rm)
        print(f"── {rel}")
        print(f"   分母：基线 {len(b)} 个 fn 名 · 现在 {len(n)} 个（按上面那套缩进切法）")
        print(f"   没动 {same} · 变了 {len(chg)} · 新增 {len(add)} · 没了 {len(rm)}")
        for k in chg:
            print(f"   变了  {k}  md5 {b[k]} → {n[k]}")
        for k in add:
            print(f"   新增  {k}  md5 {n[k]}")
        for k in rm:
            print(f"   没了  {k}  md5 {b[k]}")
        print()
    print(f"合计：没动 {tot_same} · 变了 {tot_chg} · 新增 {tot_add} · 没了 {tot_del}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
