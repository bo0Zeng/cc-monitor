#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R21 落地拍：**那两句话在盘上还有几份副本，其中几份在本拍写区外。**

本拍改了两句话的事实：
  A. `launchId: null` 合并了**几种**原因（支四让「四种」变成「五种」）；
  B. `bare: true` 的定义（少了「environ 这一刻读得到」这个合取项）。

而 `src-tauri/src/doc_claim_registry.rs` 那张棘轮盯的是「daemon 读**几个键**」，
**不盯这两句** ⇒ 它们今天没有任何机检看着，全靠人记得改。
本尺子就是给 `§6` 那几个数一个**可复算的家**。

# 尺子怎么切的（分母写在前面）

- **人群**：`git ls-files` 的全部路径里，后缀 ∈ `.rs/.ts/.tsx/.md` 的那些
  （脚本自己打印这个分母）。⚠ 用 `git ls-files` 是为了与 `doc_claim_registry`
  那张表的口径一致 —— `target/` `node_modules/` 天然不在里面。
- **针**：下面 `NEEDLES` 里的正则，**逐条写明它认什么**。
- **本拍写区**：`WRITE_AREA` 那 4 条路径前缀（件文件与 `evidence/` 不产出线上事实，
  不进这张表）。

# 🔴 它的射程（**这不是「所有写法」的穷举**）

正则只认**今天盘上那几种写法**。同一件事写成别的句子（比如「不区分原因」而不写数字）
本尺子**看不见** —— 所以 `§6` 报的是「**我登记过这几处**」，不是「全仓只有这几处」。
那个分母没人给得出，写不出来就别写（本仓 `brief` 硬规则 3）。

# 复算

    python3 evidence/K-R21-claim-census.py            # 在工作树根下，宿主/沙箱都行（纯文本）
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

EXTS = {".rs", ".ts", ".tsx", ".md"}

WRITE_AREA = (
    "remote-daemon-proto/src/platform/proc.rs",
    "remote-daemon-proto/src/observe/accounts_query.rs",
    "remote-daemon-proto/src/control/identity_tag.rs",
    "doc/IPC-PROTOCOL.md",
)

# `(行针, 上下文针, 窗口半径)` —— **两段式**：行针先粗筛，再要求它前后
# `窗口` 行里出现上下文针。单靠行针会把别的主语的「三种原因 / 四种原因」一并扫进来
# （现打：`local_backend.rs` `tmux.rs` `ccm_cli_contract.rs` 各一条，主语都不是 `launchId`）
# —— 那正是本仓最高频那条病（**量具的作用域对不上事实**）在尺子这一侧的形状。
NEEDLES = {
    # A · 「launchId 的 null 合并了几种原因」这句话的每一份副本。
    #     行针认两种写法：写了计数词的（四种/五种…原因），与只列举不写数的（「不区分原因」）。
    "A · launchId:null 的原因有几种": (
        re.compile(r"(?:[一二两三四五六七八九十]\s*种原因)|(?:不区分原因)"),
        re.compile(r"launchId|LAUNCH_ID|launch_id|身份\s*token|不作数"),
        4,
    ),
    # B · `bare` 的**定义**。行针只认那句定义的骨架（「没设 CLAUDE_CONFIG_DIR」），
    #     再要求窗口里出现 `bare` —— 断言里的 `bare: true` 字面因此天然落选。
    "B · bare 的定义": (
        re.compile(r"没设\s*`?\**`?CLAUDE_CONFIG_DIR"),
        re.compile(r"\bbare\b"),
        8,
    ),
}


def main() -> int:
    root = Path(".").resolve()
    out = subprocess.run(
        ["git", "ls-files"], cwd=str(root), capture_output=True, text=True
    )
    if out.returncode != 0:
        print("❌ 跑不动 git ls-files —— 人群口径就是它", file=sys.stderr)
        return 2
    files = [f for f in out.stdout.splitlines() if Path(f).suffix in EXTS]
    if len(files) < 200:
        print(f"❌ 人群只有 {len(files)} 个文件 —— 口径坏了，下面整张表会零命中地绿", file=sys.stderr)
        return 2
    head = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=str(root), capture_output=True, text=True
    ).stdout.strip()
    print(f"量于提交：{head}")
    print(f"分母：git ls-files 里后缀 ∈ {sorted(EXTS)} 的文件 **{len(files)}** 个\n")

    for label, (rx, ctx, win) in NEEDLES.items():
        inside, outside = [], []
        for rel in files:
            try:
                text = (root / rel).read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            lines = text.splitlines()
            for i, line in enumerate(lines, 1):
                if not rx.search(line):
                    continue
                window = "\n".join(lines[max(0, i - 1 - win) : i + win])
                if not ctx.search(window):
                    continue
                (inside if rel.startswith(WRITE_AREA) else outside).append(
                    (rel, i, line.strip()[:120])
                )
        print(f"── {label} ─────────────────────────────────")
        print(f"   命中 {len(inside) + len(outside)} 处 = 写区内 {len(inside)} · **写区外 {len(outside)}**")
        for tag, rows in (("写区内", inside), ("写区外", outside)):
            for rel, i, s in rows:
                print(f"     [{tag}] {rel}:{i}  {s}")
        print()

    print("⚠ 射程：正则只认今天盘上那几种写法 —— 这是「我登记过这几处」，不是全仓穷举。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
