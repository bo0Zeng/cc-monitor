#!/usr/bin/env python3
"""K-R29 `D5①②` 的量具：**本件没动的那一面，逐块比字节**。

`D5①` 不动签字表 `SIGNED` 的任何一条判档；
`D5②` 那四条既有形状判据的断言一个字不变。

做法：把 `readonly_guard.rs` 在**两个提交**（默认 `基点` vs `工作树`）上各自取出
下面这几块的**逐字文本**，比 md5。散文式的「我没动它」不算读数。

⚠ 分母写明：本量具只比**下面 `BLOCKS` 点名的那几块**，不比整份文件
（整份文件当然变了 —— 本件就是往里加了三条判据）。
⚠ 取块靠**括号/中括号配平**，不是 Rust 词法器：块头锚点认不到就当场报错，不静默返回空串
（空串对空串会「相同」，那是本仓最经典的一种假绿）。

用法：
    python3 evidence/K-R29-frozen-face.py                 # 基点 b4e289f vs 工作树
    python3 evidence/K-R29-frozen-face.py <ref-a> <ref-b>  # 任意两个 ref（`WT` = 工作树）
"""

import hashlib
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REL = "remote-daemon-proto/src/readonly_guard.rs"

# `(名字, 块头锚点, 开括号, 闭括号, 期望)` —— 锚点之后第一个开括号起，配平到闭括号止。
#
# 🔴 最后一行是**非空对照**：`VERDICTS` 那张表本件**真的改了**（`D4①` 改的就是它）。
# 少了它，「五块全同」既可能是真没动、也可能是量具压根没在比 —— 这一行把两者分开。
BLOCKS = [
    ("SIGNED 签字表", "const SIGNED: &[(&str, &str, &str, &str)] = &", "[", "]", "same"),
    (
        "既有判据①every_dependency_this_manifest_declares_carries_a_signature",
        "fn every_dependency_this_manifest_declares_carries_a_signature()",
        "{",
        "}",
        "same",
    ),
    (
        "既有判据②the_signature_table_has_no_ghost_entries_and_every_verdict_is_a_registered_one",
        "fn the_signature_table_has_no_ghost_entries_and_every_verdict_is_a_registered_one()",
        "{",
        "}",
        "same",
    ),
    (
        "既有判据③the_only_signed_write_surface_still_rides_on_a_feature_this_manifest_leaves_off",
        "fn the_only_signed_write_surface_still_rides_on_a_feature_this_manifest_leaves_off()",
        "{",
        "}",
        "same",
    ),
    (
        "既有判据④an_unsigned_dependency_is_really_reported_so_that_the_zero_is_not_vacuous",
        "fn an_unsigned_dependency_is_really_reported_so_that_the_zero_is_not_vacuous()",
        "{",
        "}",
        "same",
    ),
    (
        "★非空对照：VERDICTS 判档表（`D4①` 改的就是它，必须**变了**）",
        "const VERDICTS: &[(&str, &str, &str)] = &",
        "[",
        "]",
        "changed",
    ),
]


def source_at(ref: str) -> str:
    if ref == "WT":
        return (ROOT / REL).read_text(encoding="utf-8")
    out = subprocess.run(
        ["git", "-C", str(ROOT), "show", f"{ref}:{REL}"],
        capture_output=True,
        check=True,
    )
    return out.stdout.decode("utf-8")


def block_of(src: str, anchor: str, op: str, cl: str) -> str:
    i = src.find(anchor)
    if i < 0:
        raise SystemExit(f"锚点找不到：{anchor!r} —— 量具与被测对象漂开了，**不许当成空块比过去**")
    if src.find(anchor, i + 1) >= 0:
        raise SystemExit(f"锚点命中不止一次：{anchor!r} —— 取块会歧义")
    # 🔴 从锚点**之后**找开括号，不是从锚点开头找 —— 初版写的是 `src.index(op, i)`，
    #    于是 `const SIGNED: &[(&str, …)] = &` 这个锚点里**自带的那个 `[`** 先被找到，
    #    取出来的「签字表」是 26 字节的类型标注 `[(&str, &str, &str, &str)]`，两边当然相同。
    #    那是一次**恒同**的假绿：被测对象根本没进分母。〔本轮自查逮到〕
    j = src.index(op, i + len(anchor))
    depth, k = 0, j
    while k < len(src):
        if src[k] == op:
            depth += 1
        elif src[k] == cl:
            depth -= 1
            if depth == 0:
                return src[j : k + 1]
        k += 1
    raise SystemExit(f"配平不上：{anchor!r}")


def main() -> int:
    a = sys.argv[1] if len(sys.argv) > 1 else "b4e289f"
    b = sys.argv[2] if len(sys.argv) > 2 else "WT"
    sa, sb = source_at(a), source_at(b)
    print(f"# 被测对象：{ROOT / REL}")
    print(f"# 左 = {a} · 右 = {b}（`WT` = 工作树）")
    print(f"# 分母：下面这 {len(BLOCKS)} 块，**不是**整份文件（整份必然变了）")
    print()
    bad = 0
    for name, anchor, op, cl, expect in BLOCKS:
        ba, bb = block_of(sa, anchor, op, cl), block_of(sb, anchor, op, cl)
        ha = hashlib.md5(ba.encode()).hexdigest()[:12]
        hb = hashlib.md5(bb.encode()).hexdigest()[:12]
        got = "same" if ha == hb else "changed"
        ok = got == expect
        if not ok:
            bad += 1
        mark = "同" if got == "same" else "变了"
        print(
            f"{'ok ' if ok else '★对不上'} {mark:4s} 期望={expect:8s} "
            f"{ha} {hb}  {len(ba):6d}B {len(bb):6d}B  {name}"
        )
    print()
    print(f"结论：{len(BLOCKS)} 块里 {len(BLOCKS) - bad} 块与期望相符、{bad} 块对不上")
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main())
