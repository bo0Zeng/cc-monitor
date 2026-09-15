#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R80 `KR80D2` 的第二把刀：那 3 份文件**只改了格式，没改内容**。

## 为什么要这一把，而不是只看 `daemon` 那格的读数

`KR80D2` 的死值验有两条：
  · 第一条 —— `remote-daemon-proto` 下 `cargo fmt --check` 从 **6 处 ⇒ 0 处**；
  · 第二条 —— **`daemon` 那一格读数必须逐格等于基线 694，动了就是越界的证据**。
第二条是**行为面**的证据（判据一条不多一条不少地照跑）。但它买不到一件事：
一个**不改变任何判据结果**的内容改动（改一句注释、挪一个参数）它看不见。
那 3 份里 `argv.rs` 是 `ccm` argv 的**唯一解析口**（`KR48D2`）、
`protocol_doc_guard.rs` 是**判据本身** —— 值得再要一把**文本面**的刀。

## 它怎么判

把 `git show <基点>:<文件>` 与盘上那份各自**归一**，然后逐字节比：
  ① 先把**字符串字面量与字符字面量原样抠出来**，两侧**逐条逐字节**对比
     （抠出来是为了不让第 ② 步「删空白」把串**里面**的改动抹掉）；
  ② 抠掉之后：删掉**全部空白**（空格 / tab / 换行），
     再反复删掉**紧挨着 `)` `]` `}` 的那个逗号**（rustfmt 换行时会补尾随逗号 ——
     那是**排版**，不是内容；本轮 6 处里有 2 处补了）。
  ⇒ 两侧归一后相等 ⇒ **除了空白与尾随逗号，一个字节都没动。**

## ⚠ 非空对照（`K-R79` 那一课：**自检别写成地板**）

`C0` 先断言两侧的**原文确实不一样** —— 要是文件根本没被改过，
上面那条「归一后相等」永远满足，等于没在自检。

## ⚠ 它买不到什么

  · 它**不判改得对不对**（rustfmt 说了算），只判「改的只有空白与尾随逗号」。
  · 字面量的抠法是正则，不是 Rust 词法器：`'a'` 这类**字符字面量**与生命周期 `'a`
    在正则眼里有歧义。⚠ 但**两侧用的是同一个归一器** ⇒ 认错也认得一样，
    它只可能让本尺子**漏报**，不会让它**误报**。本轮那 3 份现打 0 处字符字面量。

## 跑法

    python3 evidence/K-R80-fmt-content-unchanged.py [<基点 rev>]

基点默认 `HEAD`（本件在 `be271a0` 上开工，工作树尚未提交时 `HEAD` 就是它）。
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE = sys.argv[1] if len(sys.argv) > 1 else "HEAD"

FILES = [
    "remote-daemon-proto/src/agents/mod.rs",
    "remote-daemon-proto/src/control/ccm/argv.rs",
    "remote-daemon-proto/src/protocol_doc_guard.rs",
]

# 原始串 `r"…"` / `r#"…"#` 要排在普通串前面，否则 `r` 后面那个 `"` 会被当成普通串的开头。
LITERAL = re.compile(
    r'r#+"(?:.|\n)*?"#+'      # r#"…"#（任意个 #）
    r'|r"(?:[^"\\]|\\.)*"'    # r"…"
    r'|"(?:[^"\\]|\\.|\n)*"'  # "…"（含跨行的 \ 续行串）
    r"|'(?:\\.|[^\\'\n])'"    # 'x' / '\n'
)
WS = re.compile(r"\s+")
TRAILING_COMMA = re.compile(r",([)\]}])")


def literals(text):
    return [m.group(0) for m in LITERAL.finditer(text)]


def normalize(text):
    stripped = LITERAL.sub("\x00LIT\x00", text)
    stripped = WS.sub("", stripped)
    prev = None
    while prev != stripped:                       # `,))` 这种要削到不动为止
        prev = stripped
        stripped = TRAILING_COMMA.sub(r"\1", stripped)
    return stripped


def git_show(rev, path):
    out = subprocess.run(["git", "-C", str(ROOT), "show", f"{rev}:{path}"],
                         capture_output=True, text=True, encoding="utf-8")
    if out.returncode != 0:
        raise SystemExit(f"读不到 {rev}:{path} —— {out.stderr.strip()}")
    return out.stdout


def main():
    fails = []
    print(f"# `K-R80` `KR80D2` 文本面：只改格式没改内容 —— 基点 `{BASE}`")
    print()
    print("| 文件 | 原文字节 | 今天字节 | 字面量条数（前/后） | 尾随逗号净增 | 归一后 |")
    print("|---|---|---|---|---|---|")
    for f in FILES:
        old = git_show(BASE, f)
        new = (ROOT / f).read_text(encoding="utf-8")

        if old == new:                                     # C0 非空对照
            fails.append(f"C0 `{f}` 与基点**逐字节相同** —— 那这一格是地板，不是判据")

        lo, ln = literals(old), literals(new)
        if lo != ln:
            diff = [(a, b) for a, b in zip(lo, ln) if a != b]
            fails.append(f"C1 `{f}` 的字面量变了：{len(lo)} 条 → {len(ln)} 条，"
                         f"逐条对比头一处不同：{diff[:1]}")

        no, nn = normalize(old), normalize(new)
        same = no == nn
        if not same:
            i = next((k for k in range(min(len(no), len(nn))) if no[k] != nn[k]), min(len(no), len(nn)))
            fails.append(f"C2 `{f}` 归一之后**仍然不同** —— 头一处分歧在归一串第 {i} 字节："
                         f"原 {no[max(0, i-40):i+40]!r} / 今 {nn[max(0, i-40):i+40]!r}")

        commas = len(TRAILING_COMMA.findall(WS.sub("", LITERAL.sub("\x00LIT\x00", new)))) - \
            len(TRAILING_COMMA.findall(WS.sub("", LITERAL.sub("\x00LIT\x00", old))))
        print(f"| `{f}` | {len(old.encode())} | {len(new.encode())} | {len(lo)} / {len(ln)} | "
              f"{commas:+d} | {'**逐字节相同**' if same else '🔴 不同'} |")

    print()
    if fails:
        print(f"KR80D2(文本面): FAIL={len(fails)}")
        for x in fails:
            print(f"  ✗ {x}")
        return 1
    print("KR80D2(文本面): OK —— 3/3 份「删空白 + 削尾随逗号」之后与基点逐字节相同，"
          "且字面量逐条未变；`C0` 非空对照通过（3 份原文都确实变了）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
