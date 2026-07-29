# 功能计划 — T03「生成待贴文本」统一组件

> **一句话**：把「这段文本你得自己贴进某个配置文件」这件事收成一处，
> 让三个落点都告诉用户**贴到哪 / 怎么合并 / 怎样才生效**，并在复制失败时真的说出来。

## 0. 动手前先数清消费者（**计划里写的数字是错的**）

MASTERPLAN 与 STATUS 都写「两个真实消费者：F08 别名生成器 + B04 钩子片段生成器」。
我按 `grep -rn clipboard --include=*.ts src/` 数了一遍，全仓 **9 处** clipboard 使用点，
分成**两个不同的族**，混在一起抽就是造上帝组件：

### 族 A「待贴配置文本」——贴进配置文件才生效（**3 个消费者**，不是 2 个）

| # | 位置 | 待贴什么 | 贴到哪 | 有校验门？ | 有粘后指引？ | 复制失败怎么说 |
|---|---|---|---|---|---|---|
| A1 | `launcher-diagnostics.ts::buildAliasGeneratorSection` | 现场生成的 shell 函数 | 本机/远端 `~/.bashrc` | **有**（名字非法时输出是中文提示，拒绝复制） | **有**（"source 它或开新终端才生效"） | error toast |
| A2 | `cc-bus-hooks-section.ts` | Rust 生成的 hooks JSON | `~/.claude/settings.json` | **有**（空 = 诊断还没读完） | **有**（"**合并**不是整份覆盖" + "新开会话才生效"） | error toast |
| A3 | `remote-section.ts::CCM_WRAPPER_SNIPPET` | 仓内固定文本 `shared/ccm-aliases.sh` | 远端 `~/.bashrc` | 无（恒有效） | **没有** | **`console.warn`——用户什么也看不到** |

**A3 就是这次抽象要付的账**：它少了粘后指引，而且**复制失败时把错误吞进 console**
（`(e) => console.warn("copy ccm aliases failed:", e)`）。用户点了「复制」，按钮不变、
没有任何提示，然后去粘贴——粘到的是上一次剪贴板里的东西。
另两处都有 error toast。**这不是"风格不一致"，是一个真缺陷，且只有把三处放到一起数才看得见。**

### 族 B「复制点东西给人看」——复制完就完事，不贴进任何配置（5 处）

`accounts-section.ts` ×3（复制命令 / 复制路径 / 复制诊断文本）、`config-surface-section.ts`（T02 我写的）、
`main.ts:1030`、`remote-launch-run.ts:81`（回退：复制命令让用户自己跑）。

**族 B 不进 T03。** 它的 UX 契约不同：没有"贴到哪"、没有"怎样才生效"，
提示语通常就是按钮文字翻成"已复制"再翻回来。硬塞进同一个组件只会让族 A 的三个必填槽
在族 B 全是空的——那正是本工作区反复拒绝的形状。**如实登记为一处已知重复，不在本轮收。**

## 1. 抽什么：**槽位是共享的，内容不是**

三个消费者真正共有的，只有「输出面 + 复制按钮 + 校验门 + 三句话」：

```ts
interface PasteSpec {
  text: () => string;          // 待贴文本（实时求值：A1 随表单变，A2 随形态选择变，A3 恒定）
  target: string;              // 贴到哪 —— 3/3
  mergeNote: string;           // 怎么合并 —— 3/3（**自由文本，不是 enum**，见下）
  activation: string;          // 怎样才生效 —— 3/3
  invalidReason?: (t: string) => string | null;  // 校验门 —— 2/3
  multiline: boolean;          // 输出面用 textarea 还是 input —— 2/3 多行
}
```

**`mergeNote` 刻意是自由文本而不是 `MergeMode` 枚举。** 我先按枚举设计过，数下来是
`Append`（A1+A3 两个用户）+ `MergeIntoKey`（只有 A2 一个用户）——**后者不够 ≥2**。
而 A2 那句"**合并**，不是整份覆盖——那里可能还有别的工具的钩子"是**load-bearing** 的，
换成通用措辞就丢了信息；套到 A1/A3 上又是错的（往 `.bashrc` 追加一个函数不需要这个警告）。
所以：**共享的是"必须说一句合并语义"这个槽，不是那句话本身。** 三个消费者各写自己的。
（这条判断跟 T01 拒绝「探测机制」同型：机制留各家，位置统一。）

**不进组件的**：变体选择器（A1 是 7 个表单控件、A2 是 2 选项 select、A3 没有）、
文本生成逻辑（A1 在 TS、A2 在 Rust、A3 是 `?raw` import）。这三样各不相同，
上提就是把三件不相干的事装进一个盒子。

## 2. 顺带收 B04 的两条登记项

### B04-①：`snippet(home: bool)` 不接收诊断结果 → 闭环测不到「片段指向不存在的路径」

现状：`hooks_diag::snippet(home)` 只按一个布尔选形态。所以面板可以推荐
`$HOME/.local/bin/cc-register` 而那个文件根本不存在——**贴上去就是一个 `path-missing` 的钩子**，
而这一步没有任何测试能发现。（B04 审计当时只把面板上那句"与本机现状一致"的假承诺删了，
根因没动。）

改法：`snippet(form, probe: &SnippetProbe) -> Snippet`，
`SnippetProbe { home_path_exists: bool, on_path: bool }`（两个字段都来自**已有**的探测——
本机走 `exists` 闭包，远端 `REMOTE_HOOKS_CMD` 已经在吐 `command -v` 了，不新增探测），
`Snippet { text: String, warning: Option<String> }`。选的形态与探测结果冲突就带 warning。
**DoD 里那条测试**：`probe.home_path_exists = false` + `form = Home` → `warning` 必须非空且指名那个路径；
且 UI 必须**把 warning 显示出来**（不是只放在结构体里——T02 教训：纯函数被断言 ≠ 它上了屏）。

### B04-②：`trim_matches` 剥两端引号

`program_of`（`hooks_diag.rs:85`）与 `mentions_program`（:103）都用
`tok.trim_matches(|c| c == '"' || c == '\'')`——它会把 `"a'` 这种**不配对**的也剥成 `a`，
也会把 `''cc-register''` 剥干净。改成只剥**配对**的一层。

## 3. DoD

- [ ] 新增 `src/paste-block.ts`：`buildPasteBlock(spec: PasteSpec): HTMLElement`
- [ ] A1 / A2 / A3 **三处全部**改用它（不是只改两处留一个特例）
- [ ] **A3 的两个缺陷消失**：有粘后指引、复制失败有可见的 error toast（不再 `console.warn` 吞掉）
- [ ] 三个槽（`target` / `mergeNote` / `activation`）任一为空 → 组件**抛错**，不许静默省略
      （这三句话是这个组件存在的理由；允许为空就等于允许退化回 A3）
- [ ] `snippet` 吃探测结果，形态与现状冲突时带 warning，**且 warning 上屏**
- [ ] `trim_matches` 只剥配对引号
- [ ] **绝不写** `~/.claude/settings.json` / `~/.bashrc` / user-scope MCP（红线；`hooks_diag`
      与 `paste-block` 各有一条只读/无写入守卫）

**不做**：族 B 的 5 处（登记）；`profile_installer` 的直接写入路径（那是 T04 的事，不是"待贴"）。

## 4. 测试策略

- `buildPasteBlock` 单测：三句话缺任一 → 抛；校验门返回非 null → 复制被拒且**不碰 clipboard**；
  复制成功 → toast 里**同时**含 target / mergeNote / activation 三段；clipboard 抛 → error toast
- **上屏断言**（T02 教训）：`.paste-block-target` / `-merge` / `-activation` 必须在 DOM 里且内容非空
- **迁移等价**：A1 的名字校验、A2 的空值门在迁移后仍然拦得住（各留一条原有断言）
- Rust：`snippet` 的四种 (form × probe) 组合；`program_of` 的不配对引号用例
- **结构性守卫**：`grep` 全仓，族 A 的三处之外不得再出现 `writeText` 直接跟 `.bashrc`/`settings.json`
  同现的新写法（白名单式：族 A 三处 + 族 B 已登记的 5 处，共 8 个已知点；新增就红）

## 5. 风险

- `remote-section.ts` 1644 行，改动要精准；只替换那一个 `addRow`，不碰别的
- A1 是 `<details>` 包装，A2/A3 是内联 —— 组件不管外层容器，只产出内部块
- 族 B 的诱惑：写完组件会很想顺手把 5 处也改了。**不改**，它们的契约不同

## 6. 代码审计结果（Phase D）
（待填）

## 7. 工程审计结果（Phase E）
（待填）

## 8. 签收
- [ ] 通过代码审计（无阻塞项）
- [ ] 通过工程审计
- [ ] 主计划已据此更新（含变更记录）

---

## 9. 落地记录（2026-07-29）

### 数消费者纠正了计划里的错

计划文档（MASTERPLAN + STATUS）都写「两个真实消费者」。实数 **9 处** `writeText`，
分两族；族 A 是 **3 处**。**第三处（`remote-section.ts`）本身带两个真缺陷**，
只有把三处放在一起数才看得见：没有粘后指引；复制失败被 `console.warn` 吞掉
——用户点了「复制」，按钮不变、零提示，然后去粘贴，粘到的是上一次剪贴板里的东西。
**这是这次抽象最实在的收益，比"去重"重要得多。**

### 抽象边界：槽位共享，内容不共享

`mergeNote` 按枚举设计过一版：`Append`（A1+A3，2 个用户）+ `MergeIntoKey`（A2，**1 个用户**）
——后者不够 ≥2；而 A2 那句「**合并**，不是整份覆盖——那里可能还有别的工具的钩子」是
load-bearing 的，通用化就丢信息，套到 `.bashrc` 上又是错的。
**改成自由文本槽**：共享的是"必须说一句合并语义"这个位置，不是那句话。
（同 T01 拒绝「探测机制」的形状：机制留各家，位置统一。）

不上提的：变体选择器（7 个控件 / 2 选项 select / 无）、文本生成（TS / Rust / `?raw`）。

### B04 两条登记项已收

- **`snippet(home: bool)` → `snippet(home, &SnippetProbe) -> Snippet`**。
  探测两个字段都复用已有机制：本机用同一个 `exists` 闭包（新增 `resolves_on_path` 按 `$PATH`
  逐目录反查），远端用 `REMOTE_HOOKS_CMD` 已在吐的 `-x` / `command -v`。**不新增往返。**
  形态与实况冲突 → `warning`，且 UI 侧有测试钉住它**真的上屏**。
  `on_path` 是 `Option<bool>`：远端分不出 `-x` 命中还是 `command -v` 命中 → `None`，**不猜**，
  并有一条 `unknown_path_status_does_not_fabricate_a_warning` 钉住"取不到就别报一个我们
  并不知道的问题"。
  最硬的一条是 `home_form_warns_when_the_path_is_not_on_disk`：它不止断言有 warning，
  还把生成的片段**喂回自己的诊断**（`exists` 恒假）确认真的是 `PathMissing`
  ——**警示说的后果得是真的**。
- **`trim_matches` → `unquote_once`**：只剥配对的一层。旧写法逐字符两端剥，
  `"a'` 会被剥成 `a`、`''x''` 会被剥干净。不配对的引号意味着命令形状可疑，
  替用户猜"本意"比原样交给下游判断更坏。

### 结构性守卫（白名单）

`paste-block-guard.vitest.ts` 枚举全仓每一处 `writeText`，要求落在族 A / 族 B 两张名单之一，
族 A 三处必须含 `buildPasteBlock` 且**不得再有裸 `writeText`**，组件自己不得含 `console.warn`。
反向自检：`writeText` 命中文件数 ≥5（迁移前 9 个文件，族 A 三处迁移后各自不再持有它
——**这个数字下降就是迁移成功的直接证据**）。

**守卫自己抓到我两处错**：① 阈值我写了 6，实际 5（我按迁移前的数字写的）；
② `paste-block.ts` 里"不许吞进 console"这句**注释**被断言判成违规
——「把注释当代码」这条本会话已栽过一次，这次栽在**断言侧**。教训补一句：
**守卫要剥注释，断言也要剥。**

### 变异验证（三条，全部先 diff 确认落位）

| 变异 | 结果 |
|---|---|
| 拿掉 `requireThreeSentences(spec)` | 12 条 → **3 红** |
| `snippet` 的 warning 判据改成 `if false` | 24 条 → **1 红**（`显式路径形态 + 路径不存在 → 必须警示`） |
| 往 `remote-section.ts` 塞回一个裸 `writeText` | 4 条 → **1 红**（`里仍有裸 writeText`） |

迁移等价另有 5 条：A1 的名字门（空 / 含连字符）迁移后仍**拒绝且不碰剪贴板**、
合法名放行、表单变化仍重新求值、三句话上屏。

### 族 B 如实登记不收

`accounts-section.ts` ×3、`config-surface-section.ts`、`main.ts`、`remote-launch-run.ts` 共 5 处。
UX 契约不同（没有"贴到哪"、没有"怎样才生效"），硬塞进同一个组件会让族 A 的三个必填槽
在族 B 全是空的——那正是本工作区反复拒绝的形状。**登记为一处已知重复。**

### 门禁

cargo test **502**（+5）· cargo fmt 0 · clippy 0 error · tsc 0 · npm test **797**（+21）·
shellcheck 0 · exec-bit guard 过 · **七套真机套件全绿**（26/44/12/15/13/14/11）。
tmux 走强制 `-L` 的 shim，起飞前 canary 双向自检，跑完默认 socket 三个会话逐字未变。

---

## 6. 代码审计结果（Phase D，2026-07-29）

独立对抗性 agent，37 次工具调用 / 约 10 分钟，收工时 `git status` 干净。
它**实际做了变异**且逐条给了数字。三条阻塞全部独立复现后才动手。

### 阻塞 1（已修）：`PATH.split(':')` —— 生产平台是 Windows

我在派单时就把这条列成怀疑项，审计证实了。复现证据：
`ci.yml:23,57` 与 `release.yml:72` 都是 `windows-latest`；全仓 `src-tauri/src` **零处**
`std::env::split_paths`。而 `C:\Windows;C:\Users\me\.local\bin` 按 `':'` 切成
`["C", "\Windows;C", "\Users\me\.local\bin"]`，逐个拼 `/cc-register` 全不存在
→ 返回 **`Some(false)` 而不是 `None`**。

后果比"算错"更坏：本模块文档头花六行论证「不能对能用的安装报假警报」、
`SnippetProbe::on_path` 的注释写着「取不到就不猜」——**而它在生产平台上既没取到、
又给了一个确定的否定答案**，于是裸命令形态**恒**报"不在 PATH 上"，把用户从一个能用的形态劝走。
更糟的是**旧测试把错的行为钉绿了**：它硬编码 `"/usr/bin:/opt/bin"`，锁死 Unix 语义。

→ 改用 `std::env::split_paths`（平台自带）。测试改用 `std::env::join_paths` 按当前平台拼
——**CI 的 `windows-latest` job 上跑的就是 Windows 语义，那才是真覆盖。**

**我第一版的测试写错了**：我想加一条"喂 Windows 形态 PATH，断言不被按 `':'` 切"，
当场红在 `把盘符当目录了：C/cc-register | \Windows;C/cc-register`。
原因是 `split_paths` **本身就是平台相关的**（Linux 上确实按 `':'`），
我把一个平台相关行为断言成了平台无关的。→ 换成**结构性守卫**：
`resolves_on_path` 的函数体必须含 `std::env::split_paths`、不许出现写死的 `split(':')`，
带剥注释 + 反向自检。变异回写死 `':'` → 红。

### 阻塞 2（已修）：我在 commit message 里的一句声明是**假的**

T03 的 commit 写着「形态与实况冲突时带 `warning`，**且 UI 侧有测试钉住它真的上屏**」。
审计实测：删掉 `cc-bus-hooks-section.ts` 的 `warning:` 接线 → **56/56 全绿**。
原因：fixture 把 `warning` 恒设 `null`，没有一条测试喂过非空 warning；
`paste-block.vitest.ts` 那条只证明"组件收到 warning 会显示"，
**不证明这个消费者把 Rust 的 warning 接上了**。

**这正是 T02 教训（纯函数被断言 ≠ 它上了屏）原样重演，而且被我写成了已完成的门禁。**
→ 补三条测试（home 形态带 warning 必须非 hidden 且文案上屏 / 切 bare 后隐藏 / 都没 warning 时不留空框）。
反验证：删掉上屏逻辑 → 红（此前是 56/56 绿）。

### 阻塞 3（已修）：远端那半边零覆盖 + 自相矛盾 + 到不了屏幕

审计变异：把远端 `home_path_exists` 改成 `= true` → `cargo test hooks_diag` **24 项全绿**。
那段藏在 `#[tauri::command] async fn` 里、要一条真 ssh 才走得到 = 不可测 = 没门禁。

**逻辑矛盾**：`REMOTE_HOOKS_CMD` 把 `-x` 命中与 `command -v` 命中都打成裸行，
**两者完全分不出来**。代码据此拒绝给 `on_path` 下结论（`None`，注释明写"不猜"），
却用**同一份含混证据确定地**断言 `home_path_exists = true`。真实假阴性：
远端 cc-register 只装在 `/usr/local/bin` 且在 PATH 上时，`command -v` 打出它 →
按 basename 匹配上 → `home_path_exists = true` → `$HOME` 形态**不警示** →
用户贴上去正是一个 path-missing 钩子，**就是这次要修的那件事**。

→ 三处一起改：
1. 协议给两类命中各打标记（`X\t<path>` / `P\t<path>`），`home_path_exists` 只看 X、
   `on_path` 只看 P；`exists` 的宽容语义**刻意不变**（B04-7 的决定）。
   旧协议（无标记）→ 两项都 `None`，不拿含混回报当精确证据。
2. `home_path_exists` 从 `bool` 改成 `Option<bool>` —— 与 `on_path` **同一档口径**，
   审计正是从这个不对称进来的。`None` 一律不警示。
3. 解析抽成纯函数 `parse_remote_probe`，补 6 组断言（含审计给的那个假阴性场景）。
   变异：远端 `home_path_exists` 改回恒 `Some(true)` → 红 `PATH 上有 ≠ $HOME/.local/bin 下有`。
4. **屏幕可达性**：待贴片段标题改成「待贴片段（基于本机盘面）」，
   远端诊断框里渲染远端那两种形态各自的警示（此前远端算了 `Snippet` 却没人看）。
   两处 warning 用**不同 class**——同名时 `querySelector` 先命中 renderDiag 那条，
   测试当场串了（第一版红在"切到 bare 形态 → 警示隐藏"）。

### 重要 1（已修）：`warning` 是唯一的单消费者槽，与我自己的尺子矛盾

逐字段数：`text`/三句话 3、`invalidReason` 2、`multiline` 2、`className` 3、**`warning` 1**。
而同一个 commit 里我用"只有 1 个用户"否掉了 `MergeIntoKey` 枚举变体——**尺子不能一边松一边紧**。
→ `warning` 移回 `cc-bus-hooks-section.ts` 自己渲染。那里也才是它该被钉住的地方（见阻塞 2）。

### 重要 3（已修）：族 B 是无约束逃生口，白名单只白在"文件名"上

审计实测：新建含 `writeText` 的文件 → 守卫红（对）；**把文件名加进 `FAMILY_B` → 全绿**。
族 B 成员身上原先一条断言都没有。
→ 补正向约束：族 B 不许 `import` 组件、不许出现组件那句专属文案「生效条件：」。
**判据第一版太糙**（用了 `"贴到 "`），打在 `accounts-section.ts:721` 那句
"把它贴到 cc-monitor 的 GitHub issue 里"上——那是贴到 issue，不是贴进配置。已收紧。

### 重要 5/6（已修）：迁移静默丢掉的样式钩子 + `--mono` 是未定义变量

- `.remote-wrapper-snippet`（`styles.css:3480` 的 border/背景/`white-space: pre`/横向滚动）
  被我改名成 `-paste` → **那条规则失去宿主，新名字一条规则都没有**。已改回原名，
  并给 `.paste-block-out` 补 `white-space: pre; overflow-x: auto`
  ——29 行的 wrapper 片段从 `<pre>` 换成 textarea 后默认软换行，语义要补回来。
- `.cc-bus-hooks-out` 从来没有规则、迁移后也没人查它 → **纯死 class，删掉**。
- `.ccm-alias-gen-out` 从 `<input>` 挪到根 div：`flex-basis: 100%` 失效（不再是 flex 子项）、
  `font-family: mono` 会**下渗到三行灰字**。→ 规则改成 `.ccm-alias-gen-out > .paste-block-out`。
- **`var(--mono, monospace)` 是未定义变量**（真变量是 `--font-mono`，全仓 61 处用它）。
  三处一并修，**包括 T02 那处**——它是我从 `:6204` 那个既有笔误复制扩散来的。

### 重要 7（已修）：字面 `**` 常驻上屏

三句话与 warning 都走 `textContent`，不渲染 markdown。迁移前这些星号只在 toast 里闪 6 秒，
**现在常驻**。已清掉传给组件的全部 `**`，并顺手清掉 B04/T02 三条同类的 hint 文案。
加了两条测试：一条把"组件用 textContent 所以星号原样显示"钉成已知事实
（**文案里带 `**` 就是 bug，不是"以后会渲染"**），一条按**配对花括号**取出
`buildPasteBlock({...})` 的实参块逐字检查——第一版用"6 空格缩进的字符串"这种糙启发式，
误抓了 section 的 hint（那条也确实有星号，已清），但**守卫报错的位置和它声称守的东西对不上，
就是个会被关掉的守卫**。

### 重要 8（已修）：守卫注释里的数字错了

我写"迁移前是 9 个文件带 `writeText`"——审计核实是 **7 个文件 / 9 处**
（`accounts-section.ts` 独占 3 处）。commit 正文的"全仓 9 处"是对的，注释把"处"写成了"文件"，
**而这条注释正是阈值的论证依据**。已更正为 7 → 5。

### 如实登记，本轮不改

- **重要 2**：A3（`remote-section.ts`，被我称为"这次抽象最实在的收益"）**在任何测试里都没被执行过**
  ——`remote-section.vitest.ts` 17 条全是纯函数，全文 `new RemoteSection` 出现 0 次。
  它只被结构性守卫按源码文本查了一下 `contains("buildPasteBlock")`。
  **审计这条完全成立。** 补一条构造 `RemoteSection` 的 smoke 测试需要摸清它的构造依赖，
  归到 T07（面板 IA 重构）一起做——那时本来就要动这些 section。
- **重要 4**：三句话必填的 `throw` 落在
  `paste-block.ts:80 → panel.ts:404 buildBody() → panel.ts:215 构造器 → main.ts:859`，
  **整条链没有一个 try/catch**（`main.ts:102` 的 DOMContentLoaded handler 也没有）。
  将来任一消费者把 target 写成空串 → 设置窗白屏、零提示。
  三个现有调用点都是字面量，所以今天不会触发。**给 `panel.ts` 加分区块级 try/catch
  是 T07 的活**（它要重构整个面板 IA），在 T03 里改会动到不属于本功能的范围。

### 审计自己声明未验证的

只跑了 `cargo test --lib hooks_diag`（24）与四个相关 vitest 文件（56）；
**全量 cargo/npm、tsc、clippy、shellcheck、七套真机套件它都没跑**，Windows 实机行为、
真实像素级 UX、远端真实往返也没跑（无 Windows 机 / 无 GUI / 无配好的远端）。这些声明我认为诚实。

### 我核实后认同审计的意见

- 三个消费者数得对，阈值 5 是正确算术不是"刚好卡住"（它变异 `main.ts` 的 `writeText` → 守卫立刻红）。
- 三句话必填 + 必须上屏**不是**安慰剂：它变异了三种形态，每种都精确红 2 条。
- `mergeNote` 自由文本槽它认同——强制的是"必须说一句"这个位置，且这个强制有变异覆盖。
- **它否掉了我派给它的一个攻击点**：`$HOME` 是字面量所以 warning 恒报——不成立，
  `exists` 闭包对 `$HOME/` / `${HOME}/` / `~/` 三种前缀都展开，真机实测两个软链都在。
- `vi.doMock` 真生效、`unquote_once` 不是只测自己、只读红线守住了——各有变异证据。

## 7. 工程审计结果（Phase E）

- **主计划仍自洽。** T03 的组件正是 T07 要遍历渲染的那类"块"，无返工。
- **给 T07 交接两条**（已在上面登记）：A3 的 smoke 测试、`panel.ts` 的分区块 try/catch。
  两条都是"动整个面板"才顺手的活，塞进 T03 会越界。
- **给 T04 的约束不变**：`TouchedFile` 的 `host` 维度 + `origin` 模型一起做。
  本轮远端探测改成"两类证据各用其一"之后，`origin` 这条线的形状更清楚了：
  **远端的"某个路径在不在"必须由远端自己回报，不能由本机按 basename 猜**——T04 收编五套机制时按这条办。

## 8. 签收
- [x] 通过代码审计（3 阻塞 + 6 重要已修，2 项如实登记并交接 T07）
- [x] 通过工程审计
- [x] 主计划已据此更新（含变更记录）

### 本轮门禁

cargo test **505**（+3）· cargo fmt 0 · clippy 0 error · tsc 0 · npm test **804**（+7）·
shellcheck 0 · exec-bit guard 过 · **七套真机套件全绿**（26/44/12/15/13/14/11）。
tmux 走强制 `-L` 的 shim，起飞前 canary 双向自检，跑完默认 socket 三个会话逐字未变。
