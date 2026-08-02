# U5' · 打包完善：本机分发链

- 工作区：unified-backend · 主计划 §3 第三梯队（**替代被砍掉的 U5**）· 任务 #93
- 风险档：**中**（不动 daemon 内核；动打包面与一条新的本机写盘路径）
- 由来：用户 2026-08-01「本机 backend 生命周期不管，我后面装新版再自己搞
  （**前提是你把东西都打包完善了**）」。⇒ sidecar/监督/自愈划掉，**打包面变成硬要求**。

## Phase B 摸底：今天装新版，本机拿到什么

| 事实 | 实测 |
|---|---|
| `.deb` / `.msi` 里有什么 | **只有 app 二进制**。`tauri.conf.json` 的 `bundle.resources` 与 `externalBin` **都是空的** |
| `ccm` | destination `RemoteHomeRelative(".local/bin/ccm")` + `HostScope::Remote` ⇒ **只往远端装** |
| `cc-bus` | `LocalHomeRelative` 但 **`installable: false`** ⇒ app 根本装不了 |
| `cc-acct-iso` | 实际部署走远端 SFTP（`deploy_remote_acct_iso`） |
| 全部安装命令 | `deploy_remote_daemon` / `install_remote_ccm_helper` / `deploy_remote_acct_iso` / `write_remote_mcp_server` —— **无一本机** |

⇒ **装新版之后本机一个工具都不会被安装或更新。** 用户报的「terminal 里敲命令还是 attach」
只是这件事的一个症状：他机器上那份 `ccm` 停在 2026-07-27（`CCM_VERSION=1`，
capabilities 缺 `detach,tmux-size`），此后所有 ccm 修复它一个都没收到。

### 好消息：字节已经在包里了

`CCM_CLI_SCRIPT`（`include_str!("../../shared/ccm")`）与 cc-acct-iso 的
`include_bytes!` **早就编进 app 二进制**了 —— 远端部署正是从这里取字节。
**缺的只是「往本机写」这条路径**，不是「把东西塞进包」。

⇒ 这让 U5' 比想象的小：不必动 `bundle.resources`（那会引入第二份真相源），
复用已有的内嵌字节 + 一条本机 destination 即可。

### 版本可见性：原料有，没人给本机看

`ccm --ccm-probe` 会打 `version=<CCM_VERSION>` 与 capabilities 行。
仓里 `2` / 机器上 `1` —— **信号本身是准的**，问题是 app 只对**远端** ccm 做这件事
（`machine-card.ts` 有远端的装/卸按钮），本机那份从来没人查过。

⚠ **但 `CCM_VERSION` 不够**：它是**能力协议版本**，只在能力变动时 bump。
`666cc14`（attach 那条修复）**没有**改变能力面 —— 如果它没顺带 bump，版本号就看不出差别。
⇒ DoD ② 不能只比 `CCM_VERSION`，要比**内容指纹**。

## DoD

| # | 项 | 验收 |
|---|---|---|
| ① | **本机能装**：三个工具至少 `ccm` 有本机安装路径 | 一条 tauri command + 前端入口；写 `~/.local/bin/ccm`（`0o755`），字节取自已内嵌的 `CCM_CLI_SCRIPT` |
| ② | **装完看得出对不对** | 本机 ccm 的状态里显示**内容指纹**（app 内嵌那份的 hash vs 磁盘那份的 hash），不只比 `CCM_VERSION`。变异：把磁盘那份改一个字节 ⇒ 状态必须显示「不一致」 |
| ③ | **不一致时说得出差在哪** | 至少告诉用户「你这份是旧的 / 我这份是新的」，而不是只说「不一致」 |
| ④ | 文档写清手工步骤且**可照做** | 一条能直接粘的命令 + 它做了什么；不是「见某某文档」的转指 |
| ⑤ | 全量门禁绿 + 本机写盘路径有护栏 | 写盘只许落 `~/.local/bin/ccm` 这一个路径，且**不许覆盖非 ccm 的文件**（防打错路径毁东西） |

**不做**（用户明确划掉）：Tauri sidecar · 启停监督 · 崩溃自愈 · 监听型传输。
**也不做**：不动 `bundle.resources`（内嵌字节已是单一真相源，加 resources 会变两份）·
不写用户 `~/.bashrc`（红线；别名块归用户自己）· 不装 `cc-bus` / `cc-acct-iso` 到本机（本轮只收 `ccm`，
它是用户当下真正被卡住的那个；另两个登记）。

## 与主计划对接

- **账本 S15「本机分发链」** —— 本功能交付它的工具面那一半（daemon 本机分发是 U5，已砍）。
- **开放-6** —— 本功能就是它的落点，做完销号。

## 逐条实现步骤

1. `tool_registry` 加 `ccm` 的**本机** destination + `HostScope::Local`（或新增一条本机项）。
   *验证*：`tool_registry` 的既有守卫（`parity_ledger` / 字段纪律）不红。
2. 新增 tauri command `install_local_ccm()`：写 `~/.local/bin/ccm`，`0o755`，
   **原子写 + 覆盖前校验目标确实是 ccm**（或不存在）。
   *验证*：单测覆盖「目标是别的文件 ⇒ 拒绝」。
3. 状态查询 `local_ccm_status()`：返回 `{ installed, disk_fingerprint, embedded_fingerprint, disk_version, embedded_version }`。
   *验证*：变异 —— 改磁盘那份一个字节 ⇒ 指纹不一致。
4. 前端入口（设置面板本机区）：显示状态 + 一个「安装/更新」按钮。
5. 文档：`doc/` 里写清手工路径（一条可粘命令）+ 为什么本机与远端是两条链。
6. 全量门禁。

## 测试策略

- **绝不碰用户真实的 `~/.local/bin/ccm`**：所有测试走 `$HOME` 覆盖或 tempdir。
- 变异一律退出码判定；`cp -a` 还原后 `touch`。

## 实现期与计划的偏离

（待填）

## 代码审计结果（D）

（待填）

## 工程审计结果（E）

（待填）

## 签收

- [ ] 过代码审计（D）
- [ ] 过工程审计（E）
- [ ] 主计划已更新（F）
