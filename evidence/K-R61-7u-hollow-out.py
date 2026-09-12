#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""M12（`7u`）：**把本件的实现整个退掉，判据一个字不动。**

🔴 纪律 ⑱：**不许** `git checkout <基线> -- <生产文件>` —— 本仓 Rust 判据与实现同住一份
文件，整份退回会把两条新判据一起退掉，读数会变成「一条都不红」，结论完全反过来。
⇒ 这里**逐处掏空**：五处的措辞退回旧版（旧住址 + 旧前提 + 不点 token）、
`CAPABILITIES` 里那个 token 删掉，而**判据体、判据名、住址表、自剪线全部原样留着**。

作用面刻意**只到自剪线之前**：自剪线之后是判据自己的字面量副本，动它就是改判据。
"""
import io
import sys

WT = "/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r61"
H = WT + "/src-tauri/src/history.rs"
M = WT + "/remote-daemon-proto/src/control/ccm/mod.rs"
CUT = "〔K-R61 判据组自剪线〕"

s = io.open(H, encoding="utf-8").read()
at = s.index(CUT)
head, tail = s[:at], s[at:]

# 五处的住址逐处退回旧的（各断言恰好一次）
site_pairs = [
    ("转发做到了、也声明了（`remote-daemon-proto/src/control/ccm/mod.rs`），\\",
     "放行会让装旧 ccm 的机器静默吃掉它（`shared/ccm:624`），\\"),
    ("/// 补进了 `remote-daemon-proto/src/control/ccm/mod.rs` 的 `CAPABILITIES`，",
     "/// 补进了 `shared/ccm:624` 的 `capabilities=`，"),
    ("        //   `base-url-across-tmux` 已进 `remote-daemon-proto/src/control/ccm/mod.rs`",
     "        //   那个 token 已进 `shared/ccm:624`"),
    ("    /// `remote-daemon-proto/src/control/ccm/mod.rs` 的 `CAPABILITIES` 里，",
     "    /// `shared/ccm:624` 的 `capabilities=` 里，"),
    ("             （第四样 —— `remote-daemon-proto/src/control/ccm/mod.rs` 的 `CAPABILITIES` 加\\n\\",
     "             （第四样 —— `shared/ccm:624` 的 `capabilities=` 加\\n\\"),
]
n = 0
for old, new in site_pairs:
    c = head.count(old)
    if c != 1:
        sys.exit(f"🔴 M12 锚点命中 {c} 次（该 1）：{old[:60]!r}")
    head = head.replace(old, new)
    n += 1

# 前提句与 token 在自剪线之前**全部**退掉（这就是「把实现掏空」的那一半）
n_premise = head.count("转发做到了、也声明了")
head = head.replace("转发做到了、也声明了", "放行会让装旧 ccm 的机器静默吃掉它")
n_token = head.count("base-url-across-tmux")
head = head.replace("base-url-across-tmux", "转发那个变量的 token")

io.open(H, "w", encoding="utf-8").write(head + tail)

# daemon：token 从 CAPABILITIES 里删掉
ms = io.open(M, encoding="utf-8").read()
old_caps = '    "account-via-daemon",\n    "base-url-across-tmux",\n];'
if ms.count(old_caps) != 1:
    sys.exit("🔴 M12 daemon 锚点不是一处")
io.open(M, "w", encoding="utf-8").write(ms.replace(old_caps, '    "account-via-daemon",\n];'))

print(f"[M12] **变异已落地**：五处住址各 1 次（{n} 处）· 前提句 {n_premise} 处 · "
      f"token 字面 {n_token} 处（均在自剪线之前）· daemon CAPABILITIES 删 1 个 token")
print("[M12] 判据体 / 判据名 / 住址表 / 自剪线：**一个字节没动**")
