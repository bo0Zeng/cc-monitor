#!/usr/bin/env python3
"""W-F1b 摸底量具 ⑤：**零生产改动**（`WF1bD5`）+ 它的死值验（`W1bM3`）。

`WF1bD5` 判定逐字：`git diff <基点> -- src src-tauri/src remote-daemon-proto/src e2e` **为空**。
`WF1bD5` 的「acceptor 怎么失效」逐字：「可以『顺手修一下』⇒ **三个口径各给一次**
（`git status` · 逐文件 md5 · `git diff`）。」
⇒ 本量具就是那三个口径，写成一份可重跑的东西。

## 🔴 逐文件那把尺子为什么必须 `-z`

本仓路径**全是中文**。裸 `git ls-files` 会把非 ASCII 路径**加引号并转义**
（`"src-tauri/src/\344\270\255..."`），照那个字符串去开文件当场 `FileNotFoundError`，
而「打不开」与「不一样」在一张 DIFF 表上长得一模一样 ——
`N-G2` 那把尺子的初版正是这样读出 **5 份假 `DIFF`** 的。所以人群一律走 `git ls-files -z`。

## 死值验 `W1bM3`：改一行不提交，三个口径各认得出

`--mutate` 模式：往一份**生产面**文件尾部追加一行注释 → 三个口径各打一次 → **原样写回** →
再打一次，并断言 `git status` 回到空、该文件 md5 回到原值。
⚠ 追加的那一行是 `//` 注释，**不改语义**；而本量具**不编译**它 ——
它证的是「三把尺子看得见一次改动」，不是「改了还编得过」。
⚠ 写回不是「再改一次」，是**把开工时读到的原始字节整份写回**，并**用 md5 核**它真的回去了。

## 分母

  · `git status --porcelain -z` 的分母 = 整棵工作树。
  · 逐文件 md5 的分母 = `git ls-files -z -- <四条生产面路径>` 给出的**跟踪文件**
    （未跟踪文件不在这个分母里 —— 它们由 `git status` 那一口径管）。
  · `git diff` 的分母 = 命令自己点名的那四条路径。
  三个分母**不同**，这是有意的：三口径各盖一块，合起来才盖满。

## 被测对象指向哪棵树

`WT` 常量 = `/home/zbl/文档/claudecode-frontend/.claude/worktrees/w-f1b`，基点 `BASE`。

用法：  python3 evidence/W-F1b-zero-production-diff.py
       python3 evidence/W-F1b-zero-production-diff.py --mutate    # 死值验 W1bM3
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path

WT = Path("/home/zbl/文档/claudecode-frontend/.claude/worktrees/w-f1b")
BASE = "d73ea46"
PROD = ["src", "src-tauri/src", "remote-daemon-proto/src", "e2e"]
# 死值验切在哪一份：生产面里一份与本件无关的小文件（**不是** `e2e/weak-net/` 的台架代码）
MUTATE_TARGET = "src-tauri/src/pubkey.rs"


def git(args: list[str], binary: bool = False):
    p = subprocess.run(["git"] + args, cwd=WT, capture_output=True)
    if binary:
        return p.returncode, p.stdout
    return p.returncode, p.stdout.decode("utf-8", "replace")


def gauge_status() -> dict:
    """口径①：`git status --porcelain -z`（分母 = 整棵工作树）。"""
    rc, out = git(["status", "--porcelain", "-z"], binary=True)
    entries = [e.decode("utf-8", "replace") for e in out.split(b"\0") if e]
    prod_hits = [e for e in entries
                 if any(e[3:].startswith(p + "/") or e[3:] == p for p in PROD)]
    return {"rc": rc, "entries": entries, "prod_hits": prod_hits,
            "prod_clean": not prod_hits}


def gauge_diff() -> dict:
    """口径③：`git diff <基点> -- <四条生产面路径>`（分母 = 命令点名的那四条）。"""
    rc, out = git(["diff", BASE, "--"] + PROD)
    rc2, stat = git(["diff", "--stat", BASE, "--"] + PROD)
    return {"rc": rc, "empty": out.strip() == "", "bytes": len(out),
            "stat": stat.strip()}


def gauge_md5() -> dict:
    """口径②：逐文件 md5（工作树 vs 基点那棵树），人群走 `git ls-files -z`。"""
    rc, out = git(["ls-files", "-z", "--"] + PROD, binary=True)
    files = [f.decode("utf-8", "replace") for f in out.split(b"\0") if f]
    diffs, missing = [], []
    for rel in files:
        p = WT / rel
        if not p.is_file():
            missing.append(rel)
            continue
        now = hashlib.md5(p.read_bytes()).hexdigest()
        rc_b, blob = git(["cat-file", "blob", f"{BASE}:{rel}"], binary=True)
        if rc_b != 0:
            missing.append(rel + " (基点上无此 blob)")
            continue
        if hashlib.md5(blob).hexdigest() != now:
            diffs.append({"file": rel, "base": hashlib.md5(blob).hexdigest(),
                          "now": now})
    return {"population": len(files), "diffs": diffs, "missing": missing,
            "all_same": not diffs and not missing}


def snapshot(tag: str) -> dict:
    return {"tag": tag, "status": gauge_status(), "md5": gauge_md5(),
            "diff": gauge_diff()}


def show(s: dict) -> None:
    st, m5, df = s["status"], s["md5"], s["diff"]
    print(f"  【{s['tag']}】")
    print(f"    口径① git status  ：生产面命中 {len(st['prod_hits'])} 处"
          f"{'（' + ', '.join(st['prod_hits']) + '）' if st['prod_hits'] else ''}"
          f" · 全树条目 {len(st['entries'])} 条 ⇒ 生产面干净={st['prod_clean']}")
    print(f"    口径② 逐文件 md5 ：人群 {m5['population']} 份 · 不同 {len(m5['diffs'])} 份"
          f"{'（' + ', '.join(d['file'] for d in m5['diffs']) + '）' if m5['diffs'] else ''}"
          f" · 缺 {len(m5['missing'])} 份 ⇒ 全同={m5['all_same']}")
    print(f"    口径③ git diff   ：退出码 {df['rc']} · {df['bytes']} 字节 ⇒ 为空={df['empty']}"
          f"{'  ' + df['stat'] if df['stat'] else ''}")


def main() -> int:
    rc, head = git(["rev-parse", "HEAD"])
    rc, base_full = git(["rev-parse", BASE])
    rep = {"worktree": str(WT), "head": head.strip(), "base": base_full.strip(),
           "prod_paths": PROD}

    print("=" * 84)
    print(f"W-F1b · 零生产改动三口径    树={WT}")
    print(f"HEAD={head.strip()}    基点 {BASE}={base_full.strip()}")
    print("=" * 84)

    rep["clean"] = snapshot("交回态（应当：三口径全绿）")
    show(rep["clean"])

    if "--mutate" in sys.argv:
        print("-" * 84)
        print(f"【死值验 W1bM3】往 {MUTATE_TARGET} 尾部追加一行注释，不提交")
        target = WT / MUTATE_TARGET
        original = target.read_bytes()
        orig_md5 = hashlib.md5(original).hexdigest()
        try:
            target.write_bytes(original + b"\n// W-F1b W1bM3 death-value line\n")
            rep["mutated"] = snapshot("改了一行·未提交（应当：三口径全红）")
            show(rep["mutated"])
        finally:
            target.write_bytes(original)
        back_md5 = hashlib.md5(target.read_bytes()).hexdigest()
        rep["restored"] = snapshot("已原样写回（应当：回到全绿）")
        show(rep["restored"])
        rep["restore_ok"] = back_md5 == orig_md5
        print(f"    写回核对：原 md5={orig_md5} 现 md5={back_md5} ⇒ 一致={rep['restore_ok']}")

        m = rep["mutated"]
        caught = {
            "口径① git status": not m["status"]["prod_clean"],
            "口径② 逐文件 md5": not m["md5"]["all_same"],
            "口径③ git diff": not m["diff"]["empty"],
        }
        rep["W1bM3_all_three_caught"] = all(caught.values())
        print(f"    三口径各认得出这一刀：{caught} ⇒ 全中={rep['W1bM3_all_three_caught']}")

    print("=" * 84)
    print(json.dumps(rep, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
