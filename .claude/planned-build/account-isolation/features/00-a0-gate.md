# A0 — Phase 0 gate:CLAUDE_CONFIG_DIR 真隔离验证

> load-bearing 门禁。整套设计压在「设 `CLAUDE_CONFIG_DIR=<dir>` 后 `.credentials.json`+`.claude.json` 落 `<dir>/`」。不成立则退回 swap 单文件顺序切换(无并发)。

## DoD
- [x] 在**真 CC** 上实测 CLAUDE_CONFIG_DIR 是否重定位 config dir。
- [x] 查官方文档佐证(credentials 是否明文重定位、哪些必须 per-config-dir、哪些可共享)。
- [x] 只读探测,**不登录、不动真 `~/.claude`**。
- [x] 结论记入 STATUS/MASTERPLAN。

## 执行 & 证据(2026-07-23,PASS)
1. **实测**(本机 CC 2.1.218,aya = 账号所在 Linux):
   - baseline:`claude mcp list` → 2 个用户级 server(exit 0)。
   - relocate:`CLAUDE_CONFIG_DIR=/tmp/ccprobe.$$ claude mcp list` → **在探测目录建了 `.claude.json` + `backups/`**;输出变了(用户级 server 消失,只剩项目级 code-picture 显 "Pending approval" —— 因全新空 config 无审批记录)。
   - ⇒ CLAUDE_CONFIG_DIR **把 config dir 整体重定位**;relocate 实例不读真 `~/.claude.json`。
2. **文档**(claude-code-guide agent 查 code.claude.com/docs):
   - authentication.md 明文:设了 CLAUDE_CONFIG_DIR(Linux/Windows)则 `.credentials.json` **落该目录而非 `~`**(HIGH 置信)。
   - `.claude.json` 重定位:文档未逐字点名,但 "every ~/.claude path lives under that directory" 措辞隐含(MEDIUM-HIGH)+ 实测已证。
   - claude-directory.md 列全 config 内容;env-vars.md 有 CLAUDE_CONFIG_DIR。
   - **未文档化**:CLAUDE_CONFIG_DIR 未被官方"包装"成多账号功能;symlink 跨账号共享无官方保证。

## 结论
- **PASS**:两个不同 CLAUDE_CONFIG_DIR = 两套隔离 credentials + `.claude.json` = 并发不互踢成立。设计不必退回顺序切换。

## Caveat → 缓解(带进 A1 设计)
- 官方未测/未文档 symlink 共享;`projects/`(auto memory `MEMORY.md`)并发写有理论竞争。
- **缓解论证**:单账号今天已多会话并发共享同一 `~/.claude/projects/`(cc-monitor/tmux 多开每天在做)= 该竞争**现状已存在且被容忍**。隔离凭据 + 共享 `projects/` **不引入新风险类别**,只是把"多会话"从单号扩到跨号。
- 决策:`.claude.json` **隔离**(已定,避免 oauthAccount 串号 + 114KB 高频写竞争);`projects/`/`skills/`/`history` 等**照用户意图共享**(接受既有等级的并发写风险)。

## 签收
- [x] 门禁通过 · [x] 结论落 STATUS/MASTERPLAN · [x] caveat 带进 A1
