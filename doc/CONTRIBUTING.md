# 贡献者操作手册

给 cc-monitor 添砖加瓦前先读这份。包含：
- **§ 1 撤回 / 修改 checklist** — 删功能时必跑的项
- **§ 2 添加新东西 cookbook** — 加 IPC / jsonl 类型 / 设置项 / 快捷键 / 跨进程协议的食谱
- **§ 3 发版 / CHANGELOG 规范** — 详见 [RELEASING.md](RELEASING.md)

不变量清单 → [INVARIANTS.md](INVARIANTS.md)。架构总览 → [ARCHITECTURE.md](ARCHITECTURE.md)。

---

## 1. 撤回 / 修改 checklist

### 1.1 撤回某个特性 / IPC 命令 / 跨进程协议

**全套必做**：

```bash
# 假设撤掉 BindRegistry State + cc_integration_status IPC
cd src-tauri

# 1. State 消费者全 grep
grep -rn 'State<.*Arc<BindRegistry>>' src/
grep -rn 'BindRegistry' src/

# 2. app.manage 调用全 grep
grep -rn 'app.manage(bind_registry' src/

# 3. IPC handler 注册全 grep
grep -rn 'cc_integration_status' src/

# 4. 前端 invoke 依赖全 grep
cd .. && grep -rn 'invoke<.*"cc_integration_status"' src/

# 5. 跨进程文件 IO 全 grep（如果撤的是文件协议）
grep -rn 'ps-await\|ps-registry' src-tauri/src/ src/

# 6. 删完跑：
cd src-tauri && cargo check && cargo test --lib
cd .. && npm run build

# !! cargo check 不能挡 State 漏 manage 的运行时 panic !!
# 必须额外起 dev mode 实测每个会消费 X 的 IPC 命令的前端入口
powershell -NoProfile -File scripts\run.ps1 dev
```

**为什么不能光靠 cargo check**：详 [STATE-MATRIX.md § 4.1](STATE-MATRIX.md#41-撤回某个-state-类型如删-bindregistry) — 漏 `manage` 是运行时 panic，类型系统抓不住。

### 1.2 修改跨进程协议（ps-await / ps-registry / sid-hwnd-cache / auto-launch）

修改 [IPC-PROTOCOL.md](IPC-PROTOCOL.md) 定义的任一文件 schema 都要：

- [ ] 改 **写入方** 代码（PS 端模板 `src-tauri/scripts/cc.ps1.tpl` 或 Rust 端 `bind.rs` / `profile_installer.rs`）
- [ ] 改 **读取方** 代码（serde struct）
- [ ] 更新 [IPC-PROTOCOL.md](IPC-PROTOCOL.md) 字段定义
- [ ] **向后兼容性**：旧文件应能被新版本读取（serde `#[serde(default)]` 字段新增 OK，删字段需 RFC）
- [ ] 编码必须 UTF-8 无 BOM（[INVARIANTS § 3](INVARIANTS.md#3-所有跨进程-json-文件--utf-8-无-bom)）
- [ ] 双端原子写

### 1.3 修改 jsonl 解析 (`messages.rs` / `parser.rs` / `cards/index.ts`)

- [ ] 后端 `JsonlRecord` enum 加 variant 用 `#[serde(rename)]` + `#[serde(default)]`
- [ ] `JsonlRecord::is_displayable()` 决定是否显示
- [ ] 前端 `cards/index.ts` `renderMessage` dispatch 表加分支
- [ ] 测试覆盖至少一个真实样本（放 `parser.rs` 的 `#[cfg(test)] mod tests` 里）

### 1.4 改 Tauri capability / permission

- [ ] 改 `src-tauri/capabilities/default.json`
- [ ] cargo build 后看 `src-tauri/gen/schemas/acl-manifests.json` 实际 permission set 内容确认
- [ ] dev mode 实测涉及的 IPC 不报 `Permission xxx not allowed`

**警示**：plugin 的 `<plugin>:default` permission set 通常**不包含所有** `allow-*`；某些 allow 默认空 scope 需要 inline 给 path/url pattern。详 [DEVELOPMENT.md § 查 capability 报错](DEVELOPMENT.md#查-capability-报错)。

### 1.5 发版前

- [ ] 改 **版本号三处对齐**（必做）：
  - `package.json::version`
  - `src-tauri/Cargo.toml::[package].version`
  - `src-tauri/tauri.conf.json::version`
- [ ] `Cargo.lock` 提交（Rust 应用必须锁版本）
- [ ] [CHANGELOG.md](../CHANGELOG.md) 加新版本段（写法见 [RELEASING.md](RELEASING.md)）
- [ ] `cargo fmt --check + cargo check + cargo test --lib + npm run build` 全绿
      （`.github/workflows/ci.yml` 第一步就是 `cargo fmt --check` 严格 verify；
      本地写完代码先 `cargo fmt` 一次再发版，避免 tag 推完才发现 CI 红需要补
      style commit 的尴尬。v2.0.0 就踩过这个坑）
- [ ] **手测关键 UI 入口**：
  - [ ] 启动 monitor，Tab 自动出现
  - [ ] 点 Tab ↗ 拉对应终端窗口
  - [ ] Ctrl+\` 同上
  - [ ] **Ctrl+H 历史浏览器打开**
  - [ ] Ctrl+, 设置面板打开，PowerShell 集成区扫描出 profile
  - [ ] 设置面板 hover 各个 `?` 图标，tooltip 在 viewport 内可见
  - [ ] 设置面板 [打开 profile]，资源管理器或编辑器弹出
  - [ ] /resume 一个历史会话
  - [ ] 用 cc 跑 claude，PowerShell 端到端，看 ps-await → ps-registry → sid-hwnd-cache
  - [ ] 装 cc 集成到一个**有自定义内容的 profile**，确认用户原内容保留 + 生成 `.ccm-backup-<ts>` 备份

### 1.6 Git 操作

- [ ] `git commit -m "release: vX.Y.Z"`（**不加 Claude coauthor**）
- [ ] `git tag vX.Y.Z`
- [ ] `git push origin main && git push origin vX.Y.Z`
- [ ] release.yml CI 跑过（约 6-8 min），产 NSIS + MSI + monitor.exe + SHA256SUMS

---

## 2. 添加新东西 cookbook

每个食谱给"动哪些文件 + 测试什么 + 检查清单"。

### 2.1 添加新 IPC 命令

**目标**：加一个 `monitor_get_active_ids() -> Vec<String>` 返回当前活跃 session id 列表。

**步骤**：

1. **后端** `src-tauri/src/lib.rs`（或独立 module 如 `stats.rs`）：

```rust
#[tauri::command]
async fn monitor_get_active_ids(
    session_map: tauri::State<'_, std::sync::Arc<session_map::SessionMap>>,
) -> Result<Vec<String>, String> {
    // SessionMap 提供哪些公开 API 见 src-tauri/src/session_map.rs；
    // 这里假设你需要的方法已存在，否则先在 session_map.rs 暴露一个。
    Ok(session_map.list_active_session_ids())
}
```

⚠️ **示例 API 是说明性的**，落地前必须 `cargo check` 确认 `SessionMap` 上真有 `list_active_session_ids()`，没有就先在 `session_map.rs` 暴露。当前 `SessionMap` 公开方法见 `documentSymbol src-tauri/src/session_map.rs` 或 `pub fn` grep。

2. **注册到 invoke_handler**（`lib.rs::run()` 内）：

```rust
.invoke_handler(tauri::generate_handler![
    /* ... 现有命令 ... */
    monitor_get_active_ids,    // ← 加这行
])
```

3. **如果用了 State**：去 [STATE-MATRIX.md § 2](STATE-MATRIX.md#2-消费者矩阵ipc-命令) 对应 State 下加一行 `lib.rs::monitor_get_active_ids(...)`。

4. **前端调用**：

```ts
const activeIds = await invoke<string[]>("monitor_get_active_ids");
```

5. **检查**：
- [ ] `cargo check + cargo test --lib`
- [ ] `npm run build` TS 编译过
- [ ] dev mode 实测命令真的能从前端 invoke 到（State 漏 manage 才能挡住）
- [ ] [STATE-MATRIX.md § 2](STATE-MATRIX.md) 表已更新

### 2.2 添加新 jsonl 记录类型

**目标**：claude 后续新加 `type=memory_recall` 记录，cc-monitor 要解析 + 渲染。

**步骤**：

1. **后端** `src-tauri/src/messages.rs::JsonlRecord` enum 加 variant：

```rust
#[serde(rename = "memory_recall")]
MemoryRecall {
    uuid: String,
    timestamp: String,
    #[serde(default)]
    content: serde_json::Value,
    #[serde(rename = "sessionId", default)]
    session_id: Option<String>,
},
```

2. **决定是否 displayable**：`impl JsonlRecord::is_displayable()` 加 match arm 返回 true（如果要渲染）或 false（如果只是 metadata 不显示）。

3. **`parser.rs` 测试**：加一行真实样本断言能 parse 成功。

4. **前端类型** `src/cards/index.ts` 或对应 type 文件加 TS 类型 + dispatch：

```ts
// 在 renderMessage 的 switch 里
case "memory_recall":
  return renderMemoryRecall(record);
```

5. **写渲染逻辑**：通常新建 `src/cards/memory-recall.ts` 包装成折叠卡 / 普通卡。

6. **检查**：
- [ ] `cargo test --lib parser` 通过
- [ ] 跑一个含该新类型的 jsonl 文件 → 前端能正常显示

### 2.3 添加新跨进程协议文件

如想加 `cc-monitor 状态心跳` 文件给 PS 端查 monitor 是否在跑：

详细步骤 → [IPC-PROTOCOL.md § 添加新的跨进程协议文件](IPC-PROTOCOL.md#添加新的跨进程协议文件)。

**关键**：
- 路径必须在 `~/.claude/claudecode-frontend/` 下
- UTF-8 无 BOM
- 原子写
- 反序列化容错（`#[serde(default)]` + `#[serde(other)]`）

### 2.4 添加新外观设置项

**目标**：在设置面板"颜色"分组下加一个 `--info` token。

**步骤**：

1. **CSS** `src/styles.css::root` 加 `--info: #6699cc;` 默认值 + 引用处替换字面量。
2. **TS 类型** `src/theme.ts::ThemeConfig` interface 加 `"info"?: string`。
3. **`TOKENS` 数组**加 `{ key: "info", category: "color" }`。
4. **设置面板** `src/settings/panel.ts::FIELDS` 加 `{ key: "info", label: "信息色", type: "color", group: "color" }`。
5. **检查**：
- [ ] 设置面板能看到新 token 字段
- [ ] 拖 color picker 实时预览生效
- [ ] 关闭 monitor 后重启，颜色保留

### 2.5 添加新全局快捷键

**目标**：加 `Ctrl+P` 打开命令面板（假设）。

**步骤**：

1. **`src/main.ts` keydown handler** 加 case：

```ts
if (e.ctrlKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "p") {
  e.preventDefault();
  commandPalette.toggle();
  return;
}
```

2. **冲突检查**：grep 所有 `keydown` listener 看是否别处也注册了 Ctrl+P。

3. **更新文档** [`../README.md`](../README.md) 快捷键表加一行。

4. **已实现**：v2.5 issue #5 落地，所有 chord 走 `src/keybindings/` dispatch table（actions.ts + dispatcher）。新加 action 应该加到 actions.ts 而非 main.ts keydown handler；本节描述的"直接在 main.ts 加 case" 已过时，仅作背景参考。

### 2.6 添加新 Tauri capability permission

**目标**：用 plugin-X 的某个 IPC，capability 没默认授权。

**步骤**：

1. **cargo build 一次** 让 `src-tauri/gen/schemas/acl-manifests.json` 重新生成。
2. **看 plugin-X 的 `permissions`** 找具体 `allow-foo` 的定义，看 description 是否需要 scope。
3. **`capabilities/default.json` 加 permission**：

简单（无 scope）：
```json
{ "permissions": [ "plugin-x:allow-foo", ... ] }
```

带 scope（多见，default 通常空 scope）：
```json
{ "permissions": [
  { "identifier": "plugin-x:allow-foo", "allow": [{ "scope_field": "pattern" }] },
  ...
] }
```

4. **dev mode 实测**：调用涉及 IPC 不报 `Permission xxx not allowed`。

---

## 3. PR 流程

1. fork → branch（命名 `feat/<short-desc>` / `fix/<short-desc>`）
2. 改代码 + 测试 + 文档（参照本文档对应 cookbook）
3. `cargo fmt + cargo clippy + cargo test --lib + npm run build` 全绿（动 `src/cards/diff.ts` 另跑 `npm run test:diff`；动 `src/branching.ts` / `branch-fold.ts` 另跑 `npm run test:branching`；动 `cards/api-error.ts` 另跑 `npm run test:api-error`）
4. PR 描述：
   - 解决什么问题（链到 issue）
   - 怎么解决（一句话）
   - 涉及的文件 + 变动量
   - 手测过哪些路径
5. 提 PR → 等 CI → review → merge

CHANGELOG / 版本号 / tag 由 maintainer 在 release 时统一处理，PR 不需要碰这些。
