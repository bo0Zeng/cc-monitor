# F12 — remote-section 数据层抽 remote-config（治分层倒挂）

> 账本 I6/G3。中风险**行为等价重构**：把 config 数据层从 1801 行 UI 文件 `settings/remote-section.ts`
> 抽到新 `src/remote-config.ts`，非 UI 模块改依赖数据模块。**只搬不改**——运行时行为逐字节等价。

## 背景（摸底）
- `settings/remote-section.ts` **1801 行**：UI（`RemoteSection`/`MachineCard` 类 + DOM）**混住**数据层
  （`RemoteConfig`/`RemoteHostConfig` 类型、`readRemoteConfig`/`writeRemoteConfig`/`resolveRemoteConfigByOrigin`
  配置 CRUD、`findHostByOrigin`/`sftpEligibleHosts`/`parseAddressLines` 纯函数）。
- **分层倒挂**：6 个含非 UI 的模块（`tabs.ts`/`account-chip.ts`/`cards/index.ts`/`main.ts`/`port-forward.ts`
  + 3 测）从 UI 文件 import 数据符号 → 依赖整个 1801 行 UI 模块、测试被迫整体 mock。审计 I6 指定抽 `src/remote-config.ts`。
- `panel.ts` import 的是 `RemoteSection` **UI 类**（留 remote-section，不动）。

## DoD
- [x] 建 `src/remote-config.ts`（180 行）**逐字节搬入**数据层：`import {loadConfig,saveConfig} from "./config"`；类型
  `RemoteHostConfig`/`RemoteConfig`；`export const HOST_DEFAULTS`；纯函数 `parseAddressLines`/`sftpEligibleHosts`/
  `findHostByOrigin`；私有 `coerceAddresses`/`coerceHost`；CRUD `readRemoteConfig`/`writeRemoteConfig`/
  `resolveRemoteConfigByOrigin`。
- [x] `remote-section.ts`（1801→1640）删已搬定义，改 `import {HOST_DEFAULTS,parseAddressLines,read/writeRemoteConfig,类型} from "../remote-config"`；**UI 类逻辑一字未改**；`sameHost`/`sameRemote`(dirty-check)/`describeStage`/`defaultDaemonPathFor` 留原处。
- [x] **8 个** importer 改源 → `remote-config`（tabs/account-chip/cards·index/main/port-forward + 摸底漏的 **accounts-section.ts** + **sftp/panel.ts** 类型；panel.ts 的 `RemoteSection` UI 类留 remote-section）。
- [x] 测试：`remote-section.vitest` 数据符号 import 改指 `remote-config`（describeStage/shouldShowResetFingerprint 留）；`account-chip.vitest`(mock target + type)/`accounts-section.vitest`(mock target) 同步；panel-groups.vitest 的 `RemoteSection` mock 留 remote-section 不动。
- [x] **验证**：tsc 0 / npm test 595（不减）/ build 0；无 import 环（remote-config 仅依赖 config）；无残留从 UI 文件 import 数据符号。
  - 注：数据层测试现居 `remote-section.vitest`（import 自 remote-config），**覆盖已达成**；co-locate 到 `remote-config.vitest` 属可选后续（不影响覆盖/解耦）。

## 不做什么
- **不改任何运行时行为**（纯搬迁 + 改 import；UI 渲染/交互一字不动）。
- **不动 `RemoteSection`/`MachineCard` 类逻辑**、不动 `describeStage`/`ConnectStage`/`sameHost`/`sameRemote`/
  `defaultDaemonPathFor`（UI 显示/dirty-check，留原处）。
- 不 push/发版/bump；daemon 零改（本功能纯前端）。

## 与主计划对接（共享面）
- 账本「`remote-section.ts` 数据层 → `remote-config.ts`」（F12/F13）：本功能落该最终形态。F13 脊柱拆分不再碰此数据层。

## 审计结果
- **代码审计(D)（中风险主线程自审——纯机械搬迁，tsc/595 测/build 三重网兜底）**：
  - *行为等价*：逐字节搬（tsc 0 + 595 测不减 + build 0 证），无逻辑改动。
  - *无隐藏耦合/环*：remote-config **仅** import `./config`（loadConfig/saveConfig），无 UI/DOM 依赖；config.ts 不反向 import → 无环。方向修正（UI→数据、非 UI 模块→数据，皆正确向）。
  - *计划符合度*：数据层全搬、UI 类零改；摸底漏的 accounts-section.ts + sftp/panel.ts 经 tsc 兜出并修（8 importer 全迁）。
  - *红线*：纯前端、daemon 零改、无发版/轮询/bashrc。
- **工程审计(E)**：分层倒挂已治——非 UI 模块（tabs/account-chip/cards/main/port-forward/accounts-section/sftp）依赖 180 行数据模块而非 1801 行 UI 文件；测试可 mock 小模块。remote-section 1801→1640。F13 脊柱拆分不再碰此数据层。主计划自洽。

## 签收
- [x] **F12 过 D+E+F**（中风险主线程自审 + 三重网）：数据层抽 `src/remote-config.ts`、8 importer 迁移、行为逐字节等价（tsc0/npm595/build0）、无环、无残留 UI-文件数据 import。remote-section 1801→1640。
