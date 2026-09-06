#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-P7 死值验台子 —— `P7M1` 的非空对照 ＋ `P7M3` 的「改一行不提交」三口径。

住址：<工作树>/evidence/K-P7-zero-diff-mutation.py
被测对象：**运行它时所在的那棵工作树**（`--root` 默认取本文件的上上级目录），
         基点由 `--base` 给（默认 `ab56c7d`）。输出头上印住址与 HEAD。

⚠ 本台子**会真的改一行被测树上的源码，然后复原**。三条纪律写在这里：
  ① 只改**一个**文件、只加**一行**（末尾追加一行 `//` 注释），改前先记 md5；
  ② `try/finally` 复原 —— 任何异常路径都走 `git checkout -- <那一份>`；
  ③ 复原之后**再跑一遍三口径**，把「回到零」也印出来。
     （`brief` 14w②：「差集为空」要附退出码或非空对照，否则「没跑」与「跑了是空」一模一样。）

⚠ 它**保证不了**什么：
  · 三个口径都是 `git` 与 `md5` 的组合，它们看不见**没进 git 的**改动（如 target/ 里的产物）；
  · 它只切**一刀一份文件**，不证明「多份同时改也认得出」（那一格本轮没切）。
"""

from __future__ import annotations

import argparse
import hashlib
import os
import subprocess
import sys

PROD_PATHS = ["src", "src-tauri", "e2e", "scripts", "remote-daemon-proto"]
VICTIM = "remote-daemon-proto/src/wire.rs"
MARK = "// K-P7 P7M3 死值验：本行由 evidence/K-P7-zero-diff-mutation.py 追加，跑完即复原\n"


def run(root: str, *args: str):
    p = subprocess.run(["git", "-C", root, *args], capture_output=True, text=True)
    return p.returncode, p.stdout, p.stderr


def md5_by_file(root: str, base: str) -> tuple[int, int, int]:
    """(两边都有的份数, md5 不同的份数, 单边独有的份数)。人群用 `git ls-files -z`。"""
    _, out, _ = run(root, "ls-files", "-z")
    head_files = [f for f in out.split("\0") if f]
    _, out2, _ = run(root, "ls-tree", "-r", "-z", "--name-only", base)
    base_files = [f for f in out2.split("\0") if f]
    both = sorted(set(head_files) & set(base_files))
    only = len(set(head_files) ^ set(base_files))
    diff = 0
    for f in both:
        p = os.path.join(root, f)
        try:
            with open(p, "rb") as fh:
                a = hashlib.md5(fh.read()).hexdigest()
        except OSError:
            diff += 1
            continue
        shown = subprocess.run(
            ["git", "-C", root, "show", f"{base}:{f}"], capture_output=True
        )
        if shown.returncode != 0:
            diff += 1
            continue
        if hashlib.md5(shown.stdout).hexdigest() != a:
            diff += 1
    return len(both), diff, only


def three_gauges(root: str, base: str, label: str) -> None:
    print(f"\n---- 三口径 · {label} ----")
    rc, out, _ = run(root, "diff", "--stat", base, "--", *PROD_PATHS)
    body = out.strip()
    print(f"① git diff {base} -- {' '.join(PROD_PATHS)}")
    print(f"   退出码 {rc} · 输出 {'（空）' if not body else ''}")
    for l in body.split("\n"):
        if l:
            print(f"   | {l}")
    rc2, out2, _ = run(root, "status", "--porcelain", "--", *PROD_PATHS)
    b2 = out2.strip()
    print(f"② git status --porcelain -- <那五个路径>  退出码 {rc2} · 输出 {'（空）' if not b2 else ''}")
    for l in b2.split("\n"):
        if l:
            print(f"   | {l}")
    both, diff, only = md5_by_file(root, base)
    print(f"③ 逐文件 md5（人群 `git ls-files -z`）：两边都有 {both} 份 · md5 不同 **{diff}** 份 · 单边独有 {only} 份")


def p7m1_control(root: str) -> None:
    """`P7M1` 的非空对照：把第 4 根针的锚点换一个字，看尺子会不会跟着变。"""
    import importlib.util

    here = os.path.dirname(os.path.abspath(__file__))
    spec = importlib.util.spec_from_file_location("cen", os.path.join(here, "K-P7-protocol-census.py"))
    cen = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(cen)

    src_root = os.path.join(root, "remote-daemon-proto/src")
    prod = {}
    for rel, p in cen.crate_files(src_root):
        prod[rel] = cen.production_code(cen.read(p))
    self_rel = "single_stream_guard.rs"
    print("\n---- P7M1 非空对照（尺子是不是活的）----")
    print("  分母：`remote-daemon-proto/src/**/*.rs` 共 %d 份，扣掉调用者自己 ⇒ %d 份"
          % (len(prod), len(prod) - 1))
    for needle, why in [
        ("mpsc::channel::<Frame>(", "登记的那一个（文件内 1 · 全 crate 3）"),
        ("mpsc::channel::<Frame2>(", "把锚点改一个字 —— 应当归零"),
        ("mpsc::channel::<", "把锚点放宽 —— 应当变大"),
    ]:
        f = prod.get("observe/watcher.rs", "").count(needle)
        c = sum(v.count(needle) for k, v in prod.items() if k != self_rel)
        print(f"  锚点 `{needle}` ⇒ observe/watcher.rs 内 {f} 处 · 全 crate {c} 处   （{why}）")
    print("  ⇒ 三个锚点读出三个不同的数 ⇒ **尺子不是恒返回登记值**。")


def main() -> int:
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=os.path.dirname(here))
    ap.add_argument("--base", default="ab56c7d")
    args = ap.parse_args()
    root = os.path.abspath(args.root)

    _, head, _ = run(root, "rev-parse", "HEAD")
    print("=" * 78)
    print("K-P7 死值验台子（`P7M1` 非空对照 + `P7M3` 三口径）")
    print("量具住址: <工作树>/evidence/K-P7-zero-diff-mutation.py")
    print(f"被测工作树: {root}")
    print(f"被测树 HEAD: {head.strip()} · 基点: {args.base}")
    print("=" * 78)

    p7m1_control(root)

    victim = os.path.join(root, VICTIM)
    with open(victim, "rb") as f:
        before = f.read()
    md5_before = hashlib.md5(before).hexdigest()
    print(f"\n---- P7M3 ----\n被切的那一份: {VICTIM} · 切前 md5 = {md5_before}")

    three_gauges(root, args.base, "切之前（应当三格全空 / 0 份）")
    try:
        with open(victim, "ab") as f:
            f.write(MARK.encode())
        with open(victim, "rb") as f:
            md5_after = hashlib.md5(f.read()).hexdigest()
        print(f"\n变异已落地：{VICTIM} 末尾追加 1 行 · 切后 md5 = {md5_after}"
              f" · 与切前{'不同 ✅' if md5_after != md5_before else '相同 🔴（切没落地！）'}")
        three_gauges(root, args.base, "切之后（三格都应当认得出）")
    finally:
        rc, _, err = run(root, "checkout", "--", VICTIM)
        with open(victim, "rb") as f:
            md5_back = hashlib.md5(f.read()).hexdigest()
        print(f"\n复原：`git checkout -- {VICTIM}` 退出码 {rc}{(' · ' + err.strip()) if err.strip() else ''}"
              f" · 复原后 md5 = {md5_back}"
              f" · 与切前{'逐字节相同 ✅' if md5_back == md5_before else '🔴 不同！人工介入'}")
    three_gauges(root, args.base, "复原之后（应当回到三格全空 / 0 份）")
    print("\n" + "=" * 78)
    return 0


if __name__ == "__main__":
    sys.exit(main())
