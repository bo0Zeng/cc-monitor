#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R117 第一拍（只读摸底）的尺子 —— 安装面人群 · 归属 · `Side` 栏复量。

╔══════════════════════════════════════════════════════════════════════════════╗
║ 量具住址 · 被测对象（纪律 12：量具住址要唯一定位到那一份，且要写明被测对象指向哪棵树）║
╚══════════════════════════════════════════════════════════════════════════════╝

* 本文件的住址：`<被测树>/evidence/K-R117-ruler.py`（**跟着被测树走**，不住临时目录）。
* 被测对象：默认 = **本文件所在目录的父目录**（即那棵树的根）。`--root` 可以指到别处
  （死值验就是这么做的：把树拷一份、在副本上变异、让同一份尺子去量副本）。
* 每趟开头都会把「我在量哪棵树 · 那棵树的 HEAD 是什么」印出来 —— 那两行就是这份读数的分母。

╔══════════════════════════════════════════════════════════════════════════════╗
║ 它判什么 · 不判什么                                                            ║
╚══════════════════════════════════════════════════════════════════════════════╝

**判**（判红，退出码非 0）：
  R1 人群没塌（每个解析器都有地板；扫不到东西一律 CRASH，不许当「零违例」）。
  R2 `installable: true` 那个数：**19 与 7 的差要逐行对得上**（Σ 分档 == 朴素 grep 数），
     且「真·字段声明」那一档必须等于 `TOOLS` 里 `installable == true` 的条数
     （两把互相独立的尺子对拍：一把数文本行、一把解声明表）。
  R3 归档覆盖：`LEDGER` 现打的每一个能力 id 都要在 `CAP_ARCHIVE` 里恰好一条。
     **少一条红** = 新长出一条命令（带新能力 id）而没人归档；**多一条红** = 表在腐烂。
  R4 种子不许被划出安装面：从 `tool_registry::claims()` 的装 / 卸实现符号派生出来的那几条
     命令，它们的能力**必须**归 ①/②/③。有人把一条真装口归成「非装面」⇒ 当场红。
  R5 写盘落点（`WRITE_SITES`）同样两向对拍：带 `Some(tool)` 的行逐行归档，
     多一行 / 少一行都红。
  R6 命令住址从源码派生：`LEDGER` 里的每条命令都要在 `#[tauri::command]` 面上找得到，
     找不到红（住址这一列不许手写）。
  R7 `Side` 栏那四处连锁**现打还在不在**（`EXPECTED_LOCAL_OR_BOTH` · `ORIGIN_TAKING_BOTH` ·
     `tmux.manage` 那条散文 · 钉那句散文的判据名）。少一处红 —— 那意味着 `K-R115` 交回的
     代价读数已经不是今天的形状。

〔`K-R128` 09-15 加的三条 —— 第三块（S1–S5）开工前那两条「说了但没有闸」的判定〕：
  R8 切件分组表的**闭集判定**（`KR128D1`）：`SPLIT_GROUPS` 五组的并集**逐字等于**现打人群
     （两向点名）· 五组**两两交集为空**。防的是「切件方案与量具各说各话」。
  R9 前端落点**棘轮**（`KR128D2`）：每组现打落点名单 **==** `FRONTEND_PIN` 钉住的名单。
     🔴 钉的是**名单**，份数一律 `len()` 现算（表里一个基数字面量都没有）；判 `==` 不判 `<=`。
     🔴 **落点只认调用形状 `.<命令>(`** —— 只在注释 / 散文里提到命令名的那一份不算落点，
        否则这把尺子可以**靠删一条注释变绿**，而「落点收到 1」正是 S1–S5 的收工判据。
  R10 **纪律 A 的闸**（`KR128D3`）：`R10a` 22 条在 `src/ipc/commands.ts` 里逐条有包装层入口
     （TS 键 ＋ `invoke` 线上串两侧都在）· `R10b` `claims()` 每个装 / 卸符号要么是真
     `#[tauri::command]`、要么在 `CLAIMS_NON_COMMAND_SYMBOLS` 明示名单里。
     🔴 钉的是**命令名**不是文件字节 ⇒ 往那三份里加一条**与这 22 条无关**的新命令**不红**。

〔`K-R131` 09-15 加的一条 —— 第三块（S1–S5）的**收工目标**〕：
  R11 收工目标的**算术闭合**（`KR131D3`）：（五组目标的并集 － 非入口）**逐字等于**全盘目标
     那张名单（两向点名）· 豁免不许是死钉 · 豁免与全盘目标不许相交 · 逐组目标表与分组表
     同一批组。🔴 **目标只许有一处住址**（那三张表），`FRONTEND_PIN` 的散文里不许再有第二份。

**不判**（诚实边界，逐条写死）：
  B1 **不判归属对不对**。`CAP_ARCHIVE` 的每一格是**人的判断**（依据写在那一格的 `why` 里），
     本尺子只保证「没有一条命令没人回答过」，不保证「回答得对」。
     ——— 同 `tool_registry::UnmanagedEnv.who` 那条头注的口径：设计判断不是读数。
  B2 **不判「翻了 `Side` 会不会真的对」**。它只数「今天有几行、签字表说几行、连锁在不在」。
  B3 **`Side` 的派生类（`RemoteOnly` / `FramePlane` / …）本尺子不重算** ——
     那是 `parity_ledger::command_dispatch_class()` 的活，它要用 `guard_core` 的整套剥法
     （块注释 / 行尾注释 / `#[cfg(test)] mod` 的词法掩码收尾）。在这里抄一份近似的剥法，
     就是本仓头注反复点名的「同一件事的第二份表示，而它们会各自漂」。
     ⇒ 本尺子只复量**纯数据**那几个（55 行 · 签字表四档 · 人裁 6 条里几条是「说假话」），
     派生那一半由 `cargo test -p monitor` 跑 `the_remote_side_column_is_signed_off` 给。
  B4 只看 `src-tauri/src/**.rs`（外加 `build.rs` 这个名字出现在 `WRITE_SITES` 里那一行）。
     `remote-daemon-proto/` 那棵树、`src/**.ts` 前端面、shell 脚本，本尺子一概盖不到。
  B5 解析是**文本解析**，不是 `syn`。它认的是本仓 rustfmt 之后的那几种固定形状
     （见每个 parser 的 docstring）；形状一变就会掉到地板断言上（R1）——
     **宁可 CRASH，不许静默少数几条**。
  B6 〔`K-R128`〕**`src-tauri/src/parity_ledger.rs` 这一份 `R10` 判不了，而且是结构性的**：
     那 22 条命令名就是从它解析出来的（`parse_ledger`）⇒ 拿它回头判它恒真（空真）。
     它的闸在 `R8`（现打人群变了，而 `SPLIT_GROUPS` 是字面量名单、不会跟着变 ⇒ 两向红）。
     ⇒ **别把 `§S5e` 读成「三份共用文件都判了」。**
  B7 〔`K-R128`〕`R9` 只认**一种**调用形状（`.<命令>(`）。别的形状（`invoke("<名>")` 直呼 ·
     先解构再裸调）本尺子**看不见**。为了让它不静默，`§S5d` 把「只提到、没有调用形状」
     那一档**逐处 `文件:行号` 印出来**（只出读数、不判红）——
     哪天有人换了调用形状，那一份会从「落点」掉进第二档，在读数面上当场可见。
  B8 〔`K-R128`〕`R9` 度量的是「**几份文件**」，不是「几处引用」⇒ 往一份**已经在名单里**的
     文件里再加一处引用，**不红**。这是刻意的：第三块的目标就是把引用收进那几份留下来的文件。
  B9 〔`K-R131`〕**`R11` 只读本文件自己的三张表，一行被测树都不读** ⇒ 它够不着「目标定得
     对不对」，也不会因为产品代码变了而红。它买到的只有「那四条算术关系没人能悄悄写歪」。
  B10 〔`K-R131`〕**「落点」不等于「用户看到的一处」，而收工判据量的是前者** ——
     两条现打的反例都在盘上：① `src/ccm-probe.ts` 是渲染链的探测缓存，**界面上没有这一处**，
     却算一份落点；② `buildAccountAliasBlock` 现打**渲染在两处**（`panel.ts` 那一组 ＋
     `accounts-section.ts`），而调用写在 `launcher-diagnostics.ts` 里 ⇒ 只算**一份**落点。
     ⇒ **「该组落点收到 1」买到的是「调用收敛到一份文件」，不是「用户只看到一处」。**
     今天这个差由 `FRONTEND_NON_ENTRY` 逐份写出来顶着，**没有闸在数「用户看到几处」**。
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from collections import Counter, OrderedDict

# ══════════════════════════════════════════════════════════════════════════════
# 归档表 —— **人的判断住这里**，每一格都要给依据（定框 id）。
#
# 🔴 这张表不是「人群」。人群从源码派生（`LEDGER` 现打有几个能力 id 就是几个），
#    这张表只回答「那一条归三处里的哪一处」。覆盖不全 ⇒ R3 当场红。
#
# 三处的名字取自 `R63` 裁定三（件文件 `§0b`）：
#   ① 装后端  ② 生成 rc 片段让用户自己填  ③ 装 MCP / skill 等
#   非装面   —— 它根本不是安装 / 卸载 / 查装态的动作
# ══════════════════════════════════════════════════════════════════════════════

B1 = "①装后端"
B2 = "②生成rc片段"
B3 = "③装MCP/skill"
NA = "非装面"

BUCKETS = (B1, B2, B3, NA)

# cap -> (bucket, 依据, 一句话)
CAP_ARCHIVE: "OrderedDict[str, tuple]" = OrderedDict([
    # ─────────────────────────── ① 装后端 ───────────────────────────
    ("ccm.install", (B1, "R64+K26",
                     "`ccm` 就是后端的 CLI 入口；`install_local_ccm_entry` 放下去的逐字是"
                     "「后端二进制自己的改名副本」⇒ 装 ccm 与装后端是同一个动作")),
    ("ccm.uninstall", (B1, "R64+K26", "同上，卸那一侧")),
    ("ccm.install-ui", (B1, "R64+K26", "装 ccm 之前的预览 / 扫 PATH，是同一颗按钮的前半")),
    # 🔴 `K-R135`（`R85`/`R87`/`R88`）：用户级 PATH 那一格（现在状态 · 加 · 撤）。
    # 归 ① 而不是 ②：② 是「生成 rc 片段**让用户自己填**」，而这一格是**用户点一下、产品就执行**
    # （`R85` 逐字「应该让用户手动点击加，也能管理删除」）。它是「装 ccm」这件事的**后半** ——
    # 二进制放下去了却敲不到等于没装（`K-R129`/`K-R132` 那条已发版缺陷就是这一形）
    # ⇒ 与 `ccm.install` 是同一颗按钮的两半。⚠ 同族的 `ccm.install-ui` 早就在 ① 里，
    # 而它的理由逐字写着「装 ccm 之前的预览 / **扫 PATH**」—— PATH 这件事本来就归 ①。
    ("ccm.user-path", (B1, "R85+R87+R88",
                       "把我们那个 bin 目录放上**用户级** PATH，让三种终端都敲得到 `ccm`；"
                       "POSIX 那一侧的同一件事是写进 rc 的那个围栏块")),
    ("ccm.status", (B1, "R64+K26+K-R111",
                    "查装态。`K-R111` 已判 `cc_integration_status` 归属重判、该挪进安装面")),
    ("daemon.deploy", (B1, "K33+K27", "推 / 撤远端那一份后端，本来就是「装后端」")),
    ("acct-iso.deploy", (B1, "R75〔用@09-14「account进后端」〕", "已裁：account 进后端 ⇒ 落 ①")),
    ("acct-iso.check", (B1, "R75", "同上，查装态那一半")),
    # ──────────────────── ② 生成 rc 片段让用户自己填 ────────────────────
    ("alias.account-commands", (B2, "K33+K34",
                                "「有需要动用户 alias 的就生成命令让用户自己填」；"
                                "先例 `accounts-section.ts::renderRcSnippet`")),
    ("acct-iso.shellinit", (B2, "K33",
                            "它产出的就是一段 rc 片段（`cc-acct-iso` 的 `cmd_shellinit` "
                            "只 `printf`、一个字节都不写盘）")),
    # ─────────────────────── ③ 装 MCP / skill 等 ───────────────────────
    ("mcp.write", (B3, "K34", "「包括安装 skill / MCP 等等」")),
    ("mcp.remove", (B3, "K34", "同上，撤那一侧")),
    ("cc-bus.deploy", (B3, "K34",
                       "落点是 `<claude_dir>/skills/cc-bus/` ⇒ 属「装 skill」，不属「装后端」")),
    ("cc-bus.install-state", (B3, "K34", "同上，查装态那一半")),
    ("skill.inbox", (B3, "K34",
                     "写侧（`write_skill_file`）是往用户项目里装东西；读侧两条由 "
                     "`CMD_OVERRIDE` 划出去 —— 同一个能力 id 里读写两性质")),
    # ─────────────────────────── 非装面 ───────────────────────────
    # 下面每一条都是「它不是装 / 卸 / 查装态的动作」。理由一律给**它到底在干什么**，
    # 不写「与安装无关」这种同义反复。
    ("accounts.last-used", (NA, "—", "读账号最近用过谁")),
    ("accounts.list", (NA, "—", "列账号")),
    ("accounts.session-accounts", (NA, "—", "列会话用的账号")),
    ("accounts.trust", (NA, "—", "查远端信任态（ssh 那一侧，不是装东西）")),
    ("app.auto-launch", (NA, "—", "app 自己的开机自启开关")),
    ("app.config", (NA, "—", "app 自己的配置读写")),
    ("app.daemon-policy", (NA, "—", "app 自己的退出策略开关")),
    ("app.data-paths", (NA, "—", "报 app 自己的数据目录")),
    ("app.diagnostics", (NA, "—", "app 自己的诊断开关 / 埋点")),
    ("app.logs", (NA, "—", "app 自己的日志")),
    ("app.window.self", (NA, "—", "窗口动作")),
    ("app.window.session", (NA, "—", "窗口动作")),
    ("app.window.settings", (NA, "—", "窗口动作")),
    ("audit.config-surface", (NA, "—", "配置面审计页的**读**侧（它报告安装面，不改它）")),
    ("audit.drift-ledger", (NA, "—", "漂移账本的读侧")),
    ("cc-bus.cockpit", (NA, "—", "cc-bus 驾驶舱的读 / 发消息，不是装 cc-bus")),
    ("creds.relay-key", (NA, "—", "中转那把第三方 API key（monitor 自己的凭据文件）")),
    ("daemon.lifecycle", (NA, "—", "起 / 停 / 列后端进程 —— 是**跑**它，不是**装**它")),
    ("daemon.status", (NA, "—", "问后端活没活 —— 同上，不是查装态")),
    ("history.branch", (NA, "—", "会话历史")),
    ("history.delete", (NA, "—", "会话历史")),
    ("history.list-projects", (NA, "—", "会话历史")),
    ("history.list-sessions", (NA, "—", "会话历史")),
    ("history.metadata", (NA, "—", "会话历史")),
    ("history.read-session", (NA, "—", "会话历史")),
    ("hooks.diagnose", (NA, "—",
                        "**只诊断**钩子，不装它（`TouchEffect::GenerateOnly`：产品自己不动手）")),
    ("launch.render-attach", (NA, "—", "渲染一条命令串")),
    ("launch.render-cli", (NA, "—", "渲染一条命令串")),
    ("launch.render-payload", (NA, "—", "渲染一份载荷")),
    ("launch.send-into", (NA, "—", "往已有会话里送一段")),
    ("mcp.list-origins", (NA, "—", "列 MCP 的 origin —— 读")),
    ("mcp.list-project-dirs", (NA, "—", "列项目目录 —— 读")),
    ("mcp.read", (NA, "—", "读 MCP 配置")),
    ("panorama.code-graph", (NA, "—", "代码全景（sidecar 的**用**，不是装它）")),
    ("plugins.marketplaces", (NA, "—", "列插件市场 —— 只读枚举")),
    ("port-forward", (NA, "—", "端口转发")),
    ("relay.routing", (NA, "—", "问这几个账号走不走中转")),
    ("search.history", (NA, "—", "搜索")),
    ("search.index", (NA, "—", "搜索索引（app 自己的索引，不落用户环境）")),
    ("session.activity", (NA, "—", "会话")),
    ("session.forget", (NA, "—", "会话")),
    ("session.launch", (NA, "—", "起会话")),
    ("session.list-active", (NA, "—", "会话")),
    ("session.tasks", (NA, "—", "会话")),
    ("sftp.file-panel", (NA, "—", "文件面板（用户自己搬文件，不是产品装东西）")),
    ("ssh.host-config", (NA, "—", "ssh 主机配置 / 推公钥 —— 连得上那一层，不是装我们的东西")),
    ("subagent.load", (NA, "—", "读 subagent 定义")),
    ("terminal.focus", (NA, "—", "把终端提到前台")),
    ("tmux.local-census", (NA, "—", "列 tmux 会话")),
    ("tmux.manage", (NA, "—", "tmux 会话管理")),
    ("usage.aggregate", (NA, "—", "用量")),
    ("usage.per-account", (NA, "—", "用量")),
])

# 同一个能力 id 里读写两性质时，按命令名覆盖。**key 必须在 `LEDGER` 里**（R3 的第三向）。
CMD_OVERRIDE = {
    "list_skills": (NA, "K34", "列 skill —— 只读，不是装"),
    "read_skill_file": (NA, "K34", "读那个收件箱文件 —— 只读"),
}

# 写盘落点（`write_site_registry::WRITE_SITES`）里**带 tool id 的那几行** ＋ `§0b` 另外点名的
# `build.rs::embed_daemons`（它的 tool id 是 `None`，见 §S5 那段读数）。
# key = "文件::函数"。两向对拍：人群从 `WRITE_SITES` 现打，这张表少一行 / 多一行都红。
SITE_ARCHIVE = {
    "local_backend.rs::install_local_ccm_entry": (
        B1, "R64+K26", "放的就是「后端二进制自己的改名副本」"),
    "build.rs::embed_daemons": (
        B1, "K33", "把内嵌后端复制进 `OUT_DIR` —— 装后端那条链的构建期一环"),
    "profile_installer.rs::install_to_profile": (
        B2, "K33", "今天真写用户 profile；② 的目标形状是改成只生成待贴片段"),
    "profile_installer.rs::uninstall_from_profile": (
        B2, "K33", "同上，摘那一侧"),
    "profile_installer.rs::atomic_write_string": (
        B2, "K33", "上面两个动作唯一的落盘漏斗"),
    "profile_installer.rs::atomic_replace_path": (
        B2, "K33", "跨设备回退的 rename，同一条落盘链"),
    "cc_bus_deploy.rs::deploy_into": (
        B3, "K34", "写 `<claude_dir>/skills/cc-bus/`"),
    "mcp.rs::write_json_atomic": (
        B3, "K34", "写项目级 MCP 配置，是 `mcp.write` 的真落点"),
}

# `§0b` 里点名、而 `WRITE_SITES` 的 tool id 是 `None` 的那几行 —— 单列，别混进「带 id」那个数。
SITE_TOOLID_NONE_BUT_ARCHIVED = ("build.rs::embed_daemons",)

# ══════════════════════════════════════════════════════════════════════════════
# `K-R128`（09-15）—— 第三块（S1–S5）开工前，那两条「说了但没有闸」的判定
#
# 🔴 这一整块是 `K-R128` 新加的；`K-R117` 第一拍那七条（`R1`–`R7`）**一个字节没动**，
#    `§S5b` 那段读数的输出也**逐字不变**（只把它那段目录遍历抽成了 `collect_ts()`，
#    好让本块与它**共用同一个分母**——两处各写一遍 `os.walk` 正是本文件头注点名的
#    「同一件事的第二份表示，而它们会各自漂」）。
#
# 为什么要有这一块（两条，失效方向不一样）：
#   甲 `K-R117` `§3-3` 的**收工判据**逐字要「该件那一组的前端落点数恒等于 1」，
#      而 `§S5b` 今天**只出读数、不判红** ⇒ 那句话没有闸。
#   乙 **纪律 A**（「第二拍全程不改命令名 ⇒ 三份共用文件一字不动 ⇒ 五件写区才真不相交」）
#      今天**完全建立在纪律上，没有任何东西在盘它**，而 S1 / S5 是方案里唯一许并跑的一对。
# ══════════════════════════════════════════════════════════════════════════════

# ── 分组表（`KR128D1`）—— **人的判断住这里**，逐行写死出处 ────────────────────
#
# 出处：`K-R117` 件文件 `features/K-R117-安装面收成三处.md#§3-3` 那张五行表。
# ⚠ **那张表没有逐条列命令名** —— 它每一行给的是「后端写区 · 前端写区 · 收几条」。
#   下面这五行是从「归处（①②③）＋ 后端写区那一列 ＋ 收几条那一列」派生出来的
#   **人的判断**，派生依据逐行写在第三栏。**派生对不对由 `R8` 盘**（闭集两向 ＋ 交集空）。
#
# ⚠ **刻意没有拿「今天住哪」去盘这张表**，理由写出来别当省略：`§3-3` 的「写区（后端）」
#   那一列写的是**搬过去之后**的落点，不是今天的住址 —— S4 那一行现打就对不上
#   （它写 `account_aliases.rs` · `profile_installer.rs`，而那两条命令今天住
#   `acct_iso_deploy.rs:126` 与 `lib.rs:1772`）。拿住址当判据会把「计划」读成「现状」。
#
# 🔴 **这张表是本块唯一的字面量名单**，它不是「人群」的第二住址：人群现打从 `LEDGER`
#    ＋ `CAP_ARCHIVE` 派生（就是 `§S5` 那 22 条），这张表只回答「那一条归五件里的哪一件」。
#    两边任何一侧漂了，`R8a`/`R8b` 当场红。
SPLIT_GROUPS: "OrderedDict[str, tuple]" = OrderedDict([
    ("S1", ("①-远端半", (
        "deploy_remote_daemon",
        "install_remote_ccm_helper",
        "uninstall_remote_ccm_helper",
        "uninstall_remote_daemon",
    ), "§3-3 第一行：后端写区 `sftp.rs`，收 4 条 —— 括号里逐字「`daemon.deploy`×2 ＋ "
       "`ccm.install`/`ccm.uninstall` 的远端半」。现打这四条的住址恰好都在 `sftp.rs`")),
    ("S2", ("①-本机半", (
        "cc_integration_install",
        "cc_integration_preview",
        "cc_integration_scan_path",
        "cc_integration_status",
        "cc_integration_uninstall",
        "ccm_user_path_add",
        "ccm_user_path_remove",
        "ccm_user_path_status",
        "local_ccm_entry_status",
        "probe_ccm_cli",
    ), "§3-3 第二行：后端写区 `lib.rs`（`cc_integration_*` ＋ `ccm_user_path_*`）· `ccm_probe.rs`，"
       "收 10 条 = `lib.rs` 里五条 `cc_integration_*` ＋ **三条 `ccm_user_path_*`** ＋ `ccm_probe.rs` 里两条。"
       "⚠ 限定词承重：`lib.rs` 里还住着 `write_account_aliases`，那条归 S4。"
       "🔴 〔`K-R135` 09-15〕那三条 `ccm_user_path_*`（用户级 PATH 那一格：现在状态 / 加 / 撤）"
       "是本轮新长出来的装口，归 S2 的理由有三条、且**没有第二个连贯的归属**："
       "① 它们归档在 `①装后端`（见 `CAP_ARCHIVE` 的 `ccm.user-path`）⇒ 只可能落 S1/S2/S3；"
       "② 它们是**本机**半（S1 是远端、S3 是 acct-iso）；③ 它们住 `lib.rs`，正是 S2 的后端写区。"
       "⚠ 而且**前端落点一个字都没多**：它们的调用点在 `src/launcher-diagnostics.ts`，"
       "那一份本来就在 `FRONTEND_PIN['S2']` 里 ⇒ `R9` 那条棘轮不动。"
       "⚠ **这一处不是纯计数随动，它把 S2 的射程从「逐字 `cc_integration_*`」扩到也含 "
       "`ccm_user_path_*`** —— 属切件方案的改动，已在 `K-R135 §8` 里点名请 PM 追认")),
    ("S3", ("①-account 半", (
        "check_remote_acct_iso",
        "deploy_remote_acct_iso",
    ), "§3-3 第三行：后端写区 `acct_iso_deploy.rs` ＋ 本机装口新落点，收「2 条 ＋ 1 条欠口」。"
       "① 里住 `acct_iso_deploy.rs` 的恰好这两条（同文件的 `remote_acct_iso_shellinit` 归 ②）；"
       "那「1 条欠口」今天盘上还不存在 ⇒ 不进闭集")),
    ("S4", ("②生成 rc 片段", (
        "remote_acct_iso_shellinit",
        "write_account_aliases",
    ), "§3-3 第四行：件 = ②，收「2 条 ＋ 4 处写盘落点」。② 这一处现打恰好 2 条命令"
       "（`§S5` 归处栏）；那 4 处写盘落点不是命令，住 `SITE_ARCHIVE`，不进本闭集")),
    ("S5", ("③装 MCP/skill", (
        "cc_bus_install_state",
        "deploy_local_cc_bus",
        "remove_project_mcp_server",
        "remove_remote_mcp_server",
        "write_project_mcp_server",
        "write_remote_mcp_server",
        "write_skill_file",
    ), "§3-3 第五行：件 = ③，收「7 条 ＋ 2 处写盘落点」。③ 这一处现打恰好 7 条命令；"
       "那 2 处写盘落点同样住 `SITE_ARCHIVE`，不进本闭集")),
])

# ── 前端落点棘轮（`KR128D2`）—— 钉的是**名单**，份数由 `len()` 现算 ───────────
#
# 🔴 **为什么是棘轮、不是「恒等于 1」**（件文件 `§0c`）：目标是每组收成 1 份、全盘 3 份，
#    而今天全盘 8 份。把「恒等于 1」直接打开 ⇒ 当场全红，而且要红到第三块做完 ——
#    那不是闸，那是把门禁钉死。⇒ 钉**今天这一刻的名单**，判**逐字相等**：
#    第一天装上就是绿的；任何人往上**加一份落点**，或者把这张表**改馊**，当场红。
#
# 🔴 **判 `==` 不判 `<=`**（`§0c` 逐字）：`<=` 只防涨、不防「悄悄记错」，
#    而本区最贵的病正是「数与名单不同句」。⇒ 这里**只钉名单**，份数一律 `len()` 现算，
#    表里一个基数字面量都不许有（`brief` 13b）。
#
# 🔴 **它挡路的时候，合法出路只有一条**（`references/testing.md` 判据硬规则 11/12）：
#    S1–S5 哪一件把自己那一组的落点收掉了，**同一拍**把那一件那一行的名单改小 ——
#    「降不动就是没做完」。**不许改成 `<=`、不许把名单改大来让今天好过。**
#    ⇒ 重裁落点逐行写在第三栏。
#
# 每行：组 -> (钉住的落点名单, 这个数是哪天量的 · 用什么量的, 该变的时候谁来改)
FRONTEND_PIN: "OrderedDict[str, tuple]" = OrderedDict([
    ("S1", (("src/settings/machine-card.ts",),
            "量于 09-15 · `K-R128` 实现方现打（本文件 `§S5d`，被测树 = `--root`）",
            "S1 收完远端半那一拍改这一行。**目标不在这一栏** —— 住 "
            "`FRONTEND_GOAL_PER_ITEM['S1']`（`K-R131` 09-15：这一栏从前逐字写着"
            "「目标：空」，与 `FRONTEND_GOAL_PER_GROUP = 1` 同份输出里打架）")),
    ("S2", (("src/ccm-probe.ts",
             "src/launcher-diagnostics.ts",
             "src/settings/cc_integration.ts"),
            "量于 09-15 · 同上",
            "S2 收完本机半那一拍改这一行。**目标不在这一栏** —— 住 "
            "`FRONTEND_GOAL_PER_ITEM['S2']`。"
            "⚠ `src/settings/panel.ts` **不在**这张名单里：它今天只在一句注释里提到 "
            "`cc_integration_status`，不是调用点（见 `§S5d` 第二档）")),
    ("S3", (("src/settings/accounts-section.ts",),
            "量于 09-15 · 同上",
            "S3 那一拍改这一行。**目标不在这一栏** —— 住 "
            "`FRONTEND_GOAL_PER_ITEM['S3']`。⚠ 现打只剩 1 份这件事**只是现打**，"
            "不等于到位：`K-R131` 裁定 S3 的落点要从 `accounts-section.ts` 搬到 ① 那一处。"
            "（`§3-3` 写「S3 今天 2」是把 `src/accounts.ts` 那条**注释里的提名**"
            "算成了落点，见 `§S5d` 第二档 —— 那一半仍然成立）")),
    ("S4", (("src/launcher-diagnostics.ts",
             "src/settings/accounts-section.ts"),
            "量于 09-15 · 同上",
            "S4 那一拍改这一行。**目标不在这一栏** —— 住 "
            "`FRONTEND_GOAL_PER_ITEM['S4']`")),
    ("S5", (("src/settings/cc-bus-section.ts",
             "src/settings/mcp-section.ts",
             "src/views/inbox-view.ts"),
            "量于 09-15 · 同上",
            "S5 那一拍改这一行。**目标不在这一栏** —— 住 "
            "`FRONTEND_GOAL_PER_ITEM['S5']`")),
])

# ── 收工目标〔`K-R131` 09-15 裁定〕 ─────────────────────────────────────────────
#
# 🔴 **这里从前是一个标量** `FRONTEND_GOAL_PER_GROUP = 1`（五组共用），而 `FRONTEND_PIN`
#    第三栏的散文同时写着「S1 …（目标：空）」⇒ **同一份输出里 S1 的目标既是 1 又是空**。
#    那正是本区自己命名的最贵那条病「**数与名单不同句**」，而它长在治这条病的尺子上。
#    ⇒ `K-R131` 把目标收成**一处住址**：下面这三张表。**散文里不许再出现第二份目标。**
#
# 🔴 **「全盘目标」这个数的出处**（`KR131D1`：现打出处，不许说「大家都这么写」）：
#    `DECISIONS.md#R63` **裁定三**〔`src: 用@09-13`，用户逐字〕——
#      「现在应该只有安装后端, 以及bash命令等等 / 然后就是安装mcp功能\skill等等 /
#        不要拆成什么多个」
#    ⇒ R63 裁定三把它落成三条：一处「安装后端」· 一处「生成 bash/PowerShell 那段让用户
#      自己填」· 一处「安装 MCP / skill 等」，并逐字明令
#      「**不许再按『装 ccm 助手 / 装别名 / 装 tmux hooks / 装账号隔离』这样按实现分**」。
#    ⚠ **`K-R117#§3-3` 那一行给的出处是半个** —— 它写「前端落点 10 份 → 目标 3 份」，
#      第三栏署的是「`#§C5` 现打」，而 `§C5` 只现打得出**左边那个 10**；
#      **右边那个 3 不在 `§C5` 里**，它来自 R63 裁定三。⇒ 出处补在这里。
#    ⚠ **左边那个数今天也换了尺子**：`§3-3` 的 10 用的是 `§S5b`「被谁**引用**」的口径；
#      而收工判据用的是 `§S5d`「有**调用形状**」的口径（`K-R128` 收窄的）⇒ 两者现打不同。
#      **目标要按 `§S5d` 的分母说话**，不许把 `§3-3` 那个 10 的目标原样搬过来。
#
# 🔴 **「处」不是「份文件」** —— R63 数的是**用户要去的地方**，这把尺子数的是**有调用形状的
#    `.ts` 文件**。两者今天差两份，逐份写在 `FRONTEND_NON_ENTRY` 里并由 `R11b` 钉住。
#    ⇒ 算术这样才闭合：`五组目标的并集 － 非入口 == 全盘目标那张名单`（`R11a`）。

# 全盘目标 —— **这张表就是 R63 裁定三那三条**。key = 收工之后用户去的那一份文件。
FRONTEND_GOAL_OVERALL: "OrderedDict[str, tuple]" = OrderedDict([
    ("src/settings/machine-card.ts", (
        "①一处「安装后端」",
        "R63裁定三+K15+K35+K36+R75",
        "「装后端」本来就是**对某一台机器**的动作 ⇒ 它的入口该跟着机器走。UI 侧这一处"
        "今天已经存在：`S4b-3b-2` 把 `MachineCard` 拆成「连接 / 组件」两栏，**组件栏**"
        "逐字就是「这台机器上装了什么」。`K15`/`K35`/`K36`（本地 = 不走 ssh 的远端 · "
        "没有「没有后端」这回事 · 两份后端逐字相同）⇒ **本机页要与远端页同形**，"
        "本机的装口进同一栏，而不是另起一处「终端集成」。"
        "⚠ 现打 `remote-section.rebuild_cards` 只给远端建 `MachineCard`，本机页是一个"
        "空 div、只靠 per-machine 分节填 ⇒ **本机今天在结构上是二等公民**，"
        "而那正是 R63 点名的「按实现分」。补本机那一栏是 `S2` 的活。")),
    ("src/launcher-diagnostics.ts", (
        "②一处「生成 bash/PowerShell 那段让用户自己填」",
        "R63裁定三+K33",
        "它今天**已经是**这一处：模块头注逐字记着 Phase D UX 审计把「诊断」与「别名生成器」"
        "两半合并到同一处的理由（分居两处会让用户看到诊断却无路可循）；`paste-block.ts` 那个"
        "共用待贴组件的规范宿主也是它。⇒ `remote_acct_iso_shellinit` 那块待贴片段"
        "（今天住 `accounts-section.ts`）搬进来，② 就只剩一处。")),
    ("src/settings/mcp-section.ts", (
        "③一处「安装 MCP / skill 等」",
        "R63裁定三+K34",
        "R63 逐字「安装mcp功能\\skill等等」。`cc-bus` 的装口（`deploy_local_cc_bus` / "
        "`cc_bus_install_state`）今天寄在**驾驶舱**里，而驾驶舱 `S6` 已搬出设置、成了顶层"
        "运营视图 ⇒ 「装它」与「用它」今天挤在同一处。收工形状是**装的进这一处、用的留在"
        "各自的运营视图**。")),
])

# 逐组目标 —— **收工判据按这张表判**，不再是「恒等于 1」。
# 每行：组 -> (收工之后该组落点名单**逐字**应当是什么, 为什么)
FRONTEND_GOAL_PER_ITEM: "OrderedDict[str, tuple]" = OrderedDict([
    ("S1", (("src/settings/machine-card.ts",),
            "**已达标，S1 的前端这一半不用做**。四条远端装口今天就住在组件栏里，"
            "而那正是 ① 该在的地方。⚠ 从前那句「目标：空」讲不通：R63 要的是「收成一处」"
            "不是「取消入口」，降到空等于用户再也没有地方部署远端后端，与 `K27`"
            "（部署是产品的一部分，由客户端做）直接冲突。")),
    ("S2", (("src/settings/machine-card.ts", "src/ccm-probe.ts"),
            "「终端集成」那一块（`cc_integration_*`）与本机 ccm 入口查询并进组件栏的"
            "**本机页那一份**；`src/ccm-probe.ts` 留着，它不是入口（见 `FRONTEND_NON_ENTRY`）。"
            "⚠ 一条要一起裁的：`profile_installer` 那四处写盘落点归档在 **②**，而调它们的"
            "`cc_integration_install` 归 **①** ⇒ `§3-3` 说的「② 的行为要改（写盘→只生成）」"
            "落地那一拍，这一行可能要分出一份到 ②。**那是 S2 立件时要回来重裁的**，不是今天。")),
    ("S3", (("src/settings/machine-card.ts",),
            "**没达标，前端这一半要做**：把「一键部署 cc-acct-iso」从「账号」栏的空态里"
            "搬进组件栏。R63 裁定三逐字点名「不许再按『…装账号隔离』这样按实现分」，"
            "而今天它正是一个按实现分出来的独立装口。`R75`〔用@09-14「account 进后端」〕"
            "已把它归 ①。账号栏留一句指路（**不带命令调用** ⇒ 不算落点）。")),
    ("S4", (("src/launcher-diagnostics.ts",),
            "`remote_acct_iso_shellinit` 那块待贴片段从 `accounts-section.ts` 搬到 ② 那一处，"
            "与 `write_account_aliases` 同处。")),
    ("S5", (("src/settings/mcp-section.ts", "src/views/inbox-view.ts"),
            "`cc-bus` 的两条装口从驾驶舱搬进 ③；`src/views/inbox-view.ts` 留着，"
            "它不是装口（见 `FRONTEND_NON_ENTRY`）。")),
])

# 非入口落点 —— **有调用形状、但不是「用户要去装东西的那一处」**。
# 🔴 形状同 `CLAIMS_NON_COMMAND_SYMBOLS`：**明示名单 ＋ 逐条理由**，不许用「反正它不像」。
# 🔴 每一条的理由都要**自己站得住**（不是「不这么划 3 就闭合不了」）——
#    `KR131D1` 逐字警告过「别为了让 3 闭合去硬凑分组」。
FRONTEND_NON_ENTRY: "OrderedDict[str, str]" = OrderedDict([
    ("src/ccm-probe.ts",
     "它是 `probe_ccm_cli` 的**按 origin 的 5 分钟 TTL 缓存**，消费者是 `remote-launch-run.ts` "
     "的 `renderLaunchCommand`（决定这次走 CLI 渲染器还是兜底渲染器）。**用户界面上没有这一处**，"
     "它也不装任何东西。把它搬进设置里的某个分节等于让底层渲染链去 import 一个 UI 模块。"
     "⇒ 它会**永远**留在 `§S5d` 的落点名单里，而永远不该被算成一处安装面入口。"),
    ("src/views/inbox-view.ts",
     "它是收件箱**编辑 overlay** 的保存按钮，调 `write_skill_file`。而 `write_skill_file` "
     "**装不了 skill**：`skill_host::resolve_editable` 第一刀就是 "
     "`requested.canonicalize()`，报错逐字「解析路径失败（**文件必须已存在**）」⇒ 它只能"
     "改一份**已经装好的** skill 的白名单内文件。"
     "🔴 **这说明归档表那一格的依据现打是假的** —— `CAP_ARCHIVE['skill.inbox']` 逐字写着"
     "「写侧（`write_skill_file`）是往用户项目里装东西」。**`K-R131` 不改它**"
     "（`K-R117` 已签收，`[J5]` 不许事后改），把这条退回 PM：订正之后 `skill.inbox` 写侧"
     "落「非装面」⇒ 人群 22→21、`S5` 收 6 条、本表这一行可以撤，全盘目标不变。"),
])

# ── 纪律 A 的闸（`KR128D3`）—— 三份共用文件 ───────────────────────────────────
#
# 纪律 A 逐字：「第二拍全程不改命令名」⇒ 三份共用文件一个字节都不用动 ⇒ 五件写区才真不相交。
# ⚠ **钉的是命令名，不是文件的字节**（件文件 `§0b`/`KR128D3` 逐字）：那三份里还住着
#   别的东西，按字节钉会把无关改动也判红，那种闸三天内就会被人绕过去。
#
# 三份各自怎么判、以及**哪一份判不了**，逐份写死（`brief` 17：判不了就写判不了）：
#
#   · `src/ipc/commands.ts`（包装层）—— **判**。22 条逐条要有包装层入口，
#     而且**两侧都要在**：TS 键 与 `invoke("…")` 的线上串。这一格是**跨语言对拍**
#     （Rust 侧的 `LEDGER` ↔ TS 侧的包装层），两边同源的可能性为零 ⇒ 不是空真。
#
#   · `src-tauri/src/tool_registry.rs` —— **判**。`claims()` 的每一个装 / 卸实现符号，
#     要么解析得到一条真 `#[tauri::command]`，要么在下面 `CLAIMS_NON_COMMAND_SYMBOLS`
#     这张**明示**名单里。⚠ 这一格补的是一个真漏：`§S5` 那段种子表用
#     `if sym in ledger_cmds` 把解析不到的符号**静默丢掉** ⇒ 在 `tool_registry.rs` 里
#     改一个命令名，今天种子表只会**少一条**，一声不响。
#
#   · `src-tauri/src/parity_ledger.rs` —— 🔴 **本尺子判不了，而且是结构性的**：
#     那 22 条命令名**就是从这份文件解析出来的**（`parse_ledger`）⇒ 拿它回头判这份文件
#     恒真，是教科书式的空真（`references/testing.md` 判据硬规则 7）。
#     **它的闸在别处**：`§S5c` 的闭集判定 —— 在 `parity_ledger.rs` 里改掉一个命令名，
#     现打人群就变了，而 `SPLIT_GROUPS` 是**字面量名单**、不会跟着变 ⇒ `R8a` 两向点名当场红。
#     ⇒ 这一份**有闸，只是闸不在本节**。本节逐字印出这句话，别让它读成「三份都判了」。
#
# key = "文件::函数"（与 `claims()` 印出来的住址同形）。
CLAIMS_NON_COMMAND_SYMBOLS = {
    "profile_installer.rs::install_to_profile":
        "② 那一族的**落盘实现**，本来就不是 Tauri 命令（它住 `WRITE_SITES`，"
        "归档在 `SITE_ARCHIVE`）—— `posix-rc-aliases` / `powershell-profile` 两个工具共用它",
    "profile_installer.rs::uninstall_from_profile":
        "同上，摘那一侧",
}

# `src/ipc/commands.ts` 包装层的**形状地板**：整份文件里「键: (」这一形现打有多少条。
# ⚠ 它不是判据，是**反向自检**：形状一变（比如包装层改写成 class 方法），
#   下面那 22 条会齐刷刷判不到 ⇒ 那时该 CRASH（形状坏了），不该印 22 条红。
WRAPPER_KEY_FLOOR = 100

# ══════════════════════════════════════════════════════════════════════════════
# 解析器（全部只读文本；每个都有地板断言）
# ══════════════════════════════════════════════════════════════════════════════

RED: list = []
NOTE: list = []


def red(tag: str, msg: str) -> None:
    RED.append((tag, msg))


def crash(msg: str) -> None:
    print(f"\nCRASH（地板断言没过，本趟所有读数作废）：{msg}", file=sys.stderr)
    sys.exit(3)


def slurp(path: str) -> str:
    with open(path, encoding="utf-8") as fh:
        return fh.read()


def const_block(src: str, head: str) -> str:
    """取一个 `const X: … = &[` 到**列 4 的 `];`** 之间那一段（本仓 rustfmt 的固定形状）。"""
    i = src.index(head)
    j = src.index("\n    ];", i)
    return src[i:j]


def parse_tools(src: str):
    """`TOOLS` 的 `(id, installable, uninstallable)`。

    形状：`id: "x",` 之后最近的一条 `installable: <bool>,` 与 `uninstallable: <bool>,`。
    只在 `pub const TOOLS` 那一段里找 ⇒ `UNMANAGED_ENV` 的 `id:` 不会混进来。
    """
    seg = const_block(src, "pub const TOOLS: &[ToolSpec] = &[")
    out = []
    for m in re.finditer(r'\n        id: "([^"]+)",', seg):
        tail = seg[m.end():]
        inst = re.search(r"\n        installable: (true|false),", tail)
        unin = re.search(r"\n        uninstallable: (true|false),", tail)
        if not inst or not unin:
            crash(f"TOOLS 里 `{m.group(1)}` 解不出 installable/uninstallable")
        out.append((m.group(1), inst.group(1) == "true", unin.group(1) == "true"))
    return out


def parse_unmanaged(src: str):
    """`UNMANAGED_ENV` 的 `(id, who)`。"""
    seg = const_block(src, "pub const UNMANAGED_ENV: &[UnmanagedEnv] = &[")
    out = []
    for m in re.finditer(r'\n        id: "([^"]+)",', seg):
        tail = seg[m.end():]
        who = re.search(r"\n        who: Provisioning::(\w+),", tail)
        out.append((m.group(1), who.group(1) if who else "?"))
    return out


def parse_ledger(src: str):
    """`LEDGER` 的 `(cmd, cap, side)`。形状：三元组字面量，`Side::X` 收尾。"""
    seg = const_block(src, "const LEDGER: &[(&str, &str, Side)] = &[")
    return re.findall(r'"([A-Za-z0-9_]+)"\s*,\s*"([^"]+)"\s*,\s*Side::(\w+)', seg)


def parse_write_sites(src: str):
    """`WRITE_SITES` 的 `(file, fn, Option<tool>)`。形状：`("f", "n", Some("t")|None,`。"""
    seg = const_block(
        src, "pub(crate) const WRITE_SITES: &[(&str, &str, Option<&str>, &str)] = &[")
    out = []
    for m in re.finditer(r'\(\s*"([^"]+\.rs)"\s*,\s*"([A-Za-z0-9_]+)"\s*,\s*'
                         r'(None|Some\("([^"]+)"\))\s*,', seg):
        out.append((m.group(1), m.group(2), m.group(4)))
    return out


def parse_claims(src: str):
    """`tool_registry::claims()` 的 `(tool, install_addr, uninstall_addr)`。

    形状：`Claim { tool: "x", home: …, install: Some(ImplSite { addr: "f::n", … }) … }`。
    取法：按 `Claim {` 切段，段内找 `install:` / `uninstall:` 之后最近的 `addr: "…"`。
    """
    i = src.index("fn claims() -> Vec<Claim> {")
    seg = src[i:src.index("\n    }\n", i)]
    out = []
    parts = seg.split("Claim {")[1:]
    for p in parts:
        tm = re.search(r'tool: "([^"]+)"', p)
        if not tm:
            continue
        addrs = {}
        for field in ("install", "uninstall"):
            fm = re.search(field + r":\s*Some\(", p)
            if fm:
                am = re.search(r'addr: "([^"]+)"', p[fm.end():])
                if am:
                    addrs[field] = am.group(1)
        out.append((tm.group(1), addrs.get("install"), addrs.get("uninstall")))
    # `profile_install()` / `profile_uninstall()` 这两个闭包形 —— 上面那条 regex 看不见它们，
    # 因为那两行写的是 `install: Some(profile_install()),`。补一次：按闭包体里的 addr 取。
    closure = {}
    for name in ("profile_install", "profile_uninstall"):
        cm = re.search(r"let " + name + r" = \|\| \{?\s*ImplSite \{\s*\n?\s*addr: \"([^\"]+)\"",
                       seg)
        if cm:
            closure[name] = cm.group(1)
    fixed = []
    for tool, ins, unins in out:
        if ins is None and re.search(r'tool: "' + re.escape(tool) + r'"[\s\S]{0,400}?'
                                     r"install: Some\(profile_install\(\)\)", seg):
            ins = closure.get("profile_install")
        if unins is None and re.search(r'tool: "' + re.escape(tool) + r'"[\s\S]{0,400}?'
                                       r"uninstall: Some\(profile_uninstall\(\)\)", seg):
            unins = closure.get("profile_uninstall")
        fixed.append((tool, ins, unins))
    return fixed


def scan_tauri_commands(src_root: str):
    """`#[tauri::command]` → `{命令名: "相对路径:行号"}`。**住址这一列只许从这里来。**"""
    attr = "#[tauri::" + "command]"
    out = {}
    for base, _dirs, files in os.walk(src_root):
        for fn in sorted(files):
            if not fn.endswith(".rs"):
                continue
            path = os.path.join(base, fn)
            rel = os.path.relpath(path, src_root).replace("\\", "/")
            lines = slurp(path).split("\n")
            for k, line in enumerate(lines):
                if line.strip() != attr:
                    continue
                for j in range(k + 1, min(k + 12, len(lines))):
                    m = re.search(r"\bfn ([A-Za-z0-9_]+)\s*\(", lines[j])
                    if m:
                        out.setdefault(m.group(1), f"{rel}:{j + 1}")
                        break
    return out


# ══════════════════════════════════════════════════════════════════════════════
# §S1 —— `installable: true`：19 与 7 差在哪
# ══════════════════════════════════════════════════════════════════════════════

def bucket_installable_line(line: str, lineno: int, test_from: int) -> str:
    s = line.strip()
    if "uninstallable: true" in line:
        return "被 `uninstallable: true` 这个**更长的串**顺带命中"
    if s.startswith("//") or s.startswith("///") or s.startswith("//!"):
        return "文档 / 行注释"
    if lineno >= test_from:
        return "测试段（`#[cfg(test)]` 之后）"
    if re.match(r"^installable: true,$", s):
        return "真·字段声明"
    return "生产段里的字符串（报错文案 / 格式串）"


def section_installable(reg_src: str, tools):
    print(head("§S1 `installable: true` —— 19 与 7 差在哪（逐行分档，Σ 必须等于朴素 grep 数）"))
    lines = reg_src.split("\n")
    test_from = next((i + 1 for i, l in enumerate(lines) if l == "#[cfg(test)]"), len(lines) + 1)
    hits = [(i + 1, l) for i, l in enumerate(lines) if "installable: true" in l]
    naive = len(hits)
    occ = reg_src.count("installable: true")
    buckets = Counter()
    detail = []
    for ln, l in hits:
        b = bucket_installable_line(l, ln, test_from)
        buckets[b] += 1
        detail.append((ln, b, l.strip()[:72]))
    print(f"  朴素口径（PM 那条 `grep -c \"installable: true\"`）：**{naive}** 行"
          f"（同串出现 {occ} 次 ⇒ 一行一次，不是某行数了两遍）")
    print(f"  `#[cfg(test)]` 起于第 {test_from} 行")
    print("  分档：")
    for b, n in buckets.most_common():
        print(f"    · {n:2d}  {b}")
    print(f"  Σ = {sum(buckets.values())}")
    real = buckets.get("真·字段声明", 0)
    decl = sum(1 for _, inst, _ in tools if inst)
    print(f"  ⇒ **真·字段声明 {real} 条**；独立那把尺子（解 `TOOLS` 声明表）数出 "
          f"`installable == true` **{decl} 条**")
    print("  逐行（前 6 列宽截断）：")
    for ln, b, txt in detail:
        print(f"    {ln:5d}  {b:<34s} {txt}")
    if sum(buckets.values()) != naive:
        red("R2a", "分档没把朴素口径分完")
    if real != decl:
        red("R2b", f"两把尺子对不上：文本行数 {real} vs 声明表 {decl}")
    return naive, real, decl


# ══════════════════════════════════════════════════════════════════════════════
# 输出小工具
# ══════════════════════════════════════════════════════════════════════════════

def head(t: str) -> str:
    return "\n" + "─" * 78 + f"\n{t}\n" + "─" * 78


# ══════════════════════════════════════════════════════════════════════════════
# `K-R128` 的三节（`§S5c` 分组闭集 · `§S5d` 落点棘轮 · `§S5e` 纪律 A）
# ══════════════════════════════════════════════════════════════════════════════

PASSED: list = []


def ok(msg: str) -> None:
    """记一条**过了**的判定。门禁那一格靠 `len(PASSED)` 认「这一格真的跑了」。

    ⚠ 它只收 `§S5c`/`§S5d`/`§S5e` 三节；`R1`–`R7` 那七条**不进这个数**
    （它们只在红的时候出声，没有逐条的「过了」事件）⇒ 报这个数时分母要这么写。
    """
    PASSED.append(msg)


def collect_ts(root: str):
    """`src/**.ts` 的分母（`§S5b` 与 `§S5c`–`§S5e` **共用这一份**，别各写一遍）。

    排除 `*.vitest.ts` 与 `src/generated/`。返回 `{相对路径: 正文}`，
    遍历次序 = `os.walk` ＋ 每层 `sorted(files)`（定序，别让读数随文件系统漂）。
    """
    out = OrderedDict()
    ts_root = os.path.join(root, "src")
    if not os.path.isdir(ts_root):
        return out
    for base, _d, files in os.walk(ts_root):
        if os.sep + "generated" in base:
            continue
        for fn in sorted(files):
            if fn.endswith(".ts") and not fn.endswith(".vitest.ts"):
                p = os.path.join(base, fn)
                out[os.path.relpath(p, root).replace("\\", "/")] = slurp(p)
    return out


def frontend_landing(texts, cmds, wrapper: str):
    """把 `src/**.ts` 里对这几条命令的出现分成**两档**：调用点 · 只提到没调用。

    落点（第一档）= 有**调用形状**的那一份文件。调用形状逐字只认一种：`.<命令>(`
    —— 本仓前端一律经包装层调（`commands.<名>({…})`，含换行的链式 `.<名>({…})`）。

    🔴 **为什么第二档非分不可**（本件最要防的形状）：只在注释 / 散文 / 字符串里写了
    命令名的那一份**不是用户入口**。把它算成落点，这把尺子就**可以靠删一条注释变绿** ——
    而「落点数收到 1」正是 S1–S5 的收工判据 ⇒ 那等于给第三块发了一条假出口。
    现打就有两份是这一形（`src/accounts.ts` · `src/settings/panel.ts`，逐处印在下面）。

    ⚠ **买不到什么**：别的调用形状（`invoke("<名>")` 直呼 · 先解构再裸调）本函数**看不见**。
      为了让它**不静默**，第二档逐处 `文件:行号` 印出来：哪天有人换了调用形状，
      那一份会从「落点」掉进「只提到」那一档，**在读数面上当场可见**（不是判红，是点名）。

    ⚠ `wrapper`（`src/ipc/commands.ts`）**不算落点** —— 22 条逐条都在它里面，
      它是共用包装层，不是「散在各处的用户入口」。它另有 `§S5e` 专门判。

    返回 `(calls, mentions)`：
      `calls`    `{命令 -> (相对路径, …)}`（定序）
      `mentions` `[(相对路径, 行号, 命令), …]`（定序）—— 只提到没调用的那一档
    """
    calls, mentions = OrderedDict(), []
    for cmd in cmds:
        call_re = re.compile(r"\." + re.escape(cmd) + r"\s*\(")
        word_re = re.compile(r"\b" + re.escape(cmd) + r"\b")
        hit = []
        for rel, text in texts.items():
            if rel == wrapper:
                continue
            if call_re.search(text):
                hit.append(rel)
            elif word_re.search(text):
                for i, ln in enumerate(text.split("\n"), 1):
                    if word_re.search(ln):
                        mentions.append((rel, i, cmd))
        calls[cmd] = tuple(sorted(hit))
    return calls, sorted(mentions)


def wrapper_entries(text: str, cmds):
    """包装层两侧：TS 键（`  <名>: (`）与线上串（`invoke…("<名>"`）各命中几次。

    形状取自本仓今天 `src/ipc/commands.ts` 的固定写法，逐字两例：
      `  probe_ccm_cli: (args: { origin: string }) => invoke<CcmProbeResult>("probe_ccm_cli", args),`
      `  write_skill_file: (args: {…}) =>` ＋ 下一行 `    invoke<void>("write_skill_file", args),`
    ⇒ 键与串**允许不在同一行**，两侧分别数。
    ⚠ 形状一变（改写成 class 方法 / 改用别的桥）会让 22 条齐刷刷判不到 ⇒
      由 `WRAPPER_KEY_FLOOR` 那条反向自检先 CRASH，不许印 22 条红。
    """
    out = OrderedDict()
    for cmd in cmds:
        key = len(re.findall(r"^ +" + re.escape(cmd) + r"\s*:\s*\(", text, re.M))
        wire = len(re.findall(r"invoke[^(\n]*\(\s*\"" + re.escape(cmd) + r"\"", text))
        out[cmd] = (key, wire)
    return out


def section_split_closure(cmds_now):
    """`§S5c`（`KR128D1`）：`S1..S5 → 命令名集合` 的分组表 ＋ **闭集判定**。

    判两样：`R8a` 并集**逐字等于**现打人群（两向点名）· `R8b` 五组两两**交集为空**。
    这一条就是在防「切件方案与量具各说各话」。
    """
    print(head("§S5c 切件分组表 S1–S5 ＋ 闭集判定〔`K-R128` `KR128D1`〕"))
    print("  ⚠ 分组表是**人的判断**（出处 `features/K-R117-安装面收成三处.md#§3-3` 那五行），"
          "人群是**现打派生**（`LEDGER` ＋ `CAP_ARCHIVE` ⇒ 就是 `§S5` 那张表）。"
          "下面判的是这两侧**对不对得上**。")
    union, dupes = [], OrderedDict()
    for g, (label, members, why) in SPLIT_GROUPS.items():
        print(f"  {g} {label:<16s}{len(members)} 条：{', '.join(sorted(members))}")
        print(f"      依据：{why}")
        for m in members:
            dupes.setdefault(m, []).append(g)
        union.extend(members)
    now = set(cmds_now)
    uni = set(union)
    print(f"  ⇒ 五组并集 **{len(uni)} 条**（逐条出现 {len(union)} 次）· "
          f"现打人群 **{len(now)} 条**（分母 = `§S5` 归处非「{NA}」的那几行）")

    only_table = sorted(uni - now)
    only_now = sorted(now - uni)
    if only_table:
        red("R8a", "分组表里这些命令**今天的人群里找不到**（`§S5b` 那 22 条里没有它）："
                   + ", ".join(only_table)
                   + " —— ⚠ **别改表去凑**：要么是切件方案指了一条盘上不存在的命令，"
                     "要么是有人改了命令名（纪律 A 被破了）。两种都要人回来裁")
    if only_now:
        red("R8a", "现打人群里这些命令**五件谁都没认领**：" + ", ".join(only_now)
                   + " —— 新长出一条安装面命令而切件方案没覆盖它，"
                     "或者有人改了命令名 ⇒ 第三块的写区当场不完整")
    if not only_table and not only_now:
        ok(f"R8a 分组表并集逐字等于现打人群（{len(uni)} 条，两向差集都空）")
        print(f"  ✓ R8a 并集逐字相等，两向差集都是空的（表→人群 0 · 人群→表 0）")

    overlap = {m: gs for m, gs in dupes.items() if len(gs) > 1}
    if overlap:
        for m in sorted(overlap):
            red("R8b", f"命令 `{m}` 同时被 {', '.join(overlap[m])} 认领 —— "
                       f"五件的写区就是这么撞上的")
    else:
        ok(f"R8b 五组两两交集为空（{len(SPLIT_GROUPS)} 组 / {len(union)} 条名额）")
        print(f"  ✓ R8b 两两交集为空（{len(union)} 条名额没有一条被两件认领）")
    for m in sorted(uni & now):
        ok(f"R8 命令 `{m}` 恰好归入一组")


def section_frontend_ratchet(texts, cmds_now, wrapper: str):
    """`§S5d`（`KR128D2`）：前端落点**棘轮** —— 钉名单、判逐字相等、份数现算。

    `R9` 每组现打落点名单 **==** `FRONTEND_PIN` 钉住的名单。
    不等就红，并点名**是哪一组、哪一份文件、往哪个方向变的**。
    """
    print(head("§S5d 前端落点棘轮〔`K-R128` `KR128D2`〕"))
    if len(texts) < 30:
        print(f"  （`src` 下只有 {len(texts)} 份 .ts —— 本节跳过，"
              f"多半是 `--root` 指到了只有 src-tauri 的夹具）")
        return
    calls, mentions = frontend_landing(texts, cmds_now, wrapper)
    print("  ⚠ **落点 = 有调用形状 `.<命令>(` 的那一份 `.ts`**；只在注释 / 散文 / 字符串里"
          "提到命令名的**不算**（第二档单列）。⇒ 这把尺子**不能靠删一条注释变绿**。")
    print("  ⚠ 判的是**名单逐字相等**（不是 `<=`，也不是只比份数）—— 份数一律 `len()` 现算。")
    print(f"  {'组':<5s}{'现打':<5s}{'钉住':<5s}{'目标':<5s}名单（现打）")
    total_now = set()
    for g, (label, members, _why) in SPLIT_GROUPS.items():
        pin, when, whoi = FRONTEND_PIN[g]
        now = tuple(sorted({f for m in members for f in calls.get(m, ())}))
        total_now |= set(now)
        want = tuple(sorted(pin))
        goal, goal_why = FRONTEND_GOAL_PER_ITEM[g]
        print(f"  {g:<5s}{len(now):<5d}{len(want):<5d}{len(goal):<5d}"
              f"{', '.join(now) if now else '（零）'}")
        print(f"        钉于：{when}")
        print(f"        该降的时候谁来改：{whoi}")
        # 🔴 目标**逐字印名单**，份数 `len()` 现算 —— 表里一个基数字面量都没有（`brief` 13b）。
        print(f"        目标（`FRONTEND_GOAL_PER_ITEM['{g}']`，唯一住址）："
              f"{', '.join(sorted(goal))}")
        print(f"        为什么：{goal_why}")
        if now == want:
            ok(f"R9 {g} 落点名单逐字相等（{len(now)} 份）")
            continue
        grew = sorted(set(now) - set(want))
        shrank = sorted(set(want) - set(now))
        if grew:
            red("R9a", f"{g}（{label}）**多出落点**：{', '.join(grew)} —— "
                       f"钉住的是 {', '.join(want) if want else '（零）'}。"
                       f"有人往上加了一份前端落点 ⇒ 第三块那一件的写区变大了，"
                       f"**不许靠把表改大让它绿**（`§0c`）")
        if shrank:
            red("R9b", f"{g}（{label}）**少了落点**：{', '.join(shrank)} —— "
                       f"钉住的是 {', '.join(want)}，现打 "
                       f"{', '.join(now) if now else '（零）'}。"
                       f"要么这一件真收干净了（那就**同一拍**把 `FRONTEND_PIN['{g}']` 降下来），"
                       f"要么那一处是被别的改动顺手带没的 ⇒ 回来裁")
    print(f"  ⇒ 全盘并集 **{len(total_now)} 份**（目标 {len(FRONTEND_GOAL_OVERALL)} 处，"
          f"名单住 `FRONTEND_GOAL_OVERALL`，判定在 `§S5f`）："
          f"{', '.join(sorted(total_now))}")
    print(f"  ── 第二档：**只提到、没有调用形状** 现打 {len(mentions)} 处 "
          f"（只出读数、不判红；它在这里是为了让「换了调用形状」不静默）──")
    for rel, ln, cmd in mentions:
        print(f"    {rel}:{ln}  提到 `{cmd}`")


def section_frontend_goal_closure():
    """`§S5f`（`KR131D3`）：收工目标的**算术闭合** —— 五组目标的并集 == 全盘目标那张名单。

    这一条治的是 `K-R131` 逮到的那个形状：目标从前是**五组共用的一个标量**
    （`FRONTEND_GOAL_PER_GROUP = 1`），于是「每组 1 份 × 5 组 = 5 个名额」与
    「全盘目标 3 份」在同一份输出里对不上，**而盘上没有任何一处说那 3 份是哪 3 份**。
    ⇒ 现在目标是**三张名单**，这一节把它们之间的算术关系钉死：

      `R11a` （五组目标的并集 － `FRONTEND_NON_ENTRY`）**逐字等于** `FRONTEND_GOAL_OVERALL`
             的键集（**两向点名**）。不闭合当场红。
      `R11b` `FRONTEND_NON_ENTRY` 每一条都**真的出现在**某一组的目标里 ——
             防「留一条永远不会红的豁免」（同 `LEDGER` ⑥ 那条「不留永远不会红的钉子」）。
      `R11c` `FRONTEND_NON_ENTRY` 与 `FRONTEND_GOAL_OVERALL` **不许相交** ——
             一份文件不能既是「那三处之一」又是「不是入口」。
      `R11d` `FRONTEND_GOAL_PER_ITEM` 的键集 == `SPLIT_GROUPS` 的键集（两向）——
             新切出一组而没给它目标，或给一个不存在的组写目标，都当场红。

    ⚠ **诚实边界（`references/testing.md` 判据硬规则 10）：这一节只读本文件自己的三张表，
      一行被测树都不读。** ⇒ 它**够不着**「目标定得对不对」，也**不会**因为产品代码变了而红；
      它买到的只有一样：**这四条算术关系没人能悄悄写歪**。
      「目标定得对不对」是人的判断，依据逐条写在表的第三栏里（同 `CAP_ARCHIVE` 的 `B1` 口径）。
    ⚠ 也因为它不读树，它**不是空真**：分母是三张非空表，`R11b`/`R11d` 各自的地板就是
      「表非空」，空了下面 `len()` 对拍立刻两向点名。
    """
    print(head("§S5f 收工目标的算术闭合〔`K-R131` `KR131D3`〕"))
    print("  🔴 **目标只许有一处住址** —— 就是这三张表。`FRONTEND_PIN` 第三栏只写"
          "「谁来改」，不写目标；散文里再出现一份目标就是 `K-R131` 治的那个病本身。")
    print(f"  全盘目标（`FRONTEND_GOAL_OVERALL`，出处 `DECISIONS.md#R63` 裁定三）"
          f"**{len(FRONTEND_GOAL_OVERALL)} 处**：")
    for f, (place, why_id, why) in FRONTEND_GOAL_OVERALL.items():
        print(f"    {place:<34s}{f}")
        print(f"        依据：{why_id} —— {why}")
    print(f"  非入口落点（`FRONTEND_NON_ENTRY`）**{len(FRONTEND_NON_ENTRY)} 份** —— "
          f"有调用形状、但不是用户要去装东西的那一处：")
    for f, why in FRONTEND_NON_ENTRY.items():
        print(f"    {f}\n        理由：{why}")

    # ── R11d：逐组目标表与分组表**同一批组** ────────────────────────────────
    gi = set(FRONTEND_GOAL_PER_ITEM)
    gs = set(SPLIT_GROUPS)
    only_goal, only_split = sorted(gi - gs), sorted(gs - gi)
    if only_goal:
        red("R11d", f"`FRONTEND_GOAL_PER_ITEM` 里这几组**不在 `SPLIT_GROUPS` 里**："
                    f"{', '.join(only_goal)} —— 给一个不存在的组写了目标")
    if only_split:
        red("R11d", f"`SPLIT_GROUPS` 里这几组**没有目标**：{', '.join(only_split)} —— "
                    f"新切出一组就要同拍给它一行目标，否则它的收工判据是空的")
    if not only_goal and not only_split:
        ok(f"R11d 逐组目标表与分组表同一批组（{len(gi)} 组，两向差集都空）")
        print(f"  ✓ R11d 两表组名逐字相等（{len(gi)} 组，目标→分组 0 · 分组→目标 0）")

    # ── R11c：豁免与全盘目标不许相交 ──────────────────────────────────────
    both = sorted(set(FRONTEND_NON_ENTRY) & set(FRONTEND_GOAL_OVERALL))
    if both:
        red("R11c", f"这几份**既在全盘目标里、又被划成非入口**：{', '.join(both)} —— "
                    f"一份文件不能同时是「那几处之一」和「不是入口」")
    else:
        ok("R11c 非入口名单与全盘目标名单不相交")
        print("  ✓ R11c 两张名单交集为空")

    # ── R11b：豁免不许是死钉 ──────────────────────────────────────────────
    union = set()
    for g, (goal, _why) in FRONTEND_GOAL_PER_ITEM.items():
        union |= set(goal)
    dead = sorted(set(FRONTEND_NON_ENTRY) - union)
    if dead:
        red("R11b", f"这几条豁免**没有任何一组的目标用到**：{', '.join(dead)} —— "
                    f"一条永远不会被触发的豁免只会让下一个人以为这里判过了")
    else:
        ok(f"R11b 非入口名单每一条都真的出现在某一组的目标里（{len(FRONTEND_NON_ENTRY)} 条）")
        print(f"  ✓ R11b {len(FRONTEND_NON_ENTRY)} 条豁免逐条有落点")

    # ── R11a：算术闭合（本节的正题）────────────────────────────────────────
    lhs = union - set(FRONTEND_NON_ENTRY)
    rhs = set(FRONTEND_GOAL_OVERALL)
    print(f"  ⇒ 五组目标并集 **{len(union)} 份** － 非入口 **{len(FRONTEND_NON_ENTRY)} 份** "
          f"= **{len(lhs)} 处**；全盘目标 **{len(rhs)} 处**")
    only_items = sorted(lhs - rhs)
    only_all = sorted(rhs - lhs)
    if only_items:
        red("R11a", f"这几份**是某一组的目标、却不在全盘目标名单里**：{', '.join(only_items)} "
                    f"—— 算术不闭合：某一件打算收到一处新地方，而「全盘一共收成几处」"
                    f"那张名单没跟着改。⚠ **两边都要人来裁**，别顺手改一侧让它绿")
    if only_all:
        red("R11a", f"全盘目标里这几处**没有任何一组认领**：{', '.join(only_all)} —— "
                    f"名单上写着要收到这里，而五件谁都没把它当目标 ⇒ 这一处收工那天不会有人去做")
    if not only_items and not only_all:
        ok(f"R11a 五组目标的并集（扣掉非入口）逐字等于全盘目标名单（{len(rhs)} 处，两向差集都空）")
        print(f"  ✓ R11a 算术闭合（组→全盘 0 · 全盘→组 0）")
    for f in sorted(lhs & rhs):
        ok(f"R11 目标文件 `{f}` 两侧都在")


def section_discipline_a(texts, cmds_now, claims, cmd_addr, wrapper: str):
    """`§S5e`（`KR128D3`）：**纪律 A 的闸** —— 三份共用文件的命令名。

    判两份、明说第三份判不了（逐份理由见 `CLAIMS_NON_COMMAND_SYMBOLS` 上面那段头注）：
      `R10a` `src/ipc/commands.ts`：22 条逐条要有包装层入口，TS 键与线上串**两侧都在**。
      `R10b` `tool_registry.rs`：`claims()` 每个装 / 卸符号要么是真 `#[tauri::command]`，
             要么在明示的非命令名单里。
      `parity_ledger.rs`：**判不了（空真）**，它的闸在 `§S5c`。

    🔴 **钉的是命令名，不是文件字节** —— 往这三份里加一条**与这 22 条无关**的新命令
       **不许红**（否则以后没人敢动这三份文件）。本节每一条判定都只在这 22 个名字上取值。
    """
    print(head("§S5e 纪律 A 的闸 —— 三份共用文件的命令名〔`K-R128` `KR128D3`〕"))
    print("  纪律 A 逐字：「第二拍全程不改命令名 ⇒ 三份共用文件一字不动 ⇒ 五件写区才真不相交」。"
          "S1 / S5 是方案里唯一许并跑的一对 ⇒ 这条纪律破了，两件当场撞车。")
    print("  🔴 判的是**命令名集合**，不是文件字节：往这三份里加一条与这 22 条无关的新命令"
          "**不红**（按字节钉的闸三天内就会被人绕过去）。")

    # ① `src/ipc/commands.ts`
    if len(texts) < 30:
        print(f"\n  ① `{wrapper}` —— 本档**跳过**（`src` 下只有 {len(texts)} 份 .ts，"
              f"多半是 `--root` 指到了只有 src-tauri 的夹具）。"
              f"⚠ 跳过**不是过了**：这一趟这一档一条读数都没有。")
    else:
        wtext = texts.get(wrapper)
        if wtext is None:
            crash(f"`{wrapper}` 不在 `src/**.ts` 的分母里 —— 包装层没了，`§S5e` ① 的分母塌了")
        floor = len(re.findall(r"^ +[A-Za-z_][A-Za-z0-9_]*\s*:\s*\(", wtext, re.M))
        print(f"\n  ① `{wrapper}`（包装层）—— 形状地板：整份现打 {floor} 条「键: (」"
              f"（地板 {WRAPPER_KEY_FLOOR}）")
        if floor < WRAPPER_KEY_FLOOR:
            crash(f"`{wrapper}` 里「键: (」现打 {floor} < 地板 {WRAPPER_KEY_FLOOR} —— "
                  f"包装层的写法变了，本档那 {len(cmds_now)} 条会齐刷刷判不到"
                  f"（那是形状坏了，不是 {len(cmds_now)} 条红）")
        ent = wrapper_entries(wtext, cmds_now)
        bad = [(c, k, w) for c, (k, w) in ent.items() if k != 1 or w != 1]
        for c, k, w in sorted(bad):
            red("R10a", f"`{c}` 在 `{wrapper}` 里的包装层入口不完整：TS 键命中 {k} 次 · "
                        f"`invoke(\"…\")` 线上串命中 {w} 次（各应当恰好 1 次）—— "
                        f"{'命令名被改过' if (k == 0 or w == 0) else '同名入口出现了不止一处'}，"
                        f"**纪律 A 破了**")
        for c in sorted(c for c, (k, w) in ent.items() if k == 1 and w == 1):
            ok(f"R10a `{c}` 包装层两侧都在")
        print(f"     {len(cmds_now)} 条里两侧都在的 **{len(ent) - len(bad)} 条**"
              f"（分母 = `§S5` 现打人群 {len(cmds_now)} 条；两侧 = TS 键 ＋ `invoke` 线上串）")

    # ② `src-tauri/src/tool_registry.rs`
    addrs = OrderedDict()
    for tool, ins, unins in claims:
        for a in (ins, unins):
            if a and "::" in a:
                addrs.setdefault(a, []).append(tool)
    print(f"\n  ② `src-tauri/src/tool_registry.rs` —— `claims()` 现打 {len(addrs)} 个"
          f"不同的装 / 卸实现符号：")
    for a in sorted(addrs):
        sym = a.split("::")[-1]
        if sym in cmd_addr:
            mark = f"→ 真 `#[tauri::command]`（住 {cmd_addr[sym]}）"
            ok(f"R10b `{a}` 解析到真 `#[tauri::command]`")
        elif a in CLAIMS_NON_COMMAND_SYMBOLS:
            mark = f"→ 明示的非命令实现：{CLAIMS_NON_COMMAND_SYMBOLS[a]}"
            ok(f"R10b `{a}` 在明示的非命令名单里")
        else:
            mark = "→ ✗ 两头落空"
            red("R10b", f"`claims()` 里的装 / 卸符号 `{a}` **既不是一条真 "
                        f"`#[tauri::command]`，也不在 `CLAIMS_NON_COMMAND_SYMBOLS` 里** —— "
                        f"多半是有人在 `tool_registry.rs` 里改了命令名（纪律 A 破了）。"
                        f"⚠ 这一格补的正是 `§S5` 那张种子表的静默漏：它用 "
                        f"`if sym in ledger_cmds` 把解析不到的符号直接丢掉，只会少一条、不出声")
        print(f"     {a:<52s}{mark}")

    # ③ `src-tauri/src/parity_ledger.rs`
    print(f"\n  ③ `src-tauri/src/parity_ledger.rs` —— 🔴 **本节判不了，而且是结构性的**：")
    print(f"     那 {len(cmds_now)} 条命令名**就是从这份文件解析出来的**（`parse_ledger`）"
          f"⇒ 拿它回头判这份文件恒真，是空真。")
    print(f"     **它的闸在 `§S5c`**：在这份文件里改掉一个命令名 ⇒ 现打人群变，"
          f"而 `SPLIT_GROUPS` 是字面量名单、不会跟着变 ⇒ `R8a` 两向点名当场红。")
    print(f"     ⇒ 这一份**有闸，只是闸不在本节** —— 别把本节读成「三份都判了」。")


def main() -> int:
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=os.path.dirname(here),
                    help="被测树的根（默认 = 本文件所在目录的父目录）")
    args = ap.parse_args()
    root = os.path.abspath(args.root)

    src_dir = os.path.join(root, "src-tauri", "src")
    if not os.path.isdir(src_dir):
        crash(f"`{src_dir}` 不存在 —— `--root` 指错树了")
    try:
        sha = subprocess.run(["git", "-C", root, "rev-parse", "--short", "HEAD"],
                             capture_output=True, text=True, timeout=20).stdout.strip()
    except Exception:
        sha = ""
    print(f"量具：{os.path.abspath(__file__)}")
    print(f"被测树：{root}" + (f"（HEAD {sha}）" if sha else "（不是 git 树 / 取不到 HEAD）"))

    reg_src = slurp(os.path.join(src_dir, "tool_registry.rs"))
    led_src = slurp(os.path.join(src_dir, "parity_ledger.rs"))
    wsr_src = slurp(os.path.join(src_dir, "write_site_registry.rs"))

    tools = parse_tools(reg_src)
    unmanaged = parse_unmanaged(reg_src)
    ledger = parse_ledger(led_src)
    sites = parse_write_sites(wsr_src)
    claims = parse_claims(reg_src)
    cmd_addr = scan_tauri_commands(src_dir)

    # ── R1 地板（反向自检）：扫不到东西一律 CRASH，不许当「零违例」 ──────────────
    floors = [("TOOLS", len(tools), 5), ("UNMANAGED_ENV", len(unmanaged), 5),
              ("LEDGER", len(ledger), 100), ("WRITE_SITES", len(sites), 20),
              ("claims()", len(claims), 5), ("#[tauri::command]", len(cmd_addr), 100)]
    print(head("§S0 地板（反向自检 —— 这几个数塌了，下面每一条都会空真地绿）"))
    for name, got, floor in floors:
        print(f"  {name:<20s} 现打 {got:4d}  地板 {floor}")
        if got < floor:
            crash(f"{name} 现打 {got} < 地板 {floor}")

    section_installable(reg_src, tools)

    # ── §S2 声明侧 ────────────────────────────────────────────────────────────
    print(head("§S2 声明侧 —— `TOOLS` ＋ `UNMANAGED_ENV`"))
    print(f"  `TOOLS` {len(tools)} 条：")
    for tid, inst, unin in tools:
        print(f"    {tid:<22s} installable={str(inst):<5s} uninstallable={unin}")
    print(f"  `UNMANAGED_ENV` {len(unmanaged)} 条，按 `who` 分："
          f"{dict(Counter(w for _, w in unmanaged))}")
    for uid, who in unmanaged:
        if who == "AppShips":
            print(f"    · `{uid}` = `Provisioning::AppShips`"
                  f"（app 独有、该我们装；有没有装口由 `EnvTier::of` 另算）")

    print(f"  `claims()` {len(claims)} 条（工具 → 装 / 卸实现住址）：")
    for tool, ins, unins in claims:
        print(f"    {tool:<22s} install={ins}  uninstall={unins}")

    # ── §S3 动作侧 ────────────────────────────────────────────────────────────
    caps = OrderedDict()
    for cmd, cap, side in ledger:
        caps.setdefault(cap, []).append((cmd, side))
    print(head("§S3 动作侧 —— `LEDGER`"))
    print(f"  {len(ledger)} 行命令 / {len(caps)} 个能力 id")
    print(f"  `Side` 分布：{dict(Counter(s for _, _, s in ledger))}")

    # ── §S4 写盘落点 ──────────────────────────────────────────────────────────
    with_tool = [(f, n, t) for f, n, t in sites if t]
    print(head("§S4 写盘落点 —— `WRITE_SITES`"))
    print(f"  {len(sites)} 行，其中**带 tool id** 的 {len(with_tool)} 行：")
    tool_ids = {t for t, _, _ in tools}
    for f, n, t in with_tool:
        flag = "" if t in tool_ids else "   ⚠ 这个 id 不在 TOOLS 里"
        print(f"    {f}::{n}  → `{t}`{flag}")
    # claims() 与 WRITE_SITES 对同一处的归属说法一致吗（两张表都是源码里的权威源）
    claim_by_addr = {}
    for tool, ins, unins in claims:
        for a in (ins, unins):
            if a:
                claim_by_addr.setdefault(a, set()).add(tool)
    print("  ⚠ 两张表对同一处的归属 —— 对不上的逐处点名（本仓今天**没有**判据在对拍这一格）：")
    conflicts = 0
    for f, n, t in with_tool:
        addr = f"{f}::{n}"
        if addr in claim_by_addr and t not in claim_by_addr[addr]:
            conflicts += 1
            print(f"    · `{addr}`：`WRITE_SITES` 说它是 `{t}` 的安装动作，"
                  f"而 `claims()` 说这个符号是 {sorted(claim_by_addr[addr])} 的实现")
    print(f"    （现打 {conflicts} 处对不上）")

    # ── §S5 安装面人群 ＋ 归档（R3 / R4 / R5） ─────────────────────────────────
    print(head("§S5 安装面人群 ＋ 归档"))

    # 种子：从 `claims()` 派生 —— 这几条是「盘上确凿的装 / 卸实现」
    ledger_cmds = {c: cap for c, cap, _ in ledger}
    seed_caps, seed_rows = set(), []
    for tool, ins, unins in claims:
        for a in (ins, unins):
            if not a or "::" not in a:
                continue
            sym = a.split("::")[-1]
            if sym in ledger_cmds:
                seed_caps.add(ledger_cmds[sym])
                seed_rows.append((tool, a, ledger_cmds[sym]))
    print(f"  种子（`claims()` 的装 / 卸实现符号里，同时是 Tauri 命令的）{len(seed_rows)} 条"
          f" ⇒ {len(seed_caps)} 个能力 id：")
    for tool, a, cap in sorted(seed_rows):
        print(f"    `{a}`  →  能力 `{cap}`（工具 `{tool}`）")

    # ═══ DV-ARCHIVE-GATES-BEGIN ═══（死值验刀③「表整段拿掉」摘的就是这两块之间的东西）
    # R3 两向对拍
    missing = [c for c in caps if c not in CAP_ARCHIVE]
    stale = [c for c in CAP_ARCHIVE if c not in caps]
    if missing:
        red("R3a", "这些能力 id 在 `LEDGER` 里而**没人归档**（新长出来的装口就从这里红）："
                   + ", ".join(sorted(missing)))
    if stale:
        red("R3b", "归档表里这些能力 id 今天不在 `LEDGER` 里（表在腐烂）："
                   + ", ".join(sorted(stale)))
    bad_override = [c for c in CMD_OVERRIDE if c not in ledger_cmds]
    if bad_override:
        red("R3c", "`CMD_OVERRIDE` 里这些命令名不在 `LEDGER` 里：" + ", ".join(bad_override))

    # R4 种子不许被划出安装面
    for cap in sorted(seed_caps):
        got = CAP_ARCHIVE.get(cap, (None,))[0]
        if got not in (B1, B2, B3):
            red("R4", f"能力 `{cap}` 有一条**盘上确凿的装 / 卸实现**（见种子表），"
                      f"却被归成 `{got}`")

    # ═══ DV-ARCHIVE-GATES-END ═══

    # 逐条落哪一处（21+ 的那张表，住址从源码派生）
    rows = []
    for cmd, cap, side in ledger:
        b, why, note = CMD_OVERRIDE.get(cmd, CAP_ARCHIVE.get(cap, (NA, "?", "?")))
        if b == NA:
            continue
        addr = cmd_addr.get(cmd)
        if addr is None:
            red("R6", f"命令 `{cmd}` 在 `LEDGER` 里，却在 `#[tauri::command]` 面上找不到")
            addr = "（找不到）"
        rows.append((b, cap, cmd, side, addr, why))
    rows.sort(key=lambda r: (BUCKETS.index(r[0]), r[1], r[2]))
    print("\n  ── Tauri 命令这一半（逐条：今天住哪 · 归哪一处 · 依据）──")
    print(f"  {'归处':<14s}{'能力 id':<24s}{'命令':<32s}{'Side':<8s}{'今天住哪':<30s}依据")
    for b, cap, cmd, side, addr, why in rows:
        print(f"  {b:<14s}{cap:<24s}{cmd:<32s}{side:<8s}{addr:<30s}{why}")
    bycmd = Counter(r[0] for r in rows)
    print(f"  合计 **{len(rows)} 条**："
          + " · ".join(f"{b} {bycmd.get(b, 0)}" for b in (B1, B2, B3)))
    cap_set = sorted({r[1] for r in rows})
    file_set = sorted({r[4].split(":")[0] for r in rows})
    print(f"  跨 **{len(cap_set)} 个能力 id** / **{len(file_set)} 份后端文件**")
    print(f"    能力 id：{', '.join(cap_set)}")
    print(f"    文件：{', '.join(file_set)}")

    # ═══ DV-ARCHIVE-GATES-2-BEGIN ═══
    # R5 写盘落点两向对拍
    site_keys = {f"{f}::{n}" for f, n, _ in with_tool} | set(SITE_TOOLID_NONE_BUT_ARCHIVED)
    s_missing = sorted(site_keys - set(SITE_ARCHIVE))
    s_stale = sorted(set(SITE_ARCHIVE) - site_keys)
    if s_missing:
        red("R5a", "这些写盘落点申报成了某个工具的安装动作，而**没人归档**：" + ", ".join(s_missing))
    if s_stale:
        red("R5b", "归档表里这些写盘落点今天不在人群里：" + ", ".join(s_stale))
    # ═══ DV-ARCHIVE-GATES-2-END ═══
    all_sites = {f"{f}::{n}": t for f, n, t in sites}
    print("\n  ── 非 Tauri 命令的内部动作这一半 ──")
    print(f"  {'归处':<14s}{'落点':<50s}{'WRITE_SITES 的 tool id':<26s}依据")
    for k in sorted(SITE_ARCHIVE, key=lambda k: (BUCKETS.index(SITE_ARCHIVE[k][0]), k)):
        b, why, _note = SITE_ARCHIVE[k]
        tid = all_sites.get(k, "（不在 WRITE_SITES 里）")
        print(f"  {b:<14s}{k:<50s}{str(tid):<26s}{why}")
    bysite = Counter(v[0] for v in SITE_ARCHIVE.values())
    print(f"  合计 **{len(SITE_ARCHIVE)} 处**："
          + " · ".join(f"{b} {bysite.get(b, 0)}" for b in (B1, B2, B3)))

    print(f"\n  ⇒ **人群合计 {len(rows)} 条命令 ＋ {len(SITE_ARCHIVE)} 处内部动作"
          f" = {len(rows) + len(SITE_ARCHIVE)} 项**（现打；分母 = `LEDGER` {len(ledger)} 行 ＋"
          f" `WRITE_SITES` {len(sites)} 行）")

    # ── §S5b 前端落点（切件的写区要按这个切，别手写） ─────────────────────────
    print(head("§S5b 前端落点 —— 每条安装面命令在 `src/**.ts` 里被谁引用"))
    print("  ⚠ 分母 = `<root>/src` 下的 `.ts`，**排除** `*.vitest.ts` 与 `src/generated/`；"
          "`src/ipc/commands.ts` 是**共用包装层**（每条都在它里面），单列不重复印。")
    ts_root = os.path.join(root, "src")
    # 〔`K-R128` 09-15〕这一段目录遍历抽成了 `collect_ts()` —— **本节输出逐字不变**，
    # 改它只为一件事：让 `§S5c`–`§S5e` 与本节**共用同一份分母**。两处各写一遍 `os.walk`
    # 正是本文件头注点名的「同一件事的第二份表示，而它们会各自漂」。
    texts = collect_ts(root)
    ts_files = list(texts)
    wrapper = "src/ipc/commands.ts"
    if len(ts_files) < 30:
        print(f"  （`{ts_root}` 下只有 {len(ts_files)} 份 .ts —— 本节跳过，"
              f"多半是 `--root` 指到了只有 src-tauri 的夹具）")
    else:
        by_file = {}
        for _b, _cap, cmd, _s, _a, _w in rows:
            hits = [rel for rel, t in texts.items()
                    if rel != wrapper and re.search(r"\b" + re.escape(cmd) + r"\b", t)]
            for h in hits:
                by_file.setdefault(h, []).append(cmd)
            in_wrapper = re.search(r"\b" + re.escape(cmd) + r"\b", texts.get(wrapper, ""))
            print(f"    {cmd:<32s}{'包装层✓' if in_wrapper else '包装层✗'}  "
                  f"{', '.join(hits) if hits else '（前端没有非测试引用）'}")
        print(f"  ⇒ 前端落点 **{len(by_file)} 份文件**（不含共用包装层 `{wrapper}`）：")
        for f in sorted(by_file):
            print(f"    {f:<40s} {len(by_file[f]):2d} 条：{', '.join(sorted(by_file[f]))}")

    # ── `K-R128` 三节：切件闭集 · 落点棘轮 · 纪律 A ─────────────────────────────
    # 人群 = `§S5` 归处非「非装面」的那几行的**命令名**（现打派生，不是手抄的名单）。
    cmds_now = [r_[2] for r_ in rows]
    section_split_closure(cmds_now)
    section_frontend_ratchet(texts, cmds_now, wrapper)
    section_frontend_goal_closure()
    section_discipline_a(texts, cmds_now, claims, cmd_addr, wrapper)

    # ── §S6 `Side` 栏复量（D4：只量代价，不许翻） ───────────────────────────────
    print(head("§S6 `Side` 栏 —— 复量（纯数据那一半；派生那一半见 docstring 的 B3）"))
    remote = [c for c, _, s in ledger if s == "Remote"]
    print(f"  `Side::Remote` 现打 **{len(remote)}** 行（分母 = `LEDGER` {len(ledger)} 行）")
    seg = const_block(led_src, "const REMOTE_SIDE_SIGNOFF")
    signoff = re.findall(r"\(Derived::(\w+), (\d+)\)", seg)
    print("  签字表 `REMOTE_SIDE_SIGNOFF`：" + " · ".join(f"{k} {v}" for k, v in signoff)
          + f"（合计 {sum(int(v) for _, v in signoff)}）")
    if sum(int(v) for _, v in signoff) != len(remote):
        red("R7a", "签字表合计 ≠ `Side::Remote` 行数")
    seg = const_block(led_src, "const FRAME_PLANE_VERDICTS")
    verdicts = re.findall(r'"([A-Za-z0-9_]+)"\s*,\s*FrameVerdict::(\w+)', seg)
    lies = [c for c, v in verdicts if v == "LiesTodayOwedACorrection"]
    print(f"  人裁表 `FRAME_PLANE_VERDICTS` {len(verdicts)} 条，其中"
          f"「说假话·欠一次订正」**{len(lies)}** 条：{', '.join(lies)}")
    print("  连锁四处，现打在不在：")
    chain = [
        ("EXPECTED_LOCAL_OR_BOTH",
         re.search(r"const EXPECTED_LOCAL_OR_BOTH: usize = (\d+);", led_src)),
        ("ORIGIN_TAKING_BOTH", re.search(r"const ORIGIN_TAKING_BOTH", led_src)),
        ("tmux.manage 那条 ASYMMETRY_REASONS",
         re.search(r'\("tmux\.manage", Asym::', led_src)),
        ("the_tmux_manage_row_stops_waiting_for_a_daemon_primitive",
         re.search(r"fn the_tmux_manage_row_stops_waiting_for_a_daemon_primitive", led_src)),
    ]
    for name, m in chain:
        if m is None:
            red("R7b", f"连锁那一处找不到了：{name}")
            print(f"    ✗ {name}")
        else:
            extra = ""
            if name == "EXPECTED_LOCAL_OR_BOTH":
                extra = f" = {m.group(1)}（翻 5 行 ⇒ {int(m.group(1)) + len(lies)}）"
            if name == "ORIGIN_TAKING_BOTH":
                n = len(re.findall(r'\n        \(\n            "([A-Za-z0-9_]+)",',
                                   const_block(led_src, "const ORIGIN_TAKING_BOTH")))
                extra = f" 现打 {n} 条（翻 5 行 ⇒ 要逐条判要不要 +5）"
            print(f"    ✓ {name}{extra}")

    # 第五处：`refuse_local_write` 的生产调用点 —— 散文里点名的比盘上多
    # 剥法只做**最粗**的一刀：列 0 的第一个 `#[cfg(test)]` 之后一律算测试段。
    # ⚠ 它比 `guard_core::production_code` 弱（不剥块注释、不认 `pub(crate) mod tests`）——
    #    写出来是因为下面这个数要拿去跟散文对拍，**剥不干净会少数**，所以两档都印。
    calls, calls_test = [], []
    for base, _d, files in os.walk(src_dir):
        for fn in sorted(files):
            if not fn.endswith(".rs"):
                continue
            p = os.path.join(base, fn)
            ls = slurp(p).split("\n")
            cut = next((i for i, l in enumerate(ls) if l == "#[cfg(test)]"), len(ls))
            for i, l in enumerate(ls):
                if "refuse_local_write(&origin," in l and not l.strip().startswith("//"):
                    (calls if i < cut else calls_test).append(
                        f"{os.path.relpath(p, src_dir)}:{i + 1}")
    print(f"  ⚠ 现打 `refuse_local_write(&origin,` 的**生产段**调用点 **{len(calls)}** 处："
          f"{', '.join(calls) if calls else '（零）'}"
          f"；测试段另有 {len(calls_test)} 处（{', '.join(calls_test)}）")
    print("    —— 而 `ASYMMETRY_REASONS` 的 `cc-bus.cockpit` 那条散文今天仍逐字写着"
          "「`cc_bus_spawn` / `cc_bus_kill` 对 `<local>` 走 `refuse_local_write`」。")

    # ── 裁决 ──────────────────────────────────────────────────────────────────
    print(head("裁决"))
    # 🔴 门禁那一格的读数行：`run_gate` 靠「N passed」认这一格**真的跑了**
    #   （`0 passed 不是绿` 是它的另一条判定）。〔`K-R128` 09-15 加〕
    print(f"  installface: {len(PASSED)} passed（分母 = `§S5c`/`§S5d`/`§S5e`/`§S5f` 四节逐条记下的"
          f"**过了**的判定条数，`len()` 现算 —— ⚠ `R1`–`R7` 那七条**不在这个数里**："
          f"它们只在红的时候出声，没有逐条的「过了」事件 ⇒ 这个数**不是**"
          f"「本尺子judge过的全部条数」）")
    if RED:
        for tag, msg in RED:
            print(f"  RED [{tag}] {msg}")
        print(f"\nRULER: FAIL（{len(RED)} 条）")
        return 1
    print("  没有红。⚠ 「没有红」只说明这几条判定过了（`R1`–`R7` ＋ `K-R128` 的 "
          "`R8`/`R9`/`R10` ＋ `K-R131` 的 `R11`），**不说明归档表的每一格判断是对的**"
          "（诚实边界 B1），**不说明 `parity_ledger.rs` 被 `§S5e` 判过**"
          "（那一份逐字判不了，闸在 `§S5c`），也**不说明收工目标定得对**"
          "（`R11` 只判算术闭合，诚实边界 B9）。")
    print("\nRULER: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
