#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R5 · 实现拍（C）的量具 ②：按切法 `R7` **重数**这一片面上的信号，并当锚点尺子用。

住址（唯一）：`<工作树>/evidence/K-R5-C-r7-census.py`
被测对象：`<工作树>/src-tauri/src/capability_registry.rs`（工作树默认取本文件的 `../`）。
          09-02 这一拍是 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r5`，分支 `track/k-r5`。

# 切法（逐字照抄 `K-G2` `§19.0` 的 `R7`，**我没换尺子**）

> **一支 = 一个「能用一刀改成恒定答案、而语法与类型契约都不变」的最小语法位置。**

逐类：① `if`/`else if`/`if let` 的条件（**顶层每个 `||` / `&&` 操作数各算一支**）·
② 非通配 `match` 臂（**或模式的每个候选各算一支**）· ③ 闭包体里的一个析取/合取项 ·
④ early-return / `let … else` 守卫 · ⑤ 一个 `const` 字面量表。

**排除**（逐条）：循环边界（`while i < chars.len()` · `for … in …`）· 通配臂与 `else` 兜底 ·
纯计算（`out.push` · `i += 1` · `root.join` · `format!` 里的措辞）· 判据自己的 `assert!` 与
**那几组常驻探针**（`K-R5` 之前 9 组，之后 21 组）与 `stripped_ok` 那条抽取器自检
（**看守不算被看守**）· 同一条判据的另两块人群 `BEFORE` 与 `LIFECYCLE`。

**面**（换面就换数）：四张常量表 + 五个原语（`strip_toml_comments` · `decode_toml_escapes` ·
`word_in` · `backslash_in_key_position` · `keys_in`）+ 主判据体那个 `filter_map` 闭包
（`K-R5` 09-02 把它抽成了具名函数 `offenders_under`，**闭包体一个字节没动** ⇒ **面没换、数没变**）。

# 它是干什么用的

1. `--census`：把 41 支逐支印出来（分母 = 本表；`R7` 的规则与排除项就在上面）。
2. `--anchors`：**每一刀切之前先跑这一把** —— 逐支断言它的锚点在盘上恰好命中 N 次
   （`brief` 7）。锚点是**整行原文**，不是行号：🔴 行号是快照，注释一加就馊。
3. `--landed <支>`：断言那一刀**已落地**（原文没了、变异体在盘上）。
4. `--verify-clean`：核「盘上此刻没有残留变异」（41 条锚点全部命中 + 文件 md5）。
5. `--fn-md5 <rev>`：改动面 —— 逐函数 md5（基线 vs 工作树）。

⚠ **它判不了什么**：它**不判**某一支「今天有没有牙」—— 那要真去跑门（`K-R5-C-r7-door.py`）。
本量具只管**分母**（哪 41 支）与**锚点**（那一刀切得准不准）。
"""
from __future__ import annotations

import argparse
import hashlib
import pathlib
import re
import subprocess
import sys

# `B1`/`B2` 共用的那条锚点：Rust 源里 `"""` 是**双引号串里的三个转义引号**，
# `'''` 是**双引号串里的三个单引号** —— 直接写进 Python 字面量太容易写歪，拼出来。
_B12_OLD = ('            if line.contains("' + '\\"' * 3 + '") || line.contains("'
            + "'''" + '") {')

# ─────────────────────────────────────────────────────────────────────────────
# 41 支：(id, 面, 类, 锚点原文, 变异后的原文, 一句话说这一刀切的是什么)
# 🔴 锚点必须在文件里**恰好命中 1 次**（`--anchors` 会断言）；命中 0 或 ≥2 都要停下来改锚点。
# ─────────────────────────────────────────────────────────────────────────────
SIGNALS: list[tuple[str, str, str, str, str, str]] = [
    # ── A · 四张常量表（类 ⑤）────────────────────────────────────────────────
    ("A1", "常量表", "⑤",
     '    const CARGO_EXEC_KEYS: &[&str] = &["runner", "linker", "rustflags"];',
     '    const CARGO_EXEC_KEYS: &[&str] = &[];',
     "执行面那三个键的人群整张清空"),
    ("A2", "常量表", "⑤",
     '    const CARGO_BLINDING_KEYS: &[&str] = &["include"];',
     '    const CARGO_BLINDING_KEYS: &[&str] = &[];',
     "致盲键 `include` 的人群整张清空"),
    ("A3", "常量表", "⑤",
     '    const CARGO_CFG_DIRS: &[&str] = &[".", "src-tauri", "remote-daemon-proto"];',
     '    const CARGO_CFG_DIRS: &[&str] = &[];',
     "3 个发起面整张清空"),
    ("A4", "常量表", "⑤",
     '    const CARGO_CFG_NAMES: &[&str] = &["config.toml", "config"];',
     '    const CARGO_CFG_NAMES: &[&str] = &[];',
     "2 种文件名整张清空"),

    # ── B · strip_toml_comments（11 支）──────────────────────────────────────
    ("B1", "strip_toml_comments", "①",
     _B12_OLD, '            if false || line.contains("' + "'''" + '") {',
     "`\"\"\"` 那个析取项恒假"),
    ("B2", "strip_toml_comments", "①",
     _B12_OLD, '            if line.contains("' + '\\"' * 3 + '") || false {',
     "`\'\'\'` 那个析取项恒假"),
    ("B3", "strip_toml_comments", "①",
     '                if quote.is_some() {\n                    if escaped {',
     '                if false {\n                    if escaped {',
     "循环**内**「此刻在引号里」那一支恒假"),
    ("B4", "strip_toml_comments", "①",
     '                    if escaped {\n                        escaped = false;',
     '                    if false {\n                        escaped = false;',
     "「上一个字符是反斜杠」那一支恒假"),
    ("B5", "strip_toml_comments", "①",
     "                    } else if c == '\\\\' && quote == Some('\"') {",
     "                    } else if false && quote == Some('\"') {",
     "`c == '\\\\'` 那个合取项恒假"),
    ("B6", "strip_toml_comments", "①",
     "                    } else if c == '\\\\' && quote == Some('\"') {",
     "                    } else if c == '\\\\' && true {",
     "`quote == Some('\"')` 那个合取项恒真"),
    ("B7", "strip_toml_comments", "①",
     "                    } else if quote == Some(c) {",
     "                    } else if false {",
     "「引号在这里闭合」那一支恒假"),
    ("B8", "strip_toml_comments", "①",
     "                } else if c == '\"' || c == '\\'' {",
     "                } else if false || c == '\\'' {",
     "`c == '\"'`（进基本串）那个析取项恒假"),
    ("B9", "strip_toml_comments", "①",
     "                } else if c == '\"' || c == '\\'' {",
     "                } else if c == '\"' || false {",
     "`c == '\\''`（进字面串）那个析取项恒假"),
    ("B10", "strip_toml_comments", "①",
     "                } else if c == '#' {",
     "                } else if false {",
     "「看见 `#` 就切」那一支恒假"),
    ("B11", "strip_toml_comments", "①",
     '            if quote.is_some() {\n                unmodeled.push(format!("第 {no} 行的引号到行尾还没闭合"));',
     '            if false {\n                unmodeled.push(format!("第 {no} 行的引号到行尾还没闭合"));',
     "循环**后**「行尾引号还没闭合」那一支恒假"),

    # ── C · decode_toml_escapes（6 支）───────────────────────────────────────
    ("C1", "decode_toml_escapes", "②",
     "                Some(['\\\\', 'u']) => 4,",
     "                Some(['\\\\', 'u']) => 0,",
     "`\\u`（4 位）那个非通配臂答通配臂的值"),
    ("C2", "decode_toml_escapes", "②",
     "                Some(['\\\\', 'U']) => 8,",
     "                Some(['\\\\', 'U']) => 0,",
     "`\\U`（8 位）那个非通配臂答通配臂的值"),
    ("C3", "decode_toml_escapes", "②",
     "                Some(['\\\\', 'x']) => 2,",
     "                Some(['\\\\', 'x']) => 0,",
     "`\\x`（2 位）那个非通配臂答通配臂的值"),
    ("C4", "decode_toml_escapes", "①",
     "            if width > 0 {",
     "            if false {",
     "「这是一处转义」那道分岔恒假"),
    ("C5", "decode_toml_escapes", "①",
     "                if let Some(hex) = chars.get(i + 2..i + 2 + width) {",
     "                if let Some(hex) = chars.get(i + 2..i + 2 + width).filter(|_| false) {",
     "「取得到那几位十六进制」那道 `if let` 恒 `None`"),
    ("C6", "decode_toml_escapes", "①",
     "                    if let Some(ch) = decoded {",
     "                    if let Some(ch) = decoded.filter(|_| false) {",
     "「那几位真解得出一个字符」那道 `if let` 恒 `None`"),

    # ── D · word_in（5 支）──────────────────────────────────────────────────
    ("D1", "word_in", "①",
     "            c.is_alphanumeric() || c == '_' || c == '-'",
     "            false || c == '_' || c == '-'",
     "词字符判定里 `is_alphanumeric()` 那个析取项恒假"),
    ("D2", "word_in", "①",
     "            c.is_alphanumeric() || c == '_' || c == '-'",
     "            c.is_alphanumeric() || false || c == '-'",
     "词字符判定里 `c == '_'` 那个析取项恒假"),
    ("D3", "word_in", "①",
     "            c.is_alphanumeric() || c == '_' || c == '-'",
     "            c.is_alphanumeric() || c == '_' || false",
     "词字符判定里 `c == '-'` 那个析取项恒假"),
    ("D4", "word_in", "③",
     "            !before.is_some_and(is_word) && !after.is_some_and(is_word)",
     "            true && !after.is_some_and(is_word)",
     "整词边界的**前**半恒真"),
    ("D5", "word_in", "③",
     "            !before.is_some_and(is_word) && !after.is_some_and(is_word)",
     "            !before.is_some_and(is_word) && true",
     "整词边界的**后**半恒真"),

    # ── E · backslash_in_key_position（6 支）─────────────────────────────────
    ("E1", "backslash_in_key_position", "②",
     "                    b'{' | b',' => start = i + 1,",
     "                    b',' => start = i + 1,",
     "或模式候选 `b'{'` 删掉"),
    ("E2", "backslash_in_key_position", "②",
     "                    b'{' | b',' => start = i + 1,",
     "                    b'{' => start = i + 1,",
     "或模式候选 `b','` 删掉"),
    ("E3", "backslash_in_key_position", "②",
     "                    b'=' => {",
     "                    b'=' if false => {",
     "`b'='` 那一臂加 `if false` 守卫（落到通配臂）"),
    ("E4", "backslash_in_key_position", "①",
     "                        if line[start..i].contains('\\\\') {",
     "                        if false {",
     "「键位这一段里有反斜杠」那一支恒假"),
    ("E5", "backslash_in_key_position", "③",
     "            !line.contains('=') && line.contains('\\\\')",
     "            false && line.contains('\\\\')",
     "兜底第二支的「整行没有 `=`」那个合取项恒假"),
    ("E6", "backslash_in_key_position", "③",
     "            !line.contains('=') && line.contains('\\\\')",
     "            !line.contains('=') && false",
     "兜底第二支的「这一行有反斜杠」那个合取项恒假"),

    # ── F · 主判据体那个 filter_map 闭包（7 支）──────────────────────────────
    ("F1", "filter_map 闭包（装配层）", "④",
     "                if !p.is_file() {",
     "                if false {",
     "「这一格不是文件就跳过」那道守卫恒假（⇒ 6 格全走下去）"),
    ("F2", "filter_map 闭包（装配层）", "④",
     '                    return Some(format!("{rel}（存在但读不出文本，本条看不了它的内容）"));',
     "                    return None;",
     "`let Ok(raw) … else` 那道守卫换成另一张脸（读不出就当没事）"),
    ("F3", "filter_map 闭包（装配层）", "①",
     "                if !unmodeled.is_empty() {",
     "                if false {",
     "预处理 fail-closed 那一支恒假"),
    ("F4", "filter_map 闭包（装配层）", "①",
     "                if !exec.is_empty() {",
     "                if false {",
     "「设了执行面的键」那一支恒假"),
    ("F5", "filter_map 闭包（装配层）", "①",
     "                if !blind.is_empty() {",
     "                if false {",
     "「设了致盲键 include」那一支恒假"),
    ("F6", "filter_map 闭包（装配层）", "①",
     "                if backslash_in_key_position(&text) {",
     "                if false {",
     "「键位置有反斜杠」那一支恒假"),
    ("F7", "filter_map 闭包（装配层）", "①",
     "                if why.is_empty() {",
     "                if true {",
     "「一条理由都没有 ⇒ 不算 offender」那道分岔恒真（⇒ 永不上报）"),

    # ── G · keys_in（2 支）──────────────────────────────────────────────────
    ("G1", "keys_in", "③",
     "            .filter(|key| word_in(text, key) || word_in(&decoded, key))",
     "            .filter(|key| false || word_in(&decoded, key))",
     "扫**原文**那一遍恒假"),
    ("G2", "keys_in", "③",
     "            .filter(|key| word_in(text, key) || word_in(&decoded, key))",
     "            .filter(|key| word_in(text, key) || false)",
     "扫**解码后**那一遍恒假"),
]

FACE_ORDER = [
    "常量表", "strip_toml_comments", "decode_toml_escapes", "word_in",
    "backslash_in_key_position", "filter_map 闭包（装配层）", "keys_in",
]


def target(wt: pathlib.Path) -> pathlib.Path:
    return wt / "src-tauri" / "src" / "capability_registry.rs"


# ── 改动面：逐函数 md5（基线 vs 工作树）─────────────────────────────────────
# ⚠ 口径写明白，别读大：`brief` 第四部分要的「`ast` 逐函数 md5」那条是**给 Python 写的**
#   （`ast` 模块）。Rust 没有现成的 `ast`，这里用**按缩进切块**的粗抽取器：
#   从 `    fn <名>` 那一行起、往上吞掉紧挨的 `///`/`#[…]`，到下一个同缩进的 `}` 为止。
#   ⇒ **改文档注释也会让 md5 变**，这是刻意的（本件就改了好几处头注，那要看得见）。
# ⚠ 切法与 `evidence/K-R13-C-fn-md5.py` 同形，但那一份**硬写着另一棵树和另一个文件**
#   ⇒ 我没有去改它（`brief` `5k`：同住址下换了被测对象 = 一次静默的假读数），另起一份。
FN_RE = re.compile(r"^(?P<ind>[ ]*)(?:pub )?(?:async )?fn (?P<name>[A-Za-z0-9_]+)")
FN_FLOOR = 8  # 抽取器自检地板：切不到这么多就说明切法坏了，按 CRASH 报


def split_fns(src: str) -> dict[str, str]:
    lines = src.splitlines()
    out: dict[str, str] = {}
    i = 0
    while i < len(lines):
        m = FN_RE.match(lines[i])
        if not m:
            i += 1
            continue
        ind, name = m.group("ind"), m.group("name")
        start = i
        while start > 0 and lines[start - 1].strip().startswith(("///", "#[", "//")):
            start -= 1
        close = ind + "}"
        end = len(lines) - 1
        for j in range(i + 1, len(lines)):
            if lines[j].rstrip() == close:
                end = j
                break
        key, n = name, 2
        while key in out:
            key, n = f"{name}#{n}", n + 1
        out[key] = hashlib.md5("\n".join(lines[start:end + 1]).encode()).hexdigest()[:12]
        i = end + 1
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--wt", default=str(pathlib.Path(__file__).resolve().parent.parent))
    ap.add_argument("--census", action="store_true")
    ap.add_argument("--anchors", action="store_true")
    ap.add_argument("--verify-clean", action="store_true")
    ap.add_argument("--one", default="", help="只核这一支的锚点（如 F2）")
    ap.add_argument("--landed", default="", help="断言这一支的变异**已落地**（原文没了、变异体在盘上）")
    ap.add_argument("--fn-md5", default="", metavar="REV", help="改动面：逐函数 md5，基线 = 这个 rev")
    a = ap.parse_args()

    wt = pathlib.Path(a.wt).resolve()
    f = target(wt)
    src = f.read_text(encoding="utf-8")

    if a.census:
        print(f"# `R7` 切法下的分母（被测对象 {f}）")
        print(f"# 文件 md5 {hashlib.md5(src.encode()).hexdigest()} · 行数 {len(src.splitlines())}")
        by_face: dict[str, int] = {}
        for sid, face, cls, *_ in SIGNALS:
            by_face[face] = by_face.get(face, 0) + 1
        print("\n| 面 | R7 支数 |")
        print("|---|---|")
        for face in FACE_ORDER:
            print(f"| {face} | {by_face[face]} |")
        print(f"| **合计** | **{len(SIGNALS)}** |")
        print("\n| 支 | 面 | 类 | 这一刀切的是什么 |")
        print("|---|---|---|---|")
        for sid, face, cls, _old, _new, what in SIGNALS:
            print(f"| `{sid}` | {face} | {cls} | {what} |")
        return 0

    if a.fn_md5:
        old_src = subprocess.run(
            ["git", "-C", str(wt), "show",
             f"{a.fn_md5}:src-tauri/src/capability_registry.rs"],
            capture_output=True, text=True, check=True,
        ).stdout
        old_fns, new_fns = split_fns(old_src), split_fns(src)
        for label, d in (("基线", old_fns), ("工作树", new_fns)):
            if len(d) < FN_FLOOR:
                print(f"CRASH：{label}只切出 {len(d)} 个函数（地板 {FN_FLOOR}）"
                      f"—— 抽取器坏了，下面的对比此刻不算数")
                return 2
        print(f"基线 = {a.fn_md5}（{len(old_fns)} 个函数） · 工作树（{len(new_fns)} 个函数）")
        print(f"{'函数':<48} {'基线':<14} {'工作树':<14} 判")
        for name in sorted(set(old_fns) | set(new_fns)):
            o, n = old_fns.get(name, "—"), new_fns.get(name, "—")
            verdict = "新增" if o == "—" else ("删了" if n == "—" else ("变了" if o != n else "没动"))
            print(f"{name:<48} {o:<14} {n:<14} {verdict}")
        return 0

    if a.landed:
        row = [s for s in SIGNALS if s[0] == a.landed]
        if not row:
            print(f"🔴 没有这一支：{a.landed}")
            return 2
        sid, face, cls, old, new, what = row[0]
        n_new, n_old = src.count(new), src.count(old)
        ok = n_new >= 1 and n_old == 0
        print(f"{'变异已落地 ✓' if ok else '🔴 变异没落地'} {sid}（{face}）：{what}")
        print(f"  盘上现文命中：变异体 {n_new} 次 · 原文 {n_old} 次")
        print(f"  变异体逐字：{new!r}")
        print(f"  文件 md5 {hashlib.md5(src.encode()).hexdigest()}")
        return 0 if ok else 1

    if a.anchors or a.verify_clean or a.one:
        bad = 0
        rows = [s for s in SIGNALS if not a.one or s[0] == a.one]
        seen_anchor: dict[str, str] = {}
        for sid, face, cls, old, new, what in rows:
            hits = src.count(old)
            mark = "ok " if hits == 1 else "🔴 "
            if hits != 1:
                bad += 1
            # 同一条锚点被两支共用（`||` 两侧）是正常的，印出来免得被读成重复
            share = seen_anchor.get(old)
            seen_anchor.setdefault(old, sid)
            note = f"（与 `{share}` 共用同一条锚点）" if share else ""
            print(f"{mark}{sid:<3} 锚点命中 {hits} 次{note} · {what}")
            # 🔴 只有「锚点不见了」才算残留 —— 光看变异体在不在会假阳：
            #    `F2` 的变异体逐字是 `return None;`，而 `F1` 那道守卫的函数体本来就是它。
            #    〔09-02 自查现打逮到一次假阳〕
            if a.verify_clean and hits == 0 and src.count(new) and new != old:
                print(f"🔴 {sid} 的**变异后原文**也在盘上 —— 可能有残留变异没复原：{new!r}")
                bad += 1
        print(f"\n文件 md5 {hashlib.md5(src.encode()).hexdigest()}")
        print(f"锚点异常 {bad} 条 / 共 {len(rows)} 条")
        return 1 if bad else 0

    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
