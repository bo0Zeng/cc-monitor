# 状态 / STATUS — gate-integrity（恢复工作的入口，每次先读这里）

- **当前阶段**：**✅ 本区三个功能全部完成**（G-A / G-B / G-C 均已签收）。<br>（2026-08-01 订正：本行原写「C-F 实现中」，与紧接着的下一行自相矛盾。）
- **当前功能**：无（G-C 刚签收 —— **本区三个功能全部完成**）
- **当前步骤**：n/a
- **已完成功能**：**G-B**（vendored bash 进门禁，`0b297ed`）· **G-A**（八套真机套件断言数地板，
  2026-07-30）—— 见 `features/G-B-vendored-bash-gates.md` / `features/G-A-e2e-assertion-floors.md`
- **下一个功能**：**无（本区完成）** → 回 README 表走 #14 local-as-remote
- **G-C 签收（2026-07-30）**：见 `features/G-C-e2e-suites-into-ci.md`。**BACKLOG E41 已销**。
  遗留一条：`e2e/README.md` 要补「这 6 套不进 `npm test` 的代价」一句（开放问题 1 的尾巴）。
- **G-C 开工前的实测（2026-07-30 已测，下轮别重测）**：
  - **计划说「6 套」，实际是 7 个文件**：`graylight-suite.sh`(tmux 12) ·
    `graylight-daemon-frames.sh`(24) · `restart-suite.sh`(8) · `restart-daemon-frames.sh`(8) ·
    `resume-suite.sh`(15) · `resume-daemon-frames.sh`(17) · `gen-idle-tmux.sh`(7)。
    另有 `f40-suite.sh` 也没有 npm script，**开工先定它算不算在内**。
  - **三样东西全部为零**：`-L` 隔离 **0 处**（E41 的实质）· `合计 PASS=` 打印 **0 处**
    （⇒ G-A 的 `assert-pass-floor.sh` 现在还接不上它们）· `package.json` 的 `test:` script **0 个**。
  - ⇒ G-C 每套要做四件事：**加 socket 隔离 → 加 `PASS=` 计数（格式与另 8 套逐字一致）→
    加 npm script → 加带地板的 CI 步骤**。地板必须**真跑一遍**测出来（G-A 的教训：能真跑就别抄）。
  - **顺序建议**：先给一套走通全链路（建议 `gen-idle-tmux.sh`，tmux 调用最少=7），
    再复制到其余六套；**每套加完 `-L` 就先单跑一遍**，别攒到最后一起跑。
- **阻塞 / 待用户确认**：
  - ~~**[待批准] 主计划**~~ → **2026-07-29 已批准**
  - **[待定] G-C 的 6 套要不要进本地 `npm test`** —— 建议**只进 CI**。
    理由：`npm test` 应保持「不需要 tmux/daemon 就能跑」。
    代价（本地改 `shared/ccm` 不会自动触发）如实写进 `e2e/README.md`
  - ~~**[待定] `run-tests.sh` 若今天就红**~~ → **已实测：171/171 全绿**（有史以来第一次运行）
    ⇒ 直接以正常步骤进 CI，不需要「允许失败」那一轮
  - ~~**[待定] 地板值与 `ci.yml` 标签的同源对拍要不要做**~~ → **G-A 已处置，但不是「对拍」**：
    标签里的「N 项」与地板是同一个数的第二处字面量 ⇒ **直接把标签那处消掉**，留被强制的地板一处。
    消掉比钉两处一致更彻底（同 Z03 提 `UNSET_CONFIG_DIR_PREFIX` 的思路）
- **最近一次计划回看时间**：2026-07-29（Phase A 落盘）
- **自动模式（/loop）**：**全自动**（用户 2026-07-29）。本区**无需任何外部授权**。
  **G-B 要跑在 `account-zero` Z01 之前**（Z01 要改那个工具，没有网不能改）。
- **备注**：
  - **路线图第 ③ 项。规模小（3 个功能），但它保护其余全部工作。**
  - **G-B 同时是 `account-zero` Z01 的前置**：Z01 要改 `vendor/cc-acct-iso/scripts/`，
    而那 1348 行今天在 shellcheck 门禁之外、它自己的 424 行测试从没跑过。
    **没有网不能改那个工具。** ⇒ 若 `account-zero` 先启动，G-B 插到它前面。
  - **本工作区最先改 `ci.yml`**，`rust-ts-boundary` C05 与 `local-as-remote` L0/L4 之后追加。
    **不改任何 workflow 触发条件。**
  - **验收判据**：「加上了地板」不算，**「人为删一条断言，CI 红」才算**。
  - ~~**地板值直接用 Phase G 的实测数，不必重测**~~ → **G-A 重测了，8 套全部本地真跑**，
    结果与 Phase G 逐个一致（152）。**能真跑就别抄**——顺带揪出 `cc-spawn-uplift` 里一条
    绕开 `chk` 的手搓判定（输出 21 行 PASS 而计数只到 20）。原记录如下：
    `tmux-target` 26 · `ccm-cli` 44 · `ccm-print-parity` 12 · `ccm-acceptance` 15 ·
    `ccm-pretrust` 13 · `cc-spawn-uplift` 21 · `tmux-guarded` 14 · `usage-probe` 7 = **152**。
    前 7 套尾部已打印 `PASS=<n>`，**`cc-spawn-uplift` 没打印、要先加**。
  - 注意：静态 `ck`/`chk` 调用数 ≠ 运行期断言数（`ccm-cli` 静态 36 / 实测 44）。**用实测值。**

---

## G-B 签收（2026-07-30）

**做了两件事**（`ci.yml` 的 `e2e-smoke` job，只追加、既有步骤相对顺序未动、触发条件零改动）：

1. shellcheck 清单 **32 → 36** 个文件（加 vendored 四个）+ **覆盖面地板 `[ N -ge 36 ]`**
2. `run-tests.sh` 进 CI + **断言条数地板 `[ N -ge 171 ]`**

**三条实测把主计划订正了两处**：

| 主计划的说法 | 实测 |
|---|---|
| 「实测今天扩进来零告警」 | **成立且更强**：`--severity=error` 零告警，**默认档（含 warning/info/style）也是零** |
| 「`run-tests.sh` 若今天就红，那是 G-B 的第一个发现」 | **没命中**：首次运行 **171/171 全绿** |
| 账本第 4 行的 pattern `scripts/**` | **⚠ 它自己就是第一个发现**：不开 globstar 时等价 `scripts/*`，把 `scripts/test`（目录）喂给 shellcheck ⇒ `openBinaryFile: inappropriate type` **rc=2 恒红**。已改成显式四文件 + 订正账本 |

**为什么两条都要地板**：这两个检查的天然失效模式都是**静默缩水而非报错**——shellcheck 少扫
几个文件照样 rc=0；`run-tests.sh` 的退出码只看失败数 `F`，**`F=0` 就 exit 0 ⇒ 一条不跑也会绿**。
三条变异全成立（覆盖面 36→35 必红 · `**` pattern 实证恒红 · 条数报告改成 3 必红）。

**解除的阻塞**：`account-zero` 的 cc-acct-iso 半区（**Z01/Z04/Z06/Z08**）从此有网
——「没有网不能改那个工具」的条件已满足。

**留给 Z06/Z08 的一条动作**：它们本来就要改上游 + re-vendor（按 `VENDOR.md` 菜谱重算
`.vendor_id` = 6 个文件固定顺序 sha256）⇒ **届时顺手把断言地板搬进脚本自己**，
补上主计划 §2「同源」那处刻意偏离。
