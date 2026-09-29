#!/usr/bin/env python3
# ruff: noqa: E501
"""S29：「**为了版本切换 / 旧状态兼容**而存在的东西」四档普查 —— 只产清单，不产改动。

住址：`<仓根>/tests/evidence/S29-legacy-compat-census.py`
主产物：`调研/真相源/95-兼容债与卸载残留-四档普查.md`
读数落点：`tests/evidence/S29-readings.md`

裁定依据：用户 2026-09-19 逐字（`99` 条 80，**通则**）
    「不要管旧配置」「现在用户只有我一个，不要管这个」「我们的新版本要完全抛弃旧的」

跑法（仓根下）：
    python3 tests/evidence/S29-legacy-compat-census.py              # 主报告（四档）
    python3 tests/evidence/S29-legacy-compat-census.py --addresses  # 逐处住址（含代码/散文分列）
    python3 tests/evidence/S29-legacy-compat-census.py --tier 甲    # 只看一档
    python3 tests/evidence/S29-legacy-compat-census.py --negatives  # 🔴 负向断言逐条
    python3 tests/evidence/S29-legacy-compat-census.py --unpaired   # 🔴 丙档：无对拍的字面量族
    python3 tests/evidence/S29-legacy-compat-census.py --json
    python3 tests/evidence/S29-legacy-compat-census.py --selftest   # 反空真自检（扫空树 ⇒ 必须失败）

退出码：
    0 = 每一处登记的切口都真切到了东西（地板全过）
    3 = **有登记的切口空转**（源码动过而本清单没跟上）—— 这是失败，不是「0 命中的绿」
    4 = 自检失败（`--selftest`：扫空树却没红 ⇒ 地板是坏的）

═══════════════════════════════════════════════════════════════════════════════
 🔴 一、这把尺子回答哪一句话 —— 写死在这里
═══════════════════════════════════════════════════════════════════════════════

**判准**：一段代码若**唯一的存在理由是认出 / 迁移 / 兼容一份旧状态**，它就是设计债务。

⚠ 但「唯一的存在理由」这一句**本脚本判不了** —— 那是人读源码之后下的判断。
⇒ 本脚本做的是**三件能机械做的事**，别的不做、也不假装能做：

  **(1) 住址与存续**：登记在册的每一处切口（文件＋行段）今天还在不在、切到几行。
      切空 ⇒ 退 3。这挡的是「清单写完源码又动了，清单静默变旧」。

  **(2) 代码 / 散文分列**：🔴 **本仓的注释量极大**（登记的 95 处切口里，
      散文比代码多一倍有余）。把注释算进「待删行数」，这份清单就没用了。
      ⇒ 每一处都拆成 `code` / `prose` 两个数，**汇总只加 `code`**。

  **(3) 负向断言点名**：`!contains(旧词)` / `not.toContain(旧词)` / 「全 false 才是期望值」
      这一族判据 —— **词一删就永远满足**。逐条点名，连同被守的词一起收。

四档的归属（甲/乙/丙/丁）是**人写进 `CUTS` 表里的**，不是算出来的。
改归属必须改这张表，并在主产物里写清理由。

═══════════════════════════════════════════════════════════════════════════════
 🔴 二、定义 —— 四个词各钉一次，改定义必须改这段注释
═══════════════════════════════════════════════════════════════════════════════

**(1) 一处切口（cut）** = `(文件, 起行, 止行)` 的一段闭区间，行号 1 起、含两端。
  段的边界按「删这一族时这几行会一起走」画，不按语法块画。

**(2) `code` 行** = 该行剥掉注释与其中的字符串内容之后**仍有非空白字符**。
  剥法：`//` / `///` / `//!` 到行尾 · `/* … */` 跨行 · `#` 到行尾（`.py`/`.sh`）。
  ⚠ 剥之前先认字符串（`"` `'` 反引号 `r#"…"#`），否则 `"https://x"` 会被当成注释开头。

**(3) `prose` 行** = 该行有非空白字符，但不是 `code` 行。（空行两边都不算。）

**(4) 负向断言（negative）** = 判据的期望值是「**没有**某个词 / 某组值全 `false`」。
  它的分母是那个词。词删掉之后它恒绿 ⇒ 它既拦不住乱改，也没了对象。

═══════════════════════════════════════════════════════════════════════════════
 🔴 三、射程边界 —— 不在里面的，本脚本一个字都不说
═══════════════════════════════════════════════════════════════════════════════

· 只读 `src/**` 与 `tests/**`，**不读** `src/bridge/vendor/**`、`node_modules/**`。
· **不改一行代码**，不写 `src/**`，不写 `调研/设计/**`。
· 「旧版 Claude Code 自己写的 JSONL 少个字段」这一族（`session_map.rs` / `turn-notify.ts` /
  `bridge.rs` / `codex_record.rs` / `cards/slash.ts`）**不在人群里** —— 那是**第三方的**
  数据格式，不是我们自己的旧状态。理由写在主产物「丁档」里，不靠本脚本判。
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

# ═══════════════════════════════════════════════════════════════════════════
#  登记表：四档 × 逐处切口
#  tier: 甲=兼容债(删) · 乙=卸载残留清理(交用户裁) · 丙=缺判据(补) · 丁=别的原因(不动)
# ═══════════════════════════════════════════════════════════════════════════


@dataclass
class Cut:
    fam: str          # 族名
    tier: str         # 甲/乙/丙/丁
    path: str         # 仓根相对路径
    lo: int           # 起行（1 起，含）
    hi: int           # 止行（含）
    what: str         # 这一段是什么
    anchor: str = ""  # 🔴 锚：这一段里必须出现的字面量（切歪了会红）


CUTS: list[Cut] = [
    # ── 甲①：`daemonless` 旧键识别整族（`K-R59` 留下的墓碑 + 那条指名告知） ──────
    Cut("A1-daemonless", "甲", "src/remote-config.ts", 51, 62, "`LEGACY_NO_BACKEND_KEY` 常量＋头注", "daemonless"),
    Cut("A1-daemonless", "甲", "src/remote-config.ts", 68, 75, "`RemoteConfig.legacyNoBackend` 字段＋头注", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/remote-config.ts", 112, 124, "`legacyNoBackendHosts()` 纯函数＋头注", "legacyNoBackendHosts"),
    Cut("A1-daemonless", "甲", "src/remote-config.ts", 156, 156, "空配置分支上那一格", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/remote-config.ts", 173, 174, "读盘时 coerce 之前认旧键", "legacyNoBackendHosts"),
    Cut("A1-daemonless", "甲", "src/remote-config.ts", 178, 178, "读失败兜底上那一格", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/remote-config.ts", 209, 215, "`REMOTE_HOST_FIELDS` 头注里那段「为什么不列它」", "daemonless"),
    Cut("A1-daemonless", "甲", "src/remote-config.ts", 356, 359, "patch 时原样带过那一格", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/settings/readiness.ts", 65, 70, "`Gap.code` 字段（全仓只有这一条缺口有名字）", "code?: string"),
    Cut("A1-daemonless", "甲", "src/settings/readiness.ts", 73, 84, "`NO_BACKEND_GAP_CODE` ＋ `NO_BACKEND_CONSEQUENCE`", "REMOTE_NO_BACKEND"),
    Cut("A1-daemonless", "甲", "src/settings/readiness.ts", 167, 175, "`ReadinessInput.legacyNoBackend` 入参＋头注", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/settings/readiness.ts", 198, 211, "`computeGaps` 里那条压过账本的早分支", "NO_BACKEND_GAP_CODE"),
    Cut("A1-daemonless", "甲", "src/first-run-hint.ts", 76, 77, "`FirstRunHintDeps.legacyNoBackend`", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/first-run-hint.ts", 118, 118, "透传给 `computeGaps`", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/main.ts", 613, 616, "主窗口那份快照变量", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/main.ts", 620, 620, "注入 `FirstRunHint`", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/main.ts", 631, 632, "`reload()` 里刷新那份快照", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/settings/remote-section.ts", 208, 208, "`original` 快照上那一格", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/settings/remote-section.ts", 366, 371, "`renderGaps` 里那个 Set ＋注入", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/settings/remote-section.ts", 395, 397, "把 `g.code` 写进 DOM `data-code`", "dataset.code"),
    Cut("A1-daemonless", "甲", "src/settings/remote-section.ts", 1030, 1032, "`collect()` 原样带过那一格", "legacyNoBackend"),
    Cut("A1-daemonless", "甲", "src/settings/machine-card.ts", 431, 436, "〔散文墓碑〕那个 checkbox 原来在这", "daemonless"),
    Cut("A1-daemonless", "甲", "src/settings/accounts-section.ts", 367, 367, "〔散文墓碑〕那一档的支也没了", "daemonless"),
    Cut("A1-daemonless", "甲", "src/account-chip.ts", 41, 41, "〔散文墓碑〕以前这里排掉 daemonless 主机", "daemonless"),
    Cut("A1-daemonless", "甲", "src/session-accounts-poll.ts", 105, 105, "〔散文墓碑〕以前这里 filter 掉", "daemonless"),
    Cut("A1-daemonless", "甲", "src/accounts.ts", 107, 107, "〔散文墓碑〕那条错误串没了", "daemonless"),
    Cut("A1-daemonless", "甲", "src/launch-render-fallback.ts", 29, 30, "〔散文墓碑〕删除线那两行", "daemonless"),
    Cut("A1-daemonless", "甲", "src/bridge/src/lib.rs", 1601, 1604, "〔散文墓碑〕后端故意不读那个键", "daemonless"),
    Cut("A1-daemonless", "甲", "src/bridge/src/lib.rs", 519, 521, "〔散文墓碑〕定框 C7 第二格", "daemonless"),
    Cut("A1-daemonless", "甲", "src/bridge/src/ssh_source.rs", 2953, 2955, "〔散文墓碑〕原来的顶层二选一", "daemonless"),
    Cut("A1-daemonless", "甲", "src/bridge/src/ssh_source.rs", 3786, 3795, "〔散文墓碑〕降级读取整段删除", "daemonless"),
    Cut("A1-daemonless", "甲", "src/bridge/src/accounts.rs", 367, 368, "〔散文墓碑〕原来这里还有一个早返回", "daemonless"),
    Cut("A1-daemonless", "甲", "src/bridge/src/doc_claim_registry.rs", 200, 203, "〔散文〕③ 的前提不存在了", "daemonless"),
    Cut("A1-daemonless", "甲", "src/bridge/src/doc_claim_registry.rs", 237, 237, "🔴 `§33b` 三问表里那一整问（**代码行**）", "daemonless"),
    Cut("A1-daemonless", "甲", "src/bridge/src/rust_timer_registry.rs", 36, 37, "〔散文〕定时器登记表里那一条", "DAEMONLESS_POLL_INTERVAL"),
    Cut("A1-daemonless", "甲", "src/bridge/src/byte_cap_registry.rs", 16, 16, "〔散文〕后两处已退役", "daemonless"),
    # ── 甲②：旧单对象 `remote` 配置形态（无 `hosts` 键） ───────────────────────
    Cut("A2-single-obj", "甲", "src/remote-config.ts", 147, 149, "〔散文〕`readRemoteConfig` 头注那句向后兼容", "向后兼容"),
    Cut("A2-single-obj", "甲", "src/remote-config.ts", 166, 169, "🔴 `else if (typeof obj.host …)` 归一成 1 台", "obj.host"),
    Cut("A2-single-obj", "甲", "src/bridge/src/lib.rs", 1503, 1505, "〔散文〕`load_remote_configs` 头注那句", "旧单对象"),
    Cut("A2-single-obj", "甲", "src/bridge/src/lib.rs", 1537, 1539, "🔴 `None => vec![remote]` 那条回落", "vec![remote]"),
    Cut("A2-single-obj", "甲", "src/settings/remote-section.ts", 11, 13, "〔散文〕模块头注那句", "向后兼容"),
    Cut("A2-single-obj", "甲", "src/settings/remote-section.ts", 1129, 1129, "〔散文〕分节注释那句", "向后兼容旧单对象"),
    # ── 乙①：PowerShell v1.7.0–1.7.1 装错位置的块 ────────────────────────────
    Cut("B1-ps-misplaced", "乙", "src/bridge/src/profile_installer.rs", 457, 471, "`legacy_profile_paths()` 两条错位路径", "profile.ps1"),
    Cut("B1-ps-misplaced", "乙", "src/bridge/src/profile_installer.rs", 473, 491, "`scan_legacy_profiles()` 扫那两条", "scan_legacy_profiles"),
    Cut("B1-ps-misplaced", "乙", "src/bridge/src/lib.rs", 2150, 2153, "`CcStatusResponse.legacy_profile_paths_with_block` 字段", "legacy_profile_paths_with_block"),
    Cut("B1-ps-misplaced", "乙", "src/bridge/src/lib.rs", 2188, 2191, "`cc_status` 里扫那两条", "scan_legacy_profiles"),
    Cut("B1-ps-misplaced", "乙", "src/bridge/src/lib.rs", 2196, 2196, "`cc_status` 出参上那一格", "legacy_profile_paths_with_block"),
    Cut("B1-ps-misplaced", "乙", "src/settings/cc_integration.ts", 9, 12, "〔散文〕模块头注那段病史", "v1.7.0-1.7.1"),
    Cut("B1-ps-misplaced", "乙", "src/settings/cc_integration.ts", 259, 261, "`legacyArea` 容器", "legacyArea"),
    Cut("B1-ps-misplaced", "乙", "src/settings/cc_integration.ts", 359, 359, "调 `renderLegacy`", "renderLegacy"),
    Cut("B1-ps-misplaced", "乙", "src/settings/cc_integration.ts", 436, 463, "🔴 `renderLegacy()` 那整段告知", "v1.7.0/1.7.1 残留"),
    # ── 乙②：POSIX rc 里围栏之外那几行裸 ccm ─────────────────────────────────
    Cut("B2-rc-bare", "乙", "src/bridge/src/profile_installer.rs", 183, 206, "`LegacyRcKind` / `LegacyRcLine` 两个类型", "LegacyRcLine"),
    Cut("B2-rc-bare", "乙", "src/bridge/src/profile_installer.rs", 208, 228, "`mentions_ccm()` 带词边界", "mentions_ccm"),
    Cut("B2-rc-bare", "乙", "src/bridge/src/profile_installer.rs", 229, 245, "`fence_marker()` 认我们自己的围栏", "fence_marker"),
    Cut("B2-rc-bare", "乙", "src/bridge/src/profile_installer.rs", 247, 292, "🔴 `scan_legacy_rc_lines()` 正题", "scan_legacy_rc_lines"),
    Cut("B2-rc-bare", "乙", "src/bridge/src/profile_installer.rs", 294, 303, "`function_name_of()`", "function_name_of"),
    Cut("B2-rc-bare", "乙", "src/bridge/src/profile_installer.rs", 306, 354, "🔴 `render_manual_cleanup_hint()` 产物", "render_manual_cleanup_hint"),
    Cut("B2-rc-bare", "乙", "src/bridge/src/profile_installer.rs", 82, 88, "`ProfileScan.manual_cleanup_hint` 字段", "manual_cleanup_hint"),
    Cut("B2-rc-bare", "乙", "src/bridge/src/profile_installer.rs", 525, 533, "`scan_profile` 里装那一格", "render_manual_cleanup_hint"),
    Cut("B2-rc-bare", "乙", "src/launcher-diagnostics.ts", 306, 310, "`rcLegacy` 节点", "rcLegacy"),
    Cut("B2-rc-bare", "乙", "src/launcher-diagnostics.ts", 367, 369, "把提示显出来", "manual_cleanup_hint"),
    Cut("B2-rc-bare", "乙", "src/launcher-diagnostics.ts", 383, 383, "查不到时收起来", "rcLegacy"),
    # ── 乙③：旧 bash `ccm` 的 `~/.config/ccm/config` ─────────────────────────
    Cut("B3-ccm-config", "乙", "src/backend/control/ccm/plan.rs", 70, 82, "🔴 「发现它在就说一句，然后照常跑」", "CCM_CONFIG"),
    Cut("B3-ccm-config", "乙", "src/backend/control/ccm/argv.rs", 104, 104, "`CONFIG_REL` 常量", "config/ccm/config"),
    # ── 乙④：F02 之前的老 `cc-*` tmux 会话 ───────────────────────────────────
    Cut("B4-old-tmux", "乙", "src/common/gate-core/src/lib.rs", 74, 81, "〔散文〕两种形状都要认、一个都不许删", "cc-*"),
    Cut("B4-old-tmux", "乙", "src/common/gate-core/src/lib.rs", 91, 91, "🔴 `old_prefix` 那一支", "old_prefix"),
    Cut("B4-old-tmux", "乙", "src/common/gate-core/src/lib.rs", 102, 102, "🔴 `old_prefix ||` 并进最终判定", "old_prefix"),
    Cut("B4-old-tmux", "乙", "src/bridge/src/tmux.rs", 793, 798, "〔散文〕monitor 侧转调的头注", "向后兼容回归"),
    Cut("B4-old-tmux", "乙", "src/account-restart.ts", 26, 28, "〔散文〕那是向后兼容不是今天产的形状", "向后兼容"),
    Cut("B4-old-tmux", "乙", "src/tmux-sessions.ts", 34, 39, "〔散文〕cwd 回退的契约", "向后兼容"),
    Cut("B4-old-tmux", "乙", "src/tmux-sessions.ts", 62, 66, "🔴 `findClaudeTmux` 的 cwd 回退分支", "anySidKnown"),
    Cut("B4-old-tmux", "乙", "src/tmux-sessions.ts", 84, 98, "`isCwdFallbackMatch()`（回退命中时显式提示）", "isCwdFallbackMatch"),
    Cut("B4-old-tmux", "乙", "src/bridge/src/tmux.rs", 17, 21, "〔散文〕`@ccm_sid` 空串 ⇒ 回落 path/cmd", "向后兼容"),
    # ── 丙：缺判据（改名/改格式会静默） ──────────────────────────────────────
    Cut("C1-fence-4th", "丙", "src/bridge/src/profile_installer.rs", 234, 237, "🔴 `fence_marker` 里那**第四份**围栏字面量", "# === cc-monitor"),
    Cut("C1-fence-4th", "丙", "src/bridge/src/profile_installer.rs", 46, 47, "`BEGIN_MARKER` / `END_MARKER`", "BEGIN_MARKER"),
    Cut("C1-fence-4th", "丙", "src/bridge/src/sftp.rs", 912, 913, "`CCM_PROFILE_BEGIN` / `_END`", "CCM_PROFILE_BEGIN"),
    Cut("C1-fence-4th", "丙", "src/bridge/src/account_aliases.rs", 50, 55, "`RC_BEGIN` / `FILE_BEGIN` 两对", "RC_BEGIN"),
    Cut("C2-template-top", "丙", "src/common/creds-core/src/store.rs", 108, 125, "🔴 `TEMPLATE` 里那个**顶层** `api_key` 空格", "api_key"),
    Cut("C2-template-top", "丙", "tests/bridge/creds_store_tests.rs", 73, 73, "🔴 今天唯一那条 TEMPLATE 判据（恒真）", "TEMPLATE.contains"),
    Cut("C3-ts-scan", "丙", "tests/bridge/backend/control/launch_wire_f07_main_path_tests.rs", 544, 548, "🔴 Rust 判据拿字面量扫 TS 源码", '"daemonless",'),
    # ── 丁：别的原因（不动，但点名说清为什么不算） ───────────────────────────
    Cut("D1-force-legacy", "丁", "src/behavior.ts", 70, 70, "`forceLegacyLaunchRenderer` 存盘键名", "forceLegacyLaunchRenderer"),
    Cut("D1-force-legacy", "丁", "src/behavior.ts", 106, 112, "字段＋头注（手动逃生口）", "forceLegacyLaunchRenderer"),
    Cut("D1-force-legacy", "丁", "src/behavior.ts", 124, 124, "默认值 false", "forceLegacyLaunchRenderer"),
    Cut("D1-force-legacy", "丁", "src/behavior.ts", 158, 161, "读盘", "forceLegacyLaunchRenderer"),
    Cut("D1-force-legacy", "丁", "src/behavior.ts", 180, 180, "写盘", "forceLegacyLaunchRenderer"),
    Cut("D1-force-legacy", "丁", "src/remote-launch-run.ts", 55, 64, "🔴 唯一的生产分支", "forceLegacyLaunchRenderer"),
    Cut("D1-force-legacy", "丁", "src/settings/panel.ts", 263, 266, "面板缓存它（防被别的勾选重置）", "forceLegacyLaunchRenderer"),
    Cut("D1-force-legacy", "丁", "src/settings/panel.ts", 337, 337, "open() 时读", "forceLegacyLaunchRenderer"),
    Cut("D1-force-legacy", "丁", "src/settings/panel.ts", 454, 454, "保存时原样带回", "forceLegacyLaunchRenderer"),
    Cut("D2-legacy-acct", "丁", "src/common/creds-core/src/store.rs", 66, 88, "`LEGACY_ACCOUNT_ID` 常量＋头注", "LEGACY_ACCOUNT_ID"),
    Cut("D2-legacy-acct", "丁", "src/common/creds-core/src/store.rs", 367, 383, "🔴 `read_accounts` 里折出那一行", "LEGACY_ACCOUNT_ID"),
    Cut("D2-legacy-acct", "丁", "src/settings/accounts-section.ts", 173, 181, "界面上那一行「顶层那一把（历史格式）」", "relay-key-legacy"),
    Cut("D3-proto-neg", "丁", "src/bridge/src/tmux.rs", 345, 350, "`SkipReason::LegacyAmbiguousEmpty`（旧 daemon 空串歧义）", "LegacyAmbiguousEmpty"),
    Cut("D3-proto-neg", "丁", "src/bridge/src/tmux.rs", 795, 800, "〔散文〕`enter` 落在两个 mode 名而不是字段", "cc-*"),
    Cut("D3-proto-neg", "丁", "src/bridge/src/remote_history.rs", 8, 9, "〔散文〕旧 daemon 首行 hello ⇒ 明确报版本过旧", "旧 daemon 兼容"),
    Cut("D3-proto-neg", "丁", "src/bridge/src/inbound_client.rs", 81, 84, "能力协商失败的措辞（「多半是旧版本，请重装」）", "旧版本"),
    Cut("D4-build-id", "丁", "src/bridge/src/local_daemon.rs", 443, 445, "`hello_verdict` 拿 `DAEMON_BUILD_ID` 逐字比", "DAEMON_BUILD_ID"),
    Cut("D4-build-id", "丁", "src/bridge/build.rs", 176, 187, "从 daemon 源码抠 `BUILD_ID` emit 成编译期 env", "BUILD_ID"),
]

# ═══════════════════════════════════════════════════════════════════════════
#  🔴 负向断言登记表 —— 「词一删就永远满足」的那一族
#  guarded: 它的分母（被守的那个词）。词没了 ⇒ 这条判据没了对象。
#  already_green: 今天源码里**已经**没有那个词 ⇒ 它此刻就是恒绿的。
# ═══════════════════════════════════════════════════════════════════════════


@dataclass
class Negative:
    path: str
    line: int
    guarded: str          # 被守的词
    guarded_in: str       # 那个词该出现在哪份源码里
    shape: str            # 断言形状
    note: str = ""


NEGATIVES: list[Negative] = [
    Negative("tests/bridge/backend/control/launch_wire_f07_main_path_tests.rs", 546,
             '"daemonless",', "src/remote-config.ts", '!cfg.contains(…)',
             "守的是 `REMOTE_HOST_FIELDS` 里不许再有那一项。**今天已恒绿**"),
    Negative("tests/bridge/backend/control/launch_wire_f07_main_path_tests.rs", 553,
             "daemonlessInput", "src/settings/machine-card.ts", '!card.contains(…)',
             "守的是机器卡片上不许再有那个 input。**今天已恒绿**"),
    Negative("tests/bridge/backend/control/launch_wire_f07_main_path_tests.rs", 558,
             "daemonless_stream_loop", "src/bridge/src/ssh_source.rs", '!src.contains(…)',
             "守的是数据源里不许再有那条轮询回落。**今天已恒绿**"),
    Negative("tests/bridge/doc_claim_registry_tests.rs", 441,
             '"daemonless",', "src/remote-config.ts", "carriers[0] 期望 false",
             "三格全 false ⇒ 判词「已退役」。**今天已恒绿**"),
    Negative("tests/bridge/doc_claim_registry_tests.rs", 445,
             "daemonlessInput", "src/settings/machine-card.ts", "carriers[1] 期望 false",
             "同上"),
    Negative("tests/bridge/doc_claim_registry_tests.rs", 449,
             "daemonless_stream_loop", "src/bridge/src/ssh_source.rs", "carriers[2] 期望 false",
             "同上"),
    Negative("tests/settings/remote-section.vitest.ts", 252,
             "daemonless", "src/remote-config.ts", "not.toContain",
             "断 `hosts[0]` 上不再冒出那个属性"),
    Negative("tests/settings/remote-section.vitest.ts", 268,
             "daemonless", "src/remote-config.ts", "not.toContain",
             "断保存一次就把旧键写没 —— **这一条是迁移本体的证据**，和上面七条不同"),
    Negative("tests/settings/remote-section.vitest.ts", 250,
             "nano", "tests/settings/remote-section.vitest.ts", "not.toContain",
             "阴性对照（不是恒挂）—— 随 `legacyNoBackend` 一起走"),
    Negative("tests/bridge/creds_store_tests.rs", 367,
             "LEGACY_ACCOUNT_ID", "src/common/creds-core/src/store.rs", "!rows.iter().any(…)",
             "🔴 **不是恒绿** —— 它守的是写侧不许落到那一行，词还活着（丁档）"),
    Negative("tests/bridge/creds_store_tests.rs", 443,
             "store::merge_key(", "src/bridge/src/creds_store.rs", "matches(…).count() == 0",
             "🔴 形状上恒绿，但**不是兼容债**：它守的是写侧不许再往顶层那一格写，而顶层那一行今天仍读得出来（丁档）。"
             "⚠ 它上面就带着一条反空真（断生产段里有 `store::merge_account_key(`）—— 这一族里唯一自带阴性对照的"),
]

# ═══════════════════════════════════════════════════════════════════════════
#  🔴 丙档：无对拍的字面量族 —— 同一个事实有 N 份独立字面量，没有一条判据把它们绑在一起
# ═══════════════════════════════════════════════════════════════════════════

UNPAIRED: list[dict] = [
    {
        "id": "U1 · 围栏前缀 `# === cc-monitor`",
        "fact": "「这一行是不是**我们**写进用户 rc/profile 的边界」",
        "literals": [
            ("src/bridge/src/profile_installer.rs", "BEGIN_MARKER", '"# === cc-monitor BEGIN"'),
            ("src/bridge/src/sftp.rs", "CCM_PROFILE_BEGIN", '"# === cc-monitor remote ccm BEGIN ==="'),
            ("src/bridge/src/account_aliases.rs", "RC_BEGIN", '"# === cc-monitor aliases BEGIN v1 ==="'),
            ("src/bridge/src/account_aliases.rs", "FILE_BEGIN", '"# === cc-monitor account aliases BEGIN v1 ==="'),
            ("src/bridge/src/profile_installer.rs", "fence_marker() 里的裸串", '"# === cc-monitor"'),
        ],
        "gap": "第 5 份是**裸字面量**，与前四个常量之间没有任何断言。改名改掉前四个而漏掉它 ⇒ "
               "`scan_legacy_rc_lines` 不再认得我们自己的围栏 ⇒ 装完立刻回头指着我们刚写的那几行说「这是旧的」。",
        "probe": ("src/bridge/src/profile_installer.rs", "# === cc-monitor"),
    },
    {
        "id": "U2 · 用户盘上那份围栏（**改名 = 永久残留**）",
        "fact": "「卸载时找得到当年装进去的那一块」",
        "literals": [
            ("src/bridge/src/profile_installer.rs", "strip_block/find_block_range", "靠 BEGIN/END_MARKER 配对"),
            ("用户机器上的 ~/.bashrc / $PROFILE", "已写进去的那一块", "写的是**当年那一版**的字面量"),
        ],
        "gap": "🔴 围栏常量一改名，盘上那一块就**永远对不上**：界面说「未安装」、卸载按钮藏起来、"
               "那几行留在用户 rc 里没人再提。本仓今天没有任何东西认得「上一版的围栏」。",
        "probe": ("src/bridge/src/profile_installer.rs", "strip_block"),
    },
    {
        "id": "U3 · `TEMPLATE` 顶层 `api_key` ↔ `read_accounts` 顶层那一支",
        "fact": "「我们发给用户的模板里教他填的那一格，读得回来」",
        "literals": [
            ("src/common/creds-core/src/store.rs", "TEMPLATE", '顶层 `"api_key": ""`'),
            ("src/common/creds-core/src/store.rs", "read_accounts", "折成 LEGACY_ACCOUNT_ID 那一行"),
        ],
        "gap": "今天唯一那条判据是 `TEMPLATE.contains(KEY_FIELD)`（`creds_store_tests.rs:73`）——"
               "**恒真**：`_note_accounts` 那段说明文字里就含 `api_key`。"
               "⇒ 把顶层那一支删掉，模板照旧教用户填一格没人读的东西，一条判据都不红。",
        "probe": ("src/common/creds-core/src/store.rs", "read_accounts"),
    },
    {
        "id": "U4 · `forceLegacyLaunchRenderer` 存盘键名",
        "fact": "「用户手改 config.json 写下的那个逃生口开关」",
        "literals": [
            ("src/behavior.ts", "KEY_FORCE_LEGACY_LAUNCH_RENDERER", '"forceLegacyLaunchRenderer"'),
        ],
        "gap": "只有一份字面量（好），但它是**盘上的键名**：改名 ⇒ 用户此前写下的 `true` 被静默当未知键忽略、"
               "逃生口无声关闭。全仓没有任何「认出旧键名」的机制（`daemonless` 那一族反而有）。",
        "probe": ("src/behavior.ts", "forceLegacyLaunchRenderer"),
    },
]

# ═══════════════════════════════════════════════════════════════════════════
#  代码 / 散文 剥离
# ═══════════════════════════════════════════════════════════════════════════

HASH_EXT = {".py", ".sh", ".toml", ".yml", ".yaml"}
SLASH_EXT = {".ts", ".mts", ".rs", ".js", ".css"}


def strip_to_code(text: str, ext: str) -> list[str]:
    """逐行剥掉注释；字符串内容**保留**（登记表/针/报文都是承重的代码）。

    返回与输入等长的列表，每项是该行剥完之后剩下的东西。
    """
    out: list[str] = []
    in_block = False          # /* … */
    in_str: str | None = None  # '"' | "'" | '`'
    in_raw = False            # r#"…"#（Rust）
    for line in text.split("\n"):
        buf: list[str] = []
        i = 0
        n = len(line)
        while i < n:
            c = line[i]
            if in_block:
                if ext in SLASH_EXT and line.startswith("*/", i):
                    in_block = False
                    i += 2
                    continue
                i += 1
                continue
            if in_raw:
                if line.startswith('"#', i):
                    in_raw = False
                    buf.append('"#')
                    i += 2
                    continue
                buf.append(c)
                i += 1
                continue
            if in_str is not None:
                buf.append(c)
                if c == "\\" and i + 1 < n:
                    buf.append(line[i + 1])
                    i += 2
                    continue
                if c == in_str:
                    in_str = None
                i += 1
                continue
            # 不在串里
            if ext in SLASH_EXT:
                if line.startswith("//", i):
                    break  # 到行尾都是注释
                if line.startswith("/*", i):
                    in_block = True
                    i += 2
                    continue
                if ext == ".rs" and line.startswith('r#"', i):
                    in_raw = True
                    buf.append('r#"')
                    i += 3
                    continue
            if ext in HASH_EXT and c == "#":
                break
            if c in ('"', "'", "`"):
                in_str = c
                buf.append(c)
                i += 1
                continue
            buf.append(c)
            i += 1
        out.append("".join(buf))
        # 🔴 **每行末尾一律清掉「在串里」这个状态**（`in_block` 与 Rust 的 `r#"…"#` 除外）。
        #
        # 不清会出一种**静默把注释算成代码**的病，实测咬过一次：
        # 某一行里有个没配对的引号 / 反引号（中文行里很常见），状态就一路挂到文件末尾，
        # 后面每一行都被当成「在串里」⇒ `//` 不再被认成注释 ⇒ 53 行纯注释被算进代码数。
        # （`src/launcher-diagnostics.ts` 上现打：修之前 53 行，修之后 0 行。）
        #
        # 代价：真正跨行的模板串 / 带 `\` 续行的 Rust 串，其续行按新行重新判 ——
        # 那些行仍被算成 code（它们本来就是代码），只有「续行里正好有 `//`」会少算。
        # 宁可这样：**少算注释比多算注释安全**，多算会把待删行数说大。
        in_str = None
    return out


@dataclass
class CutReading:
    cut: Cut
    exists: bool = False
    code: int = 0
    prose: int = 0
    blank: int = 0
    anchor_ok: bool = False
    code_lines: list[tuple[int, str]] = field(default_factory=list)
    prose_lines: list[int] = field(default_factory=list)


def read_cut(root: Path, cut: Cut) -> CutReading:
    r = CutReading(cut=cut)
    p = root / cut.path
    if not p.is_file():
        return r
    r.exists = True
    ext = p.suffix
    raw = p.read_text(encoding="utf-8", errors="replace")
    src = raw.split("\n")
    stripped = strip_to_code(raw, ext)
    lo = max(1, cut.lo)
    hi = min(len(src), cut.hi)
    seg = []
    for ln in range(lo, hi + 1):
        orig = src[ln - 1]
        code = stripped[ln - 1]
        seg.append(orig)
        if not orig.strip():
            r.blank += 1
        elif code.strip():
            r.code += 1
            r.code_lines.append((ln, orig.rstrip()))
        else:
            r.prose += 1
            r.prose_lines.append(ln)
    if cut.anchor:
        r.anchor_ok = cut.anchor in "\n".join(seg)
    else:
        r.anchor_ok = bool(seg)
    return r


def census(root: Path) -> dict:
    readings = [read_cut(root, c) for c in CUTS]
    fams: dict[str, dict] = {}
    for r in readings:
        f = fams.setdefault(
            r.cut.fam, {"tier": r.cut.tier, "code": 0, "prose": 0, "cuts": 0, "missing": 0}
        )
        f["cuts"] += 1
        f["code"] += r.code
        f["prose"] += r.prose
        if not (r.exists and r.anchor_ok):
            f["missing"] += 1
    tiers: dict[str, dict] = {}
    for fam, v in fams.items():
        t = tiers.setdefault(v["tier"], {"code": 0, "prose": 0, "cuts": 0, "fams": []})
        t["code"] += v["code"]
        t["prose"] += v["prose"]
        t["cuts"] += v["cuts"]
        t["fams"].append(fam)
    # 负向断言：核一核它守的那个词今天在不在源码里
    negs = []
    for ng in NEGATIVES:
        tp = root / ng.path
        gp = root / ng.guarded_in
        present_in_test = False
        if tp.is_file():
            lines = tp.read_text(encoding="utf-8", errors="replace").split("\n")
            if 1 <= ng.line <= len(lines):
                present_in_test = True
        word_alive = None
        if gp.is_file():
            ext = gp.suffix
            body = gp.read_text(encoding="utf-8", errors="replace")
            code_only = "\n".join(strip_to_code(body, ext))
            word_alive = ng.guarded in code_only
        negs.append(
            {
                "path": ng.path,
                "line": ng.line,
                "guarded": ng.guarded,
                "guarded_in": ng.guarded_in,
                "shape": ng.shape,
                "note": ng.note,
                "test_line_exists": present_in_test,
                "guarded_word_alive_in_code": word_alive,
            }
        )
    unpaired = []
    for u in UNPAIRED:
        pth, needle = u["probe"]
        f = root / pth
        hit = f.is_file() and needle in f.read_text(encoding="utf-8", errors="replace")
        unpaired.append({**{k: v for k, v in u.items() if k != "probe"}, "probe_hit": hit})
    return {
        "readings": readings,
        "fams": fams,
        "tiers": tiers,
        "negatives": negs,
        "unpaired": unpaired,
    }


TIER_NAME = {
    "甲": "甲 · 兼容债（删）",
    "乙": "乙 · 卸载残留清理（🔴 交用户裁）",
    "丙": "丙 · 缺判据（补判据，不是删）",
    "丁": "丁 · 别的原因（不动）",
}


def report(res: dict, tier_filter: str | None = None) -> list[str]:
    out: list[str] = []
    rs: list[CutReading] = res["readings"]
    tot_c = sum(r.code for r in rs)
    tot_p = sum(r.prose for r in rs)
    miss = [r for r in rs if not (r.exists and r.anchor_ok)]
    out.append(
        f"人群：{len(CUTS)} 处切口 · 代码 {tot_c} 行 · 散文 {tot_p} 行 "
        f"（散文占 {tot_p * 100 // max(1, tot_c + tot_p)}%）· 空转 {len(miss)} 处"
    )
    out.append("")
    for tier in ("甲", "乙", "丙", "丁"):
        if tier_filter and tier != tier_filter:
            continue
        t = res["tiers"].get(tier)
        if not t:
            continue
        out.append(f"── {TIER_NAME[tier]} ── 代码 {t['code']} 行 · 散文 {t['prose']} 行 · {t['cuts']} 处")
        for fam in sorted(t["fams"]):
            v = res["fams"][fam]
            flag = f"  🔴 空转 {v['missing']}" if v["missing"] else ""
            out.append(
                f"    {fam:<18} 代码 {v['code']:>4} · 散文 {v['prose']:>4} · {v['cuts']:>2} 处{flag}"
            )
        out.append("")
    if miss:
        out.append("🔴 空转的切口（源码动过而本清单没跟上）：")
        for r in miss:
            why = "文件不在" if not r.exists else f"锚 `{r.cut.anchor}` 不在该行段里"
            out.append(f"    {r.cut.path}:{r.cut.lo}-{r.cut.hi}  [{r.cut.fam}]  {why}")
        out.append("")
    ng = res["negatives"]
    green = [n for n in ng if n["guarded_word_alive_in_code"] is False]
    out.append(f"🔴 负向断言：{len(ng)} 条登记 · 其中 **{len(green)} 条今天已恒绿**（被守的词在生产代码里已不存在）")
    out.append("")
    out.append(f"🔴 丙档无对拍的字面量族：{len(res['unpaired'])} 族")
    return out


def addresses(res: dict, tier_filter: str | None = None) -> list[str]:
    out: list[str] = []
    for r in res["readings"]:
        if tier_filter and r.cut.tier != tier_filter:
            continue
        head = f"[{r.cut.tier}][{r.cut.fam}] {r.cut.path}:{r.cut.lo}-{r.cut.hi}  代码 {r.code} · 散文 {r.prose}"
        if not (r.exists and r.anchor_ok):
            head += "   🔴 空转"
        out.append(head)
        out.append(f"        {r.cut.what}")
        for ln, txt in r.code_lines:
            out.append(f"        {ln:>5} │ {txt[:150]}")
        out.append("")
    return out


def negatives_report(res: dict) -> list[str]:
    out = ["🔴 负向断言逐条 —— 「词一删就永远满足」的那一族", ""]
    for n in res["negatives"]:
        state = (
            "**已恒绿**" if n["guarded_word_alive_in_code"] is False
            else ("词还活着" if n["guarded_word_alive_in_code"] else "查不到那份源码")
        )
        out.append(f"  {n['path']}:{n['line']}")
        out.append(f"      形状：{n['shape']}   分母：`{n['guarded']}`（住 {n['guarded_in']}）")
        out.append(f"      状态：{state}")
        if n["note"]:
            out.append(f"      说明：{n['note']}")
        if not n["test_line_exists"]:
            out.append("      🔴 那份判据文件里没有这一行 —— 住址过期了")
        out.append("")
    return out


def unpaired_report(res: dict) -> list[str]:
    out = ["🔴 丙档 —— 同一个事实有 N 份独立字面量，没有一条判据把它们绑在一起", ""]
    for u in res["unpaired"]:
        out.append(f"  {u['id']}")
        out.append(f"      事实：{u['fact']}")
        for path, name, lit in u["literals"]:
            out.append(f"        · {path}  `{name}` = {lit}")
        out.append(f"      缺的是：{u['gap']}")
        if not u["probe_hit"]:
            out.append("      🔴 探针没命中 —— 这一族的住址过期了")
        out.append("")
    return out


def main() -> int:
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument("--addresses", action="store_true")
    ap.add_argument("--negatives", action="store_true")
    ap.add_argument("--unpaired", action="store_true")
    ap.add_argument("--tier", choices=["甲", "乙", "丙", "丁"])
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--root", default=str(REPO))
    a = ap.parse_args()

    if a.selftest:
        # 🔴 反空真：对着一棵空树跑，必须**每一处都空转**且退 3。
        import tempfile

        with tempfile.TemporaryDirectory() as td:
            res = census(Path(td))
            miss = [r for r in res["readings"] if not (r.exists and r.anchor_ok)]
            tot = sum(r.code for r in res["readings"])
            ok = len(miss) == len(CUTS) and tot == 0
            print(f"自检：空树上空转 {len(miss)}/{len(CUTS)} 处 · 代码行合计 {tot}")
            if not ok:
                print("🔴 自检失败：空树上居然切到了东西（或没全空转）—— 地板是坏的")
                return 4
        # 🔴 第二格自检：**剥法**本身 —— 纯注释行不许被算成 code。
        #   （这一格是实测咬出来的：行末不清「在串里」状态时，
        #     `src/launcher-diagnostics.ts` 上 53 行纯注释被算进了代码数。）
        probes = [
            "src/launcher-diagnostics.ts", "src/remote-config.ts",
            "src/bridge/src/profile_installer.rs", "src/settings/cc_integration.ts",
            "src/main.ts", "src/bridge/src/tmux.rs",
        ]
        mis = 0
        seen = 0
        for rel in probes:
            f = Path(a.root) / rel
            if not f.is_file():
                continue
            seen += 1
            body = f.read_text(encoding="utf-8", errors="replace")
            st = strip_to_code(body, f.suffix)
            lines = body.split("\n")
            mis += sum(
                1
                for i, ln in enumerate(lines)
                if ln.lstrip().startswith(("//", "///", "//!")) and st[i].strip()
            )
        print(f"剥法自检：{seen} 份语料里，纯注释行被算成 code 的有 {mis} 行（必须是 0）")
        if seen == 0:
            print("🔴 自检失败：一份语料都没读到 —— 这一格是空的")
            return 4
        if mis:
            print("🔴 自检失败：剥法把注释算成了代码 —— 待删行数会被说大")
            return 4

        # 再验一次：真树上必须切得到
        res2 = census(Path(a.root))
        tot2 = sum(r.code for r in res2["readings"])
        if tot2 == 0:
            print("🔴 自检失败：真树上一行都没切到 —— 取法坏了")
            return 4
        print(f"自检通过：真树上切到 {tot2} 行代码，空树上 0 行。")
        return 0

    root = Path(a.root)
    res = census(root)

    if a.json:
        print(
            json.dumps(
                {
                    "fams": res["fams"],
                    "tiers": {k: {kk: vv for kk, vv in v.items()} for k, v in res["tiers"].items()},
                    "cuts": [
                        {
                            "fam": r.cut.fam, "tier": r.cut.tier, "path": r.cut.path,
                            "lo": r.cut.lo, "hi": r.cut.hi, "what": r.cut.what,
                            "code": r.code, "prose": r.prose,
                            "ok": r.exists and r.anchor_ok,
                        }
                        for r in res["readings"]
                    ],
                    "negatives": res["negatives"],
                    "unpaired": res["unpaired"],
                },
                ensure_ascii=False,
                indent=2,
            )
        )
    elif a.negatives:
        print("\n".join(negatives_report(res)))
    elif a.unpaired:
        print("\n".join(unpaired_report(res)))
    elif a.addresses:
        print("\n".join(addresses(res, a.tier)))
    else:
        print("\n".join(report(res, a.tier)))

    miss = [r for r in res["readings"] if not (r.exists and r.anchor_ok)]
    bad_probe = [u for u in res["unpaired"] if not u["probe_hit"]]
    bad_neg = [n for n in res["negatives"] if not n["test_line_exists"]]
    if miss or bad_probe or bad_neg:
        if not (a.json or a.addresses or a.negatives or a.unpaired):
            print(
                f"\n🔴 触地板：切口空转 {len(miss)} · 丙档探针落空 {len(bad_probe)} · 判据住址过期 {len(bad_neg)}"
            )
        return 3
    return 0


if __name__ == "__main__":
    os.chdir(REPO)
    sys.exit(main())
