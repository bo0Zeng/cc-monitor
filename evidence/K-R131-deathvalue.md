# `K-R131` 死值验 —— `R11`（收工目标的算术闭合）

> **被测量具**：`evidence/K-R117-ruler.py`。**被测对象**：每一版**各自一份新副本**（`brief` 12c）。
> **量于**：09-15，真树 HEAD `9c9c68b`。**全部在沙箱里跑，宿主上零次。**

## §0 沙箱怎么跑的 —— **如实报一条已知缺口**

`K31` 唯一许可命令是 `.claude/devbox/gate`，而它最后一行逐字写死：

```
  bash -o pipefail -c 'mkdir -p "$HOME/.claude/projects" && bash scripts/gate.sh'
```

⇒ **跑不了单把量具**。缺口已立 `K-R130`。本件的办法是**同镜像 · 同挂载 · 同 `--network none` ·
同 `-e` 集合 · 同 `-w`、只换最后那条命令** —— 跑器逐字：

```bash
#!/usr/bin/env bash
set -o pipefail
WT="${1:?用法: kr131-sbx.sh <工作树> <命令>}"; shift
PROJ=/home/zbl/文档/claudecode-frontend
SKILL=/home/zbl/.claude-accts/z/skills/planned-build
TAG=$(basename "$WT")
mkdir -p "$PROJ/.claude/pm-targets/$TAG"
exec docker run --rm --network none \
  -v "$PROJ:$PROJ" -v "$SKILL:$SKILL:ro" \
  -v "ccmon-cargo-registry:/opt/rust/cargo/registry" \
  -e "CARGO_TARGET_DIR=$PROJ/.claude/pm-targets/$TAG" \
  -e HOME=/home/zbl -e PB_WS -e GATE_DIAG_KEY -e GATE_DIAG_TAIL -e GATE_DIAG_WHOLE \
  -w "$WT" ccmon-devbox:latest \
  bash -o pipefail -c "mkdir -p \"\$HOME/.claude/projects\" && $*"
```

**与 `gate` 的差集只有最后那条命令**（挂载 · 网络 · 隔离档逐条相同）。镜像里没有 PyYAML，本件没用到。

## §1 夹具住址 —— **每一版一份新副本，`.git` 已删**

夹具根（**只属于 `K-R131`**，别的 agent 不共用这个名字）：
`/home/zbl/文档/claudecode-frontend/.claude/pm-targets/kr131-fixtures/`

子目录名一律**中性**（`brief` 12：断言用的子串不许取自夹具的名字 —— 本件所有断言都不含路径）：

| 夹具 | 它是哪一版 | 被测对象指向哪棵树 |
|---|---|---|
| `alpha` | **未变异对照**（证明副本本身的分母与真树相同） | 它自己 |
| `bravo` | **刀①a**：把 S4 的目标**改成**一个不在全盘名单里的文件 | 它自己 |
| `delta` | **刀①b（最小面）**：只**追加**一份不在名单里的目标，别处一个字节不动 | 它自己 |
| `charlie` | **刀②（阴性对照）**：拿掉 `R11a` 那一格判据 ＋ 叠加刀①a | 它自己 |
| `echo` | **把实现整个退掉**：`section_frontend_goal_closure()` 从 `main` 摘掉 | 它自己 |

每份副本都是 `cp -a src src-tauri evidence/K-R117-ruler.py` 之后 `find -name .git -exec rm -rf`；
**现打每份 `.git` 计数 = 0**（`brief` 12c：工作树的 `.git` 是一行指回原仓的指针，
在副本里跑 `git` 会写进**原树**的暂存区）。

## §2 判定行怎么核的 —— **`SELFTEST_BAR_RE` 够不着这把量具，如实说**

`brief` 第 8 条要求「用 `SELFTEST_BAR_RE` 那个口径核判定行，别自创正则」。
**现打它够不着**：`SELFTEST_BAR_RE = re.compile(r"^  (?:ok   |FAIL )(?!\s)")`
（住 `bin/vocab/judge.py:90`），射程是 **`pb check` 的自检条**；
拿它去扫本量具七版输出，**每一版命中都是 0**（已实测，见下表最后一栏）。
⇒ 本件核的是这把量具**自己的**判定行：`installface: N passed`
（`N = len(PASSED)` 现算；`run_gate` 也是靠它认「这一格真的跑了」，`0 passed 不是绿`）。
★ 这正是本仓最高频的那一类（「量具的作用域对不上事实」）—— **报出来，不冒充。**

## §3 变异表（逐行真实输出）

| # | 夹具 | 切在哪个函数 / 哪一处 | 锚点命中 | `installface` | RED 条数 | 逐条 RED 标签 | `RULER` | 退出码 | `SELFTEST_BAR_RE` |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 真树（入场） | 未变异 | — | **61** | 0 | — | OK | 0 | 0 |
| 1 | 真树（交回） | 未变异 | — | **68** | 0 | — | OK | 0 | 0 |
| 2 | `alpha` | 未变异（副本对照） | — | **68** | 0 | — | OK | 0 | 0 |
| 3 | `bravo` | 模块级常量 `FRONTEND_GOAL_PER_ITEM`，`"S4"` 那一行 | **1**（期望 1） | **66** | **2** | `R11a` · `R11a` | FAIL | 1 | 0 |
| 4 | `delta` | 模块级常量 `FRONTEND_GOAL_PER_ITEM`，`"S2"` 那一行 | **1**（期望 1） | **67** | **1** | `R11a` | FAIL | 1 | 0 |
| 5 | `charlie` | 函数 `section_frontend_goal_closure`（摘掉 `R11a` 两处 `red()`）＋ `"S4"` 那一行 | **1** ＋ **1** | **66** | **0** | — | OK | 0 | 0 |
| 6 | `echo` | 函数 `main`（摘掉 `section_frontend_goal_closure()` 那一行） | **1**（期望 1） | **61** | 0 | — | OK | 0 | 0 |

**分母怎么数的**：
- `installface` 的分母 = `§S5c`/`§S5d`/`§S5e`/`§S5f` 四节逐条记下的「过了」事件数，`len(PASSED)` 现算。
  ⚠ `R1`–`R7` 那七条**不在这个数里**（它们只在红的时候出声）。
- `RED 条数` 的分母 = 输出里 `^  RED \[` 逐行数（正则现打，不是眼数）。
- 每一行的「锚点命中」都是**切之前**断言出来的，不等于期望就不切（脚本 `assert`）。

**逐行读法**：

- **#3 刀①a**（`KR131D3` ①「把某一组的目标改成一个不在全盘名单里的文件 ⇒ 必须红」）：
  红 **2 条，全部是 `R11a`，两向各一条** —— 逐字：
  `RED [R11a] 这几份**是某一组的目标、却不在全盘目标名单里**：src/settings/data-section.ts …`
  `RED [R11a] 全盘目标里这几处**没有任何一组认领**：src/launcher-diagnostics.ts …`
  「改成」是**替换**⇒ 两向同时失衡，两条红都是该响的。判定行 68→66：少的两条正是
  `R11a` 那条 `ok` ＋ `launcher-diagnostics.ts` 那条「两侧都在」。**没有别的格被打红。**
- **#4 刀①b（最小面，`brief` 第 9 条）**：只**追加**一份，不拿走任何一份 ⇒ **只红 1 条，只有组→全盘那一向**。
  判定行 68→67（只少 `R11a` 那条 `ok`）。**证明那一格的牙是它自己的，不是粗刀带出来的。**
- **#5 刀②（阴性对照）**：把 `R11a` 那一格整格拿掉再叠加刀①a ⇒ **0 红、退出码 0**。
  ⇒ #3 的红**确实是 `R11a` 给的**，不是别处顺带。判定行 66（与 #3 同）⇒ 判定行没塌、不是 CRASH。
- **#6 把实现整个退掉**：`installface` 回到 **61**，与入场逐字相同 ⇒
  **新增的 7 条断言一条都不剩**（68 − 61 = 7 = `R11a`/`R11b`/`R11c`/`R11d` 各 1 ＋
  三份目标文件各 1 条「两侧都在」）。**没有一条是仪式性的**。
- **假红方向**（`KR131D3` 明令）：#1 与 #2 —— 按裁定正常填一遍，**一次都没红**。

**CRASH：0 次**（七版判定行分别是 61/68/68/66/67/66/61，**没有一版掉到 0**，没有异常退出）。

## §4 这把判据买不到什么（`references/testing.md` 判据硬规则 10）

- `R11` **只读本文件自己的三张表，一行被测树都不读** ⇒ 它**够不着**「目标定得对不对」，
  也**不会**因为产品代码变了而红。它买到的只有「那四条算术关系没人能悄悄写歪」。
  （已写进量具头注诚实边界 `B9`。）
- 它也**不判「用户看到几处」** —— 见头注 `B10`：`buildAccountAliasBlock` 现打渲染在两处
  而只算一份落点；`src/ccm-probe.ts` 界面上零处却算一份落点。
- 🔴 **一处自证，抬进上报口（`brief` 15：写在注释里等于埋掉）**：裁决那一行的分母串
  「`§S5c`/`§S5d`/`§S5e`/`§S5f` 四节」是**手写的字面量**，不是现算。
  `echo` 那一版已经把它逮到了：`§S5f` 被摘掉之后那句话仍写着「四节」⇒ **它会说谎**。
  本件**没有**修它（要修得给节名建一个登记表，属另一件的面），**记在这里**。
