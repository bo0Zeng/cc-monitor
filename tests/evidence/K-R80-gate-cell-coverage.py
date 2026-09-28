#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R80 `KR80D3`：门禁**每一格的分母各盖到哪几棵树**的登记 ＋ 它的机检。

## 它治的是什么

`K-R80` 的题面是：`fmt` 那一格的分母里逐字写着「`remote-daemon-proto` 是另一个
workspace，本行盖不到」——**一格「我盖不到那儿」的诚实注释，被当成了处置**。
盲区自己写在注释里，而没有任何东西在数它们。
⇒ 本尺子把「哪一格盖到哪几棵树」变成一份**逐格点名、机器对得上**的登记。

## ⚠ 它买得到什么、买不到什么（写死，别读宽）

**买得到**（下面 6 条 `C1`–`C6`，任一条不满足就 `exit 1`）：
  · `C1` `scripts/gate.sh` 里真有的判定格 ↔ 本表的键，**两侧互为子集**（多一格少一格都红）
  · `C2` 每条登记的**逐字锚点**在 `gate.sh` 里 `count() == 1`（登记指得到真东西，且不含糊）
  · `C3` 每一格对**每一棵树**都有一个裁词，取值只许是 `全 / 部 / 无`（**没有空白格**
        —— 这一条就是「**逐格点名它盖不到的那些**」的机器口径）
  · `C4` 树的全集 == `git ls-files` 现打的顶层目录（＋ vendor 那一棵单拆 ＋「仓根文件」一格）
        ⇒ 新增一棵顶层树而没人回来登记 ⇒ 红
  · `C5` `gate.sh` 末尾那行 `GATE: OK —— <n> 格全绿` 里的 `n` == 现打的判定格数
        ⇒ 加了一格而那行点名没跟 ⇒ 红（那一行 09-10 起就馊过一次：加了 `fmt`/`winchk`
          而它还写着「三道门 + 生成物漂移 + pb check + 四套 ccm e2e」= 9 格，盘上已是 11 格）
  · `C6` 每一个裁词都带一句**非空**的理由（`无` 也要写为什么盖不到）
  · `C6b`（`K-R82` 09-12 加）**0 格覆盖的那几棵树，要明写「为什么不需要门」，而那句话本身被钉住**
        —— 理由不许敷衍（黑名单 + 字数下限），且每条都拴一个盘上查得到的钉子
        （留档里的逐字串 / 它点名的那一格 / 它点名的那几份仓根文件 / `package.json` 里那个 key）；
        钉子没了或陈了 ⇒ 红。🔴 **「不需要」与「没查」在输出上一模一样，这一条要求写死是前者。**
        ⚠ 编号是 `C6b` 而**不是 `C7`**：本仓的 `C7` 是宪章那条「vendor 不动」，同名会互相冒充。
  · `C5b`（`K-R91` 09-12）**门禁自述射程** —— `gate.sh` 头注里「自己说自己跑几格 /
        跑哪些东西」那几句话，与**盘上现打的那些格**、**盘上现打的那些文件**对不上 ⇒ 红。
        🔴 **两半都判**：① 自述的格数（自述节 · 裁决行 · 现打格数，三方对拍）；
        ② **点名的东西在不在盘上** —— 头注 `:22–25` 那一形**一个数字都没有**，烂的是
        「点名了两个 09-11 就删掉的脚本」，**只认数字的判据认不出它**。
  · `C6c`（`K-R91` 09-12）**0 覆盖那棵树的理由，每一份成员都归到一档** ——
        分母是**那棵树的现打成员**（`git ls-files` 现打），**不是登记里写死的清单**；
        罩在一句全称句下而没被逐份归档的 ⇒ 红，且**逐字点名是哪几份**。
        ⚠ 它**不判归得对不对**（要读语义），只判**有没有归** —— 与 `C6b` 同一条边界。

**买不到**（同样是判据，只是方向相反 —— 别把绿读成这个）：
  · 它**不判裁词对不对**。`fmt` 那一格到底盖没盖住 `src-tauri`，机器在这里问不出来；
    它只保证**每一格都被表过态、锚点指得到真东西、格数对得上**。
    ⇒ 一条**写错的**裁词能骗过本尺子。**这是登记的机检，不是覆盖率的判据。**
  · `C6c` **不判「归得对不对」**，只判「有没有归」：把 `index.html` 归进「文档」那一档
    它照样绿。⇒ 它买的是**枚举盖住了那棵树**，不是**分档分对了**。
  · `C5b` **骗得过的那一形写死**：一句标着 `〔量于` 而内容其实是今天的自述 —— 那要读语义。
  · 它**不补任何盲区**（`K-R80 §0d` 逐字：数出来归数出来，补是另一件）。
    ⚠ `K-R82`（09-12）**补了其中一棵**：`hooks/` 从 0 格变成 1 格（`gate.sh` 里新的 `hooks` 格）。
    但那是**在 `gate.sh` 里加了一道真门**，本文件仍然只登记、只对拍 —— 这条边界一个字没变。

## 跑法

    python3 evidence/K-R80-gate-cell-coverage.py [<gate.sh 路径>]

不给参数就用仓根的 `scripts/gate.sh`（仓根 = 本文件的上一级）。
给参数是为了**对着变异过的副本跑**（死值验），不必去动真文件。
"""

import fnmatch
import json
import os
import re
import subprocess
import sys
from pathlib import Path

# `K_R80_ROOT` 只为**死值验**存在：把本文件拷进 scratchpad 变异之后，仓根仍要指回真工作树。
# ⚠ 它不是配置项，日常跑一律不带。
ROOT = Path(os.environ.get("K_R80_ROOT") or Path(__file__).resolve().parents[2])
# 🔴 〔订正 09-19〕默认住址原先写着 `ROOT / "scripts" / "gate.sh"` —— 那是**重构前**的住址。
# 重构后 `gate.sh` 搬进 `tests/scripts/`，于是不带参数跑本文件**当场 `FileNotFoundError`**。
# ⚠ **它崩得响，却没人听见** —— 本文件那时不在门禁的执行链上（只被 `gate.sh` 头注引为
#   「可复跑」）。两头坏叠在一起的后果实测：`pb check` 那一格 09-18 已整格删除，而裁决行
#   照旧印「21 格全绿」，挂了一整天没人响。本拍两头都治：住址改对，**并把本文件接成真的一格**。
GATE = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "tests" / "scripts" / "gate.sh"

# ── 树的全集 ────────────────────────────────────────────────────────────────
# 🔴 〔重写 09-19〕**整张表换成重构后的布局，并且把 `C4` 的单位从「顶层目录」改成「前缀分区」。**
#
#   上一版的 12 棵是重构前的地址（`src-tauri/` · `remote-daemon-proto/` · `e2e/` ·
#   `scripts/` · `shared/` · `hooks/` · `doc/` · `evidence/`），现打**一棵都不存在** ——
#   给它真住址跑一趟，`C4` 一口气吐 7 条「登记里的树盘上不存在」。
#
#   ⚠ **不能照着新的顶层目录原样抄。** 重构后 `git ls-files` 的顶层只剩 4 个
#   （`src/` `tests/` `.github/` `.cargo/`）——照抄就是把这张表压成 4 棵，
#   而这张表存在的全部意义是**那张盲区转置表**（`K-R80` 的题面就是「`fmt` 盖不到
#   另一个 workspace」这一条）。4 棵粗到每一格都同时盖 `src/` 与 `tests/` ⇒ 一句话都说不出。
#
#   ⇒ 改判据的**单位**，不改它买的东西：树按**前缀**登记、**首匹配**归属（长前缀写在前），
#     `C4` 从「集合等于顶层目录」换成两条更强的：
#       · **分区**：`git ls-files` 每一份都归到恰好一棵登记的树（catch-all 兜底 ⇒ 不可能漏）
#       · **非空**：每一棵登记的树现打份数 > 0（搬走/删空 ⇒ 红，这是上一版唯一守住的那条）
#     ⚠ 前者比上一版**严**：上一版只对拍顶层目录名，一份文件归没归进某棵树它不看。
#
#   两处刻意与目录结构不一样，各有理由：
#   · `src/bridge/vendor/` 从 `src/bridge/` 里**单拆出来** —— `C7` 逐字「vendor
#     `code-picture-core` **不动**」，它被 `cargo` 那一格显式 `--exclude`，与 `src/bridge/`
#     其余部分**受不同的门管**，混成一棵就点不出这一格盲区。
#     ⚠ 今天 vendor 下是**两棵**（`code-picture-core` 25 份 · `cc-acct-iso` 8 份），
#       上一版的 `VENDOR` 只指 `code-picture-core` ⇒ `cc-acct-iso` 当时落在 `src-tauri/` 里。
#   · `<仓根文件>` 是一格，装 `package.json` / `vite.config.ts` / `README.md` 那些
#     不属于任何目录的文件（`git ls-files` 里不含 `/` 的那些）。
#
# 〔现打 09-19〕15 棵合计 **1343** 份 == `git ls-files` 现打 1343（分区，不重不漏）。
VENDOR = "src/bridge/vendor/"
ROOTFILES = "<仓根文件>"
# 🔴 **顺序是承重的**：`tree_of()` 首匹配即归属 ⇒ 长前缀必须排在它的 catch-all 之前。
#    把 `src/` 挪到 `src/backend/` 前面，后端那 69 份会被前端那棵吞掉，而**一条判据都不红**。
TREES = [
    "src/backend/",          # 69   后端 Rust（重构前的 `remote-daemon-proto/`）
    VENDOR,                  # 33   vendor 两棵：code-picture-core 25 · cc-acct-iso 8
    "src/bridge/",           # 157  Tauri 侧 Rust（重构前的 `src-tauri/`），不含 vendor
    "src/generated/",        # 82   `ts_rs` 生成物（重构前不单列）
    "src/shared/",           # 19   重构前的 `shared/`
    "src/doc/",              # 9    重构前的 `doc/`
    "src/",                  # 126  前端 TS —— catch-all，必须排在上面几棵之后
    "tests/e2e/",            # 54   重构前的 `e2e/`
    "tests/evidence/",       # 384  重构前的 `evidence/`
    "tests/scripts/",        # 7    重构前的 `scripts/`
    "tests/hooks/",          # 1    重构前的 `hooks/`
    "tests/",                # 384  判据本体（bridge/ backend/ views/ settings/ + vitest）
    ".github/",              # 3
    ".cargo/",               # 1
    ROOTFILES,               # 14
]


def tree_of(path):
    """一份文件归哪棵树 —— **首匹配**。不含 `/` 的归 `<仓根文件>`。"""
    for t in TREES:
        if t is not ROOTFILES and path.startswith(t):
            return t
    return ROOTFILES


FULL, PART, NONE = "全", "部", "无"

# ── 登记 ────────────────────────────────────────────────────────────────────
# 每一格：`anchor` = 在 `gate.sh` 里逐字唯一的一串（`C2` 数它 == 1）；
#         `cover` = **对每一棵树**的裁词 + 一句理由（`C3`/`C6` 要求一格不空）。
#
# ⚠ 「部」的注里凡是带数字的，都注明是**哪把尺子现打的**；本文件末尾那段
#   `cross_tree_reads()` 会把那把尺子再跑一遍印出来 —— 它**不是判据**，是让这些数可复算。
def blind(reason):
    return (NONE, reason)


OUT_OF_TREE = "本格的命令根本不进这棵树"


def all_blind(reason=OUT_OF_TREE):
    return {t: blind(reason) for t in TREES}


REGISTRY = {}


def cell(name, anchor, cwd, cmd, **verdicts):
    c = all_blind()
    c.update(verdicts)
    REGISTRY[name] = {"anchor": anchor, "cwd": cwd, "cmd": cmd, "cover": c}


# ── `K-R82`（09-12）：第 13 格 `hooks` ────────────────────────────────────────
# 它治的正是本文件下面那张转置表印出来的一行：`hooks/` 一格都盖不到，
# 而它是**会被 git 执行、跑在每一次提交上、能改仓**的东西（`DECISIONS.md#R42` 裁定四）。
# ⚠ **不另起一份「hooks 覆盖登记」** —— 两份账必漂（`KR82D2` 逐字点名的失效方向）。
#   这一格由本文件自己的 `C1`（名字对得上）· `C2`（锚点唯一）· `C3`/`C6`（12 棵树逐棵表态带理由）
#   · `C5`（裁决行 13 格）接住，一条新判据都没另立。
cell(
    "hooks",
    anchor="run_gate hooks '每个被跟踪的 hook 文件 3 条",
    cwd="仓根",
    cmd="bash tests/scripts/hooks-are-runnable.sh",
    **{
        "tests/hooks/": (FULL, "`git ls-files hooks/` 下**每一个**被跟踪的文件，各 3 条："
                         "盘上可执行（`test -x`）· 库里记着可执行位（index mode `100755`）· "
                         "语法过得了它自己 shebang 声明的解释器。⚠ 买的是「**跑得起来**」这一层，"
                         "**不买「它拦得对」**——`pre-commit` 那条 `[profile.dev]` 正则判得准不准，"
                         "本格一个字都问不出来"),
        "tests/scripts/": blind("本格的**尺子**住在 `scripts/hooks-are-runnable.sh` —— "
                          "但尺子不是分母。把量具算进它自己的覆盖，正是本区最高频那族病"
                          "（量具的作用域对不上事实）⇒ 这里刻意判「无」"),
    },
)

# ── `K-R115`（09-14）：第 14 格 `copy2` ───────────────────────────────────────
# 它是**第一格盖到 `evidence/`** 的门 ⇒ 那棵树今天不再是「0 格覆盖」，
# 下面 `NO_GATE_NEEDED` 里它那一条**同拍删掉**（留着 `C6b` 会红：那条说明陈了）。
# ⚠ 那条说明当初写的是「0 格覆盖正是它的用途 —— `[J3 陈账]` 死锁的泄压口」。
#   **泄压口在实质上还在**：本格只在两种情况下红（某处 `copy2` 的目的地落在被 git
#   跟踪的树内内容上 · 某份 `.py` 连 `ast` 都解析不了），改一份 `.md` / 加一份读数
#   一格读数都不动。**但那条登记的字面不再成立**，所以删它、不是改它。
cell(
    "copy2",
    # 〔`K-R122` 09-14〕锚点跟着 `gate.sh` 那一行改了：原先是
    #   `run_gate copy2 '`evidence/*.py` 现打` —— 那个「现打」后面跟的是一个**手抄的份数**，
    #   本件把它摘了（份数以判据本体自己印的那一行为准）⇒ 锚点收到「不含那个数」的那一段。
    anchor="run_gate copy2 '`evidence/*.py` 里，`shutil` 保元数据复制族",
    cwd="仓根",
    cmd="python3 tests/evidence/K-R115-ruler.py",
    **{
        "tests/evidence/": (PART, "只判 `evidence/*.py` 里 `shutil` 保元数据复制族"
                            "（`copy2` / `copytree` / `copystat`）的**调用点**"
                            "（现打 11 处）：它的目的地落不落在被 git 跟踪的树内内容上。"
                            "⚠ 这棵树的其余部分（`.md` 留档 · `.tsv`/`.json` 读数 · "
                            "`.py` 里除这一族之外的每一行）本格**一个字都不问**。"
                            "⚠ 尺子自己也住这棵树（`evidence/K-R115-ruler.py`）"
                            "—— 它自己那份里这一族**零命中**，所以不构成自匹配；"
                            "哪天它自己用上了，本格会把它和别人一样判一遍"),
    },
)

# ── `K-R128`（09-15）：第 21 格 `installface` ─────────────────────────────────
# 判据本体 = `evidence/K-R117-ruler.py`（`K-R117` 第一拍那把摸底尺子，`K-R128` 往上加了
# `R8`/`R9`/`R10` 三条判定）。它盖到的是**安装面那 22 条命令**这一条窄线，
# 横跨 `src/`（前端落点）· `src-tauri/`（`LEDGER` / `claims()` / 命令定义）两棵树。
# ⚠ **不另起一份覆盖登记** —— 与 `hooks` 那一格同一条道理：两份账必漂。
cell(
    "installface",
    anchor="run_gate installface '判过的条数（`§S5c`/`§S5d`/`§S5e` 三节",
    cwd="仓根",
    cmd="python3 tests/evidence/K-R117-ruler.py",
    **{
        "src/": (PART, "只判**安装面那 22 条命令**在 `src/**.ts` 里的**落点分布**"
                       "（哪几份文件调它们，现打 8 份）＋ 它们在包装层 "
                       "`src/ipc/commands.ts` 里的入口在不在（TS 键 ＋ `invoke` 线上串两侧）。"
                       "⚠ 这棵树的其余部分本格**一个字都不问**：别的命令、组件、样式、"
                       "任何一行逻辑对不对，一格读数都不动。"
                       "⚠ 连这 22 条**调用得对不对**也不问 —— 只问「在哪几份文件里被调」。"
                       "⚠ 只认调用形状 `.<命令>(`；`invoke(\"<名>\")` 直呼它看不见"
                       "（那一档在 `§S5d` 第二档里只出读数、不判红）"),
        "src/bridge/": (PART, "只判那 22 条命令的**名字**在三份共用文件里的形状："
                             "`tool_registry.rs` 的 `claims()` 每个装 / 卸符号要么解析到一条真 "
                             "`#[tauri::command]`、要么在明示的非命令名单里。"
                             "⚠ `parity_ledger.rs` 那一份**判不了**（那 22 条就是从它解析出来的 "
                             "⇒ 空真），它的闸在同一份尺子的 `§S5c` 闭集判定，不在这一档。"
                             "⚠ 这棵树的 Rust 逻辑、错误处理、`Side` 栏对不对，本格一概不问"),
        "tests/evidence/": blind("本格的**尺子**就住这棵树（`evidence/K-R117-ruler.py`）—— "
                           "尺子不是分母。把量具算进它自己的覆盖，正是本区最高频那族病"
                           "（量具的作用域对不上事实）⇒ 这里刻意判「无」，"
                           "与 `hooks` / `copy2` 两格对自己那棵树的判法一致"),
    },
)

# ── `K-R115`（09-14）：第 15 格 `deadcode` ────────────────────────────────────
# `KR115D2` 甲：`dead_code` 这一维此前门禁**没有格**，`K-R109` 要量它只能自己拼一条
# `docker run … cargo check`（因此破了「唯一许可命令」的字面）。本格把它收进来。
cell(
    "deadcode",
    anchor="run_gate deadcode '`cargo check -p monitor`",
    cwd="src-tauri/",
    cmd="cargo check -p monitor（非 test）＋ never used 递减棘轮",
    **{
        "src/bridge/": (PART, "**只有 `-p monitor` 一个包的生产段**：非 test 构建里 "
                             "`never used` 的条数，上限 54 / 下限 40 的递减棘轮。"
                             "⚠ 同 workspace 的其余成员不在 `-p` 里；`#[cfg(test)]` 里的"
                             "死代码任何非 test 构建都看不见 —— 这两块本格都盖不到"),
        VENDOR: blind("`-p monitor` 只编它自己那一个包的生产段；vendor 作为依赖被编，"
                      "而依赖的警告不进 `-p` 那个包的 `never used` 计数（`cargo` 只报"
                      "本包的 lint）⇒ 这棵树本格一条都数不到"),
        "src/backend/": blind("另一个 workspace，`-p monitor` 够不着 —— "
                                      "它那棵树的 `dead_code` 今天**仍然没有格**，"
                                      "这是本格明写的盲区，不是漏登"),
    },
)

# ── `K-R122`（09-14）：第 17 格 `shellcheck` ──────────────────────────────────
# `KR122D2` 甲：**这一维此前门禁一格都没有**（`grep -c -i shellcheck scripts/gate.sh` 当时 = 0），
# 而它在 CI 里是独立一个 job —— `K-R119` 推 tag 那一趟五条红里有一条就是它。
# ⚠ 本格**不另起一份人群清单**：它把 `.github/workflows/ci.yml` 那段 `FILES=` 现读进来展开
#   （那份清单同时是 `src-tauri/src/shell_lint_registry.rs` 的被解析对象）⇒ 一个闭集一个住址。
cell(
    "shellcheck",
    anchor="run_gate shellcheck '不是「几条断言过了」",
    cwd="仓根",
    cmd="shellcheck --severity=error <人群从 ci.yml 现读>",
    **{
        "tests/e2e/": (PART, "现打 `e2e/*.sh` **30** 份 ＋ `e2e/weak-net/*.sh` **4** 份 ＋ "
                       "`e2e/fake-claude` **1** 份 = 35 / 这棵树现打 **50** 份。"
                       "⚠ 剩下那 15 份（`.ts` / `.py` / `.tsv` / `fixtures/`）本格一行都不读；"
                       "⚠ 买的是「`--severity=error` 这一档没有告警」，**不买「脚本干得对」**"),
        "src/shared/": (PART, "只有 `shared/cc-bus/scripts/*` **14** 份 / 这棵树现打 **19** 份 —— "
                          "`shared/cc-bus/examples/` 那 3 份与仓根那 1 份不在人群里"),
        "tests/scripts/": (PART, "`scripts/*.sh` 现打 **3** 份 / 这棵树现打 **6** 份 —— "
                           "`run.ps1` 全仓没有 linter（`audit-0805` 登记的诚实边界），"
                           "`assert-coverage-floors.mjs` 是 JS"),
        "src/bridge/": (PART, "只有 vendored `cc-acct-iso` 那 **4** 份 bash（逐份点名，不用 glob —— "
                             "`ci.yml` 那段注释逐字记着为什么：不开 globstar 时 `**` 等价于 `*`，"
                             "会把一个目录喂给 shellcheck ⇒ 恒红）。这棵树的其余部分与本格无关"),
        "tests/hooks/": (PART, "只有 `hooks/pre-commit` 这**一份**，而且是**逐份点名**进人群的、不是 glob "
                         "⇒ 往这棵树加第二份 hook，本格看不见它（那一维由 `hooks` 那一格的 "
                         "`git ls-files hooks/` 盖）"),
        ".github/": blind("`ci.yml` 在本格里是**人群清单**（被读的那份配置），不是被检对象 —— "
                          "把量具自己算进它的覆盖，正是本区最高频那族病"),
    },
)

cell(
    "fmt",
    anchor="run_gate fmt '不是数出来的数",
    cwd="src-tauri/",
    cmd="cargo fmt --all --check",
    **{
        "tests/": ("部", "〔现打 09-19〕**死值验证过**：往 `tests/bridge/accounts_tests.rs` 尾部加一行乱格式，`cargo fmt --all --check` 当场 `rc=1` ⇒ rustfmt 顺着 `#[path = '../../../tests/bridge/…']` 进了这棵树。⚠ 只盖 `tests/bridge/` 那 152 份 `.rs`，这棵树里 155 份 `.ts` 它一份不碰"),
        "src/bridge/": (PART, "只有那 8 个 workspace 成员从 crate 根顺 mod 走得到的 `.rs`；"
                             "`tauri.conf.json` / `Cargo.toml` 那些非 Rust 文件一概不在"),
        VENDOR: blind("`[workspace] exclude` 把它排掉了 —— 而 `C7` 逐字「vendor 不动」，"
                      "把它拉进出货门禁就是一道我们满足不了的闸"),
        "src/backend/": blind("另一个 workspace —— **这就是 `K-R80` 的题面**，"
                                      "今天由 `fmt-daemon` 那一格盖"),
    },
)

cell(
    "fmt-backend",
    anchor="run_gate fmt-backend '不是数出来的数",
    cwd="remote-daemon-proto/",
    cmd="cargo fmt --check（刻意不加 --all）",
    **{
        "tests/": ("部", "〔现打 09-19〕同法死值验：`tests/backend/inbound_tests.rs` 加乱行 ⇒ `rc=1`。只盖 `tests/backend/` 那 75 份 `.rs`"),
        "src/backend/": (PART, "唯一成员 `cc-monitor-remote`，射程 = 从 `src/main.rs` "
                                       "顺 `mod` 走得到的那些 `.rs`；走不到的文件本格看不见"),
        "src/bridge/": blind("刻意**不加** `--all`：加了 rustfmt 实收 12 个 crate 根、11 个在这棵树外"),
        VENDOR: blind("同上 —— 加 `--all` 会把这棵我们无权修的树拉进来"),
    },
)

cell(
    "winchk",
    anchor="run_gate winchk '不是数出来的数",
    cwd="src-tauri/",
    cmd="cargo check --locked -p monitor --target x86_64-pc-windows-gnu",
    **{
        "tests/": ("无", "〔现打 09-19〕本格是 `-p monitor` **不带 `--all-targets`** ⇒ 只编 lib ＋ bin，test target 一个不编 ⇒ `tests/bridge/` 那 152 份在 Windows 目标下**没人编过**"),
        "src/bridge/": (PART, "`-p monitor` 一个包的**生产段编得过**；7 个共享 crate 作为依赖被编。"
                             "⚠ `#[cfg(test)]` 不进 `check`，「行为对」更买不到"),
        VENDOR: (PART, "作为 `monitor` 的依赖被编。⚠ 这一条**本件没现打**，"
                       "是从依赖关系推的 —— 按「未验」读"),
        "src/backend/": blind("本格逐字写着它盖不到：那 17 处 `cfg(windows)` 没人跨编"),
    },
)

# ── WIN1（第四波 4D）：第 30 格 `winlink` ───────────────────────────────────
# 守的要求：用户裁决 V115 那一趟 RT1 的 F1（`RT1.md §8`）—— `winchk` 只 `check`、不链接，
# 而 `-gnu` 交叉链接 `monitor_lib.dll` 当场 `export ordinal too large`。本格真链两个二进制。
cell(
    "winlink",
    anchor="run_gate winlink '不是数出来的数",
    cwd="src/bridge/",
    cmd="cargo build --locked -p monitor --bins --target x86_64-pc-windows-gnu",
    **{
        "src/bridge/": (PART, "`-p monitor` 的两个二进制（`monitor` · `cc-monitor-filewin`）在 Windows target 上"
                             "**真链接**一趟（dev）。⚠ 只链不跑；test 档不链（那一半是 `winchk` 的 `check`）；"
                             "`-gnu` 不是 `-msvc`"),
        VENDOR: (PART, "作为 `monitor` 的依赖被编、被链。⚠ 按依赖关系推的，按「未验」读"),
        "src/backend/": blind("另一个 workspace；它在 Windows 上编不编得过归 `winchk-backend`，链接这一维本格不管"),
    },
)

# ── `K-R122`（09-14）：第 18 格 `ci-e2e-prereq` ───────────────────────────────
# `KR122D1` ③④：`K-R119` 那趟云端五条红里有**两条**是「job 的前置没跟上产品变化」——
# 而本脚本自己在跑四套 ccm e2e 之前有一步 build，CI 那两个 job 没有 ⇒ 两边的「绿」同形。
# ⚠ 本格是**第一格把 `.github/` 当被测对象**的门（在它之前那棵树只被 `cargo` 那格
#   「经扫描型守卫读进去」地擦到 3 个读点）。
cell(
    "ci-e2e-prereq",
    anchor="run_gate ci-e2e-prereq '判过的 e2e 调用行数",
    cwd="仓根",
    cmd="python3 tests/evidence/K-R122-ruler.py",
    **{
        ".github/": (PART, "只判 `ci.yml` 一份文件里的**一个切片**：`steps:` 里那 20 条 e2e "
                           "调用行，各自的 build 前置齐不齐。⚠ 这棵树的其余部分"
                           "（`release.yml` 整份 · 那些 job 的 runner / 工具链 / needs / if）"
                           "本格一个字都不问；⚠ 它**不跑任何 e2e**，「前置齐了」≠「那一套会绿」"),
        "tests/e2e/": (PART, "那 20 条调用行指到的 `.sh`（现打 20 份，其中 15 份硬门后端二进制）"
                       "**只被读一个字面量**（`debug/cc-monitor-remote` 在不在）—— "
                       "脚本里的任何一行断言、任何一处行为，本格都不看"),
        ROOTFILES: blind("`package.json` 的 `scripts` 那一块在本格里是**索引**"
                         "（把套件名解析到那份 `.sh`），**不是被检对象** —— "
                         "与上面 `shellcheck` 那一格判 `.github/` 「无」同一条理由："
                         "被读的那份配置不算被它盖到。⚠ 这一条判「无」是**有代价**的："
                         "`test:<套件>` 那条脚本被改坏时本格报的是「找不到脚本」，"
                         "而那句诊断说的是**索引坏了**，不是「仓根文件有问题」"),
        "tests/scripts/": blind("本格的**尺子**住 `evidence/K-R122-ruler.py`，不在这棵树上；"
                          "而尺子本来也不该算进自己的覆盖"),
        "tests/evidence/": blind("同上 —— 把量具自己算进它的覆盖，正是本区最高频那族病"),
    },
)

# ── `K-R122`（09-14）：第 18 格 `winchk-daemon` ───────────────────────────────
# `KR122D2` 甲：上面 `winchk` 那一格的裁词逐字写着「那 17 处 `cfg(windows)` 没人跨编」，
# 而 `K-R119` 那一趟云端正是红在它上面（daemon job 第 7 步，**10 个编译错全在 test 档**）。
# ⚠ 与 CI 的差别写在 `gate.sh` 那一格的分母里（target `-gnu` vs `-msvc`），这里不抄第二份。
cell(
    "winchk-backend",
    anchor="run_gate winchk-backend '不是数出来的数",
    cwd="remote-daemon-proto/",
    cmd="cargo check --all-targets --target x86_64-pc-windows-gnu",
    **{
        "tests/evidence/": blind("〔TQ1 09-24〕这棵树里原有的那一条活 `[[bench]]`（秤 7）搬去了 `tests/benches/`（bench 源码的唯一住址）"
                                 "⇒ 这棵树今天没有一份被本格编译；它的读数 `.md` ／ 一次性量具 `.py` 本格一份不碰。"
                                 "（09-19 的现物照记：步 8 改名漏了那份 bench 里的 `CARGO_BIN_EXE_cc-monitor-remote`，宿主 `cargo test` 全绿，**只有本格红**）"),
        "tests/": ("部", "〔现打 09-19〕本格带 `--all-targets` ⇒ test target 也编 ⇒ 盖 `tests/backend/` 那 75 份；"
                         "〔TQ1 09-24〕bench target 也编 ⇒ 盖 `tests/benches/` 下 `[[bench]]` 指着的那份（秤 7）。"
                         "★ 这条有现物：步 8 改名漏了秤 7 里的 `CARGO_BIN_EXE_cc-monitor-remote`，宿主 `cargo test` 全绿，**只有本格红**"),
        "src/backend/": (PART, "唯一成员 `cc-monitor-remote` 的**生产段 ＋ test 档**"
                                       "（`--all-targets` 是承重的：云端那 10 个错一个都不在生产段）"
                                       "在 Windows target 上**编得过**。"
                                       "⚠ 只买「编得过」，**买不到「在 Windows 上跑得对」** —— "
                                       "`check` 一行代码都不执行；⚠ `-gnu` 不是 `-msvc`，"
                                       "MSVC ABI 专属的那一类本格盖不到"),
        "src/bridge/": blind("另一个 workspace，由上面 `winchk` 那一格盖"),
        VENDOR: blind("同上 —— 它是 `src-tauri` 那棵的依赖，本命令的编译图里没有它"),
    },
)

# ── `K-R124`（09-15）：第 20 格 `release-gate` ────────────────────────────────
# `KR124D1`/`KR124D2`：`ci.yml` 里那条「`release.yml` 手工触发守卫」**从加进去那天起就不可能过**
# （`run:` 块里的 `${{ … }}` 被 runner 先求值 ⇒ 要比的字面渲染成 `"false"`），
# 而它用 `yaml.safe_load` 写 ⇒ **在本地一次都跑不起来** ⇒ 两头都看不见，坏了一个月。
# ⚠ 本格与 CI 那一步**跑的是同一份判据本体**（`evidence/K-R124-ruler.py`），不是两份抄件
#   —— 所以这里不重抄它的射程，射程住那份文件的头注。
cell(
    "release-gate",
    anchor="run_gate release-gate '判过的条数",
    cwd="仓根",
    cmd="python3 tests/evidence/K-R124-ruler.py",
    **{
        ".github/": (PART, "只判 `release.yml` **一份文件里点名的那几处**：触发器解析得出来 · "
                           "`workflow_dispatch` 在不在 · `inputs.publish` 的 type/default · "
                           "`env.PUBLISH` 的**字面** · 两处「往 Release 上写」与 CI 门那一步的 `if:` · "
                           "两处发布步骤各自的正文来源。"
                           "〔`19b` 09-19 加〕再加**产字节那条路**：条 63 承诺的三格 ↔ 产线两向相等 · "
                           "本文件里「跑 `cargo build`/`zigbuild`」的步骤 ↔ 登记两向相等 · "
                           "出现的 target triple ↔ 登记两向相等 · 三个 job 的 `runs-on` 逐字 · "
                           "每一处抠 `const BUILD_ID`／身份戳界标的 `-Path` 解出来的住址逐处计数相等 · "
                           "`mlugg/setup-zig` 与 `taiki-e/install-action` 那两个版本。"
                           "⚠ 这份文件的其余每一步（needs / 缓存 / 打包 / 校验和 / 上传清单）本格一个字不问；"
                           "⚠ `ci.yml` 整份**不在本格射程里**（那棵树的切片由 `shellcheck` 与 "
                           "`ci-e2e-prereq` 两格各判一块）；⚠ 它**不跑那条流水线**，"
                           "「盘上这份文本满足这几条」≠「云端那一趟会绿」——"
                           "「登记的那一步在文件里」也**不等于**「那一步在 runner 上编得出字节」"),
        "tests/scripts/": (PART, "三份，各只判一点点：① `release-notes.mjs` —— 它在不在盘上、"
                           "`--check` 跑不跑得出一段非空的正文（那份文件里的段落切法 · 拼装 · "
                           "写文件那一半本格都不看）；② 〔`19b` 09-19 加〕`gate.sh` —— **只读**"
                           "`run_gate muslbuild '…'` 那一条裁词里点名的工具链版本，与 `release.yml` "
                           "真装的那两个两向对拍。`gate.sh` 的其余每一行本格一个字不看"
                           "（那是 `gate-selfdesc` 那一格的事）；"
                           "③ 〔`19c` 09-19 加〕`re-embed.sh` —— 在不在盘上 · 它那两行配方登记"
                           "（`REEMBED_TARGETS` / `REEMBED_BUILD_FLAGS`）与 `release.yml` 产字节那两步"
                           "**两向对拍** · 它往哪两个目录倒字节 · **真跑一次 `--check`**（只读，要有数）。"
                           "⚠ 那份脚本里的编译与拷贝那一半本格**不跑**（它要 zig ＋ cargo-zigbuild，"
                           "本格一个都不装）"),
        "src/bridge/": (PART, "〔`19b` 09-19 加〕**只读 `build.rs` 里那两个取身份的函数**"
                           "（`backend_source_build_id` / `backend_stamp_marks` ＋ "
                           "`emit_backend_build_id` 调不调前者）：函数体里不许再有兜底值"
                           "（`\"unknown\"` / `unwrap_or_default()`），且「抠不到」那一支必须 `panic!`。"
                           "〔`19c` 09-19 再加两处，仍只读文本〕④ `build.rs` 的三个常量与两个内嵌函数："
                           "`REEMBED_CMD` 逐字 · `EMBEDDED_BACKENDS_DIR`/`NATIVE_BACKEND_DIR` 的值 · "
                           "`embed_backends`/`embed_native_backend` 的出路各点名那条命令 ≥2 处、"
                           "代码行里不许手抄第二条产字节配方 · mtime 安全网仍看**两份**源码；"
                           "⑤ `src/bridge/.gitignore` —— **只读**带 `⇐ 内嵌落点` 锚的那几行，"
                           "与上面那几个落点常量两向集合相等（现物：步 8 改名后那两行指空过）。"
                           "⚠ 买的是**源码形状**，不是行为 —— 它不编也不跑 `build.rs`；"
                           "这棵树的其余每一份本格一个字不碰"),
        "src/backend/": (PART, "〔`19b` 09-19 加〕**只读 `lib.rs` 里那三行 `const`**"
                           "（`BUILD_ID` · `BUILD_STAMP_OPEN` · `BUILD_STAMP_CLOSE`），"
                           "各要求**恰好 1 行**命中 —— 这是「`release.yml` 里那几处抽取住址"
                           "指不指得到真东西」的实打那一半（步 9 漏改一处的现物就是这一格逮的）。"
                           "⚠ 常量的**值**对不对本格不判，这棵树的其余每一份也不碰"),
        ROOTFILES: (PART, "经生成器 `--check` 读进去的那两份：`package.json` 的 `version`、"
                          "以及 `CHANGELOG.md` 里 `## [<version>]` 那一段**在不在、非不非空**。"
                          "⚠ 这是**分母**不是依赖（本格真的在判它们：版本号没有对应段 ⇒ 本格红）；"
                          "⚠ 那一段**写得对不对**本格一个字不判"),
        "tests/evidence/": blind("本格的尺子自己就住在这棵树上 —— 把量具算进它自己的覆盖，"
                           "正是本区最高频那族病（与上面 `ci-e2e-prereq` 同一条理由）。"
                           "〔`19b` 09-19 补一句〕它还 **import** 同树的 `K-G4-platform-ledger.py` "
                           "取条 63 的承诺面（那是全仓唯一一份，不许抄第二份）—— "
                           "**那是取值，不是覆盖**：本格对那份账本的内容一个字都不判，"
                           "判它的是 `platform` 那一格"),
    },
)

cell(
    "cargo",
    # 🔴 〔`K-R115` 09-14〕**这条锚点在本件之前就已经指空了**：它写着 `cargo 8`，
    #   而 `gate.sh` 现打是 `run_gate_sum cargo 9`（workspace 长到 9 个成员那天没人回来改）。
    #   ⇒ 本尺子在**本件动它之前**就红着一条 `C2`（现打读数住
    #   `evidence/K-R115-deathvalue.md#§E`）。这不是本件弄红的，是本件顺手量到的。
    # 〔CP2c 09-25〕新共享 crate `copy-core` 进 workspace ⇒ 成员 9 → 10，`gate.sh` 那行同拍改成 `cargo 10`；
    #   本锚点当时没人跟 ⇒ 合并列车那次门禁 `gate-selfdesc` C2 红，主会话在这里跟上。
    # 〔US1 · 4D〕新共享 crate `relay-route-core` ⇒ 成员 10 → 11，`gate.sh` 同拍改 `cargo 11`，本锚点同拍跟上。
    # 〔DUP2 · 4D〕新共享 crate `agent-tools-core`（J19）⇒ 成员 11 → 12，`gate.sh` 同拍改 `cargo 12`，本锚点同拍跟上。
    # 〔DUP3 · 4D〕新共享 crate `upstream-url-core`（J9）⇒ 成员 12 → 13（本路增量 ＋1），`gate.sh` 同拍改 `cargo 13`，本锚点同拍跟上。
    anchor="run_gate_sum cargo 13 bash -c",
    cwd="src-tauri/",
    cmd="cargo test --workspace --lib",
    **{
        "src/generated/": ("无", "〔现打 09-19〕这棵树是 `.ts`，Rust 那侧碰不到它 —— **盯「Rust 源改了而生成物没跟」的是 `generated` 那一格，不是本格**"),
        "tests/": ("部", "〔现打 09-19〕`tests/bridge/` 那 152 份 `.rs` 靠 `src/bridge/src/*.rs` 里的 `#[path]` 挂进 crate ⇒ 本格**编它们、跑它们**（步 7b 把测试段整批搬出生产树之后，两棵生产树的真 `#[test]` 是 0/0，测试全在这棵树里）。⚠ `.ts` 那一半本格看不见"),
        "src/bridge/": (FULL, "13 个成员的 `--lib` 判据，合计求和 + 包数相等断言（CP2c 加 `copy-core` 后 9 → 10 · US1 加 `relay-route-core` 后 10 → 11 · DUP2 加 `agent-tools-core` 后 11 → 12 · DUP3 加 `upstream-url-core` 后 12 → 13）"
                             "〔09-14 现打：`gate.sh` 那行是 `run_gate_sum cargo 9`；"
                             "上一版这里与锚点都写着 8〕"),
        VENDOR: blind("显式 `--exclude code-picture-core`（`C7`：vendor 不动）"),
        "src/backend/": (PART, "**经扫描型守卫读进去**：现打 18 个读点"
                                       "（尺子见本文件 `cross_tree_reads()`）"),
        "tests/scripts/": (PART, "同上，现打 6 个读点（`shared_crate_registry` 读 `gate.sh` 那条最有名）"),
        "src/doc/": (PART, "同上，现打 9 个读点"),
        "src/shared/": (PART, "同上，现打 5 个读点"),
        ".github/": (PART, "同上，现打 3 个读点（`capability_registry` 读 `ci.yml`）"),
        "tests/e2e/": (PART, "同上，现打 4 个读点"),
        "src/": blind("前端那棵树归 `npm` 与 `generated` 两格；Rust 判据里没有读点"),
    },
)

# ── 〔第四波 S4〕第 26 格 `f3-copy`（秤 F3 两向）退役：它量的零流量复制随浏览 / 复制离开 SFTP 一起删了，
#   判据本体那份台架文件一起删了 ⇒ 登记与云端对照那两处一起摘（`gate.sh` 29 格 → 28 格）。


# ── `13b` 步 1（09-20）：第 27 格 `comm-boundary` ───────────────────────────────
#
# 🔴 **它比 `f3-copy` 更需要一个自己的名字，理由是那一族的人群可能是空集。**
#   那一族（C1–C5 ＋ X1–X6 ＋ 锚 ＋ 元判据，共 15 条）人群为空时，
#   绿的理由是 `0 == 0`。⇒ 模块从 `lib.rs` 摘掉时**十五条连同元判据一起消失**，
#   而 `cargo` 那一格只会合计小 15 —— **「摘掉了」与「全绿」在终端上分不开**。
# ⚠ 同 `f3-copy`：相等断言里那个写死的 `pin=15` 是刻意的第三条腿 ——
#   「真删掉一条判据」会让 `declared` 与 `ran` 一起掉（两侧同源），只有 pin 认得出来。
cell(
    "comm-boundary",
    anchor="run_gate comm-boundary '判过的条数 = 通信层那一族",
    cwd="仓根（内层 cd src/bridge）",
    cmd="cargo test -p monitor --lib comm_boundary_registry::tests::（＋ 三方对拍 ＋ 两条锚点）",
    **{
        "tests/": (PART, "〔现打 09-20〕两种碰法要分开：**跑**的只有 "
                         "`tests/bridge/comm_boundary_registry_tests.rs` 这一份（1400 行、15 条），"
                         "本格还**读它的文本**数 `#[test]` 条数（三方对拍的 `declared` 那一边）；"
                         "而那一族的判据自己会**扫过这棵树的绝大部分**（后缀 `rs`/`ts`/`toml`，"
                         "明写排掉 `tests/evidence/`）去找那枚成员标记。"
                         "⚠ 扫过 ≠ 判过：对没盖标记的文件，扫描的结论只有「它不是成员」"),
        "src/bridge/": (PART, "〔现打 09-20〕**编的是 `-p monitor` 一个包**；"
                              "判据另**读** `src/bridge/src/comm_boundary_registry.rs`（`#[path]` 挂载 ＋ "
                              "那句「登记在册的通信层成员：N 份」的散文，三方相等的一条腿）。"
                              "这棵树的其余每一份被**扫**（找标记），不被判"),
        "src/": (PART, "〔现打 09-20〕前端 TS 被**扫**（`.ts` 也在后缀面里，`§3.2` 那条"
                       "「前端不知道 transport」要靠它）。今天一份成员都没有 ⇒ 扫的结论全是「不是成员」"),
        "src/backend/": (PART, "〔现打 09-20〕被**扫**（找标记）。语料见证里逐字钉着 "
                               "`src/backend/wire.rs` 必须在这一趟扫描面里 —— 那是防「扫描面被改窄」的钉子"),
        "src/shared/": (PART, "〔现打 09-20〕被**扫**，同上"),
        "src/doc/": (PART, "〔现打 09-20〕被**扫**（`.md` 不在后缀面里 ⇒ 实际命中为 0，"
                           "但它在根之下、不在排除名单里，如实记成「扫过」而不是「不碰」）"),
        "src/generated/": (PART, "〔现打 09-20〕被**扫**（生成物同样可能被人盖标记，"
                                 "不给它开口子）"),
        ROOTFILES: (PART, "〔现打 09-20〕`src/bridge/Cargo.toml` 那一份是语料见证之一"
                          "（每个后缀各一个逐字住址，`toml` 那个落在这里）。⚠ 真正的仓根文件"
                          "（`package.json` 等）不在 `src`/`tests` 两个根之下 ⇒ 本格不碰"),
    },
)

# ── TQ1（09-24）：第 29 格 `test-tiers` ───────────────────────────────────────
#
# 同 `comm-boundary` 那一形：那一族（测试层分级，12 条）挂在 `guard-core` 的 lib 上，
# `mod test_tiers;` 那一行被摘掉时十二条一起消失，而 `cargo` 那一格只会合计小一点。
# ⚠ 它**扫**的面很宽（为了圈人群），**判**的面窄 —— 下面逐棵把「扫过」与「判过」分开写。
cell(
    "test-tiers",
    anchor="run_gate test-tiers '判过的条数 = 测试层分级那一族",
    cwd="仓根（内层 cd src/bridge）",
    cmd="cargo test -p guard-core --lib test_tiers::（＋ 三方对拍 ＋ 两条锚点）",
    **{
        "tests/": (PART, "〔现打 09-24〕**判**的是人群：`tests/bridge|backend/**/*.rs` 与 `tests/**/*.{vitest,test}.ts` "
                         "逐份归层（分区两向相等）、每份够不够得着它的跑者、扫描层的路径字面量在不在盘、"
                         "集成层有没有静默跳过、每条 `#[ignore]` 的触发链；另读 `tests/benches/` 的份数对 `[[bench]]`。"
                         "⚠ 判的是**这些文件在不在执行链上**，一条判据的断言对不对它一个字不看；"
                         "本格还**读**判据本体那份文本数 `#[test]`（三方对拍的一条腿）"),
        "tests/e2e/": (PART, "〔现打 09-24〕全部 shell 逐份归「套件 / 辅助件」（套件 == `package.json` 在跑的那批，两向），"
                             "辅助件要有人引用（剥注释后按文件名找）。⚠ 套件里的任何一行断言本格都不看"),
        "tests/evidence/": (PART, "〔现打 09-24〕只**读**两样：真机层一条触发者住这里（`SR1a-link-loopback.py` 里那条测试名）· "
                                  "引用语料扫过这棵树的 `.rs`/`.ts`（找辅助件的名字）。这棵树的量具与读数本格一份不判"),
        "tests/scripts/": (PART, "〔现打 09-24〕只被**扫**：引用语料（按文件名找 e2e 辅助件的用户）。不判"),
        "tests/hooks/": (PART, "〔现打 09-24〕只被**扫**：同上"),
        "src/bridge/": (PART, "〔现打 09-24〕**编**的是 `-p guard-core` 一个包；另**读**这棵树全部 `.rs` 找 `#[path]` 挂载"
                              "（单元层可达那一条的人群）与 `Cargo.toml` 的 `[[bench]]`。那些 `.rs` 的内容本格不判"),
        "src/backend/": (PART, "〔现打 09-24〕**读**全部 `.rs` 找 `#[path]` 挂载、读 `Cargo.toml` 的 `[[bench]]`（秤 7 标没标 `test = true`）。"
                               "⚠ 冒烟档那一趟**不在本格**：它在 `backend` 那一格的 `cargo test` 里跑"),
        VENDOR: (PART, "〔现打 09-24〕只被**扫**（找 `#[path]` 挂载；vendor 的 `Cargo.toml` 明写不看）。不判"),
        ROOTFILES: (PART, "〔现打 09-24〕**判** `package.json`（`npm test` 那条 `&&` 链够不够得着每份 `.test.ts`；"
                          "e2e 套件从它派生）与 `vitest.config.ts`（include 那一整行钉住）。其余仓根文件不碰"),
        ".github/": (PART, "〔现打 09-24〕只被**扫**：引用语料里有 `workflows/*.yml`（找 e2e 辅助件的名字）。不判"),
    },
)

cell(
    "generated",
    anchor="git diff --quiet --exit-code -- src/generated/",
    cwd="仓根",
    cmd="git diff --quiet --exit-code -- src/generated/",
    **{
        "src/generated/": ("全", "〔现打 09-19〕本格的命令逐字是 `git diff --quiet --exit-code -- src/generated/` ⇒ 分母**就是这棵树**，82 份一份不漏"),
        "src/": (PART, "**只有 `src/generated/`**，而且只判**已跟踪文件的 diff** ——"
                       "全新的生成物是 untracked，本格看不见（那一格归 `generated-boundary-guard`）"),
    },
)

cell(
    "backend",
    anchor="run_gate backend '单包 src/backend",
    cwd="remote-daemon-proto/",
    cmd="cargo test",
    **{
        "tests/": ("部", "〔现打 09-19〕同上，`tests/backend/` 那 75 份 `.rs` 由 `src/backend/*.rs` 的 `#[path]` 挂进来 ⇒ 本格编它们、跑它们"),
        "src/backend/": (FULL, "单包全量 `cargo test`（含 `#[cfg(test)]` 那一族守卫）"),
        "src/bridge/": (PART, "跨轨对拍：现打 4 个读点（`control/gate.rs` 读 Gate 2 黄金夹具、"
                             "`control/launch.rs` 读 monitor 的 `tmux.rs`）"),
        ".github/": (PART, "现打 1 个读点"),
        "tests/e2e/": (PART, "现打 2 个读点"),
        "src/doc/": (PART, "现打 5 个读点（协议文档对拍）"),
    },
)

# ── 〔TAIL 09-26〕第 32 格 `panorama-engine`：全景小程序是独立 crate（自己一份 lock），别的格编不到它 ──
cell(
    "panorama-engine",
    anchor="run_gate panorama-engine '单包 src/panorama-engine",
    cwd="src/panorama-engine/",
    cmd="cargo test",
    **{
        "src/": (PART, "只有 `src/panorama-engine/`（单包全量 `cargo test`）＋ `cli_tests` 跨树读 `src/panorama/types.ts` 一处"),
        "tests/": (PART, "只有 `tests/panorama-engine/cli_tests.rs`（由 `main.rs` 的 `#[path]` 挂进来）"),
        VENDOR: (PART, "编 `code-picture-core`（path 依赖）但不跑它的测试（那归 `ci.yml` 的 `-p code-picture-core`）"),
        "src/bridge/": (PART, "编 `guard-core`（dev 依赖）但不跑它的测试"),
    },
)

# ── `K-R118`（09-14）：第 16 格 `tsc` ─────────────────────────────────────────
# `KR118D1` ②：**「这棵树编不编得出发版产物」这一维此前门禁一格都没有**。
# `npm` 那一格跑的是 `npm test`（`tsx` / `vitest` 都是转译执行，`esbuild` 只剥类型），
# 云端那条 `npx tsc --noEmit` 只在 `main`/tag/PR 上跑 —— 两条路同时断，
# 于是一条 09-12 引入的 `TS2322` 在 15 格全绿之下活了两天。
# ⚠ 本格**不改变** `src/` 这棵树的覆盖档（`npm` 那一格已经是 `全`）——
#   它加的是**另一维**：`npm` 买行为，本格买类型。两格都在，档位不叠加。
cell(
    "tsc",
    anchor="run_gate tsc '不是「几条断言过了」",
    cwd="仓根",
    cmd="node_modules/.bin/tsc --noEmit --listFiles（＋ 程序面份数对账）",
    **{
        "src/generated/": ("全", "〔现打 09-19〕`include` 的第一项是 `src` ⇒ 这 82 份 `ts_rs` 生成物全部过 `tsc --noEmit`，且它们在 `/src/` 下 ⇒ **也在本格那条恒等对账的两侧**"),
        "tests/": ("部", "〔现打 09-19〕`tsconfig.json` 的 `include` 是 `['src', 'tests']` ⇒ 这棵树的 155 份 `.ts` **确实被 tsc 读进程序、真判了类型**。⚠ 〔订正 09-19〕上一版这条裁词写的是「本格那条恒等对账盖不到它们」——**当时是真的**：`want` 数 `find src tests/e2e`、`got` 的正则是 `/(src|e2e)/`，两侧同时把 `tests/` 的其余 162 份剔掉，等式照样成立。死值验坐实过：把 `include` 收窄成 `['src', 'tests/e2e']`，**旧公式 210 == 210 全绿**，而那 162 份当场不再被检。⇒ 本拍把两侧都改成按 `include` 的真值数（**372 == 372**），同一刀下新公式 372 != 210 **红**。所以这棵树的 `.ts` 今天**既被判了类型、也进了那条恒等对账**"),
        "src/": (FULL, "`tsconfig.json` 的 `include` 第一项就是这棵树 ⇒ 下面每一份 "
                       "`.ts`/`.tsx`/`.mts` 都进程序，**而且本格自己现打对账**"
                       "（真读进程序的份数 == 盘上现打的份数，两个数同一趟算，一个都不写死）。"
                       "⚠ 买的是**类型**这一层：`tsc --noEmit` 不跑一行代码 ⇒ "
                       "「类型对而行为错」本格一个字都问不出来（那一维归 `npm` 那一格）"),
        "tests/e2e/": (PART, "`include` 的第二项，但这棵树下绝大多数是 `.sh` —— "
                       "现打只有 `.ts`/`.mts` 那几份进程序，shell 套件本格一行都读不到"),
        ROOTFILES: blind("`tsconfig.json` 是本格的**配置**（它决定程序面），不是被检对象；"
                         "而仓根那几份 `.ts`（`vite.config.ts` / `vitest.config.ts`）"
                         "**不在 `include` 里** ⇒ 一行都没进程序。这是本格明写的盲区，不是漏登"),
    },
)

cell(
    "npm",
    anchor="run_gate npm '17 个套件",
    cwd="仓根",
    cmd="npm test（16 个 tsx 套件 + vitest run）",
    **{
        "src/generated/": ("部", "〔现打 09-19〕生成物被套件 import ⇒ 会被编进去，但它们自己不是套件"),
        "tests/": ("部", "〔现打 09-19〕`vitest.config.ts` 的 `include: ['tests/**/*.vitest.ts']` ⇒ 现打 **134** 份 `.vitest.ts` 全在本格的分母里。⚠ 229 份 `.rs` 与 `test-support/` 那几份不是套件，本格不跑"),
        "src/": (FULL, "16 个 tsx 套件全在 `src/` 下 + `vitest.config.ts` 的 "
                       "`include: [\"src/**/*.vitest.ts\"]`。⚠ **「跑了 0 个也照绿」那一格 15/17 守不住**"
                       "（本格分母那句话逐字写着）"),
        "src/bridge/": (PART, "现打 2 个读点"),
        "src/backend/": (PART, "现打 1 个读点"),
        "tests/scripts/": (PART, "现打 2 个读点"),
        "tests/e2e/": (PART, "现打 1 个读点"),
        "src/shared/": (PART, "现打 1 个读点"),
    },
)

E2E_NOTE = ("七套后端二进制 e2e 之一（`ccm` 四套 ＋ 第二波 T4 接进来的令牌两套 ＋ 〔TAIL〕`backend-cc-bus`）。`e2e/` 下的套件今天远不止四套 —— "
            "`ccm-acceptance` / `ccm-pretrust` / `cc-spawn-uplift` 等**都不在这道门里**"
            "（那笔账逐字记在本文件头注引的 `gate.sh` 那一段：一次真行为变更的 71 条红里"
            "「这道门看得见 9 条、看不见 62 条」）")
for suite, anchor in [
    ("ccm tests/e2e/ccm-print-parity", "run_e2e ccm-print-parity 12"),
    ("ccm tests/e2e/ccm-rbind-title", "run_e2e ccm-rbind-title  8"),
    ("ccm tests/e2e/ccm-cli", "run_e2e ccm-cli               58"),
    ("ccm tests/e2e/ccm-contract-parity", "run_e2e ccm-contract-parity   45"),
    # 〔第二波 T4 09-24〕令牌那两套（`设计/80 §8.7` 步 2 / 步 3）—— 此前只被 shellcheck、不被执行。
    #   被测对象同是那个后端二进制（`ccm` 即 `cc-monitor-backend`），读法与上面四格一字不差。
    ("ccm tests/e2e/backend-rbind-token", "run_e2e backend-rbind-token   11"),
    ("ccm tests/e2e/rbind-token-endtoend", "run_e2e rbind-token-endtoend   9"),
    # 〔TAIL 09-26〕后端的 cc-bus 基础命令（真跑 cc-bus 脚本 ＋ 隔离 tmux socket）—— 此前只挂在不通电的 `ci.yml` 上。
    ("ccm tests/e2e/backend-cc-bus", "run_e2e backend-cc-bus        96"),
    # 〔E2 尾 09-27〕同样只挂在不通电的 `ci.yml` 上、各红了几天没人看见的那四套（gate2 · 本机后端监护 · 换号两套）。
    ("ccm tests/e2e/backend-gate2", "run_e2e backend-gate2         35 exact-with-skip"),
    ("ccm tests/e2e/local-backend", "run_e2e local-backend         15"),
    ("ccm tests/e2e/restart-frames", "run_e2e restart-frames         5"),
    ("ccm tests/e2e/restart", "run_e2e restart               24"),
]:
    cell(
        suite,
        anchor=anchor,
        cwd="仓根",
        cmd="bash tests/e2e/assert-pass-floor.sh <套件> <地板> exact",
        **{
            "tests/e2e/": (PART, E2E_NOTE),
            "src/backend/": (PART, "**被测对象是它编出来的二进制** "
                                           "`$CARGO_TARGET_DIR/debug/cc-monitor-remote`"
                                           "（`K-R48` 第二拍起）——买的是行为，不是它的源码"),
        },
    )

# 🔴 〔墓碑 09-19〕**`pb check` 那一格的登记整块删掉。**
#   那一格 09-18 已按用户拍板**从 `gate.sh` 整格删除**（`PB_WS` / `planned-build` 在
#   `gate.sh` 非注释处现打零命中）。本文件的 `C1` 本该当场红「登记里有格盘上没有」——
#   **它没红，因为没人跑它**（不在执行链上 ＋ 默认住址还指着重构前的 `scripts/gate.sh`）。
#   ⇒ 那一格从裁决行上消失用了一天，而**本文件就是那个该响没响的东西**。
#   ⚠ 这条墓碑**不是判据**：真正拦「它偷偷回来」的是 `C1` 的两向对拍 ——
#     `gate.sh` 里再出现这一格而登记没跟，`got - want` 当场点名。


# ── 第 25 格 `worktree-clean`（09-19）：门禁的**前置条件** ────────────────────
# 它买的是「仓里没有第二份工作副本」。本仓一族判据的人群是「走文件系统」，
# 仓内一出现 worktree / 变异副本，它们的人群就静默膨胀 —— 恒等那几条红得响，
# **而用地板的那几条一声不吭地过去**。本格让这个条件先出声。
cell(
    "worktree-clean",
    anchor="run_gate worktree-clean '判过的条数",
    cwd="仓根",
    cmd="python3 tests/evidence/K-W25-worktree-clean.py",
    **{
        # 🔴 它**走遍整棵树**（那正是它的活），但它**不读任何文件的内容** ——
        #    只数「这个扩展名有几份」。⇒ 对每一棵树都是「部」，不是「全」。
        "src/backend/": (PART, "〔现打 09-19〕只数 `.rs` 的份数，一行内容都不读"),
        VENDOR: (PART, "同上。⚠ vendor 里的 `.rs` **算进 git 那一侧**（它们被跟踪）⇒ 两侧同口径"),
        "src/bridge/": (PART, "同上"),
        "src/shared/": (PART, "只数 `.sh`/`.ts` 的份数"),
        "src/": (PART, "只数 `.ts` 的份数"),
        "src/generated/": (PART, "同上 —— 生成物也在 git 里，两侧同口径"),
        "tests/e2e/": (PART, "只数 `.sh` 的份数"),
        "tests/evidence/": (PART, "只数 `.sh`/`.ts`/`.rs` 的份数（这棵树三种都有）"),
        "tests/scripts/": (PART, "只数 `.sh` 的份数"),
        "tests/hooks/": (PART, "同上"),
        "tests/": (PART, "只数 `.rs`/`.ts` 的份数"),
        ROOTFILES: (NONE, "抽样的四个扩展名在仓根一份都没有（现打：仓根 14 份全是 "
                          "`.md`/`.json`/`.js`/`.html`/`.ts` 里的配置那几份，"
                          "而 `.ts` 那两份 —— `vite.config.ts`/`vitest.config.ts` —— "
                          "**两侧都数得到**，不构成差额）"),
    },
)


# ── 第 23、24 格 `muslbuild` / `platform`（`G4`，09-19）──────────────────────
# `设计/01 §7.3` 要的「每一个我们发布的平台都编得过」＋「两个壳都要编得过」。
# 行数由条 63 定：承诺三格（本机 Windows x86_64 · 远端 Linux · 本机 Linux），
# (Windows, aarch64) 显式拒绝。
cell(
    "muslbuild",
    anchor="run_gate muslbuild '不是数出来的数",
    cwd="src/backend",
    cmd="cargo zigbuild --target {x86_64,aarch64}-unknown-linux-musl",
    **{
        "src/backend/": (FULL, "〔现打 09-19〕本格把这个 crate 在**两个 musl arch** 上各编一趟 "
                               "⇒ 整棵生产树都过编译器。⚠ 只编 bin ＋ lib，**不带 `--all-targets`** "
                               "⇒ test 档那一半在 musl 上没人编（那是另一回事：测试要跑，不是要编）"),
        VENDOR: (NONE, "本 crate 不依赖 vendor 那两棵（`code-picture-core` 被 `cargo` 那格显式 "
                       "`--exclude`，`cc-acct-iso` 是 bridge 侧的）"),
        "src/bridge/": (NONE, "🔴 **本格只编后端那一个 crate** —— 前端那棵树在 musl 上"
                              "**没有任何门禁**，而它也不需要：远端只装后端字节，前端不过去"),
    },
)

cell(
    "platform",
    anchor="run_gate platform '判过的条数",
    cwd="仓根",
    cmd="python3 tests/evidence/K-G4-platform-ledger.py",
    **{
        "tests/scripts/": (PART, "`P1`/`P2` 读 `gate.sh` **这一份**的文本（格名 ＋ 逐字锚点）；"
                                 "这棵树里另外 6 份 `.sh` 本格一个字不看"),
        # 〔S5 · 第四波 · V105 清账〕这里原来判「部」：`P4`（「壳-折」那一维有对象）读这棵树的三样现物。
        #   那一档放弃、`P4` 删了之后本格一个字都不读这棵树 ⇒ 缺省的「无」（`all_blind`）就是实情。
        "tests/evidence/": (NONE, "🔴 判据本体住这棵树，但本格不读这棵树的任何文件 —— "
                                  "**判据自己住哪不算覆盖**（同 `gate-selfdesc` 那一条）"),
    },
)


# ── 第 22 格 `gate-selfdesc`（09-19）：**被测对象就是 `gate.sh` 自己** ────────────
# 🔴 本文件自己成了门禁的一格。立它的起因逐字记在 `gate.sh` 那一格的头注里：
#   `pb check` 09-18 整格删除，而裁决行点了它一整天 —— 而**本文件的 `C5` 正是为这件事写的**，
#   它没红只因为**没人跑它**（＋默认住址指着重构前的 `scripts/gate.sh`）。
# ⚠ **登记它自己不是循环论证**：`C1` 拿 `gate.sh` 现打的格名与本登记两向对拍 ——
#   本格从 `gate.sh` 里消失而这条登记还在，`want - got` 当场点名。
cell(
    "gate-selfdesc",
    anchor="run_gate gate-selfdesc '判过的条数",
    cwd="仓根",
    cmd="python3 tests/evidence/K-R80-gate-cell-coverage.py",
    **{
        "tests/scripts/": (PART, "本格**只读 `gate.sh` 这一份**（这棵树现打 7 份），"
                                 "而且只读它的**文本**：格名 · 逐字锚点 · 裁决行的数 · 头注自述节。"
                                 "这棵树里另外 6 份 `.sh` 本格一个字不看"),
        "tests/e2e/": (PART, "`C5b` 要验「头注点名的套件盘上真有那份文件」⇒ 本格查"
                             "这棵树里四套套件的**文件在不在**，**不看内容、不跑它们**"),
        "tests/evidence/": (NONE, "🔴 **判据本体住这棵树，但本格不读这棵树的任何文件** ——"
                                  "它读的是 `tests/scripts/gate.sh`。**判据自己住哪不算覆盖**，"
                                  "把这两件事混起来，每把尺子都会「盖住」它自己所在的树"),
        ".cargo/": (NONE, "🔴 本格确实会打开 `.cargo/config.toml`（`C6b` 的钉子：验那条"
                          "「不需要门」的理由还站不站得住），**但那不算覆盖**：钉子验的是"
                          "**一句说明**，不是这棵树的内容。算成覆盖，等于让一条「不需要门」的"
                          "说明**自己把自己变成有门** —— `C6b` 会立刻翻脸说「这条说明陈了」"),
        ROOTFILES: (NONE, "同上：`C6b` 的钉子③ 读 `package.json` 的 `scripts.test` 验一句话，"
                          "不判这棵树的内容。本树的 `部` 来自 `release-gate`，不来自本格"),
    },
)


# ── `K-R82` `KR82D3`：0 格覆盖的那几棵树，**明写「不需要门」** ────────────────
#
# 🔴 **「不需要」与「没查」在输出上一模一样** —— 上面那张转置表印出 `0` 的时候，
#   它一个字都没说这个 `0` 是哪一种。`DECISIONS.md#R42` 裁定四拍的就是这件事：
#   `hooks/` 立件（今天成了第 13 格），另两棵**写进登记，不留成「没查」**。
#
# 🔴 **而「明写」如果只是一句注释，它会腐** —— 本区已有 46 次那样的先例
#   （`R42` 裁定零：PM 把「门禁九格」当读数用了 46 次）。⇒ 下面每一条「不需要」
#   都拴着一个**盘上查得到的钉子**，`C6b` 逐条验：钉子没了 / 陈了，当场红。
#
# ⚠ 理由**逐条给，不许写「不重要」** —— `C6b` 里有一条黑名单专门挡那种写法。
#
# ── `K-R91`（09-12）：每一条底下多了一份 `archive`，那是 `C6c` 的分档表 ────────────
#
# 🔴 **上一版这里栽的是「一句全称句罩着一群没数过的文件」**（`DECISIONS.md#R46` 裁定三，
#   PM 现打于 `cba446b`）：仓根被跟踪文件 **16** 份，而 `ROOTFILES` 的 `why` 里逐字点名 6 份、
#   被 `README*.md` 通配罩 2 份、散文里指得出 1 份 ⇒ **7 份只被「其余的……是文档与仓库元数据」
#   那一句罩着**，而那句话**对其中至少 5 份是假的**（`index.html` 是应用入口 ·
#   `vite.config.ts` 是构建配置 · `tsconfig.json` / `eslint.config.js` / `.stylelintrc.json` 都是配置）。
# ⚠ **`C6b` 逮不到，而那不是它的 bug** —— 它 docstring 逐字写着边界：买的是「这句话还站得住」，
#   不买「这句话对不对」。它查理由在不在 / 敷不敷衍 / 钉子指不指得到，
#   **不查「理由的枚举盖没盖全那棵树」**。⇒ `C6c` 补的正是这一维。
#
# 🔴 **判词是两个，不是一个**：`不需要门` 与 `未裁` 在上一版的输出里长得一模一样
#   （都印成「0 覆盖 + 一句理由」）。`K-R91 §0b` 逐字禁本件给那几份加门 ⇒ 那几份的诚实判词
#   就是**未裁**：**「已被逐份数到」不等于「不需要门」**，这张表不许把后者写成前者。
NEED_NONE, UNJUDGED = "不需要门", "未裁"

# 🔴 〔`K-R115` 09-14〕**`evidence/` 那一条整段删了，而这是一次「登记的字面不再成立」，
#   不是一次整理**：本件给门禁加了第 14 格 `copy2`（判据本体 `evidence/K-R115-ruler.py`），
#   它是**第一格盖到这棵树**的门 ⇒ 「0 格覆盖」这个前提当场不成立，`C6b` 会说「这条说明陈了」。
#   上一版那段话逐字写着「`0 格覆盖`**正是它的用途**」——`[J3 陈账]` 死锁的泄压口。
#   ⚠ **泄压口在实质上还在**（本格只在两种情况下红：某处保元数据复制的目的地落在被 git
#   跟踪的树内内容上 · 某份 `.py` 连 `ast` 都解析不了；改 `.md` / 加读数一格都不动），
#   **但那句话的字面不成立了** ⇒ 删它，不许改成一句还罩得住的话。
#   🔴 这一改**推翻了 `K-R80` 记下的一条理由**，不是机械随动 —— 实现方不自批，交回 PM 裁。
NO_GATE_NEEDED = {
    # 🔴 〔新增 09-19〕重构把两个 Cargo 工程搬进 `src/` 之后才出现的一棵树。
    ".cargo/": {
        "why": "这棵树只有一份 `config.toml`，内容是 `[build] target-dir = \".build/bridge\"` ——"
               "它把构建产物**赶出 `src/`**。⇒ 它是本仓那一族「走一遍源码树收所有 `.rs`」判据的"
               "**前置条件**，不是任何一格的**分母**；登记只记分母，所以这里判 0 不是漏登。"
               "🔴 **但它坏掉的后果比一般配置重，写清楚**：`target/` 下有成千上万份 build script "
               "生成的 `.rs`，一旦落回 `src/` 里，那一族判据的人群会被生成代码淹没 —— "
               "**不是报错，是安静地扫错东西**（本仓自己的说法：「恒绿看起来和真绿一模一样」）。"
               "⇒ 判词写死是 `未裁`：该不该给它单立一道门是另一次裁定，本条只买「已被数到」。"
               "⚠ 与 `tests/hooks/` 的区别仍在那一句：那棵树里的东西**会被执行**。",
        # 钉子：那份文件得真在、且那条键真是这个值 —— 有人把 target-dir 改回默认，理由当场失效。
        "witness_file": ".cargo/config.toml",
        "witness_text": 'target-dir = ".build/bridge"',
        "archive": [
            {"档": "构建 / 工具链配置", "判": UNJUDGED,
             "成员": ["config.toml"],
             "why": "它决定构建产物落在哪；改坏它不改变产品行为，但会让一族判据的人群失真。"
                    "该不该给它加门未裁。"},
        ],
    },
    ROOTFILES: {
        # 🔴 〔加标 09-19〕`release-gate` 那一格读 `package.json` 的 `version` ⇒ 本树 **1 格覆盖**，
        #   不再是孤儿。但 14 份里进那一格的只有 1 份 ⇒ 逐份分档照做（见上面 `partial` 那段）。
        "partial": True,
        # 🔴 `K-R91`（09-12）**这一段整段重写过**，上一版逐字是：
        #   「② 其余的（`README*.md` / `CHANGELOG.md` / `LICENSE` / `PHASE-G-REPORT.md` /
        #     `.gitattributes` / 那份审阅报告）是**文档与仓库元数据**，改坏它们不改变任何产品行为」
        #   —— 那是一句**罩住整棵树**的全称句，而它对 `index.html` / `vite.config.ts` /
        #   `tsconfig.json` / `eslint.config.js` / `.stylelintrc.json` **是假的**。
        #   ⚠ 上一版那句话本身**没删**，它下移进了「文档」那一档 —— 那一档里它是真的。
        "why": "这些仓根文件（`git ls-files` 里不含 `/` 的那些）**逐份归到下面 `archive` 那几档**，"
               "**分母是现打的**、不是这里写死的一张清单（`C6c` 每趟重数）。"
               "🔴 **每一档带自己的判词，`不需要门` 与 `未裁` 分开写**："
               "① `npm` 那一格的命令本体住在 `package.json` 的 `scripts.test` 里、"
               "`vitest.config.ts` 给它 `include` —— 这两份坏了那一格根本起不来 ⇒ 它们是那格的"
               "**依赖**，不是那格的**分母**；登记只记分母，所以这两份在本树上判 0，不是漏登。"
               "⚠ 〔订正 09-19〕上一句原先写的是「**这里仍判 0**」——那是说**整棵树** 0 格覆盖，"
               "今天不成立了：`release-gate` 读 `package.json` 的 `version`，本树现打 **1 格**。"
               "改的是那句话的射程（从整棵树收到这两份），**不是把普查关掉**。"
               "② 文档与仓库元数据那两档，改坏它们不改变任何产品行为，**不值一道出货闸**。"
               "③ **应用入口与构建 / 工具链配置那两档今天 0 格覆盖，而它们既不是文档也不是仓库元数据** ——"
               "判词写死是 `未裁`：该不该给它们加门是另一次裁定（`K-R91` `§0b` 逐字禁本件加门），"
               "本条只买「**已被逐份数到**」。"
               "⚠ 与 `hooks/` 的区别仍在那一句：那棵树里的东西**会被执行**。",
        # 钉子①：说「npm 那格靠它起来」，那格就得真在登记里（改名/删格 ⇒ 红）。
        "witness_cell": "npm",
        # 钉子②：那两份仓根文件得真在盘上的仓根文件集合里（挪走 ⇒ 红）。
        "witness_rootfiles": ["package.json", "vitest.config.ts"],
        # 钉子③：`npm test` 那条命令得真住在 `package.json` 的 `scripts.test` 里
        #        —— 它哪天搬走了，上面「① 是依赖不是分母」这句话就不成立了 ⇒ 红。
        "witness_json_key": ("package.json", ["scripts", "test"]),
        # `C6c`：分母 = `git ls-files` 里不含 `/` 的那些，**每趟现打**。
        # ⚠ 通配串刻意写得**指得住**：`C6c` 有一张全域通配黑名单（`*` / `**` / `*.*`），
        #   拿一条 `*` 把整棵树罩住 = 换个写法的全称句，那正是本条要治的那一形。
        "archive": [
            {"档": "`npm` 那一格的依赖（不是它的分母）", "判": NEED_NONE,
             "成员": ["package.json", "vitest.config.ts"],
             "why": "那一格的命令本体住 `scripts.test`、`include` 住 `vitest.config.ts`；"
                    "它们坏了那一格根本起不来 ⇒ 是**依赖**不是**分母**，登记只记分母"},
            {"档": "应用入口与构建 / 工具链配置", "判": UNJUDGED,
             "成员": ["index.html", "settings.html", "viewer.html", "vite.config.ts", "tsconfig.json",
                     "eslint.config.js", ".stylelintrc.json"],
             "why": "🔴 **这一档今天 0 格覆盖，而「文档与仓库元数据」那顶帽子对它们是假的**："
                    "`index.html` 是应用入口 · `vite.config.ts` 决定构建产物 · "
                    "`tsconfig.json` / `eslint.config.js` / `.stylelintrc.json` 决定类型与 lint 的口径。"
                    "⇒ 本条**不声称它们不需要门**，只声称**它们已被逐份数到**。"
                    "〔U1 09-24〕三入口拆分后多了 `settings.html` / `viewer.html`；三份 html 与 vite input、"
                    "`lib.rs` 开窗 url 的三处对账住 `tests/entry-graphs.vitest.ts`（经 `npm` 那一格跑到），"
                    "但那是**依赖面的对账**，不是这一档的分母 ⇒ 判档不改"},
            {"档": "依赖锁", "判": UNJUDGED, "成员": ["package-lock.json"],
             "why": "它决定 `npm ci` 装出来的是哪一棵依赖树 —— 同样既不是文档也不是仓库元数据。"
                    "0 格覆盖，加不加门另裁"},
            {"档": "仓库元数据", "判": NEED_NONE, "成员": [".gitignore", ".gitattributes"],
             "why": "它们只对 git 自己说话（哪些文件不跟踪 / 换行与 diff 怎么处理），"
                    "不进构建、不进运行时、不被产品代码读"},
            {"档": "文档", "判": NEED_NONE,
             # 🔴 〔订正 09-19〕摘掉 `PHASE-G-REPORT.md` 与 `项目审阅报告-*.md` ——
             #   两份**今天一份都匹配不到**（`C6c` 每趟现数，所以它红得出来）。
             #   ⚠ 摘的是**档标**不是判词：剩下四份仍归「文档」，那句「改坏它们不改变
             #   任何产品行为」对它们一个字没变。
             "成员": ["README*.md", "CHANGELOG.md", "LICENSE"],
             "why": "改坏它们不改变任何产品行为 —— **这一句对这一档是真的**；"
                    "上一版把同一句话当成罩住整棵树的全称句用，那正是 `K-R91` 立案的那一形"},
        ],
    },
}

# 挡「写了等于没写」的那几种写法。⚠ 它只挡**已知的**那几个词，不声称能识别所有敷衍
# —— 给得出分母的才写数：这里的分母就是下面这个列表本身。
VAGUE = ["不重要", "无所谓", "没必要", "TODO", "待定", "暂时", "以后再说", "略"]
MIN_WHY = 60   # 字符。60 是「一句能读的理由」的量级，不是精确阈值。

# ── `C6c`（`K-R91` 09-12）的三张小表 ────────────────────────────────────────
# ⚠ 与 `VAGUE` 同一条诚实边界：它们只挡**已知的**那几种写法，不声称能识别所有全称句。
#   给得出分母的数才写 —— 这里的分母就是这三张表本身。
SWEEPING = ["其余的", "其余那些", "剩下的都", "诸如此类", "之类的都"]   # 全称句黑名单（挡「一句话罩一片」）
CATCH_ALL = {"*", "**", "*.*", "?*", "*?", "*/*"}                      # 全域通配黑名单（挡「换个写法的全称句」）
MIN_BUCKET_WHY = 24   # 一档的理由比整棵树的短，量级不同，阈值也不同


# ── `w24c`（09-19）：第 24 格 `ccbus-twophase` ────────────────────────────────
# 步 24c：cc-bus 的两阶段读口（`cc-peek` ＋ `cc-commit`）· 三个适配 trait · Windows 那一侧。
# ⚠ 这一格是本门禁里**唯一会真跑 cc-bus 那几条命令**的格（在一次性 `CC_BUS_HOME` 里）。
cell(
    "ccbus-twophase",
    anchor="run_gate ccbus-twophase '判过的条数",
    cwd="仓根",
    cmd="python3 tests/evidence/W24C-ccbus-twophase-ruler.py",
    **{
        "src/shared/": (PART, "只有 `src/shared/cc-bus/` 那一棵：`scripts/` **整目录**被扫两遍"
                        "（`.pos` 的写点 · 锁文件表达式），另有 7 份被逐字读"
                        "（`cc-peek` · `cc-commit` · `cc-bus-adapt.sh` ＋ 两份实现 ＋ "
                        "`cc-bus-agent-claude.sh` · `cc-recv` 的 sha256）＋ `SKILL.md` 那几句。"
                        "〔09-24 kinds/保活〕另读 `examples/kinds.tsv`（随包 kinds 表逐行过装表判、与内置对拍）"
                        "与 `examples/cc-keepalive`（保活调用方，真跑）；scripts/ 整目录再被扫第三遍（保活零提及）。"
                        "⚠ 这棵树的其余部分（`examples/` 里另外三份 · `ccm-aliases.sh` 之类）"
                        "本格一个字都不问；⚠ `cc-spawn`/`cc-kill`/`cc-busd` 只在那几条全目录扫里"
                        "被**数到**，它们自己干得对不对本格问不出来"),
        "tests/evidence/": blind("本格的**尺子**就住这棵树（`evidence/W24C-ccbus-twophase-ruler.py`）"
                                 "—— 尺子不是分母。把量具算进它自己的覆盖，正是本区最高频那族病"
                                 "⇒ 这里刻意判「无」，与 `hooks` / `copy2` / `gate-selfdesc` 三格"
                                 "对自己那棵树的判法一致"),
    },
)

# ── 现打：`git ls-files` ────────────────────────────────────────────────────
# 🔴 **必须走 `-z`。** 第一版用的是裸 `ls-files` + 按换行切，而本仓有中文文件名 ——
#   git 默认会把它们**加引号并转义**（`"doc/\350\256\241…"`），于是顶层目录被切成 `"doc/`
#   与 `"evidence/`，`C4` 当场报「盘上多出两棵树」。**那是尺子的病，不是盘上的事实**
#   （本仓最高频那族：量具的作用域对不上事实）。`-z` 出的是原始字节、NUL 分隔，不转义。
def ls_files():
    out = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z"],
                         capture_output=True, text=True, encoding="utf-8", errors="surrogateescape")
    return [p for p in out.stdout.split("\0") if p]


# ── 现打：跨树读点矩阵（不是判据，是让上面那些「部」里的数可复算）──────────────
def cross_tree_reads():
    """粗尺：**同一行**里既出现文件读调用、又出现别的树的住址 ⇒ 记一个读点。

    ⚠ 它的失效面写清楚，别把它当权威：
      · 跨行写法（住址在上一行的常量里）**数不到**；
      · 注释里提到别的树而并没有读它 ⇒ **数多了**；
      · 只认这几种读法：`read_to_string` / `include_str!` / `readFileSync` /
        `read_dir` / `readdirSync`。
    ⇒ 它给的是**量级**，不是精确数。上面登记里那些「现打 N 个读点」就是这把尺子的读数。
    """
    read_call = re.compile(r"read_to_string|include_str!|readFileSync|read_dir|readdirSync")
    # 🔴 〔订正 09-19〕上一版这里**另写了一份归属表**（`markers` 八条 ＋ 下面那串 if/elif），
    #    与文件头的 `TREES` 是两份账，重构一来两份同时腐、而且互相看不见。
    #    ⚠ 本拍那道「树键位改写」的正则还**误伤过它一次** —— 它的键是「源码行里要找的字串」、
    #      值才是树名，两者同形 ⇒ 正则把键当树名改了一半。**这正是第二份账的典型死法。**
    #    ⇒ 删掉，一律从 `TREES` 派生：归属只有一个住址。
    markers = {t: t for t in TREES if t is not ROOTFILES}
    matrix = {}
    for f in ls_files():
        if not f or f.rsplit(".", 1)[-1] not in ("rs", "ts", "mts", "js", "mjs"):
            continue
        owner = tree_of(f)
        try:
            txt = (ROOT / f).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for line in txt.split("\n"):
            if not read_call.search(line):
                continue
            for marker, tree in markers.items():
                if marker in line and tree != owner:
                    matrix.setdefault(owner, {}).setdefault(tree, 0)
                    matrix[owner][tree] += 1
    return matrix


# ── 现打：`gate.sh` 里到底有几格判定 ─────────────────────────────────────────
# 只认**行首**的调用 —— `gate_selftest` 里那 10 条探针都写成 `probe="$(run_gate 自检… )"`，
# 缩进着、且套在命令替换里，本正则一条都不会认（**这一条是承重的**：把探针当成判定格，
# 格数会凭空多 10 个，`C5` 当场变成一条永远对不上的假判据）。
CELL_PATTERNS = [
    (re.compile(r"^run_gate_sum (\S+) ", re.M), lambda m: m.group(1)),
    (re.compile(r"^run_gate (\S+) ", re.M), lambda m: m.group(1)),
    (re.compile(r"^run_e2e (\S+) ", re.M), lambda m: "ccm tests/e2e/" + m.group(1)),
]
# 这两格不走那三个函数，各自手写判定 ⇒ 单独按**逐字锚点**认。
HANDWRITTEN = {
    "generated": "git diff --quiet --exit-code -- src/generated/",
}
ROLLCALL = re.compile(r"GATE: OK —— (\d+) 格全绿")


def found_cells(text):
    cells = []
    for pat, name_of in CELL_PATTERNS:
        for m in pat.finditer(text):
            cells.append(name_of(m))
    for name, anchor in HANDWRITTEN.items():
        if anchor in text:
            cells.append(name)
    return cells


# ── `C5b`（`K-R91` `KR91D1`）：门禁**自述射程**那几句话，腐了有人说话 ────────────
#
# 🔴 题面（`DECISIONS.md#R46` 裁定三，PM 现打于 `cba446b`）—— `gate.sh` 头注里两处自述已腐：
#   · `:19–20` 逐字「三道门 + 一道生成物漂移检查 + `pb check` + 四套 `ccm` e2e」
#     ⇒ 3+1+1+4 = **9**，而当天盘上是 **13** 格。
#     🔴 **那个算式与 `R42 裁定零` 里被 PM 传播了 46 次的错数是同一个算式、同一份文件。**
#   · `:22–25` 逐字点名 `ccm-acceptance` / `ccm-pretrust` —— 那两个文件 09-11 `K-R48`
#     第二拍就删了（现打 `ls` 双 `No such file`）。
#   `C5` 只对拍**裁决行**那一句（那行的数 vs 现打格数），**钉不到头注**。
#
# 🔴 **失效方向写死：只认「格数」这个数字，就认不出第二处** —— 那句话里**一个数字都没有**，
#   烂的是「点名了两个不存在的文件」。⇒ 本条**两半都判**。
#
# ★ 取法：**自述句只许住在一段带围栏的「自述节」里**，围栏之外的头注不许再有第二份自述。
#   围栏**内**：① 格数三方对拍（自述节的数 · 裁决行的数 · 现打格数）
#              ② 逐格点名与现打格**集合相等**（少点一格 / 多点一格都红）
#              ③ 节里点名的每一条盘上住址**现打存在**。
#   围栏**外**（头注）：④ 带自我指称词又带「N 格 / N 道门」的行 ⇒ 红
#              ⑤ 点名了 `e2e/` 下**没有那份文件**的套件名的段落 ⇒ 红。
#
# ⚠ **豁免只有一种（逐字 `〔量于`），粒度刻意不同** —— 自述与历史读数在散文里长得一模一样：
#   · ④ 按**行**豁免：一句就是一条声称；
#   · ⑤ 按**段落首行**豁免：一段账里点名十次是同一笔账，逐行贴标记会把段落读碎。
# ⚠ **买不到**：一句标着 `〔量于` 而内容其实是今天的自述，骗得过它（那要读语义）；
#   围栏**之内**的散文它也只判上面那三样，不判每一句话对不对。
# ⚠ **射程 = 头注**（文件开头到第一行非注释代码）。正文里的注释不在本条射程里 ——
#   本件的两处腐都在头注，扩到全文要另量一次分母，`K-R91` 不自批。
SELF_OPEN = "〔自述·射程〕本脚本此刻跑哪几格"
SELF_CLOSE = "〔自述·射程〕完"
SELF_COUNT = re.compile(r"〔自述·格数〕\s*(\d+)\s*格")
SELF_ROLL = "〔自述·点名〕"
SELF_REF = re.compile(r"本脚本|本文件|本门|它跑的是")
SELF_NUM = re.compile(r"(?:[0-9]+|[一二两三四五六七八九十]+)\s*(?:格|道门)")
HIST_MARK = "〔量于"
BACKTICK = re.compile(r"`([^`]+)`")
SUITE_TOK = re.compile(r"^(?:ccm|cc)-[a-z0-9][a-z0-9-]*$")
PATHISH = re.compile(r"^[\w\-./@]+$")


def header_lines(text):
    """头注 = 文件开头到第一行非注释代码（`set -uo pipefail`）。返回 1 基行号 + 原文。"""
    out = []
    for i, ln in enumerate(text.split("\n"), 1):
        if ln.strip() and not ln.lstrip().startswith("#"):
            break
        out.append((i, ln))
    return out


def header_paragraphs(hdr):
    """连续的注释行算一段；空行或只有一个 `#` 的行断段。"""
    paras, cur = [], []
    for i, ln in hdr:
        if ln.strip() in ("#", ""):
            if cur:
                paras.append(cur)
                cur = []
            continue
        cur.append((i, ln))
    if cur:
        paras.append(cur)
    return paras


def suite_on_disk(tok):
    # 🔴 〔订正 09-19〕住址原先是 `e2e/{tok}` —— 重构后套件搬进 `tests/e2e/`，
    #    于是**四套都查无此文件**，C5b 一口气吐三条假红（三段头注被判成「点了不存在的套件」）。
    #    ⚠ 这一形值得记：**尺子指错地方时，它的红与真红长得一模一样。**
    return any((ROOT / f"tests/e2e/{tok}{ext}").exists() for ext in (".sh", ".test.sh", ""))


# ── `C5c`（`K-R122` `KR122D3` 的兄弟条，09-14）：**裁决行上那句射程，有人守** ────────
#
# 🔴 题面（`K-R119` 09-14 的读数）：同一棵树本门禁 **16 格全绿**，云端 8 个 job 里 **5 个红**。
#   `KR122D2` 二选一里的**乙**要求「在裁决行上明写射程，逐字说出本门禁不看什么」——
#   而一句没有判据在守的散文，下一轮就是一句**没人守的散文**（`brief` 第 17 条那一族）。
#
# ★ 取法：射程表写成 `键|说明`，**键进机检、说明不进**。
#   ① 表非空（地板：挡「把数组掏空、函数留着」那一形 —— 那时它照印一行「下面这 0 件事」）；
#   ② 每一项形状对（`键|非空说明`，键 `^[a-z0-9-]+$`）、键不重复；
#   ③ **键集合与现打的判定格名互不相交** —— 这一条是它唯一真有牙的地方：
#      哪天有人把某一维收成了格（本件就干了两次：`shellcheck` · `winchk-daemon`），
#      而这里还自称「不看」，当场红。与 `C5b` 治的腐同源：**自述与盘上现状分叉**。
#   ④ 印出来的条数**现算**（源码里得有 `${#GATE_BLIND[@]}`），不许写死一个数 ——
#      写死就是 `R42 裁定零` 那个被传播了 46 次的错数的同一形状。
#   ⑤ 那个打印函数**真的被裁决那一支调到**（定义 ＋ 调用，至少两处命中）。
#
# ⚠ **它买不到什么**：这张表是**黑名单**，`C5c` 只保证「列出来的这几条不会悄悄变成散文」，
#   **不保证射程之外只有这几条**（那个分母没人数得出）。说明那一栏写得对不对，它一个字都不判。
BLIND_DECL = "GATE_BLIND=("
BLIND_COUNT_EXPR = "${#GATE_BLIND[@]}"
BLIND_PRINTER = "gate_print_blind"
BLIND_KEY_RE = re.compile(r"^[a-z0-9-]+$")


def blind_items(text):
    """抠出 `GATE_BLIND=( … )` 里那几条双引号字符串。返回 (条目列表, 诊断列表)。"""
    out, bad = [], []
    n = text.count(BLIND_DECL)
    if n != 1:
        bad.append(f"C5c `gate.sh` 里 `{BLIND_DECL}` 命中 {n} 处（应当恰好 1 处）—— "
                   f"射程表是裁决行那句话的唯一住址，没有它那句话就没人守")
        return out, bad
    body = text.split(BLIND_DECL, 1)[1]
    end = body.find("\n)")
    if end < 0:
        bad.append("C5c `GATE_BLIND=(` 找不到收尾的 `)` —— 数组的形状变了，本条抠不到东西")
        return out, bad
    for ln in body[:end].split("\n"):
        ln = ln.strip()
        if not ln or ln.startswith("#"):
            continue
        if not (ln.startswith('"') and ln.endswith('"')):
            bad.append(f"C5c 射程表里有一项不是一整条双引号字符串：{ln[:60]!r}")
            continue
        out.append(ln[1:-1])
    return out, bad


def check_blind_scope(text, cells):
    items, out = blind_items(text)
    if out:
        return out
    if not items:
        out.append("C5c 射程表是**空的** —— 那时裁决行后面照印一句「下面这 0 件事」，"
                   "而「什么都不漏」正是本条要挡的那句假话")
        return out
    keys = []
    for it in items:
        if "|" not in it:
            out.append(f"C5c 射程表这一项没有 `键|说明` 的形状：{it[:60]!r}")
            continue
        k, why = it.split("|", 1)
        if not BLIND_KEY_RE.match(k):
            out.append(f"C5c 射程表的键 {k!r} 不合形状（只许 `^[a-z0-9-]+$`）—— "
                       f"键要进机检，形状松了就点不准")
        if not why.strip():
            out.append(f"C5c 射程表的 `{k}` 只有键、没有说明 —— 空白格不算「逐字说出来」")
        keys.append(k)
    dup = sorted({k for k in keys if keys.count(k) > 1})
    if dup:
        out.append(f"C5c 射程表里的键重复了：{dup}")
    clash = sorted(set(keys) & set(cells))
    if clash:
        out.append(f"C5c 射程表仍自称**不看** {clash}，而 `gate.sh` 现打**已经有这几格了** —— "
                   f"买到了却还在说买不到，与 `C5b` 治的腐同源。收成格的那一拍要把这一条摘掉")
    if BLIND_COUNT_EXPR not in text:
        out.append(f"C5c 印射程那一行没有现算条数（找不到 `{BLIND_COUNT_EXPR}`）—— "
                   f"写死一个数就是 `R42 裁定零` 那个错数的同一形状")
    hits = text.count(BLIND_PRINTER)
    if hits < 2:
        out.append(f"C5c `{BLIND_PRINTER}` 在 `gate.sh` 里只命中 {hits} 处（定义 ＋ 裁决那一支的调用，"
                   f"至少 2 处）—— 射程表还在，而**没有人印它**：那就退回成一段注释了")
    return out


# ── `C5d`（`G4` 空洞③，09-20）：**裁决行那串点名有人守** ＋ 格数不许有第三份住址 ──
#
# 🔴 题面（本拍现打，**非截断** grep，两条都逐字）：
#   · `gate.sh` 的裁决行逐字 `echo "GATE: OK —— 26 格全绿（worktree-clean · hooks · …）"`
#     ⇒ 那个数是**字面量**。而且它必须是字面量：`C5` 的 `ROLLCALL` 正则就是靠抠这个
#     字面量去跟**现打格数**对拍的 —— 改成「门禁自己算」，那条对拍的两侧当场同源、恒真。
#   · 而同一份文件的射程表里，`did-ci-actually-run` 那一条的说明逐字写着
#     「几格由裁决行现算，这里刻意不写死一个数」⇒ **那句话是假的**。
#   · 它**没人守**：`C5c` 的 docstring 逐字「说明那一栏写得对不对，它一个字都不判」。
#   ⇒ 「一处写死、另一处自称算出来」的两份住址 —— 而**腐掉的正是后面那一句散文**。
#
# 🔴 **更值钱的那一半：裁决行括号里那串点名，此前一条判据都没有。**
#   `C5` 只抠 `(\d+)`；`C5b` 的逐格点名住在**头注的自述节**里，射程就是头注（那条
#   docstring 自己写着「射程 = 头注」）⇒ **钉不到裁决行**。于是那串名字是一份
#   **能自己烂掉的第二住址** —— 而 `pb check` 09-18 烂的就是它（裁决行点了一整天
#   一个已经整格删掉的名字，见上面 `:74` 那条墓碑）。当时靠人发现，不是靠判据。
#
# ★ 取法（两半，各有各的失效方向）：
#   ① **裁决行括号里的点名 ↔ 现打格名，两向集合相等。** 少点一格 / 多点一格都红。
#      e2e 那四格在裁决行上写**套件短名**（`ccm-cli`），归一那一跳与 `found_cells()` 里
#      `run_e2e` 那条**同一份写法**：短名不在格名集合里时补前缀 `ccm tests/e2e/`。
#      🔴 写成**两向相等**不写成「点到的都存在」：后者在那串点名被清空时**恒真**。
#   ② **射程表的说明栏里不许出现逐字 `现算`。** 「这个数怎么来的」只许住两处
#      （头注自述节 · 裁决行），那两处各有判据钉着；而说明栏 `C5c` 逐字声明
#      「一个字都不判」⇒ 在那儿说取法 = 一句**没人守**的散文，而它已经腐过一次。
#
# ⚠ **买不到什么**（逐条写死，别读成「裁决行从此都对」）：
#   · 裁决行那句散文的其余每个字（「可以出货」之类）本条不判；
#   · ② 那一半是**关键词黑名单**：换个说法说同一句假话，它逮不到（那要读语义）；
#   · **刻意没做**「说明栏里不许出现 `N 格` 这一形的数」那条更宽的闸 —— 现打会**假红**：
#     `ci-job-shape` 那条说明里「条 63 承诺的**三格**」是一处**正当**的引用（说的是
#     平台承诺，不是本门禁的格数）。一条会假红的闸会被人关掉，比没有闸更坏。
VERDICT_ROLL = re.compile(r"GATE: OK —— \d+ 格全绿（([^）]*)）")
E2E_PREFIX = "ccm tests/e2e/"
BLIND_BANNED = "现算"


def check_verdict_rollcall(text, cells):
    out = []
    ms = VERDICT_ROLL.findall(text)
    if len(ms) != 1:
        out.append(f"C5d 裁决行上 `GATE: OK —— N 格全绿（…）` 那串点名命中 {len(ms)} 处"
                   f"（应当恰好 1 处）—— 那串名字没了 / 有了第二份，本条按红处理")
        return out
    got = set(cells)
    named = set()
    for tok in ms[0].split("·"):
        tok = tok.strip().strip("`").strip()
        if not tok:
            continue
        # 归一：e2e 四格在裁决行上写短名，规范名带前缀（与 `found_cells()` 同一份写法）
        named.add(tok if tok in got else E2E_PREFIX + tok)
    if named != got:
        out.append(f"C5d 裁决行那串点名与现打的格**对不上**："
                   f"点了而盘上没有 {sorted(named - got)} · "
                   f"盘上有而没点 {sorted(got - named)} —— "
                   f"`C5` 只抠那一行的数、`C5b` 只管头注的自述节，"
                   f"这串名字此前没有任何判据（`pb check` 09-18 就是从这儿烂的）")
    items, bad = blind_items(text)
    if bad:
        return out + bad
    for it in items:
        if "|" not in it:
            continue
        k, why = it.split("|", 1)
        if BLIND_BANNED in why:
            out.append(f"C5d 射程表 `{k}` 的说明里出现了逐字 `{BLIND_BANNED}` —— "
                       f"「这个数怎么来的」只许住头注自述节与裁决行那两处（各有判据钉着）；"
                       f"说明栏 `C5c` 逐字声明「一个字都不判」⇒ 在这儿说取法就是"
                       f"**一句没人守的散文**，而它 09-20 之前正逐字躺着一句假话"
                       f"（自称裁决行那个数是算出来的，而它是字面量）")
    return out



# ── `C8`（`G4` 空洞③，09-20）：**这道门到底被谁调用** ────────────────────────────
#
# 🔴 题面（`真相源/92 §2.1.2` ①，本拍非截断复打）：
#   `grep -c 'bash tests/scripts/gate.sh' .github/workflows/*.yml` ⇒ 落地前**两份都是 0**；
#   `.git/hooks/` 下零个非 sample 钩子；`tests/hooks/` 下只有一份 `pre-commit`，而它
#   **默认是死的**（要人手 `git config core.hooksPath` 才活）。
#   ⇒ **此前没有任何东西强制这道门在出货前跑过。**「跑了」与「没跑」在终端上一模一样。
#
# ★ 取法：**一张登记，两向对拍，三个消费者。**
#   登记（下面 `INVOCATION`）是「每一格在云端有没有人跑、没有的话为什么」的**唯一住址**；
#   `ci.yml` 那个 `local-gate` job 的 `GATE_ONLY:` 与它**两向集合相等**（`C8b`）——
#   ⇒ 改了 yml 不改登记、或改了登记不改 yml，**两边都当场红**。
#   「哪几格不进云端」因此**不可能静默**：少跑一格必须先在这儿写一条理由。
#
# 🔴 **反空真锚是 `C8a` 那两向相等**（登记 ↔ 现打格名），不是「登记里每条都找得到格」——
#   后者在登记被清空时**恒真**。这与 `C1` 是同一条道理、同一种写法。
#
# ★ 「不进云端」分**两档**，刻意不混成一档 ——「云端另有人跑」与「云端根本没人看」
#   在一张只写「不进」的表上**长得一模一样**，而后者才是空洞：
#   · `ELSEWHERE`：云端**另有 job 跑同一份被测对象** ⇒ 必须给一条**逐字锚点**，
#     `C8c` 现打要求它在 `ci.yml` 里命中 ≥1（钉子没了 / 陈了，当场红）。
#     ⚠ 锚点买的是「那一步写在 yml 里」，**不是**「那一步在 runner 上跑过、绿过」。
#   · `NOWHERE`：**云端这一维零覆盖**，登记里明写缺什么。不许给锚点（给了就是自相矛盾，`C8c` 红）。
#
# ⚠⚠ **本条的诚实边界，写死在这里**：本拍在**断网沙箱**里做，`gh run view` 一次都做不到
#   ⇒ 下面每一条买到的都只是「**盘上这几份文本满足这几条**」。
#   `C8` 一个字都没说「云端那一趟会绿」，也没说「那个 job 起过」。
#   事实上**它一趟都没起过**：`ci.yml` 的触发器只有 `push`(main/`v*`) 与 `pull_request`，
#   而本仓红线是**不推送** ⇒ 那个 job 与 `tests/hooks/pre-push` 今天**都触发不了**，
#   逐字登记在下面 `HOOKS` 与 `gate.sh` 射程表的 `did-ci-actually-run` 那一条里。
CLOUD, ELSEWHERE, NOWHERE = "进云端子集", "云端另有 job 盖着", "云端零覆盖"

INVOCATION = {}


def invoke(name, where, why, anchor=None):
    INVOCATION[name] = {"where": where, "why": why, "anchor": anchor}


# ── 进云端子集的那几格：只要 git · bash · python3 · node · shellcheck ────────────
# ⚠ 挑人群的口径是**「这个 job 装什么就能跑什么」**，不是「哪几格重要」——
#   后者要拍脑袋，前者现打得出来。要 zig / musl target / mingw / 整棵 cargo 的，一律不进。
_CLOUD_WHY = "只要 `actions/checkout` ＋ runner 自带的 git/bash/python3/node/shellcheck，不装任何工具链"
invoke("worktree-clean", NOWHERE,
       "🔴 **它在云端构造上红不了** —— 云端那一趟是 `actions/checkout` 出来的**全新单份副本**，"
       "「仓里有没有第二份工作副本」这个条件在那儿恒成立。把一格恒绿的东西放进云端子集，买到的是**一分虚的绿**，"
       "而虚的绿与真的绿在流水线的勾上一模一样 ⇒ 明着不放。它治的是本机并发 worktree 那一族（`真相源` 记过 09-19 一天绊三次），那是本机的活")
invoke("hooks", CLOUD, _CLOUD_WHY + "。它判 `tests/hooks/` 下每一份的可执行位 · index mode · 语法 —— 其中 **index mode 那一条只有云端这种全新 checkout 才最有意义**（本机那一份 `chmod` 过的看不出来）")
invoke("copy2", CLOUD, _CLOUD_WHY)
invoke("shellcheck", CLOUD, _CLOUD_WHY + "。⚠ 云端另有 `E2E scripts health` 那个 job 跑同一档，本格与它**同一份人群**（都从 `ci.yml` 现读）⇒ 这里是第二道，不是唯一一道")
invoke("ci-e2e-prereq", CLOUD, _CLOUD_WHY + "：它只读盘上三份文本")
invoke("release-gate", CLOUD, _CLOUD_WHY + "：判据本体是 python3，它顺带调的 `release-notes.mjs --check` 只用 node 内置模块（现打：那份文件的 import 全是 `node:fs`/`node:url`/`node:path`）⇒ **不需要 `npm ci`**")
invoke("gate-selfdesc", CLOUD, _CLOUD_WHY + "。🔴 它是本 job 最承重的一格：`C8` 自己就住在它里面 ⇒ 云端那一趟会自己检查「登记与 yml 对不对得上」")
invoke("platform", CLOUD, _CLOUD_WHY)
invoke("installface", CLOUD, _CLOUD_WHY)
invoke("generated", NOWHERE,
       "🔴 同 `worktree-clean`：它判的是**工作树与 index 的 diff**，而云端那一趟是全新 checkout ⇒ 那个 diff 恒空、**在构造上红不了**。"
       "⚠ 别把这条读成「生成物漂移云端有人管」——真要在云端买到它，得先跑一趟 `npm run gen:types` 再 diff，"
       "那要 `npm ci` ＋ 整棵 cargo（`ts-rs` 是跑测试时导出的）。那笔账本拍没量（断网，起不了云端）⇒ 明着欠着，不拿一个恒绿的格顶上")

# ── 云端另有 job 盖着的那几格：逐条给**逐字锚点**，`C8c` 现打钉着 ────────────────
invoke("fmt", ELSEWHERE,
       "`rust` 那个 job（windows-latest）跑整个 workspace 的 fmt。⚠ 同一条命令、不同 host",
       anchor="cargo fmt --all --check")
invoke("fmt-backend", ELSEWHERE,
       "`backend` 那个 job（ubuntu-latest）在 `src/backend` 上跑 fmt",
       anchor="run: cargo fmt --check")
invoke("cargo", ELSEWHERE,
       "`rust`（windows-latest）与 `rust-linux`（ubuntu-latest）**两个 job** 各跑一趟同一条命令。"
       "⚠ 云端那两趟**不带 `--lib`** ⇒ 人群比本格宽（含 integration/doc 档）；"
       "而本格多一条**包数相等**断言，云端没有 ⇒ 一个 crate 静默掉出 workspace 时**云端看不见**",
       anchor="cargo test --workspace")
invoke("comm-boundary", ELSEWHERE,
       "那 15 条靠 `#[path]` 挂在 `monitor` 的 lib 上 ⇒ 云端那两趟 workspace test **会跑到它们**。"
       "🔴 **但云端没有本格的三方对拍与那两条逐字锚点** —— 整个模块被摘掉时，"
       "云端只是合计小 15，而「小一点」与「有判据没跑」在那边的输出上一模一样。"
       "⚠ 这一条对本格尤其要命：那一族的人群**可能是空集**，"
       "一个人群为空、又没人看它还在不在的判据族，买到的是零。"
       "⇒ 这一维**只有本机这一格买得到**，如实记着",
       anchor="cargo test --workspace")
invoke("test-tiers", ELSEWHERE,
       "那 12 条靠 `#[path]` 挂在 `guard-core` 的 lib 上，而 `guard-core` 是 `src/bridge` workspace 的成员 ⇒ 云端那两趟 workspace test **会跑到它们**。"
       "🔴 **但云端没有本格的三方对拍与那两条锚点** —— 那一行 `mod` 被摘掉时云端只是合计小 12。"
       "⇒ 「测试层分级没有静默消失」这一维**只有本机这一格买得到**，如实记着",
       anchor="cargo test --workspace")
invoke("backend", ELSEWHERE,
       "`backend` 那个 job 在 `src/backend` 上跑单包 `cargo test`",
       anchor="name: Remote backend (Linux) lint + test")
invoke("panorama-engine", NOWHERE,
       "`ci.yml` 在这棵树里只跑 `cargo test -p code-picture-core`（vendor 自己的测试），"
       "不跑本程序的 `cli_tests` ⇒ 这一维云端零覆盖，只有本机这一格")
invoke("tsc", ELSEWHERE,
       "`frontend` 那个 job 跑 `npm run build`，而 `tsc --noEmit` 是它的第一步。"
       "⚠ 云端买的是「build 过得去」，**没有**本格那条「真读进 tsc 的份数 == 盘上现打份数」的恒等对账"
       "（`include` 被收窄那一形，云端看不见）",
       anchor="name: npm run build (tsc + vite)")
invoke("npm", ELSEWHERE,
       "`frontend` 那个 job 跑 `npm test`（同一条命令）",
       anchor="name: unit tests (node pure-fn + vitest DOM)")
invoke("winchk", ELSEWHERE,
       "`rust` 那个 job 整个跑在 **windows-latest 原生**（host = `x86_64-pc-windows-msvc`）"
       "⇒「monitor 在 Windows 上编不编得过」这一维云端有人看。"
       "⚠ **ABI 不同**：本格是 `-gnu` 交叉，云端是 `-msvc` 原生 ⇒ 两边各盖一半，不是同一格",
       anchor="cargo clippy --workspace --all-targets")
invoke("winchk-backend", ELSEWHERE,
       "`backend` 那个 job 有一条 `-msvc` 跨 target check（`真相源/92 §2.1.1` 的 `C3`）。"
       "⚠ 同上：本格 `-gnu`、云端 `-msvc`",
       anchor="cargo check --all-targets --target x86_64-pc-windows-msvc")
invoke("winlink", NOWHERE,
       "`ci.yml` 的 Windows 那个 job（`rust`，`windows-latest`）只跑 clippy 与 `cargo test`（链的是测试二进制，"
       "不链 `monitor.exe`）；真产 exe 的是 `release.yml`（`-msvc` 原生，不在 CI 上）⇒ "
       "「`-gnu` 上两个二进制链得起来」这一维云端零覆盖，只有本机这一格")
for _s in ("backend-rbind-token", "rbind-token-endtoend"):
    invoke("ccm tests/e2e/" + _s, NOWHERE,
           "〔第二波 T4 09-24〕`ci.yml` 里这一套**只在 shellcheck 人群里**，没有 `assert-pass-floor.sh` 调用行 —— "
           "`ci.yml` 步 2 / 步 3 那两段注释逐字「也没有加 `assert-pass-floor` 那一行 …… 待拍板」。"
           "⇒ 云端这一格零覆盖。本格只把它接进**本机**执行链，不替那件待拍板的事拍板")
for _s in ("ccm-print-parity", "ccm-rbind-title", "ccm-cli", "ccm-contract-parity", "backend-cc-bus",
           "backend-gate2", "local-backend", "restart-frames", "restart"):
    invoke("ccm tests/e2e/" + _s, ELSEWHERE,
           "云端有一条同套件的 `assert-pass-floor.sh` 调用行。"
           "⚠ 那几条调用行**在 GitHub runner 上一趟都没跑过**（本仓不推送）—— "
           "`ci.yml` 里 `weak-net` 那一步的头注已经为同一笔账登记过一次",
           anchor="assert-pass-floor.sh " + _s)

# ── 云端零覆盖的那几格：明写缺什么，**不许给锚点** ──────────────────────────────
invoke("muslbuild", NOWHERE,
       "🔴 现打（非截断）：`ci.yml` 里 `musl` 与 `zigbuild` **各命中 1 处，两处都在注释里**，"
       "没有任何一步真编 musl ⇒ **云端这一维零覆盖**，与 `真相源/92 §2.1.4` 那条读数一致。"
       "缺的是：zig 0.14.0 ＋ cargo-zigbuild 0.23.0 ＋ 两个 musl target —— 装得上，但那是一笔"
       "**本拍量不了**的账（沙箱断网，`gh run view` 做不到）⇒ 不猜，明着登记欠着")
invoke("deadcode", NOWHERE,
       "本格是 `cargo check -p monitor` **非 test 构建**里 `never used` 的**恒等棘轮**（钉的数只住 `gate.sh` 那一行 `pin=`；"
       "〔第二波 T4 订正〕这里原先抄着一个 36，而那时 `pin` 早已是 35 —— 散文副本必腐，删了数不删话）。"
       "云端那几趟 clippy 跑的是 `--all-targets`（含 test 档，那些函数有调用方）⇒ **量的不是同一个数**，"
       "也没有任何一处棘轮。⇒ 这一维云端零覆盖")
invoke("ccbus-twophase", NOWHERE,
       "它有 14 条是**真跑**（一次性 `CC_BUS_HOME` 里跑令牌/CAS/并发/Stop 钩子两条路）。"
       "那几条在 GitHub runner 上依赖什么，**本拍一次都没量过**（断网，起不了云端）。"
       "把一格我判不了的东西塞进云端子集，换回来的是一个我读不懂的红或绿 ⇒ 明着不进。"
       "解锁条件写死：**有人能真跑一趟那个 job 并把逐条读数贴回来**，再定")


# ── `HOOKS`：`tests/hooks/` 下每一份**今天触发得了吗**，如实登记 ─────────────────
# 🔴 **「装了钩子」不等于「它跑过」** —— 这张表买的是前半句的**诚实**，不是后半句。
#   后半句由 `tests/evidence/K-G4C-gate-receipt.py` 那张收据买。
# ★ `fires` 那一栏**有现物钉着**（`C8e`）：现打 `git config --get core.hooksPath`，
#   它为空 ⇒ 盘上**每一份** hook 都不在 git 的钩子路径上 ⇒ 登记里任何一条 `fires: True` 当场红。
#   ⚠ 反过来那一半**机检买不到**：`core.hooksPath` 设上了也不代表 `pre-push` 会跑
#   （要有人真 push，而本仓红线是不推送）⇒ 那一条只能是散文，已在 `why` 里逐字写死。
HOOKS = {
    "tests/hooks/pre-commit": {
        "fires": False,
        "why": "**默认是死的**，逐字写在它自己的头注里：要人手 `git config core.hooksPath tests/hooks` "
               "才活。刻意不替用户设 —— 那会改用户本地配置，且 `core.hooksPath` 一设，"
               "`.git/hooks/` 下的东西会全部失效。⚠ 它挡的是 `C7`（`[profile.dev]` 不许进提交），"
               "**不跑门禁**：门禁全量含两趟 musl 交叉编译，挂到每一次提交上，人会立刻 "
               "`--no-verify` 把它关掉，而「关掉了」与「过了」在终端上一模一样 ⇒ 更坏",
    },
    "tests/hooks/pre-push": {
        "fires": False,
        "why": "🔴 **今天触发不了，两层各一条，都如实记**：① 与上面同因 —— `core.hooksPath` 没设，"
               "它不在 git 的钩子路径上（现打，`C8e` 钉着）；② **就算设上也仍然触发不了** —— "
               "本仓红线是**不推送**（用户 09-20 明确裁定），没有 push 就没有 pre-push。"
               "⇒ 它今天是一份**备好的机制**，不是一道在跑的闸。**别把「装上了」读成「有用了」。**"
               "⚠ 那也正是它存在的意义：门禁那张收据的判据（`K-G4C-gate-receipt.py`）"
               "在它里面**真接了线**，哪天用户改口允许推送，这条路当天就是活的，不用再补一次。",
    },
}

def git_config(key):
    """现打一条 git 配置。取不到就返回空串（`C8e` 把空串读成「没设」）。"""
    try:
        out = subprocess.run(["git", "-C", str(ROOT), "config", "--get", key],
                             capture_output=True, text=True)
        return out.stdout.strip()
    except Exception:
        return ""


GATE_ONLY_RE = re.compile(r"^\s*GATE_ONLY:\s*(\S.*?)\s*$", re.M)
CI_YML = ROOT / ".github/workflows/ci.yml"
GATE_CALL = "bash tests/scripts/gate.sh"
RECEIPT_RULER = "tests/evidence/K-G4C-gate-receipt.py"
RECEIPT_WRITER = "gate_write_receipt"


def check_invocation(text, cells):
    """`C8`：门禁被谁调用 —— 登记 ↔ 现打格名 ↔ `ci.yml` 三处对拍。取法见上面那段头注。"""
    out = []
    got, want = set(cells), set(INVOCATION)
    # C8a：反空真锚 —— 两向集合相等
    if got - want:
        out.append(f"C8a `gate.sh` 里有格**没登记「云端跑不跑」**：{sorted(got - want)} —— "
                   f"加一格就回本文件的 `INVOCATION` 补一条（写 `CLOUD` 就同拍改 `ci.yml` 的 "
                   f"`GATE_ONLY:`，写不进云端就明写为什么）")
    if want - got:
        out.append(f"C8b 调用登记里有格**盘上没有**：{sorted(want - got)} —— 登记陈了")

    for name, ent in sorted(INVOCATION.items()):
        if ent["where"] not in (CLOUD, ELSEWHERE, NOWHERE):
            out.append(f"C8c `{name}` 的去处 {ent['where']!r} 不在闭集里")
        if not (ent["why"] or "").strip():
            out.append(f"C8c `{name}` 没给理由 —— 空白格不算登记（同 `C6`）")

    if not CI_YML.exists():
        out.append(f"C8d `{CI_YML}` 盘上不存在 —— 本条的被测对象没了")
        return out
    ci = CI_YML.read_text(encoding="utf-8")

    # C8c：`ELSEWHERE` 的锚点现打要在 `ci.yml` 里；`NOWHERE` 不许有锚点
    for name, ent in sorted(INVOCATION.items()):
        if ent["where"] == ELSEWHERE:
            a = ent["anchor"]
            if not a:
                out.append(f"C8c `{name}` 自称「云端另有 job 盖着」却**没给逐字锚点** —— "
                           f"一句没有钉子的「别处有人看」就是一句没人守的散文")
            elif ci.count(a) < 1:
                out.append(f"C8c `{name}` 的锚点在 `ci.yml` 里命中 0 次：{a!r} —— "
                           f"那一步搬走了 / 改了名，而这条登记还在说「云端有人看」。"
                           f"**那正是本条要抓的形状**（买到了没有，却还在说买到了）")
        elif ent["where"] == NOWHERE and ent["anchor"]:
            out.append(f"C8c `{name}` 自称**云端零覆盖**却带了一条锚点 —— 自相矛盾，"
                       f"两档只能挑一档")

    # C8d：`local-gate` 那个 job 真的在 `ci.yml` 里，且**门禁之后**跑收据判据
    n_gate = ci.count(GATE_CALL)
    if n_gate != 1:
        out.append(f"C8d `ci.yml` 里 `{GATE_CALL}` 命中 {n_gate} 处（应当恰好 1 处）—— "
                   f"这道门在云端零调用，正是 `G4` 空洞③ 的题面；多于 1 处则两个 job "
                   f"各跑一趟，收据会互相盖掉")
    else:
        i_gate = ci.index(GATE_CALL)
        if RECEIPT_RULER not in ci:
            out.append(f"C8d `ci.yml` 里找不到 `{RECEIPT_RULER}` —— **跑了门禁而没人验收据**，"
                       f"那就退回成「装了钩子」那一形：门禁哪一步静默跳过了，云端一个字都不会说")
        elif ci.index(RECEIPT_RULER) < i_gate:
            out.append(f"C8d `ci.yml` 里 `{RECEIPT_RULER}` 排在 `{GATE_CALL}` **前面** —— "
                       f"验的是上一趟留下的收据，那是一张陈收据，比没有更坏")

    # C8d：`GATE_ONLY:` 那一行 ↔ 登记里的 `CLOUD` 集合，**两向相等**
    ms = GATE_ONLY_RE.findall(ci)
    if len(ms) != 1:
        out.append(f"C8d `ci.yml` 里 `GATE_ONLY:` 命中 {len(ms)} 处（应当恰好 1 处）—— "
                   f"云端跑哪几格只许有一份住址")
    else:
        yml_set = {t for t in ms[0].split() if t}
        cloud = {n for n, e in INVOCATION.items() if e["where"] == CLOUD}
        # 归一：`GATE_ONLY` 写套件短名，登记与 `found_cells()` 用规范名（同 `C5d` 那一跳）
        norm = {t if t in got else E2E_PREFIX + t for t in yml_set}
        if norm != cloud:
            out.append(f"C8d `ci.yml` 的 `GATE_ONLY:` 与登记里的「{CLOUD}」**对不上**："
                       f"yml 点了而登记没写 {sorted(norm - cloud)} · "
                       f"登记写了而 yml 没点 {sorted(cloud - norm)} —— "
                       f"🔴 **这一条就是「不许静默少跑」那句话的机器面**："
                       f"少跑一格必须先在登记里写一条理由，改 yml 不改登记当场红")

    # C8e：`HOOKS` 登记 ↔ `git ls-files tests/hooks/`，两向相等 ＋ `fires` 有现物钉着
    on_disk = {f for f in ls_files() if f.startswith("tests/hooks/")}
    reg = set(HOOKS)
    if on_disk - reg:
        out.append(f"C8e `tests/hooks/` 下有文件**没登记「今天触发得了吗」**：{sorted(on_disk - reg)}")
    if reg - on_disk:
        out.append(f"C8e 钩子登记里有文件**盘上没有**：{sorted(reg - on_disk)} —— 登记陈了")
    if not on_disk:
        out.append("C8e `tests/hooks/` 下一个被跟踪的文件都没有 —— 分母是空的，"
                   "这张表全成空真（不许当成绿，同 `hooks` 那一格的口径）")
    hooks_path = git_config("core.hooksPath")
    for f, ent in sorted(HOOKS.items()):
        if not isinstance(ent.get("fires"), bool):
            out.append(f"C8e `{f}` 的 `fires` 不是布尔 —— 「触发得了吗」只许答是/否")
        if not (ent.get("why") or "").strip():
            out.append(f"C8e `{f}` 没说清为什么 —— 空白格不算如实登记")
        if ent.get("fires") and not hooks_path:
            out.append(f"C8e `{f}` 登记着**触发得了**，而现打 `git config core.hooksPath` 是空的 "
                       f"⇒ 盘上没有一份 hook 在 git 的钩子路径上，这条登记是假的。"
                       f"🔴 「装了钩子」不等于「它会跑」——本条买的正是这句话")

    # C8f：收据这条路在 `gate.sh` 里**真接上了**，判据本体也在盘上
    n_writer = text.count(RECEIPT_WRITER)
    if n_writer < 2:
        out.append(f"C8f `gate.sh` 里 `{RECEIPT_WRITER}` 只命中 {n_writer} 处"
                   f"（定义 ＋ 裁决那一段的调用，至少 2 处）—— 收据的写手还在，"
                   f"而**没有人调它**：那就退回成一段注释了（同 `C5c` 对 `gate_print_blind` 的判法）")
    if not (ROOT / RECEIPT_RULER).exists():
        out.append(f"C8f `{RECEIPT_RULER}` 盘上不存在 —— 收据没人判，"
                   f"「这一趟真跑了门禁吗」这句话又回到没有判据的状态")
    return out


def check_self_description(text, cells, rollcall):
    out = []
    hdr = header_lines(text)
    opens = [i for i, ln in hdr if SELF_OPEN in ln]
    closes = [i for i, ln in hdr if SELF_CLOSE in ln]
    if len(opens) != 1 or len(closes) != 1 or closes[0] <= opens[0]:
        out.append(f"C5b 头注里找不到**恰好一段**自述节（开围栏 {len(opens)} 处 · "
                   f"收围栏 {len(closes)} 处，应当各 1 处且开在前）—— "
                   f"自述句散在散文里没有任何东西对得上它，正是 `K-R91` 立案的那一形")
        inside, block = set(), []
    else:
        inside = set(range(opens[0], closes[0] + 1))
        block = [(i, ln) for i, ln in hdr if i in inside]

    if block:
        btxt = "\n".join(ln for _, ln in block)
        ms = SELF_COUNT.findall(btxt)
        if len(ms) != 1:
            out.append(f"C5b 自述节里 `〔自述·格数〕N 格` 命中 {len(ms)} 处（应当恰好 1 处）")
        else:
            n = int(ms[0])
            if n != len(cells):
                out.append(f"C5b 自述节自称 **{n} 格**，而 `gate.sh` 里现打是 **{len(cells)} 格** "
                           f"—— 头注的自述腐了（`C5` 只对拍裁决行，钉不到这里）")
            if rollcall is not None and n != rollcall:
                out.append(f"C5b 自述节自称 {n} 格，而裁决行那句自称 {rollcall} 格 —— "
                           f"同一份文件里两处自述互相对不上")
        roll = [i for i, ln in block if SELF_ROLL in ln]
        if len(roll) != 1:
            out.append(f"C5b 自述节里 `{SELF_ROLL}` 命中 {len(roll)} 处（应当恰好 1 处）")
        else:
            start = roll[0]
            payload = []
            for i, ln in block:
                if i < start:
                    continue
                if i > start and "〔自述·" in ln:
                    break
                payload.append(ln.split(SELF_ROLL, 1)[-1] if i == start else ln)
            # 逐格点名切开：行首的 `#`、围栏竖线、空白与反引号都不是名字的一部分。
            named = {x.strip().lstrip("#│ \t").strip().strip("`").strip()
                     for chunk in payload for x in chunk.split("·")}
            named = {x for x in named if x and "〔" not in x and "─" not in x}
            got = set(cells)
            if named != got:
                out.append(f"C5b 自述节逐格点名与现打的格**对不上**："
                           f"点了而盘上没有 {sorted(named - got)} · "
                           f"盘上有而没点 {sorted(got - named)}")
        for i, ln in block:
            for tok in BACKTICK.findall(ln):
                if PATHISH.match(tok) and ("/" in tok or "." in tok) and not (ROOT / tok).exists():
                    out.append(f"C5b 自述节 `gate.sh:{i}` 点名的 `{tok}` **盘上不存在** —— "
                               f"自述里点名的东西必须现打找得到（`:22–25` 那一形一个数字都没有，"
                               f"烂的正是这一维）")

    for i, ln in hdr:
        if i in inside or HIST_MARK in ln:
            continue
        if SELF_REF.search(ln) and SELF_NUM.search(ln):
            out.append(f"C5b `gate.sh:{i}` 在自述节**之外**又说了一遍自己跑几格："
                       f"{ln.strip()[:70]!r} —— 自述句只许住在自述节里；"
                       f"若这是历史读数，就在**这一行**逐字带上 `{HIST_MARK} …〕`")

    for para in header_paragraphs(hdr):
        if para[0][0] in inside:
            continue
        if HIST_MARK in para[0][1]:
            continue
        bad = {}
        for i, ln in para:
            for tok in BACKTICK.findall(ln):
                if SUITE_TOK.match(tok) and not suite_on_disk(tok):
                    bad.setdefault(tok, i)
        if bad:
            out.append(f"C5b `gate.sh:{para[0][0]}–{para[-1][0]}` 这一段点名了 `tests/e2e/` 下"
                       f"**没有那份文件**的套件："
                       + " · ".join(f"`{t}`（首见 :{n}）" for t, n in sorted(bad.items()))
                       + f" —— 若这一段是历史账，就在**段落首行**逐字带上 `{HIST_MARK} …〕`")
    return out


def check_no_gate_needed(orphans):
    """`C6b`（`K-R82` `KR82D3`）：0 格覆盖的树必须**明写「不需要门」**，且那句话本身被钉住。

    ⚠ 它是 `C6`（「一格的裁词都不许空」）同一条道理换了个方向：`C6` 管**裁词**的理由，
      本条管**整棵树 0 覆盖**的理由。⇒ 编号刻意是 `C6b` 而**不是 `C7`**：
      本仓的 `C7` 是宪章那条「vendor 不动」，同名会让两处互相冒充。
    ⚠ 它买的是「**这句话还站得住**」，**不买「这句话对不对**」—— 与 `C1`–`C6` 同一个边界：
      一条写错的理由能骗过它。它只保证理由**在**、**不敷衍**、**指得到的东西还在盘上**。
    """
    out = []
    rootfiles = {p for p in ls_files() if "/" not in p}
    # 🔴 〔订正 09-19〕`C6b` 的人群从「孤儿树」扩成「孤儿树 ∪ 明标 `partial` 的树」。
    #   题面：`<仓根文件>` 今天被 `release-gate` 盖到 **1 格**（它读 `package.json` 的 `version`）
    #   ⇒ 按上一版那条两向对拍，它**不再是孤儿**，于是「登记说它不需要门」当场红，
    #   而正确的动作不是删掉那条说明 —— 14 份仓根文件里真正进了那一格的**只有 1 份**，
    #   另外 13 份的处境一个字没变。**一条 `部` 裁词把整棵树的逐份普查关掉了，这才是病。**
    #   ⇒ 开一个 `partial` 标，两条纪律拴住它，不让它变成万能豁免：
    #     · 标了 `partial` 的树**必须真被至少一格盖到**（否则它就是孤儿，该摘掉这个标）
    #     · 它的 `archive` 仍要**逐份盖全**（`C6c` 一个字没放松）
    partial = {t for t, e in NO_GATE_NEEDED.items() if e.get("partial")}
    for t in sorted(partial & set(orphans)):
        out.append(f"C6b `{t}` 标了 `partial`（说它被格盖到了），而它今天 **0 格覆盖** —— "
                   f"这个标陈了，摘掉它，这棵树就是普通孤儿")
    got, want = set(orphans) | partial, set(NO_GATE_NEEDED)
    for t in sorted(got - want):
        out.append(f"C6b `{t}` 今天 0 格覆盖，而登记里**一条说明都没有** —— "
                   f"「不需要」与「没查」在输出上一模一样，这一条要求写死是前者")
    for t in sorted(want - got):
        out.append(f"C6b 登记说 `{t}` 不需要门，而它今天**已经被格盖到了** —— 这条说明陈了，回来删")
    for t in sorted(got & want):
        ent = NO_GATE_NEEDED[t]
        why = (ent.get("why") or "").strip()
        if len(why) < MIN_WHY:
            out.append(f"C6b `{t}` 的「不需要门」只有 {len(why)} 字（下限 {MIN_WHY}）—— "
                       f"读不出理由的一句话等于没写")
        hit = [w for w in VAGUE if w in why]
        if hit:
            out.append(f"C6b `{t}` 的理由里出现了敷衍词 {hit} —— "
                       f"件计划 `KR82D3` 逐字：理由要逐条给，不许写「不重要」")
        wf, wt = ent.get("witness_file"), ent.get("witness_text")
        if wf:
            fp = ROOT / wf
            if not fp.exists():
                out.append(f"C6b `{t}` 的钉子 `{wf}` 盘上不存在 —— 那句「不需要」失去依据")
            else:
                n = fp.read_text(encoding="utf-8", errors="replace").count(wt)
                if n != 1:
                    out.append(f"C6b `{t}` 的钉子 `{wf}` 里逐字 {wt!r} 命中 {n} 次（应当恰好 1 次）"
                               f"—— 那份留档改了措辞或没了，理由指空了")
        wc = ent.get("witness_cell")
        if wc and wc not in REGISTRY:
            out.append(f"C6b `{t}` 的理由点名了 `{wc}` 那一格，而登记里没有这一格 —— 理由指空了")
        for f in ent.get("witness_rootfiles", []):
            if f not in rootfiles:
                out.append(f"C6b `{t}` 的理由点名的仓根文件 `{f}` 今天不在 `git ls-files` 的"
                           f"仓根文件里 —— 理由指空了")
        wj = ent.get("witness_json_key")
        if wj:
            f, path = wj
            try:
                cur = json.loads((ROOT / f).read_text(encoding="utf-8"))
                for k in path:
                    cur = cur[k]
                if not str(cur).strip():
                    raise ValueError("空值")
            except Exception as exc:
                out.append(f"C6b `{t}` 的理由说 `{'.'.join(path)}` 住在 `{f}` 里，"
                           f"而现打取不到（{type(exc).__name__}: {exc}）—— 那句话不成立了")
    return out


# ── `C6c`（`K-R91` `KR91D2`）：0 覆盖那棵树的理由，**每一份成员都归到一档** ──────
#
# 🔴 **分母是那棵树的现打成员**（`git ls-files` 每趟重数），**不是登记里写死的清单** ——
#   写死清单那条路的失效方向逐字写在件计划里：那样往仓根加一份文件**永远不会红**，
#   与 `R42 裁定零` 那个「数的是那句话，不是那些格」是同一形。
#
# ⚠ 它**不判归得对不对**（把 `index.html` 归进「文档」它照样绿 —— 那要读语义，`T126`
#   关掉了那条路），只判**有没有归**。这与 `C6b` 是同一条边界，方向不同：
#   `C6b` 查「这句话还站不站得住」，`C6c` 查「这句话的枚举盖没盖全那棵树」。
def tree_members(tree, files):
    """那棵树**现打**的成员。
    🔴 〔订正 09-19〕走 `tree_of()` 的**首匹配**，不再用裸 `startswith` ——
    重构后树之间**有前缀包含关系**（`src/` ⊃ `src/bridge/`），裸 `startswith` 会让
    catch-all 那棵把所有子树的成员**再数一遍**，分母当场虚高、而且没有任何判据会红。"""
    return sorted(p for p in files if tree_of(p) == tree)


def bucket_hits(pattern, tree, members):
    """一条通配串在**现打成员**里命中哪几份。树内成员按「树相对名」也匹配一次。"""
    hits = []
    for m in members:
        rel = m[len(tree):] if tree != ROOTFILES and m.startswith(tree) else m
        if fnmatch.fnmatchcase(m, pattern) or fnmatch.fnmatchcase(rel, pattern):
            hits.append(m)
    return hits


def check_archive(orphans, files):
    out = []
    # 人群与 `C6b` 同步：孤儿树 ∪ 明标 `partial` 的树（理由见 `check_no_gate_needed`）。
    for t in list(orphans) + [t for t, e in NO_GATE_NEEDED.items()
                              if e.get("partial") and t not in orphans]:
        ent = NO_GATE_NEEDED.get(t)
        if not ent:
            continue          # 那一形归 `C6b`（「登记里一条说明都没有」），这里不重复报
        members = tree_members(t, files)
        arch = ent.get("archive") or []
        if not arch:
            out.append(f"C6c `{t}` 的理由**没有分档表** —— 它现打 {len(members)} 份成员，"
                       f"而理由只是一段散文 ⇒ 那是「一句全称句罩着一群没数过的文件」，"
                       f"正是 `K-R91` 立案的那一形")
        covered = set()
        for b in arch:
            name = b.get("档") or "<无名档>"
            verdict = b.get("判")
            if verdict not in (NEED_NONE, UNJUDGED):
                out.append(f"C6c `{t}` / 档「{name}」的判词 {verdict!r} 不在闭集 "
                           f"{NEED_NONE}/{UNJUDGED} 里 —— 「已被数到」与「不需要门」不许混成一个词")
            why = (b.get("why") or "").strip()
            if len(why) < MIN_BUCKET_WHY:
                out.append(f"C6c `{t}` / 档「{name}」的理由只有 {len(why)} 字"
                           f"（下限 {MIN_BUCKET_WHY}）—— 读不出理由的一句话等于没写")
            for w in VAGUE + SWEEPING:
                if w in why:
                    out.append(f"C6c `{t}` / 档「{name}」的理由里出现了 {w!r} —— "
                               f"敷衍词与全称句都挡：一档的理由要说的是**这一档**")
            pats = b.get("成员") or []
            if not pats:
                out.append(f"C6c `{t}` / 档「{name}」一条通配串都没有 —— 空档归不了任何一份")
            for pat in pats:
                if pat in CATCH_ALL:
                    out.append(f"C6c `{t}` / 档「{name}」用了全域通配 {pat!r} —— "
                               f"拿一条通配把整棵树罩住就是换个写法的全称句，本条不收")
                    continue
                hits = bucket_hits(pat, t, members)
                if not hits:
                    out.append(f"C6c `{t}` / 档「{name}」的通配串 {pat!r} 今天**一份都匹配不到** —— "
                               f"这一条档标陈了（分母是现打的，登记要跟着盘走）")
                covered.update(hits)
        missing = [m for m in members if m not in covered]
        if missing:
            out.append(f"C6c 🔴 `{t}` 现打 **{len(members)}** 份成员里 **{len(missing)}** 份"
                       f"**没有归到任何一档**（分母 = `git ls-files` 现打，不是登记里的清单）"
                       f"—— 逐字点名：" + " · ".join(f"`{m}`" for m in missing))
    return out


def verdict_of(ent, tree):
    """取一格对一棵树的裁词；登记漏了就回 `?`（不崩，让 `C3` 那句话印得出来）。"""
    v = ent["cover"].get(tree)
    return v[0] if isinstance(v, tuple) and v else "?"


def main():
    text = GATE.read_text(encoding="utf-8")
    fails = []

    cells = found_cells(text)
    dup = [c for c in set(cells) if cells.count(c) > 1]
    if dup:
        fails.append(f"C1 `gate.sh` 里同名判定格出现不止一次：{sorted(dup)} —— 登记按名字索引，同名就点不准")
    got, want = set(cells), set(REGISTRY)
    if got - want:
        fails.append(f"C1 `gate.sh` 里有格**没登记**：{sorted(got - want)} —— "
                     f"加一格就回 `evidence/K-R80-gate-cell-coverage.py` 补一条")
    if want - got:
        fails.append(f"C1 登记里有格**盘上没有**：{sorted(want - got)} —— 登记陈了")

    for name, ent in sorted(REGISTRY.items()):
        n = text.count(ent["anchor"])
        if n != 1:
            fails.append(f"C2 `{name}` 的逐字锚点在 `gate.sh` 里命中 {n} 次（应当恰好 1 次）：{ent['anchor']!r}")
        miss = [t for t in TREES if t not in ent["cover"]]
        if miss:
            fails.append(f"C3 `{name}` 对这几棵树没有裁词：{miss}")
        extra = [t for t in ent["cover"] if t not in TREES]
        if extra:
            fails.append(f"C3 `{name}` 给了不在全集里的树表态：{extra}")
        for t, v in ent["cover"].items():
            if not (isinstance(v, tuple) and len(v) == 2):
                fails.append(f"C3 `{name}` / `{t}` 的裁词不是「(裁词, 理由)」两元组")
                continue
            verdict, why = v
            if verdict not in (FULL, PART, NONE):
                fails.append(f"C3 `{name}` / `{t}` 的裁词 {verdict!r} 不在闭集 {FULL}/{PART}/{NONE} 里")
            if not (why or "").strip():
                fails.append(f"C6 `{name}` / `{t}` 的裁词没带理由 —— 空白格不算点名")

    # ── `C4`〔重写 09-19〕树的全集 ↔ 盘 ──────────────────────────────────────
    # 上一版判的是「登记的树 == `git ls-files` 现打的**顶层目录**」。重构后顶层只剩 4 个，
    # 照那条判法这张表只能有 4 棵 —— 而那张盲区转置表正是靠**细粒度**说话的。
    # ⇒ 换单位，买更强的两条（见文件头 `TREES` 那段）：
    #   · `C4a` **分区**：每一份被跟踪文件归到恰好一棵登记的树，份数合计 == 现打总数
    #   · `C4b` **非空**：每一棵登记的树现打份数 > 0
    # 🔴 `C4a` 写成**恒等**不写成「有没有漏」 —— 「一份都没漏」与「一份都没数」
    #   在布尔面上同形。catch-all（`src/` / `tests/`）被删、或 `tree_of()` 的顺序被改，
    #   合计当场对不上；这是本条唯一能红的路，也正是它存在的理由。
    tracked = [f for f in ls_files() if f]
    per_tree = {t: len(tree_members(t, tracked)) for t in TREES}
    if sum(per_tree.values()) != len(tracked):
        fails.append(f"C4a 分区对不上：{len(TREES)} 棵树合计 {sum(per_tree.values())} 份，"
                     f"而 `git ls-files` 现打 {len(tracked)} 份 —— "
                     f"catch-all 那两棵（`src/` / `tests/`）被动过，或 `TREES` 的顺序被改了")
    for t in TREES:
        if per_tree[t] == 0:
            fails.append(f"C4b 登记里的树 `{t}` 现打 **0 份** —— 它搬走了或被删空了，登记陈了")
        if t is not ROOTFILES and not (ROOT / t).exists():
            fails.append(f"C4b 登记里的树 `{t}` 盘上不存在")

    m = ROLLCALL.search(text)
    if not m:
        fails.append("C5 `gate.sh` 末尾找不到 `GATE: OK —— <n> 格全绿` 那一行 —— 那行点名没了，本条按红处理")
    elif int(m.group(1)) != len(cells):
        fails.append(f"C5 `GATE: OK` 那行自称 {m.group(1)} 格，而现打是 {len(cells)} 格 —— 加了格而点名没跟")
    # `C5b`（`K-R91`）：**扩 `C5` 的射程，一个字没动它上面那三方对拍** ——
    # 它买的是裁决行，本条买的是**头注里的自述**（那儿的腐 `C5` 钉不到）。
    fails += check_self_description(text, cells, int(m.group(1)) if m else None)
    # `C5c`（`K-R122`）：裁决行上那句**射程**（`KR122D2` 乙），取法见上面那段头注。
    fails += check_blind_scope(text, cells)
    # `C5d`（`G4` 空洞③ 09-20）：裁决行那串**点名**与盘上两向相等 ＋
    #   射程表的说明栏里不许再有第二份「这个数怎么来的」。取法见上面那段头注。
    fails += check_verdict_rollcall(text, cells)
    # `C8`（`G4` 空洞③ 09-20）：这道门**被谁调用** —— 登记 ↔ 现打格名 ↔ `ci.yml`
    #   三处对拍 ＋ 钩子那张「今天触发得了吗」。取法与诚实边界见上面那段头注。
    fails += check_invocation(text, cells)

    print(f"# `K-R80` `KR80D3` 门禁分格覆盖登记 —— 量于 `{GATE}`")
    print()
    print(f"判定格 **{len(cells)}** 个 · 树的全集 **{len(TREES)}** 棵"
          f"（现打合计 {sum(per_tree.values())} 份，分区不重不漏）")
    print()
    print("| 格 | cwd | 命令 | 全 | 部 | 🔴 盖不到（逐棵点名） |")
    print("|---|---|---|---|---|---|")
    # ⚠ 这张顺序表**只管印，不管判** —— `c in REGISTRY` 那道过滤会让写错的名字**静静消失**
    #   （步 8 把 `fmt-daemon`/`winchk-daemon`/`daemon` 改名之后，这里三行陈了一个月没人响）。
    #   ⇒ 下面那条 `missing` 把「登记里有、这张表没点到」当场印出来，不让它再静默。
    order = [c for c in ("worktree-clean", "hooks", "copy2", "shellcheck", "ci-e2e-prereq", "release-gate",
                         "gate-selfdesc", "platform",
                         "installface", "fmt", "fmt-backend", "winchk", "winchk-backend", "winlink",
                         "muslbuild", "cargo", "comm-boundary", "test-tiers", "deadcode", "generated", "backend",
                         "panorama-engine", "tsc", "npm")
             if c in REGISTRY] + sorted(c for c in REGISTRY if c.startswith("ccm tests/e2e/"))
    missing = sorted(set(REGISTRY) - set(order))
    if missing:
        print(f"> ⚠ 这张表的打印顺序没点到：{missing}（登记里有，下面按名字补在末尾）")
        order += missing
    for name in order:
        ent = REGISTRY[name]
        # ⚠ 一律走 `verdict_of` —— 直接下标会在「登记漏了一棵树」那一形上 `KeyError` 崩掉，
        #   而崩掉虽然也是非零退出，`C3` 那句「哪一格漏了」就一个字都印不出来（红了但说不清）。
        full = [t for t in TREES if verdict_of(ent, t) == FULL]
        part = [t for t in TREES if verdict_of(ent, t) == PART]
        none = [t for t in TREES if verdict_of(ent, t) == NONE]
        print(f"| `{name}` | `{ent['cwd']}` | `{ent['cmd']}` | {'·'.join(full) or '—'} | "
              f"{'·'.join(part) or '—'} | **{len(none)}**：{'·'.join(none)} |")
    print()
    print("## 反过来看：**每一棵树被几格盖到**（同一份登记的转置，不是第二把尺子）")
    print()
    print("| 树 | 全 | 部 | 盖到它的格数 |")
    print("|---|---|---|---|")
    orphans = []
    for t in TREES:
        f = [c for c in order if verdict_of(REGISTRY[c], t) == FULL]
        p = [c for c in order if verdict_of(REGISTRY[c], t) == PART]
        if not f and not p:
            orphans.append(t)
        fs = "·".join("`%s`" % x for x in f) or "—"
        ps = "·".join("`%s`" % x for x in p) or "—"
        print(f"| `{t}` | {fs} | {ps} | {len(f) + len(p)} |")
    print()
    if orphans:
        # ⚠ 格数**现算**。上一版这里写死着 `12` —— 那正是 `R42 裁定零` 那个被传播了 46 次的
        #   错数的同一形状（数的是那句话，不是那些格）。`K-R82` 改成 `len(cells)`。
        print(f"🔴 **这 {len(orphans)} 棵树今天 {len(cells)} 格里一格都盖不到**："
              + " · ".join(f"`{t}`" for t in orphans))
        print()
        print("### 它们各自**为什么不需要门**（`K-R82` `KR82D3`，由 `C6b` 逐条钉住）")
        print()
        print("🔴 **「不需要」与「没查」在输出上一模一样** —— 上面那个 `0` 本身分不开这两种，"
              "下面这几条写死是前者。")
        for t in orphans:
            ent = NO_GATE_NEEDED.get(t)
            print()
            print(f"- **`{t}`** —— " + (ent["why"] if ent else "**登记里没有这一条** —— 见下面 `C6b`"))
        print()
        print("### 逐份归档（`C6c`：**分母是那棵树的现打成员**，不是登记里写死的清单）")
        print()
        print("🔴 **`不需要门` 与 `未裁` 是两件事** —— 上一版这两种在输出上长得一模一样。")
        files_now = ls_files()
        for t in orphans:
            ent = NO_GATE_NEEDED.get(t)
            members = tree_members(t, files_now)
            arch = (ent or {}).get("archive") or []
            print()
            print(f"- **`{t}`** 现打 **{len(members)}** 份成员 · **{len(arch)}** 档")
            covered = set()
            for b in arch:
                hits = []
                for pat in b.get("成员") or []:
                    if pat not in CATCH_ALL:
                        hits += bucket_hits(pat, t, members)
                hits = sorted(set(hits))
                covered.update(hits)
                shown = " · ".join(f"`{x}`" for x in hits[:8]) + (" …" if len(hits) > 8 else "")
                print(f"  - 〔{b.get('判')}〕**{b.get('档')}** —— 通配 "
                      + " · ".join(f"`{x}`" for x in b.get("成员") or [])
                      + f" ⇒ 现打命中 **{len(hits)}** 份"
                      + (f"（前 8 份：{shown}）" if len(hits) > 8 else (f"：{shown}" if hits else ""))) 
                print(f"    - {b.get('why')}")
            missing = [x for x in members if x not in covered]
            if missing:
                print(f"  - 🔴 **没归到任何一档的 {len(missing)} 份（逐字点名）**："
                      + " · ".join(f"`{x}`" for x in missing))
            else:
                print(f"  - ✅ **{len(members)} 份全部归到了档**（分母现打；往这棵树新增一份"
                      f"落不进任何一档的文件 ⇒ `C6c` 当场红）")
    else:
        print("每一棵树至少被一格盖到。")
    print()
    fails += check_no_gate_needed(orphans)
    fails += check_archive(orphans, ls_files())
    print("## 逐格的理由（`C6`：一格都不许空）")
    for name in order:
        ent = REGISTRY[name]
        print(f"\n### `{name}`")
        for t in TREES:
            v, why = ent["cover"].get(t, ("?", "**登记漏了这一棵** —— 见上面 C3"))
            print(f"- {v} `{t}` —— {why}")
    print()
    print("## 现打：跨树读点矩阵（不是判据，是让上面那些「部」的数可复算）")
    for owner, row in sorted(cross_tree_reads().items()):
        print(f"- `{owner}` 里的读点指向：" + " · ".join(f"`{t}` {n}" for t, n in sorted(row.items())))

    print()
    if fails:
        print(f"KR80D3: FAIL={len(fails)}")
        for f in fails:
            print(f"  ✗ {f}")
        return 1
    # 🔴 门禁那一格的读数行：`run_gate` 靠「N passed」认这一格**真的跑了**
    #   （`0 passed 不是绿` 是它的另一条判定）。〔09-19 接进门禁时加〕
    # 分母**每趟现算**，逐项写死在下面，不许手抄：
    n_arch = sum(len(b.get("成员") or []) for e in NO_GATE_NEEDED.values()
                 for b in (e.get("archive") or []))
    checks = (2                                   # C1 两向集合对拍
              + len(REGISTRY)                     # C2 每格一条逐字锚点，count()==1
              + len(REGISTRY) * len(TREES) * 2    # C3 裁词在闭集里 + C6 裁词带理由
              + 1                                 # C4a 分区恒等（15 棵合计 == 现打总数）
              + len(TREES) * 2                    # C4b 每棵非空 + 每棵盘上存在
              + 1                                 # C5  裁决行的数 == 现打格数
              + 2                                 # C5d 裁决行那串点名两向 ＋ 说明栏黑名单
              + 2                                 # C8a 调用登记两向集合对拍
              + len(INVOCATION)                   # C8c 每格一条去处＋理由（`ELSEWHERE` 还要锚点现打在 ci.yml）
              + 3                                 # C8d job 在、收据判据排在门禁之后、GATE_ONLY 两向
              + 2 + len(HOOKS)                    # C8e 钩子登记两向 ＋ 每份一条「今天触发得了吗」
              + 2                                 # C8f 收据写手接上了 ＋ 判据本体在盘
              + len(NO_GATE_NEEDED)               # C6b 每条「不需要门」的说明 + 钉子
              + n_arch)                           # C6c 分档表里逐份点名的成员
    # 🔴 **这个数不是本条的反空真锚** —— 它只说「判了多少条」。真正拦「一条都没判」的是
    #   `C1` 那两向**集合相等**：登记空了、或 `gate.sh` 读成了空串，`got`/`want` 当场分叉。
    #   ⇒ 「扫到 0 格所以绿」在本条上走不通，而这句话有死值验撑着（刀 2：改一格的名 ⇒ C1 红）。
    print(f"gate-selfdesc: {checks} passed（分母 = C1 两向 2 ＋ C2 锚点 {len(REGISTRY)} ＋ "
          f"C3/C6 逐格逐树裁词与理由 {len(REGISTRY)}×{len(TREES)}×2 ＋ C4a 分区恒等 1 ＋ "
          f"C4b 每棵树非空与在盘 {len(TREES)}×2 ＋ C5 格数对拍 1 ＋ "
          f"C5d 裁决行点名两向 ＋ 说明栏黑名单 2 ＋ "
          f"C8 被谁调用（登记两向 2 ＋ 逐格去处 {len(INVOCATION)} ＋ ci.yml 三条 3 ＋ "
          f"钩子登记 {2 + len(HOOKS)} ＋ 收据接线 2）＋ "
          f"C6b「不需要门」{len(NO_GATE_NEEDED)} 条 ＋ C6c 分档成员 {n_arch} 份）")
    print("KR80D3: OK —— C1..C6 全过；C5b 全过（`K-R91` `KR91D1`）；"
          "C5c 全过（`K-R122` `KR122D2` 乙：裁决行那句射程）；"
          "C5d 全过（`G4` 空洞③：裁决行那串点名两向相等 ＋ 说明栏不许有第二份取法）；"
          "C8 全过（`G4` 空洞③：门禁被谁调用 —— 登记 ↔ 格名 ↔ `ci.yml` 两向相等 ＋ "
          "钩子「今天触发得了吗」如实登记；⚠ 它买的**只是盘上这几份文本**，"
          "一个字都没说云端那一趟会绿，也没说那个 job 起过）；"
          "C6b 全过（`K-R82` `KR82D3`）；C6c 全过（`K-R91` `KR91D2`）"
          "（⚠ 它只判「登记完整且指得到真东西」，不判裁词对不对、不判归得对不对）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
