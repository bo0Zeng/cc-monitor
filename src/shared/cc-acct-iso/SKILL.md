---
name: cc-acct-iso
description: Claude Code 多账号「并发隔离 + 实时共享」通用管线。每个账号一个 CLAUDE_CONFIG_DIR(各自 .credentials.json/.claude.json,两号同时跑不互踢),skills/memory/history/settings/plugins 全部 symlink 回同一共享库实时同步。用户在装/迁移/加号/换号/自检/回退多账号配置时,或问「两个账号怎么又分开又同步」「怎么同时跑两个号不掉登录」「cc-monitor 按会话切账号的账号库怎么建」时用。
---

# cc-acct-iso:多账号并发隔离 + 实时共享

## 一句话模型
**身份靠 config-dir 分开,数据靠 symlink 合一。**

```
~/.claude/                  ← 共享库(纯共享,不含任何账号态)
  skills/ projects/ sessions/ settings.json CLAUDE.md plugins/ history.jsonl …
~/.claude-accts/
  accounts.json             ← manifest(契约 v1):shell 工具写,cc-monitor 读
  z/  .credentials.json .claude.json backups/  (私有实体) + 上面每一项的 symlink
  b/  同上
```
起某个号 = `CLAUDE_CONFIG_DIR=~/.claude-accts/<名> claude`。两个号各读各的凭据 → **同时跑不互踢、token 各刷各的**;而 skills/记忆/历史/设置是同一批实体文件 → **一处改动,处处生效**。

## 命令
```bash
cc-acct-iso init <默认账号名>            # 初始化 + 把现有默认账号迁进来
cc-acct-iso add <名> [--from-credentials <文件>] [--seed-claude-json <目录|文件>] [--email <邮箱>] [--default]
cc-acct-iso rm <名> [--force]            # 只删该号 config-dir,不动共享库
cc-acct-iso list [--json]                # 账号 / 邮箱 / 登录状态 / config-dir
cc-acct-iso which [<config-dir>]         # 当前 $CLAUDE_CONFIG_DIR 是哪个号
cc-acct-iso verify [<名>...] [--no-probe] # 自检:结构 + 隔离 + 共享(不登录)
cc-acct-iso sync                         # 幂等补链/修断链/修权限/刷新 manifest 邮箱
cc-acct-iso rollback [<时间戳>|latest]   # 从自动备份还原
cc-acct-iso run <名> [-- <claude 参数>]  # 用该号起 claude(唯一登录入口)
cc-acct-iso shellinit                    # 打印 rc 片段(export + <名>cc 函数)
cc-acct-iso config                       # 打印当前生效配置
```
**动文件的命令(`init`/`add`/`rm`/`sync`/`rollback`)一律默认 dry-run 只打印计划,加 `--apply` 才落盘**(`--apply` 放命令前后都行);`--apply` 前自动 `cp -a` 备份到 `$ACCTS_DIR/.backup-<ts>/`,任何一步都能 `rollback` 回去。

## 迁移(用户自己跑,一步一停)
```bash
cc-acct-iso init <默认账号名>                 # ① 先看计划(dry-run,什么都不动)
cc-acct-iso init <默认账号名> --apply         # ② 落盘:凭据 + .claude.json 搬进 ACCTS_DIR,其余建 symlink
cc-acct-iso verify                            # ③ 自检
cc-acct-iso shellinit                         # ④ 打印 rc 片段 → 自己贴进 ~/.bashrc(工具不改你的 rc)
cc-acct-iso add <第二个号> --from-credentials <旧快照.json> --apply   # ⑤ 从旧快照导入,免得重登
cc-acct-iso verify                            # ⑥ 再自检(两号邮箱应不同)
# 出任何问题:cc-acct-iso rollback latest --apply
```
- **迁移只搬 2 个文件**(`$SHARED_STORE/.credentials.json` 和 `$HOME/.claude.json`),其余实体文件原地不动 —— 别的脚本里硬编码的 `~/.claude/skills` 一类路径照常有效。
- **迁移后裸 `claude` 会找不到凭据** → 必须装 `shellinit` 里那行 `export CLAUDE_CONFIG_DIR=<默认号>`(或一律用 `<名>cc` / `cc-acct-iso run`)。
- 没有旧快照的新号:`add <名>` 之后 `run <名>` 进去 `/login`。工具**从不触发登录、不读凭据内容**。
- 装了旧的顺序切换方案(在 rc 里 swap `.credentials.json` 的那种)→ 迁移后**删掉它**,两套并存会互相打架。
- 如果你原来的 `.credentials.json` 是一条 **symlink**(软链式切号),`init` 会**拒绝迁移**并告诉你怎么解链 —— 直接搬会让两个号指向同一份真实凭据,隔离当场失效。
- **多次 rollback 要从新到旧逐个来**(`rollback latest --apply` → 再 `rollback <上一个时间戳> --apply`);跳着回退会得到半新半旧的混合态。

## 配置(参数化,不硬编码任何路径/账号名)
`$CC_ACCT_ISO_CONFIG` 或 `~/.cc-acct-iso/config`(sourceable `KEY=VAL`),CLI 选项优先:

| KEY | 默认 | 义 |
|---|---|---|
| `SHARED_STORE` | `$HOME/.claude` | 共享库(实体文件都在这) |
| `ACCTS_DIR` | `$HOME/.claude-accts` | 账号 config-dir 根 + manifest + 备份 |
| `ISOLATE_SET` | `.credentials.json .claude.json backups policy-limits.json stats-cache.json` | 各号私有实体(**不**建 symlink) |
| `SHARE_SET` | `@auto` | `@auto` = 共享库顶层减 ISOLATE_SET 减 SHARE_EXCLUDE;也可显式写白名单 |
| `SHARE_EXCLUDE` | `accounts *.bak *.bak-*` | `@auto` 下排除的项(支持 glob) |
| `LEGACY_HOME_ITEMS` | `.claude.json` | 无 env 时住 `$LEGACY_HOME_DIR/<项>`、有 env 时住 `<cfg>/<项>` —— 迁移时源取前者 |
| `LEGACY_HOME_DIR` | `$HOME` | 上面那些项的历史所在目录 |
| `LAUNCHER` | `claude` | 启动命令 |

CC 升级后共享库多出新文件 → 跑 `sync` 补链。CLI 覆盖:`--shared-store` / `--accts-dir` / `--launcher`。
配置文件是被 `source` 的(只写 `KEY=VAL`,别写命令);非本人所有的配置文件会被忽略,且它**改不动** dry-run 保护。

## manifest 契约(v1)
`$ACCTS_DIR/accounts.json` —— **本工具写,cc-monitor 读**(按会话切账号靠它拿 configDir):
```json
{ "version": 1, "updatedAt": "2026-07-23T18:00:00Z",
  "sharedStore": "<abs>", "acctsDir": "<abs>",
  "accounts": [ { "name": "z", "email": "z@x.edu", "configDir": "<abs>",
                  "isDefault": true, "mode": "isolated" } ] }
```
- `email` 取自各号 `.claude.json` 的 `oauthAccount.emailAddress`(**不是 token**);读不到就留空,`sync` 会补。
- `mode`:`isolated`(正常)/ `in-place`(逃生口,cc-monitor **不应支持**——那种模式下 `.claude.json` 会分裂)。
- 写入是**原子**的(临时文件 + `mv`),读者不会看到半截 JSON;写者之间用 `flock $ACCTS_DIR/.lock` 互斥,**读者无需加锁**。
- `$ACCTS_DIR` 是 700 → 消费方进程必须以**同一 uid** 运行。
- **给消费方(A2/A3)的硬提醒**:`configDir` 仍是**不可信字符串**。本工具已在源头拒绝 `' " \ \` $ ; | & < > * ? ( ) !` 与控制字符,但仍可能含空格与非 ASCII。注入环境变量请优先用不经 shell 的方式(如 `Command::env`);必须拼 shell 时要自己转义 + 白名单校验。
- 需要「实时邮箱 + 是否已登录」就用 `cc-acct-iso list --json`(进程级稳定接口),别自己去 stat 各种文件。

## 安全边界
- **绝不读取/回显/传输凭据内容**,只搬/链/`stat`;`list`/manifest 只出账号名、邮箱、路径。
- 破坏性操作:dry-run 默认 + `cp -a` 备份 + `rollback`;`rm`/rollback 的删除范围被限制在 `$ACCTS_DIR` 内,rollback 的还原目标被限制在共享库/账号库/`$LEGACY_HOME_DIR` 内,备份时间戳参数只接受 `[0-9A-Za-z._-]`(防目录穿越)。
- rollback 逐条独立执行:一条失败不会挡住其余(先救数据再清垃圾),会被覆盖的现场文件先挪进 `<备份>/pre-rollback/` 而不是直接删。
- 路径规范化 + 拒绝相对路径/`..`/控制字符/shell 危险字符;`ACCTS_DIR` 不许套在 `SHARED_STORE` 里(反之亦然)。
- `umask 077`:工具创建的一切默认只有本人可读;`verify` 会把非 600 的凭据、非 700 的账号目录判 **FAIL**,`sync --apply` 修回去。
- 不改用户 rc、不 `systemctl`、不触发登录 —— 需要你做的一律打印出来让你自己贴。

## 已知边界(诚实说明)
- **备份目录里是凭据明文副本**(`cp -a` 保住 600,靠 `$ACCTS_DIR` 700 兜底)。`rm <账号>` **不会**抹掉这些副本,备份也**不自动清理** —— 要彻底删号请自己 `rm -rf $ACCTS_DIR/.backup-*`(删完就没法 rollback 了)。
- **`verify` 不是纯只读**:活体探测会经第一个账号写一个 `.cc-acct-iso-probe.<pid>` 到某个共享目录里、随后删掉(被 SIGKILL 打断可能留残渣)。不想要就用 `--no-probe`。
- **`--seed-claude-json` 只剥 `oauthAccount`**:源文件里 `mcpServers` 的环境变量(可能含 API key)会一并复制到新账号目录。
- **`CLAUDE_CONFIG_DIR` 隔离**已实测(设它之后 `.credentials.json` + `.claude.json` 都落该目录,不回退读 `~`)且有官方文档佐证;**symlink 跨账号共享是未文档化用法**,CC 官方没保证。
- `projects/`(含 auto memory)被多号共享 → 并发写有理论竞争。但**单账号今天已多会话并发共享同一个 `projects/`**,竞争等级不变,只是从"跨会话"扩到"跨账号"。
- `.claude.json` **隔离**而非共享:它含 `oauthAccount`(共享会串号)且是高频写的大文件。代价是各号的项目信任/onboarding 各自一份 —— 新号可用 `add --seed-claude-json <老号目录>` 种一份。
- **运行中的会话不能中途换号**(凭据在启动时读)——切号 = 下次启动生效。
- 依赖 GNU coreutils(`stat -c`、`tac`)与 bash 4+;`jq` 可选(没有则用降级解析,但 `--seed-claude-json` 必须有 jq,且 manifest 被重新排版过就解析不了)。目标平台是 Linux。

## 安装 / 卸载
```bash
bash ~/.claude/skills/cc-acct-iso/scripts/cc-acct-iso-install.sh          # 软链命令 + 建 ~/.cc-acct-iso/ + 放示例配置
bash ~/.claude/skills/cc-acct-iso/scripts/cc-acct-iso-install.sh --uninstall
```
安装只做软链和建配置目录,**不动账号、不改 rc**;剩下要手动做的会打印出来。

## 自测
```bash
bash ~/.claude/skills/cc-acct-iso/scripts/test/run-tests.sh
```
19 组 166 条断言,全部在 `mktemp` 造的假 `$HOME` 上跑(假凭据、假 `.claude.json`、桩启动器),**零真机风险**;覆盖迁移/dry-run 零落盘/隔离/共享/sync 幂等与收敛/verify 判障与不放绿灯/rollback 安全与韧性/权限/并发/契约字段/无 jq 降级。
