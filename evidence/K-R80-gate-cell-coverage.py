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
ROOT = Path(os.environ.get("K_R80_ROOT") or Path(__file__).resolve().parent.parent)
GATE = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "scripts" / "gate.sh"

# ── 树的全集 ────────────────────────────────────────────────────────────────
# 分母不是拍脑袋定的：`C4` 拿 `git ls-files` 现打的**顶层目录**跟它对拍。
# 两处刻意与顶层目录不一样，各有理由，都写在这里：
#   · `src-tauri/vendor/code-picture-core/` 从 `src-tauri/` 里**单拆出来** ——
#     `C7` 逐字「vendor `code-picture-core` **不动**」，它被 `cargo` 那一格显式 `--exclude`，
#     与 `src-tauri/` 的其余部分**受不同的门管**，混成一棵就点不出这一格盲区。
#   · `<仓根文件>` 是一格，装 `package.json` / `vite.config.ts` / `README.md` 那些
#     不属于任何顶层目录的文件（`git ls-files` 里 `NF==1` 的那些）。
VENDOR = "src-tauri/vendor/code-picture-core/"
ROOTFILES = "<仓根文件>"
TREES = [
    "src/",
    "src-tauri/",            # 不含 vendor 那一棵
    VENDOR,
    "remote-daemon-proto/",
    "e2e/",
    "scripts/",
    "shared/",
    "hooks/",
    "doc/",
    ".github/",
    "evidence/",
    ROOTFILES,
]

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
    cmd="bash scripts/hooks-are-runnable.sh",
    **{
        "hooks/": (FULL, "`git ls-files hooks/` 下**每一个**被跟踪的文件，各 3 条："
                         "盘上可执行（`test -x`）· 库里记着可执行位（index mode `100755`）· "
                         "语法过得了它自己 shebang 声明的解释器。⚠ 买的是「**跑得起来**」这一层，"
                         "**不买「它拦得对」**——`pre-commit` 那条 `[profile.dev]` 正则判得准不准，"
                         "本格一个字都问不出来"),
        "scripts/": blind("本格的**尺子**住在 `scripts/hooks-are-runnable.sh` —— "
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
    cmd="python3 evidence/K-R115-ruler.py",
    **{
        "evidence/": (PART, "只判 `evidence/*.py` 里 `shutil` 保元数据复制族"
                            "（`copy2` / `copytree` / `copystat`）的**调用点**"
                            "（现打 11 处）：它的目的地落不落在被 git 跟踪的树内内容上。"
                            "⚠ 这棵树的其余部分（`.md` 留档 · `.tsv`/`.json` 读数 · "
                            "`.py` 里除这一族之外的每一行）本格**一个字都不问**。"
                            "⚠ 尺子自己也住这棵树（`evidence/K-R115-ruler.py`）"
                            "—— 它自己那份里这一族**零命中**，所以不构成自匹配；"
                            "哪天它自己用上了，本格会把它和别人一样判一遍"),
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
        "src-tauri/": (PART, "**只有 `-p monitor` 一个包的生产段**：非 test 构建里 "
                             "`never used` 的条数，上限 54 / 下限 40 的递减棘轮。"
                             "⚠ 同 workspace 的其余成员不在 `-p` 里；`#[cfg(test)]` 里的"
                             "死代码任何非 test 构建都看不见 —— 这两块本格都盖不到"),
        VENDOR: blind("`-p monitor` 只编它自己那一个包的生产段；vendor 作为依赖被编，"
                      "而依赖的警告不进 `-p` 那个包的 `never used` 计数（`cargo` 只报"
                      "本包的 lint）⇒ 这棵树本格一条都数不到"),
        "remote-daemon-proto/": blind("另一个 workspace，`-p monitor` 够不着 —— "
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
        "e2e/": (PART, "现打 `e2e/*.sh` **30** 份 ＋ `e2e/weak-net/*.sh` **4** 份 ＋ "
                       "`e2e/fake-claude` **1** 份 = 35 / 这棵树现打 **50** 份。"
                       "⚠ 剩下那 15 份（`.ts` / `.py` / `.tsv` / `fixtures/`）本格一行都不读；"
                       "⚠ 买的是「`--severity=error` 这一档没有告警」，**不买「脚本干得对」**"),
        "shared/": (PART, "只有 `shared/cc-bus/scripts/*` **14** 份 / 这棵树现打 **19** 份 —— "
                          "`shared/cc-bus/examples/` 那 3 份与仓根那 1 份不在人群里"),
        "scripts/": (PART, "`scripts/*.sh` 现打 **3** 份 / 这棵树现打 **6** 份 —— "
                           "`run.ps1` 全仓没有 linter（`audit-0805` 登记的诚实边界），"
                           "`assert-coverage-floors.mjs` 是 JS"),
        "src-tauri/": (PART, "只有 vendored `cc-acct-iso` 那 **4** 份 bash（逐份点名，不用 glob —— "
                             "`ci.yml` 那段注释逐字记着为什么：不开 globstar 时 `**` 等价于 `*`，"
                             "会把一个目录喂给 shellcheck ⇒ 恒红）。这棵树的其余部分与本格无关"),
        "hooks/": (PART, "只有 `hooks/pre-commit` 这**一份**，而且是**逐份点名**进人群的、不是 glob "
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
        "src-tauri/": (PART, "只有那 8 个 workspace 成员从 crate 根顺 mod 走得到的 `.rs`；"
                             "`tauri.conf.json` / `Cargo.toml` 那些非 Rust 文件一概不在"),
        VENDOR: blind("`[workspace] exclude` 把它排掉了 —— 而 `C7` 逐字「vendor 不动」，"
                      "把它拉进出货门禁就是一道我们满足不了的闸"),
        "remote-daemon-proto/": blind("另一个 workspace —— **这就是 `K-R80` 的题面**，"
                                      "今天由 `fmt-daemon` 那一格盖"),
    },
)

cell(
    "fmt-daemon",
    anchor="run_gate fmt-daemon '不是数出来的数",
    cwd="remote-daemon-proto/",
    cmd="cargo fmt --check（刻意不加 --all）",
    **{
        "remote-daemon-proto/": (PART, "唯一成员 `cc-monitor-remote`，射程 = 从 `src/main.rs` "
                                       "顺 `mod` 走得到的那些 `.rs`；走不到的文件本格看不见"),
        "src-tauri/": blind("刻意**不加** `--all`：加了 rustfmt 实收 12 个 crate 根、11 个在这棵树外"),
        VENDOR: blind("同上 —— 加 `--all` 会把这棵我们无权修的树拉进来"),
    },
)

cell(
    "winchk",
    anchor="run_gate winchk '不是数出来的数",
    cwd="src-tauri/",
    cmd="cargo check --locked -p monitor --target x86_64-pc-windows-gnu",
    **{
        "src-tauri/": (PART, "`-p monitor` 一个包的**生产段编得过**；7 个共享 crate 作为依赖被编。"
                             "⚠ `#[cfg(test)]` 不进 `check`，「行为对」更买不到"),
        VENDOR: (PART, "作为 `monitor` 的依赖被编。⚠ 这一条**本件没现打**，"
                       "是从依赖关系推的 —— 按「未验」读"),
        "remote-daemon-proto/": blind("本格逐字写着它盖不到：那 17 处 `cfg(windows)` 没人跨编"),
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
    cmd="python3 evidence/K-R122-ruler.py",
    **{
        ".github/": (PART, "只判 `ci.yml` 一份文件里的**一个切片**：`steps:` 里那 20 条 e2e "
                           "调用行，各自的 build 前置齐不齐。⚠ 这棵树的其余部分"
                           "（`release.yml` 整份 · 那些 job 的 runner / 工具链 / needs / if）"
                           "本格一个字都不问；⚠ 它**不跑任何 e2e**，「前置齐了」≠「那一套会绿」"),
        "e2e/": (PART, "那 20 条调用行指到的 `.sh`（现打 20 份，其中 15 份硬门后端二进制）"
                       "**只被读一个字面量**（`debug/cc-monitor-remote` 在不在）—— "
                       "脚本里的任何一行断言、任何一处行为，本格都不看"),
        ROOTFILES: blind("`package.json` 的 `scripts` 那一块在本格里是**索引**"
                         "（把套件名解析到那份 `.sh`），**不是被检对象** —— "
                         "与上面 `shellcheck` 那一格判 `.github/` 「无」同一条理由："
                         "被读的那份配置不算被它盖到。⚠ 这一条判「无」是**有代价**的："
                         "`test:<套件>` 那条脚本被改坏时本格报的是「找不到脚本」，"
                         "而那句诊断说的是**索引坏了**，不是「仓根文件有问题」"),
        "scripts/": blind("本格的**尺子**住 `evidence/K-R122-ruler.py`，不在这棵树上；"
                          "而尺子本来也不该算进自己的覆盖"),
        "evidence/": blind("同上 —— 把量具自己算进它的覆盖，正是本区最高频那族病"),
    },
)

# ── `K-R122`（09-14）：第 18 格 `winchk-daemon` ───────────────────────────────
# `KR122D2` 甲：上面 `winchk` 那一格的裁词逐字写着「那 17 处 `cfg(windows)` 没人跨编」，
# 而 `K-R119` 那一趟云端正是红在它上面（daemon job 第 7 步，**10 个编译错全在 test 档**）。
# ⚠ 与 CI 的差别写在 `gate.sh` 那一格的分母里（target `-gnu` vs `-msvc`），这里不抄第二份。
cell(
    "winchk-daemon",
    anchor="run_gate winchk-daemon '不是数出来的数",
    cwd="remote-daemon-proto/",
    cmd="cargo check --all-targets --target x86_64-pc-windows-gnu",
    **{
        "remote-daemon-proto/": (PART, "唯一成员 `cc-monitor-remote` 的**生产段 ＋ test 档**"
                                       "（`--all-targets` 是承重的：云端那 10 个错一个都不在生产段）"
                                       "在 Windows target 上**编得过**。"
                                       "⚠ 只买「编得过」，**买不到「在 Windows 上跑得对」** —— "
                                       "`check` 一行代码都不执行；⚠ `-gnu` 不是 `-msvc`，"
                                       "MSVC ABI 专属的那一类本格盖不到"),
        "src-tauri/": blind("另一个 workspace，由上面 `winchk` 那一格盖"),
        VENDOR: blind("同上 —— 它是 `src-tauri` 那棵的依赖，本命令的编译图里没有它"),
    },
)

cell(
    "cargo",
    # 🔴 〔`K-R115` 09-14〕**这条锚点在本件之前就已经指空了**：它写着 `cargo 8`，
    #   而 `gate.sh` 现打是 `run_gate_sum cargo 9`（workspace 长到 9 个成员那天没人回来改）。
    #   ⇒ 本尺子在**本件动它之前**就红着一条 `C2`（现打读数住
    #   `evidence/K-R115-deathvalue.md#§E`）。这不是本件弄红的，是本件顺手量到的。
    anchor="run_gate_sum cargo 9 bash -c",
    cwd="src-tauri/",
    cmd="cargo test --workspace --exclude code-picture-core --lib",
    **{
        "src-tauri/": (FULL, "9 个成员的 `--lib` 判据，合计求和 + 包数相等断言"
                             "〔09-14 现打：`gate.sh` 那行是 `run_gate_sum cargo 9`；"
                             "上一版这里与锚点都写着 8〕"),
        VENDOR: blind("显式 `--exclude code-picture-core`（`C7`：vendor 不动）"),
        "remote-daemon-proto/": (PART, "**经扫描型守卫读进去**：现打 18 个读点"
                                       "（尺子见本文件 `cross_tree_reads()`）"),
        "scripts/": (PART, "同上，现打 6 个读点（`shared_crate_registry` 读 `gate.sh` 那条最有名）"),
        "doc/": (PART, "同上，现打 9 个读点"),
        "shared/": (PART, "同上，现打 5 个读点"),
        ".github/": (PART, "同上，现打 3 个读点（`capability_registry` 读 `ci.yml`）"),
        "e2e/": (PART, "同上，现打 4 个读点"),
        "src/": blind("前端那棵树归 `npm` 与 `generated` 两格；Rust 判据里没有读点"),
    },
)

cell(
    "generated",
    anchor="git diff --quiet --exit-code -- src/generated/",
    cwd="仓根",
    cmd="git diff --quiet --exit-code -- src/generated/",
    **{
        "src/": (PART, "**只有 `src/generated/`**，而且只判**已跟踪文件的 diff** ——"
                       "全新的生成物是 untracked，本格看不见（那一格归 `generated-boundary-guard`）"),
    },
)

cell(
    "daemon",
    anchor="run_gate daemon '单包 remote-daemon-proto",
    cwd="remote-daemon-proto/",
    cmd="cargo test",
    **{
        "remote-daemon-proto/": (FULL, "单包全量 `cargo test`（含 `#[cfg(test)]` 那一族守卫）"),
        "src-tauri/": (PART, "跨轨对拍：现打 4 个读点（`control/gate.rs` 读 Gate 2 黄金夹具、"
                             "`control/launch.rs` 读 monitor 的 `tmux.rs`）"),
        ".github/": (PART, "现打 1 个读点"),
        "e2e/": (PART, "现打 2 个读点"),
        "doc/": (PART, "现打 5 个读点（协议文档对拍）"),
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
        "src/": (FULL, "`tsconfig.json` 的 `include` 第一项就是这棵树 ⇒ 下面每一份 "
                       "`.ts`/`.tsx`/`.mts` 都进程序，**而且本格自己现打对账**"
                       "（真读进程序的份数 == 盘上现打的份数，两个数同一趟算，一个都不写死）。"
                       "⚠ 买的是**类型**这一层：`tsc --noEmit` 不跑一行代码 ⇒ "
                       "「类型对而行为错」本格一个字都问不出来（那一维归 `npm` 那一格）"),
        "e2e/": (PART, "`include` 的第二项，但这棵树下绝大多数是 `.sh` —— "
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
        "src/": (FULL, "16 个 tsx 套件全在 `src/` 下 + `vitest.config.ts` 的 "
                       "`include: [\"src/**/*.vitest.ts\"]`。⚠ **「跑了 0 个也照绿」那一格 15/17 守不住**"
                       "（本格分母那句话逐字写着）"),
        "src-tauri/": (PART, "现打 2 个读点"),
        "remote-daemon-proto/": (PART, "现打 1 个读点"),
        "scripts/": (PART, "现打 2 个读点"),
        "e2e/": (PART, "现打 1 个读点"),
        "shared/": (PART, "现打 1 个读点"),
    },
)

E2E_NOTE = ("四套 `ccm` e2e 之一。`e2e/` 下的套件今天远不止四套 —— "
            "`ccm-acceptance` / `ccm-pretrust` / `cc-spawn-uplift` 等**都不在这道门里**"
            "（那笔账逐字记在本文件头注引的 `gate.sh` 那一段：一次真行为变更的 71 条红里"
            "「这道门看得见 9 条、看不见 62 条」）")
for suite, anchor in [
    ("ccm e2e/ccm-print-parity", "run_e2e ccm-print-parity 12"),
    ("ccm e2e/ccm-rbind-title", "run_e2e ccm-rbind-title  8"),
    ("ccm e2e/ccm-cli", "run_e2e ccm-cli               46"),
    ("ccm e2e/ccm-contract-parity", "run_e2e ccm-contract-parity   45"),
]:
    cell(
        suite,
        anchor=anchor,
        cwd="仓根",
        cmd="bash e2e/assert-pass-floor.sh <套件> <地板> exact",
        **{
            "e2e/": (PART, E2E_NOTE),
            "remote-daemon-proto/": (PART, "**被测对象是它编出来的二进制** "
                                           "`$CARGO_TARGET_DIR/debug/cc-monitor-remote`"
                                           "（`K-R48` 第二拍起）——买的是行为，不是它的源码"),
        },
    )

cell(
    "pb check",
    anchor='python3 "$HOME/.claude-accts/z/skills/planned-build/bin/pb.py" check',
    cwd="仓根（查的目录在仓外）",
    cmd="pb.py check ../.claude/planned-build/$PB_WS",
    # 12 棵树全「无」——刻意的，见下
    **{t: blind("本格的分母**根本不在本仓**：它查 `../.claude/planned-build/$PB_WS` 那个计划仓") for t in TREES},
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
    ROOTFILES: {
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
               "**依赖**，不是那格的**分母**；登记只记分母，所以这里仍判 0，不是漏登。"
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
             "成员": ["index.html", "vite.config.ts", "tsconfig.json",
                     "eslint.config.js", ".stylelintrc.json"],
             "why": "🔴 **这一档今天 0 格覆盖，而「文档与仓库元数据」那顶帽子对它们是假的**："
                    "`index.html` 是应用入口 · `vite.config.ts` 决定构建产物 · "
                    "`tsconfig.json` / `eslint.config.js` / `.stylelintrc.json` 决定类型与 lint 的口径。"
                    "⇒ 本条**不声称它们不需要门**，只声称**它们已被逐份数到**"},
            {"档": "依赖锁", "判": UNJUDGED, "成员": ["package-lock.json"],
             "why": "它决定 `npm ci` 装出来的是哪一棵依赖树 —— 同样既不是文档也不是仓库元数据。"
                    "0 格覆盖，加不加门另裁"},
            {"档": "仓库元数据", "判": NEED_NONE, "成员": [".gitignore", ".gitattributes"],
             "why": "它们只对 git 自己说话（哪些文件不跟踪 / 换行与 diff 怎么处理），"
                    "不进构建、不进运行时、不被产品代码读"},
            {"档": "文档", "判": NEED_NONE,
             "成员": ["README*.md", "CHANGELOG.md", "LICENSE", "PHASE-G-REPORT.md",
                     "项目审阅报告-*.md"],
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
    markers = {
        "scripts/": "scripts/", ".github/": ".github/", "e2e/": "e2e/", "doc/": "doc/",
        "remote-daemon-proto": "remote-daemon-proto/", "src-tauri": "src-tauri/",
        "shared/": "shared/", "hooks/": "hooks/",
    }
    matrix = {}
    for f in ls_files():
        if not f or f.rsplit(".", 1)[-1] not in ("rs", "ts", "mts", "js", "mjs"):
            continue
        if f.startswith("src-tauri/vendor/"):
            owner = VENDOR
        elif f.startswith("src-tauri/"):
            owner = "src-tauri/"
        elif f.startswith("remote-daemon-proto/"):
            owner = "remote-daemon-proto/"
        elif "/" in f:
            owner = f.split("/")[0] + "/"
        else:
            owner = ROOTFILES
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
    (re.compile(r"^run_e2e (\S+) ", re.M), lambda m: "ccm e2e/" + m.group(1)),
]
# 这两格不走那三个函数，各自手写判定 ⇒ 单独按**逐字锚点**认。
HANDWRITTEN = {
    "generated": "git diff --quiet --exit-code -- src/generated/",
    "pb check": 'fails+=("pb check（没给 PB_WS',
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
    return any((ROOT / f"e2e/{tok}{ext}").exists() for ext in (".sh", ".test.sh", ""))


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
            out.append(f"C5b `gate.sh:{para[0][0]}–{para[-1][0]}` 这一段点名了 `e2e/` 下"
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
    got, want = set(orphans), set(NO_GATE_NEEDED)
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
    """那棵树**现打**的成员。`<仓根文件>` = `git ls-files` 里不含 `/` 的那些。"""
    if tree == ROOTFILES:
        return sorted(p for p in files if "/" not in p)
    return sorted(p for p in files if p.startswith(tree))


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
    for t in orphans:
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

    top = {p.split("/")[0] + "/" for p in ls_files() if "/" in p}
    registered_top = {t for t in TREES if t not in (VENDOR, ROOTFILES)}
    if top != registered_top:
        fails.append(f"C4 树的全集与 `git ls-files` 现打的顶层目录对不上："
                     f"盘上多出 {sorted(top - registered_top)} · 登记里多出 {sorted(registered_top - top)}")
    for t in TREES:
        if t in (ROOTFILES,):
            continue
        if not (ROOT / t).exists():
            fails.append(f"C4 登记里的树 `{t}` 盘上不存在")

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

    print(f"# `K-R80` `KR80D3` 门禁分格覆盖登记 —— 量于 `{GATE}`")
    print()
    print(f"判定格 **{len(cells)}** 个 · 树的全集 **{len(TREES)}** 棵"
          f"（顶层目录 {len(registered_top)} + vendor 单拆 + 仓根文件）")
    print()
    print("| 格 | cwd | 命令 | 全 | 部 | 🔴 盖不到（逐棵点名） |")
    print("|---|---|---|---|---|---|")
    order = [c for c in ("hooks", "copy2", "shellcheck", "ci-e2e-prereq",
                         "fmt", "fmt-daemon", "winchk", "winchk-daemon", "cargo",
                         "deadcode", "generated", "daemon", "tsc", "npm")
             if c in REGISTRY] + sorted(c for c in REGISTRY if c.startswith("ccm e2e/")) + ["pb check"]
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
    print("KR80D3: OK —— C1..C6 全过；C5b 全过（`K-R91` `KR91D1`）；"
          "C5c 全过（`K-R122` `KR122D2` 乙：裁决行那句射程）；"
          "C6b 全过（`K-R82` `KR82D3`）；C6c 全过（`K-R91` `KR91D2`）"
          "（⚠ 它只判「登记完整且指得到真东西」，不判裁词对不对、不判归得对不对）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
