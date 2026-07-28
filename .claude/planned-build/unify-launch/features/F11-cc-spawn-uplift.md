# 功能计划 — F11 预信任能力上提进 ccm（cc-spawn 并入 ccm，本轮范围收窄）

> 对应主计划 §1 的 F11。本文件是该功能从规划到签收的全程记录。
> **动手前先读 MASTERPLAN §0 核心思想**——推论④「向下兼容 = 少传几个参数，不是第二条代码
> 路径」是本功能的直接体现：`ccm --tmux` 今天已经是"起 tmux 会话"的唯一实现，`cc-spawn` 是
> 仍然独立存在的**第三套**实现（第一套是本地 PowerShell/F06，第二套是终端旧 4-block/F02 已
> 收编）——本功能把它剩下的、真正有价值的能力（预信任）搬进 `ccm`，而不是让 `cc-spawn` 继续
> 自己维护一份重复逻辑。

## 0. 本计划的来源（Phase B 方法论说明）

开了一个 Explore fork 直接读 `cc-spawn`（136 行）+ `shared/ccm` 全文，摸清两者的重复面与真实
缺口——**未开 Plan agent fanout**，理由：改动范围本身没有架构分歧（"把 A 脚本的一段逻辑原样
搬到 B 脚本"不是需要比较方案的开放设计问题），唯一需要认真决策的是**范围边界**，见下。

**Explore fork 的关键证据 + 本席核实**：

- `cc-spawn` 实际路径是 `~/.claude/skills/cc-bus/scripts/cc-spawn`（**不在本仓库里**，已用
  `ls`/`wc -l` 核实：7699 字节、136 行、属于 cc-bus skill 这个独立的用户级基础设施，被这台机器
  上所有 Claude Code 会话共用，不是 cc-monitor 专属）。
- `cc-spawn` 的预信任写入（已通读全文核实）：
  - `claude`：读/写 `~/.claude.json`（`CCSPAWN_CLAUDEJSON` 覆盖，测试钩子）—— `jq` 检查
    `.projects[$absdir].hasTrustDialogAccepted == true`，已信任则跳过；否则 `jq` 写入该字段，
    **写前 `cp -p` 备份、写后校验 JSON 合法再 `mv` 落地、失败则清理临时文件不动原文件**（原子、
    fail-safe）。
  - `codex`：读/写 `~/.codex/config.toml`（`CCSPAWN_CODEXTOML` 覆盖）——`flock` 加锁，`grep`
    检查该目录的 `[projects."<路径>"]` stanza 是否已存在，不存在才追加
    `trust_level = "trusted"`；**路径含控制字符时跳过预信任**（避免写出破损 TOML）；路径含
    `"`/`\` 时正确转义成 TOML basic-string key。
  - 两者都有失败兜底：预信任没成功 → 起会话后轮询 6×0.5s 抓 pane 文本找"Yes, I trust this
    folder"，找到就发 Enter（screen-scrape 兜底，不是主路径）。
- `shared/ccm` 核实：`agent_needs_bus_id()`（`shared/ccm:109`）+ `CC_BUS_ID` 导出
  （`shared/ccm:399-401`）与 `cc-spawn` 的 codex 注入**功能等价、确实重复**（`cc-spawn` 显式
  传会话名，`ccm` 用 `tmux display-message -p '#S'` 自己查——效果相同）。`grep` 全文
  "claude.json"/"codex/config.toml"/"trust" **零命中**——`ccm` 今天完全没有预信任能力。
- **紧迫性核实**（Explore fork 明确标注这是推断非实锤，本席认同这条谨慎表述）：MASTERPLAN 里
  R10 调研提到的"claude 卡信任确认页数小时"是在 `cc-spawn` 场景下观察到的（cc-spawn 派生的
  目录通常是全新目录，从未被任何 CLI 访问过）；`ccm --tmux` 目前**没有独立证实**发生过同样的
  卡死——`ccm` 常见场景是 resume 已有会话（目录早被访问过，天然已信任）或用户主动"开新
  Claude"（目录可能也是新的，理论上会撞同一个坑，但没有已发作的真实报告）。本功能仍然做，
  理由是"防患于未然 + 消灭重复代码"两条都成立，但**不夸大成"已确认的紧急 bug"**。

## 1. 目标与验收标准（DoD）

- **目标**：把 `cc-spawn` 的预信任写入逻辑（claude 的 `~/.claude.json` / codex 的
  `~/.codex/config.toml`）原样搬进 `shared/ccm` 的 `--tmux` 建会话路径，让 `ccm --tmux` 起的
  全新会话也享受"不卡信任确认框"的好处——**不改 `cc-spawn` 本体**（见下方范围边界）。

- **范围边界（本次关键决策）**：**只做"上提进 `ccm`"这一半，不做"改造 `cc-spawn` 去调用
  `ccm`"这一半**。理由：
  1. `cc-spawn` 物理上不在这个仓库（`~/.claude/skills/cc-bus/scripts/cc-spawn`）——它是
     cc-bus 这个独立、用户级、跨项目共享的基础设施的一部分，不是 cc-monitor 的代码。
  2. 用户的"全部功能自动做"授权范围是这个 planned-build 工作区（`.claude/planned-build/
     unify-launch/`），对应的是 cc-monitor 这个仓库——**不延伸到自动改写仓库之外、影响这台
     机器上所有 CC 会话的共享基础设施**。这类改动即便技术上正确，也应该走独立的、用户明确
     知情的授权，不该被"功能自动做"这句话隐式覆盖。
  3. 技术上也不成比例：`cc-spawn` 今天工作正常（没有已发作的 bug），先做"消灭 `ccm` 自身的
     能力缺口"这一半已经能拿到"新建会话不卡信任框"的核心收益；"改 `cc-spawn` 去调 `ccm`"是
     纯粹的代码去重，收益小、风险是动了一个跨项目共享脚本。
  - **明确留给未来、需要用户另行拍板的后续项**（不在本功能验收范围内，只记录不实现）：
    `cc-spawn` 的建会话/送环境/送任务这三步改经调用 `ccm`，只保留 `cc-register`/
    `spawned.tsv`/复用判定这些 cc-bus 专属部分——账本已经写好了目标形态（MASTERPLAN 共享面
    账本 `~/.local/bin/cc-spawn` 那一行），留着给下一次用户明确要求"现在去改 cc-spawn"时用。

- **验收标准**：
  - [x] `shared/ccm`：在 `--tmux` 建会话分支（`new-session` 之前，即 §0 读到的
        `shared/ccm:340-377` 附近）新增预信任逻辑：
    - `agent`（即 `$agent` 变量）为 `claude` 时：对 `$cwd`（已解析的绝对路径）执行与
      `cc-spawn` 完全相同的 `~/.claude.json` 读写逻辑（`jq` 检查 → 备份 → 写 → 校验 → `mv`
      落地 / 失败清理），**照抄不重新设计**——这段逻辑已经过 `cc-spawn` 的生产使用验证，
      重新发明只会引入新 bug。测试钩子沿用同一命名习惯：`CCM_CLAUDEJSON` 覆盖
      `~/.claude.json`（对齐 `CCSPAWN_CLAUDEJSON` 的既有命名模式）。
    - `agent` 为 `codex` 时：对 `$cwd` 执行与 `cc-spawn` 相同的 `~/.codex/config.toml`
      读写逻辑（`flock`/幂等 grep/控制字符跳过/TOML 转义），测试钩子 `CCM_CODEXTOML`。
    - 其余 `agent` 值（未来可能的第三个 agent）：不尝试预信任，恒不新增行为——这是 `ccm`
      今天的现状，本功能只加两个已知分支，不是回归。
  - [x] 预信任失败不阻断建会话——同 `cc-spawn` 的"失败即打印 stderr 警告，继续走既有流程"
        语义，`ccm` 不能因为一次 `jq`/`flock` 失败就拒绝起会话。
  - [x] 新增 e2e 测试（沿用 `e2e/ccm-cli.test.sh` 的既有真机验收风格，`CCM_CLAUDEJSON`/
        `CCM_CODEXTOML` 指向隔离的临时文件）：
    - claude：全新 `~/.claude.json`（模拟未信任）→ `ccm --tmux --agent claude` 建会话后，
      文件里该目录的 `hasTrustDialogAccepted` 确实变 `true`；已信任的目录 → 文件不被
      不必要地重写（幂等）。
    - codex：全新 `~/.codex/config.toml` → 建会话后含该目录的 `trust_level = "trusted"`
      stanza；已有 stanza 的目录不重复追加（幂等、TOML 仍合法）；路径含 `"`/`\` 正确转义；
      路径含控制字符时确认**跳过**且原文件不受影响。
    - 预信任写入本身失败（如目标文件只读）→ `ccm` 仍能正常建会话（不因预信任失败而整体失败）。
  - [x] `doc/INVARIANTS.md` 或 `shared/ccm` 内联注释：记一条"预信任逻辑与 `cc-spawn` 保持
        同源"——若未来 `cc-spawn` 那边修了这段逻辑的 bug（如 TOML 转义漏了什么字符），`ccm`
        这边的对应实现也要跟着补，两处目前是**有意的代码重复**（不同仓库，无法共享一份源），
        不是各自独立维护也不用管。
  - [x] 门禁：`test:ccm-cli`（新增用例）/`test:ccm-acceptance`/`test:ccm-print-parity`/
        `test:tmux-target`/`test:tmux-guarded`/`resume-suite`/`restart-suite` 全绿；
        `tsc`/`npm test`/`cargo test` 全绿（本功能只碰 `shared/ccm`，TS/Rust 侧预期零受影响，
        因此不需要 TS/Rust 侧新测试）。

- **明确不做什么**：
  - **不改 `cc-spawn` 本体**——见上方范围边界，留给用户明确要求时再做。
  - **不给 `ccm` 的非容器（直连/无 tmux）路径加预信任**——预信任要解决的是"detached 会话没人
    盯着信任框"这个问题；直连路径用户就在终端里，信任框自己按一下即可，价值远低于 tmux 路径，
    不值得为此再复制一份预信任逻辑到第二个代码位置。
  - **不把 `CC_BUS_ID`/`agent_needs_bus_id` 的重复实现去重**——`ccm` 与 `cc-spawn` 各自的
    codex 身份注入虽然功能等价，但分属两个仓库，去重需要抽公共库或让一方调用另一方，属于
    "改 cc-spawn 本体"的范畴，同上方理由不在本轮做。
  - **不新增任何 UI**——这是纯 CLI/后端能力，cc-monitor 前端不需要感知这条变化（预信任对
    用户来说只是"起会话时少弹一次信任框"，不需要任何界面呈现）。

## 2. 与主计划的对接 + 关键决策（附理由）

**触及的共享面**：仅 `shared/ccm`（+ 新增 e2e 测试脚本/用例）。**不触及**任何 TS/Rust 代码——
本功能与 unify-launch 的 IR/维度注册表无关，纯粹是 `ccm` 自身 shell 脚本能力的补强。

**一处关键决策**（范围边界，已在 §1 详述）：本次只做"上提进 `ccm`"，不动 `cc-spawn` 本体。
这是本功能规划阶段做出的、经过深思的授权边界判断——不是回避工作量，是"改一个跨项目共享的
基础设施脚本"这件事本身需要用户明确知情，不应隐式归入"全部功能自动做"这句授权的默认范围。

## 3. 接口 / 契约设计

### 3.1 `shared/ccm`：`--tmux` 路径新增预信任（照抄 `cc-spawn` 的逻辑，改用 `$cwd`/`$agent`）

```bash
# 预信任（原样搬自 cc-spawn，测试钩子改名 CCM_CLAUDEJSON/CCM_CODEXTOML）
if [ "$agent" = claude ]; then
  cj="${CCM_CLAUDEJSON:-$HOME/.claude.json}"
  if [ -f "$cj" ] && command -v jq >/dev/null 2>&1; then
    if ! jq -e --arg d "$cwd" '.projects[$d].hasTrustDialogAccepted == true' "$cj" >/dev/null 2>&1; then
      cp -p "$cj" "$cj.bak-ccm.$$" 2>/dev/null || true
      tmpj="$cj.tmp.ccm.$$"
      if jq --arg d "$cwd" '(.projects[$d].hasTrustDialogAccepted) = true' "$cj" > "$tmpj" 2>/dev/null \
         && jq -e . "$tmpj" >/dev/null 2>&1; then
        mv "$tmpj" "$cj"; rm -f "$cj.bak-ccm.$$"
      else
        rm -f "$tmpj"
      fi
    fi
  fi
elif [ "$agent" = codex ]; then
  ct="${CCM_CODEXTOML:-$HOME/.codex/config.toml}"
  mkdir -p "$(dirname "$ct")" 2>/dev/null || true
  [ -f "$ct" ] || : > "$ct"
  if [[ "$cwd" != *[[:cntrl:]]* ]]; then
    cesc=${cwd//\\/\\\\}; cesc=${cesc//\"/\\\"}
    ( flock 9
      if ! grep -qF "[projects.\"$cesc\"]" "$ct" 2>/dev/null; then
        cp -p "$ct" "$ct.bak-ccm.$$" 2>/dev/null || true
        if printf '\n[projects."%s"]\ntrust_level = "trusted"\n' "$cesc" >> "$ct" 2>/dev/null; then
          rm -f "$ct.bak-ccm.$$"
        else
          [ -f "$ct.bak-ccm.$$" ] && mv -f "$ct.bak-ccm.$$" "$ct"
        fi
      fi
    ) 9>>"$ct.lock"
  fi
fi
```
落点：紧挨着 `--tmux` 分支已解析好 `$cwd`/`$agent` 之后、`tmux new-session`（`shared/ccm:367`
附近）之前——不能更早（`cwd` 还没解析成绝对路径）也不能更晚（会话已经起来再补信任没有意义，
信任框在 `send-keys` 送进去的那条命令实际执行时才会弹出）。

**Phase D 审计再修正**（双 agent 审各自独立发现 + 复现，以下均已修复，实际代码见
`shared/ccm` 内 `# F11（unify-launch）：预信任` 起始的代码块，不在此重复贴全文）：

1. **【阻塞，UX agent 复现】`$cwd` 不保证绝对路径**——`resolve_cwd()` 对 `--cwd <非 auto 值>`
   原样透传，不像 cc-spawn 的 `$absdir` 恒经 `cd && pwd` 规范化。直接拿 `$cwd` 当 JSON/TOML
   key，在 `--cwd <相对路径>` 场景下会写出字面量 `"proj1"` 这种野 key——对真实信任表毫无
   意义（Claude/Codex 用真绝对路径查）、还会往用户真实配置文件永久写入垃圾条目，且 `jq -e .`
   校验照样通过、不报任何错。**已实测复现**。修法：预信任专用 `cwd_abs="$(cd "$cwd" 2>/dev/null
   && pwd)"`，解析失败（目录已消失等极端情况）则放弃预信任而非拿一个可能错误的相对路径瞎写。
2. **【阻塞，UX agent 发现】完全丢失了 cc-spawn 真正的安全网**——cc-spawn 用 `pretrusted`
   变量追踪写入是否成功，写入没成功时（**任何**原因：`jq` 缺失、文件不存在、写入失败）会
   轮询 6×0.5s 抓 pane 文本找"Yes, I trust this folder"、自动按 Enter；本次移植只搬了
   "写入"，没搬这层兜底。而 cc-monitor 前端对**本地**（非 SSH）tmux 会话完全没有画面预览
   能力（`capture-pane` 预览只对远端 origin 开放）——没有这层兜底，detached 会话的信任框会
   静默卡死且没有任何人能发现。已补齐 `pretrusted` 变量追踪 + 对应轮询兜底（嵌进 `seq`
   字符串，`send-keys` 之后、`attach` 之前，只对 `claude` 做，同 cc-spawn 范围），并用一个
   会真的打印信任框文本 + 等 `read` 的假 launcher 端到端验证了轮询确实检测到文本、确实发了
   Enter、确实让等待中的进程收到（不只是语法层面正确）。
3. **【重要，UX agent】计划自己承诺的 stderr 诊断没兑现**——§1 验收标准写了"失败即打印 stderr
   警告"，实现最初漏了这三处（同 cc-spawn 的三条诊断文案）——已补齐。
4. **【重要，UX agent】受众变宽后无退出口不合适**——`cc-spawn` 是窄众协作工具，`ccm --tmux`
   是 cc-monitor 主 UI 的默认统一实现，受众判断不同。已加 `CCM_NO_PRETRUST=1` 环境变量
   逃生口（同时关闭持久化写入**和**轮询自动按 Enter 两条路径——选择不自动管理信任的用户
   不该被换了个机制的路径悄悄绕过），并写进 `--help` 用法文本。
5. **【阻塞，后端 agent 独立复现】F11 让既有 `e2e/ccm-acceptance.sh` 污染开发机真实全局配置**
   ——该脚本每个场景默认 `--agent claude`，本次落地前从未设置 `CCM_CLAUDEJSON`/
   `CCM_CODEXTOML`（F11 之前 `--tmux` 没有任何写真实文件的副作用，该脚本"纯净沙盒"的
   既有假设本就成立；F11 落地后这个假设被打破）。**已实测复现两次**（连跑两次会分别在
   本机真实 `~/.claude.json`/`~/.codex/config.toml` 里新增不同的 `/tmp/tmp.XXXXXXXX` 垃圾
   trust 条目）——已清理污染 + 给 `ccm-acceptance.sh` 补上同款隔离（`CCM_CLAUDEJSON`/
   `CCM_CODEXTOML` 指向本次隔离 `$TMP` 下的副本），复测确认真实文件不再受影响。这不是 F11
   代码本身的 bug，是 F11 改变 `ccm --tmux` 默认行为后遗留给一个"假设自己是纯净沙盒"的
   既有测试文件的真实回归——签收前必须堵上，否则每次门禁/CI 跑动都会再次静默写坏用户真实
   配置。
6. **【建议，后端 agent】`$agent` 的 `else` 兜底在当前代码里实际不可达**——`shared/ccm` 参数
   解析阶段（`case "$agent" in claude|codex) ;; *) die ... esac`）已经在更早处硬拒绝任何
   非 `claude`/`codex` 的值，F11 的 `if/elif`（无 `else`）永远不会被"未知第三个 agent"值
   触达。不是 bug，只是这条防御性表述目前是前瞻声明而非当下真会走到的路径，记录以防未来
   误判需要为此加代码。

## 4. 实现步骤（严格顺序执行）

- [x] **步骤 1**：在 `shared/ccm` 的 `--tmux` 分支里加上述预信任代码块（§3.1），紧邻
      `tmux new-session` 之前。
      — 验证：`bash -n shared/ccm` 语法检查通过；手动跑一次 `ccm --tmux --agent claude
      --print`（不实际建会话，只看 `--print` 输出不受影响——预信任只是副作用，不改变吐出的
      命令串本身）确认 `--print` 路径没被意外触发预信任（预信任只应该在真建会话的分支里）。
- [x] **步骤 2**：新增 e2e 测试用例（`e2e/ccm-cli.test.sh` 或新建同风格脚本），覆盖 §1 验收
      标准列出的场景（claude 未信任/已信任、codex 未信任/已信任/特殊字符转义/控制字符
      路径跳过、预信任写入失败不阻断建会话）——Phase D 审计后扩到 9 个（新增：相对 `--cwd`
      规范化回归、`CCM_NO_PRETRUST` opt-out、真失败场景、轮询兜底端到端验证）。
      — 验证：新脚本/新用例本身可独立跑通，隔离用临时目录+`CCM_CLAUDEJSON`/`CCM_CODEXTOML`，
      不碰真实 `~/.claude.json`/`~/.codex/config.toml`。
- [x] **步骤 3**：`shared/ccm` 内联注释记录"与 `cc-spawn` 保持同源"的维护提示（§1 验收标准
      倒数第二条）。
- [x] **步骤 4**：双 agent 审（后端架构 + UX，prompt 自包含带 MASTERPLAN §0 全文；UX agent
      重点核对范围边界决策——"只上提不改 cc-spawn"是否合理，以及预信任逻辑照抄是否真的
      逐字节对齐 cc-spawn 原文，没有引入转写错误）。
- [x] **步骤 5**：MASTERPLAN §1/§3/§7 更新；全量门禁；commit（仅 `shared/ccm` + 新 e2e 测试，
      不touch cc-spawn）。

## 5. 测试策略

- **逐字节对拍**：新写的 `ccm` 预信任代码块与 `cc-spawn` 原文逐行比对，除了变量名（`$absdir`→
  `$cwd`，测试钩子改名）和文件路径变量前缀（`CCSPAWN_`→`CCM_`）之外，锁存储/校验/回滚逻辑
  完全一致——这是"照抄经过生产验证的逻辑，不重新发明"这条设计原则的落地。
- **真机验收**：本功能改的是会实际写文件系统（`~/.claude.json`/`~/.codex/config.toml`）的
  shell 逻辑，必须有真实执行的 e2e 测试（不能只是字符串断言）——`jq`/`flock`/TOML 转义这些
  都是真实可能出错的地方，同 `cc-spawn` 自己用 `CCSPAWN_CLAUDEJSON`/`CCSPAWN_CODEXTOML`
  测试钩子隔离的既有验证模式。
- **回归**：`test:ccm-cli`/`test:ccm-print-parity` 既有断言零改动全绿——本功能对这两者是纯
  增量，不改变任何既有命令的输出。**`test:ccm-acceptance` 例外**——Phase D 审计发现并复现
  该脚本本身需要补隔离（见 §3.1"Phase D 审计再修正"第5条），已修，其 15 个既有断言内容
  本身零改动，只是新增了两行环境变量隔离前缀。

## 6. 代码审计结果（Phase D）

两个独立 agent 并行审（prompt 自包含，各带 MASTERPLAN §0 核心思想全文），**均发现真实阻塞项**
（本功能是这条 unify-launch 主线里，双 agent 各自独立复现出可实测 bug 数量最多的一次——不是
巧合，是这个功能本身触及"写用户真实全局配置文件"这个此前从未有功能碰过的高风险面，理应受到
更严格的审视）。

**UX/风险 agent**：发现 2 条阻塞 + 2 条重要，均已修复（详见 §3.1"Phase D 审计再修正"1-4）：
① `--cwd <相对路径>` 场景下 `$cwd` 非绝对路径导致预信任写出野 key，已实测复现，修法是专用
`cwd_abs` 规范化；② 完全丢失 cc-spawn 的 screen-scrape 轮询兜底，且 cc-monitor 前端对本地
tmux 会话零可见性——独立核实了 `capture-pane` 预览确实只对远端 origin 开放，本地 detached
会话卡死没人能发现，已补齐 `pretrusted` 追踪 + 轮询兜底，并用真实打印信任框文本的假 launcher
端到端验证轮询确实生效（不只是语法正确）；③ 计划承诺的 stderr 诊断未兑现，已补齐；④ 受众
从 cc-spawn 的窄众协作工具变成 ccm 这个主 UI 默认路径后无退出口不合适，已加 `CCM_NO_PRETRUST`
opt-out（同时关闭写入和轮询两条路径）。也发现场景 6（chmod 400）实际不测真失败——同后端
agent 独立复现的结论一致，已订正测试标题为诚实描述、并补场景 7 测真失败。

**后端架构 agent**：核实了预信任逻辑与 cc-spawn 原文逐行对齐（除变量名/文件后缀前缀外完全
一致）、`--print` 路径确实零副作用（真实重跑确认）、`$agent` 的 `else` 兜底在当前参数解析
（更早已 `die` 拒绝未知 agent 值）下实际不可达（记录为前瞻声明非当下路径，非 bug）。**独立
发现并复现了一条本功能自身未曾预料的阻塞项**：本次改动让**既有**（非本功能新增）的
`e2e/ccm-acceptance.sh` 从"纯净沙盒"变成会真实污染开发机全局配置的测试——该脚本默认
`--agent claude`、此前从未设置隔离钩子，是因为 F11 之前 `--tmux` 从无写真实文件的副作用，
这个假设本就成立；F11 落地后假设被打破却没人去检查这个既有文件。**已实测复现两次**（分别
新增了两条不同的 `/tmp/tmp.XXXXXXXX` 垃圾 trust 条目），已清理本机真实文件的污染并给该脚本
补齐同款隔离，复测确认真实文件不再受影响。审计过程中还观察到一个巧合：审计 agent 自己重跑
测试时也触发了同一污染（其报告里详细记录了它自己复现+清理的过程），侧面印证这条阻塞项的
复现率是确定性的（100%，不是偶发）。「建议」三条：`CCM_NO_PRETRUST` 与 cc-spawn 的有意分歧
已在维护提示里补充说明；两边预信任失败路径都不清理孤儿 `.bak-*.$$` 备份文件（继承自
cc-spawn 的既有行为，危害小，记录不处理）；范围边界决策（只上提不改 cc-spawn）经独立评估
判定合理，不对称收益不成立，把决定权留给用户是恰当的保守选择。

## 7. 工程审计结果（Phase E）

主线程对账（读 MASTERPLAN §0/§0.1/§3 + 本功能计划）：F11 落地后主计划仍自洽——本功能只碰
`shared/ccm`（+ 两个 e2e 测试脚本），不触及 unify-launch 的 IR/维度注册表/TS/Rust 代码，与
主线正交。范围边界决策（只上提进 `ccm`、不改仓库外的 `cc-spawn` 本体）经两位独立审计 agent
（一位在 Phase D 报告里明确给出独立判断"任何触碰它的改动一旦引入 bug，爆炸半径是这台机器上
所有 Claude Code 会话"）与主线程本次工程审计三方一致认可，不是自说自话的合理化。

**这一轮审计本身验证了"高风险面理应受到更严格审视"这条工程直觉**：本功能是 unify-launch
迄今唯一一个"写用户真实全局配置文件（而非 cc-monitor 自己的 config.json）"的功能，双 agent
各自独立发现并复现的阻塞项数量（2+1，全部可实测复现，无一条是纯推测）明显高于此前任何一个
F0x 功能——这不是审计质量的偶然波动，是这个功能触及的风险面客观上更高（真实文件系统副作用 +
影响面超出 cc-monitor 自身）。**账本预见的重叠现在优雅处理**：F11 让既有 `e2e/ccm-acceptance.sh`
的"纯净沙盒"假设失效这件事，是本次工程审计里最值得记取的一类教训——任何未来功能只要让
`ccm --tmux` 新增一个会触碰真实文件系统/外部状态的副作用，都必须回头核对这份既有测试文件
是否也需要跟着隔离，不能假设"我没碰这个文件，它就不受影响"。已在 §3.1 相应条目里写清楚这个
教训，供 F08（同样会改 `shared/ccm`，含 `--model` 支持）落地时对照检查。

未发现任何会拖累后续功能的新耦合/技术债。两位审计的全部阻塞项 + 重要项均已就地修复，无遗留。

## 8. 签收（Sign-off）

- [x] 通过代码审计
- [x] 通过双 agent 架构/UX 审（含范围边界决策的合理性核对）
- [x] 通过工程审计
- [x] 主计划已据此更新
