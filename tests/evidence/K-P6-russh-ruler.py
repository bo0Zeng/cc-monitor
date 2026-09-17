#!/usr/bin/env python3
"""K-P6 量具 ①：`russh` 的**逐处分类**尺子 —— `KP6D1` 要的那把「更准的」。

## 它替掉的是哪把尺子

件文件 `§0a③` 逐字写着 PM 用的那把：

    代码行 = `grep -n russh` 之后**剔掉行首是 `//` `/*` `*` 的**

并且 PM 自己标了它是粗口径：「**这把尺子会把行尾注释算进代码行**」。
它还有 PM 没说的**第二个**粗口径：**`russh` 是按子串匹配的**，
于是 `russh_sftp::client::SftpSession` 被算成一处 `russh` ——
而 `russh` 与 `russh_sftp` 是**两个 crate**（`src-tauri/Cargo.toml` 里两条独立依赖行），
本件 `§2.1` 恰好把 SFTP 划在写区之外 ⇒ 这两个数**必须分开**，否则
「界面那侧 `russh` 归零」这句话的分母里混着一批本件根本不该动的东西。

## 本尺子怎么量（口径逐条写死）

1. **人群**：`git ls-files -z` 拿**索引里跟踪的**文件（本仓路径含中文，`-z` 是必需的），
   再筛 `.rs`。默认作用域 = `src-tauri/src/`（`§0a③` 那把 grep 的作用域，逐字）；
   `--scope all` 可以把整棵树一起量（分母会变，输出里会写明是哪一个）。
2. **词法**：对每一份源码走一遍 **Rust 词法状态机**，每个字节标一个状态：
   `code` / `line_comment` / `doc_line_comment`(`///` `//!`) /
   `block_comment` / `doc_block_comment`(`/**` `/*!`) / `string` / `char`。
   块注释按 Rust 规则**可嵌套**；裸串 `r#"…"#`（任意个 `#`）、字节串 `b"…"`、
   生命周期 `'a`（不是字符字面量）都单独处理。
3. **词**：按**标识符边界**匹配，不是子串。边界字符集 = `[A-Za-z0-9_]`。
   于是 `russh_sftp` **不**算一处 `russh`（它是一个完整的标识符），
   `russh::client` 算一处 `russh`。两个词各自出一列。
4. **两个单位都报**：
   · **处**（occurrence）= 词出现几次；
   · **行**（line）= 有至少一处 `code` 状态命中的**不同行**数 —— 这是 `§0a③` 表里那个单位。
   混用这两个单位正是本仓「分母差」那一族，故两列并排印。

## 它同时把 PM 那把粗尺子**重打一遍**

`coarse_*` 列 = 逐字复现 `§0a③` 的算法（子串 `russh`，剔掉 `lstrip()` 后以
`//` / `/*` / `*` 打头的行）。**并排印**是刻意的：说「我这把更准」而不给出
「准在哪几处」，那只是一句形容词。差额逐处列在【④】里。

## ⚠ 本尺子**保证不了**什么（别读大一格）

- 它答的是「**这个词写在代码位置上**」，**不是**「这一行真的持有一个 russh 句柄」。
  `mcp.rs` 那一处是函数形参类型、`structural_scan.rs` 那一处是一个**字符串里的路径名**
  （本尺子会把它判成 `string`，正是差额之一）—— 谁是真句柄要人读，尺子只缩小人要读的面。
- 它**不**跨文件解析 `use` 别名：有人 `use russh as ssh;` 之后再用 `ssh::client`，
  本尺子数不到。`KP6D3` 的「判据要断拨号这件事、不是断一个词」说的就是这个洞，
  本尺子**不**声称堵住它。
- `#[cfg(test)]` 段**不剥**：本尺子报的是整份文件。要「只生产段」的读数得另一把尺子
  （仓里 `guard_core::production_code` 是那一把），本轮不需要，故不做。

## 住址与被测对象

量具住址：`evidence/K-P6-russh-ruler.py`（工作树 `.claude/worktrees/k-p6`）。
被测对象由 `--tree` 给，**默认取本脚本所在仓的根**（`evidence/` 的上一级）——
换树重跑只要把脚本连同那棵树一起用，或显式 `--tree`。
输出头一行会把**树的绝对路径 + HEAD sha** 印出来，别拿掉。
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys

WORD_CHARS = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_")
IDENT_START = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_")

CODE = "code"
LINE_COMMENT = "line_comment"
DOC_LINE_COMMENT = "doc_line_comment"
BLOCK_COMMENT = "block_comment"
DOC_BLOCK_COMMENT = "doc_block_comment"
STRING = "string"
CHAR = "char"

COMMENT_STATES = (LINE_COMMENT, DOC_LINE_COMMENT, BLOCK_COMMENT, DOC_BLOCK_COMMENT)
ALL_STATES = (
    CODE,
    LINE_COMMENT,
    DOC_LINE_COMMENT,
    BLOCK_COMMENT,
    DOC_BLOCK_COMMENT,
    STRING,
    CHAR,
)


def classify_bytes(src: str) -> list[str]:
    """给每个字符标一个词法状态。返回与 `src` 等长的状态表。

    状态机刻意写得笨而直白 —— 它要被人读懂，不是要快。
    """
    n = len(src)
    out = [CODE] * n
    i = 0
    while i < n:
        c = src[i]
        # ── 行注释 ──────────────────────────────────────────────
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            # `///` 是文档注释，但 `////`（四个及以上）在 Rust 里是普通注释。
            # `//!` 是内部文档注释。
            third = src[i + 2] if i + 2 < n else ""
            fourth = src[i + 3] if i + 3 < n else ""
            is_doc = (third == "/" and fourth != "/") or third == "!"
            state = DOC_LINE_COMMENT if is_doc else LINE_COMMENT
            j = src.find("\n", i)
            j = n if j < 0 else j
            for k in range(i, j):
                out[k] = state
            i = j
            continue
        # ── 块注释（可嵌套）────────────────────────────────────
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            third = src[i + 2] if i + 2 < n else ""
            fourth = src[i + 3] if i + 3 < n else ""
            # `/**/` 是空注释不是文档注释；`/***` 也不是。
            is_doc = (third == "*" and fourth not in ("/", "*")) or third == "!"
            state = DOC_BLOCK_COMMENT if is_doc else BLOCK_COMMENT
            depth = 1
            j = i + 2
            while j < n and depth > 0:
                if src[j] == "/" and j + 1 < n and src[j + 1] == "*":
                    depth += 1
                    j += 2
                    continue
                if src[j] == "*" and j + 1 < n and src[j + 1] == "/":
                    depth -= 1
                    j += 2
                    continue
                j += 1
            for k in range(i, min(j, n)):
                out[k] = state
            i = j
            continue
        # ── 裸串 r"…" / r#"…"# / br#"…"# ───────────────────────
        if c in ("r", "b") and _raw_prefix_at(src, i):
            i = _consume_raw_string(src, i, out)
            continue
        # ── 普通串 / 字节串 ────────────────────────────────────
        if c == '"' or (c == "b" and i + 1 < n and src[i + 1] == '"'):
            start = i
            i = i + 1 if c == '"' else i + 2
            while i < n:
                if src[i] == "\\":
                    i += 2
                    continue
                if src[i] == '"':
                    i += 1
                    break
                i += 1
            for k in range(start, min(i, n)):
                out[k] = STRING
            continue
        # ── 字符字面量 vs 生命周期 ─────────────────────────────
        if c == "'":
            end = _char_literal_end(src, i)
            if end is None:
                # 生命周期：`'a` / `'static` —— 是代码，照 CODE 走一个字符。
                i += 1
                continue
            for k in range(i, end):
                out[k] = CHAR
            i = end
            continue
        i += 1
    return out


def _raw_prefix_at(src: str, i: int) -> bool:
    """`i` 处是不是一个裸串前缀（`r"` / `r#` / `br"` / `br#`），且前一个字符不是标识符字符。"""
    if i > 0 and src[i - 1] in WORD_CHARS:
        return False
    j = i
    if src[j] == "b":
        j += 1
        if j >= len(src) or src[j] != "r":
            return False
    if j >= len(src) or src[j] != "r":
        return False
    j += 1
    while j < len(src) and src[j] == "#":
        j += 1
    return j < len(src) and src[j] == '"'


def _consume_raw_string(src: str, i: int, out: list[str]) -> int:
    n = len(src)
    start = i
    j = i
    if src[j] == "b":
        j += 1
    j += 1  # 'r'
    hashes = 0
    while j < n and src[j] == "#":
        hashes += 1
        j += 1
    j += 1  # 开引号
    closer = '"' + "#" * hashes
    end = src.find(closer, j)
    end = n if end < 0 else end + len(closer)
    for k in range(start, end):
        out[k] = STRING
    return end


def _char_literal_end(src: str, i: int) -> int | None:
    """`i` 处的 `'` 若是字符字面量，返回它结束后的下标；是生命周期则返回 None。"""
    n = len(src)
    j = i + 1
    if j >= n:
        return None
    if src[j] == "\\":
        j += 2
        # 转义可能是 `\u{1F600}`
        if j < n and src[j - 1] == "u" and src[j] == "{":
            close = src.find("}", j)
            if close < 0:
                return None
            j = close + 1
        return j + 1 if j < n and src[j] == "'" else None
    # 生命周期：`'` + 标识符 且后面不是 `'`
    if src[j] in IDENT_START:
        k = j
        while k < n and src[k] in WORD_CHARS:
            k += 1
        if k < n and src[k] == "'":
            return k + 1  # `'a'` 这种单字符字面量
        return None
    # 其余：`'x'` / `' '` / `'"'` …
    return j + 2 if j + 1 < n and src[j + 1] == "'" else None


def word_hits(src: str, word: str) -> list[int]:
    """按**标识符边界**找 `word`，返回起始下标表。"""
    out: list[int] = []
    start = 0
    while True:
        i = src.find(word, start)
        if i < 0:
            return out
        before_ok = i == 0 or src[i - 1] not in WORD_CHARS
        after = i + len(word)
        after_ok = after >= len(src) or src[after] not in WORD_CHARS
        if before_ok and after_ok:
            out.append(i)
        start = i + 1


def line_of(src: str, idx: int) -> int:
    return src.count("\n", 0, idx) + 1


def coarse_ruler(src: str, needle: str = "russh") -> tuple[int, int, list[int]]:
    """逐字复现 `§0a③` 那把粗尺子。返回 (总命中行, 代码行, 代码行的行号表)。

    PM 的算法：`grep -n <needle>` 拿到命中行，再剔掉 `lstrip()` 之后以
    `//` / `/*` / `*` 打头的行。**它按行数，不按处数**，且 `needle` 是子串。
    """
    total = 0
    code_lines: list[int] = []
    for no, line in enumerate(src.splitlines(), 1):
        if needle not in line:
            continue
        total += 1
        s = line.lstrip()
        if s.startswith("//") or s.startswith("/*") or s.startswith("*"):
            continue
        code_lines.append(no)
    return total, len(code_lines), code_lines


def tracked_rs_files(tree: pathlib.Path, scope: str) -> list[pathlib.Path]:
    raw = subprocess.run(
        ["git", "-C", str(tree), "ls-files", "-z"],
        check=True,
        capture_output=True,
    ).stdout
    names = [p for p in raw.split(b"\0") if p]
    out: list[pathlib.Path] = []
    for b in names:
        rel = b.decode("utf-8", "surrogateescape")
        if not rel.endswith(".rs"):
            continue
        if scope == "src-tauri" and not rel.startswith("src-tauri/src/"):
            continue
        out.append(pathlib.Path(rel))
    return sorted(out)


def main() -> int:
    here = pathlib.Path(__file__).resolve()
    ap = argparse.ArgumentParser()
    ap.add_argument("--tree", default=str(here.parent.parent))
    ap.add_argument(
        "--scope",
        default="src-tauri",
        choices=["src-tauri", "all"],
        help="src-tauri = `§0a③` 那把 grep 的作用域（src-tauri/src/**.rs）；all = 整棵跟踪树的 .rs",
    )
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    tree = pathlib.Path(args.tree).resolve()
    head = subprocess.run(
        ["git", "-C", str(tree), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()

    files = tracked_rs_files(tree, args.scope)
    rows = []
    diffs = []
    for rel in files:
        src = (tree / rel).read_text("utf-8", "surrogateescape")
        if "russh" not in src:
            continue
        states = classify_bytes(src)
        row = {
            "file": str(rel),
            "russh": {s: 0 for s in ALL_STATES},
            "russh_sftp": {s: 0 for s in ALL_STATES},
            "russh_code_lines": [],
            "russh_sftp_code_lines": [],
        }
        for word in ("russh", "russh_sftp"):
            for idx in word_hits(src, word):
                st = states[idx]
                row[word][st] += 1
                if st == CODE:
                    ln = line_of(src, idx)
                    key = f"{word}_code_lines"
                    if ln not in row[key]:
                        row[key].append(ln)
        c_total, c_code, c_lines = coarse_ruler(src)
        row["coarse_total_lines"] = c_total
        row["coarse_code_lines"] = c_code
        # 差额：粗尺子判成「代码行」而本尺子在该行上一处 `russh`(严格词) 的 code 命中都没有
        strict = set(row["russh_code_lines"])
        for ln in c_lines:
            if ln not in strict:
                text = src.splitlines()[ln - 1]
                diffs.append(
                    {
                        "file": str(rel),
                        "line": ln,
                        "text": text.strip()[:150],
                        "why": _why(text, row, ln),
                    }
                )
        rows.append(row)

    rows.sort(key=lambda r: (-r["russh"][CODE], r["file"]))

    tot = {s: 0 for s in ALL_STATES}
    tot_sftp = {s: 0 for s in ALL_STATES}
    coarse_code_sum = 0
    strict_code_line_sum = 0
    for r in rows:
        for s in ALL_STATES:
            tot[s] += r["russh"][s]
            tot_sftp[s] += r["russh_sftp"][s]
        coarse_code_sum += r["coarse_code_lines"]
        strict_code_line_sum += len(r["russh_code_lines"])

    result = {
        "tree": str(tree),
        "head": head,
        "scope": args.scope,
        "population_rs_files_tracked": len(files),
        "files_with_any_russh_substring": len(rows),
        "rows": rows,
        "totals_russh_by_state": tot,
        "totals_russh_sftp_by_state": tot_sftp,
        "coarse_code_line_sum": coarse_code_sum,
        "strict_russh_code_line_sum": strict_code_line_sum,
        "coarse_minus_strict_rows": diffs,
    }
    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0

    w = 92
    print("=" * w)
    print(f"K-P6 · russh 逐处分类尺子   树={tree}")
    print(f"                            HEAD={head}   作用域={args.scope}")
    print("=" * w)
    print(
        f"【人群】`git ls-files -z` 里跟踪的 `.rs` 共 {len(files)} 份"
        f"（作用域 {args.scope}）；其中含子串 `russh` 的 {len(rows)} 份"
    )
    print("-" * w)
    print("【① 严格词 `russh` —— 逐处按词法状态分类】（单位=处）")
    hdr = f"{'文件':<42}{'code':>5}{'行注':>5}{'文档行':>7}{'块注':>5}{'文档块':>7}{'串':>4}{'代码行数':>9}"
    print(hdr)
    for r in rows:
        b = r["russh"]
        print(
            f"{r['file']:<42}{b[CODE]:>5}{b[LINE_COMMENT]:>5}{b[DOC_LINE_COMMENT]:>7}"
            f"{b[BLOCK_COMMENT]:>5}{b[DOC_BLOCK_COMMENT]:>7}{b[STRING]:>4}"
            f"{len(r['russh_code_lines']):>9}"
        )
    print(
        f"{'合计':<42}{tot[CODE]:>5}{tot[LINE_COMMENT]:>5}{tot[DOC_LINE_COMMENT]:>7}"
        f"{tot[BLOCK_COMMENT]:>5}{tot[DOC_BLOCK_COMMENT]:>7}{tot[STRING]:>4}"
        f"{strict_code_line_sum:>9}"
    )
    print("-" * w)
    print("【② 另一个 crate `russh_sftp` —— 单列，别混进上表】（单位=处）")
    for r in rows:
        b = r["russh_sftp"]
        if sum(b.values()) == 0:
            continue
        print(
            f"{r['file']:<42}{b[CODE]:>5}{b[LINE_COMMENT]:>5}{b[DOC_LINE_COMMENT]:>7}"
            f"{b[BLOCK_COMMENT]:>5}{b[DOC_BLOCK_COMMENT]:>7}{b[STRING]:>4}"
            f"{len(r['russh_sftp_code_lines']):>9}"
        )
    print(
        f"{'合计':<42}{tot_sftp[CODE]:>5}{tot_sftp[LINE_COMMENT]:>5}"
        f"{tot_sftp[DOC_LINE_COMMENT]:>7}{tot_sftp[BLOCK_COMMENT]:>5}"
        f"{tot_sftp[DOC_BLOCK_COMMENT]:>7}{tot_sftp[STRING]:>4}"
    )
    print("-" * w)
    print("【③ PM 那把粗尺子重打一遍（子串 + 只剔行首注释，单位=行）】")
    print(f"{'文件':<42}{'总命中行':>10}{'粗·代码行':>11}{'严格·代码行':>13}")
    for r in rows:
        print(
            f"{r['file']:<42}{r['coarse_total_lines']:>10}"
            f"{r['coarse_code_lines']:>11}{len(r['russh_code_lines']):>13}"
        )
    print(
        f"{'合计':<42}{sum(r['coarse_total_lines'] for r in rows):>10}"
        f"{coarse_code_sum:>11}{strict_code_line_sum:>13}"
    )
    print("-" * w)
    print(f"【④ 差额逐处 —— 粗尺子算作「代码行」而严格尺子不算的 {len(diffs)} 行】")
    for d in diffs:
        print(f"  {d['file']}:{d['line']}  〔{d['why']}〕")
        print(f"      {d['text']}")
    print("=" * w)
    return 0


def _why(text: str, row: dict, ln: int) -> str:
    if ln in row["russh_sftp_code_lines"]:
        return "命中的是 `russh_sftp`（另一个 crate），不是 `russh`"
    s = text.strip()
    if "//" in text or "/*" in text:
        return "`russh` 落在**行尾注释**里（PM 自己点名的那个粗口径）"
    if '"' in text:
        return "`russh` 落在**字符串字面量**里"
    if s.startswith("*") or s.startswith("//"):
        return "块注释续行"
    return "落在注释/字符串态上（详见词法表）"


if __name__ == "__main__":
    sys.exit(main())
