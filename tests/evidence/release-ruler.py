#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""发版尺子 —— **发版那条流水线的两件事：闸没被偷偷改过 · 正文是我们写的那份。**

# 它守的性质是（九句）

1. `release.yml` 里「发不发布」那个闸
   （`env.PUBLISH` 的**字面** ＋ 每一处「往 Release 上写」与 CI 门那一步的 `if:`）
   **与钉在本文件里的那一份逐字相同**。
2. **每一处「往 Release 上写」的步骤都带一个正文来源**，
   而且那份正文**同一个 job 里真的有人生成它**、生成器盘上真的在、生成出来的段真的非空。
3. **产字节那条路**（⑨–⑪）：平台承诺面的每一格都有一条产线在本文件上
   （两向），本文件上没有第二条**没人登记**的产线（两向），每一处抠后端 `const BUILD_ID` /
   身份戳界标的住址**实打指得到真东西**，而「半 bump」的发版核对两半都在：本文件把内嵌的
   两份 musl 字节对上源码 `BUILD_ID`，`build.rs` 要求几份内嵌字节彼此同一版（不等当场失败，
   `"unknown"` 兜底从类型上消失）。
4. **`BUILD_ID` bump 的同拍债：re-embed**（⑬）。产字节那条路的
   **本机那一端**有唯一一条命令（`tests/scripts/re-embed.sh`），它的配方与上面那条产线
   **同源**（target 两向集合相等 ＋ 旗标逐字相同）；它铺的落点与 `build.rs` 吃的落点
   两向相等、且**三个落点全部被 `.gitignore` 挡着**（两向）；`build.rs` 那几处守卫的
   **出路只有这一个住址**，不许再手抄第二条产字节配方。
   逐条与它买不到什么，见下面 ⑬ 那一段的头注。
5. **本包的每一个 `[[bin]]` 都有一条登记过的入包路线**（⑭，
   两向集合相等），主二进制恰好一个、带 `required-features` 的 bin 零命中、
   `externalBin` ↔ 登记两向且不与本包的 bin 重名、消费侧找它的那个文件名主干
   与产出侧**逐字同源**、而这条路线依据的**打包工具版本**钉住。
   ⚠ 它推翻了一条写在 `Cargo.toml` 与 `filewin::proc` 头注里的报备（「安装包只装主二进制
   与 sidecar」）—— 现打读数与机制逐条住下面 ⑭ 那一段的头注。
6. **产物路径的根 ↔ 两份 `.cargo/config.toml` 声明的 `target-dir`**
   （⑮，两向集合相等）。根写错是完全静默的坏法 —— Release 建得出来、上面一个
   安装包都没有。逐条住下面 ⑮ 那一段的头注。
7. **Release 资产 ↔ 登记表**（⑯）：`RELEASING.md` §2.2 那张表是资产名的登记（对外的资产名，改名要同拍改这张表）。
   两处发布步骤的 `files:` ↔ 登记两向一一对上；登记、交叉编译、改名成资产的 musl 目标都等于
   x86_64 / aarch64；每份校验和恰好一步生成、排在发布前，算的那几份 ↔ 登记里进它的资产两向一一对上。
   ⚠ 只认文件名；glob 那几样（deb · 安装器 · msi）真叫什么由打包器定，它判不了。
8. **第三方许可声明 ↔ 现打的依赖**（⑰）：`THIRD-PARTY-NOTICES.txt` 里的 crate ∪ `tests/evidence/third-party-not-shipped.txt`
   == 两份 `Cargo.lock` 的第三方包（两向、两份不相交）；声明里的 npm 包 == `package-lock.json` 的生产依赖
   （两向）；声明里的 Maven 坐标 == 手机 `src/mobile/app/gradle.lockfile` 进发布包那一档（两向），每个的许可都在
   许可名单（`third-party-about.toml` 的 `accepted`：只收 MIT / Apache 兼容的，不收 GPL）里（⑰e）；发版那一趟在每个发布 job 之前重新生成并逐字比（`--check`），装的 cargo-about 版本 == 生成器钉的；声明进安装包
   （`tauri.sidecar.conf.json` 的 `bundle.resources`）。⚠ 「哪些包进产物」由 cargo-about 按目标平台算，本格不重算，只认生成器写下的那一分。

9. **手机那一格**（⑱）：`build-mobile` 在、打 `:app:assembleRelease --no-daemon`、排在 `build-windows` 之后；
   四个签名 secrets 只在它还原那一步的 `env:` 上各出现一次（别处、`run:` 里、`ci.yml` 里零次）；还原只写
   `$RUNNER_TEMP`，碰签名变量的行都重定向进文件、不开 xtrace；打包之后 `if: always()` 删掉；对着 APK 核签名与
   versionName；keystore / keystore.properties 被 `.gitignore` 挡住、仓里一份都没有。
   ⚠ 只读盘上文本：secrets 真给没给、runner 上签不签得出来，判不了（那要一趟 workflow_dispatch）。

# 🔴 第 3 句为什么归这份判据，而不是归 `platform` 那一格

平台账本（`platform`，第 24 格）判的是「**门禁盖到了哪些平台**」——
它读 `gate.sh`。本条判的是「**产线盖到了哪些平台**」—— 它读 `release.yml`。
两件事分开的理由是硬的：三个落点全部 gitignore ⇒ **字节不进仓**，
三条产线**只由 `release.yml` 一个文件驱动** ⇒ 「这张表的门禁只能建在 `release.yml` 上，
不能建在 `cargo` 上」。⇒ 门禁编得过 ≠ 发版那趟产得出字节，**两格都要有**。
⚠ **平台承诺面本文件一个字不抄**：`PROMISED` 的唯一住址是那份账本，这里 import 它。

# 🔴 它为什么住在这里，而不是住在 `ci.yml` 的 `run:` 块里

**runner 会把 `run:` 块里的 `${{ … }}` 先求值再交给 shell** ⇒ 写进 `ci.yml` 的 `run: |` 里，
上面第 1 条要比的那个字面 `CANON_ENV` 在渲染之后变成
`"false"`（`push` / tag 上是 `"true"`），**与盘上那串模板在三个触发器上都必不相等**
⇒ 那样写的守卫**永远不可能过**。

⇒ 字面不进 `run:` 块，而且比「经 `env:` 传」更远：
**整段判据住这份 `.py`**。`.py` 不经 GitHub 的表达式渲染器，`${{` 在这里就是四个字符。
不用转义（`${{` → `${{ '${{' }}` 之类）：
**转义只治「这一个字面」，住进 `.py` 同时治了「这段判据在本地跑不起来」** —— 而后者
正是坏了没人看见的那一形。

# 🔴 刻意不用 PyYAML

沙箱镜像 `ccmon-devbox:latest` 里没有 PyYAML
（`docker run --rm ccmon-devbox:latest python3 -c 'import yaml'` ⇒ `ModuleNotFoundError`）。
用 `yaml.safe_load` 写 ⇒ 它**在本地跑不起来** ⇒ 只能在云端试，
而云端每试一次要一趟 CI。下面那个 `parse_workflow()` 是本文件自带的 **YAML 子集切块器**，
它切 `on:` / `env:` / `jobs:` / 每一步的 `with:`。

# ⚠ 它买不到什么（逐条，别读宽）

1. **它不执行 GitHub 的表达式求值器。**「这个表达式在那个事件下真的是 false 吗」这一格靠的是
   「字面与钉住的那一份逐字相同」，不是求值。等价改写（`== 'true'` 写成 `!= 'false'`）会被
   判红 —— 假阳，但方向是安全的那一侧。
2. **「往 Release 上写」只认两种形状**：`uses:` 是 `softprops/action-gh-release`，
   或 `run:` 里出现 `gh release` / `gh api` 打 `/releases`。
   **换第三种路子上传（手写 `curl` 打 `uploads.github.com`）它看不见。**
3. 它读的是**盘上的 `release.yml`**，不是某一次 run 真正跑的那一份。
4. 🔴 **它不证明「云端真会绿」。** 它证明的是「**盘上这几份文本满足下面逐条列出的那些条件**」。
   `release.yml` 那一族**在本地一步都真跑不起来**（触发器只有推 `v*` tag 与手工 dispatch；
   `windows-latest` / `ubuntu-latest` 两个 runner 本机都没有；`cargo zigbuild` 那两趟、
   `npx tauri build` 那两趟、`softprops/action-gh-release` 那两处，本判据**一条都不执行**）。
   ⇒ 买到的是「**盘上这份文本满足这几条**」，**不是**「云端那一趟会绿」。
   ⇒ 尤其是 ⑨–⑪ 那几条：「**登记的那一步在文件里**」≠「**那一步在 runner 上编得出字节**」，
   更不等于「那份字节在目标机器上跑得起来」。真机行为这一维仍然是**判不了**，不是「通过」。
   本地绿 ＋ 云端红这一形真出过。
5. 第 2 条性质里「正文真的是我们写的那份」，机器认的是
   **「有 `body_path` · 不是 `generate_release_notes` · 那个路径同 job 里有人生成 · 生成器跑得出非空的段」**。
   **正文写得对不对、好不好，它一个字都不判**（那要读语义）。
6. **切块器是手写的 YAML 子集**，不是 YAML 实现。锚点、多文档、流式映射（`{a: 1}`）、
   复杂 key 一概不支持 —— 挡这一形的是下面那几条**地板**（`on` 解析得出来 · job 数 ·
   step 总数 · 「往 Release 上写」至少两处 · CI 门恰好一处）：切块器坏了地板先红，
   而不是静默地「零违例」。

# 跑法

    python3 evidence/release-ruler.py                             # 判本树
    RELEASE_RULER_ROOT=<别的树> python3 evidence/release-ruler.py   # 死值验：对着变异过的副本跑
    RELEASE_WORKFLOW=<某份 release.yml> python3 …          # 只换被测的那一份

退出码 0 = 过；1 = 有违例 / 地板没过 / 切块器坏了。
最后一行恒印 `release-gate: <N> passed（…）`，那个 `N` = 上面逐行印出来的 `PASS` 条数。
"""
import collections
import importlib.util
import json
import os
import posixpath
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# ⚠ `tomllib` 是 py≥3.11 的标准库（沙箱镜像与本机现打都有；本文件刻意不引第三方 ——
#   PyYAML 那次的账住头注）。**没有它 ⇒ ⑭/⑮ 那两组「判不了，按红记」**，
#   绝不退化成静默跳过（那正是「地板在『变少』方向上是瞎的」的一个变形）。
try:
    import tomllib
except ImportError:                                     # pragma: no cover —— py < 3.11
    tomllib = None

ROOT = Path(os.environ.get("RELEASE_RULER_ROOT") or Path(__file__).resolve().parents[2])
TARGET = Path(os.environ.get("RELEASE_WORKFLOW") or (ROOT / ".github" / "workflows" / "release.yml"))

#: 🔴 **闭集只有这一个住址**：`release.yml` 里「发不发布」那个闸的字面，钉在这两个常量上。
CANON_ENV = "${{ github.event_name == 'push' || inputs.publish == true }}"
CANON_IF = "env.PUBLISH == 'true'"

#: 正文生成器的住址（相对仓根）。`release.yml` 两处发布步骤的 `body_path` 都由它产出。
# `scripts/` 并进了 `tests/scripts/`。`release.yml` 已经改对，
# 是本守卫这个常量没跟 —— 而它红的那条诊断（「生成器不在盘上」）指的方向是对的。
RENDERER = "tests/scripts/release-notes.mjs"

# ══ ⑨–⑪：产字节那条路 ═══════════════════════════════════════════════════════
#
#: 平台**承诺面**住这份账本，本文件只 import、**不抄第二份**
#: （它是 `platform` 那一格的本体）。
LEDGER = "tests/evidence/platform-ledger.py"
#: 内嵌那一段的住址 —— ⑪ 读它，确认「抠不到 `BUILD_ID`」是一条响亮的失败。
BUILD_RS = "src/frontend/shell/build.rs"

#: 🔴 **本文件里全部「跑 cargo 编后端」的步骤的完整登记** —— 与盘上现打**两向集合相等**。
#: 失效方向逐字：有人给一个**没承诺的平台**加一条交叉编译（例：arm64 的 Windows），
#: 而今天的输出面上一个字都不会说 ⇒ 那份跑不起来的字节会被静默发出去（`D7` 反面）。
#: 每条：(job, 步骤名逐字, 这一趟编出什么, 它喂给承诺面的哪一格)
COMPILE_STEPS = [
    ("build-backends", "Cross-compile backend for both musl targets",
     "两个 musl target 的静态字节（x86_64 ＋ aarch64）",
     "远端 Linux（musl 两个 arch）· 本机 Linux（同一份字节自释放，不另编）"),
    # 本机后端 ＋ 文件窗口程序（文件窗口包 `cc-monitor-filewin` 自己的 `[[bin]]`，内嵌进 monitor；开窗时放到 `~/.cc-monitor/bin/` 再起）
    #   一步编完、铺进 `native-backend/`（`re-embed.sh --stage-native`）。
    ("build-windows", "Build and stage native backend + filewin",
     "runner host triple 的原生 `cc-monitor-backend.exe` ＋ `cc-monitor-filewin.exe`",
     "本机 Windows x86_64（本机后端 · 文件窗口，都内嵌进 `cc-monitor.exe`）"),
    ("build-linux", "Build and stage native filewin",
     "runner host triple 的原生 glibc `cc-monitor-filewin`（链 GTK，musl 那两份替不了它）",
     "本机 Linux x86_64（文件窗口；内嵌进裸 `cc-monitor`）"),
    # 本机 Linux 不另编后端：安装包旁边不放 glibc 后端（`externalBin`），
    #   本机 Linux 只用 musl 那一份自释放（`BYTE_LINES` 第三格）。
]

#: 🔴 **承诺面的每一格，它的字节由本文件哪几步产出/铺到 `build.rs` 够得着的地方。**
#: `plat` 必须与账本 `PROMISED` 里那几个平台名**逐字相同**（⑨a 两向集合相等靠它）。
#: `steps` 里每一项 `(job, 步骤名)` 在那个 job 里 `count() == 1`（⑨c）。
BYTE_LINES = [
    {
        "plat": "本机 Windows x86_64",
        "runner": ("build-windows", "windows-latest"),
        "steps": [
            # 本机后端与文件窗口程序两件字节，同一步编、铺。
            ("build-windows", "Build and stage native backend + filewin"),
        ],
        "into": "`build.rs::embed_native_backend` 的 `include_bytes!` ⇒ **进 exe 本体**，"
                "起的时候放到 `~/.cc-monitor/bin/ccm` 再跑；安装包旁边不另放一份。"
                "文件窗口程序经 `build.rs::embed_native_filewin` 进 exe 本体",
    },
    {
        "plat": "远端 Linux（musl 两个 arch）",
        "runner": ("build-backends", "ubuntu-latest"),
        "steps": [
            ("build-backends", "Cross-compile backend for both musl targets"),
            ("build-backends", "Stage binaries"),
            ("build-windows", "Place + verify embedded backends"),
        ],
        "into": "artifact `embedded-backends` → `src/frontend/shell/embedded-backends/` → "
                "`build.rs::embed_backends` 的 `include_bytes!` ⇒ 进 exe 本体",
    },
    {
        # 本机 Linux 只承诺 x86_64（这一格的 runner 就是 x86_64 的 `ubuntu-latest`）；
        #   本机 (Linux, aarch64) 不承诺 —— 名字逐字与账本 `PROMISED` 同改（⑨a 两向靠它）。
        "plat": "本机 Linux x86_64",
        "runner": ("build-linux", "ubuntu-latest"),
        "steps": [
            ("build-linux", "Place embedded backends"),
            # 文件窗口程序：这一格唯一另编的一份（原生 glibc，链 GTK）。
            ("build-linux", "Build and stage native filewin"),
        ],
        "into": "后端**不另编一份**（待点① 逐字：`local_daemon.rs::start_local_backend` 里"
                "那道 `cfg!(target_os = \"linux\")` 闸让本机 Linux 直接用远端那两份 **musl 静态**字节自释放）"
                "⇒ 这一件的产线增量是 **0**，它要的是**门禁多一格 ＋ 一次真机验**。"
                "文件窗口程序另编一份，经 `build.rs::embed_native_filewin` 进裸 `cc-monitor` 本体。"
                "⚠ 真机验这一维本判据**买不到**",
    },
]

#: 🔴 **本文件里出现的全部 target triple，与登记的那一份两向集合相等。**
#: 它是「没覆盖的格子怎么显式拒绝」落在**产线**这一侧的形状：承诺面显式拒绝的那一格
#: （arm64 的 Windows）在全仓是零脚印（`platform` 那一格的 `P3` 在盯），
#: 而本条盯的是**别的**没承诺的 triple 被悄悄加进产线。
TRIPLES = {"x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"}

#: 🔴 **每一处抠后端源码常量的住址**（路径, 常量名）→ 出现次数。**Counter 相等，不是包含。**
#: 它治的那一形：`BUILD_ID` 换了住处，`release.yml` 里**两处**要抠它，只改了一处 ⇒ 另一处指着一份**没有那个 const**
#: 的文件，真发版时死在「抠不到后端的 const BUILD_ID」上，而红的原因与它要守的事无关。
#: ⇒ 本条不只比登记，还**实打去读那份文件**，抠不出恰好一行就红（⑩b）。
IDENTITY_READS = {
    ("src/backend/common/id.rs", "BUILD_ID"): 2,
    ("src/common/deploy-contract/src/lib.rs", "STAMP_OPEN"): 1,
    ("src/common/deploy-contract/src/lib.rs", "STAMP_CLOSE"): 1,
}

# ══ ⑬：`BUILD_ID` bump 的同拍债 —— re-embed ═══════════════════════════════════
#
# 🔴 **它为什么也归这一格，而不是新开第 25 格**
#
# ⑨–⑪ 收的是「产字节那条路」：⑪ 读 `src/frontend/shell/build.rs`（吃字节那一侧）。⑬ 收的是同一条路上
# **产字节**那一端：一条 re-embed 命令，musl 那两份的配方只住它，`release.yml` 那一步只调它的 `--build-only`
# —— 判它就得同时读那两份文本，而「同时读那两份」正是本格的定义。
# ⇒ 本组判据与 ⑨⑩⑪ 共用同一个被测面，拆出去就得把 `release.yml` 的解析再抄一份。
#
# 🔴 **它治的病**：协议面一变就要 bump `BUILD_ID`，
# 而 bump 那一刻 `embedded-backends/` 里那两份 musl 字节立刻变旧 ⇒ `build.rs` 的
# **半 bump 守卫**当场 panic，**整棵树编不过**。
# 今天盘上有一张 mtime 安全网，但那**只是安全网不是机制** —— 机制那一半是
# `tests/scripts/re-embed.sh`：一条真跑得起来的命令 ＋ 一条 `--check`。
#
# ⚠ **本组买不到什么**（与本文件头注第 4 条同一条边界，别读宽）：
#   · ⑬a–⑬e 全是**盘上文本**的对拍；⑬i 真跑 `--build-only` · `--stage-*` 用的是一个只记账的 `cargo` 替身 ——
#     「配方对」≠「那条命令今天在这台机器上跑得出字节」（它要 `cargo-zigbuild` ＋ `zig`，本判据一次都不装、不编）。
#   · ⑬g 真跑的是 `--check`（**只读**），不是 re-embed 本身。它在一棵**没铺字节**的树上
#     只答得出「这里没有一份对不上的字节」——那不是「字节是对的」。
#   · 「本机重编过了」**永远不等于**「发版办完了」：本机产物与发版 CI 的 zigbuild
#     形态不同（`build.rs` 自陈：static-pie / 未 strip / 不同 rustc）。

#: re-embed 那条命令的本体。
REEMBED_SH = "tests/scripts/re-embed.sh"
#: 身份戳界标的住址（契约 crate；re-embed 与 `release.yml` 都从这一份抠）。
MARKS_SRC = "src/common/deploy-contract/src/lib.rs"
#: 落点的 gitignore 住址。
BRIDGE_GITIGNORE = "src/frontend/shell/.gitignore"
#: 🔴 `build.rs` 里那个**出路唯一住址**常量的值，逐字。
#: 它指的脚本必须在盘上 —— 「出路指着一个不存在的命令」与「没有出路」在文案上一模一样。
REEMBED_CMD = "bash tests/scripts/re-embed.sh"
#: ⑬i：`release.yml` 里编字节的每一步（与 `COMPILE_STEPS` 同一批）→ 它逐字只调的那个口（配方只住 re-embed.sh）。
REEMBED_STEPS = {
    ("build-backends", "Cross-compile backend for both musl targets"): "--build-only",
    ("build-windows", "Build and stage native backend + filewin"): "--stage-native",
    ("build-linux", "Build and stage native filewin"): "--stage-filewin",
}
#: ⑬d 注册侧：`build.rs` 里登记落点目录名的那两个常量。
LANDING_CONSTS = ["EMBEDDED_BACKENDS_DIR", "NATIVE_BACKEND_DIR"]
#: ⑬d 没有 `binaries/` 那个落点（Tauri `externalBin` 的暂存区）：安装包不旁放本机后端。
#: ⑬d 盘侧：`.gitignore` 里每一行落点上面那句机检锚，逐字。
LANDING_MARK = "# ⇐ 内嵌落点"
#: ⑬e：这两个函数体里的**出路**必须点名 `REEMBED_CMD`，各至少两处（重编 ＋ 诚实关闭）。
EXIT_PATH_FNS = ["embed_backends", "embed_native_backend", "embed_native_filewin"]
#: ⑬e 禁词 —— 「手抄第二条产字节配方」回来的样子。**只在代码行上判，注释行剥掉**
#: （注释行里提到那段配方是说明，不是指令；剥法见 `rust_code_lines`）。
BANNED_RECIPE = [
    ("--config 'target.", "把 `rust-lld` 那条本机专用配方抄回出路里 —— 它与发版那趟**不是同一条路**"),
    ("cargo build --release --target", "同上：第二条产字节配方 ＝ 第二个住址"),
    ("cargo zigbuild --target", "半条配方（漏了 `--release --locked`）⇒ 照它敲出来的字节不是发版那份"),
]

#: ⑪ 「半 bump」的发版核对（monitor 的「我这一版」只认内嵌字节自报的 id，`build.rs` 不读后端源码）：
#: 本文件把两份 musl 字节对上源码 `BUILD_ID`（`VERIFY_STEP`）；`build.rs` 要求几份内嵌字节彼此同一版
#: （`AGREE_FN`：不等当场 panic，函数体里不许有兜底值）⇒ 产物里三份字节 id 相等且 == `BUILD_ID`。
VERIFY_STEP = ("build-windows", "Place + verify embedded backends")
AGREE_FN = "emit_embedded_id"
AGREE_BANNED = ['"unknown"', "unwrap_or_default()"]

# ══ ⑭ 本包的每一个 `[[bin]]` ↔ 安装包的清单 ═════════════════════
#
# 🔴 **它守的是一个容易想错的事实。** 直觉是「Tauri 安装包只装主二进制与 `externalBin` sidecar ⇒ 装机那份上
# `resolve_window_bin()` 会找不到文件窗口程序」。
#
# **那是错的。** 两趟实测（用的是本仓这份 `src/frontend/shell/Cargo.toml` ＋
# `tauri.conf.json` ＋ `tauri.sidecar.conf.json`，`@tauri-apps/cli` 现锁的 2.11.2；
# 二进制喂的是占位字节 —— 本条问的是**清单**，不是那几个字节能不能跑）：
#   · 真 `.deb`（`npx tauri bundle --bundles deb`）：`dpkg-deb -c` 现打 `usr/bin/` 三份 ——
#     `monitor` · **`cc-monitor-filewin`** · `cc-monitor-backend`，**同一个目录**；
#   · NSIS 那份清单（`--target x86_64-pc-windows-msvc` 生成的 `installer.nsi`）现打逐字
#     `File /a "/oname=cc-monitor-filewin.exe" …`，与 `${MAINBINARYNAME}.exe`（= `monitor`）
#     同落 `$INSTDIR`。
# ⇒ **`resolve_window_bin()` 的「exe 旁那一份」这一支在装机布局下命中。**
#
# 机制（读的是 `tauri-cli` 2.11.2 与 `tauri-bundler` 2.9.x 的源码，不是推论）：
#   `tauri-cli` 的 `interface/rust.rs` 里那个 `get_binaries` 把 Cargo.toml 里**每一个** `[[bin]]` 都交给打包器
#   （`name == 包名` 或 `default-run` 的那个 `is_main = true`，其余 `false`）；而
#   `windows/nsis/mod.rs` · `windows/msi/mod.rs` · `linux/debian.rs` 三处都写着
#   `for bin in settings.binaries() { if !bin.main() { …和主二进制装在一处… } }`。
#
# 🔴 于是这一组要守的性质，与「补一条 `externalBin`」是两回事：
#   「第二个 `[[bin]]` 进不进安装包」**本仓一个字都没配** —— 它由那把工具的行为决定
#   ⇒ 立得住的判据是：**每一个 `[[bin]]` 都得有一条登记过的入包路线**（⑭a，两向集合相等）、
#   那条路线的**依据**（工具版本）钉住（⑭f）、消费侧找它的那个名字与产出侧**逐字同源**（⑭e）、
#   带 `required-features` 的 bin **零命中**（⑭c，那一形会被工具静默跳过）、
#   以及「别顺手把它加进 `externalBin`」这条反面（⑭d）。
#
# ⚠ **本组买不到什么**（与本文件头注第 4 条同一条边界，别读宽）：
#   · 它读的全是**盘上的文本**。「装机后它真在那儿」这一维，全仓唯一对着**真字节**问过的
#     地方是 `release.yml` 的 `The .deb really ships every binary this package declares`
#     那一步 —— 而它**只在发版那一趟跑**，且只覆盖 `.deb`。
#   · **Windows 那两个安装包一趟都没验过**：上面那条 `installer.nsi` 读数是**生成出来的
#     安装脚本**，不是「装完之后 `C:\Program Files\cc-monitor\` 里真有那个文件」。
#     没有 Windows 机器、也不跑 NSIS/WiX ⇒ 那一格仍然是**判不了**，不是「通过」。
#: 本包的 manifest —— `[[bin]]` 的唯一住址。
BRIDGE_CARGO = "src/frontend/shell/Cargo.toml"
#: `externalBin` 的唯一住址（刻意不在 `tauri.conf.json` 里，理由住 `release.yml` 那一步）。
SIDECAR_CONF = "src/frontend/shell/tauri.sidecar.conf.json"
#: 打包工具版本的唯一住址（两个 job 的 `npm install` / `npm ci` 都照它装）。
PACKAGE_LOCK = "package-lock.json"
CLI_LOCK_KEY = "node_modules/@tauri-apps/cli"
#: 🔴 上面那条「非主 `[[bin]]` 也装」的机制，**依据就是这一版的行为**。升级它 ⇒ ⑭f 红
#: ⇒ 那时要重读 `get_binaries` 与三个打包器里那三处 `if !bin.main()`，再动这个数。
CLI_VERSION = "2.11.2"
#: 🔴 本包每一个 `[[bin]]` ↔ 它进安装包的路线。⑭a 与 Cargo.toml 现打**两向集合相等**。
#: `consumer` = 装机那份上「谁去找它」的那个文件名主干常量的住址（⑭e 逐字对拍）。
BIN_SHIPPING = {
    "cc-monitor": {
        "route": "主二进制",
        "into": "打包器的 main binary（`package.default-run` 点名它；包名仍是 `monitor`）：NSIS/MSI 落 "
                "`$INSTDIR\\cc-monitor.exe`（`installer.nsi` 的 `MAINBINARYNAME` 跟它走）；"
                "deb 落 `/usr/bin/cc-monitor`（`.desktop` 的 `Exec` 跟它走）",
        "consumer": None,
    },
}
#: 🔴 `externalBin` 那几项 ↔ 登记，两向集合相等（⑭d）。键 = 那个名字的住址（`(文件, const)`，实打去读，不抄名字）。
#: 🔴 这一张是**空的**：安装包旁边不放第二份本机后端 —— 它与内嵌进 exe 的那份逐字节相同，
#:   而本机后端无论如何都先放到 `~/.cc-monitor/bin/ccm` 再跑（`local_backend::resolve_or_extract`），旁边那份只是字节来源。
#:   ⇒ ⑭d 判「`externalBin` 零命中」；要加回来，先答「那份字节谁用、为什么不是内嵌那份」。
FOREIGN_SIDECARS = {}

# ══ ⑮ 产物路径的根 ↔ 声明的 `target-dir` ════════════════════════
#
# 🔴 **它守的那一形：** cargo 工程挪了目录、或仓根 `.cargo/config.toml` 改了 `target-dir`
# （今天是 `[build] target-dir = ".build/shell"`），`release.yml` 里的产物路径没跟
# —— 发版之间隔得久，**没人踩得到**。
# 它坏得完全静默：三个 glob 全空 → `Get-ChildItem … -ErrorAction SilentlyContinue` 吞掉
# 「路径不存在」→ `SHA256SUMS.txt` 空着照发 → `action-gh-release` 的 `files:` 一条都不匹配、
# 而 `fail_on_unmatched_files` 默认 `false` ⇒ **Release 建得出来，上面一个安装包都没有。**
# ⇒ 本条把「产物根」收成一个**两向集合相等**：`release.yml` 里每一处 cargo 产物路径的根
#   ↔ 两份**进 git 的** `.cargo/config.toml` 声明的 `target-dir`。
#
# ⚠ 边界：本机个人的 `src/frontend/shell/.cargo/`（被 `.gitignore` 挡着、不进 CI）**不在射程**；
#   本条也不问「那个目录在 runner 上真有东西」—— 那仍然是发版那一趟才知道的事。
CARGO_CONFIGS = [
    ".cargo/config.toml",
    "src/backend/.cargo/config.toml",
]
#: ⑮ 的口径：路径里有一段**恰好**是这些之一 ⇒ 它是一条 cargo 产物路径。
PROFILE_SEGS = {"release", "debug"}
#: 交叉编译时 cargo 会在档位前多插一层 `<triple>/` ⇒ 剥掉它，剩下的才是产物根。
TRIPLE_SEG = re.compile(r"^[a-z0-9_]+-(?:pc|unknown|apple)-")


def read_rel(rel):
    """读仓内一份文件；读不到给 `None`（调用方按红记，不静默跳过）。"""
    try:
        return (ROOT / rel).read_text(encoding="utf-8")
    except OSError:
        return None


def promised_platforms():
    """承诺的平台集合 —— **唯一住址是那份账本**，本文件不抄。

    读不到 / 导不进来一律给 `None`，调用方按红记（不许退化成「空集，于是两向相等」）。
    """
    p = ROOT / LEDGER
    if not p.exists():
        return None
    try:
        spec = importlib.util.spec_from_file_location("k_g4_platform_ledger", p)
        mod = importlib.util.module_from_spec(spec)
        # ⚠ 账本模块顶层会读 `sys.argv[1]`（它自己的死值验入口）。本文件带参数跑时
        #   那个参数与账本无关 ⇒ 借跑期间把 argv 收成一个元素，跑完还回去。
        saved, sys.argv = sys.argv, sys.argv[:1]
        try:
            spec.loader.exec_module(mod)
        finally:
            sys.argv = saved
        return {plat for plat, _, _, _ in mod.PROMISED}
    except Exception as e:                      # noqa: BLE001 —— 导不进来就是判不了，按红记
        return ("导入失败", repr(e))


def rust_fn_body(src, name):
    """抠出 `fn <name>(` 那一对大括号之间的正文。找不到给 `None`。"""
    m = re.search(r"^fn %s\b" % re.escape(name), src, re.M)
    if not m:
        return None
    i = src.find("{", m.start())
    if i < 0:
        return None
    depth, j = 0, i
    while j < len(src):
        if src[j] == "{":
            depth += 1
        elif src[j] == "}":
            depth -= 1
            if depth == 0:
                return src[i:j + 1]
        j += 1
    return None


def rust_code_lines(body):
    """把一段 Rust 正文里**整行的 `//` 注释**剥掉，只留代码行。

    ⚠ 它**不是** Rust 词法器：行尾注释、`/* */`、字符串里的 `//` 一概不处理。
    够用的理由是窄的 —— ⑬e 的禁词只出现在两处：① 出路那几段**字符串字面量**里
    （那正是要判的），② 讲那段配方的**整行注释**里（那是说明，不是指令）。剥掉整行注释刚好把这两者分开。
    """
    return "\n".join(l for l in body.splitlines() if not l.lstrip().startswith("//"))


def rust_str_const(src, name):
    """抠 `const <name>: &str = "…";` 的值。抠不到给 `None`（调用方按红记）。"""
    m = re.search(r'^\s*(?:pub )?const %s: &str = "([^"]+)";' % re.escape(name), src, re.M)
    return m.group(1) if m else None


def toml_load(rel):
    """读仓内一份 TOML。没 `tomllib` / 读不动 / 解不动，一律给 `None`（调用方按红记）。"""
    if tomllib is None:
        return None
    try:
        with (ROOT / rel).open("rb") as f:
            return tomllib.load(f)
    except (OSError, ValueError):
        return None


def json_load(rel):
    """读仓内一份 JSON。读不动 / 解不动给 `None`（调用方按红记）。"""
    try:
        with (ROOT / rel).open("rb") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def cargo_bins(doc):
    """Cargo.toml 里 `[[bin]]` 的 `[(名字, required-features)]`。解不出给 `None`。

    ⚠ `required-features` 这一栏是承重的（⑭c）：`tauri-cli::get_binaries` 对
    「要求的 feature 这一趟没开」的 bin 是 `continue` —— **静默不进包**，一个字都不说。
    """
    if not isinstance(doc, dict):
        return None
    bins = doc.get("bin")
    if not isinstance(bins, list):
        return None
    out = []
    for b in bins:
        if isinstance(b, dict) and isinstance(b.get("name"), str):
            out.append((b["name"], b.get("required-features")))
    return out


def external_bin_stems(doc):
    """`tauri.sidecar.conf.json` 的 `bundle.externalBin` → 落点主干名集合（没有这一键 = 空集）。解不出给 `None`。

    口径与 Tauri 一致：条目是**路径**，打包时它找 `<路径>-<triple>[.exe]`、装进去时把
    `-<triple>` 剥掉 ⇒ 装机后的文件名就是这里的最后一段（现打：`binaries/cc-monitor-backend`
    ⇒ `$INSTDIR\\cc-monitor-backend.exe` / `/usr/bin/cc-monitor-backend`）。
    """
    if not isinstance(doc, dict):
        return None
    ext = ((doc.get("bundle") or {}) if isinstance(doc.get("bundle"), dict) else {}).get("externalBin", [])
    if not isinstance(ext, list) or not all(isinstance(p, str) for p in ext):
        return None
    return {p.rsplit("/", 1)[-1] for p in ext}


def declared_target_dirs():
    """两份**进 git 的** `.cargo/config.toml` 声明的 `target-dir` → {住址: 相对仓根的路径}。

    抠不到的那一份给一个说明串 —— 它必然不等于任何真路径 ⇒ 落在 ⑮ 的红那一侧，
    **不许**当成「这一份没声明，于是不算」（那就又是一条空真）。
    """
    out = {}
    for rel in CARGO_CONFIGS:
        doc = toml_load(rel)
        td = (doc or {}).get("build")
        td = td.get("target-dir") if isinstance(td, dict) else None
        if not isinstance(td, str):
            out[rel] = "<`[build] target-dir` 抠不到：%s>" % ("没 tomllib" if tomllib is None else "键不在")
            continue
        # 相对路径以**那份配置的上级的上级**为基准（形状恒为 `<base>/.cargo/config.toml`）。
        base = posixpath.dirname(posixpath.dirname(rel))
        out[rel] = posixpath.normpath(posixpath.join(base, td))
    return out


def prod_path_roots(steps):
    """`release.yml` 每一步里出现的 **cargo 产物根** → {根: [原样的那几个 token]}。

    口径窄得刻意（宽了就会把不是产物的路径也判进来）：
      · 只看每一步的 `run:` 与 `with:` 的值。**注释不在里面** —— 头注里逐字引着搬树前的
        旧路径，那是账不是指令；`run:` 块里整行的 `#` 也在这里剥掉；
      · Windows 那几步混用反斜杠 ⇒ 先把 `\\` 归一成 `/`；
      · 只认「有一段**恰好**是 `release`/`debug`」的 token ⇒ `staged/…`、
        `backend-artifacts/…`、`src/frontend/shell/binaries/…`、`src/frontend/shell/embedded-backends/…`
        这些不是 cargo 产物，本条一个都不碰。
    """
    out = collections.defaultdict(list)
    blobs = []
    for _, _, st in steps:
        run = str(st.get("run") or "")
        blobs.append("\n".join(l for l in run.splitlines() if not l.lstrip().startswith("#")))
        with_ = st.get("with")
        if isinstance(with_, dict):
            blobs.extend(str(v) for v in with_.values())
    for blob in blobs:
        for raw in re.split(r"[\s\"'`,|()=;]+", blob.replace("\\", "/")):
            segs = [s for s in raw.split("/") if s]
            hit = next((i for i, s in enumerate(segs) if s in PROFILE_SEGS), None)
            if hit is None or hit == 0:
                continue
            pre = segs[:hit]
            # 档位前面那一层是「给哪个 target 编的」：literal triple（`aarch64-unknown-…`）
            # 或一个 shell 变量（`$t` / `$triple`）。两形都剥，剩下的才是产物根。
            if len(pre) > 1 and (TRIPLE_SEG.match(pre[-1]) or pre[-1].startswith("$")):
                pre = pre[:-1]
            out["/".join(pre)].append(raw)
    return out


def sh_array(text, name):
    """抠 shell 里 `NAME=(a b c)` 的元素列表。抠不到给 `None`（调用方按红记）。"""
    m = re.search(r"^%s=\(([^)]*)\)\s*$" % re.escape(name), text, re.M)
    return m.group(1).split() if m else None


def gitignore_landings(text):
    """`.gitignore` 里带 `⇐ 内嵌落点` 锚的那几行，剥成目录名集合。

    口径：锚那一行之后的**第一条非注释非空行**就是它标的那条 pattern。
    ⚠ 锚与 pattern 之间允许夹别的注释（那几行落点各自带一大段理由）。
    """
    out, lines, i = [], text.splitlines(), 0
    while i < len(lines):
        if lines[i].strip() == LANDING_MARK:
            j = i + 1
            while j < len(lines) and (not lines[j].strip() or lines[j].lstrip().startswith("#")):
                j += 1
            out.append(lines[j].strip() if j < len(lines) else "<锚后面没有 pattern>")
        i += 1
    return {p.strip("/") for p in out}


def ps_identity_reads(text, doc):
    """本文件里每一处 `Select-String -Path <p> -Pattern 'const <X>…'` 解出的 (路径, 常量名)。

    路径三种形：字面量 · `$env:NAME`（查 workflow 的 `env:`）· `$var`（查同文件里那条赋值）。
    解不出来的**照样登记**，值写成 `<解不出: …>` —— 它会让下面的 Counter 对不上而红，
    **不会**被悄悄丢掉（丢掉 = 少一处抽取点没人看，正是本条要治的形状）。
    """
    env = {k: v for k, v in (doc.get("env") or {}).items() if isinstance(v, str)}
    assigns = {}
    for m in re.finditer(r"\$(\w+)\s*=\s*(\"[^\"]*\"|\$env:\w+)", text):
        assigns.setdefault(m.group(1), m.group(2))

    def resolve(raw):
        raw = raw.strip()
        if raw.startswith('"') and raw.endswith('"'):
            return raw[1:-1]
        if raw.startswith("$env:"):
            return env.get(raw[5:], "<解不出: env.%s 不在 workflow 的 env: 里>" % raw[5:])
        if raw.startswith("$"):
            v = assigns.get(raw[1:])
            if v is None:
                return "<解不出: 找不到 %s 的赋值>" % raw
            return resolve(v)
        return raw

    out = collections.Counter()
    for m in re.finditer(r"-Path\s+(\S+)\s+-Pattern\s+'const\s+(\w+)\b", text):
        out[(resolve(m.group(1)), m.group(2))] += 1
    return out


def source_const_hits(rel, name):
    """盘上那份源码里 `(pub )?const <name>: &str = "…";` 命中几行。读不到给 `None`。"""
    src = read_rel(rel)
    if src is None:
        return None
    return len(re.findall(r"^\s*(?:pub )?const %s: &str = \"[^\"]+\";" % re.escape(name),
                          src, re.M))


# ── YAML 子集切块器 ──────────────────────────────────────────────────────────
_BLOCK_SCALAR = {"|", ">", "|-", ">-", "|+", ">+"}
_KEY_RE = re.compile(r"^([^:]+):(?:\s+(.*))?$")


def _strip_comment(line):
    """剥掉**引号外**的行内注释。块标量的正文不走这条路（`files: |` 里 `#` 不是注释）。"""
    out = []
    quote = None
    for i, ch in enumerate(line):
        if quote:
            out.append(ch)
            if ch == quote:
                quote = None
            continue
        if ch in "\"'":
            quote = ch
            out.append(ch)
            continue
        if ch == "#" and (i == 0 or line[i - 1] in " \t"):
            break
        out.append(ch)
    return "".join(out).rstrip()


def _scalar(raw):
    raw = raw.strip()
    if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in "\"'":
        return raw[1:-1]
    if raw == "true":
        return True
    if raw == "false":
        return False
    if raw in ("", "null", "~"):
        return None
    if re.fullmatch(r"-?\d+", raw):
        return int(raw)
    return raw


def _indent_of(s):
    return len(s) - len(s.lstrip())


def _read_block_scalar(lines, i, parent_indent):
    body, block_indent = [], None
    while i < len(lines):
        raw = lines[i]
        if raw.strip() == "":
            body.append("")
            i += 1
            continue
        ind = _indent_of(raw)
        if ind <= parent_indent:
            break
        if block_indent is None:
            block_indent = ind
        body.append(raw[block_indent:] if len(raw) >= block_indent else raw.lstrip())
        i += 1
    while body and body[-1] == "":
        body.pop()
    return ("\n".join(body) + "\n") if body else "", i


def _first_content(lines, i, indent):
    while i < len(lines):
        s = _strip_comment(lines[i])
        if not s.strip():
            i += 1
            continue
        if _indent_of(s) < indent:
            return None, i
        return s, i
    return None, i


def _parse_block(lines, i, indent):
    s, i = _first_content(lines, i, indent)
    if s is None:
        return None, i
    if s.lstrip().startswith("- "):
        return _parse_seq(lines, i, _indent_of(s))
    return _parse_map(lines, i, _indent_of(s))


def _parse_map(lines, i, indent):
    out = {}
    while i < len(lines):
        s = _strip_comment(lines[i])
        if not s.strip():
            i += 1
            continue
        ind = _indent_of(s)
        if ind < indent:
            break
        if ind > indent:          # 子块已被递归吃掉，剩下的只可能是切块器够不着的形状
            i += 1
            continue
        body = s.strip()
        if body.startswith("- "):
            break
        m = _KEY_RE.match(body)
        if not m:
            i += 1
            continue
        key = _scalar(m.group(1))
        rest = (m.group(2) or "").strip()
        i += 1
        if rest in _BLOCK_SCALAR:
            val, i = _read_block_scalar(lines, i, indent)
        elif rest == "":
            val, i = _parse_block(lines, i, indent + 1)
        else:
            val = _scalar(rest)
        out[key] = val
    return out, i


def _parse_seq(lines, i, indent):
    out = []
    while i < len(lines):
        s = _strip_comment(lines[i])
        if not s.strip():
            i += 1
            continue
        ind = _indent_of(s)
        if ind < indent:
            break
        if ind > indent:
            i += 1
            continue
        body = s.strip()
        if not body.startswith("- "):
            break
        inner = body[2:]
        if _KEY_RE.match(inner):
            # `- key: v` ⇒ 把那个短横换成两个空格，整项当一份缩进 +2 的映射解析。
            patched = list(lines)
            patched[i] = lines[i][:ind] + "  " + lines[i][ind + 2:]
            item, i = _parse_map(patched, i, ind + 2)
            out.append(item)
        else:
            out.append(_scalar(inner))
            i += 1
    return out, i


def parse_workflow(text):
    doc, _ = _parse_block(text.split("\n"), 0, 0)
    return doc if isinstance(doc, dict) else {}


# ── ⑯ Release 资产登记 ──────────────────────────────────────────────────────
#: 资产登记的唯一住址：`RELEASING.md` §2.2 两个标记之间那张表（对外的资产名）。
ASSET_DOC = "src/doc/RELEASING.md"
ASSET_BEGIN = "<!-- 发版资产登记：起 -->"
ASSET_END = "<!-- 发版资产登记：止 -->"
#: 对外契约：远端后端两个 musl 目标，资产名 `cc-monitor-backend-<arch>`。
BACKEND_ASSET_PREFIX = "cc-monitor-backend-"
MUSL_ARCHES = {"x86_64", "aarch64"}
STAGE_STEP = ("build-backends", "Stage binaries")


def asset_registry():
    """登记表 → [(资产名, job, 校验和文件名或 None)]；表抠不出给 `None`。"""
    text = read_rel(ASSET_DOC)
    if not text or text.count(ASSET_BEGIN) != 1 or text.count(ASSET_END) != 1:
        return None
    body = text.split(ASSET_BEGIN, 1)[1].split(ASSET_END, 1)[0]
    rows = []
    for line in body.splitlines():
        cells = [c.strip().strip("`") for c in line.strip().strip("|").split("|")]
        if len(cells) != 4 or cells[0] in ("资产", "") or set(cells[0]) <= set("-"):
            continue
        rows.append((cells[0], cells[1], None if cells[2] in ("—", "-", "") else cells[2]))
    return rows


def leaf(path):
    """路径的最后一段（`/` 与 `\\` 都认：Windows 那一步写反斜杠）。"""
    return re.split(r"[\\/]", path.strip().strip('"'))[-1]


def match_both_ways(globs, names):
    """glob 叶名 ↔ 登记名，一一对上才算：回 (没人认的 glob, 没 glob 认的名, 认了不止一个的)。"""
    import fnmatch
    hits = {g: [n for n in names if fnmatch.fnmatchcase(n, g)] for g in globs}
    lone_globs = sorted(g for g, h in hits.items() if not h)
    multi = sorted(g for g, h in hits.items() if len(h) > 1)
    covered = collections.Counter(n for h in hits.values() for n in h)
    lone_names = sorted(n for n in names if covered[n] != 1)
    return lone_globs, lone_names, multi


def hashed_files(run):
    """校验和那一步算的那几份（叶名）：bash 的 `for f in …; do` 或 pwsh 的 `$want = @(…)`。"""
    m = re.search(r"^\s*for f in (.+?); do\s*$", run, re.M)
    if m:
        return [leaf(t) for t in m.group(1).split()]
    m = re.search(r"\$want = @\((.+?)\)", run)
    if m:
        return [leaf(t) for t in re.findall(r'"([^"]+)"', m.group(1))]
    return None


# ── ⑰ 第三方许可声明 ────────────────────────────────────────────────────────
NOTICES = "THIRD-PARTY-NOTICES.txt"
NOT_SHIPPED = "tests/evidence/third-party-not-shipped.txt"
NOTICES_GEN = "tests/scripts/third-party-notices.py"
NOTICES_CHECK = "python3 %s --check" % NOTICES_GEN
CARGO_LOCKS = ["src/backend/Cargo.lock", "src/frontend/shell/Cargo.lock"]
SIDECAR_CONF = "src/frontend/shell/tauri.sidecar.conf.json"


def listed(text, kind):
    """声明 / 不进产物那份里 `<kind>: 名 版本` 的行 → 集合（`名 版本`）。"""
    return set(re.findall(r"^\s*%s: (\S+ \S+)\s*$" % kind, text or "", re.M))


def locked_third_party():
    """两份 `Cargo.lock` 里有来源（注册表 / git）的包；读不到给 `None`。"""
    got = set()
    for rel in CARGO_LOCKS:
        text = read_rel(rel)
        if text is None:
            return None
        for name, ver, src in re.findall(
                r'\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"\n(?:source = "([^"]+)")?', text):
            if src:
                got.add("%s %s" % (name, ver))
    return got


def npm_production():
    """`package-lock.json` 里不带 `dev` 的包（`名 版本`）；读不到给 `None`。"""
    text = read_rel("package-lock.json")
    if text is None:
        return None
    pk = json.loads(text).get("packages") or {}
    return {"%s %s" % (k.rsplit("node_modules/", 1)[-1], v.get("version"))
            for k, v in pk.items() if k and not v.get("dev")}


MAVEN_LOCK = "src/mobile/app/gradle.lockfile"
MAVEN_CONF = "releaseRuntimeClasspath"
ABOUT_CONF = "tests/scripts/third-party-about.toml"


def maven_release_locked():
    """手机 `:app` 的 Gradle 锁文件里进发布包那一档的坐标（`组:件 版本`）；读不到给 `None`。"""
    text = read_rel(MAVEN_LOCK)
    if text is None:
        return None
    return {"%s:%s %s" % (g, a, v) for g, a, v, confs in re.findall(r"^([^#\s:]+):([^:\s]+):([^=\s]+)=(\S+)$", text, re.M)
            if MAVEN_CONF in confs.split(",")}


def maven_declared(notices):
    """声明里 Maven 那一段：`组:件 版本` → 它所在那一组的许可（组头 `-------- 名 (许可) --------`）。"""
    sec = re.search(r"^=+ Maven artifacts[^\n]*\n(.*?)(?=^=+ |\Z)", notices or "", re.M | re.S)
    got, lic = {}, None
    for line in (sec.group(1) if sec else "").splitlines():
        head = re.match(r"^-------- .* \((.+)\) --------$", line)
        if head:
            lic = head.group(1)
        m = re.match(r"^\s*maven: (\S+ \S+)\s*$", line)
        if m:
            got[m.group(1)] = lic
    return got


def accepted_licenses():
    """依赖许可的名单（cargo-about 配置的 `accepted`，Rust 与 Maven 同一份；只收 MIT / Apache 兼容的）。"""
    m = re.search(r"^accepted = \[(.*?)\]", read_rel(ABOUT_CONF) or "", re.M | re.S)
    return set(re.findall(r'"([^"]+)"', m.group(1))) if m else set()


# ── ⑱ 手机那一格（签名包）──────────────────────────────────────────────────
MOBILE_JOB = "build-mobile"
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
#: 签名材料只从这四个 GitHub secrets 来（`src/doc/RELEASING.md` §2.3）。
SIGN_SECRETS = {"ANDROID_KEYSTORE_B64", "ANDROID_KEYSTORE_PASSWORD", "ANDROID_KEY_ALIAS", "ANDROID_KEY_PASSWORD"}
#: 签名凭据那几样不许进仓：`.gitignore` 要挡住、`git ls-files` 里一份都不许有。
SIGN_FILES = ["src/mobile/keystore.properties", "src/mobile/release.jks", "src/mobile/release.keystore",
              "release.jks", "keystore.properties"]
SIGN_FILE_RE = re.compile(r"(^|/)(keystore\.properties|[^/]*\.jks|[^/]*\.keystore)$")


def secret_refs(text):
    """文本里 `secrets.ANDROID_…` 的名字（逐个出现，不去重）。"""
    return re.findall(r"secrets\.(ANDROID_[A-Z0-9_]+)", text or "")


def needs_closure(jobs, jname):
    """`jname` 经 `needs` 直接间接等着的 job 集合（不含它自己）。"""
    seen, stack = set(), [jname]
    while stack:
        need = (jobs.get(stack.pop()) or {}).get("needs") or []
        if isinstance(need, str):  # 自带的切块器把行内 `[a, b]` 留成字符串
            need = [x.strip() for x in need.strip("[]").split(",") if x.strip()]
        for n in need:
            if n not in seen:
                seen.add(n)
                stack.append(n)
    return seen


# ── 判据 ────────────────────────────────────────────────────────────────────
def run_checks(emit):
    fails = []
    passes = [0]

    def check(ok, label, detail):
        emit(("PASS  " if ok else "FAIL  ") + label + " :: " + detail)
        if ok:
            passes[0] += 1
        else:
            fails.append(label)

    if not TARGET.exists():
        emit("::error::读不到 %s —— 判不了，按红记" % TARGET)
        return 1, 0, ["地板·读得到 release.yml"]
    doc = parse_workflow(TARGET.read_text(encoding="utf-8"))
    on = doc.get("on")
    jobs = doc.get("jobs") or {}

    # ── 地板：先证够得到，再问有没有违例（否则「零违例」是空真）──────────────
    check(isinstance(on, dict) and len(on) > 0, "地板·触发器解析得出来",
          "on 的键 = %r" % (sorted(map(str, on)) if isinstance(on, dict) else on,))
    check(len(jobs) >= 4, "地板·job 数",
          "%d 个 job（分母 = release.yml 顶层 jobs 的键数）" % len(jobs))
    steps = []
    for jname, j in (jobs.items() if isinstance(jobs, dict) else []):
        for idx, st in enumerate(((j or {}).get("steps") or [])):
            if isinstance(st, dict):
                steps.append((jname, idx, st))
    check(len(steps) >= 20, "地板·step 总数",
          "%d 步（分母 = 全部 job 的 steps 逐条）—— 切块器坏了这个数会塌" % len(steps))
    if fails:
        emit("::error::地板没过 —— 后面的判据一律不算数")
        return 1, passes[0], fails

    # ── ① 触发得了吗 ────────────────────────────────────────────────────────
    wd = on.get("workflow_dispatch", "<缺>")
    check(wd != "<缺>", "①触发得了（有 workflow_dispatch）", "on.workflow_dispatch = %r" % (wd,))

    # ── ② 手工触发默认不发布 ─────────────────────────────────────────────────
    inputs = (wd or {}).get("inputs", {}) if isinstance(wd, dict) else {}
    pub = inputs.get("publish") if isinstance(inputs, dict) else None
    check(isinstance(pub, dict), "②有 publish 这个输入",
          "inputs 的键 = %r" % (sorted(inputs) if isinstance(inputs, dict) else inputs,))
    if isinstance(pub, dict):
        check(pub.get("type") == "boolean", "②publish 是 boolean", "type=%r" % (pub.get("type"),))
        check(pub.get("default") is False, "②publish 默认 false",
              "default=%r（这一格红 = 随手点一下就发一个版）" % (pub.get("default"),))

    # ── ③ 那个量只有一个住址，且字面就是钉住的那一份 ───────────────────────────
    env_pub = (doc.get("env") or {}).get("PUBLISH")
    check(env_pub == CANON_ENV, "③env.PUBLISH 与钉住的字面逐字相同", "盘上=%r" % (env_pub,))

    # ── ④ 每一处「往 Release 上写」都挂着那个闸 ─────────────────────────────
    pubsteps = []
    for jname, idx, st in steps:
        uses = str(st.get("uses") or "")
        run = str(st.get("run") or "")
        writes = uses.startswith("softprops/action-gh-release") \
            or "gh release" in run \
            or ("gh api" in run and "/releases" in run)
        if writes:
            pubsteps.append((jname, idx, st))
    check(len(pubsteps) >= 2, "④地板·找得到「往 Release 上写」的步骤",
          "%d 处（分母 = 全部 job 的全部 step 共 %d 个）" % (len(pubsteps), len(steps)))
    for jname, idx, st in pubsteps:
        sname = st.get("name") or st.get("uses")
        check(st.get("if") == CANON_IF, "④闸·%s / %s" % (jname, sname), "if=%r" % (st.get("if"),))

    # ── ⑤ CI 门那一步：只在要发布时拦（拦法与闸同一个住址）───────────────────
    gate = [st for _, _, st in steps if "green CI run" in str(st.get("name") or "")]
    check(len(gate) == 1, "⑤地板·找得到 CI 门那一步", "%d 处" % len(gate))
    if len(gate) == 1:
        check(gate[0].get("if") == CANON_IF, "⑤CI 门挂在同一个闸上",
              "if=%r（这一格红 = 真发版那条路上 CI 门可能被跳过）" % (gate[0].get("if"),))

    # ── ⑥ 每一处发布步骤都带正文来源 ─────────────────────────────────────────
    #   失效方向逐字：只给 Windows 那处加 ⇒ Linux 那处照样发 GitHub 自动生成的。
    #   ⇒ 这一条对 `pubsteps` **逐条**判，不是「至少有一处带」。
    paths = []
    for jname, idx, st in pubsteps:
        sname = st.get("name") or st.get("uses")
        with_ = st.get("with") or {}
        bp = with_.get("body_path")
        inline = with_.get("body")
        gen = with_.get("generate_release_notes")
        check(bool(bp) or bool(inline), "⑥正文来源·%s / %s" % (jname, sname),
              "body_path=%r · body=%r（这一格红 ⇒ 真发出去的正文是 GitHub 自动生成那份）"
              % (bp, inline))
        check(gen is not True, "⑥不回落自动生成·%s / %s" % (jname, sname),
              "generate_release_notes=%r" % (gen,))
        if bp:
            paths.append((jname, idx, str(bp)))
    check(len(paths) == len(pubsteps), "⑥地板·每一处发布步骤都有 body_path",
          "%d/%d 处（分母 = 上面数出来的发布步骤）" % (len(paths), len(pubsteps)))
    check(len({p for _, _, p in paths}) <= 1, "⑥正文只有一个住址",
          "body_path 的取值集合 = %r" % (sorted({p for _, _, p in paths}),))

    # ── ⑦ 那份正文**同一个 job 里真的有人生成它**，而且排在发布步骤前面 ───────
    #   口径与 `ci.yml` 里「build 排在 e2e 前面」那条同源：认字面量 + 认书写顺序。
    for jname, idx, bp in paths:
        producers = [i for jn, i, st in steps
                     if jn == jname and RENDERER in str(st.get("run") or "")
                     and bp in str(st.get("run") or "") and i < idx]
        check(bool(producers), "⑦生成器排在发布步骤前面·%s" % jname,
              "`%s` 在本 job 第 %s 步之前%s" % (RENDERER, idx,
                                              ("被第 %d 步调用，且那一行点名了 `%s`" % (min(producers), bp))
                                              if producers else
                                              "**没有任何一步既调它、又点名 `%s`** —— "
                                              "那个 body_path 没人产出，真发版时 action 读不到它" % bp))

    # ── ⑧ 生成器盘上真的在，而且真的吐得出非空的正文 ──────────────────────────
    #   🔴 这一条是本条的**死值**那一半：不是「不报错」，是**有数**。
    rp = ROOT / RENDERER
    check(rp.exists(), "⑧地板·生成器在盘上", "%s" % rp)
    if rp.exists():
        try:
            proc = subprocess.run(["node", str(rp), "--check"], cwd=str(ROOT),
                                  capture_output=True, text=True, timeout=60)
            line = (proc.stdout or proc.stderr).strip().splitlines()
            check(proc.returncode == 0, "⑧生成器吐得出本版正文",
                  (line[-1] if line else "（无输出）") + "（rc=%d）" % proc.returncode)
        except FileNotFoundError:
            check(False, "⑧生成器吐得出本版正文",
                  "这台机器上没有 `node` —— **判不了，按红记**（不许退化成静默跳过）")
        except subprocess.TimeoutExpired:
            check(False, "⑧生成器吐得出本版正文", "`node` 跑超时 60s —— 判不了，按红记")

    # ══ ⑨–⑪：产字节那条路 ═══════════════════════════════════════════════════
    text =TARGET.read_text(encoding="utf-8")
    by_job = collections.Counter()
    for jname, _, st in steps:
        nm = st.get("name")
        if nm:
            by_job[(jname, str(nm))] += 1

    # ── ⑨a 承诺面的每一格都有产线，且**没有**登记了却不在承诺面里的格（两向）────
    #   🔴 **反空真锚就是这一条**：单向「承诺表里每格都找得到」在承诺表被清空时恒真。
    promised = promised_platforms()
    if promised is None:
        check(False, "⑨a地板·承诺面读得到",
              "`%s` 不在盘上 —— 承诺面没有住址了，本条**判不了，按红记**"
              "（绝不退化成「空集，于是两向相等」）" % LEDGER)
    elif isinstance(promised, tuple):
        check(False, "⑨a地板·承诺面读得到",
              "import `%s` 失败：%s —— 判不了，按红记" % (LEDGER, promised[1]))
    else:
        mine = {b["plat"] for b in BYTE_LINES}
        check(mine == promised, "⑨a承诺的平台 ↔ 产线登记，两向集合相等",
              "承诺了却没产线 %s · 有产线却不在承诺面里 %s（承诺面现打 %s，唯一住址 `%s`）"
              % (sorted(promised - mine), sorted(mine - promised), sorted(promised), LEDGER))

    # ── ⑨b 本文件里「跑 cargo 编后端」的步骤（直接跑，或经 re-embed.sh 跑）↔ 登记，两向集合相等 ────────────────
    on_disk = set()
    for jname, _, st in steps:
        if re.search(r"\bcargo\s+(?:zigbuild|build)\b|\bre-embed\.sh\s+--(?:build-only|stage-native|stage-filewin)\b",
                     str(st.get("run") or "")):
            on_disk.add((jname, str(st.get("name") or "<无名步骤>")))
    registered = {(j, n) for j, n, _, _ in COMPILE_STEPS}
    check(on_disk == registered, "⑨b编后端的步骤 ↔ 登记，两向集合相等",
          "盘上有而没登记 %s · 登记了而盘上没有 %s —— 前者是**给一个可能没承诺的平台"
          "悄悄加了一条产线**（`D7` 反面），后者是登记陈了"
          % (sorted(on_disk - registered), sorted(registered - on_disk)))

    # ── ⑨c 每一格登记的步骤在那个 job 里恰好一处 ─────────────────────────────────
    for b in BYTE_LINES:
        for jname, sname in b["steps"]:
            n = by_job[(jname, sname)]
            check(n == 1, "⑨c产线步骤·%s / %s / %s" % (b["plat"], jname, sname),
                  "在那个 job 里命中 %d 次（应当恰好 1 次）—— 0 = 这一格的字节今天没人产/没人铺" % n)

    # ── ⑨d runner 标签逐字 ──────────────────────────────────────────────────────
    #   ⚠ **它买到的只有「标签没被人换掉」**：`windows-latest` 今天是 x86_64，
    #     而那是 GitHub 说了算的事，本判据**问不出来**。承诺面「本机 Windows 不含 arm64」
    #     这一票落在产线上的全部依据就是这个标签 ⇒ 至少得钉住它别被人改。
    for b in BYTE_LINES:
        jname, want = b["runner"]
        got = ((jobs.get(jname) or {}) or {}).get("runs-on")
        check(got == want, "⑨d runner·%s / %s" % (b["plat"], jname),
              "runs-on=%r（登记 %r）。⚠ 本条买的是「标签没被换掉」，"
              "**买不到**「那个标签今天是哪个 arch」" % (got, want))

    # ── ⑨e 本文件里出现的全部 target triple ↔ 登记，两向集合相等 ────────────────
    seen_triples = set(re.findall(r"\b[a-z0-9_]+-(?:pc|unknown|apple)-[a-z0-9_.-]+\b", text))
    seen_triples = {t for t in seen_triples if t.count("-") >= 2}
    check(seen_triples == TRIPLES, "⑨e target triple ↔ 登记，两向集合相等",
          "盘上有而没登记 %s · 登记了而盘上没有 %s —— 多出来的那个就是"
          "「给一个没承诺的格子悄悄开了产线」的样子"
          % (sorted(seen_triples - TRIPLES), sorted(TRIPLES - seen_triples)))

    # ── ⑩ `BUILD_ID` / 身份戳界标的抽取住址 ────────────────────────────────────
    reads = ps_identity_reads(text, doc)
    check(reads == IDENTITY_READS, "⑩抽取住址 ↔ 登记，逐处计数相等",
          "盘上现打 %r（登记 %r）—— 多/少/改路径都红；`<解不出…>` = 那一处的 `-Path` "
          "解析不出来，按红记不按忽略记" % (dict(reads), dict(IDENTITY_READS)))
    for (rel, const) in sorted({k for k in IDENTITY_READS} | set(reads)):
        if rel.startswith("<解不出"):
            continue
        hits = source_const_hits(rel, const)
        check(hits == 1, "⑩b住址实打·%s 里的 `const %s`" % (rel, const),
              "%s（应当恰好 1 行）—— 🔴 **这一格就是两处只改了一处的形状**："
              "住址指着一份没有那个 const 的文件，发版当场死在抽取上"
              % ("那份文件**读不到**" if hits is None else "命中 %d 行" % hits))

    # ── ⑪ 半 bump 的发版核对：两份 musl 字节 == 源码 `BUILD_ID`（本文件）· 几份内嵌字节彼此同一版（`build.rs`）──
    verify = next((str(st.get("run") or "") for jn, _, st in steps
                   if (jn, str(st.get("name") or "")) == VERIFY_STEP), "")
    for needle, why in [
        ('foreach ($a in "x86_64","aarch64")', "两份 musl 逐个核"),
        ("ReadAllBytes", "读的是那份字节本身"),
        ('$want = "$open$src$close"', "拿源码 `BUILD_ID` 拼出要找的那一段戳"),
        ("IndexOf($want)", "字节里找不到就红"),
    ]:
        check(needle in verify, "⑪发版核对·%s" % why,
              "`%s / %s` 那一步里%s `%s` —— 少它，「内嵌字节自报的 id == 源码 `BUILD_ID`」这条"
              "就没人核了（`build.rs` 刻意不读源码）" % (VERIFY_STEP[0], VERIFY_STEP[1],
                                                     "有" if needle in verify else "**没有**", needle))
    brs = read_rel(BUILD_RS)
    check(brs is not None, "⑪地板·`%s` 读得到" % BUILD_RS, "%s" % (ROOT / BUILD_RS))
    if brs is not None:
        body = rust_fn_body(brs, AGREE_FN)
        if body is None:
            check(False, "⑪`%s` 找得到" % AGREE_FN,
                  "`%s` 里没有这个函数 —— 它被改名/删了，本条此刻是空真，按红记" % BUILD_RS)
        else:
            hit = [w for w in AGREE_BANNED if w in body]
            check(not hit, "⑪`%s` 没有兜底值" % AGREE_FN, "函数体里出现 %r" % hit)
            check("panic!" in body and "BACKEND_EMBEDDED_ID" in body,
                  "⑪几份内嵌字节不同版 ⇒ 构建当场失败，同版才交出共同 id",
                  "`%s` 里 `panic!` %s、`BACKEND_EMBEDDED_ID` %s" % (
                      AGREE_FN, "在" if "panic!" in body else "**不在**",
                      "在" if "BACKEND_EMBEDDED_ID" in body else "**不在**"))
        main = rust_fn_body(brs, "main") or ""
        wired = all(w in main for w in ("embed_backends(&mut carried)",
                                        "embed_native_backend(&mut carried)",
                                        "%s(&carried)" % AGREE_FN))
        check(wired, "⑪两条内嵌路的 id 都交给 `%s`" % AGREE_FN,
              "`main` 里%s把两条内嵌路收来的 id 交给它 —— 漏一条 = 那一份不进「同一版」的核对" % (
                  "" if wired else "**没有**"))

    # ══ ⑬：`BUILD_ID` bump 的同拍债 —— re-embed ═══════════════════════════════
    reembed =read_rel(REEMBED_SH)
    gi = read_rel(BRIDGE_GITIGNORE)

    # ── ⑬a 地板：出路那条命令有唯一住址，而且它指得到一份真在盘上的东西 ──────────
    cmd_const = rust_str_const(brs, "REEMBED_CMD") if brs else None
    check(cmd_const == REEMBED_CMD, "⑬a`build.rs::REEMBED_CMD` 与钉住的字面逐字相同",
          "盘上=%r（钉住 %r）—— 它是「bump 之后怎么办」的唯一住址，"
          "三处 panic ＋ 两条 warning 全从它取值" % (cmd_const, REEMBED_CMD))
    check(reembed is not None, "⑬a地板·`%s` 在盘上" % REEMBED_SH,
          "%s —— 出路指着一个不存在的命令，与「没有出路」在文案上一模一样"
          % (ROOT / REEMBED_SH))
    check(gi is not None, "⑬a地板·`%s` 读得到" % BRIDGE_GITIGNORE, "%s" % (ROOT / BRIDGE_GITIGNORE))

    if reembed is None or brs is None or gi is None:
        emit("::error::⑬ 的地板没过 —— 本组后面几条一律不算数")
        return (1 if fails else 0), passes[0], fails

    # ── 配方的唯一住址是 re-embed.sh；发版里编字节的每一步只调它 ─────────────────────
    #   musl 两份（`REEMBED_TARGETS` ＋ `REEMBED_BUILD_FLAGS`）与本机那几份（本机后端 · 文件窗口）都只住那份脚本：
    #   发版的编字节步逐字只调它的一个口，且没有一步自己跑 cargo 编字节（`tauri build` 是打包器起的 cargo，不在射程里）。
    #   各口「只编不铺 / 只编只铺」真跑一趟是下面的 ⑬i。
    targets = sh_array(reembed, "REEMBED_TARGETS")
    flags = sh_array(reembed, "REEMBED_BUILD_FLAGS")
    check(bool(targets) and bool(flags), "⑬i地板·配方的两个数组抠得到",
          "`%s` 的 REEMBED_TARGETS=%r REEMBED_BUILD_FLAGS=%r" % (REEMBED_SH, targets, flags))
    for (jn0, sn0), mode in REEMBED_STEPS.items():
        runs = [str(st.get("run") or "").strip() for jn, _, st in steps
                if (jn, str(st.get("name") or "")) == (jn0, sn0)]
        want_run = "%s %s" % (REEMBED_CMD, mode)
        check(runs == [want_run], "⑬i发版那一步只经 `re-embed.sh %s` 产字节·%s" % (mode, jn0),
              "`%s / %s` 现打 %r（要恰好一步、逐字 %r）—— 换回手抄的 cargo 就又是两条配方"
              % (jn0, sn0, runs, want_run))
    direct = [(jn, str(st.get("name") or "<无名步骤>")) for jn, _, st in steps
              if re.search(r"\bcargo\s+(?:zigbuild|build)\b", str(st.get("run") or ""))]
    check(not direct, "⑬i发版里没有一步自己跑 cargo 编字节",
          "这几步现打一条手抄的 cargo 编译 %r —— 那是 `%s` 那条配方的第二个家" % (direct, REEMBED_SH))

    # ── ⑬c 落点的 arch ↔ `build.rs` 吃字节那一侧，两向集合相等 ──────────────────
    eb = rust_fn_body(brs, "embed_backends") or ""
    m = re.search(r"for arch in \[([^\]]*)\]", eb)
    rs_arches = set(re.findall(r'"([^"]+)"', m.group(1))) if m else set()
    sh_arches = {t.split("-", 1)[0] for t in (targets or [])}
    check(bool(rs_arches) and rs_arches == sh_arches,
          "⑬c re-embed 铺的 arch ↔ `embed_backends` 吃的 arch，两向集合相等",
          "脚本派生 %s · `build.rs` 现打 %s —— 铺了没人吃（多出来的那份白编）"
          "或吃了没人铺（那一格的自动部署静默关掉）"
          % (sorted(sh_arches), sorted(rs_arches) if m else "<`for arch in [...]` 抠不到>"))
    check("cc-monitor-backend-{arch}" in eb and "cc-monitor-backend-$arch" in reembed,
          "⑬c两侧的文件名模板对得上",
          "`build.rs` 里 `cc-monitor-backend-{arch}` %s · 脚本里 `cc-monitor-backend-$arch` %s"
          % ("在" if "cc-monitor-backend-{arch}" in eb else "**不在**",
             "在" if "cc-monitor-backend-$arch" in reembed else "**不在**"))

    # ── ⑬d 三个内嵌落点 ↔ `.gitignore` 挡着的那几行，两向集合相等 ────────────────
    #   🔴 它守的那一形：落点目录改了名、`.gitignore` 里那几行没跟 ⇒ `git check-ignore` 不命中，
    #     落点没被挡住；而 `release.yml` 文件头把「三个落点全部 gitignore」
    #     当硬事实在用 —— 它正是「门禁只能建在 `release.yml` 上」这条推理的全部前提。
    registered = {rust_str_const(brs, c) or "<`const %s` 抠不到>" % c for c in LANDING_CONSTS}
    ignored = gitignore_landings(gi)
    check(registered == ignored, "⑬d内嵌落点 ↔ `.gitignore` 的机检锚，两向集合相等",
          "登记了却没被挡 %s · 挡着却没人登记 %s（登记侧 = `build.rs` 的 %s；"
          "盘侧 = `%s` 里带 `%s` 锚的那几行）—— 前者是**re-embed 每跑一次就往工作树倒一次"
          "二十多 MB 垃圾**，后者是一条挡着空气的陈行"
          % (sorted(registered - ignored), sorted(ignored - registered),
             "/".join(LANDING_CONSTS), BRIDGE_GITIGNORE, LANDING_MARK))
    for d in sorted(re.findall(r'^[A-Z_]+_DIR="\$ROOT/src/frontend/shell/([^"]+)"', reembed, re.M)):
        check(d in registered, "⑬d脚本往里倒字节的目录是登记过的·%s" % d,
              "`%s` %s在登记集合 %s 里 —— 不在 ＝ 那条命令往一个没人挡的目录里写二十多 MB"
              % (d, "" if d in registered else "**不**", sorted(registered)))

    # ── ⑬e 出路只有一个住址：每处都点名那条命令，且不许手抄第二条产字节配方 ────────
    for fname in EXIT_PATH_FNS:
        body = rust_fn_body(brs, fname)
        if body is None:
            check(False, "⑬e`%s` 找得到" % fname, "`%s` 里没有这个函数 —— 本条此刻是空真，按红记" % BUILD_RS)
            continue
        n = body.count("{REEMBED_CMD}")
        check(n >= 2, "⑬e`%s` 的出路点名那条命令" % fname,
              "函数体里 `{REEMBED_CMD}` 出现 %d 次（至少 2：重编那条 ＋ 诚实关闭那条）—— "
              "0 次 = 又回到「守卫说了它坏了，但没说怎么办」" % n)
        code = rust_code_lines(body)
        hit = [(w, why) for w, why in BANNED_RECIPE if w in code]
        check(not hit, "⑬e`%s` 没有第二条手抄的产字节配方" % fname,
              "代码行里出现 %r —— %s。（整行 `//` 注释已剥掉：注释里讲那段配方是说明，不是指令）"
              % ([w for w, _ in hit], "；".join(why for _, why in hit) or "—"))

    # ── ⑬g 那条命令真跑得起来，而且**有数** ─────────────────────────────────────
    #   🔴 这一条是 ⑬ 的死值那一半（同 ⑧ 的形状）：不是「不报错」，是**有数**。
    #   ⚠ 它跑的是 `--check`（只读），**不是** re-embed 本身 —— 本判据不装 zig、不编任何东西。
    try:
        proc = subprocess.run(["bash", str(ROOT / REEMBED_SH), "--check"], cwd=str(ROOT),
                              capture_output=True, text=True, timeout=120)
        last = [l for l in (proc.stdout or proc.stderr).splitlines() if l.strip()]
        last = last[-1] if last else "（无输出）"
        mm = re.match(r"re-embed: (\d+) passed", last)
        check(proc.returncode == 0 and bool(mm) and int(mm.group(1)) >= 3,
              "⑬g re-embed `--check` 跑得起来且有数",
              "%s（rc=%d）—— 红 = 这棵树上有一份内嵌字节与源码的 `BUILD_ID` 对不上"
              "（**半 bump**，出路就在那行 `::error::` 里），"
              "或者那条命令自己坏了。⚠ 它买到的**不是**「字节是对的」："
              "一棵没铺字节的树上它只答得出「这里没有一份对不上的」" % (last, proc.returncode))
    except FileNotFoundError:
        check(False, "⑬g re-embed `--check` 跑得起来且有数",
              "这台机器上没有 `bash` —— **判不了，按红记**（不许退化成静默跳过）")
    except subprocess.TimeoutExpired:
        check(False, "⑬g re-embed `--check` 跑得起来且有数", "跑超时 120s —— 判不了，按红记")

    # ── ⑬h开发构建的判词 `--check-dev` **有牙**：真跑，四个夹具 ──────────
    #   它治的：「开发构建起不起得来本机后端」在这之前**只有一句 cargo 警告**，而那句还写着
    #   「开发构建里这是正常的」（`D11` 反面）。B1 把它收成 `re-embed.sh --check-dev`：
    #   本机那一份缺席即红、铺了就**真起一趟**、第一行必须是自报源码 `BUILD_ID` 的 hello。
    #   ⇒ 本条不信那段脚本的自述，拿四个夹具**当场跑它**（沙箱仓根，只放脚本与身份那份源码；
    #   「字节」是几行 sh，**一个 cargo 都不编**）：
    #     F0 缺席                    ⇒ `--check-dev` 红、而 `--check` 绿（两个模式**恰好**差在这一格）；
    #     F1 身份戳对、却一个字不吐 ⇒ 红在「起得来」那一格（**戳对得上 ≠ 起得来**，这一格就是证据）；
    #     F2 身份戳对、hello 自报对 ⇒ 绿（阴性对照：尺子不是恒红）；
    #     F3 hello 自报的是别的版本 ⇒ 红在「起得来」那一格。
    #   ⚠ 买不到：「这棵树上那份真字节起得来」—— 那是 `--check-dev` 在真树上跑的事（B1 交付里
    #     现打过一趟），本格在门禁里**不要求**真树铺了字节（字节不进仓，要求它会把每一棵
    #     新 clone 的树都判红；接不接进门禁是 `gate.sh` 的事，这里不管）。
    lib = read_rel("src/backend/common/id.rs")
    contract = read_rel(MARKS_SRC)
    sid = rust_str_const(lib, "BUILD_ID") if lib else None
    sop = rust_str_const(contract, "STAMP_OPEN") if contract else None
    scl = rust_str_const(contract, "STAMP_CLOSE") if contract else None
    try:
        host = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, timeout=30).stdout
        host = next((l.split(": ", 1)[1].strip() for l in host.splitlines() if l.startswith("host: ")), "")
    except (FileNotFoundError, subprocess.TimeoutExpired):
        host = ""
    check(bool(sid and sop and scl and host), "⑬h地板·夹具要的四样都取得到",
          "BUILD_ID=%r 界标=%r/%r 本机 triple=%r —— 缺一样就**判不了，按红记**" % (sid, sop, scl, host))
    if sid and sop and scl and host and reembed is not None:
        stamp = "# %s%s%s" % (sop, sid, scl)

        def fixture(body):
            d = Path(tempfile.mkdtemp(prefix="k-r124-dev-"))
            (d / "tests" / "scripts").mkdir(parents=True)
            (d / "src" / "backend" / "common").mkdir(parents=True)
            (d / "tests" / "scripts" / "re-embed.sh").write_text(reembed, encoding="utf-8")
            (d / "src" / "backend" / "common" / "id.rs").write_text(lib, encoding="utf-8")
            (d / MARKS_SRC).parent.mkdir(parents=True)
            (d / MARKS_SRC).write_text(contract, encoding="utf-8")
            if body is not None:
                nd = d / "src" / "frontend" / "shell" / "native-backend"
                nd.mkdir(parents=True)
                (nd / "cc-monitor-native").write_text("#!/bin/sh\n%s\n%s\n" % (stamp, body), encoding="utf-8")
                (nd / "cc-monitor-native").chmod(0o755)
                (nd / "cc-monitor-native.target").write_text(host + "\n", encoding="utf-8")
            return d

        def go(d, mode):
            p = subprocess.run(["bash", str(d / "tests" / "scripts" / "re-embed.sh"), mode],
                               capture_output=True, text=True, timeout=120)
            return p.returncode, [l.split(" :: ")[0][6:] for l in p.stdout.splitlines() if l.startswith("FAIL")]

        start = "本机内嵌后端起得来（真起一趟，第一行是 hello）"
        present = "开发构建起得来本机后端（本机那一份在盘上）"
        hello = '{"kind":"hello","v":1,"build_id":"%s","host_arch":"x"}'
        cases = [
            ("F0 缺席 · --check-dev", None, "--check-dev", 1, [present]),
            ("F0 缺席 · --check", None, "--check", 0, []),
            ("F1 戳对、一个字不吐", "exit 0", "--check-dev", 1, [start]),
            ("F2 戳对、hello 对（阴性对照）", "echo '%s'" % (hello % sid), "--check-dev", 0, []),
            ("F3 hello 自报别的版本", "echo '%s'" % (hello % (sid + "-stale")), "--check-dev", 1, [start]),
        ]
        for label, body, mode, want_rc, want_fails in cases:
            d = fixture(body)
            try:
                rc, got = go(d, mode)
            except subprocess.TimeoutExpired:
                rc, got = "<超时>", ["<超时>"]
            finally:
                shutil.rmtree(d, ignore_errors=True)
            check(rc == want_rc and got == want_fails, "⑬h开发构建判词有牙·%s" % label,
                  "现打 rc=%r 红在 %r（要 rc=%r 红在 %r）—— 红错地方、或该红不红，都说明那条判词"
                  "判的不是「开发构建起不起得来本机后端」" % (rc, got, want_rc, want_fails))

        # ── ⑬i 只编不铺（`--build-only`）：发版那一步与门禁 `muslbuild` 都经它产 musl 字节 ──
        #   真跑：沙箱仓根 ＋ PATH 最前面一个只记账的 `cargo` 替身（一个字节都不编）。
        #   要：退 0；替身恰好被叫 `REEMBED_TARGETS` 那几次、每次 `zigbuild <旗标> --target <t>`、在 `src/backend` 里；
        #   落点目录没被建出来（「编」与「铺」是两件事：发版那趟自己改名成资产，门禁那格只看字节）。
        d = fixture(None)
        try:
            (d / "bin").mkdir()
            (d / "bin" / "cargo").write_text(
                '#!/bin/sh\nprintf "%%s|%%s\\n" "$PWD" "$*" >> "%s"\n' % (d / "cargo.log"), encoding="utf-8")
            (d / "bin" / "cargo").chmod(0o755)
            env = dict(os.environ, PATH="%s:%s" % (d / "bin", os.environ.get("PATH", "")))
            p = subprocess.run(["bash", str(d / "tests" / "scripts" / "re-embed.sh"), "--build-only"],
                               capture_output=True, text=True, timeout=60, env=env)
            log = (d / "cargo.log").read_text(encoding="utf-8").splitlines() if (d / "cargo.log").exists() else []
            flags = " ".join(sh_array(reembed, "REEMBED_BUILD_FLAGS") or [])
            want = ["%s|zigbuild %s --target %s" % (d / "src" / "backend", flags, t)
                    for t in (sh_array(reembed, "REEMBED_TARGETS") or [])]
            landed = (d / "src" / "frontend" / "shell" / "embedded-backends").exists()
            check(p.returncode == 0 and bool(want) and log == want and not landed,
                  "⑬i`--build-only` 只编不铺",
                  "rc=%d · cargo 替身记下 %r（要 %r）· 落点目录%s建出来 —— 红 = 发版那一步与 `muslbuild` "
                  "那一格不再是同一条配方，或「只编」偷偷铺了字节（stderr：%s）"
                  % (p.returncode, log, want, "**被**" if landed else "没", (p.stderr or "").strip()[-200:]))
        except subprocess.TimeoutExpired:
            check(False, "⑬i`--build-only` 只编不铺", "跑超时 60s —— 判不了，按红记")
        finally:
            shutil.rmtree(d, ignore_errors=True)

        # ── ⑬i 只编只铺（`--stage-native` · `--stage-filewin`）：发版两个 job 经它们产本机那几份字节 ──
        #   真跑：cargo 替身只记账、在 cargo 该落产物的地方放一份假字节（一个字节都不编）。
        #   要：退 0；替身被叫的次数与目录对；`native-backend/` 里恰好铺了该铺的那几对（字节 ＋ `.target` = 这台的 host）；
        #   不判（不印 `re-embed:` 那一行：发版那一侧的核对另有其步，Windows runner 上也不起字节）。
        exe = ".exe" if "windows" in host else ""
        for mode, want_files in (("--stage-native", ["cc-monitor-filewin", "cc-monitor-native"]),
                                 ("--stage-filewin", ["cc-monitor-filewin"])):
            d = fixture(None)
            try:
                (d / "bin").mkdir()
                (d / "bin" / "cargo").write_text(
                    '#!/bin/sh\nprintf "%%s|%%s\\n" "$PWD" "$*" >> "%s"\n'
                    'case "$*" in *cc-monitor-filewin*) o="%s" ;; *) o="%s" ;; esac\n'
                    'mkdir -p "$(dirname "$o")" && printf fake > "$o"\n'
                    % (d / "cargo.log", d / ".build" / "shell" / "release" / ("cc-monitor-filewin" + exe),
                       d / ".build" / "backend" / "release" / ("cc-monitor-backend" + exe)), encoding="utf-8")
                (d / "bin" / "cargo").chmod(0o755)
                env = dict(os.environ, PATH="%s:%s" % (d / "bin", os.environ.get("PATH", "")))
                p = subprocess.run(["bash", str(d / "tests" / "scripts" / "re-embed.sh"), mode],
                                   capture_output=True, text=True, timeout=60, env=env)
                log = (d / "cargo.log").read_text(encoding="utf-8").splitlines() if (d / "cargo.log").exists() else []
                fl = " ".join(sh_array(reembed, "REEMBED_BUILD_FLAGS") or [])
                want_log = (["%s|build %s" % (d / "src" / "backend", fl)] if "cc-monitor-native" in want_files else []) \
                    + ["%s|build %s -p cc-monitor-filewin --bin cc-monitor-filewin" % (d / "src" / "frontend" / "shell", fl)]
                nd = d / "src" / "frontend" / "shell" / "native-backend"
                landed = sorted(f.name for f in nd.iterdir()) if nd.exists() else []
                want_landed = sorted(want_files + [f + ".target" for f in want_files])
                targets_ok = all((nd / (f + ".target")).exists()
                                 and (nd / (f + ".target")).read_text(encoding="utf-8").strip() == host
                                 for f in want_files)
                judged = any(l.startswith("re-embed:") for l in p.stdout.splitlines())
                check(p.returncode == 0 and log == want_log and landed == want_landed and targets_ok and not judged,
                      "⑬i`%s` 只编只铺" % mode,
                      "rc=%d · cargo 替身记下 %r（要 %r）· 铺下 %r（要 %r）· .target %s · %s —— 红 = 发版那一步"
                      "与 `--native` 不再是同一条配方，或铺漏 / 铺多（stderr：%s）"
                      % (p.returncode, log, want_log, landed, want_landed, "对" if targets_ok else "**不对**",
                         "**判了**" if judged else "没判", (p.stderr or "").strip()[-200:]))
            except subprocess.TimeoutExpired:
                check(False, "⑬i`%s` 只编只铺" % mode, "跑超时 60s —— 判不了，按红记")
            finally:
                shutil.rmtree(d, ignore_errors=True)

    # ══ ⑭ 本包的每一个 `[[bin]]` ↔ 安装包的清单 ══════════════════════
    cargo_doc = toml_load(BRIDGE_CARGO)
    bins = cargo_bins(cargo_doc)
    pkg_name = ((cargo_doc or {}).get("package") or {}).get("name")

    # ── ⑭a 地板：manifest 解得出来（没 `tomllib` / 解不动 ⇒ 判不了，按红记）─────────
    check(bins is not None and isinstance(pkg_name, str),
          "⑭a地板·`%s` 的 `[[bin]]` 与包名解得出来" % BRIDGE_CARGO,
          "现打 bin = %r · package.name = %r%s"
          % (bins, pkg_name,
             "（**这台机器上没有 `tomllib`（py<3.11）⇒ 本组判不了，按红记**）" if tomllib is None else ""))
    if bins is None or not isinstance(pkg_name, str):
        emit("::error::⑭ 的地板没过 —— 本组后面几条一律不算数")
        return 1, passes[0], fails

    names = {n for n, _ in bins}
    check(len(bins) == len(names) and len(bins) > 0, "⑭a地板·bin 的名字不重复且非空",
          "%d 个 `[[bin]]`（分母 = `%s` 里 `[[bin]]` 表的份数）：%s"
          % (len(bins), BRIDGE_CARGO, sorted(names)))

    # ── 🔴 ⑭a 本条的本体：**每一个 `[[bin]]` 都有一条登记过的入包路线**（两向集合相等）──
    #   失效方向逐字：① 有人加第三个 `[[bin]]`（像文件窗口程序那样），而**没有任何人
    #   想过它怎么进安装包、装机之后谁去找它** ⇒ 登记侧缺 ⇒ 红；② 登记里留着一个已经
    #   不存在的 bin ⇒ 盘侧缺 ⇒ 红。**刻意不是 `>=`**：地板在「少了一个」那个方向上是瞎的，
    #   而「少一个 bin 进不了包」正是本条要治的病。
    registered = set(BIN_SHIPPING)
    check(names == registered, "⑭a每个 `[[bin]]` ↔ 入包路线登记，两向集合相等",
          "盘上有而没登记 %s · 登记了而盘上没有 %s（登记侧 = 本文件的 `BIN_SHIPPING`，"
          "每条写着它走哪条路线 · 装机后落在哪 · 消费侧谁去找它）"
          % (sorted(names - registered), sorted(registered - names)))

    # ── ⑭b 主二进制恰好一个，而且就是登记里那一个 ───────────────────────────────────
    #   口径与 `tauri-cli::get_binaries` 逐字同源：`name == 包名` **或** `name == default-run` 的
    #   那个才是 main。主二进制 `cc-monitor` 与包名 `monitor` 不同 ⇒ 本仓用 `default-run` 点名它。
    #   两样都按 tauri 的口径算：换人（`default-run` 改指别处 / 又长出一个与包名同名的 bin ⇒ 两个 main）
    #   当场红 —— `installer.nsi` 的 `MAINBINARYNAME` 与 deb 的 `Exec` 都跟着主二进制走。
    default_run = ((cargo_doc or {}).get("package") or {}).get("default-run")
    mains = {n for n in names if n == pkg_name or n == default_run}
    reg_mains = {n for n, v in BIN_SHIPPING.items() if v["route"] == "主二进制"}
    check(mains == reg_mains and len(mains) == 1 and (default_run is None or default_run in names),
          "⑭b主二进制恰好一个，且与登记一致",
          "`name == package.name`（%r）或 `name == package.default-run`（%r）的 bin = %s · 登记的主二进制 = %s"
          "（`default-run` 指着一个不存在的 bin 也红）"
          % (pkg_name, default_run, sorted(mains), sorted(reg_mains)))

    # ── ⑭c 零命中：没有任何 `[[bin]]` 带 `required-features` ────────────────────────
    #   🔴 这一条是**零命中**那一形（不是地板）：`tauri-cli::get_binaries` 对
    #   「required-features 这一趟没开」的 bin 逐字 `continue` ⇒ 它**静默**不进安装包，
    #   而 `cargo build` 那边照样编得出来 ⇒ 开发树上一切正常、装机那份少一个文件。
    gated = [(n, f) for n, f in bins if f]
    check(not gated, "⑭c没有 `[[bin]]` 带 `required-features`（零命中）",
          "现打 %r（分母 = 上面那 %d 个 `[[bin]]` 逐个看）—— 命中就是「这个 bin 可能"
          "**静默**不进安装包」：`get_binaries` 对它 `continue`" % (gated, len(bins)))

    # ── ⑭d `externalBin` ↔ 登记，两向集合相等；且不许与本包的 bin 重名 ─────────────
    sc_doc = json_load(SIDECAR_CONF)
    stems = external_bin_stems(sc_doc)
    check(stems is not None, "⑭d地板·`%s` 的 `externalBin` 解得出来" % SIDECAR_CONF,
          "现打 %r" % (stems,))
    if stems is not None:
        # 登记侧**实打去读那个住址**（不抄名字）。抠不到 ⇒ 给一个说明串，它必然不等于
        # 任何真 stem ⇒ 落在下面那条相等的红这一侧（不许退化成「空集，于是相等」）。
        reg_foreign = set()
        for (rel, const), why in sorted(FOREIGN_SIDECARS.items()):
            src = read_rel(rel)
            got = rust_str_const(src, const) if src else None
            check(got is not None, "⑭d地板·sidecar 名字的住址·%s 的 `const %s`" % (rel, const),
                  "现打 %r（它是 %s）—— 抠不到就是**判不了，按红记**" % (got, why))
            reg_foreign.add(got if got is not None else "<`%s::%s` 抠不到>" % (rel, const))
        check(stems == reg_foreign, "⑭d`externalBin` ↔ 登记，两向集合相等",
              "盘上有而没登记 %s · 登记了而盘上没有 %s —— 前者是安装包旁边多放了一份没人登记的程序"
              "（本机后端那份与 exe 内嵌的逐字节相同、开机只用内嵌那份，旁边那份没人用）"
              % (sorted(stems - reg_foreign), sorted(reg_foreign - stems)))
        dup = stems & names
        check(not dup, "⑭d`externalBin` 与本包的 `[[bin]]` 不重名（零命中）",
              "两边都有 %s —— 重名 ＝ 同一个落点名被铺**两份**（NSIS 清单里那行 `/oname=` "
              "会出现两次；MSI 那边是两个组件往 `INSTALLDIR` 装同一个文件名，WiX 的 ICE30 "
              "那一族，而本机验不了）。⚠ 非主 `[[bin]]` **本来就会被装**，别再加一条"
              % sorted(dup))

    # ── ⑭e 消费侧找它的那个名字 ↔ 产出侧的 bin 名，逐字相同 ─────────────────────────
    #   失效方向：改了 Cargo.toml 里的 `name`（或改了那个 `const`）而另一边没跟 ⇒
    #   装机那份上 `resolve_window_bin()` 在 exe 旁找一个不存在的文件名 ⇒ 用户点那个按钮
    #   拿到一条响亮的失败（`D11` 不留退路 ⇒ 它不会静默，但那也**不是**功能）。
    for name, spec in sorted(BIN_SHIPPING.items()):
        if not spec["consumer"]:
            continue
        rel, const = spec["consumer"]
        src = read_rel(rel)
        got = rust_str_const(src, const) if src else None
        check(got == name, "⑭e消费侧的名字·%s 的 `const %s`" % (rel, const),
              "现打 %r · Cargo.toml 里那个 `[[bin]]` 叫 %r —— 两边是同一个文件名主干，"
              "打包器装进去时用的就是 bin 的名字（`installer.nsi` 现打 `/oname=%s.exe`）"
              % (got, name, name))

    # ── ⑭f 那条「非主 bin 也装」的机制，依据的工具版本逐字钉住 ───────────────────────
    #   ⚠ 它买的是「**产这几个安装包的那把工具没被换掉**」，**买不到**「换了之后行为会不会变」
    #     —— 那要人去重读源码。
    lock = json_load(PACKAGE_LOCK)
    locked = (((lock or {}).get("packages") or {}).get(CLI_LOCK_KEY) or {}).get("version")
    check(locked == CLI_VERSION, "⑭f打包工具版本 ↔ 登记，逐字相同",
          "`%s` 里 `%s` 现打 %r（登记 %r）—— 这一格红不等于坏了，它是一张**必须重读的传票**："
          "`cc-monitor-filewin` 进安装包**不靠本仓任何配置**，只靠这一版 `get_binaries` ＋ "
          "三个打包器里那三处 `if !bin.main()`（逐条读数住本文件 ⑭ 的头注）"
          % (PACKAGE_LOCK, CLI_LOCK_KEY, locked, CLI_VERSION))

    # ══ ⑮ 产物路径的根 ↔ 声明的 `target-dir`，两向集合相等 ════════════
    declared = declared_target_dirs()
    roots = prod_path_roots(steps)
    check(all(not v.startswith("<") for v in declared.values()),
          "⑮地板·两份 `.cargo/config.toml` 的 `target-dir` 都抠得到",
          "现打 %r" % (declared,))
    check(bool(roots), "⑮地板·`release.yml` 里找得到 cargo 产物路径",
          "现打 %d 个根（分母 = 全部 job 的 `run:`/`with:` 里带 `release`/`debug` 段的 token）：%r"
          % (len(roots), {k: len(v) for k, v in sorted(roots.items())}))
    want_roots = {v for v in declared.values()}
    got_roots = set(roots)
    check(got_roots == want_roots, "⑮产物路径的根 ↔ 声明的 `target-dir`，两向集合相等",
          "`release.yml` 里有而没声明 %s · 声明了而本文件没用 %s（声明侧 %r；"
          "每个根的原样 token：%r）—— 前者是一条**静默**缺陷"
          "（产物路径指空，三个 glob 全不匹配而只打 warning "
          "⇒ Release 上一个安装包都没有）；后者是「那棵树的产物从此没人拿」"
          % (sorted(got_roots - want_roots), sorted(want_roots - got_roots), declared,
             {k: sorted(set(v))[:3] for k, v in sorted(roots.items())}))

    # ══ ⑯ Release 资产名 · 两个 musl 目标 · 校验和 ↔ 登记表（对外的资产名）════════
    reg = asset_registry()
    check(bool(reg), "⑯地板·资产登记表抠得出来",
          "`%s` 两个标记之间现打 %d 行" % (ASSET_DOC, len(reg or [])))
    reg = reg or []
    pub_jobs = {}
    for jname, idx, st in pubsteps:
        pub_jobs.setdefault(jname, []).append((idx, st))
    reg_jobs = {j for _, j, _ in reg}
    check(reg_jobs == set(pub_jobs), "⑯a登记的 job ↔ 有发布步骤的 job，两向集合相等",
          "登记有而没发布 %s · 发布了而没登记 %s"
          % (sorted(reg_jobs - set(pub_jobs)), sorted(set(pub_jobs) - reg_jobs)))
    for jname in sorted(pub_jobs):
        globs = []
        for _, st in pub_jobs[jname]:
            globs += [leaf(x) for x in str((st.get("with") or {}).get("files") or "").splitlines() if x.strip()]
        names = [n for n, j, _ in reg if j == jname]
        lg, ln, multi = match_both_ways(globs, names)
        check(not (lg or ln or multi), "⑯b资产名·%s 的 `files:` ↔ 登记，两向一一对上" % jname,
              "没登记的上传 %s · 登记了没上传 %s · 一条认了几个 %s（上传 %r）"
              % (lg, ln, multi, globs))

    reg_arches = {n[len(BACKEND_ASSET_PREFIX):] for n, _, _ in reg if n.startswith(BACKEND_ASSET_PREFIX)}
    # 交叉编译那一侧：发版那一步只调 re-embed（⑬i），编哪几个 target 读它的 `REEMBED_TARGETS`。
    built = {t[:-len("-unknown-linux-musl")]
             for t in (sh_array(read_rel(REEMBED_SH) or "", "REEMBED_TARGETS") or [])
             if t.endswith("-unknown-linux-musl")}
    stage_run = "".join(str(st.get("run") or "") for jn, _, st in steps
                        if (jn, st.get("name")) == STAGE_STEP)
    staged = set(re.findall(r"\.build/backend/([a-z0-9_]+)-unknown-linux-musl/release/cc-monitor-backend"
                            r" staged/" + re.escape(BACKEND_ASSET_PREFIX) + r"\1\s*$", stage_run, re.M))
    check(reg_arches == built == staged == MUSL_ARCHES,
          "⑯c两个 musl 目标·登记 ↔ 交叉编译 ↔ 改名成资产，都等于对外契约的那两个",
          "登记 %s · 编 %s · 改名 %s · 契约 %s"
          % (sorted(reg_arches), sorted(built), sorted(staged), sorted(MUSL_ARCHES)))

    sums = sorted({c for _, _, c in reg if c})
    orphans = sorted(n for n, _, c in reg if c is None and n not in sums)
    check(bool(sums) and not orphans, "⑯d每样资产都进一份校验和",
          "校验和文件 %s · 没进任何一份的 %s" % (sums, orphans))
    for sname in sums:
        jobs_of = {j for n, j, _ in reg if n == sname}
        covered = [n for n, j, c in reg if c == sname]
        cov_jobs = {j for n, j, c in reg if c == sname}
        check(len(jobs_of) == 1 and cov_jobs == jobs_of,
              "⑯d校验和文件·%s 登记了、且只校验同一 job 的资产" % sname,
              "它自己登记在 %s · 它校验的资产在 %s" % (sorted(jobs_of), sorted(cov_jobs)))
        if len(jobs_of) != 1:
            continue
        jname = next(iter(jobs_of))
        first_pub = min((i for i, _ in pub_jobs.get(jname, [])), default=-1)
        gens = [(i, str(st.get("run") or "")) for jn, i, st in steps
                if jn == jname and sname in str(st.get("run") or "") and i < first_pub]
        check(len(gens) == 1, "⑯d生成步骤·%s 在 %s 里恰好一步、排在发布前" % (sname, jname),
              "现打 %d 步（第 %s 步）" % (len(gens), [i for i, _ in gens]))
        if len(gens) != 1:
            continue
        run = gens[0][1]
        got = hashed_files(run)
        lg, ln, multi = match_both_ways(got or [], covered)
        check(got is not None and not (lg or ln or multi),
              "⑯d算的那几份·%s ↔ 登记里进它的资产，两向一一对上" % sname,
              "算了没登记 %s · 登记了没算 %s · 一条认了几个 %s（算的 %r）—— 后一样就是 4.1.1 那趟："
              "两个后端上传了却不在 `SHA256SUMS-linux.txt` 里，下载的人校验不了"
              % (lg, ln, multi, got))
        cnt = re.findall(r'\[ "\$n" -eq (\d+) \]', run)
        check(all(int(x) == len(got or []) for x in cnt),
              "⑯d份数守卫·%s 那一步的恒等 == 算的份数" % sname,
              "守卫写 %s · 算的 %d 份" % (cnt, len(got or [])))

    # ══ ⑰ 第三方许可声明 ↔ 现打的依赖（新加的依赖没进声明 ⇒ 红；声明里留着已经删掉的 ⇒ 红）════════
    notices, rest = read_rel(NOTICES), read_rel(NOT_SHIPPED)
    lock = locked_third_party()
    shipped, unshipped = listed(notices, "crate"), listed(rest, "crate")
    check(bool(lock) and bool(shipped) and notices is not None and rest is not None,
          "⑰地板·声明 · 不进产物那份 · 两份锁文件都读得出来",
          "锁文件第三方 %d · 声明 %d · 不进产物 %d" % (len(lock or []), len(shipped), len(unshipped)))
    lock = lock or set()
    check(shipped | unshipped == lock and not (shipped & unshipped),
          "⑰a Rust 依赖·声明 ∪ 不进产物 == 锁文件第三方，两向且不相交",
          "锁文件有而两份都没有 %s · 两份有而锁文件没有 %s · 两份都有 %s —— 跑 `python3 %s` 重新生成"
          % (sorted(lock - shipped - unshipped)[:8], sorted((shipped | unshipped) - lock)[:8],
             sorted(shipped & unshipped)[:8], NOTICES_GEN))
    npm_want, npm_got = npm_production() or set(), listed(notices, "npm")
    check(bool(npm_want) and npm_want == npm_got, "⑰b npm 生产依赖·声明 == package-lock 不带 dev 的包，两向",
          "锁里有没声明 %s · 声明了锁里没有 %s" % (sorted(npm_want - npm_got), sorted(npm_got - npm_want)))
    gen = read_rel(NOTICES_GEN) or ""
    pinned = re.search(r'^CARGO_ABOUT_VERSION = "([^"]+)"', gen, re.M)
    checkers = [(jn, st) for jn, _, st in steps if NOTICES_CHECK in str(st.get("run") or "")]
    pub_job_names = sorted({jn for jn, _, _ in pubsteps})
    gating = [jn for jn, _ in checkers if all(jn in needs_closure(jobs, p) for p in pub_job_names)]
    check(len(checkers) == 1 and gating, "⑰c发版那一趟·`%s` 恰好一步，所在 job 是每个发布 job 都等着的" % NOTICES_CHECK,
          "现打 %d 步（%s）· 发布 job %s" % (len(checkers), [jn for jn, _ in checkers], pub_job_names))
    tools = [str((st.get("with") or {}).get("tool") or "") for jn, _, st in steps
             if checkers and jn == checkers[0][0] and "install-action" in str(st.get("uses") or "")]
    want_tool = "cargo-about@%s" % (pinned.group(1) if pinned else "?")
    check(bool(pinned) and want_tool in tools, "⑰c工具版本·那个 job 装的 cargo-about == 生成器钉的",
          "生成器钉 %s · 那个 job 装 %s" % (want_tool, tools))
    try:
        res = (json.loads(read_rel(SIDECAR_CONF) or "{}").get("bundle") or {}).get("resources") or {}
    except ValueError:
        res = {}
    into = [v for k, v in (res.items() if isinstance(res, dict) else []) if leaf(k) == NOTICES]
    check(into == [NOTICES], "⑰d声明进安装包·`%s` 的 bundle.resources 把它放进包根" % SIDECAR_CONF,
          "现打 %r" % (res,))
    mvn_want, mvn_got = maven_release_locked(), maven_declared(notices)
    check(bool(mvn_want) and set(mvn_got) == mvn_want,
          "⑰e手机 Maven 依赖·声明 == `%s` 里进发布包那一档（%s），两向" % (MAVEN_LOCK, MAVEN_CONF),
          "锁里 %d · 声明 %d · 锁里有没声明 %s · 声明了锁里没有 %s"
          % (len(mvn_want or []), len(mvn_got), sorted((mvn_want or set()) - set(mvn_got))[:8],
             sorted(set(mvn_got) - (mvn_want or set()))[:8]))
    allowed = accepted_licenses()
    off = sorted("%s (%s)" % (c, lic) for c, lic in mvn_got.items() if lic not in allowed)
    check(bool(allowed) and bool(mvn_got) and not off,
          "⑰e手机 Maven 许可·声明里每个 Maven 依赖的许可都在 `%s` 的 accepted 名单里（不许 GPL）" % ABOUT_CONF,
          "名单 %d 种 · 不在名单里的 %s" % (len(allowed), off))

    # ══ ⑱ 手机那一格：签名包 · 签名材料只从 secrets 来 · 不进仓 · 不进日志 · 用完就删 ════════
    mob = jobs.get(MOBILE_JOB) if isinstance(jobs, dict) else None
    check(isinstance(mob, dict), "⑱a手机那一格在·`%s` 这个 job" % MOBILE_JOB,
          "jobs 的键 = %r" % (sorted(jobs) if isinstance(jobs, dict) else jobs,))
    msteps = [(i, st) for jn, i, st in steps if jn == MOBILE_JOB]
    mrun = lambda st: str(st.get("run") or "")
    builds = [i for i, st in msteps if ":app:assembleRelease" in mrun(st) and "--no-daemon" in mrun(st)]
    check(len(builds) == 1, "⑱a打签名包·`./gradlew :app:assembleRelease --no-daemon` 恰好一步",
          "现打 %d 步" % len(builds))
    check("build-windows" in needs_closure(jobs, MOBILE_JOB),
          "⑱a排在 build-windows 之后（Release 由它建、版本号与 tag 一致那道检查由它守）",
          "needs 闭包 = %s" % sorted(needs_closure(jobs, MOBILE_JOB)))
    # 签名材料：整份 release.yml 里只在手机那一格的步骤 `env:` 上出现，四个各一次；`run:` 里不许直接展开。
    env_refs, other_refs = [], []
    for jname, idx, st in steps:
        env_text = "\n".join(str(v) for v in ((st.get("env") or {}) if isinstance(st.get("env"), dict) else {}).values())
        refs = secret_refs(env_text)
        (env_refs if jname == MOBILE_JOB else other_refs).extend(refs)
        other_refs.extend(secret_refs(mrun(st)) + secret_refs(json.dumps(st.get("with") or {})))
    job_level = [secret_refs(json.dumps({k: v for k, v in (j or {}).items() if k != "steps"}))
                 for j in (jobs.values() if isinstance(jobs, dict) else [])]
    other_refs += [r for refs in job_level for r in refs] + secret_refs(json.dumps(doc.get("env") or {}))
    check(sorted(env_refs) == sorted(SIGN_SECRETS) and not other_refs,
          "⑱b签名材料只从 secrets 来·四个 secrets 各在手机那一格的步骤 `env:` 上出现一次，别处零次",
          "步骤 env 上 %s · 别处（run / with / job 级 / 文件级）%s · 要 %s"
          % (sorted(env_refs), sorted(other_refs), sorted(SIGN_SECRETS)))
    ci_text = CI_WORKFLOW.read_text(encoding="utf-8") if CI_WORKFLOW.exists() else None
    check(ci_text is not None and not re.search(r"secrets\.ANDROID_|ANDROID_KEYSTORE|ANDROID_KEY_", ci_text),
          "⑱b ci.yml 的 mobile 格只打未签名包·ci.yml 里一处签名材料都没有",
          "ci.yml %s" % ("读不到" if ci_text is None else "命中 %r" % re.findall(r"\S*ANDROID_KEY\S*", ci_text)[:4]))
    # 还原那一步：只写 runner 的临时目录；碰到签名变量的每一行都是重定向进文件（不往标准输出写）；不开 xtrace。
    restore = [(i, st) for i, st in msteps if secret_refs(json.dumps(st.get("env") or {}))]
    check(len(restore) == 1, "⑱c地板·还原 keystore 的那一步恰好一步", "现打 %d 步" % len(restore))
    names = "|".join(sorted(SIGN_SECRETS))
    for i, st in msteps:
        lines = [l for l in mrun(st).splitlines() if not l.lstrip().startswith("#")]
        leaky = [l.strip() for l in lines if re.search(r"\$\{?(%s)\b" % names, l) and not re.search(r">>?\s*\"?\$", l)]
        xtrace = [l.strip() for l in lines if re.search(r"\bset\s+-[a-wyz]*x|set -o xtrace|\bbash -x\b", l)]
        check(not leaky and not xtrace, "⑱c不进日志·%s 第 %d 步" % (MOBILE_JOB, i),
              "碰签名变量却不重定向进文件的行 %r · 开 xtrace 的行 %r" % (leaky, xtrace))
    if len(restore) == 1:
        r = mrun(restore[0][1])
        writes = re.findall(r">>?\s*\"?\$\{?([A-Za-z_]+)", r)
        homes = set(re.findall(r"^\s*([A-Za-z_]+)=\"?\$\{?RUNNER_TEMP\}?/", r, re.M))
        check(bool(writes) and set(writes) <= homes and "base64 -d" in r,
              "⑱c只写在 runner 的临时目录·还原那一步写的每个文件都在 `$RUNNER_TEMP` 下",
              "写进 %s · 住在 $RUNNER_TEMP 下的 %s" % (sorted(set(writes)), sorted(homes)))
        if builds:
            b = mrun(dict(msteps)[builds[0]])
            check(re.search(r"-Pccm\.keystoreProperties=\"?\$\{?RUNNER_TEMP\}?/", b) is not None,
                  "⑱c打包那一步读的凭据文件就是临时目录里那份（`-Pccm.keystoreProperties=$RUNNER_TEMP/…`）",
                  "打包那一步：%r" % b.strip()[:200])
    # 用完就删：打包之后有一步 `if: always()` 删掉临时目录里那几份。
    cleanup = [i for i, st in msteps if st.get("if") == "always()" and "rm " in mrun(st)
               and "RUNNER_TEMP" in mrun(st) and builds and i > builds[0]]
    check(len(cleanup) == 1, "⑱d构建完删掉临时文件·打包之后恰好一步 `if: always()` 删 `$RUNNER_TEMP` 下那几份",
          "现打 %d 步" % len(cleanup))
    # 签名包真打出来了、版本号与桌面一致：对着 APK 核签名（印证书指纹）与 versionName == package.json。
    verify = [i for i, st in msteps if re.search(r'apksigner"?\s+verify\b', mrun(st)) and "--print-certs" in mrun(st)
              and "dump badging" in mrun(st) and "package.json" in mrun(st)]
    check(len(verify) == 1 and builds and verify[0] > builds[0],
          "⑱e打包之后核 APK：签名（印证书指纹）· versionName == package.json 的 version",
          "现打 %d 步" % len(verify))
    # 签名凭据那几样不进仓：`.gitignore` 挡得住，`git ls-files` 里一份都没有。
    try:
        ign = subprocess.run(["git", "check-ignore", "--no-index", "--"] + SIGN_FILES, cwd=str(ROOT),
                             capture_output=True, text=True, timeout=30)
        ignored = set(ign.stdout.split())
        tracked = [f for f in subprocess.run(["git", "ls-files"], cwd=str(ROOT), capture_output=True,
                                             text=True, timeout=60).stdout.splitlines() if SIGN_FILE_RE.search(f)]
        check(ignored == set(SIGN_FILES) and not tracked, "⑱f签名凭据不进仓·`.gitignore` 挡住、仓里一份都没有",
              "没挡住 %s · 进了仓的 %s" % (sorted(set(SIGN_FILES) - ignored), tracked))
    except (OSError, subprocess.TimeoutExpired) as e:
        check(False, "⑱f签名凭据不进仓·`.gitignore` 挡住、仓里一份都没有", "git 跑不起来：%s —— 判不了，按红记" % e)

    return (1 if fails else 0), passes[0], fails


def main():
    out = []
    rc, n, fails = run_checks(out.append)
    print("\n".join(out))
    if fails:
        print("---- 红 %d 条（分母 = 上面逐行 PASS/FAIL 打出来的那几条）----" % len(fails))
        print("::error::release.yml 发版守卫红了：" + " / ".join(fails))
        return rc
    print("release-gate: %d passed（分母 = 上面逐行印出来的 PASS 条数；被测对象 `%s`。"
          "⚠ 它不执行 GitHub 的表达式求值器、只认两种「往 Release 上写」的形状、"
          "不判正文写得对不对。"
          "🔴 **诚实边界（写死别读宽）**：`release.yml` 那一族**在本地一步都真跑不起来** "
          "⇒ 买到的是「**盘上这几份文本满足上面逐行列出的那些条件**」，**不是**「云端那一趟会绿」；"
          "⑨ 那几条尤其是 —— 「登记的那一步在文件里」≠「那一步在 runner 上编得出字节」，"
          "更不等于「那份字节在目标机器上跑得起来」。真机行为仍是**判不了**，不是「通过」。"
          "🔴 **⑬ 那一组同一条边界**：⑬a–⑬e 全是盘上文本的对拍，"
          "「配方写得一样」≠「那条命令今天跑得出字节」；⑬i 真跑 `--build-only` 用的是只记账的 cargo 替身；⑬g 真跑的是 `--check`（只读），"
          "在一棵没铺字节的树上它只答得出「这里没有一份对不上的字节」，"
          "**不是**「字节是对的」，更不是「发版办完了」。"
          "🔴 **⑭ 那一组的边界**：它读的全是**盘上的文本**（Cargo.toml 的 "
          "`[[bin]]` · `externalBin` · `proc.rs` 那个 `const` · `package-lock.json` 的 CLI 版本）"
          "⇒ 买到的是「每一个 bin 都有一条登记过的入包路线、而且那条路线的依据没被换掉」，"
          "**不是**「装机后它真在那儿」——「非主 `[[bin]]` 会被装」这件事的现打读数是"
          "一份真 `.deb`（`usr/bin/` 三份）＋ 一份**生成出来的** `installer.nsi`，"
          "而**Windows 上装完之后那一维一趟都没验过**（没有 Windows 机器、不跑 NSIS/WiX）；"
          "发版那一趟另有一步对着真 `.deb` 的字节两向核一次（`release.yml` 的 "
          "`The .deb really ships every binary this package declares`），本格不执行它。"
          "🔴 **⑮ 的边界**：它比的是「路径的根 ↔ 声明的 `target-dir`」这两段**文本**，"
          "**不问**那个目录在 runner 上真有没有东西。"
          "—— 逐条射程写在本文件头注）" % (n, TARGET))
    return rc


if __name__ == "__main__":
    sys.exit(main())
