#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R124` 的判据本体 —— **发版那条流水线的两件事：闸没被偷偷改过 · 正文是我们写的那份。**

# 它守的性质是（四句）

1. 〔`KR114D1`，`K-R114` 09-14 立〕`release.yml` 里「发不发布」那个闸
   （`env.PUBLISH` 的**字面** ＋ 每一处「往 Release 上写」与 CI 门那一步的 `if:`）
   **与钉在本文件里的那一份逐字相同**。
2. 〔`KR124D2`，本件 09-15 立〕**每一处「往 Release 上写」的步骤都带一个正文来源**，
   而且那份正文**同一个 job 里真的有人生成它**、生成器盘上真的在、生成出来的段真的非空。
3. 〔`19b`，2026-09-19 立〕**产字节那条路**：条 63 承诺的每一格都有一条产线在本文件上
   （两向），本文件上没有第二条**没人登记**的产线（两向），每一处抠后端 `const BUILD_ID` /
   身份戳界标的住址**实打指得到真东西**，而「抠不到」在 `build.rs` 那一侧是一条
   **所有构建形态都响**的失败（`"unknown"` 兜底从类型上消失）。
4. 〔`19c`，2026-09-19 立〕**`BUILD_ID` bump 的同拍债：re-embed**。产字节那条路的
   **本机那一端**有唯一一条命令（`tests/scripts/re-embed.sh`），它的配方与上面那条产线
   **同源**（target 两向集合相等 ＋ 旗标逐字相同）；它铺的落点与 `build.rs` 吃的落点
   两向相等、且**三个落点全部被 `.gitignore` 挡着**（两向）；`build.rs` 那几处守卫的
   **出路只有这一个住址**，不许再手抄第二条产字节配方；mtime 那张安全网一条没撤。
   逐条与它买不到什么，见下面 `19c` 那一段的头注。

# 🔴 第 3 句为什么归这份判据，而不是归 `platform` 那一格

`K-G4-platform-ledger.py`（`platform`，第 24 格）判的是「**门禁盖到了哪些平台**」——
它读 `gate.sh`。本条判的是「**产线盖到了哪些平台**」—— 它读 `release.yml`。
两件事分开的理由是硬的（`设计/96 §7.1.2` 现打）：三个落点全部 gitignore ⇒ **字节不进仓**，
三条产线**只由 `release.yml` 一个文件驱动** ⇒ 「这张表的门禁只能建在 `release.yml` 上，
不能建在 `cargo` 上」。⇒ 门禁编得过 ≠ 发版那趟产得出字节，**两格都要有**。
⚠ **条 63 的承诺面本文件一个字不抄**：`PROMISED` 的唯一住址是那份账本，这里 import 它。

# 🔴 它为什么住在这里，而不是住在 `ci.yml` 的 `run:` 块里

`K-R114`（`d1a0552`）把整段判据写进 `ci.yml` 的 `run: |` 里，而 **runner 会把 `run:` 块里的
`${{ … }}` 先求值再交给 shell** ⇒ 上面第 1 条要比的那个字面 `CANON_ENV` 在渲染之后变成
`"false"`（`push` / tag 上是 `"true"`），**与盘上那串模板在三个触发器上都必不相等**
⇒ **这条守卫从加进去那天起就不可能过**。云端实打读数住
`evidence/K-R123-发版读数.md § 1.3`（run `34928839471`，日志里逐字 `CANON_ENV = "false"`）。

⇒ 修法选的是单子给的第一条路 —— **把字面从 `run:` 块里挪出去**，而且挪得比「经 `env:` 传」更远：
**整段判据搬进这份 `.py`**。`.py` 不经 GitHub 的表达式渲染器，`${{` 在这里就是四个字符。
选它不选转义（`${{` → `${{ '${{' }}` 之类）的理由写在交回里，一句话：
**转义只治「这一个字面」，搬出去同时治了「这段判据在本地跑不起来」** —— 而后者才是
它坏了一个月没人看见的原因（`KR124D3`）。

# 🔴 刻意不用 PyYAML（`KR124D3` 选的乙）

沙箱镜像 `ccmon-devbox:latest` 里 `python3 -c 'import yaml'` 是
`ModuleNotFoundError: No module named 'yaml'`（**2026-09-15 现打**，命令
`docker run --rm ccmon-devbox:latest python3 -c 'import yaml'`）。
原版判据用 `yaml.safe_load` 写 ⇒ 它**在本地从来跑不起来** ⇒ 只能在云端切刀，
而云端每切一刀要一趟 CI。下面那个 `parse_workflow()` 是本文件自带的 **YAML 子集切块器**，
口径与 `evidence/K-R122-ruler.py` 的 `ci_steps()` 同源（那一份只切 `steps:`，这一份要
`on:` / `env:` / `jobs:` / 每一步的 `with:`，所以写全了）。

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
   ⇒ 你买到的是「**盘上这份文本满足这几条**」，**不是**「云端那一趟会绿」。
   ⇒ 尤其是 `19b` 那几条：「**登记的那一步在文件里**」≠「**那一步在 runner 上编得出字节**」，
   更不等于「那份字节在目标机器上跑得起来」。真机行为这一维仍然是**判不了**，不是「通过」。
   本地绿 ＋ 云端红这一形，`K-R119` 那趟已经实打过一次（读数住 `evidence/K-R119-发版读数.md`）。
5. 第 2 条性质里「正文真的是我们写的那份」，机器认的是
   **「有 `body_path` · 不是 `generate_release_notes` · 那个路径同 job 里有人生成 · 生成器跑得出非空的段」**。
   **正文写得对不对、好不好，它一个字都不判**（那要读语义）。
6. **切块器是手写的 YAML 子集**，不是 YAML 实现。锚点、多文档、流式映射（`{a: 1}`）、
   复杂 key 一概不支持 —— 挡这一形的是下面那几条**地板**（`on` 解析得出来 · job 数 ·
   step 总数 · 「往 Release 上写」至少两处 · CI 门恰好一处）：切块器坏了地板先红，
   而不是静默地「零违例」。

# 跑法

    python3 evidence/K-R124-ruler.py                      # 判本树
    K_R124_ROOT=<别的树> python3 evidence/K-R124-ruler.py   # 死值验：对着变异过的副本跑
    RELEASE_WORKFLOW=<某份 release.yml> python3 …          # 只换被测的那一份

退出码 0 = 过；1 = 有违例 / 地板没过 / 切块器坏了。
最后一行恒印 `release-gate: <N> passed（…）`，那个 `N` = 上面逐行印出来的 `PASS` 条数。
"""
import collections
import importlib.util
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(os.environ.get("K_R124_ROOT") or Path(__file__).resolve().parents[2])
TARGET = Path(os.environ.get("RELEASE_WORKFLOW") or (ROOT / ".github" / "workflows" / "release.yml"))

#: 🔴 **闭集只有这一个住址**：`release.yml` 里「发不发布」那个闸的字面，钉在这两个常量上。
CANON_ENV = "${{ github.event_name == 'push' || inputs.publish == true }}"
CANON_IF = "env.PUBLISH == 'true'"

#: 正文生成器的住址（相对仓根）。`release.yml` 两处发布步骤的 `body_path` 都由它产出。
# 〔搬树 2026-09-18〕`scripts/` 并进了 `tests/scripts/`。`release.yml` 已经改对，
# 是本守卫这个常量没跟 —— 而它红的那条诊断（「生成器不在盘上」）指的方向是对的。
RENDERER = "tests/scripts/release-notes.mjs"

# ══ `19b`（2026-09-19）：产字节那条路 ═══════════════════════════════════════════
#
#: 条 63 的**承诺面**住这份账本，本文件只 import、**不抄第二份**
#: （`G4` 09-19 立，它是 `platform` 那一格的本体）。
LEDGER = "tests/evidence/K-G4-platform-ledger.py"
#: 门禁本体 —— ⑫ 要读它那条 `muslbuild` 裁词里点名的工具链版本。
GATE_SH = "tests/scripts/gate.sh"
#: 内嵌那一段的住址 —— ⑪ 读它，确认「抠不到 `BUILD_ID`」是一条响亮的失败。
BUILD_RS = "src/bridge/build.rs"

#: 🔴 **本文件里全部「跑 cargo 编后端」的步骤的完整登记** —— 与盘上现打**两向集合相等**。
#: 失效方向逐字：有人给一个**没承诺的平台**加一条交叉编译（例：arm64 的 Windows），
#: 而今天的输出面上一个字都不会说 ⇒ 那份跑不起来的字节会被静默发出去（条 63 `D7` 反面）。
#: 每条：(job, 步骤名逐字, 这一趟编出什么, 它喂给条 63 的哪一格)
COMPILE_STEPS = [
    ("build-backends", "Cross-compile backend for both musl targets",
     "两个 musl target 的静态字节（x86_64 ＋ aarch64）",
     "远端 Linux（musl 两个 arch）· 本机 Linux（同一份字节自释放，不另编）"),
    ("build-windows", "Build local backend (native)",
     "runner host triple 的原生 `.exe`",
     "本机 Windows x86_64"),
    ("build-linux", "Build local backend (native)",
     "runner host triple 的原生 glibc 字节",
     "🟡 **本机 Linux 的第二份来源** —— `设计/96 §7.3` 逐字「哪一份该留、哪一份该删，"
     "我判不了 —— 要先有条 62 那次迁移（步 `19`）把落点收成一个」。"
     "⇒ 本条**不是**在认可它，是把「同一格今天有两份来源」这件事钉成一个**数**："
     "变成三份会红，被人在裁之前偷偷删掉一份也会红"),
]

#: 🔴 **条 63 承诺的每一格，它的字节由本文件哪几步产出/铺到 `build.rs` 够得着的地方。**
#: `plat` 必须与账本 `PROMISED` 里那几个平台名**逐字相同**（⑨a 两向集合相等靠它）。
#: `steps` 里每一项 `(job, 步骤名)` 在那个 job 里 `count() == 1`（⑨c）。
BYTE_LINES = [
    {
        "plat": "本机 Windows x86_64",
        "runner": ("build-windows", "windows-latest"),
        "steps": [
            ("build-windows", "Build local backend (native)"),
            ("build-windows", "Stage local backend for externalBin"),
            ("build-windows", "Stage native backend for self-extract"),
        ],
        "into": "① Tauri `externalBin` ⇒ **只进安装包**（装完落在 exe 同目录）；"
                "② `build.rs::embed_native_backend` 的 `include_bytes!` ⇒ **进 exe 本体**（`K-R42`）",
    },
    {
        "plat": "远端 Linux（musl 两个 arch）",
        "runner": ("build-backends", "ubuntu-latest"),
        "steps": [
            ("build-backends", "Cross-compile backend for both musl targets"),
            ("build-backends", "Stage binaries"),
            ("build-windows", "Place + verify embedded backends"),
        ],
        "into": "artifact `embedded-backends` → `src/bridge/embedded-backends/` → "
                "`build.rs::embed_backends` 的 `include_bytes!` ⇒ 进 exe 本体",
    },
    {
        "plat": "本机 Linux",
        "runner": ("build-linux", "ubuntu-latest"),
        "steps": [
            ("build-linux", "Place embedded backends"),
        ],
        "into": "**不另编一份**（`设计/96 §7.1.5` 待点① 逐字：`local_daemon.rs::start_local_backend` 里"
                "那道 `cfg!(target_os = \"linux\")` 闸让本机 Linux 直接用远端那两份 **musl 静态**字节自释放）"
                "⇒ 这一格的产线增量是 **0**，它要的是**门禁多一格 ＋ 一次真机验**。"
                "⚠ 真机验这一维本判据**买不到**",
    },
]

#: 🔴 **本文件里出现的全部 target triple，与登记的那一份两向集合相等。**
#: 它是「没覆盖的格子怎么显式拒绝」落在**产线**这一侧的形状：条 63 显式拒绝的那一格
#: （arm64 的 Windows）在全仓是零脚印（`platform` 那一格的 `P3` 在盯），
#: 而本条盯的是**别的**没承诺的 triple 被悄悄加进产线。
TRIPLES = {"x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"}

#: 🔴 **每一处抠后端源码常量的住址**（路径, 常量名）→ 出现次数。**Counter 相等，不是包含。**
#: 它治的是一件真发生过的事：步 9（09-19）把 `BUILD_ID` 从 `main.rs` 搬到 `lib.rs`，
#: `release.yml` 里**两处**要抠它，那一拍**只改了一处** ⇒ 另一处指着一份**没有那个 const**
#: 的文件，真发版时死在「抠不到后端的 const BUILD_ID」上，而红的原因与它要守的事无关。
#: ⇒ 本条不只比登记，还**实打去读那份文件**，抠不出恰好一行就红（⑩b）。
IDENTITY_READS = {
    ("src/backend/lib.rs", "BUILD_ID"): 2,
    ("src/backend/lib.rs", "BUILD_STAMP_OPEN"): 1,
    ("src/backend/lib.rs", "BUILD_STAMP_CLOSE"): 1,
}

# ══ `19c`（2026-09-19）：`BUILD_ID` bump 的同拍债 —— re-embed ═══════════════════
#
# 🔴 **它为什么也归这一格，而不是新开第 25 格**
#
# `19b` 已经把「产字节那条路」的**两端**收进本格：⑪ 读 `src/bridge/build.rs`（吃字节那一侧）、
# ⑫ 读 `tests/scripts/gate.sh`（门禁那一侧的工具链版本）。`19c` 补的是同一条路上
# **本机产字节**那一端：一条 re-embed 命令，它的配方必须与 `release.yml` 那一步**同源**
# —— 判它就得同时读那两份文本，而「同时读那两份」正是本格的定义。
# ⇒ 本组判据与 ⑨⑩⑪⑫ 共用同一个被测面，拆出去就得把 `release.yml` 的解析再抄一份。
#
# 🔴 **它治的病**（`设计/99 §4` 步 `19c` 逐字）：协议面一变就要 bump `BUILD_ID`，
# 而 bump 那一刻 `embedded-backends/` 里那两份 musl 字节立刻变旧 ⇒ `build.rs` 的
# **半 bump 守卫**当场 panic，**整棵树编不过**（2026-09-18 实地踩过一次，四路 agent 同时编不过）。
# 今天盘上有一张 mtime 安全网，但那**只是安全网不是机制** —— 机制那一半是
# `tests/scripts/re-embed.sh`：一条真跑得起来的命令 ＋ 一条 `--check`。
#
# ⚠ **本组买不到什么**（与本文件头注第 4 条同一条边界，别读宽）：
#   · ⑬a–⑬f 全是**盘上文本**的对拍 —— 「配方写得一样」≠「那条命令今天在这台机器上跑得出字节」
#     （它要 `cargo-zigbuild` ＋ `zig`，本判据一次都不装、不跑）。
#   · ⑬g 真跑的是 `--check`（**只读**），不是 re-embed 本身。它在一棵**没铺字节**的树上
#     只答得出「这里没有一份对不上的字节」——那不是「字节是对的」。
#   · 「本机重编过了」**永远不等于**「发版那一拍办完了」：本机产物与发版 CI 的 zigbuild
#     形态不同（`build.rs` 自陈：static-pie / 未 strip / 不同 rustc）。

#: re-embed 那条命令的本体。
REEMBED_SH = "tests/scripts/re-embed.sh"
#: 落点的 gitignore 住址。
BRIDGE_GITIGNORE = "src/bridge/.gitignore"
#: 🔴 `build.rs` 里那个**出路唯一住址**常量的值，逐字。
#: 它指的脚本必须在盘上 —— 「出路指着一个不存在的命令」与「没有出路」在文案上一模一样。
REEMBED_CMD = "bash tests/scripts/re-embed.sh"
#: ⑬b 的对照物：`release.yml` 里产 musl 字节那一步（与 `COMPILE_STEPS` 第一条同源）。
MUSL_STEP = ("build-backends", "Cross-compile backend for both musl targets")
#: ⑬b 的第二个对照物：两个 job 里那条原生编译（`--native` 那一趟要与它同配方）。
NATIVE_STEP = "Build local backend (native)"
#: ⑬d 注册侧：`build.rs` 里登记落点目录名的那两个常量。
LANDING_CONSTS = ["EMBEDDED_BACKENDS_DIR", "NATIVE_BACKEND_DIR"]
#: ⑬d 第三个落点**不在 `build.rs` 里**：它是 Tauri `externalBin` 的暂存区，由
#: `release.yml` 的 `Stage local backend for externalBin` 铺，没有任何 Rust 代码提它的名字。
#: 🔴 它照样要被挡住 —— `设计/96 §7.1.2` 那条硬事实说的是「**三个**落点全部 gitignore」。
EXTERNALBIN_LANDING = "binaries"
#: ⑬d 盘侧：`.gitignore` 里每一行落点上面那句机检锚，逐字。
LANDING_MARK = "# ⇐ 内嵌落点"
#: ⑬e：这两个函数体里的**出路**必须点名 `REEMBED_CMD`，各至少两处（重编 ＋ 诚实关闭）。
EXIT_PATH_FNS = ["embed_backends", "embed_native_backend"]
#: ⑬e 禁词 —— 「手抄第二条产字节配方」回来的样子。**只在代码行上判，注释行剥掉**
#: （墓碑逐字引着那段旧配方，那是账不是指令；剥法见 `rust_code_lines`）。
BANNED_RECIPE = [
    ("--config 'target.", "把 `rust-lld` 那条本机专用配方抄回出路里 —— 它与发版那趟**不是同一条路**"),
    ("cargo build --release --target", "同上：第二条产字节配方 ＝ 第二个住址"),
    ("cargo zigbuild --target", "半条配方（漏了 `--release --locked`）⇒ 照它敲出来的字节不是发版那份"),
]

#: ⑪ `build.rs` 那一侧：这两个函数**必须**以 panic 结束「抠不到」那一支，
#: 且函数体里**不许**再出现兜底值（`设计/96 §7.2.5` 逐字：「`"unknown"` 这个值必须从类型上消失」）。
#: 每条：(函数名, 禁词逐条, 为什么)
NO_FALLBACK_FNS = [
    ("backend_source_build_id", ['"unknown"', "unwrap_or_default()"],
     "抠不到源码 `const BUILD_ID` 时给一个会参与比较的字符串 ⇒ 每台远端判 StaleBuild ⇒ "
     "无限重装（真事故，`设计/16 §5.4a`）"),
    ("backend_stamp_marks", ["unwrap_or_default()"],
     "抠不到身份戳界标时给一对空串 ⇒ 运行期拿空界标去扫，对**任何**字节都答不出身份 ⇒ "
     "与 `\"unknown\"` 同族的静默恒假"),
]


def read_rel(rel):
    """读仓内一份文件；读不到给 `None`（调用方按红记，不静默跳过）。"""
    try:
        return (ROOT / rel).read_text(encoding="utf-8")
    except OSError:
        return None


def promised_platforms():
    """条 63 承诺的平台集合 —— **唯一住址是那份账本**，本文件不抄。

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
    （那正是要判的），② 逐字引着旧配方的**整行墓碑注释**里（那是账，不是指令，
    本仓纪律要求留着）。剥掉整行注释刚好把这两者分开。
    """
    return "\n".join(l for l in body.splitlines() if not l.lstrip().startswith("//"))


def rust_str_const(src, name):
    """抠 `const <name>: &str = "…";` 的值。抠不到给 `None`（调用方按红记）。"""
    m = re.search(r'^\s*(?:pub )?const %s: &str = "([^"]+)";' % re.escape(name), src, re.M)
    return m.group(1) if m else None


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

    # ── ⑥ 每一处发布步骤都带正文来源（`KR124D2`）─────────────────────────────
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
    #   口径与 `K-R122` 的「build 排在 e2e 前面」同源：认字面量 + 认书写顺序。
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

    # ══ `19b`（09-19）：产字节那条路 ═══════════════════════════════════════════
    text = TARGET.read_text(encoding="utf-8")
    by_job = collections.Counter()
    for jname, _, st in steps:
        nm = st.get("name")
        if nm:
            by_job[(jname, str(nm))] += 1

    # ── ⑨a 条 63 承诺的每一格都有产线，且**没有**登记了却不在承诺面里的格（两向）────
    #   🔴 **反空真锚就是这一条**：单向「承诺表里每格都找得到」在承诺表被清空时恒真。
    promised = promised_platforms()
    if promised is None:
        check(False, "⑨a地板·条 63 的承诺面读得到",
              "`%s` 不在盘上 —— 承诺面没有住址了，本条**判不了，按红记**"
              "（绝不退化成「空集，于是两向相等」）" % LEDGER)
    elif isinstance(promised, tuple):
        check(False, "⑨a地板·条 63 的承诺面读得到",
              "import `%s` 失败：%s —— 判不了，按红记" % (LEDGER, promised[1]))
    else:
        mine = {b["plat"] for b in BYTE_LINES}
        check(mine == promised, "⑨a承诺的平台 ↔ 产线登记，两向集合相等",
              "承诺了却没产线 %s · 有产线却不在承诺面里 %s（承诺面现打 %s，唯一住址 `%s`）"
              % (sorted(promised - mine), sorted(mine - promised), sorted(promised), LEDGER))

    # ── ⑨b 本文件里「跑 cargo 编后端」的步骤 ↔ 登记，两向集合相等 ────────────────
    on_disk = set()
    for jname, _, st in steps:
        if re.search(r"\bcargo\s+(?:zigbuild|build)\b", str(st.get("run") or "")):
            on_disk.add((jname, str(st.get("name") or "<无名步骤>")))
    registered = {(j, n) for j, n, _, _ in COMPILE_STEPS}
    check(on_disk == registered, "⑨b编后端的步骤 ↔ 登记，两向集合相等",
          "盘上有而没登记 %s · 登记了而盘上没有 %s —— 前者是**给一个可能没承诺的平台"
          "悄悄加了一条产线**（条 63 `D7` 反面），后者是登记陈了"
          % (sorted(on_disk - registered), sorted(registered - on_disk)))

    # ── ⑨c 每一格登记的步骤在那个 job 里恰好一处 ─────────────────────────────────
    for b in BYTE_LINES:
        for jname, sname in b["steps"]:
            n = by_job[(jname, sname)]
            check(n == 1, "⑨c产线步骤·%s / %s / %s" % (b["plat"], jname, sname),
                  "在那个 job 里命中 %d 次（应当恰好 1 次）—— 0 = 这一格的字节今天没人产/没人铺" % n)

    # ── ⑨d runner 标签逐字 ──────────────────────────────────────────────────────
    #   ⚠ **它买到的只有「标签没被人换掉」**：`windows-latest` 今天是 x86_64，
    #     而那是 GitHub 说了算的事，本判据**问不出来**。条 63「本机 Windows 不含 arm64」
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
              "%s（应当恰好 1 行）—— 🔴 **这一格就是步 9 那次漏改的形状**："
              "住址指着一份没有那个 const 的文件，发版当场死在抽取上"
              % ("那份文件**读不到**" if hits is None else "命中 %d 行" % hits))

    # ── ⑪ 「抠不到」在 `build.rs` 那一侧是一条所有构建形态都响的失败 ────────────
    brs = read_rel(BUILD_RS)
    check(brs is not None, "⑪地板·`%s` 读得到" % BUILD_RS, "%s" % (ROOT / BUILD_RS))
    if brs is not None:
        for fname, banned, why in NO_FALLBACK_FNS:
            body = rust_fn_body(brs, fname)
            if body is None:
                check(False, "⑪`%s` 找得到" % fname,
                      "`%s` 里没有这个函数 —— 它被改名/删了，本条此刻是空真，按红记" % BUILD_RS)
                continue
            hit = [w for w in banned if w in body]
            check(not hit, "⑪`%s` 没有兜底值" % fname,
                  "函数体里出现 %r —— %s" % (hit, why))
            check("panic!" in body, "⑪`%s` 抠不到就当场失败" % fname,
                  "函数体里%s `panic!`（`设计/96 §7.2.5`：让「抠不到」在**所有**构建形态下都响，"
                  "而不是只在恰好铺了字节的那种）" % ("有" if "panic!" in body else "**没有**"))
        emitter = rust_fn_body(brs, "emit_backend_build_id")
        wired = bool(emitter) and "backend_source_build_id()" in emitter
        check(wired, "⑪`BACKEND_BUILD_ID` 的值取自那个会 panic 的住址",
              "`emit_backend_build_id` %s 调 `backend_source_build_id()` —— 不调它 = "
              "又开了一条绕过 panic 的取值路" % ("有" if wired else "**没有**"))

    # ── ⑫ 门禁 `muslbuild` 那一格的工具链版本 == 本文件真装的那两个 ──────────────
    #   G4 立那一格时逐字写着「版本一漂，本格的绿就不代表发版那趟会绿」——
    #   而在本条之前，**没有任何东西在核这句话**：两个版本号是两处手抄的。
    zig = next((str(((st.get("with") or {}).get("version")) or "")
                for _, _, st in steps if str(st.get("uses") or "").startswith("mlugg/setup-zig")), "")
    zb = next((str(((st.get("with") or {}).get("tool")) or "")
               for _, _, st in steps if str(st.get("uses") or "").startswith("taiki-e/install-action")), "")
    zb = zb.split("@")[-1] if "@" in zb else ""
    gate_text = read_rel(GATE_SH)
    verdict = ""
    if gate_text:
        m = re.search(r"^run_gate muslbuild '(.*)'\s*\\?$", gate_text, re.M)
        verdict = m.group(1) if m else ""
    check(bool(zig) and bool(zb) and bool(verdict), "⑫地板·两侧的版本都取得到",
          "`release.yml` zig=%r cargo-zigbuild=%r · `gate.sh` 那条 muslbuild 裁词%s"
          % (zig, zb, "取到了" if verdict else "**取不到**（改名/改行形了）"))
    if zig and zb and verdict:
        named = set(re.findall(r"\b(?:zig|cargo-zigbuild)\s+\**(\d+\.\d+\.\d+)", verdict))
        check(named == {zig, zb}, "⑫门禁那一格与发版那趟用同一套工具链",
              "裁词里点名 %s · `release.yml` 真装 %s —— 两向相等才算数："
              "版本一漂，`muslbuild` 那一格的绿就**不再代表**发版那趟会绿"
              % (sorted(named), sorted({zig, zb})))

    # ══ `19c`（09-19）：`BUILD_ID` bump 的同拍债 —— re-embed ═══════════════════
    reembed = read_rel(REEMBED_SH)
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

    # ── ⑬b 本机那条配方 ↔ `release.yml` 那一步，同源 ────────────────────────────
    #   🔴 **反空真锚是 target 那一条两向集合相等**：单向「脚本里每个 target 在 yml 里也有」
    #     在脚本把数组清空时恒真，而那正好等于「本机这条命令什么都不编」。
    targets = sh_array(reembed, "REEMBED_TARGETS")
    flags = sh_array(reembed, "REEMBED_BUILD_FLAGS")
    musl_run = next((str(st.get("run") or "") for jn, _, st in steps
                     if (jn, str(st.get("name") or "")) == MUSL_STEP), "")
    check(bool(targets) and bool(flags) and bool(musl_run), "⑬b地板·两侧的配方都取得到",
          "`%s` 的 REEMBED_TARGETS=%r REEMBED_BUILD_FLAGS=%r · `release.yml` 的 `%s / %s` 那一步%s"
          % (REEMBED_SH, targets, flags, MUSL_STEP[0], MUSL_STEP[1],
             "取到了" if musl_run else "**取不到**（改名/改 job 了）"))
    if targets and flags and musl_run:
        yml_targets = set(re.findall(r"--target\s+(\S+)", musl_run))
        check(set(targets) == yml_targets, "⑬b本机 re-embed 的 target ↔ 发版那一步，两向集合相等",
              "脚本有而发版没有 %s · 发版有而脚本没有 %s —— 差一个就是"
              "「本机 re-embed 完了，发版那趟还少一份字节」或者反过来"
              % (sorted(set(targets) - yml_targets), sorted(yml_targets - set(targets))))
        yml_flags = re.findall(r"cargo zigbuild\s+(.*?)\s+--target", musl_run)
        want = " ".join(flags)
        check(bool(yml_flags) and all(f == want for f in yml_flags),
              "⑬b本机 re-embed 的旗标 ↔ 发版那一步，逐字相同",
              "脚本 %r · 发版那一步现打 %r —— 少一个 `--locked` 就是"
              "「本机编的那份与发版编的那份依赖树可能不同」（理由住 `release.yml` 那一步的头注）"
              % (want, yml_flags))
        native_runs = [str(st.get("run") or "").strip() for _, _, st in steps
                       if str(st.get("name") or "") == NATIVE_STEP]
        check(bool(native_runs) and all(r == "cargo build " + want for r in native_runs),
              "⑬b`--native` 那一趟 ↔ 发版那两步，逐字相同",
              "发版那两步现打 %r · 脚本那一趟是 `cargo build %s` —— 本条盯的是"
              "「本机那一份字节」的配方，与上面 musl 那两格是两条产线" % (native_runs, want))

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
    #   🔴 **这一条今天有现物**：步 8 全仓改名（`daemon` → `backend`）之后，
    #     `.gitignore` 还写着 `/embedded-daemons/` 与 `/native-daemon/`
    #     ⇒ 09-19 现打 `git check-ignore` 两条都不命中，两个落点从那天起就没被挡住。
    #     而 `设计/96 §7.1.2` 与 `release.yml` 文件头都还把「三个落点全部 gitignore」
    #     当硬事实在用 —— **那句话在本拍之前是假的**，而它正是「门禁只能建在
    #     `release.yml` 上」这条推理的全部前提。
    registered = {rust_str_const(brs, c) or "<`const %s` 抠不到>" % c for c in LANDING_CONSTS}
    registered.add(EXTERNALBIN_LANDING)
    ignored = gitignore_landings(gi)
    check(registered == ignored, "⑬d内嵌落点 ↔ `.gitignore` 的机检锚，两向集合相等",
          "登记了却没被挡 %s · 挡着却没人登记 %s（登记侧 = `build.rs` 的 %s ＋ `externalBin` 那一格 "
          "`%s`；盘侧 = `%s` 里带 `%s` 锚的那几行）—— 前者是**re-embed 每跑一次就往工作树倒一次"
          "二十多 MB 垃圾**，后者是一条挡着空气的陈行"
          % (sorted(registered - ignored), sorted(ignored - registered),
             "/".join(LANDING_CONSTS), EXTERNALBIN_LANDING, BRIDGE_GITIGNORE, LANDING_MARK))
    for d in sorted(re.findall(r'^[A-Z_]+_DIR="\$ROOT/src/bridge/([^"]+)"', reembed, re.M)):
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
              "代码行里出现 %r —— %s。（整行 `//` 注释已剥掉：墓碑逐字引着那段旧配方是账，不是指令）"
              % ([w for w, _ in hit], "；".join(why for _, why in hit) or "—"))

    # ── ⑬f mtime 那张安全网还在，而且还在看**两份**源码 ─────────────────────────
    #   ⚠ 它是安全网不是机制（理由住脚本头注）—— 但「不是机制」不等于可以撤：
    #     撤了它，「字节比源码旧」这件事就只剩 id 那条硬校验在管，而 id 相同、
    #     内容不同（同一个 `BUILD_ID` 下改了代码）那一格**只有 mtime 看得见**。
    for want, why in [("backend_lib_rs()", "身份那一份"), ("backend_main_rs()", "分派那一份"),
                      (".max()", "取较新的那个（只看一份 ⇒ 改另一份时它当场变瞎）")]:
        check(want in eb, "⑬f mtime 安全网·%s" % want,
              "`embed_backends` 里%s —— %s" % ("有" if want in eb else "**没有**", why))
    check("比后端源码旧" in eb, "⑬f mtime 安全网还会出声",
          "那条 `cargo:warning` 的文案%s在" % ("" if "比后端源码旧" in eb else "**不**"))

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
          "🔴 **诚实边界（`19b` 加，写死别读宽）**：`release.yml` 那一族**在本地一步都真跑不起来** "
          "⇒ 买到的是「**盘上这几份文本满足上面逐行列出的那些条件**」，**不是**「云端那一趟会绿」；"
          "⑨ 那几条尤其是 —— 「登记的那一步在文件里」≠「那一步在 runner 上编得出字节」，"
          "更不等于「那份字节在目标机器上跑得起来」。真机行为仍是**判不了**，不是「通过」。"
          "🔴 **⑬（`19c`）那一组同一条边界**：⑬a–⑬f 全是盘上文本的对拍，"
          "「配方写得一样」≠「那条命令今天跑得出字节」；⑬g 真跑的是 `--check`（只读），"
          "在一棵没铺字节的树上它只答得出「这里没有一份对不上的字节」，"
          "**不是**「字节是对的」，更不是「发版那一拍办完了」。"
          "—— 逐条射程写在本文件头注）" % (n, TARGET))
    return rc


if __name__ == "__main__":
    sys.exit(main())
