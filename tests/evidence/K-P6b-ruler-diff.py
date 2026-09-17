#!/usr/bin/env python3
"""K-P6b 量具①：`§0c` 那两把对不上的尺子，**正面对照**。

## 它答哪一格

`KP6bD1①` 逐字：「`§0c` 那两格差异重打，**说清哪把尺子对、另一把差在哪**（给命令与口径）」。

盘上有两组数，同一个符号读出两个值：

| 符号 | `K-P6 §0-订正` | `K-P6b §0c`（PM 09-06 现打） |
|---|---|---|
| `connect_session` | 7 处 / 3 份 | 生产 4 处 / 3 份 |
| `connect_and_exec_cmd` | 18 处 / 11 份 | 生产 18 处 / 11 份 |
| `connect_sftp` | 14 处 / 4 份 | 生产 15 处 / 4 份 |

`§0c` 从「中间那格逐字相同」推出「那一组是生产段口径」。**本量具要检的正是这个推断** ——
两把尺子在一个符号上撞出同一个数，可以是同口径，也可以是**两处差额恰好抵消**。

## 怎么答：把尺子拆成三根独立的轴，全组合打一遍

一把「数某个符号有几处」的尺子，在本仓至少由三根轴定死。`§0c` 与 `K-P6` 各自只写了其中两根，
剩下那根两边都没写 —— 差额就藏在没写的那根上。

- **轴 A · 测试段怎么切**（三档）
  · `none` —— 不切，报整份文件（`K-P6-dial-census.py` 头注第 35–38 行写死的口径）
  · `cheap` —— **按第一个 `#[cfg(test)]` 一刀切到文件尾**（`§0c` 逐字写的那把）
  · `guard` —— `guard_core::production_code` 的剥法：**逐个**剥掉带花括号体的
    `#[cfg(test)] mod X { … }`，其余原样留下（本量具按 `test_module_ranges` 逐字镜像）
- **轴 B · 注释剥不剥**（两档）：`keep`（`§0c` 逐字「不剥注释」）/ `strip`（只认 Rust 词法的 `code` 态）
- **轴 C · 什么算一处**（四档）：`call`（`<名>(`，且不是定义行）/ `call+def` /
  `paren`（后面紧跟 `(` 就算，不问定义还是调用 —— 就是 `grep -o '<名>('`）/
  `any`（连 `use` 引入这类光提一嘴的也算）

⇒ 3 × 2 × 4 = **24 个组合**，每个符号打满 24 格。哪一格等于盘上哪一组，机器自己点名，
**不靠我事后挑一格说「你看对上了」**；最后那张【认尺子】表再要求**三个符号同时对上**，
免得拿「一个符号撞对了」当「找到那把尺子了」（`§0c` 犯的正是这一条）。

## 轴 A 的 `cheap` 那一档为什么是错的 —— 这件事仓里早就证过

`ssh_source.rs` 自己的 `write_half_guard` 模块头注逐字（本树 `src-tauri/src/ssh_source.rs:3189`，
逐字校验位那一行是：`这不是洁癖：本文件第一个 #[cfg(test)] 模块在 800 行附近，而本护栏要扫的`）：

> 「本文件第一个 `#[cfg(test)]` 模块在 800 行附近，而本护栏要扫的 `parse_frame` /
>  `stream_loop` / `probe_daemon` 全在它**后面**。monitor 侧此前流行的那个近似
>  （`split("\\n#[cfg(test)]").next()`）会把扫描面**砍掉三分之二**」

而且它旁边就挂着一条**活的**测试（`the_shared_stripper_keeps_the_part_this_guard_must_scan`）
在天天跑这个差。⇒ `§0c` 那把尺子不是「另一种口径」，它是**仓里已判过错的那个近似**。

本量具把那条测试的断言**在 Python 侧原样复打一遍**（见 `--selftest` 的【自检 2】），
用的是同样三个锚点。两边独立实现、同一个结论 ⇒ 我这份镜像没写歪。

## ⚠ 反向自检（这份量具自己会不会给假读数）

`--selftest` 打三条，**三条都必须有一格是「不一样」的**（本仓纪律：量具交出
「全同 / 全 0 / 零命中」时，同一次输出里必须有一格非零，否则那份「全同」可能是台子没接上）：

1. **剥干净了吗** —— `guard` 剥完 + 剥注释之后 `#[test]` 必须是 **0**，而**剥之前**必须是**非零**
   （这就是那一格「非零」）。判据逐字镜像 `guard_core::assert_no_test_code`
   （本树 `src-tauri/crates/guard-core/src/lib.rs:762`，它断的是 `format!("#[{}]", "test")` ⇒ **只断 `#[test]`**）。
   ⚠ **`#[cfg(test)]` 刻意不进这条断言**：`test_module_ranges` 按契约**只剥带花括号体的
   `#[cfg(test)] mod X { … }`**，挂在别的 item 上的 `#[cfg(test)]` 原样留在「生产段」里。
   本树现打的活体：`ssh_source.rs` 的 `#[cfg(test)] const KNOWN_FRAME_KINDS`。
   ⇒ **`guard` 这一档也不是「生产段」的完美尺子**，它有一个已知的、向上偏的残留；
   本自检把残留数**印出来点名**，不判红（判红就是拿我自己的口径去驳仓里那条契约）。
2. **两种剥法真的不一样** —— 三个锚点在 `guard` 段里**在**、在 `cheap` 段里**不在**。
   全「在」或全「不在」都说明台子塌了。
3. **符号真的找得到** —— 三个被测符号在 `none/keep/any` 那一格上命中数必须都 > 0。

## ⚠ 保证不了什么（别读大一格）

- 它按**名字**找，不解析 `use` 别名、不解析函数指针（`let f = connect_sftp; f(cfg)` 数不到）。
  这条洞与 `K-P6-dial-census.py` 头注第 42 行那条**是同一个洞**，本量具不声称堵住它。
- 它答的是「这个名字写在什么位置上」，**不是**「这一处运行时真的会拨号」。
- `guard` 那一档是**镜像**，不是调用 Rust 那份。两份实现漂了它自己发现不了 ——
  兜底靠【自检 1】（剥完零残留）与【自检 2】（与仓里那条活测试同结论）。

## 住址与被测对象

量具住址：`evidence/K-P6b-ruler-diff.py`（工作树 `.claude/worktrees/k-p6b`，分支 `track/k-p6b`）。
被测对象由 `--tree` 给，默认取本脚本所在仓的根；`--rev` 可以把某个历史提交的那棵树取出来量
（用 `git show <rev>:<路径>`，**不切工作树**）。输出头一行印树的绝对路径 + 被测 rev。
"""

from __future__ import annotations

import argparse
import importlib.util
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve()

_spec = importlib.util.spec_from_file_location(
    "kp6_russh_ruler", HERE.parent / "K-P6-russh-ruler.py"
)
assert _spec and _spec.loader
_ruler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_ruler)
classify_bytes = _ruler.classify_bytes
word_hits = _ruler.word_hits
line_of = _ruler.line_of
CODE = _ruler.CODE
WORD_CHARS = _ruler.WORD_CHARS

SYMBOLS = ["connect_session", "connect_and_exec_cmd", "connect_sftp"]

# 盘上那两组数，逐字抄自件文件（`处/份`，只取生产那一格）。
ON_DISK = {
    "K-P6 §0-订正": {
        "connect_session": (7, 3),
        "connect_and_exec_cmd": (18, 11),
        "connect_sftp": (14, 4),
    },
    "K-P6b §0c 生产": {
        "connect_session": (4, 3),
        "connect_and_exec_cmd": (18, 11),
        "connect_sftp": (15, 4),
    },
}
# `§0c` 还报了测试那一格，一并对。
ON_DISK_TEST = {
    "connect_session": (4, 1),
    "connect_and_exec_cmd": (13, 6),
    "connect_sftp": (2, 1),
}

# 仓里那条活测试用的三个锚点（`ssh_source.rs` 的 `write_half_guard`）。
ANCHORS = ["fn parse_frame", "async fn stream_loop", "async fn probe_daemon"]

DEF_TAIL = ("fn ",)


# ---------------------------------------------------------------- 轴 A：测试段

def cfg_is_test_only(attr: str) -> bool:
    """镜像 `guard_core::cfg_is_test_only`：属性里出现独立的 `test` 词。"""
    b = attr
    ident = lambda c: c.isalnum() or c in "_-"
    i = 0
    while True:
        k = b.find("test", i)
        if k < 0:
            return False
        before_ok = k == 0 or not ident(b[k - 1])
        after = k + 4
        after_ok = after >= len(b) or not ident(b[after])
        if before_ok and after_ok:
            return True
        i = k + 1


def test_module_ranges(src: str) -> list[tuple[int, int]]:
    """逐字镜像 `guard_core::test_module_ranges`（本树
    `src-tauri/crates/guard-core/src/lib.rs:116`，逐字校验位：
    `fn test_module_ranges(src: &str) -> Vec<(usize, usize)> {`）。

    规则：`\\n#[cfg(` 开头、**下一行必须是** `mod X {`、`cfg` 里含独立的 `test`
    ⇒ 从属性行的前导换行处起，到**下一个列 0 的 `}`** 止。不满足就不成区间。
    """
    open_tok = "\n#[cfg("
    close_tok = "\n}"
    out: list[tuple[int, int]] = []
    i = 0
    while True:
        rel = src.find(open_tok, i)
        if rel < 0:
            return out
        j = rel

        def line_end(frm: int) -> int:
            k = src.find("\n", frm)
            return len(src) if k < 0 else k

        attr_start = j + 1
        attr_end = line_end(attr_start)
        mod_start = min(attr_end + 1, len(src))
        mod_end = line_end(mod_start)
        mod_line = src[mod_start:mod_end].strip()
        is_test_mod = (
            cfg_is_test_only(src[attr_start:attr_end])
            and mod_line.startswith("mod ")
            and mod_line.endswith("{")
        )
        if not is_test_mod:
            i = attr_end
            continue
        rel_end = src.find(close_tok, j)
        if rel_end < 0:
            out.append((j, len(src)))
            return out
        end = rel_end + len(close_tok)
        out.append((j, end))
        i = end


def guard_test_spans(src: str) -> list[tuple[int, int]]:
    return test_module_ranges(src)


def cheap_test_spans(src: str) -> list[tuple[int, int]]:
    """`§0c` 那把：第一个 `#[cfg(test)]` 起，一刀切到文件尾。"""
    k = src.find("\n#[cfg(test)]")
    if k < 0:
        return []
    return [(k, len(src))]


SPLITS = {
    "none": lambda src: [],
    "cheap": cheap_test_spans,
    "guard": guard_test_spans,
}


def in_spans(idx: int, spans: list[tuple[int, int]]) -> bool:
    return any(s <= idx < e for s, e in spans)


def apply_spans_out(src: str, spans: list[tuple[int, int]]) -> str:
    """把 spans 抠掉，剩下的接起来（= `production_source`）。"""
    out = []
    i = 0
    for s, e in spans:
        out.append(src[i:s])
        i = e
    out.append(src[i:])
    return "".join(out)


def strip_comments_by_lexer(src: str) -> str:
    """只留 `code` 态的字节，其余换成空格（等长 ⇒ 下标不动）。"""
    st = classify_bytes(src)
    return "".join(c if st[i] == CODE else " " for i, c in enumerate(src))


# ---------------------------------------------------------------- 轴 C：什么算一处

def hit_kind(src: str, idx: int, name: str, line_text: str) -> str:
    after = src[idx + len(name) : idx + len(name) + 1]
    # 定义行：这一行里有 `fn <name>`
    if f"fn {name}" in line_text:
        return "def"
    if after == "(":
        return "call"
    return "mention"


KINDS = {
    "call": {"call"},
    "call+def": {"call", "def"},
    "any": {"call", "def", "mention"},
    # `paren` = 「后面紧跟一个 `(`」，**不问它是定义还是调用** —— 这一档就是
    # `grep -o 'connect_session('` 那把最朴素的尺子。单列出来是因为它与 `call+def`
    # 只在一种情形上分岔：**字符串字面量里写着 `fn <名>`、后面却不跟 `(`**
    # （本树活体 `ssh_source.rs:1084` 的 `"pub(crate) async fn connect_session",`）。
    "paren": {"call", "def"},  # 真正的筛选在 census 里按 `after == "("` 再过一道
}


# ---------------------------------------------------------------- 人群

def tracked_rs(tree: pathlib.Path, rev: str | None) -> list[str]:
    if rev:
        raw = subprocess.run(
            ["git", "-C", str(tree), "ls-tree", "-r", "-z", "--name-only", rev],
            check=True, capture_output=True,
        ).stdout
    else:
        raw = subprocess.run(
            ["git", "-C", str(tree), "ls-files", "-z"], check=True, capture_output=True
        ).stdout
    names = [p.decode("utf-8", "surrogateescape") for p in raw.split(b"\0") if p]
    return sorted(n for n in names if n.endswith(".rs") and n.startswith("src-tauri/src/"))


def read_at(tree: pathlib.Path, rel: str, rev: str | None) -> str:
    if rev:
        r = subprocess.run(
            ["git", "-C", str(tree), "show", f"{rev}:{rel}"], check=True, capture_output=True
        )
        return r.stdout.decode("utf-8", "surrogateescape")
    return (tree / rel).read_text("utf-8", "surrogateescape")


# ---------------------------------------------------------------- 量

def census(files: dict[str, str], name: str, split: str, comments: str, kind: str):
    """返回 (生产处数, 生产份数, 测试处数, 测试份数, 生产逐处明细)。"""
    prod_hits, test_hits = [], []
    prod_files, test_files = set(), set()
    for rel, src in files.items():
        if name not in src:
            continue
        spans = SPLITS[split](src)
        hay = strip_comments_by_lexer(src) if comments == "strip" else src
        lines = src.splitlines()
        # 长名先吃掉：`connect_and_exec_cmd` 里含 `connect_and_exec`；本轮三个符号
        # 互不为前缀（`connect_session` / `connect_and_exec_cmd` / `connect_sftp`），
        # 但仍按标识符边界匹配，避免 `connect_sftp_pool` 这类被算进来。
        for idx in word_hits(hay, name):
            ln = line_of(src, idx)
            text = lines[ln - 1].strip() if ln - 1 < len(lines) else ""
            k = hit_kind(src, idx, name, text)
            if k not in KINDS[kind]:
                continue
            if kind == "paren" and src[idx + len(name) : idx + len(name) + 1] != "(":
                continue
            row = (rel, ln, k, text[:110])
            if in_spans(idx, spans):
                test_hits.append(row)
                test_files.add(rel)
            else:
                prod_hits.append(row)
                prod_files.add(rel)
    return len(prod_hits), len(prod_files), len(test_hits), len(test_files), prod_hits


def selftest(files: dict[str, str]) -> int:
    bad = 0
    w = 96
    print("=" * w)
    print("【自检】三条 —— 每条都要有一格是「不一样 / 非零」，否则本量具的读数不算数")
    print("=" * w)

    src = files.get("src-tauri/src/ssh_source.rs")
    if src is None:
        print("  ✗ 自检 0：找不到 ssh_source.rs —— 台子没接上")
        return 1

    # 自检 1：剥干净 + 剥之前非零
    raw_cfg = src.count("#[cfg(test)]")
    raw_test = src.count("#[test]")
    stripped = strip_comments_by_lexer(apply_spans_out(src, guard_test_spans(src)))
    left_cfg = stripped.count("#[cfg(test)]")
    left_test = stripped.count("#[test]")
    ok1 = raw_test > 0 and left_test == 0
    bad += 0 if ok1 else 1
    print(f"  自检 1 剥法（镜像 guard_core::assert_no_test_code —— 它**只断 `#[test]`**）")
    print(f"     剥之前：#[test] = {raw_test}   ← 这一格必须**非零**（台子接上了）")
    print(f"     剥之后：#[test] = {left_test}  ← 这一格必须是 0")
    print(f"     ⇒ {'PASS' if ok1 else '**FAIL**'}")
    print(f"     ⚠ 同时点名（**不判红**，这是 `test_module_ranges` 的契约本身）：")
    print(f"        `#[cfg(test)]` 剥之前 {raw_cfg} · 剥之后仍剩 {left_cfg} —— "
          f"挂在非 `mod {{}}` item 上的那种不剥")
    for a in ("KNOWN_FRAME_KINDS",):
        print(f"        本树活体：`#[cfg(test)] const {a}` —— "
              f"它连同它下面那一块常量数组，算在 `guard` 的「生产段」里")

    # 自检 2：两种剥法真的不一样（复打仓里那条活测试）
    good = apply_spans_out(src, guard_test_spans(src))
    cheap_spans = cheap_test_spans(src)
    cheap = apply_spans_out(src, cheap_spans)
    print(f"  自检 2 `guard` 与 `cheap` 的差（复打 ssh_source.rs 的 "
          f"`the_shared_stripper_keeps_the_part_this_guard_must_scan`）")
    ok2 = True
    for a in ANCHORS:
        g, c = a in good, a in cheap
        if not (g and not c):
            ok2 = False
        print(f"     `{a:<24}` guard 里 {'在' if g else '不在':<4} · cheap 里 {'在' if c else '不在':<4}"
              f"   {'✓' if (g and not c) else '**✗**'}")
    print(f"     生产段体量：整份 {len(src)} B · guard 留下 {len(good)} B "
          f"({len(good)*100//len(src)}%) · cheap 留下 {len(cheap)} B ({len(cheap)*100//len(src)}%)")
    print(f"     ⇒ {'PASS' if ok2 else '**FAIL**'}")
    bad += 0 if ok2 else 1

    # 自检 3：符号找得到
    print("  自检 3 三个被测符号在 `none/keep/any` 上的命中（必须都 > 0）")
    ok3 = True
    for s in SYMBOLS:
        n, nf, _, _, _ = census(files, s, "none", "keep", "any")
        if n == 0:
            ok3 = False
        print(f"     `{s:<22}` {n} 处 / {nf} 份  {'✓' if n else '**✗**'}")
    print(f"     ⇒ {'PASS' if ok3 else '**FAIL**'}")
    bad += 0 if ok3 else 1
    print("=" * w)
    return bad


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--tree", default=str(HERE.parent.parent))
    ap.add_argument("--rev", default=None, help="量某个历史提交的树（git show），不切工作树")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--detail", default=None, help="逐处列出这个符号在 guard/strip/call+def 下的生产命中")
    args = ap.parse_args()

    tree = pathlib.Path(args.tree).resolve()
    head = subprocess.run(
        ["git", "-C", str(tree), "rev-parse", args.rev or "HEAD"],
        check=True, capture_output=True, text=True,
    ).stdout.strip()

    rels = tracked_rs(tree, args.rev)
    files = {r: read_at(tree, r, args.rev) for r in rels}

    w = 96
    print("=" * w)
    print(f"K-P6b · `§0c` 两把尺子的正面对照     树={tree}")
    print(f"                                     被测 rev={args.rev or 'HEAD'}  ({head})")
    print("=" * w)
    print(f"【人群】`src-tauri/src/**.rs`（跟踪的）共 {len(rels)} 份 —— "
          f"两把尺子的人群**是同一个**，差额不在这里")
    print("-" * w)

    if args.selftest:
        bad = selftest(files)
        if bad:
            print(f"🔴 自检 {bad} 条不过 —— 下面的读数一律不算数")
            return 1

    if args.detail:
        n, nf, tn, tf, rows = census(files, args.detail, "guard", "strip", "call+def")
        print(f"【逐处】`{args.detail}` 在 `guard/strip/call+def` 下的**生产**命中 {n} 处 / {nf} 份")
        for rel, ln, k, text in rows:
            print(f"     {k:<8} {rel}:{ln}   {text}")
        print("-" * w)

    for name in SYMBOLS:
        print(f"### `{name}`")
        print(f"     {'轴A测试段':<10} {'轴B注释':<8} {'轴C算法':<10} "
              f"{'生产 处/份':<14} {'测试 处/份':<14} 命中盘上哪一组")
        for split in ("none", "cheap", "guard"):
            for comments in ("keep", "strip"):
                for kind in ("call", "call+def", "paren", "any"):
                    n, nf, tn, tf, _ = census(files, name, split, comments, kind)
                    tags = []
                    for label, table in ON_DISK.items():
                        if table[name] == (n, nf):
                            tags.append(label)
                    if ON_DISK_TEST[name] == (tn, tf) and split != "none":
                        tags.append("§0c 测试格")
                    mark = ("  ← " + " ＋ ".join(tags)) if tags else ""
                    print(f"     {split:<10} {comments:<8} {kind:<10} "
                          f"{f'{n} / {nf}':<14} {f'{tn} / {tf}':<14}{mark}")
        print("-" * w)

    # ---- 认尺子：要求**三个符号同时**对上，才算「这把尺子就是那一把」
    print("【认尺子】哪一格能把盘上那一组的**三个符号一起**打出来（只对上一个不算）")
    for label, table in ON_DISK.items():
        hits = []
        for split in ("none", "cheap", "guard"):
            for comments in ("keep", "strip"):
                for kind in ("call", "call+def", "paren", "any"):
                    ok = all(
                        census(files, s, split, comments, kind)[:2] == table[s]
                        for s in SYMBOLS
                    )
                    if ok:
                        # 顺带看这一格连测试格也对不对（`§0c` 才报了测试格）
                        t_ok = all(
                            census(files, s, split, comments, kind)[2:4] == ON_DISK_TEST[s]
                            for s in SYMBOLS
                        )
                        hits.append(f"{split}/{comments}/{kind}" + ("  ＋测试格也全对" if t_ok else ""))
        print(f"  『{label}』 生产三格全对的组合 {len(hits)} 个：")
        for h in hits:
            print(f"        {h}")
        if not hits:
            print("        （一个都没有 —— 那一组不是任何一把**自洽**尺子的产物）")
    print("=" * w)
    return 0


if __name__ == "__main__":
    sys.exit(main())
