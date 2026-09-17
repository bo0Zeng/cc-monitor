#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R52 事4 的台子：`verify-committed-state.sh` 的 `daemon-win` 那一格，**三种结局各自不同**。

被测对象（写死，免得照住址复跑时量到别的树）：
    <本文件所在工作树>/scripts/verify-committed-state.sh   （相对本文件：../scripts/…）

# 为什么要有这个台子

那一格今天的病不是「跑了就绿」，是**「永远红、而红得没有信息」**：
`cargo check --target x86_64-pc-windows-msvc` 卡在 `ring` 的构建脚本上
（现打 09-11，沙箱里对 `d231e50` 跑：EXIT=101 · `failed to find tool "lib.exe"` ·
`Checking cc-monitor-remote` 命中 **0**）⇒ 它**根本没走到我们的代码**，
而那个红被写成「提交状态编不过」—— **一个与我们的代码无关的失败，冒充了一次关于我们代码的读数**。

⇒ 新口径：判绿的不是退出码，是**它有没有走到我们的代码**（needle = **包名**
`cc-monitor-remote`，不是目录名 `remote-daemon-proto`）。三种结局：

| 走到了？ | 退出码 | 该印的那一格 | 该印的结论 |
|---|---|---|---|
| 是 | 0    | `run` 打 `ok   daemon-win` | `== 提交状态编得过 ==` |
| 是 | ≠0   | `run` 打 `FAIL daemon-win` | `== 提交状态编不过 ==` |
| **否** | 任意 | **重判**成 `量不到 daemon-win`（并把 `run` 记的那一笔 `fail` 撤回） | `…Linux 上编得过 …本次没量…` |

# 这个台子怎么量

**不抄一份判定逻辑** —— 它从脚本里按锚点**抽出那两段原文**，塞进一个只有假输入的 bash 外壳里跑。
⇒ 脚本改了，台子量的就是改完那一份；没有第二处字面量会漂。

还切一刀（**没有开关，每趟都切**）：把最终结论那段里 `skipped` 那一支整个摘掉，
再看第三种结局与第一种还分不分得开。
🔴 那一刀的射程要分两档读：**比全输出**是粗刀（分档段自己多打一行，必然不同、买不到东西），
**只比最后那一行结论**才是该盖的最小面 —— 本脚本自己的头注 08-08 逐字写着
「读门禁的人（和 loop 里的我）读的就是最后那一行」。

用法：
    python3 K-R52-D4-winchk-verdict-harness.py          # 三格 + 一刀（退出码 0 = 三格都合）
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.abspath(os.path.join(HERE, "..", "scripts", "verify-committed-state.sh"))

# 抽取锚点。**逐字**取自被测脚本；抽不到就当场喊，不许静默退化成「空段照样跑」。
CLASSIFY_FROM = "  if ! grep -q 'Checking cc-monitor-remote' \"$WT/.verify-daemon-win.log\"; then"
CLASSIFY_TO = "  fi\nelse\n  skipped="
VERDICT_FROM = 'if [ "$fail" -ne 0 ]; then'
VERDICT_TO = "\nfi\n"


def slice_between(src: str, a: str, b: str, what: str) -> str:
    i = src.find(a)
    if i < 0:
        sys.exit(f"抽不到「{what}」的起点锚点 —— 脚本改过了，先更新锚点，别让台子空转：\n  {a!r}")
    j = src.find(b, i)
    if j < 0:
        sys.exit(f"抽不到「{what}」的收尾锚点 —— 同上：\n  {b!r}")
    return src[i : j + (len(b) if b == VERDICT_TO else 4)]


def harness(classify: str, verdict: str, log_text: str, rc: int) -> str:
    with tempfile.TemporaryDirectory(prefix="k-r52-winverdict-") as d:
        log = os.path.join(d, ".verify-daemon-win.log")
        with open(log, "w", encoding="utf-8") as f:
            f.write(log_text)
        sh = os.path.join(d, "run.sh")
        with open(sh, "w", encoding="utf-8") as f:
            f.write(
                "set -euo pipefail\n"
                # `run daemon-win` 那一跳由台子代打：它只做两件事 ——
                # ① 照 `run` 的规矩按退出码记一笔 `fail`；② 把日志落到脚本约定的那个住址。
                # 这样下面那段**重判**拿到的输入，与真路上一模一样。
                "fail=0\nskipped=\"\"\n"
                f'WT="{os.path.dirname(log)}"\nwin_fail_before="$fail"\n'
                f"[ {rc} -eq 0 ] || fail=1\n"
                # 脚本里那两段是缩进在 `if …; then` 里的，原样塞进来缩进无所谓。
                + classify
                + "\n"
                + verdict
                + "\n"
            )
        out = subprocess.run(
            ["bash", sh], capture_output=True, text=True, cwd=d
        )
        return (out.stdout + out.stderr).strip()


REACHED = "   Compiling foo v1.0\n    Checking cc-monitor-remote v0.0.0 (/x)\n"
BLOCKED = (
    "   Compiling ring v0.17.14\n"
    "  --- stderr\n"
    '  error occurred in cc-rs: failed to find tool "lib.exe": No such file or directory\n'
)

CASES = [
    ("走到了 + exit 0", REACHED, 0, "== 提交状态编得过 ==", "提交状态编得过"),
    ("走到了 + exit 101", REACHED, 101, "== 提交状态编不过 ==", "提交状态编不过"),
    ("**没走到** + exit 101", BLOCKED, 101, "量不到 daemon-win", "本次没量"),
]


def main() -> int:
    src = open(SCRIPT, encoding="utf-8").read()
    classify = slice_between(src, CLASSIFY_FROM, CLASSIFY_TO, "daemon-win 分档段")
    verdict = slice_between(src, VERDICT_FROM, VERDICT_TO, "最终结论段")
    print(f"== K-R52 事4 台子 ==\n被测脚本：{SCRIPT}")
    print(f"抽到的两段：分档 {len(classify)} B · 结论 {len(verdict)} B（**从脚本原文抽的，没有第二份副本**）\n")

    print("— 三格 —")
    outs = []
    ok = True
    for name, log, rc, want_line, want_verdict in CASES:
        got = harness(classify, verdict, log, rc)
        outs.append(got)
        hit_line = want_line in got
        hit_verdict = want_verdict in got
        ok &= hit_line and hit_verdict
        print(f"  [{'通过' if hit_line and hit_verdict else '不合'}] {name}")
        print(f"        期待那一格 `{want_line}` ⇒ {'有' if hit_line else '没有'}")
        print(f"        期待结论含 `{want_verdict}` ⇒ {'有' if hit_verdict else '没有'}")
        print("        实际输出：" + " ⏎ ".join(got.splitlines()))
    print()
    print("— 三格两两可分（这才是「三种结局各自不同」那句话的读数）—")
    distinct = len({o for o in outs}) == len(outs)
    print(f"  三份输出互不相同 ⇒ {'是' if distinct else '**否 —— 有两格塌成了一格**'}")
    ok &= distinct

    print()
    print("— 切一刀：摘掉结论段里 `skipped` 那一支（降级结论的唯一住址）—")
    anchor = 'elif [ -n "$skipped" ]; then'
    n = verdict.count(anchor)
    print(f"  锚点 `{anchor}` 在结论段里命中 **{n}** 次（切之前先断言它恰好 1 次）")
    if n != 1:
        print("  🔴 锚点不是恰好 1 次 ⇒ 这一刀切不下去，如实记成**没切**，不许报成「新红 0」")
        return 0 if ok else 1
    lines = verdict.splitlines()
    at = next(i for i, l in enumerate(lines) if anchor in l)
    cut = "\n".join(lines[:at] + lines[at + 2 :])
    print("  变异已落地：那一支连同它下面那一行 `echo` 一起摘掉")
    cut_outs = [harness(classify, cut, log, rc) for _, log, rc, _, _ in CASES]
    last = [o.splitlines()[-1] if o.splitlines() else "" for o in cut_outs]
    # 🔴 **两个射程分开读** —— 这一刀第一版只比了「全输出」，读出来是「仍分得开」，
    #    而那是**粗刀**：分档段自己会多打一行 `量不到 daemon-win`，全输出当然不同。
    #    而本脚本自己的头注 08-08 逐字写着：「读门禁的人（和 loop 里的我）**读的就是最后那一行**」
    #    ⇒ 该盖的最小面是**最后那一行结论**，不是全输出。
    coarse = cut_outs[0] != cut_outs[2]
    fine = last[0] != last[2]
    print(f"  ① 粗射程（比全输出）：{'仍分得开' if coarse else '分不开'}"
          "  ← 这一档**买不到东西**：分档段自己多打了一行，全输出必然不同")
    print(f"  ② 最小面（只比**最后那一行结论**）：{'仍分得开' if fine else '**分不开**'}")
    print("        第一种最后一行：" + last[0])
    print("        第三种最后一行：" + last[2])
    if fine:
        print("  ⚠ 最小面上仍分得开 ⇒ 那一支不承重，如实记，别写「有牙」")
    else:
        print("  ✅ 最小面上分不开 ⇒ **那一支承重**：摘掉它，一次「没有读数」当场"
              "被写成 `== 提交状态编得过 ==` —— 正是本脚本头注 08-08 那条教训的原形复发")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
