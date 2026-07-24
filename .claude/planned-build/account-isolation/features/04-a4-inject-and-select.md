# A4 — 按账号启动 / resume（CLAUDE_CONFIG_DIR 注入 + 选账号 UI + lastAccount 记忆）

> 注：本文件为**回溯补建**（2026-07-24）。上个 session 声称建过但磁盘上没有（D 计划符合度审计发现），
> 实现已先于文件完成。这里以磁盘代码为准，记录 A4 的真实 DoD / 落点 / 审计 / 签收。

A4 = cc-monitor 侧「切账号语义②：按会话选账号起/resume」。账号 = 一个 `CLAUDE_CONFIG_DIR`；
切账号 = 起 Claude 进程时注入该 env（进程级、起时定，运行中不可改）。远端优先（账号库在远端 Linux）。

## DoD（全部已达成）
- [x] **注入核心** `src/remote-launch.ts`：`buildEnvPrefix(configDir?)`（空 → `""` 逐字节等价旧载荷；非空 → `export CLAUDE_CONFIG_DIR=<posixQuote>; ` 拼在 `unset` 之前）+ `isValidConfigDir`（绝对路径 / 无 `..` / 拒 shell 元字符 + C0/DEL/C1 控制符 + 可欺骗 Unicode，与 daemon `is_safe_config_dir` 对齐）。三 builder（direct/tmux/launcher）+ 四 runner 加可选末位 `configDir?`。
- [x] **回归门**：无 configDir → 三 builder 输出逐字节等于旧版（`remote-launch.test.ts`）。
- [x] **lastAccount 记忆（源②）**：Rust `history.rs` `EntryMetadata.last_account` + `MetadataPatch`（三态，plain serde default）+ update 分支 + 只读命令 `list_last_accounts`（sid→lastAccount）注册进 lib.rs；前端 resume 后 `recordLastAccount` 写；徽章读（main.ts 拉 → tabs）。
- [x] **sessionBadge 三源优先级（§3）**：源① live 探测（死 live 不算）→ 源② lastAccount（标"上次用本工具起"）→ 源③ `—` 不猜。
- [x] **§7 徽章门控** `shouldShowAccountBadge`：本地 / daemonless / 未迁移 / 旧 daemon 不显徽章（避免满屏 —）。
- [x] **选账号 UI**（四落点）：历史行右键 `appendAccountResumeItems`、搜索卡片右键（showEntryMenu）、归档远端 tab 右键（`peekSelectableAccounts`）、「开新 Claude」对话框账号下拉（remote-section.ts，预选 effectiveDefault）。
- [x] **统一编排** `withAccount(origin, name, run, opts)`：三站点共用 resolve configDir + 不可选 toast 降级默认 + 记 lastAccount（有 sid 才记）。**A5 换号重启是它的超集。**
- [x] **全量验证**：tsc 0 / `npm test` 34 vitest 417 测 + 全 tsx / `cargo test --lib` 350 / `npm run build` ✓。
- [x] **真机零改动**：唯一写路径 `recordLastAccount → update_history_metadata` 落 `<monitor_data_dir>/history-metadata.json`，**不碰用户 `~/.claude`**；注入只在远端命令拼 `export`，不写凭据/账号态。
- **不做**：本地会话切号（A7）；换号破坏性重启 + compact（A5）；部署 apply（A6）。

## 对接主计划 / 共享面
- 改到共享面：`remote-launch.ts`/`remote-launch-run.ts`（加 configDir，空则等价旧行为=最终形态）、`tabs.ts`（setSessionAccounts 加 lastAccountByS/readyOrigins + 徽章门控 + 归档右键）、`main.ts`（数据管道 + readyOrigins）、`history.ts`（resume 带账号 + 搜索卡片右键）、`history.rs`/`lib.rs`（last_account + list_last_accounts）。
- account store（accounts.ts）新增消费方复用点：`accountConfigDir` / `peekSelectableAccounts` / `recordLastAccount` / `withAccount` / `shouldShowAccountBadge`（均纯/单测）。

## 交互 / 数据（照 DESIGN §3/§4/§7，不重复）
- 注入 opt-in：无账号 = 旧行为；选账号 = 注入其 configDir。
- 新会话（对话框）无 sid → 不记 lastAccount（运行期靠源① live 覆盖）——固有限制。

## 逐条实现步骤（Phase C，均已完成）
1. remote-launch.ts 注入核心 + 三 builder configDir。2. 四 runner 透传。3. history.rs last_account + list_last_accounts。4. sessionBadge 源② + shouldShowAccountBadge。5. 选账号 UI 四落点。6. withAccount 收敛三站点。7. 数据管道喂徽章。8. 全量验证。

## 审计结果（D，2026-07-24，三视角并行）
- **正确性/安全**：零阻塞零重要。命令注入面闭合（posixQuote + isValidConfigDir 双闸、与 daemon 逐码点对齐、空 configDir 逐字节回归有测）；serde 三态/异步菜单守卫/降级路径/sid 记录口径均正确。建议 3 条已修：C1 控制符对齐（remote-launch.ts + 测）、history.rs null 折叠注释澄清、resumeTab 记账守卫（随 withAccount 收敛统一）。
- **计划符合度**：零阻塞。五项应交付逐项磁盘核到真身、有对应断言，无谎报、无计划外夹带。DoD 四门实测吻合。重要 1 条=本文件缺失（已补建）。
- **架构/耦合**：零阻塞。重要项已修：main.ts 手抄 isSelectable → 复用；三站点 resolve+record 漂移 → `withAccount` 收敛（+5 单测）。遗留（记账）：tabs 菜单列表仍用同步 peek（冷缓存首开可能缺账号项），A5 加"用账号 X 重启"时改异步追加（复用 `resolveAttachMenuItem` 模式）一并解决。

## 工程审计（E）
- account store 多消费方（tabs/history/settings/main/chip）经本轮收敛后：可选判定（isSelectable）、configDir 解析 + 记账（withAccount）已单一来源；徽章门控（shouldShowAccountBadge）单一来源。无新增拖累 A5 的耦合——反而 withAccount 为 A5 铺好骨架。daemon 只读边界（§6）守住：无写/凭据路径过 daemon。
- 遗留技术债（低优先，记账不阻塞）：tabs 同步 peek vs history 异步 fetch 的冷缓存可用性分裂 → A5 顺带收敛。

## 签收
- [x] 过代码审计(D，三视角零阻塞，建议/重要项已修) · [x] 过工程审计(E) · [x] 主计划已更新(F) · [x] 测试绿（417 vitest + 350 cargo + build）
