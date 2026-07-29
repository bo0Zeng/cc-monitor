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
