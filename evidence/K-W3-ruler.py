#!/usr/bin/env python3
"""K-W3 摸底量具 —— 三把尺（尺A 原始行 / 尺B 非测试行 / 尺C 码行），**双口径**。

只读。不写任何被测文件。

住址 / 被测对象（第 12 条要求：量具住址 + 它的被测对象指向哪棵树）：
  本量具住 `evidence/K-W3-ruler.py`，被测对象 = **本量具所在的那棵工作树**
  （`git rev-parse --show-toplevel`，从本文件所在目录问起）；`--rev` 让它改从该仓的
  git blob 读内容，`--root` 换仓。⇒ 复跑时被测对象由住址唯一决定，不靠 cwd。

## 两个口径（报任何数必须说明用的哪一个）

- **口径 p（规划方口径）** = 逐字重实现 `scratchpad/measure_test_lines.py`（件文件 `§0-1`
  记的那把尺，`§3` 那两个已知答案锚点 328 / 4352 出自它）。**保留它的全部行为，包括缺陷。**
- **口径 k（本件订正口径）** = 同一把尺 + 一条修正：`#[cfg(test)]` 修饰的**分号结尾项**
  （`#[cfg(test)] \n mod foo;`）在那一行**结束**，不再往下吞到下一个带 `{` 的行为止。

⇒ 两个口径的**尺A 逐字节相同**（都是 `wc -l` 口径），只有尺B / 尺C 不同。

## 三把尺（定义逐字取自件文件 `§3`）

  尺A 原始行  = `find <dir> -name '*.rs' -print0 | xargs -0 wc -l`（换行符个数）
  尺B 非测试行 = 尺A 减 `#[cfg(test)]` 块（花括号深度配对到块尾）
  尺C 码行    = 尺B 再减空行与整行 `//`（与 `K-P2 §0b-0③` 同口径）

## 诚实边界（两个口径共有）

⚠ 花括号配对**不剔除**字符串字面量与块注释里的 `{` / `}` ⇒ 块会被算长（`selftest` 的
  夹具 C 把这条偏差**钉成期望值**，不许它悄悄变）。
⚠ 只认 `.rs`；`.ts` / `.toml` / `.md` 一概不进任何分母。
⚠ 只认 `#[cfg(test)]` 打头的 attribute 行；`#[cfg(all(test, …))]` / `#[cfg_attr(test, …)]`
  **两个口径都不认**（`audit-cfg` 子命令把它们点名出来，只出读数、不进分母）。
⚠ 它数的是**文本行**，不是语法项：`fn` 有几个它不知道。

用法：
  python3 evidence/K-W3-ruler.py selftest
  python3 evidence/K-W3-ruler.py dir  <相对路径…> [--rev REV] [--root REPO]
  python3 evidence/K-W3-ruler.py file <相对路径…> [--rev REV]
  python3 evidence/K-W3-ruler.py diff-caliper <相对路径> [--rev REV]   # 两口径逐行差
  python3 evidence/K-W3-ruler.py buckets [--rev REV]                   # KW3D1 三桶 + 「没判」桶
  python3 evidence/K-W3-ruler.py audit-cfg [<相对路径…>] [--rev REV]
"""
from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys

CFG_TEST_LOOSE_RE = re.compile(r"#\[cfg(_attr)?\(.*\btest\b")


def repo_root(explicit: str | None) -> str:
    if explicit:
        return os.path.abspath(explicit)
    here = os.path.dirname(os.path.abspath(__file__))
    out = subprocess.run(["git", "-C", here, "rev-parse", "--show-toplevel"],
                         capture_output=True, text=True, check=True)
    return out.stdout.strip()


def read_text(root: str, rel: str, rev: str | None) -> str:
    if rev:
        out = subprocess.run(["git", "-C", root, "show", f"{rev}:{rel}"],
                             capture_output=True, check=True)
        return out.stdout.decode("utf-8", errors="replace")
    with open(os.path.join(root, rel), "rb") as fh:
        return fh.read().decode("utf-8", errors="replace")


def list_rs(root: str, rel_dir: str, rev: str | None) -> list[str]:
    if rev:
        out = subprocess.run(
            ["git", "-C", root, "ls-tree", "-r", "--name-only", rev, "--", rel_dir],
            capture_output=True, text=True, check=True)
        files = [p for p in out.stdout.splitlines() if p.endswith(".rs")]
    else:
        files = []
        for dirpath, _d, filenames in os.walk(os.path.join(root, rel_dir)):
            for name in filenames:
                if name.endswith(".rs"):
                    files.append(os.path.relpath(os.path.join(dirpath, name), root))
    return sorted(files)


def split_lines(text: str) -> list[str]:
    """与 `wc -l` 同分母：数换行符，末尾换行不产生一行。"""
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines = lines[:-1]
    return lines


def mark_test(lines: list[str], caliper: str) -> list[bool]:
    """标出每一行是否落在 `#[cfg(test)]` 块内。caliper ∈ {'p','k'}。

    共有的状态机（逐字照规划方那把尺）：
      遇到 strip 后以 `#[cfg(test)]` 打头的行 ⇒ 进 pending，那一行记为测试行；
      pending 期间每一行都记为测试行，直到出现带 `{` 的行 ⇒ 用 `opens-closes` 起深度；
      depth<=0 那一行为块尾。

    口径 k 多的那一条修正：pending 期间遇到**不带 `{` 而带 `;`** 的行 ⇒ 该项在那一行结束
    （`#[cfg(test)] mod foo;` 这一形），不再往下吞。
    """
    n = len(lines)
    is_test = [False] * n
    in_block = pending = False
    depth = 0
    for i, line in enumerate(lines):
        s = line.strip()
        if not in_block and not pending and s.startswith("#[cfg(test)]"):
            pending = True
            is_test[i] = True
            continue
        if pending:
            is_test[i] = True
            opens = line.count("{")
            if opens > 0:
                depth = opens - line.count("}")
                pending = False
                in_block = depth > 0
                depth = max(depth, 0)
            elif caliper == "k" and ";" in line:
                pending = False          # ← 口径 k 唯一的修正
            continue
        if in_block:
            is_test[i] = True
            depth += line.count("{") - line.count("}")
            if depth <= 0:
                in_block = False
                depth = 0
            continue
    return is_test


def measure(text: str, caliper: str) -> dict:
    lines = split_lines(text)
    a = len(lines)
    is_test = mark_test(lines, caliper)
    test = sum(1 for f in is_test if f)
    blank_cmt = 0
    for i, line in enumerate(lines):
        if is_test[i]:
            continue
        s = line.strip()
        if s == "" or s.startswith("//"):
            blank_cmt += 1
    return {"a": a, "b": a - test, "c": a - test - blank_cmt,
            "test": test, "blank_cmt": blank_cmt}


def fake_ruler(text: str) -> int:
    """`§0-1` 那把被自打脸的假尺子：第一个 `#[cfg(test)]` 行到 EOF。只用于 selftest 对拍。"""
    lines = split_lines(text)
    for idx, line in enumerate(lines):
        if line.strip().startswith("#[cfg(test)]"):
            return len(lines) - idx
    return 0


def sum_dir(root: str, rel_dirs: list[str], rev: str | None, caliper: str) -> dict:
    files = []
    for d in rel_dirs:
        files += list_rs(root, d, rev)
    files = sorted(set(files))
    tot = {"files": len(files), "a": 0, "b": 0, "c": 0, "test": 0}
    for rel in files:
        m = measure(read_text(root, rel, rev), caliper)
        for k in ("a", "b", "c", "test"):
            tot[k] += m[k]
    return tot


# ------------------------------------------------------------------ KW3D1 三桶

REGISTRY_SUFFIX = "_registry.rs"
# 五对同名同职的宿主侧六文件。住址逐条见交回报告；名单逐字取自件文件 `§0-3` 候选③ 那张表。
PAIR_HOST_FILES = {
    "src-tauri/src/history.rs",
    "src-tauri/src/search.rs",
    "src-tauri/src/account_usage.rs",
    "src-tauri/src/local_accounts.rs",
    "src-tauri/src/accounts.rs",
    "src-tauri/src/watcher.rs",
}


def cmd_buckets(root: str, rev: str | None) -> None:
    files = list_rs(root, "src-tauri/src", rev)
    per = {}
    for rel in files:
        text = read_text(root, rel, rev)
        per[rel] = {"p": measure(text, "p"), "k": measure(text, "k")}

    def agg(sel, cal):
        t = {"files": 0, "a": 0, "b": 0, "c": 0}
        for rel in sel:
            t["files"] += 1
            for k in ("a", "b", "c"):
                t[k] += per[rel][cal][k]
        return t

    registry = [r for r in files if os.path.basename(r).endswith(REGISTRY_SUFFIX)]
    pairs = [r for r in files if r in PAIR_HOST_FILES]
    backend = [r for r in files
               if r.startswith("src-tauri/src/backend/") and r not in registry]
    rest = [r for r in files
            if r not in registry and r not in pairs and r not in backend]

    print(f"# src-tauri/src 分桶 @{rev or 'worktree'}（每桶给尺A · 尺C-p · 尺C-k）")
    groups = [("全体（分母）", files),
              ("桶3 判据面 *_registry.rs", registry),
              ("桶1a 五对·宿主侧六文件", pairs),
              ("桶1b backend/ 子树（去判据面）", backend),
              ("桶0 没判（其余）", rest)]
    for name, sel in groups:
        tp, tk = agg(sel, "p"), agg(sel, "k")
        print(f"{name}: 文件 {tp['files']} · 尺A {tp['a']}"
              f" · 尺B-p {tp['b']} · 尺C-p {tp['c']}"
              f" · 尺B-k {tk['b']} · 尺C-k {tk['c']}")
    for cal in ("p", "k"):
        s = {k: sum(agg(sel, cal)[k] for _n, sel in groups[1:])
             for k in ("files", "a", "b", "c")}
        allt = agg(files, cal)
        print(f"[口径 {cal}] 四桶合计减全体（应全为 0）: "
              + " · ".join(f"{k}={s[k] - allt[k]}" for k in ("files", "a", "b", "c")))
    print()
    print("# 桶0「没判」逐文件（尺C-k 降序；本量具不猜归属，人工归桶用）")
    for rel in sorted(rest, key=lambda r: -per[r]["k"]["c"]):
        m = per[rel]
        print(f"{m['k']['c']:6d} {m['k']['b']:6d} {m['k']['a']:6d}  {rel}")


def cmd_diff_caliper(root: str, rels: list[str], rev: str | None) -> None:
    """逐行点出两口径判定不同的行 —— 即规划方那把尺**多吞**的行。"""
    grand = 0
    for rel in rels:
        text = read_text(root, rel, rev)
        lines = split_lines(text)
        tp, tk = mark_test(lines, "p"), mark_test(lines, "k")
        bad = [i for i in range(len(lines)) if tp[i] != tk[i]]
        if not bad:
            continue
        grand += len(bad)
        print(f"--- {rel} @{rev or 'worktree'}: 两口径判定不同 {len(bad)} 行")
        runs = []
        for i in bad:
            if runs and i == runs[-1][1] + 1:
                runs[-1][1] = i
            else:
                runs.append([i, i])
        for lo, hi in runs:
            print(f"    行 {lo + 1}-{hi + 1}（{hi - lo + 1} 行）"
                  f" 口径p 判测试 / 口径k 判非测试；首行逐字: {lines[lo].strip()[:70]!r}")
    print(f"[diff-caliper] 合计 {grand} 行（分母 = 被扫文件的尺A 之和）")


def cmd_needle(root: str, needles: list[str], rels: list[str], rev: str | None) -> None:
    """在**生产段**（剥掉 `#[cfg(test)]` 块，口径 k）里数针的命中行数。

    ⚠ 射程：数的是「**含这个子串的行**」，不是「调用了这个函数」——
    注释里提到它也算（同 `local_read_surface_registry::hits()` 那条口径问题）。
    ⇒ 它买到的是「这份文件的生产段里还提不提这件事」，**不是**「它还实现着这件事」。
    """
    print(f"# 生产段针命中（口径 k 剥测试块）· 针 = {needles} · @{rev or 'worktree'}")
    print(f"# 分母 = 被扫文件的尺B-k 之和（每文件单列）")
    for rel in rels:
        text = read_text(root, rel, rev)
        lines = split_lines(text)
        is_test = mark_test(lines, "k")
        prod = [l for i, l in enumerate(lines) if not is_test[i]]
        hits = {n: sum(1 for l in prod if n in l) for n in needles}
        total = sum(hits.values())
        detail = " · ".join(f"{n}={hits[n]}" for n in needles)
        print(f"  尺B-k {len(prod):5d} · 命中合计 {total:4d}  [{detail}]  {rel}")


def cmd_audit_cfg(root: str, rels: list[str], rev: str | None) -> None:
    hard = soft = 0
    for rel in rels:
        for no, line in enumerate(read_text(root, rel, rev).split("\n"), 1):
            if CFG_TEST_LOOSE_RE.search(line):
                if line.strip().startswith("#[cfg(test)]"):
                    hard += 1
                else:
                    soft += 1
                    print(f"{rel}:{no}: 两口径都不认 ⇒ {line.strip()[:90]}")
    print(f"[audit-cfg] 认得的（strip 后以 #[cfg(test)] 打头）{hard} 处 · 不认的 {soft} 处"
          f" · 分母 = 被扫文件里含 test 的 cfg/cfg_attr attribute 行 {hard + soft} 处")


# ------------------------------------------------------------------ selftest

# 件文件 `§3` 写死的已知答案锚点（量于 4eb271f）。它们出自**口径 p**。
ANCHORS = [
    # rel, 尺A, 尺B-p, 测试-p, 假尺子, 尺B-k, 测试-k
    ("src-tauri/src/lib.rs", 2550, 2222, 328, 2469, 2256, 294),
    ("src-tauri/src/ssh_source.rs", 8206, 3854, 4352, 7292, 3865, 4341),
]

# 夹具 A：干净手数（无字符串花括号、无分号项）。两口径必须给同一答案。
FIX_A = "// c1\nfn keep() {}\n\n#[cfg(test)]\nmod t {\n    fn a() {}\n}\n\nfn keep2() {}\n"
FIX_A_EXPECT = {"a": 9, "test": 4, "b": 5, "blank_cmt": 3, "c": 2}

# 夹具 B：分号项 —— 两口径**必须给不同答案**，这是口径 k 那条修正的活体。
FIX_B = "#[cfg(test)]\nmod a;\nmod b;\nfn f() {}\n"
FIX_B_P = {"a": 4, "test": 4, "b": 0, "c": 0}
FIX_B_K = {"a": 4, "test": 2, "b": 2, "c": 2}

# 夹具 C：字符串里的 `{` —— 把已知偏差钉成期望值（两口径同吞）。
FIX_C = '#[cfg(test)]\nmod t {\n    fn a() { let s = "{"; }\n}\nfn after() {}\n'
FIX_C_EXPECT = {"a": 5, "test": 5, "b": 0, "c": 0}


def cmd_selftest(root: str) -> int:
    bad = 0

    def check(ok: bool, msg: str) -> None:
        nonlocal bad
        if not ok:
            bad += 1
        print(f"  {'OK ' if ok else 'BAD'} {msg}")

    print("== ① 件文件 §3 的已知答案锚点，量于 4eb271f（经 git blob 读，不动工作树）")
    for rel, ea, ebp, etp, efake, ebk, etk in ANCHORS:
        text = read_text(root, rel, "4eb271f")
        mp, mk = measure(text, "p"), measure(text, "k")
        f = fake_ruler(text)
        check((mp["a"], mp["b"], mp["test"], f) == (ea, ebp, etp, efake),
              f"{rel} 口径p: 尺A {mp['a']}/{ea} · 尺B {mp['b']}/{ebp}"
              f" · 测试 {mp['test']}/{etp} · 假尺子 {f}/{efake}"
              f"  ← §3 那两个数（328 / 4352）在这里兑现")
        check((mk["b"], mk["test"]) == (ebk, etk),
              f"{rel} 口径k: 尺B {mk['b']}/{ebk} · 测试 {mk['test']}/{etk}"
              f"（比口径p 少吞 {mp['test'] - mk['test']} 行）")

    print("== ② 尺A 与 `wc -l` 对拍（工作树；分母 = src-tauri/src 全部 .rs）")
    files = list_rs(root, "src-tauri/src", None)
    mine = sum(measure(read_text(root, rel, None), "k")["a"] for rel in files)
    wc = subprocess.run(
        "find src-tauri/src -name '*.rs' -print0 | xargs -0 wc -l | tail -1",
        shell=True, cwd=root, capture_output=True, text=True, check=True)
    wcn = int(wc.stdout.split()[0])
    check(mine == wcn, f"本量具尺A {mine} vs wc -l {wcn} · 文件 {len(files)}")

    print("== ③ 夹具 A 干净手数（两口径同答案）")
    for cal in ("p", "k"):
        m = measure(FIX_A, cal)
        check(all(m[k] == FIX_A_EXPECT[k] for k in FIX_A_EXPECT),
              f"口径{cal}: " + " · ".join(f"{k} {m[k]}/{FIX_A_EXPECT[k]}" for k in FIX_A_EXPECT))

    print("== ④ 夹具 B 分号项（两口径**必须**给不同答案 —— 口径 k 那条修正的活体）")
    mp, mk = measure(FIX_B, "p"), measure(FIX_B, "k")
    check(all(mp[k] == FIX_B_P[k] for k in FIX_B_P),
          f"口径p: " + " · ".join(f"{k} {mp[k]}/{FIX_B_P[k]}" for k in FIX_B_P)
          + "  ← 把 `mod b;` `fn f(){}` 两行也吞成测试")
    check(all(mk[k] == FIX_B_K[k] for k in FIX_B_K),
          f"口径k: " + " · ".join(f"{k} {mk[k]}/{FIX_B_K[k]}" for k in FIX_B_K))
    check(mp["c"] != mk["c"], f"两口径尺C 必须不等: p={mp['c']} k={mk['c']}")

    print("== ⑤ 夹具 C 字符串花括号（已知偏差钉成期望值，两口径同吞）")
    for cal in ("p", "k"):
        m = measure(FIX_C, cal)
        check(all(m[k] == FIX_C_EXPECT[k] for k in FIX_C_EXPECT),
              f"口径{cal}: " + " · ".join(f"{k} {m[k]}/{FIX_C_EXPECT[k]}" for k in FIX_C_EXPECT)
              + "  ← `fn after()` 被偏差吞掉，这是已登记的偏差、不是修好了的")

    print("== ⑥ 反例：假尺子在锚点上必须**错**（证明本量具不是那把假尺子）")
    text = read_text(root, "src-tauri/src/lib.rs", "4eb271f")
    mp = measure(text, "p")
    f = fake_ruler(text)
    check(f > mp["test"], f"真(口径p) {mp['test']} vs 假 {f} ⇒ 假的多报 {f - mp['test']} 行")

    print(f"\n[selftest] BAD={bad}")
    return bad


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["selftest", "dir", "file", "buckets",
                                    "audit-cfg", "diff-caliper", "needle"])
    ap.add_argument("paths", nargs="*")
    ap.add_argument("--needles", default=None,
                    help="needle 子命令用：逗号分隔的针")
    ap.add_argument("--rev", default=None)
    ap.add_argument("--root", default=None)
    args = ap.parse_args()
    root = repo_root(args.root)

    if args.cmd == "selftest":
        return 1 if cmd_selftest(root) else 0
    if args.cmd == "buckets":
        cmd_buckets(root, args.rev)
        return 0
    if args.cmd == "dir":
        for rel in args.paths:
            tp = sum_dir(root, [rel], args.rev, "p")
            tk = sum_dir(root, [rel], args.rev, "k")
            print(f"{rel} @{args.rev or 'worktree'}: 文件 {tp['files']} · 尺A {tp['a']}"
                  f" · 尺B-p {tp['b']} · 尺C-p {tp['c']}"
                  f" · 尺B-k {tk['b']} · 尺C-k {tk['c']}")
        return 0
    if args.cmd == "file":
        for rel in args.paths:
            text = read_text(root, rel, args.rev)
            tp, tk = measure(text, "p"), measure(text, "k")
            print(f"{rel} @{args.rev or 'worktree'}: 尺A {tp['a']}"
                  f" · 尺B-p {tp['b']} · 尺C-p {tp['c']} · 测试-p {tp['test']}"
                  f" · 尺B-k {tk['b']} · 尺C-k {tk['c']} · 测试-k {tk['test']}")
        return 0
    rels = []
    for p in (args.paths or ["src-tauri/src"]):
        rels += [p] if p.endswith(".rs") else list_rs(root, p, args.rev)
    if args.cmd == "audit-cfg":
        cmd_audit_cfg(root, rels, args.rev)
    elif args.cmd == "needle":
        if not args.needles:
            print("needle 子命令要 --needles=针1,针2", file=sys.stderr)
            return 2
        cmd_needle(root, args.needles.split(","), rels, args.rev)
    else:
        cmd_diff_caliper(root, rels, args.rev)
    return 0


if __name__ == "__main__":
    sys.exit(main())
