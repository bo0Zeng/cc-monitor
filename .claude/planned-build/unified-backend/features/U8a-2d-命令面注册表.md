# U8a-2d — 命令面注册表（把「声明 ↔ 实现」的漂移从「扫文本」变成「不可表示」）

- 工作区：unified-backend · 第六梯队 · 任务 #96
- 来源：`DESIGN-命令面怎么长大.md` §3（R1–R4）+ §5 第 3 步
- 时机（采纳设计 agent 自己的论证）：U10/U11 会把命令从 4 条推到 8-9 条。
  **4 条时改形状要迁 4 个文档小节，9 条时要迁 9 个**，且那时若干已有仓外消费方。
  现在是最便宜的一刻，也是唯一一刻手上有 `launch` 这个「既阻塞、又有 args/data、
  又还没冻结」的真样本。

## 摸底：先把设计稿的前提验一遍（血泪 7）

**R2 的前提是错的。** 设计稿说「`control/`/`observe/` 生产段不许 `derive(Serialize/Deserialize)`，
白名单**恰好一条** `resolve_query.rs`」。实测：

| 文件 | serde 类型 | 它到底是什么 |
|---|---|---|
| `control/resolve_query.rs` | `ResumeSpec` · `Capabilities` · `CommandPlan` · `ResolveError`（4 个） | 与仓外 aterm **冻结在 2026-07-18** 的一次性契约 |
| `control/fork_write.rs` | `ForkResult`（1 个） | **一次性子命令 `--fork-session` 的输出形状** |
| `observe/accounts_query.rs` | `RawAccount`（1 个） | **根本不是 wire** —— 它在解析 cc-acct-iso 写的清单**文件** |

⇒ 三处不是一处；而且后两处**搬进 `wire/`（流协议的家）是错误归类** ——
一个是一次性子命令的出参，一个是文件 schema。

**处置：R2 改形状，不改目标。** 从「白名单恰好一条 + 目录树遍历」改成
**登记制**（逐条列举 + 写明理由 + 机检），形状照抄 `readonly_guard::spawn_registry`。
「上线类型收进 `src/wire/` 目录树」等**真出现第二个流协议类型文件**时再做 —— 今天只有一个。

## DoD

1. **R1 注册表**：`CommandSpec { name, doc_anchor, codes, fields, run }` + `Run::{Async, Blocking}`。
   `dispatch` 从注册表**查**，不再是一串手写臂 ⇒ **删掉** `hello_commands_match_the_dispatch_table`
   （那条扫分派臂文本、按 8 空格缩进切的机检）。
2. `pub const COMMANDS` **保留当镜子**（monitor `inbound_client.rs` 与 e2e 都在文本抽取它），
   配一条**数据对数据**的「镜子 == 注册表」测试。
   ⇒ 净效果：**文本扫描型护栏 −1，数据对数据 +1**。
3. **R3**：`launch` 的 `args`/`data` 字段名必须出现在**它自己那一小节**的代码跨度里
   （闭掉设计稿 P2：那 8 个字段今天一个都不在护栏视野内）。
   `fields` 是手写镜子 ⇒ **必须再钉一层**：与解析器/输出构造器实测对拍。
4. **R4**：协议级 code 闭集 + 零命中（`control/` 生产段不许出现协议级 code 字面量）。
5. **R2（改形状后）**：`control/`/`observe/` 的 serde 类型**逐条登记 + 写明理由**，新增未登记 ⇒ 红。

### 不做什么

- **不把 `wire.rs` 拆成目录树**（今天只有一个流协议类型文件；等第二个出现再说）。
- **不把 `fork_write` / `accounts_query` 的类型搬进 `wire/`**（错误归类，见摸底）。
- **不给命令名编版本号**（设计稿 Q4 已裁决：改走「args 只许加、语义变就换名」）。
- **不碰 `cancel` 的特殊性** —— 它要 `replies`/`running`，签名与别的命令不同，
  留成 `dispatch` 里的硬臂并**在注册表里显式声明它是硬臂**（否则镜子对不上）。

## 设计

### 为什么 `cancel` 不进 `Run`

`cancel` 需要 `&mpsc::Sender<Frame>` 与 `&Running` —— 它**就是**取消机制本身。
硬塞进统一签名要给每条命令都传这两个东西，等于把「普通命令不许自己发帧、不许碰登记表」
这条今天成立的性质**拆掉**。⇒ 它留在 `dispatch` 里，但**在注册表里占一行**
（`Run::Builtin`），这样「镜子 == 注册表」仍然覆盖它。

### 诚实边界（设计稿自己列的反对意见，逐条对待）

- **`doc_anchor` 是约定不是事实** —— 护栏只能查「那个标题在、字段名在它下面出现」，
  查不了写得对不对（`protocol_doc_guard` 头注早就登记过这条局限）。
  但这不是**新增**局限，是把既有局限**局部化**：从「§10 全节任意反引号」收到「本命令那一小节」，
  强度只升不降。
- **`fields` 是手写的** ⇒ 它本身就是新的漂移源。所以必须配「与解析器实测对拍」那一层，
  否则等于用一个手写清单去证明另一个手写清单。
- **`spawn_blocking` 是真的不可取消** —— `Run::Blocking` 不是「修好了取消」，
  是**停止假装能取消**。这句话要留在类型的文档里。

## 逐条步骤

| # | 做什么 | 怎么验证 |
|---|---|---|
| 1 | `CommandSpec` + `Run` + `REGISTRY`；`dispatch` 改成查表 | 既有全部 daemon 测试仍绿 + e2e 27/27 |
| 2 | 删 `hello_commands_match_the_dispatch_table`；加 `the_commands_mirror_matches_the_registry` | 变异：`COMMANDS` 多/少一条 ⇒ 红 |
| 3 | `fields` 与 `launch` 解析器/输出实测对拍 | 变异：解析器加一个 `get_str` 而 `fields` 没跟 ⇒ 红 |
| 4 | R3 文档小节对拍 | 变异：把某个字段从小节里删掉 ⇒ 红 |
| 5 | R4 协议级 code 闭集 + 零命中 | 变异：`control/` 里回一条 `unknown_command` ⇒ 红 |
| 6 | R2 登记制：`control`/`observe` 的 serde 类型逐条登记 | 变异：新增一个未登记的 derive ⇒ 红 |

## 代码审计结果（D）

改动集中在 daemon 的护栏面，D 由**逐条变异复验**完成（五条新判据 + 一条被删的旧判据）：

| 判据 | 变异 | 结果 |
|---|---|---|
| `the_commands_mirror_matches_the_registry` | `COMMANDS` 多一条 `"zzz"` | **红** |
| `launch_fields_match_its_parser_and_output` | 解析器多读一个 `get_str("zzz_new_field")`、`fields` 没跟 | **红** |
| `every_command_payload_field_appears_in_its_own_doc_section`（R3） | 把 `ccm_sid` 从 `launch` 小节里改名 | **红**（逐字报出缺哪个） |
| `protocol_level_codes_are_never_emitted_from_the_control_layer`（R4） | `control/launch.rs` 里回一条 `unknown_command` | **红** |
| `every_serde_type_outside_wire_is_registered`（R2 登记制） | `control/` 新增一个未登记的 `#[derive(Serialize)]` 类型 | **红** |
| **被删的** `hello_commands_match_the_dispatch_table` | —— | 换成注册表之后它的抽取器只切出 1 条臂、**自己的计数自检把它打红了**（不是静默变绿）—— 这正是「该退休了」的信号 |

### 顺带清掉的两处

- `Disposition::spawn` / `spawn_blocking` 两个构造器改查表之后**没有调用方**了 ⇒ 删掉
  （clippy 报的，不是我记得的）。
- `CommandSpec` 的 `doc_anchor`/`codes`/`fields` **只被护栏读** ⇒
  `#[cfg_attr(not(test), allow(dead_code))]` 精确开口，并在文档里写明「那正是它们存在的理由」，
  而不是给整个类型开 `allow`。

daemon clippy 从 0 → 4 → **回到 0**（不是靠 allow 掩盖：两个构造器是真死代码）。

## 工程审计结果（E）

- **净效果就是那句话**：文本扫描型护栏 **−1**，数据对数据 **+1**，新增护栏 **+4**。
  「声明了却不接 / 接了却不声明」在**注册表这一侧不可表示**（名字与处理器是同一个值）；
  剩下的 `COMMANDS` 镜子由数据比对钉住 —— 比原来「按 8 空格缩进切分派臂文本」强一档，
  且对 rustfmt 不再脆。
- **我把设计稿的 R2 前提改了，并写清了理由**（实测 3 个文件 6 个类型，且其中两个搬进
  `wire/` 是**错误归类**）。目标不变、形状改成登记制。这是本轮第三轮「验 agent 的断言、
  打掉一条」——纪律在持续兑现。
- **诚实边界都留在了代码里**：`doc_anchor` 是约定不是事实（局部化了既有局限，强度只升不降）·
  `fields` 是手写镜子所以必须再钉一层 · `Run::Blocking` 不是「修好了取消」而是「停止假装能取消」·
  `resolve` 的命令级 `bad_request` 与协议级同名是**登记在案的例外**（aterm 契约冻结）。
- 账本 **S16 更新**（命令面从「六处副本」改写成「注册表 + 三条派生 + 一面镜子」）；
  新增 **S26**（`control`/`observe` 的 serde 类型登记制）。

## 签收

- [x] 过代码审计（D）—— 五条新判据逐个变异复验；被删的那条由它自己的计数自检送走
- [x] 过工程审计（E）—— 账本 S16 改写 + S26 新增；R2 前提订正并写明
- [x] 主计划已更新（F）
