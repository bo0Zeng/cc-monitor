# 界面（`src/frontend/ui/`）

TypeScript ＋ Vite，原生 DOM，不引框架。界面只排版后端给的成品、收手势、开终端窗口：不判定、不拼命令串、不缓存业务数据（[ARCHITECTURE §2.1](doc/ARCHITECTURE.md)）。

相关清单：壳（Rust）[`src/frontend/shell/README.md`](frontend/shell/README.md) · 后端 [`src/backend/README.md`](backend/README.md)。下表的文件名都相对 `src/frontend/ui/`；每份文件顶上的头注是那一份的说明，这里只按用途分组指路。

## 入口

三个窗口各一个入口（理由见 ARCHITECTURE §5「每个窗口一个入口」）：

| 页 | 入口 |
|---|---|
| `index.html` 主窗口 | `entry-main.ts` → `main.ts` |
| `settings.html` 设置窗 | `entry-settings.ts` |
| `viewer.html` 只读窗 · agent 窗 | `entry-viewer.ts` |

共用的启动件在 `entry-common.ts` · `entry-render-common.ts`。

## 按用途分组

| 用途 | 文件 |
|---|---|
| 对后端只有两个动作 | `call` / `subscribe` 的客户端是 `src/comms/inward/chan.ts`；`origin` 的判定只在 `ipc/origin.ts`；Tauri 命令只经 `ipc/commands.ts` 调（全仓唯一直接 `invoke` 的地方）；读回包的解码在各 `*-reads.ts` 与 `ipc/decode.ts` |
| 界面状态 | `app-store.ts`（当前机器 · 账号快照 · 当前 tab）· `tab-store.ts` ＋ `tab-router.ts` · `overlay-router.ts` |
| 会话流管线 | `events.ts`（订会话流、还 credit、按行号补 gap）→ `tabs.ts`（`TabManager`）→ `render-stream-record.ts` → `record-timeline.ts`（按 `seq` 二分插入）→ `stream.ts`（守卫式贴底）→ `render.ts`（marked · KaTeX · highlight.js · DOMPurify） |
| 视口外不渲染 | `skeleton-view.ts`（有会话索引的 tab 与查看器）· `live-window.ts`（`TailWindow`，只物化尾部）· `height-estimate.ts`（估高，绝不许抛）· `render-window.ts` |
| 卡片 | `cards/`（`index.ts` 是分发器；卡型由后端判好随记录来，界面按卡型排版） |
| 主线外折叠 | `branch-fold.ts`（按后端给的主线外清单折，界面不判谁在主线上） |
| 视图 | `views/`（历史页 · 只读查看器 · agent 窗 · 计划页 · 命令面板 · 并排监控 · 会话内查找 …） |
| 设置窗 | `settings/`（`router.ts` 分页；每页一份 `*-page.ts` / `*-section.ts`） |
| 组件 | `kit/`（按钮 · 菜单 · 弹层 · 提示 · toast …，每件配一份 CSS 模块） |
| 快捷键 | `keybindings/`（`actions.ts` 是动作清单的唯一源头，`registry.ts` 派发与弹层栈） |
| 文案 | `copy-table.ts`：对外文案的唯一取文口，表住 `src/shared/copy/table.json`（键必须是字面量） |
| 出错上屏 | `backend-errors.ts`（收壳的 `monitor-error` 事件，照那一句弹 `kit/toast.ts`） |
| 本地存储 | `local-storage.ts`（键一律前缀 `cc-monitor.`，INVARIANTS §14） |
| 生成物 | `generated/`（Rust 类型经 ts-rs 导出，`npm run gen:types` 重生成，勿手改） |
| 样式 | `styles/`（全局 token 与布局）· 组件旁的 `*.module.css` |

## 要加什么去哪

| 要加 | 去哪 |
|---|---|
| 新的记录类型 / 卡型 | 后端先判好卡型（[CONTRIBUTING §2](doc/CONTRIBUTING.md)），界面在 `cards/` 下加排版 |
| 新的设置项 | [CONTRIBUTING](doc/CONTRIBUTING.md) 的设置项那一节 |
| 新的全局快捷键 | `keybindings/actions.ts` 加一行，`main.ts` 里 `dispatcher.bind(id, cb)` |
| 调一条后端命令 | 经 `chan.call(origin, op, payload)`；回包解码写在对应的 `*-reads.ts` |
| 调一条 Tauri 命令 | 只在 `ipc/commands.ts` 里加一个包装 |

界面相关的不变量：§12（alert 不算错误反馈）· §13（浮层真挂 `document.body`）· §14（本地存储键前缀）· §21（启动重放贴底不抖）· §25（行投递至少一次，按 uuid 累积的模块自行幂等）。全集在 [INVARIANTS.md](doc/INVARIANTS.md)。
