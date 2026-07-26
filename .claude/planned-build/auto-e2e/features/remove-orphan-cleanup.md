# 砍掉「清理孤儿会话」功能（F05 removal）

> 用户 2026-07-26 决定：清理孤儿的主要用途（清 #76 的 `cc-<sid8>-N` 堆积）随 #76 修复已过时，
> 且带 UX 审计 #2 footgun（把别窗口/实例正跑的活会话误列孤儿劝杀）→ 直接砍。走 planned-build。
> 分支 account-ux。红线：daemon 零改·不改 TMUX_LS_FMT·不 push/发版/bump·不碰 ~/.bashrc·不用 emoji。

## 范围裁定（砍什么 / 留什么）
- **砍**：账号菜单里的「清理孤儿会话…」批量扫 = `cleanupOrphanTmux` + `findOrphanTmux` + `isCcmTmuxName`(仅服务孤儿) + 菜单入口 + wiring + F-E4 orphan e2e + F05 单测。
- **留（不碰）**：单会话右键「杀死会话（kill tmux）」= `killRemoteTmux`（含其 `opts?.confirm` seam，headless 测试用，仍有价值）；账号 chip 本体 / 切号 / 对齐 / 管理账号 / 刷新；`findClaudeTmux`/`findIdleTmux`/`isClaudeTmuxCommand`（灰灯+attach 用）；`restart-shims/error-toast`（orphan-shims 曾复用，删 orphan-shims 不影响 restart 自己用）。

## 移除清单（recon 已核 file:line）
### 生产代码
- [ ] `src/tabs.ts` 删 `cleanupOrphanTmux`（~2585-2627 整个方法）。
- [ ] `src/tabs.ts` 删 `findOrphanTmux`（311-318）+ `isCcmTmuxName`（296-303，仅 findOrphanTmux 用）。
- [ ] `src/account-chip.ts` 删 `cleanupOrphans?` dep（57-58）+ 「清理孤儿会话…」菜单项构造（198-204）。
- [ ] `src/main.ts` 删 `cleanupOrphans:` wiring（196）。
### 测试
- [ ] `src/tabs.vitest.ts` 删 `findOrphanTmux`/`isCcmTmuxName` import（123-124）+ 「F05 findOrphanTmux / isCcmTmuxName」describe（1335-1372）。
- [ ] `src/tabs.vitest.ts` **F-E4 confirm seam describe（1374-1440）= 手术**：删 `cleanupOrphanTmux` 的用例（~1400-1435），**保留 `killRemoteTmux` 的 seam 用例**。
### e2e
- [ ] 删 `e2e/orphan-suite.sh`、`e2e/orphan-cmd-driver.ts`、`e2e/orphan-shims/`(整目录)、`e2e/gen-live-claude-tmux.sh`。
### 文档
- [ ] `e2e/README.md` 删/改 F-E4 孤儿段（107-110）。
- [ ] `.claude/planned-build/auto-e2e/` MASTERPLAN F-E4 矩阵、STATUS、`ux-audit-2026-07-26.md`（#2 标注「已由删除功能解决」）、本 features 目录。

## DoD（可验证）
- [ ] `grep -rn "cleanupOrphan\|findOrphanTmux\|isCcmTmuxName\|清理孤儿\|cleanupOrphans" src/ e2e/` 无悬空引用（除本 plan/ux-audit 的历史记述）。
- [ ] `npx tsc --noEmit` = 0（删了会暴露任何漏删引用）。
- [ ] `npm test`（vitest）绿、不回归（删了 orphan 单测后数量下降是预期；`killRemoteTmux` seam 用例仍在仍过）。
- [ ] `npx vite build` ✓、探针不漏。
- [ ] 账号 chip 菜单里**不再有**「清理孤儿会话…」；chip/切号/对齐/管理/刷新/单会话 kill **仍在**。
- [ ] daemon 零改；`killRemoteTmux` 及其 seam 未动。

## 层级 / 做法
Phase C 主线程串行手术（每文件核对 recon 点）。Phase D = 第三方 agent 审「删干净无悬空 + 未误伤 killRemoteTmux/灰灯/attach + 门禁绿」。Phase E/F 常规。

## 审计结果 / 签收
- **C 已做（commit bdfb8ef）**：移除清单逐项落地，recon 点全核。验证：`grep` 无悬空引用（除本 plan/ux-audit 历史记述）；`tsc --noEmit` = 0（暴露不出漏删）；`npm test` = **592**（602−10 orphan 测，`killRemoteTmux` seam 用例仍在仍过）；`vite build` ✓；daemon 零改；`killRemoteTmux`/灰灯/attach/账号 chip 全保留。
- **D 代码审计**：委托第三方 agent 验"删干净无悬空 + 未误伤保留项 + 门禁绿"。
- **E 工程**：删一个过时+带 footgun 的功能，净化；UX 审计 #2 随之解决。
- **F**：STATUS + ux-audit #2 + 本条已更。
