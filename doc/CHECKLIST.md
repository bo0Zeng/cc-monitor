# cc-monitor 操作 Checklist

> **强制 checklist**。下面 4 类操作做之前先翻这里，做完逐条对账。
>
> 项目历史上的 release-blocker 全都是绕过了某条检查导致的：
> - v1.6.7 撤回漏 grep IPC State 依赖 → 5 个版本带病
> - v1.7.2 PS profile 文件名搞错 → cc 集成形同虚设
> - v1.7.3 一键安装覆盖用户已有 cc → 破坏用户配置
>
> 单测全过 ≠ 没 bug。`cargo check` 全过 ≠ 没 bug。**装上才发现**的 bug 都是漏 checklist。

---

## 1. 撤回 / 删除功能 checklist

撤任何 `pub fn` / `pub struct` / `#[tauri::command]` / `app.manage()` 之前**全部走完**：

```bash
THING="bring_terminal_to_front"  # 你要撤的东西
cd cc-monitor

# 1. 直接引用
grep -rn "$THING" src-tauri/src/ src/

# 2. 如果撤的是 State 持有的类型（如 BindRegistry），grep State 参数
grep -rn "State<.*$THING" src-tauri/src/
grep -rn "tauri::State.*$THING" src-tauri/src/

# 3. grep app.manage（看是否能整体删 manage）
grep -rn "app\.manage" src-tauri/src/

# 4. 跨线程 Arc clone 检查
grep -rn "${THING,,}\.clone\(\)\|${THING,,}_for_" src-tauri/src/

# 5. 前端 invoke 检查
grep -rn "invoke<.*\"$THING\"" src/

# 6. 文档同步
ls doc/STATE.md doc/CONTRACTS.md   # 是否在文档里登记过

# 7. 跑全套
cd src-tauri && cargo fmt --check && cargo check && cargo test --all && cd ..
npx tsc --noEmit

# 8. !! cargo check 挡不住 State 漏 manage 的运行时 panic !!
#    必须额外：起 dev mode（npm run tauri dev 或 cargo run）+ 实测每个相关 UI 入口
#    - 触发本撤回功能的 UI 入口（点 Tab ↗ / 快捷键 / 设置面板按钮）
#    - 跟本功能"看似无关"但实际共享 State 的 UI 入口（参见 doc/STATE.md）
```

**核心教训**：撤回时不能光看你直接想撤的代码。**翻 `doc/STATE.md` 看共享 State 的其他消费者**。

---

## 2. 添加新 IPC 命令 checklist

```rust
#[tauri::command]
fn new_command(args: ..., state: tauri::State<'_, Arc<MyState>>) -> Result<T, String> { ... }
```

加这种东西时：

- [ ] **lib.rs** 的 `invoke_handler![]` 列表加 `new_command`（编译期错误能挡漏注册）
- [ ] 如果接 `State<X>`：**确认 `app.manage(x.clone())` 在 setup 已注册**（编译期挡不住！必须看 `doc/STATE.md`）
- [ ] **doc/STATE.md** § 2 消费者矩阵添加一行
- [ ] 前端 `invoke<T>("new_command", { args })` 一处调用
- [ ] 单测覆盖命令的纯逻辑部分（State 部分用 `tokio::task::spawn_blocking` 隔离时无法单测，靠手动）
- [ ] **CHANGELOG**：新增功能行
- [ ] **实测**：dev mode 跑一次，点对应 UI 触发该命令，确认不报 "state not managed"

如果命令是 async / 内有阻塞调用：
- [ ] 用 `async fn` + `tokio::task::spawn_blocking` 包阻塞调用（避免堵 IPC 主线程，参考 v1.6.5 教训）

如果命令是 sync 但有 5s+ 操作：
- [ ] 必须改 async + spawn_blocking（同上）

---

## 3. 添加 / 修改跨进程文件协议 checklist

cc-monitor 跟 PowerShell（cc.ps1.tpl）/ Claude Code 通信靠几个 JSON 文件，schema 改动需要双端同步：

| 文件 | 谁写 | 谁读 |
|---|---|---|
| `~/.claude/sessions/<PID>.json` | Claude Code（**外部**，不可控） | monitor `session_map::SessionInfo` 反序列化 |
| `~/.claude/claudecode-frontend/ps-await/<PID>.json` | cc.ps1.tpl 写 | monitor `bind::AwaitRequest` 反序列化 |
| `~/.claude/claudecode-frontend/ps-registry/<PID>.json` | monitor 写 | monitor 自己读（重启加载） |
| `~/.claude/claudecode-frontend/sid-hwnd-cache.json` | monitor 写 | monitor 自己读 |
| `~/.claude/claudecode-frontend/auto-launch.json` | monitor 启动时写 path / UI toggle 写 enabled | cc.ps1.tpl 读决定是否 launch monitor |
| `~/Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1` | UI 安装写 BEGIN/END 块 | PowerShell 启动加载 |

**改任何字段之前**：

- [ ] **加字段**：用 `#[serde(default)]` 确保旧版本仍能解析新字段
- [ ] **删字段**：先用 `#[allow(dead_code)]` deprecate 一两个版本，再真删
- [ ] **重命名字段**：等价于"加新字段 + 删旧字段"，分两次发版处理
- [ ] **改字段类型**：必须考虑迁移路径（旧文件能否被新代码读？反之？）

特殊：
- **sessions/<PID>.json 是 Claude Code 写的**，**不可控**。schema 变化你只能跟随，不能反推
- **cc.ps1.tpl 字段** 在两侧定义：PS 端 `@{ ps_pid; marker; proc_start }` ↔ Rust 端 `AwaitRequest { ps_pid; marker; proc_start }`。一改两边都要改

---

## 4. 发版 checklist（vX.Y.Z）

每次 bump 版本 + 打 tag 前：

### 代码侧
- [ ] `cargo fmt --check` 干净
- [ ] `cargo test --all` 全过
- [ ] `cargo check` 0 错误（warning 可接受但每次发版应顺手清）
- [ ] `npx tsc --noEmit` 干净
- [ ] `npm run build` 跑过（vite 产物可生成）

### 版本号一致
- [ ] `package.json` `"version"`
- [ ] `src-tauri/Cargo.toml` `[package].version`
- [ ] `src-tauri/tauri.conf.json` `"version"`
- [ ] `Cargo.lock` 已重新生成（`cargo check` 自动）
- [ ] `CHANGELOG.md` 顶部加新版本条目

### CHANGELOG 必须包含
- [ ] **修复** / **新增** / **改动** / **测试** / **移除** 至少一类
- [ ] 如果有用户操作影响（UI / 命令 / 文件协议）→ 加 **用户操作流** 段
- [ ] 如果是修复"前面版本带病的 bug" → 写清"v1.X.Y 起的回归 / 漏洞"
- [ ] 如果删 / 移 / 改了 State / IPC → **同时更新 doc/STATE.md**
- [ ] 如果删 / 改了跨进程协议 → **同时更新 doc/CHECKLIST.md § 3 表格**

### 实测（关键 UI 入口）
当前 v1.7.4 至少必测：
- [ ] 启动 monitor，看 Tab 自动出现（jsonl-watcher + SessionMap）
- [ ] 点 Tab ↗ 拉对应终端窗口（`bring_terminal_to_front` 走 SidHwndCache）
- [ ] Ctrl+\` 同上
- [ ] **Ctrl+H 历史浏览器打开**（这就是 v1.6.7→1.7.4 漏的，调 `list_history_projects`）
- [ ] Ctrl+, 设置面板打开，看 PowerShell 集成区扫描出 profile（`cc_integration_status`）
- [ ] /resume 一个历史会话（`resume_history_session`）
- [ ] 用 cc 跑 claude（PowerShell 端到端，看 ps-await → ps-registry → sid-hwnd-cache）

### Git 操作
- [ ] `git commit -m "release: vX.Y.Z"` （不加 Claude coauthor）
- [ ] `git tag -a vX.Y.Z -m "Release vX.Y.Z (xxx)"`
- [ ] `git push origin main && git push origin vX.Y.Z`
- [ ] release.yml CI 跑过（约 6-8 min），产 NSIS + MSI + monitor.exe + SHA256SUMS

---

## 5. CHANGELOG 写作规范

格式参考 Keep a Changelog 1.1.0 + 项目定制：

```markdown
## [X.Y.Z] — YYYY-MM-DD

### 修复（release-blocker / 一般）
- **问题简述** —— 详细描述
  - 根因：xxx
  - 影响版本：xxx
  - 修法：xxx

### 新增
- **功能名** —— 实现 xxx，see `path/to/code.rs`

### 改动
- **改的事** —— xxx 改成 yyy，原因：xxx

### 移除
- **删的东西** —— 替代方案：xxx
- 同步更新 `doc/STATE.md`：xxx

### 影响 / Breaking Changes（如有）
- 用户：xxx
- 代码：xxx

### 迁移路径（如有）
- 从 vX.X.X 升级：xxx

### 测试
- 单元测试 N → M（新增 xxx）

### 用户操作流（如功能改动需要用户配合）
1. xxx
2. xxx
```

---

## 6. 同步规则

**本 checklist 跟仓库代码一同 maintain**。任何流程改动 / 新踩坑：
- 加到对应 §
- commit 跟代码改动放一起，让未来 git blame 能定位决策

未对齐的 PR 应该在 code review 时打回。
