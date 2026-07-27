# MASTERPLAN v2 — 统一会话启动 + 双账号集成 + 账号 UX

综合两版架构设计（启动路径 a1c53887 / 账号模型 ab0ebd3e）+ 用户 R1-R7 + 模型维度决策。红线全解（R6），目标=统一整个软件、架构做干净。已完成并 commit：F5 一键部署(9d3a7e6)、F1 切号入口(7680a43)。

## 一、统一架构（两层咬合）

```
调用点 (tabs.ts / history.ts)          组装意图
   │
账号解析层 AccountResolver              intent → ResolvedAccount({configDir,name}|null)
   │   isEnabled(注入门) / isBadgeActive(徽章门)  ← 两谓词必须分开
   ▼
LaunchSpec  {mode, backend, cwd, account, launcher, sid?, tmuxName?, extraEnv?, ...}
   │   ★可扩展：新维度=加字段+加修饰器，绝不新开 builder
   ▼
buildLaunch(spec)                       校验/派生 → 修饰器链装配 payload → 选投递动词
   │   修饰器链(有序)：accountPrefix(10) → envReset(20) → scrubNested(30)
   │                  → extraEnv/proxy/model(40) → rbind(90) → launcher(100) → resumeFlag(110)
   ▼
SESSION_BACKEND 座                      命令语法唯一来源 + create 分支 set @ccm_sid
   │   createRunAttach / runInExistingAttach / attach
   ▼
runLaunch(origin, spec)                 唯一执行器（取代 6 个 runRemote*）
   ▼
launch.rs → ssh -t bash -lic '<payload>'
控制面 tmux.rs kill/send-keys           守卫改：@ccm_sid 命中即放行（不再看名前缀）
```

**双轴分解**（架构核心）：启动 = **载荷装配（backend 无关，修饰器链）** × **投递（backend 相关，座）**。cwd/tmux 语法沉进投递层，修饰器链保持纯粹。

## 二、关键设计决策

### D1. LaunchSpec 可扩展（R5）
```ts
interface LaunchSpec {
  mode: "new"|"resume"|"restart"|"attach";
  backend: "tmux"|"direct";
  cwd: string;
  account: {configDir: string}|null;   // null=基座退化态
  launcher: string;                     // 默认 "claude"，用户可配
  sid?: string; tmuxName?: string;
  reuseExisting?: boolean; resetStaleEnv?: boolean;
  extraEnv?: Record<string,string>;     // ★模型/代理/未来维度都走这里
}
```
新维度 = 加字段 + 注册一个 `PayloadModifier{id,order,applies,fragment}`。零改 builder、零改入口。

### D2. account=null 的**非对称**（易错，必须写进契约）
- 新建子 shell（new/resume-create）→ 空前缀 `""`（与旧载荷逐字节相同）。
- **就地复用已存在 shell** → 必须主动 `unset CLAUDE_CONFIG_DIR;` 清残留旧账号 env（remote-launch.ts:186）。

### D3. 两个谓词必须分开（服务 R2）
| 谓词 | 判据 | 驱动 |
|---|---|---|
| `isEnabled` | manifest enabled | **是否注入 CLAUDE_CONFIG_DIR** |
| `isBadgeActive` | ≥2 可选账号 | **徽章/颜色显隐**（R2：单账号不显） |
今天 `accountColorsActive` 被误当两用，必须拆——因为**状态②（单账号但已装 iso）要注入但不显徽章**。

### D4. ★语义抬升（有意破坏旧逐字节行为，R6 许可）
现在「default 意图」**从不注入**（accounts.ts:459）。迁移到 iso 后这会让裸 claude **丢登录**。改为：**isEnabled 即注入 effectiveDefault**。严格用 `isEnabled` 门（非 isBadgeActive），否则状态①回归。

### D5. @ccm_sid 两通道并用（修根因 1/3/4）
- **通道A 建时显式 set**（已知 sid=resume/restart）：座 create 分支 `set-option @ccm_sid <sid>`（已有）。
- **通道B wrapper poller 回填**（未知 sid=new，且跟 /branch 漂移）：launcher 核心统一套 `( __ccm_rbind; exec <launcher> )`。
- `rbindModifier` 受「wrapper 已部署」门控——**未部署则退回裸 launcher**（否则 command not found 会话立死，这是硬前置）。

### D6. 白名单 cc-* → @ccm_sid 命中（修根因5）
`kill_remote_tmux`/`tmux_send_keys` 加 `expected_sid`，后端复核 `@ccm_sid` 而非名前缀 → 终端 `<dir>_cc` 也能重启；顺带修 list→kill 之间的名字复用 TOCTOU。名字不再承担身份职责，命名分歧自然和解。

### D7. wrapper 账号感知（修根因2）
`${CLAUDE_CONFIG_DIR:-$HOME/.claude}/sessions/$cpid.json`——shell 层的 account=null 退化。**lockstep 4 处**：shared 单一源 / sftp.rs include_str / remote-section raw import / 漂移守卫测试（**要加 needle**）。

### D8. 每账号默认模型（用户决策：走 env 覆盖 + 纳入架构）
- **不隔离 settings.json**（否则 cc-bus hooks/权限/主题被迫维护两份）。
- 账号→模型映射存 cc-monitor，起会话时经 `LaunchSpec.extraEnv` 注入 `ANTHROPIC_MODEL`。
- 这是 D1 可扩展性的**第一个真实用例**（验证「加字段+修饰器」范式）。

### D9. 终端同步
cc/ccm/cct 收进 shared 脚本 + `${CCM_LAUNCHER:-claude}` 可配；**启动器不自设 CLAUDE_CONFIG_DIR**，由 env 决定单/多账号 → 同一套脚本零分支、按 env 分叉。

### D10. 旧 swap 退役（不静默改 rc）
bashrc `_cc_acct` 凭据 swap 与 iso 冲突（会往共享库写凭据、污染隔离）。cc-monitor **检测**（BEGIN/END 围栏 + `_cc_acct` 标记）→ 冲突横幅 + 删除指引；托管删除需用户显式确认（复用 strip_profile_block 安全范式）。

## 三、三态兼容矩阵（证明同一套路径无回归）

| | ① 单账号·未装 iso | ② 单账号·已装 iso | ③ 多账号 |
|---|---|---|---|
| isEnabled | false | **true** | true |
| isBadgeActive | false | false | true |
| resolve(default) | `null` | **effectiveDefault.configDir** | 选中/current |
| launch 载荷 | 裸 claude（**逐字节旧**） | 注入默认号（否则丢登录） | 注入选中号 |
| attach/resume/restart | 同 builder，account=null | 同 builder | 同 builder |
| @ccm_sid | wrapper 读 $HOME/.claude ✓ | wrapper 读账号目录 ✓ | ✓ |
| 徽章(R2) | 不显 ✓ | **不显** ✓ | 每 tab 常显 ✓ |

## 四、功能顺序 v2

1. **U0 启动统一（地基，修 restart/resume）**：LaunchSpec+修饰器链+buildLaunch；4 builder→薄适配器（**黄金串逐字节等价护栏**）；6 executor→runLaunch；调用点迁移。
2. **U1 @ccm_sid 三修**：rbind 通道B（门控）+ 白名单改 @ccm_sid + wrapper 账号感知（lockstep）。→ 真机验：cc-monitor 新起的会话可 restart/resume；终端 cct 会话可重启。
3. **U2 账号解析层**：AccountResolver + 拆两谓词 + D4 语义抬升。→ 真机验状态②不丢登录。
4. **U3 终端同步 + 旧 swap 检测退役**（D9/D10）。
5. **U4 tab 徽章多账号即常显**（R2，依赖 U2 的 isBadgeActive）。
6. **U5 右键菜单分级 flyout**（R4，悬停+点击都行）。
7. **U6 每账号默认模型**（D8，验证可扩展性）。
8. **F2′ 全清对齐 UI**（强制全局已砍 → ⇄/⚠k/alignAll/countAccountMismatches/命令面板对齐 全删）。
9. **F3 面板砍卡片** → **F4 加号一键化** → **F7 用量(plan 窗口%)** → **Phase G**。

## 五、Top 风险
1. **黄金串是唯一等价保证**——U0 动手前必须把 4 builder 现有输出全部钉成黄金串（尤其 A3′ 的 envReset 分支）。
2. **rbind 门控是硬前置**——无门控切 new 会让没装 wrapper 的远端会话立死。
3. **本机 ~/.bashrc 的 __ccm_rbind 是手抄副本、无本地安装器**——改 shared 不会自动同步本机 → 最大漂移点，需检测+引导重装（不偷改）。
4. **D4 语义抬升是有意行为变更**——须严格用 isEnabled 门 + 真机验三态。
5. **wrapper/TMUX_LS_FMT 双写点 lockstep**——漂移守卫测试要加 needle。
6. 座是阶段①占位（daemon RPC 阶段②会把同步串变异步句柄）——executor 吃 spec 而非串已留缝。

## 六、仍守的用户偏好（非架构红线）
不用 emoji · git commit 无 Co-Authored-By · 不静默改 ~/.bashrc（生成+提示或显式确认后托管）。
