# F-history-persist(#46 历史来源首开只本地/延迟)

> 类1 feature 5(最后一个)。F76 已加 30s **内存**缓存(会话内 reopen 秒开);本 feature 补**跨启动持久化**——让每次启动**首开**也暖、不再只本地。用户拍板「做 localStorage 持久化」。

## DoD / 验收
- 远端来源快照持久化到 localStorage;`HistoryView` 构造时 hydrate → 首开渲染「本地 + 持久远端」暖帧。
- ★`loadedAt` 归 **0**:持久快照只作**首帧暖绘**,首开必刷一次(`shouldRefetchRemote(0)` 恒 true)→ 不吃跨启动陈旧 / 远端配置变更(refetch 全替换纠正)。F76 内存 30s TTL 不动(单例,hydrate 每启动一次)。
- 持久时机:仅**全台成功**写;**部分失败**清(不暖绘残缺);**全失败**保留(合法全成功快照,留作暖绘)。
- 验证:tsc + 全套 `npm test`。

## Phase D 审计(2 视角)+ 处置
- **正确性(视角1)**:无阻塞/无重要。核实 loadedAt=0 保证首开必刷(单例=每启动一次)、持久只全成功写/部分失败清/全失败不动、seq 守卫防撕裂写、仅远端元数据(~200B/项、配额安全)、malformed **不崩**。
- **范围+符合度+data-at-rest(视角2)**:**一个重要** —— hydrate 只校验数组**形状**、不校验**元素**;被篡改/旧 schema 混入 `null`/基元元素 → 首帧 `renderList` deref `p.origin` 抛 TypeError,而 renderList 在 refresh 里**未 try 包** → 冒泡出 `open()`、历史打不开(且违背模块「防脏 localStorage」惯例:siblings 都逐元素 normalize)。其余:范围最小(3 文件 54+/1-)、键名合规(`cc-monitor.history.remote-sources`、走 safe* 助手、进 enumeratePrefix)、DoD 达成、F76 不回归。
- **处置(重要→已修)**:抽 `loadPersistedRemoteCache()` 模块级助手(对齐 `loadExpandedForks`/`normalize*` 惯例),**逐元素过滤**到「非空对象 + `projectPath` 为 string」;构造改走它。既修崩溃、又对齐 read 侧风格一致性(视角2 建议2)。补 2 测:脏 localStorage(混 null/基元)→ 过滤+`open()` 不 reject;跨启动 all-fail → 持久不清 + 暖帧仍在。

## 未采纳 / 记录(建议)
- **schema-version tag**:元素过滤 + 首开强制 refetch 已把 blast radius 收在「首帧 cosmetic、自愈」,不加版本标(过度)。
- **delete 处理器不 re-persist**:删远端项目末会话时同步内存 remoteCache 但不重写 localStorage → 下次启动暖帧可能闪一下已删项;transient、refetch 自愈,与「暖帧可陈旧」设计一致,不动。
- **data-at-rest 透明**(视角2 建议1):远端项目**路径**现落本地 localStorage(host label 早由 #45 落了,路径是新增),即使不再连该远端也留;元数据、无会话内容、每启动刷新。**follow-up**:data-section 卸载说明可加一句「远端来源元数据现本地持久」。非本 bug 范畴,记此。
- host-set-change 跨启动测:正确性审计已论证 refetch 全替换纠正,未单测(中价值,略)。

## Phase E 工程审计(主线程对账)
- 共享面:`local-storage.ts`(加键、合规)、`history.ts`(构造 hydrate + refresh 两分支持久)。与 #45 的 `historyHiddenOrigins`/`historyOriginOpen` 同风格(load*/save* 助手)。无跨功能耦合。全套 350 测过。
- 未碰 F76 TTL 逻辑(`history-cache.ts` 一字未动)、本地批、渲染、#45 筛选/折叠。

## 签收:代码审计[x](重要已修+补测) 工程审计[x] 主计划已更新[x]
