#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R61 变异台：按名字施一刀 / 撤一刀。每刀切之前断言锚点**恰好命中一次**，并印「变异已落地」。

住址：本文件（scratchpad，`kr61-` 前缀，独属本轮）。
被测对象：/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r61
用法：kr61-mutate.py <刀名>        施刀（改盘上的文件）
      kr61-mutate.py --list        列出所有刀
"""
import io
import os
import sys

WT = "/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r61"

H = "src-tauri/src/history.rs"
M = "remote-daemon-proto/src/control/ccm/mod.rs"
PL = "remote-daemon-proto/src/control/ccm/plan.rs"

ADDR = "remote-daemon-proto/src/control/ccm/mod.rs"
OLD = "shared/ccm"

# 五处各自的**逐字锚点**（各含那个住址一次），用来做「单断」
SITE_ANCHORS = {
    "M1·处①常量本体": (
        H,
        "转发做到了、也声明了（`remote-daemon-proto/src/control/ccm/mod.rs`），\\",
        "转发做到了、也声明了（`shared/ccm:624`），\\",
    ),
    "M2·处②常量头注": (
        H,
        "/// 补进了 `remote-daemon-proto/src/control/ccm/mod.rs` 的 `CAPABILITIES`，",
        "/// 补进了 `shared/ccm:624` 的 `capabilities=`，",
    ),
    "M3·处③launch_local体内": (
        H,
        "        //   `base-url-across-tmux` 已进 `remote-daemon-proto/src/control/ccm/mod.rs`",
        "        //   `base-url-across-tmux` 已进 `shared/ccm:624`",
    ),
    "M4·处④互斥判据头注": (
        H,
        "    /// `remote-daemon-proto/src/control/ccm/mod.rs` 的 `CAPABILITIES` 里，",
        "    /// `shared/ccm:624` 的 `capabilities=` 里，",
    ),
    "M5·处⑤诊断文案": (
        H,
        "             （第四样 —— `remote-daemon-proto/src/control/ccm/mod.rs` 的 `CAPABILITIES` 加\\n\\",
        "             （第四样 —— `shared/ccm:624` 的 `capabilities=` 加\\n\\",
    ),
}

CUTS = dict(SITE_ANCHORS)

# M6：**只换地址不换前提**（KR61D1 逐字点名的失效方向）——
#      住址原样留着，把那句承重话换回旧措辞。
CUTS["M6·处①换地址不换前提"] = (
    H,
    "转发做到了、也声明了（`remote-daemon-proto/src/control/ccm/mod.rs`），\\",
    "放行会让装旧 ccm 的机器静默吃掉它（`remote-daemon-proto/src/control/ccm/mod.rs`），\\",
)

# M7：**只**给处① 多加一个盘上没有的住址（原住址一个字不动）⇒ 只该 KR61D2 红
CUTS["M7·处①多加一个不存在的住址"] = (
    H,
    "     差的只是这一行；`K-R61` 只重裁理由，不动行为\";",
    "     差的只是这一行（另见 `remote-daemon-proto/src/control/ccm/gone.rs`）；`K-R61` 只重裁理由，不动行为\";",
)

# M8：给处① 多加一个**存在但不相干**的住址 ⇒ KR61D2 **不主张**逮得到（该全绿）
CUTS["M8·处①多加一个存在但不相干的住址"] = (
    H,
    "     差的只是这一行；`K-R61` 只重裁理由，不动行为\";",
    "     差的只是这一行（另见 `src-tauri/src/lib.rs`）；`K-R61` 只重裁理由，不动行为\";",
)

# M10：daemon —— 把 token 从 CAPABILITIES 里删掉（**说了才算数**那一半）
CUTS["M10·daemon删掉token"] = (
    M,
    '    "account-via-daemon",\n    "base-url-across-tmux",\n];',
    '    "account-via-daemon",\n];',
)

# M11：daemon —— **掏空行为、形状留着**（纪律 ⑱：不许整份退回）：
#      那句 `export` 的拼接换成一个恒不拼的分支，`if let` / 变量 / 类型契约全留着。
CUTS["M11·daemon掏空plan.rs那句转发"] = (
    PL,
    '        if let Some(v) = env.anthropic_base_url.as_deref().filter(|v| !v.is_empty()) {\n'
    '            payload = format!("export ANTHROPIC_BASE_URL={}; {payload}", sq(v));\n'
    "        }",
    '        if let Some(v) = env.anthropic_base_url.as_deref().filter(|v| !v.is_empty()) {\n'
    "            let _ = sq(v);\n"
    "            payload = format!(\"{payload}\");\n"
    "        }",
)


def apply(name):
    path, old, new = CUTS[name]
    full = os.path.join(WT, path)
    s = io.open(full, encoding="utf-8").read()
    c = s.count(old)
    print(f"[{name}] 切在 {path} · 锚点命中 {c} 次", flush=True)
    if c != 1:
        sys.exit(f"🔴 锚点不是恰好一处（{c}）—— 这一刀作废，别记进表")
    io.open(full, "w", encoding="utf-8").write(s.replace(old, new))
    back = io.open(full, encoding="utf-8").read()
    assert new in back and old not in back, "🔴 写回去没落地"
    print(f"[{name}] **变异已落地**（{path}）", flush=True)


if __name__ == "__main__":
    if sys.argv[1] == "--list":
        for k in CUTS:
            print(k)
    else:
        apply(sys.argv[1])
