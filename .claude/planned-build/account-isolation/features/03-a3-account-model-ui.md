# A3 — 账号模型 + 全局切换 UI(前端,非破坏性)

> 设计基线见 `../DESIGN-account-switching.md` §2/§3/§4/§7/§8。本 feature **纯前端 TS**,
> 只"看得见 + 切默认账号 + 显示徽章",**不注入 env、不重启会话**(那是 A4/A5)。
> 依赖 A2 的三个 Tauri 命令。全程走 A2 的 `available:false` 降级路径,旧 daemon/未迁移零报错。

## DoD(可勾选)
- [ ] `src/accounts.ts`:账号 store(单一真相)。`invoke` 包装三命令 + TTL 缓存(照 tmuxCache 8s 范式)+ 手动刷新 + `defaultName` 落 config.json(照 `remote.hosts[]` 范式,枚举字段写全防丢)。纯函数(解析/格式化/降级判定)可 vitest。
- [ ] **状态栏 chip**:`👤 <默认账号>` / `👤 未启用`,点开浮层选单(照 `.status-cmdk` + SFTP host-picker 范式);选中=**只改默认账号**(非破坏),toast 明说"已有会话不受影响"。
- [ ] **设置面板「账号」组**:占用既有空 `remote-placeholder` 组(或并列新组);账号表(名/邮箱/mode/登录态/configDir/默认)+ 设为默认/刷新/打开该账号终端(A6 前先只 `run` 提示)/复制 configDir;顶部状态含 manifest 路径 or"未启用→部署引导占位(A6 填)"。
- [ ] **Ctrl+K 只读命令**:`账号:切默认为 X`(只读,直接进)/`账号:管理…`(开设置账号组)。**写命令(用 X 重启)留给 A5**——本 feature 不加写命令,守 F11。
- [ ] **每条会话账号徽章**:tab 行(`createTabButton`)加小徽章,内容=账号名首字,hover 显示账号+邮箱+来源;未知显 `—`(§3:不猜);**本地会话(origin===null)不显示**。`GridSessionSnapshot` 同步加 `account` 字段。
- [ ] **降级矩阵(§7)全覆盖**:未迁移/旧 daemon/daemonless/未登录/in-place/本地会话 各自表现正确,一处不报错。
- [ ] 跨窗同步:设置改默认账号 → `emit(SETTINGS_APPLIED_EVENT)` → 主窗 `listen` 重读(照既有 theme/behavior)。
- [ ] `npm test`(vitest)+ `npm run build`(tsc)全绿;新增 `accounts.vitest.ts` 覆盖纯函数(解析 A2 输出/缓存 TTL/降级判定/session→徽章映射/defaultName 读写)。
- [ ] **不做**:不改 `remote-launch.ts`(A4)、不加"用 X 重启"写动作(A5)、不做部署 apply(A6)、不碰本地会话切号(A7)。

## 对接主计划 / 共享面
- **消费 A2 的 `list_remote_accounts`/`list_remote_session_accounts`**(`src-tauri/src/accounts.rs` 返回结构)。字段以 A2 定的 camelCase 为准。
- **账号模型收敛在 `src/accounts.ts`**(账本 §cc-monitor 账号模型):#68 发现/显示/切换的前端实现落此;A4 的注入、A5 的重启都从这里取账号。
- **config.json 新顶层键 `accounts`**:`{ defaultName?: string }`(账号列表本身**不落盘**,远端 manifest 才是真相;只缓存)。照 `remote.hosts[]` 的 read/write 范式(`settings/remote-section.ts`)。
- **不触** daemon(A2 已冻)、`remote-launch.ts`(A4)。

## 数据模型(§8)
```ts
// src/accounts.ts
interface Account { name; email; configDir; isDefault; mode; exists; loggedIn; }  // 对齐 A2 RemoteAccount
interface AccountsState {
  available: boolean;        // A2 的 available;false → 隐藏账号 UI + 显示原因
  error?: string;
  meta?: { enabled; acctsDir; manifestPath; updatedAt; sharedStore; count };
  accounts: Account[];
  defaultName?: string;      // config.json accounts.defaultName;缺省跟随 manifest isDefault
}
// per-origin:多台远端各一份;本地(origin===null)无账号态
```
- `defaultName` 语义:cc-monitor 记的"新会话该用谁"。缺省 = manifest 里 `isDefault` 的那个。UI 改它 = 写 config.json,**不碰远端 manifest**(远端默认由 cc-acct-iso 管;两者可不同,以 cc-monitor 的 defaultName 为"我这台机器起会话时的选择")。
- 会话徽章数据源(§3 优先级):① `session_accounts` 的 live 探测(硬真相)② A4 会加的 `lastAccount`(本 feature 只读、可先不接)③ 未知 `—`。

## 交互细节(照 DESIGN §4,不重复;这里只记落点)
- 状态栏 chip:`main.ts` chip 区(455-510 旁)新增;浮层选单照 `showSftpHostPicker`(708-745)。
- 设置组:`settings/panel.ts:buildBody` 把 `remote-placeholder` 换成 `AccountsSection`(新 `src/settings/accounts-section.ts`,照 `remote-section.ts`/`mcp-section.ts` 导出 `element`)。
- 命令:`main.ts:buildCommands`(455 旁)按账号循环生成"切默认为 X"(照 snapshotSessions 那段 for)。
- 徽章:`tabs.ts:createTabButton`(1986)加 span;`updateTabButton`(2169)刷新;`snapshotSessions` 补 `account`。

## 降级判定(纯函数,vitest 锁死)
| A2 返回 | UI |
|---|---|
| `available:false, error 含"daemonless"` | 该台无账号 UI;徽章不显示 |
| `available:false, error 含"过旧"` | chip 显"daemon 需更新",点击→设置引导 |
| `available:true, meta.enabled:false` | chip"未启用",设置组显部署引导(A6 占位) |
| `available:true, enabled:true, accounts:[]` | chip"未启用"(有 manifest 无账号) |
| account.mode==="in-place" | 列表标注"逃生口模式,不支持切换",切换禁用 |
| account.loggedIn===false | 选单灰掉 + ⚠,点→"打开终端 /login"提示 |

## 测试策略(纯前端,无真远端)
`src/accounts.vitest.ts`(照 `remote-launch.test.ts`/`tabs.vitest.ts` 范式):
1. 解析 A2 `AccountsResult` → `AccountsState`(含 meta/accounts/available)。
2. 降级判定表(上面 6 行)逐行:给定 A2 返回 → 期望 UI 状态枚举。
3. TTL 缓存:第一次 fetch → 缓存;TTL 内再取 → 不重发;过期/手动刷新 → 重发。
4. `defaultName` 读写:config.json 无 `accounts` 键 → 跟随 manifest isDefault;写入后 → 读回一致;**写时枚举全字段**(防 remote-section 那条"静默丢字段"教训)。
5. session→徽章映射:`session_accounts` 行 → 每 sid 的徽章文本/tooltip;`account:null`→`—`;本地 origin→无徽章。
6. `mode:"in-place"` / `loggedIn:false` → 正确的禁用/警告标记。
- **DoD 硬门槛**:vitest 全绿 + `npm run build`(tsc 无 error) + 手过一遍降级路径(未迁移时 chip 显"未启用"不报错)。

## 逐条实现步骤(Phase C)
1. `src/accounts.ts`:类型 + `fetchAccounts(origin)`/`fetchSessionAccounts(origin)` invoke 包装 + TTL 缓存 + `getDefaultName`/`setDefaultName`(config.json)+ 纯函数 `deriveUiState`/`sessionBadge`。
2. `accounts.vitest.ts` 6 组(mock invoke + config)。先测后接线,锁行为。
3. 状态栏 chip(`main.ts`)+ 浮层选单 + toast。
4. `src/settings/accounts-section.ts` + 挂进 `panel.ts`;跨窗 `SETTINGS_APPLIED_EVENT`。
5. Ctrl+K "切默认为 X" / "管理…"。
6. tab 徽章(`createTabButton`/`updateTabButton`/`snapshotSessions` + `GridSessionSnapshot`)。
7. 降级路径手验 + `npm test` + `npm run build`。

## 审计结果(D/E,实现后填)
- 待填。

## 签收
- [ ] 过代码审计(D) · [ ] 过工程审计(E) · [ ] 主计划已更新(F) · [ ] 测试绿
