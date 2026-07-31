# S1 — 远端配置持久化：整表覆盖 → 按 host 局部合并

主计划 §0.2-1 的硬前置：**S2 及之后所有拆页面都依赖它**。风险栏标的是「高（数据丢失面）」。

## §0 今天的三个数据丢失面（读码核实）

### (a) 整表覆盖 —— S2 一拆页就会真丢机器

`writeRemoteConfig(next)` 把 `cfg.remote` **整个换掉**（`src/remote-config.ts:162`）。
今天不出事，是因为 `RemoteSection.collect()` 恰好 `this.cards.map(...)` 映射**全部**卡片
（`remote-section.ts:525`）—— **正确性来自 UI 的巧合，不是来自构造**。
S2 一旦把机器卡片拆到「机器详情页」（一页一台），那一页调保存就会把其余机器**静默删光**。

### (b) 序列化字段是手抄清单 —— 已经咬过两次

`writeRemoteConfig` 里逐字段手写 `{ label, host, port, … }`。
`RemoteHostConfig` 新增字段而这里没跟上 ⇒ **每次保存都静默丢掉那个字段**。
代码注释自己记着两次事故：`jump`（F56 D-B1：「设置卡填的跳板被静默丢弃」）、
`daemonless`（F59：「同 D-B1 教训：枚举字段必逐个写全，防静默丢失」）。
**两次都是事后补的，至今没有任何东西钉住第三次。**

### (c) 卡片身份靠位置 —— 改了 label 就换了 key

机器的 key 是 origin（`label.trim() || host`），而 origin **可以被用户编辑**。
整表覆盖时这个问题被掩盖了（反正全写）；改成局部合并后，「这张卡对应盘上哪一条」
必须有明确答案，否则改名会变成「新增一台 + 留下一台孤儿」。

## §1 DoD

- [x] 保存路径不再整表覆盖 —— 且比计划更进一步：**整表覆盖的 `writeRemoteConfig` 已取消导出**，
      这条 footgun 在类型层面不可达，不靠「记得别用它」。有测试钉住它不得被重新 export。
- [x] 删除机器仍然生效（含「两台同名删其一」这个我自己引入的回归，见 §6）。
- [x] 改 label / host 仍然是**改**那一条，位置也保住 —— 纯函数 + section 两层各有一条。
- [x] 序列化字段清单编译期穷尽 —— 实测删掉 `"jump"` 后 `tsc` 报
      `Type 'true' is not assignable to type '"jump"'`，**错误信息直接点名缺的字段**。
- [x] 合并逻辑是纯函数 `applyRemoteHostsPatch`，不碰文件系统。
- [x] 变异验证 9 条，逐条改坏见红、还原见绿（清单见 §6）。
- [x] 门禁全绿（vitest 894 / tsc / build / coverage 门槛 / lint 我改的两文件零告警 /
      Rust 两侧 644+176）；`remote-section.vitest.ts` 既有断言原样绿。

**不做**：不动 config.json 的**盘上格式**（Rust `load_remote_configs` 读的键不变）；
不引入并发写锁（整个 config.json 的 read-modify-write 竞态是既有面，与本功能正交，
真要治是另一件事）；不改 UI 布局（那是 S2+）。

## §2 方案

### 2.1 数据层：一个纯函数 + 一个 IO 包装

```ts
export interface RemoteHostsPatch {
  enabled?: boolean;                                  // 缺省 = 不动
  upsert?: { key: string | null; value: RemoteHostConfig }[];
  remove?: string[];                                  // 按 origin
}
/** 纯函数，可单测：把 patch 应用到一份现有配置上，返回新配置。 */
export function applyRemoteHostsPatch(cur: RemoteConfig, patch: RemoteHostsPatch): RemoteConfig
/** IO 包装：read → applyRemoteHostsPatch → writeRemoteConfig。 */
export async function patchRemoteConfig(patch: RemoteHostsPatch): Promise<void>
```

`key` = 这条记录**在盘上当前的 origin**；`null` = 新增（追加到末尾）。
key 找不到（盘上被别处删了）也按新增处理 —— 比静默丢弃安全。

**安全性质来自构造**：patch 里没提到的 host，`applyRemoteHostsPatch` 根本不碰它。
S2 拆页后一页只提交自己那几台，其余天然安全，**不依赖调用方守纪律**。

`writeRemoteConfig` **保留但取消导出**（实现时比这里原本写的更进一步）：它降级为数据层内部
的「序列化 + 落盘」单一出口，`patchRemoteConfig` 调它。不删是因为它是唯一知道盘上形状的地方；
不导出是因为「UI 不再调它」如果只靠约定，S2 拆页时随手一调就会静默删机器 ——
**不导出 = 这条 footgun 在类型层面不可达**。

### 2.2 字段穷尽性：编译期，不是测试

```ts
const REMOTE_HOST_FIELDS = [...] as const satisfies readonly (keyof RemoteHostConfig)[];
type MissingField = Exclude<keyof RemoteHostConfig, (typeof REMOTE_HOST_FIELDS)[number]>;
const _noMissingField: MissingField extends never ? true : MissingField = true;
```

漏字段 ⇒ `tsc` 报错，且错误信息里**点名**缺的那个字段。
比测试强：测试要有人想起来写，类型检查是每次编译都跑。

### 2.3 卡片身份：`persistedKey`

`MachineCard` 记住它**加载时**的 origin（新增卡片 = `null`）。保存时：

- `upsert` = 每张卡 `{ key: card.persistedKey, value: card.collect() }`
- `remove` = `本次编辑器加载时的 key 集合` − `当前还在的卡片的 key 集合`
- 保存成功后把每张卡的 `persistedKey` 更新成它的新 origin

**`remove` 的基准刻意取「本编辑器加载时的集合」而不是「盘上全量」** —— 这正是
S2 拆页后的安全边界：一页只对自己加载过的那几台负责。

## §3 共享面账本对照

| 共享面 | 本功能怎么动 | 与账本 §5-2 的最终形态 |
|---|---|---|
| `src/remote-config.ts::writeRemoteConfig` | 降级为数据层内部序列化出口**且取消导出**；UI 改走 `patchRemoteConfig` | ✅ 账本写的就是「按 host 局部合并（read-modify-write 单个 host）」 |
| `src/settings/remote-section.ts` | 只动 save 路径 + 卡片加一个 `persistedKey` 字段；**不拆 `MachineCard`**（那是 S4） | 不冲突 |

## §4 步骤

1. `remote-config.ts`：字段清单 + 编译期穷尽性检查，`writeRemoteConfig` 改用它序列化。
2. `remote-config.ts`：`applyRemoteHostsPatch` 纯函数 + `patchRemoteConfig` IO 包装。
3. 纯函数单测（含「无关 host 字节不动」「删除」「改名不分裂」「key 找不到当新增」）。
4. `MachineCard`：加 `persistedKey`（构造时 = initial 的 origin；新增卡为 null）。
5. `RemoteSection.save()`：改为算 patch 再调 `patchRemoteConfig`；维护 `loadedKeys`。
6. 变异验证 + 门禁。

## §5 测试策略

- 纯函数四类：无关 host 不动 / upsert 改名 / remove / key 缺失当新增。
- 「无关 host 字节不动」用 **深比较原对象**，不是只比长度 —— 只比长度的话，
  「改坏了某台的字段」也能蒙混过关。
- 既有 `remote-section.vitest.ts` 的 write→read 往返必须原样绿（回归护栏）。

## §6 代码审计结果（Phase D）

**做法如实记**：同 S0，本轮未开并行审计 agent（会话运行约束），主线程逐点自审 + 变异兜底。

### 自审揪出一个**我自己引入的回归**（两台机器 origin 相同时）

origin 重复是无效配置，但整表覆盖那个老写法**碰巧能正确处理它**，我的局部合并第一版不能：

- **upsert 互踩**：两条 upsert 拿同一个 key，第二条会再次命中第一条已被替换过的位置
  ⇒ **第一条编辑凭空消失**。修法：匹配过的下标不再复用（`used` 集）。
- **删除静默失效**：删除基准若按**集合**算，删掉两台同名中的一台会得出 `remove=[]`
  （另一张卡还占着同一个 key）⇒ 点了删除什么也没发生。修法：按**出现次数**比。

两条都补了测试 + 变异。**没有让 origin 重复变合法** —— 它在
`announced_registry`/`idle_registry`/`tmux_raw_registry` 里同样会互相覆盖，
持久化只是最后一环。UI 层的重名校验登记为 BACKLOG **E44**，归 S4 做。

### 其余复核

- 顺手消掉一处我刚引入的重复判据：`findHostByOrigin` 改为复用 `hostKey`
  （否则「什么算 origin」立刻就有两份实现）。
- `persistedKey` 只在 `patchRemoteConfig` **成功之后**才更新（写在 `try` 里 await 之后）
  ⇒ 保存失败时卡片身份不漂，下次重试仍指向盘上正确那条。
- 发现并钉住一条既有行为：`coerceHost` 把空 `label` 归一成 `host`（注释自陈是为了与 Rust
  `origin_label` 对齐），于是 read-modify-write 会把它固化到盘上。origin 两种形态算出同一个值
  ⇒ 功能等价；但盘上确实多出一个用户没填过的 label。**测试里写明这个事实，不假装没有。**

### 变异验证 9 条（逐条改坏见红、还原见绿）

① 退化成整表覆盖 · ② upsert 恒追加（改名分裂）· ③ remove 被忽略 · ④ 序列化清单漏字段 ·
⑤ 整表覆盖重新被导出 · ⑥ section 删除基准被抹掉 · ⑦ section 卡片身份丢失 ·
⑧ 去掉已消费下标（同名互踩）· ⑨ 删除基准退回集合比。

## §7 工程审计结果（Phase E）

- **S2 的前置条件已成立，且是结构性的**：安全性质写在 `applyRemoteHostsPatch` 的构造里
  （patch 没提到的 host 根本不读不写），不依赖调用方守纪律。S2 拆页时哪怕一页只提交一台，
  其余机器也不会被碰。
- **section 层意外可测**：`RemoteSection` 在 jsdom 里能真实例化（`populateAliases` 的 IPC
  失败被自身 try/catch 吃掉）。本仓此前没有 section 级 DOM 测试的先例，这次建立了一个
  ——**S2/S4 拆页面时这条路子可以直接复用**，比继续堆源码守卫强得多。
- **未新增跨语言双写点**。`hostKey` 与 Rust `origin_label()` 的既有对应关系（含「后端不 trim」
  这条已知差异）原样保留，注释里本来就写着，本轮不动它。
- **对后续功能的影响**：`MachineCard` 多了一个 `persistedKey` 字段。S4 要把 `MachineCard`
  提出去做详情页主体，那时这个字段**要跟着搬**——它是卡片身份，不是渲染细节。已记进账本。

## §8 签收

- [x] 过代码审计（自审 + 9 条变异；强度不足之处已在 §6 声明）
- [x] 过工程审计
- [x] 主计划已更新
