# `K-R128` 死值验 —— 给 `R8` / `R9` / `R10` 三条新判定逐刀切

> 挂 `KR128D1` / `KR128D2` / `KR128D3`。量于 **09-15**。
> 被测树 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r128`（分支 `track/k-r128`，基线 `f94d06f`）。
> 🔴 **全部在沙箱里跑**（K31）：镜像 `ccmon-devbox:latest`、`--network none`、
> 挂载档逐字沿用 `.claude/devbox/gate`（只把它固定那条 `bash scripts/gate.sh` 换成本台）。

## §0 量具住址 —— **为什么源码落在 `.md` 里**

本件的写区里有 `evidence/K-R128-deathvalue.md`，**没有** `.py`（`brief` 硬规则 2：
只许改单子里列出的那几个文件）。⇒ 变异台的**逐字源码落在本文件 `§3`**，
跑的时候拷成 `evidence/K-R128-deathvalue.py` 再跑，跑完删掉。
**这一条回交时点名给 PM 裁**：要么把 `.py` 补进写区，要么就按这个办法长期走。

⚠ **被测对象指向哪棵树**：变异台 `--tree` 默认 = 它自己所在目录的父目录；
每一刀在**一份新副本**上切（`brief` 12c），副本只带尺子够得着的三棵
（`src/` · `src-tauri/src/` · `evidence/`），**副本里的 `.git` 一律删掉**
（工作树的 `.git` 是一行指回原仓的指针，在副本里跑 git 会写进原树的暂存区）。
**副本 ↔ 全树的等价性不是假设** —— `M0` 那一刀就是拿副本跟全树的输出逐字对拍
（除「量具 / 被测树」那两行），现打**逐字相同**。

## §1 变异表（逐行真实输出）

每一刀都满足 `brief` 第 7 条：**切之前断言锚点恰好命中 N 次**，改完再数一次并打印
「变异已落地」（本趟 23 处落地记录，逐条在 `§2` 原样贴着）。

| 刀 | 挂哪条 dod | 切在哪个文件 / 哪一处 | 锚点命中 | 期望 | 现打 | 点名标签 | 判 |
|---|---|---|---|---|---|---|---|
| `M0` | — | 不切一刀（基线 ＋ 副本↔全树等价性对拍） | — | 不红 | 不红 | — | ✓ |
| `M1` | `KR128D2` 刀① | `src/settings/panel.ts` 追加一处 `.cc_integration_status(` 调用（前置断言：该形状现打 **0** 处） | 0→1 | 红 | 红 1 条 | `R9a`，逐字点名 **S2** ＋ **`src/settings/panel.ts`** | ✓ |
| `M2` | `KR128D2` 刀②（阴性对照） | `ruler.py` 里 `section_frontend_ratchet(...)` 那一行整段拿掉 ＋ 刀① | 1 | 不红 | 不红 | — | ✓ |
| `M3` | `KR128D2` **假红方向①** | `src/ccm-probe.ts` 追加一句**注释**提到 `deploy_remote_daemon`（S1） | 0→1 | **不红** | 不红 | — | ✓ |
| `M4` | `KR128D2` **假红方向②** | `src/ccm-probe.ts`（**已在 S2 名单里**）再追加一处 `.probe_ccm_cli(` 调用 | 0→1 | **不红** | 不红 | — | ✓ |
| `M5` | `KR128D3` 刀①a | `src/ipc/commands.ts`：`  probe_ccm_cli: (args: { origin: string }) =>` → `probe_ccm_cli_RENAMED` | 1 | 红 | 红 1 条 | `R10a`，点名 **`probe_ccm_cli`** ＋ **哪一份** | ✓ |
| `M6` | `KR128D3` 刀①b | `src-tauri/src/tool_registry.rs`：`claims()` 里 `addr: "cc_bus_deploy.rs::deploy_local_cc_bus",` 改名 | 1 | 红 | 红 1 条 | `R10b`，点名那个符号 | ✓ |
| `M7` | `KR128D3` 刀②（阴性对照） | `ruler.py` 里 `section_discipline_a(...)` 那一行整段拿掉 ＋ 刀①a | 1 | 不红 | 不红 | — | ✓ |
| `M8` | `KR128D3` **假红方向** | 三份共用文件各加一条**与那 22 条无关**的新命令 `zzz_unrelated_probe`（`commands.ts` 真加一个包装层入口，两份 `.rs` 各加一句） | 0→1 ×3 | **不红** | 不红 | — | ✓ |
| `M9` | `KR128D1` 刀① | `ruler.py` 的 `SPLIT_GROUPS`：S5 摘掉 `"write_skill_file",` | 1 | 红 | 红 **2** 条 | `R8a`（点名该命令「谁都没认领」）＋ `R9b`（连带） | ✓ |
| `M10` | `KR128D1` 刀② | `SPLIT_GROUPS`：S1 也认领 `write_skill_file`（同时住两组） | 1 | 红 | 红 2 条 | `R8b`（点名 **S1, S5**）＋ `R9a`（连带） | ✓ |
| `M9b` | `KR128D1` 刀①′（**最小面**） | `SPLIT_GROUPS`：S1 多认领一条盘上不存在的 `zzz_ghost_command` | 1 | 红 | 红 **1** 条 | **只有 `R8a`** | ✓ |
| `M11` | `KR128D1` 刀③（阴性对照） | `section_split_closure(...)` 整段拿掉 ＋ 刀①′ | 1 | 不红 | 不红 | — | ✓ |
| `M11b` | `KR128D1` 刀③′（阴性对照·配 `M9`） | `section_split_closure` ＋ `section_frontend_ratchet` **两节都拿掉** ＋ 刀① | 1 ×3 | 不红 | 不红 | — | ✓ |
| `M12` | `KR128D1`+`D3` **活体** | **真改一次命令名**：`parity_ledger.rs` 的 `LEDGER` 行 ＋ `cc_bus_deploy.rs` 的 `#[tauri::command]` 定义同拍改 | 1 ＋ 1 | 红 | 红 **4** 条 | `R8a`×2 ＋ `R10a` ＋ `R10b` | ✓ |
| `M13` | **反向控制** | `parity_ledger.rs` 只追加一句注释（`LEDGER` 一行没动） | 0→1 | **不红** | 不红 | — | ✓ |

**16 刀，0 刀不合期望。**

### 分母怎么数的

- 「红几条」的分母 = `ruler.py` 裁决段里 `^  RED \[标签\] …` 那一形的**行数**，
  由变异台用同一条正则现算（不是肉眼数的）。
- 「期望 = 红」那几刀**不只看 rc**：还要求点名里逐字出现指定的那几个词
  （`brief` 9：只看 rc 变红买到的是目录级塌陷）。缺一个词就判 ✗。
- 「最小面」= 只有该盖的那一条标签亮：`M1`(`R9a`) · `M5`(`R10a`) · `M6`(`R10b`) ·
  `M9b`(`R8a`) 四刀各只点红一条标签。

### 🔴 这张表自己逮到的一条（第一版是错的，如实留着）

`M11` 第一版写的是「拿掉 `R8` 那一节 ＋ `M9` 的刀 ⇒ 应当不红」，**现打是红**（`R9b`）。
原因：`SPLIT_GROUPS` **同时喂着 `R8` 和 `R9` 两条判定** ——
从表里摘掉 `write_skill_file`，S5 的落点集合也跟着少了 `src/views/inbox-view.ts`。
⇒ 那不是判据的缺陷，是**阴性对照挑错了刀**。
修法是两条：① 新增 `M9b`（一条盘上不存在的命令名，前端一处调用都没有 ⇒ `R9` 一动不动）
作为 `R8` 的**最小面**刀，`M11` 改配它；② 另加 `M11b`，把两节都拿掉来配 `M9`。
**这一耦合写进交回报告**：它是实情，不是缺陷，但读表的人必须知道。

## §2 逐刀原样输出

```
变异台：/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r128/evidence/K-R128-deathvalue.py
被测树：/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r128
全树基线：rc=0 · 判定行 `RULER:` OK · 输出 21076 字节
  · [M0] 不切一刀（基线）
  · [M0] 副本 ↔ 全树等价性：逐字相同 ✓（除「量具/被测树」两行）
  · [M1] 变异已落地 —— src/settings/panel.ts：前置断言「.cc_integration_status(」命中 0 次 → 追加 1 段
      RED [R9a] S2（①-本机半）**多出落点**：src/settings/panel.ts —— 钉住的是 src/ccm-probe.ts, src/launcher-diagnostics.ts, src/settings/cc_integration.ts。有人往上加了一份前端落点 ⇒ 第三块那一件的写区变大了，**不许靠把表改大让它绿**（`§0c`）
  · [M2] 变异已落地 —— evidence/K-R117-ruler.py：锚点命中 1 次 → 落地 1 处
  · [M2] 变异已落地 —— src/settings/panel.ts：前置断言「.cc_integration_status(」命中 0 次 → 追加 1 段
  · [M3] 变异已落地 —— src/ccm-probe.ts：前置断言「deploy_remote_daemon」命中 0 次 → 追加 1 段
  · [M4] 变异已落地 —— src/ccm-probe.ts：前置断言「void commands.probe_ccm_cli(」命中 0 次 → 追加 1 段
  · [M5] 变异已落地 —— src/ipc/commands.ts：锚点命中 1 次 → 落地 1 处
      RED [R10a] `probe_ccm_cli` 在 `src/ipc/commands.ts` 里的包装层入口不完整：TS 键命中 0 次 · `invoke("…")` 线上串命中 1 次（各应当恰好 1 次）—— 命令名被改过，**纪律 A 破了**
  · [M6] 变异已落地 —— src-tauri/src/tool_registry.rs：锚点命中 1 次 → 落地 1 处
      RED [R10b] `claims()` 里的装 / 卸符号 `cc_bus_deploy.rs::deploy_local_cc_bus_RENAMED` **既不是一条真 `#[tauri::command]`，也不在 `CLAIMS_NON_COMMAND_SYMBOLS` 里** —— 多半是有人在 `tool_registry.rs` 里改了命令名（纪律 A 破了）。
  · [M7] 变异已落地 —— evidence/K-R117-ruler.py：锚点命中 1 次 → 落地 1 处
  · [M7] 变异已落地 —— src/ipc/commands.ts：锚点命中 1 次 → 落地 1 处
  · [M8] 变异已落地 —— src/ipc/commands.ts：前置断言「zzz_unrelated_probe」命中 0 次 → 追加 1 段
  · [M8] 变异已落地 —— src-tauri/src/tool_registry.rs：前置断言「zzz_unrelated_probe」命中 0 次 → 追加 1 段
  · [M8] 变异已落地 —— src-tauri/src/parity_ledger.rs：前置断言「zzz_unrelated_probe」命中 0 次 → 追加 1 段
  · [M9] 变异已落地 —— evidence/K-R117-ruler.py：锚点命中 1 次 → 落地 1 处
      RED [R8a] 现打人群里这些命令**五件谁都没认领**：write_skill_file —— 新长出一条安装面命令而切件方案没覆盖它，或者有人改了命令名 ⇒ 第三块的写区当场不完整
      RED [R9b] S5（③装 MCP/skill）**少了落点**：src/views/inbox-view.ts —— 钉住的是 src/settings/cc-bus-section.ts, src/settings/mcp-section.ts, src/views/inbox-view.ts，现打 src/settings/cc-bus-section.ts, src
  · [M10] 变异已落地 —— evidence/K-R117-ruler.py：锚点命中 1 次 → 落地 1 处
      RED [R8b] 命令 `write_skill_file` 同时被 S1, S5 认领 —— 五件的写区就是这么撞上的
      RED [R9a] S1（①-远端半）**多出落点**：src/views/inbox-view.ts —— 钉住的是 src/settings/machine-card.ts。有人往上加了一份前端落点 ⇒ 第三块那一件的写区变大了，**不许靠把表改大让它绿**（`§0c`）
  · [M9b] 变异已落地 —— evidence/K-R117-ruler.py：锚点命中 1 次 → 落地 1 处
      RED [R8a] 分组表里这些命令**今天的人群里找不到**（`§S5b` 那 22 条里没有它）：zzz_ghost_command —— ⚠ **别改表去凑**：要么是切件方案指了一条盘上不存在的命令，要么是有人改了命令名（纪律 A 被破了）。两种都要人回来裁
  · [M11] 变异已落地 —— evidence/K-R117-ruler.py：锚点命中 1 次 → 落地 1 处
  · [M11] 变异已落地 —— evidence/K-R117-ruler.py：锚点命中 1 次 → 落地 1 处
  · [M11b] 变异已落地 —— evidence/K-R117-ruler.py：锚点命中 1 次 → 落地 1 处
  · [M11b] 变异已落地 —— evidence/K-R117-ruler.py：锚点命中 1 次 → 落地 2 处
  · [M11b] 变异已落地 —— evidence/K-R117-ruler.py：锚点命中 1 次 → 落地 1 处
  · [M12] 变异已落地 —— src-tauri/src/parity_ledger.rs：锚点命中 1 次 → 落地 1 处
  · [M12] 变异已落地 —— src-tauri/src/cc_bus_deploy.rs：锚点命中 1 次 → 落地 1 处
      RED [R8a] 分组表里这些命令**今天的人群里找不到**（`§S5b` 那 22 条里没有它）：deploy_local_cc_bus —— ⚠ **别改表去凑**：要么是切件方案指了一条盘上不存在的命令，要么是有人改了命令名（纪律 A 被破了）。两种都要人回来裁
      RED [R8a] 现打人群里这些命令**五件谁都没认领**：deploy_local_cc_bus_V2 —— 新长出一条安装面命令而切件方案没覆盖它，或者有人改了命令名 ⇒ 第三块的写区当场不完整
      RED [R10a] `deploy_local_cc_bus_V2` 在 `src/ipc/commands.ts` 里的包装层入口不完整：TS 键命中 0 次 · `invoke("…")` 线上串命中 0 次（各应当恰好 1 次）—— 命令名被改过，**纪律 A 破了**
      RED [R10b] `claims()` 里的装 / 卸符号 `cc_bus_deploy.rs::deploy_local_cc_bus` **既不是一条真 `#[tauri::command]`，也不在 `CLAIMS_NON_COMMAND_SYMBOLS` 里** —— 多半是有人在 `tool_registry.rs` 里改了命令名（纪律 A 破了）。⚠ 这一格补的正
  · [M13] 变异已落地 —— src-tauri/src/parity_ledger.rs：前置断言「[死值验 M13]」命中 0 次 → 追加 1 段

====================================================================================================
刀    挂哪条 dod                   期望    现打    红几条    点名标签            判
M0   —                         不红    不红    0      —               ✓
M1   KR128D2 刀①                红     红     1      R9a             ✓
M2   KR128D2 刀②（阴性对照）          不红    不红    0      —               ✓
M3   KR128D2 假红方向①             不红    不红    0      —               ✓
M4   KR128D2 假红方向②             不红    不红    0      —               ✓
M5   KR128D3 刀①a               红     红     1      R10a            ✓
M6   KR128D3 刀①b               红     红     1      R10b            ✓
M7   KR128D3 刀②（阴性对照）          不红    不红    0      —               ✓
M8   KR128D3 假红方向              不红    不红    0      —               ✓
M9   KR128D1 刀①                红     红     2      R8a, R9b        ✓
M10  KR128D1 刀②                红     红     2      R8b, R9a        ✓
M9b  KR128D1 刀①′（最小面）          红     红     1      R8a             ✓
M11  KR128D1 刀③（阴性对照）          不红    不红    0      —               ✓
M11b KR128D1 刀③′（阴性对照·配 `M9`）  不红    不红    0      —               ✓
M12  KR128D1+D3 活体             红     红     4      R10a, R10b, R8a ✓
M13  反向控制                      不红    不红    0      —               ✓
====================================================================================================
16 刀，0 刀不合期望。
```

## §3 变异台逐字源码

> 落在本文件里的理由见 `§0`。跑法（沙箱内，工作树根）：
> ```
> cp <本文件里这段> evidence/K-R128-deathvalue.py && python3 evidence/K-R128-deathvalue.py
> ```

```python
#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R128` 死值验台 —— 给本件新装的三条判定（`R8` / `R9` / `R10`）逐刀切。

## 量具住址 · 被测对象（`brief` 12）

* 本文件的**归档住址**：`<被测树>/evidence/K-R128-deathvalue.md` 里那段逐字源码
  （`.py` 不在本件写区 ⇒ 源码落在同名 `.md` 里，跑的时候拷出来跑）。
* 被测对象：`--tree`（默认 = 本文件所在目录的父目录）。**一个字节都不改被测树** ——
  每一刀在**一份新副本**上切（`brief` 12c：每版一份新副本；副本先删 `.git`，
  工作树的 `.git` 是一行指回原仓的指针，在副本里跑 git 会写进原树的暂存区）。
* 副本只带尺子够得着的那三棵：`src/` · `src-tauri/src/` · `evidence/`。
  **等价性不是假设** —— `M0` 那一刀就是拿副本跟全树对拍（除 HEAD 那一行）。

## 每一刀都要满足 `brief` 第 7 条

切之前**断言锚点恰好命中 N 次**再改，并打印「变异已落地」（改完再数一次）。
表里逐行记：切在**哪个文件 / 哪一处**、锚点**命中几次**。

## 判法

`期望 = 红` 的刀：rc 必须非 0，**而且**点名里要出现指定的那几个关键词
（`brief` 9：报「有牙」要说清射程；只看 rc 变红买到的是目录级塌陷）。
`期望 = 不红` 的刀：rc 必须 0。
"""

from __future__ import annotations

import argparse
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
NEED = ("src", os.path.join("src-tauri", "src"), "evidence")

# ── 变异表 ────────────────────────────────────────────────────────────────────
# kind:
#   replace   (文件, 锚点, 期望命中数, 换成什么)
#   append    (文件, 先断言这段**不存在**的串, 往文件尾追加的内容)
CUTS = [
    dict(id="M0", dod="—", why="基线：副本不切一刀（同时是副本 ↔ 全树的等价性对拍）",
         expect="不红", edits=[]),

    # ── `KR128D2` 棘轮 ────────────────────────────────────────────────────────
    dict(id="M1", dod="KR128D2 刀①", expect="红",
         why="往一个**今天不是该组落点**的前端文件里加一处调用 —— "
             "`src/settings/panel.ts` 今天只在注释里提到 `cc_integration_status`（S2）",
         want=["R9a", "S2", "src/settings/panel.ts"],
         edits=[dict(kind="append", file="src/settings/panel.ts",
                     absent=".cc_integration_status(",
                     text='\n// [死值验 M1] 本行是变异台加的，不是产品代码\n'
                          'void commands.cc_integration_status({ commandName: "cc" });\n')]),
    dict(id="M2", dod="KR128D2 刀②（阴性对照）", expect="不红",
         why="把本件新加的**棘轮判定整段拿掉**（`main()` 里那一行调用）＋ 刀① ⇒ 应当不红",
         edits=[dict(kind="replace", file="evidence/K-R117-ruler.py",
                     anchor="    section_frontend_ratchet(texts, cmds_now, wrapper)\n",
                     hits=1, to="    # [死值验 M2] 判定整段拿掉\n"),
                dict(kind="append", file="src/settings/panel.ts",
                     absent=".cc_integration_status(",
                     text='\nvoid commands.cc_integration_status({ commandName: "cc" });\n')]),
    dict(id="M3", dod="KR128D2 假红方向①", expect="不红",
         why="往一个**不是 S1 落点**的前端文件里加一句**注释**提到 S1 的命令 —— "
             "注释不是用户入口，**不许红**（否则这把尺子可以靠删注释变绿）",
         edits=[dict(kind="append", file="src/ccm-probe.ts",
                     absent="deploy_remote_daemon",
                     text="\n// [死值验 M3] 提一句 deploy_remote_daemon，但没有调用\n")]),
    dict(id="M4", dod="KR128D2 假红方向②", expect="不红",
         why="往一份**已经在名单里**的文件（`src/ccm-probe.ts`，S2）里再加一处调用 —— "
             "度量的是「几份文件」不是「几处引用」（诚实边界 B8）⇒ **不许红**",
         edits=[dict(kind="append", file="src/ccm-probe.ts",
                     absent='void commands.probe_ccm_cli(',
                     text='\nvoid commands.probe_ccm_cli({ origin: "x" });\n')]),

    # ── `KR128D3` 纪律 A ──────────────────────────────────────────────────────
    dict(id="M5", dod="KR128D3 刀①a", expect="红",
         why="在**共用文件 `src/ipc/commands.ts`** 里改掉一条命令名（只改 TS 键那一侧）",
         want=["R10a", "probe_ccm_cli", "src/ipc/commands.ts"],
         edits=[dict(kind="replace", file="src/ipc/commands.ts",
                     anchor="  probe_ccm_cli: (args: { origin: string }) =>",
                     hits=1, to="  probe_ccm_cli_RENAMED: (args: { origin: string }) =>")]),
    dict(id="M6", dod="KR128D3 刀①b", expect="红",
         why="在**共用文件 `src-tauri/src/tool_registry.rs`** 的 `claims()` 里改掉一条命令名 —— "
             "这一刀补的正是 `§S5` 种子表的**静默漏**（它会把解析不到的符号直接丢掉）",
         want=["R10b", "deploy_local_cc_bus_RENAMED"],
         edits=[dict(kind="replace", file="src-tauri/src/tool_registry.rs",
                     anchor='addr: "cc_bus_deploy.rs::deploy_local_cc_bus",',
                     hits=1,
                     to='addr: "cc_bus_deploy.rs::deploy_local_cc_bus_RENAMED",')]),
    dict(id="M7", dod="KR128D3 刀②（阴性对照）", expect="不红",
         why="把**纪律 A 那一节整段拿掉** ＋ 刀①a ⇒ 应当不红",
         edits=[dict(kind="replace", file="evidence/K-R117-ruler.py",
                     anchor="    section_discipline_a(texts, cmds_now, claims, cmd_addr, wrapper)\n",
                     hits=1, to="    # [死值验 M7] 判定整段拿掉\n"),
                dict(kind="replace", file="src/ipc/commands.ts",
                     anchor="  probe_ccm_cli: (args: { origin: string }) =>",
                     hits=1, to="  probe_ccm_cli_RENAMED: (args: { origin: string }) =>")]),
    dict(id="M8", dod="KR128D3 假红方向", expect="不红",
         why="往那三份共用文件里各加一条**与这 22 条无关**的新命令 ⇒ **不许红**"
             "（否则以后没人敢动那三份文件）",
         edits=[dict(kind="append", file="src/ipc/commands.ts",
                     absent="zzz_unrelated_probe",
                     text='\n// [死值验 M8]\n'
                          'export const zzzExtra = {\n'
                          '  zzz_unrelated_probe: (args: { x: string }) =>\n'
                          '    invoke<void>("zzz_unrelated_probe", args),\n};\n'),
                dict(kind="append", file="src-tauri/src/tool_registry.rs",
                     absent="zzz_unrelated_probe",
                     text="\n// [死值验 M8] 新命令 zzz_unrelated_probe，与那 22 条无关\n"),
                dict(kind="append", file="src-tauri/src/parity_ledger.rs",
                     absent="zzz_unrelated_probe",
                     text="\n// [死值验 M8] 新命令 zzz_unrelated_probe，与那 22 条无关\n")]),

    # ── `KR128D1` 闭集 ────────────────────────────────────────────────────────
    dict(id="M9", dod="KR128D1 刀①", expect="红",
         why="分组表里**漏掉**一条命令（S5 少认领 `write_skill_file`）⇒ 并集 ≠ 人群",
         want=["R8a", "write_skill_file", "谁都没认领"],
         edits=[dict(kind="replace", file="evidence/K-R117-ruler.py",
                     anchor='        "write_skill_file",\n', hits=1,
                     to='        # [死值验 M9] 这一条被摘掉了\n')]),
    dict(id="M10", dod="KR128D1 刀②", expect="红",
         why="把一条命令**同时**放进两组（S1 也认领 `write_skill_file`）⇒ 交集非空",
         want=["R8b", "write_skill_file", "S1", "S5"],
         edits=[dict(kind="replace", file="evidence/K-R117-ruler.py",
                     anchor='        "deploy_remote_daemon",\n', hits=1,
                     to='        "deploy_remote_daemon",\n        "write_skill_file",\n')]),
    dict(id="M9b", dod="KR128D1 刀①′（最小面）", expect="红",
         why="分组表里**多**一条盘上不存在的命令（S1 认领 `zzz_ghost_command`）⇒ 并集 ≠ 人群。"
             "🔴 这一刀是 `R8` 的**最小面**：不存在的命令名在前端一处调用都没有 ⇒ "
             "`R9` 的落点集合**一份都不动** ⇒ 只有 `R8a` 会红（对照 `M9`：那一刀连带点红了 `R9b`，"
             "因为同一张 `SPLIT_GROUPS` 同时喂着两条判定 —— 那是实情，不是缺陷）",
         want=["R8a", "zzz_ghost_command", "人群里找不到"],
         edits=[dict(kind="replace", file="evidence/K-R117-ruler.py",
                     anchor='        "deploy_remote_daemon",\n', hits=1,
                     to='        "deploy_remote_daemon",\n        "zzz_ghost_command",\n')]),
    dict(id="M11", dod="KR128D1 刀③（阴性对照）", expect="不红",
         why="把**闭集判定那一节整段拿掉** ＋ 刀①′（`M9b`）⇒ 应当不红。"
             "⚠ 对照用的是 `M9b` 不是 `M9`：`M9` 那一刀同时动了 `R9` 的输入，"
             "拿掉 `R8` 一节挡不住 `R9` 那一条红 —— **第一版就是这么写的，当场被这张表逮住**",
         edits=[dict(kind="replace", file="evidence/K-R117-ruler.py",
                     anchor="    section_split_closure(cmds_now)\n", hits=1,
                     to="    # [死值验 M11] 判定整段拿掉\n"),
                dict(kind="replace", file="evidence/K-R117-ruler.py",
                     anchor='        "deploy_remote_daemon",\n', hits=1,
                     to='        "deploy_remote_daemon",\n        "zzz_ghost_command",\n')]),
    dict(id="M11b", dod="KR128D1 刀③′（阴性对照·配 `M9`）", expect="不红",
         why="把**闭集判定 ＋ 棘轮两节都拿掉** ＋ 刀①（`M9`）⇒ 应当不红 —— "
             "`M9` 打中的是这两条判定，两条都拿掉才是它的阴性对照",
         edits=[dict(kind="replace", file="evidence/K-R117-ruler.py",
                     anchor="    section_split_closure(cmds_now)\n", hits=1,
                     to="    # [死值验 M11b] 判定整段拿掉\n"),
                dict(kind="replace", file="evidence/K-R117-ruler.py",
                     anchor="    section_frontend_ratchet(texts, cmds_now, wrapper)\n", hits=1,
                     to="    # [死值验 M11b] 判定整段拿掉\n"),
                dict(kind="replace", file="evidence/K-R117-ruler.py",
                     anchor='        "write_skill_file",\n', hits=1,
                     to='        # [死值验 M11b] 这一条被摘掉了\n')]),

    # ── 活体：真的破一次纪律 A（改命令名），看两条新闸谁认得出 ──────────────────
    dict(id="M12", dod="KR128D1+D3 活体", expect="红",
         why="**真改一次命令名**（`parity_ledger.rs` 的 `LEDGER` 行 ＋ `cc_bus_deploy.rs` 的"
             " `#[tauri::command]` 定义同拍改）—— 这就是纪律 A 被破的那一形。"
             "`parity_ledger.rs` 那一份 `§S5e` 逐字判不了，本刀证明它的闸真在 `§S5c`",
         want=["R8a", "deploy_local_cc_bus"],
         edits=[dict(kind="replace", file="src-tauri/src/parity_ledger.rs",
                     anchor='("deploy_local_cc_bus", "cc-bus.deploy", Side::Local),',
                     hits=1,
                     to='("deploy_local_cc_bus_V2", "cc-bus.deploy", Side::Local),'),
                dict(kind="replace", file="src-tauri/src/cc_bus_deploy.rs",
                     anchor="pub async fn deploy_local_cc_bus(", hits=1,
                     to="pub async fn deploy_local_cc_bus_V2(")]),
    dict(id="M13", dod="反向控制", expect="不红",
         why="往 `parity_ledger.rs` 里加一句**注释**（不动 `LEDGER` 的任何一行）⇒ 不红。"
             "没有这一刀，`M12` 的红分不清是「改了命令名」还是「碰了这份文件」",
         edits=[dict(kind="append", file="src-tauri/src/parity_ledger.rs",
                     absent="[死值验 M13]",
                     text="\n// [死值验 M13] 只是一句注释\n")]),
]


def make_copy(tree: pathlib.Path, dst: pathlib.Path) -> None:
    for sub in NEED:
        s = tree / sub
        d = dst / sub
        d.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(s, d)
    # `brief` 12c：副本里的 `.git` 一律不带（工作树的 `.git` 指回原仓）
    for p in dst.rglob(".git"):
        shutil.rmtree(p, ignore_errors=True) if p.is_dir() else p.unlink()


def apply_edits(dst: pathlib.Path, edits, cut_id: str):
    """逐刀：先断言锚点恰好命中 N 次，再改，改完再数一次并打印「变异已落地」。"""
    notes = []
    for e in edits:
        f = dst / e["file"]
        src = f.read_text(encoding="utf-8")
        if e["kind"] == "replace":
            n = src.count(e["anchor"])
            if n != e["hits"]:
                raise SystemExit(
                    f"✗ {cut_id}：锚点在 `{e['file']}` 里命中 {n} 次，期望 {e['hits']} 次"
                    f"（锚点：{e['anchor']!r}）—— 台子废了，不许当读数")
            out = src.replace(e["anchor"], e["to"])
            after = out.count(e["to"])
            f.write_text(out, encoding="utf-8")
            notes.append(f"{e['file']}：锚点命中 {n} 次 → 落地 {after} 处")
        elif e["kind"] == "append":
            n = src.count(e["absent"])
            if n != 0:
                raise SystemExit(
                    f"✗ {cut_id}：`{e['file']}` 里 {e['absent']!r} 本来就有 {n} 处，"
                    f"这一刀的前提不成立 —— 台子废了")
            f.write_text(src + e["text"], encoding="utf-8")
            notes.append(f"{e['file']}：前置断言「{e['absent']}」命中 0 次 → 追加 1 段")
        else:
            raise SystemExit(f"未知 kind：{e['kind']}")
    return notes


def run_ruler(dst: pathlib.Path):
    p = subprocess.run([sys.executable, str(dst / "evidence" / "K-R117-ruler.py")],
                       capture_output=True, text=True, timeout=300)
    return p.returncode, p.stdout + p.stderr


def reds(out: str):
    return re.findall(r"^  RED \[([^\]]+)\] (.*)$", out, re.M)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--tree", default=str(HERE.parent))
    ap.add_argument("--only", default="")
    args = ap.parse_args()
    tree = pathlib.Path(args.tree).resolve()

    print(f"变异台：{pathlib.Path(__file__).resolve()}")
    print(f"被测树：{tree}")
    base_rc, base_out = run_ruler(tree)
    print(f"全树基线：rc={base_rc} · 判定行 `RULER:` "
          f"{'OK' if 'RULER: OK' in base_out else 'FAIL'} · 输出 {len(base_out)} 字节")

    rows, bad = [], 0
    for cut in CUTS:
        if args.only and cut["id"] not in args.only.split(","):
            continue
        with tempfile.TemporaryDirectory(prefix=f"kr128-dv-{cut['id']}-") as td:
            dst = pathlib.Path(td) / "t"
            make_copy(tree, dst)
            notes = apply_edits(dst, cut["edits"], cut["id"])
            for n in notes:
                print(f"  · [{cut['id']}] 变异已落地 —— {n}")
            if not notes:
                print(f"  · [{cut['id']}] 不切一刀（基线）")
            rc, out = run_ruler(dst)
            rd = reds(out)
            got = "红" if rc != 0 else "不红"
            hit_ok, miss = True, []
            if cut["expect"] == "红":
                blob = "\n".join(f"{t} {m}" for t, m in rd)
                for w in cut.get("want", []):
                    if w not in blob:
                        hit_ok, miss = False, miss + [w]
            verdict = "✓" if (got == cut["expect"] and hit_ok) else "✗"
            if verdict == "✗":
                bad += 1
            if cut["id"] == "M0":
                # 等价性对拍：副本 ↔ 全树，除掉「被测树 / HEAD」那两行
                def norm(t):
                    return "\n".join(l for l in t.split("\n")
                                     if not l.startswith(("量具：", "被测树：")))
                same = norm(out) == norm(base_out)
                print(f"  · [M0] 副本 ↔ 全树等价性："
                      f"{'逐字相同 ✓' if same else '**不同 ✗**'}（除「量具/被测树」两行）")
                if not same:
                    bad += 1
            rows.append((cut["id"], cut["dod"], cut["expect"], got,
                         len(rd), ", ".join(sorted({t for t, _ in rd})) or "—",
                         verdict, ", ".join(miss), cut["why"]))
            for t, m in rd:
                print(f"      RED [{t}] {m[:180]}")

    print("\n" + "=" * 100)
    print(f"{'刀':<5s}{'挂哪条 dod':<26s}{'期望':<6s}{'现打':<6s}{'红几条':<7s}"
          f"{'点名标签':<16s}{'判'}")
    for r in rows:
        print(f"{r[0]:<5s}{r[1]:<26s}{r[2]:<6s}{r[3]:<6s}{r[4]:<7d}{r[5]:<16s}{r[6]}"
              + (f"  ← 点名里缺：{r[7]}" if r[7] else ""))
    print("=" * 100)
    print(f"{len(rows)} 刀，{bad} 刀不合期望。")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
```
