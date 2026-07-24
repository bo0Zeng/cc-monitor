# A1 — 通用隔离 shell 模块 cc-acct-iso + manifest(M1+M2)

> 被 A2/A3 依赖,先做。纯 shell,远端 Linux 跑。**测试全在 mktemp 沙盒,绝不碰真 `~/.claude`。** 用户拍板:归属=独立 skill / `.claude.json` 隔离 / 迁移=全迁。

## DoD(可勾选)
- [x] 独立 skill `cc-acct-iso`(像 cc-bus:`SKILL.md` + `scripts/` + install 脚本 + 示例 config),**参数化不硬编码** `~/.claude`/`z`/`b`/本用户/`claude`。
- [x] 命令齐:`init` `add` `rm` `list` `which` `verify` `sync` `rollback` `run`(隔离启动器)`shellinit` + `-h`。
- [x] 写/读 manifest(`$ACCTS_DIR/accounts.json`)符合 MASTERPLAN §契约(version/sharedStore/acctsDir/accounts[])。
- [x] 动文件的命令(`init`/`add`/`rm`/`sync`/迁移)**默认 dry-run 打印计划**,需 `--apply` 才落盘;`--apply` 前自动 `cp -a` 备份受影响项到 `$ACCTS_DIR/.backup-<ts>/`。
- [x] **不读凭据 token**:email 仅从 `<cfg>/.claude.json` 的 `oauthAccount.emailAddress` 取(非 token);绝不 cat/传 `.credentials.json` 内容。
- [x] `verify`:起各账号只读探测(**不登录**)证明「隔离」(各号 `.claude.json` 邮箱不同)+「共享」(一号 touch 共享文件另一号可见)。
- [x] `rollback`:从备份还原 + 删新建账号 config-dir。
- [x] **沙盒集成测**(`mktemp -d` 造假 config、模拟两号并发、verify、rollback、幂等 sync)全绿;dry-run 断言零落盘。
- [x] **不做**:不触发登录(登录=用户跑 `run`)、不改会话名、不碰 cc-monitor 代码(那是 A2/A3)、不本地 Windows。

## 对接主计划 / 共享面
- **写** M2 manifest(§契约)—— A2/A3 只读它。字段改 = 两端同步(账本已记)。**本 feature 定死 schema v1。**
- 不触 `remote-launch.ts` / daemon / cc-monitor(A2/A3 才动)。A1 是纯 shell,跨域独立可交付。

## 配置模型(参数化,读 `$CC_ACCT_ISO_CONFIG` 或 `~/.cc-acct-iso/config`,sourceable KEY=VAL;CLI flag 覆盖)
| KEY | 默认 | 义 |
|---|---|---|
| `SHARED_STORE` | `$HOME/.claude` | 共享库(实体文件在此,迁移后**纯共享**、不含账号态) |
| `ACCTS_DIR` | `$HOME/.claude-accts` | 各账号 config-dir 根 + manifest + 备份 |
| `ISOLATE_SET` | `.credentials.json .claude.json backups policy-limits.json stats-cache.json` | 各账号独立真实文件/目录(不建 symlink) |
| `SHARE_SET` | `@auto`(= SHARED_STORE 顶层减 ISOLATE_SET 减 SHARE_EXCLUDE) | 共享项(symlink→SHARED_STORE) |
| `SHARE_EXCLUDE` | `accounts *.bak *.bak-*` | @auto 里排除的垃圾/退休项(不链不迁) |
| `LEGACY_HOME_ITEMS` | `.claude.json` | **无 env 时住 `$HOME/<item>`、有 env 时住 `<cfg>/<item>`** 的项;迁移时源取 `$HOME/<item>` |
| `LAUNCHER` | `claude` | 启动命令 |

## 迁移模型 = **V2 纯分离**(用户 2026-07-23 拍板)
```
~/.claude/                     ← 迁移后 = 纯共享库(实体文件原地不动)
  skills/ projects/ sessions/ settings.json CLAUDE.md plugins/ history.jsonl ...
~/.claude-accts/
  accounts.json                ← manifest(M2 契约)
  <default>/  .credentials.json(真,从 ~/.claude/ 搬入) + .claude.json(真,从 ~/.claude.json 搬入) + 共享项 symlink
  <other>/    .credentials.json(真,可从 cc-acct 快照导入) + .claude.json(真,可 seed) + 共享项 symlink
```
- **对称**:所有账号(含默认)一律 `$ACCTS_DIR/<name>/`,一律靠 `CLAUDE_CONFIG_DIR` 起 → A3 无特例分支。
- **只搬 2 个文件**:`~/.claude/.credentials.json` → `<default>/.credentials.json`;`~/.claude.json` → `<default>/.claude.json`。其余实体文件**一律不动**(所以别的脚本硬编码 `~/.claude/skills` 等照常有效)。
- **裸 `claude` 会失效** → 由 install/`shellinit` 引导在 rc 里 `export CLAUDE_CONFIG_DIR=<default configDir>`(用户本来就走 `cc`/`zcc` 一类包装函数,代价≈0)。
- **`--default-in-place`(旧 V1)保留为逃生口**,非默认;文档标明其 `.claude.json` 分裂坑(裸起读 `~/.claude.json`、带 env 起读 `~/.claude/.claude.json`,两份)。
- 迁移这步动真凭据 → **用户空闲自跑**(dry-run→备份→apply→verify→可 rollback),我只沙盒测。

### 真机现状(A1 设计输入,已只读核实 2026-07-23)
- 现有多账号 = `~/.bashrc` 的 `cc-account-block`(顺序 swap 单文件 + `zcc/bcc/cct` 函数)。快照库 `~/.claude/accounts/{z,b}.json`,`.last=b`,live 凭据 md5 == `b.json` ⇒ **现默认号 = `b`**。
- **`z` 的凭据只存在于快照 `~/.claude/accounts/z.json`** → `add` 必须支持 `--from-credentials <file>` 导入,否则 z 得重新 `/login`。
- `~/.claude.json` 的 `oauthAccount.emailAddress` 与 live 凭据**不同步**(swap 不动它)⇒ 佐证 `.claude.json` 必须隔离;`list` 的 email 只能按各账号 config-dir 现读。
- 迁移完成后应**退休** `cc-account-block`(工具打印引导,不擅自改用户 rc)。

## 命令语义(动文件的一律 dry-run 默认,需 `--apply`)
- `init <default-name> [--default-in-place] [--apply]` — 建 `ACCTS_DIR`+manifest;**V2**:建 `<default-name>/`、搬 `SHARED_STORE/.credentials.json` 与 `$HOME/.claude.json` 进去、建共享 symlink、登记 `isDefault:true`。`--default-in-place` = 旧 V1 逃生口。
- `add <name> [--from-credentials <file>] [--seed-claude-json <src>] [--email <e>] [--default] [--apply]` — 建 `$ACCTS_DIR/<name>/` + 共享项 symlink;隔离项:有 `--from-credentials` 则 `cp` 进来(chmod 600)否则留空待 `run` 里 `/login`;`--seed-claude-json <configDir|file>` 复制一份 `.claude.json` 并**剥掉 `oauthAccount`**(保留项目信任/onboarding,不串号)。**不触发登录**。
- `rm <name> [--apply]` — 删账号 config-dir + manifest 条目(**不动共享库**;拒删 default 除非 `--force`)。
- `list` — 读 manifest 打印 name/email/configDir/default;email 从各 config-dir 的 `.claude.json` **现读**。
- `which [<dir>]` — 给定/当前 `$CLAUDE_CONFIG_DIR` 对应哪个账号名。
- `verify [<name>...]` — 结构自检(symlink 指向对、隔离项是真实文件非链、`SHARED_STORE` 内**不应再有** `.credentials.json`、`$HOME/.claude.json` 已搬走)+ 隔离(各 `.claude.json` 邮箱不同)+ 共享(一号 touch 共享文件另一号可见);打印 PASS/FAIL。**绝不登录。**
- `sync [--apply]` — 幂等补链:SHARED_STORE 新增顶层项(CC 升级)→ 给每个账号补 symlink;修断链;隔离项/排除项跳过。
- `rollback [<backup-ts>] [--apply]` — 从 `$ACCTS_DIR/.backup-<ts>/` 还原(含把 `.credentials.json`/`.claude.json` **搬回原位**)+ 删本次新建的账号 dir。
- `run <name> [-- <launcher args>]` — 隔离启动器:`CLAUDE_CONFIG_DIR="<cfg>" exec "$LAUNCHER" "$@"`(路径校验;唯一"起 claude"入口,登录在这)。
- `shellinit` — 打印可 `eval`/source 的 rc 片段:`export CLAUDE_CONFIG_DIR=<default cfg>` + 按 manifest 生成 `<name>cc` 一类包装函数(替代退休的 `cc-account-block`)。**只打印,不改用户 rc。**

## manifest(M2,写此文件,§契约 v1)
```json
{ "version": 1, "sharedStore": "<abs>", "acctsDir": "<abs>",
  "accounts": [ { "name": "...", "email": "...", "configDir": "<abs>", "isDefault": true } ] }
```
- 路径全绝对、工具按机器填。email 由工具从各 cfg `.claude.json oauthAccount.emailAddress` 读(不含 token);读不到留空。

## 安全 / 验证
- 破坏性命令默认 dry-run;`--apply` 前 `cp -a` 备份 + 存 rollback 步骤。
- 路径全部先规范化 + 校验(拒相对逃逸/怪字符);symlink 用绝对目标。
- 不 cat/回显/传 `.credentials.json`;只 stat 存在性。
- 幂等:重复 `add`/`sync` 不产生重复/损坏链。

## 测试策略(沙盒,零真机风险)
- `scripts/test/` bats 或纯 bash harness,全部在 `mktemp -d` 造的假 `SHARED_STORE`(塞假 skills/projects/settings.json + 假 `.claude.json`(带假 email)+ 假 `.credentials.json`)上跑:
  0. **V2 迁移**:假 `$HOME` 里造 `.claude/.credentials.json` + `$HOME/.claude.json` → `init d --apply` → 断言两文件已搬进 `<d>/`、原位已无、共享项 symlink 齐、manifest 对;`--default-in-place` 分支单独测。
  1. `init`+`add x`+`add y`(含 `--from-credentials`/`--seed-claude-json` 剥 oauthAccount)→ 断言 symlink 结构正确、隔离项非链、manifest schema/内容对。
  2. dry-run(无 `--apply`)→ 断言**零落盘**。
  3. 模拟并发:两账号 cfg 各写各的假 `.credentials.json`,互不覆盖 = 隔离。
  4. 共享:一号 `touch $SHARED_STORE/skills/foo`,另一号 cfg 经 symlink 可见 = 共享。
  5. `sync` 幂等 + 补新增顶层项 + 修断链。
  6. `rollback` 还原 + 删账号 dir。
  7. `verify` 在假 config 上判定逻辑(邮箱不同/共享可见)正确(不真起 claude — 桩住探测)。
- **DoD 硬门槛**:沙盒测全绿 + `shellcheck` 干净 + 手过一遍 `run` 拼的命令字符串正确。

## 逐条实现步骤(Phase C 照做)
1. 建 skill 骨架:`~/.claude/skills/cc-acct-iso/{SKILL.md,scripts/,examples/}`;主脚本 `scripts/cc-acct-iso`(dispatch 子命令)+ `scripts/lib.sh`(config 加载/manifest 读写/symlink/备份/校验/plan-执行两段式)。
2. config 加载 + 默认值 + `@auto` SHARE_SET 展开(SHARED_STORE 顶层 − ISOLATE_SET − SHARE_EXCLUDE)+ `LEGACY_HOME_ITEMS` 源路径解析。
3. manifest 读写(`jq` 优先,无 jq 用纯 bash 生成 + 降级读)——schema v1 定死。
4. `init`(V2 搬迁 + `--default-in-place` 逃生口)/ `add`(含 `--from-credentials`/`--seed-claude-json` 剥 oauthAccount)/ `rm`,dry-run + `--apply` + `cp -a` 备份,symlink 幂等。
5. `list` / `which` / `run`(路径校验 + posix 引用)/ `shellinit`。
6. `sync`(幂等补链/修断链)/ `rollback`(含把隔离项搬回原位)。
7. `verify`(结构 + 隔离 + 共享判定;真起 claude 的探测可桩)。
8. 沙盒测 harness(8 组,见测试策略)+ `shellcheck` + 跑绿。
9. `SKILL.md`(用法/参数/**V2 迁移引导** dry-run→备份→apply→verify→rollback→退休 cc-account-block/安全边界)+ install 脚本(软链命令、建 `~/.cc-acct-iso/`、放示例 config、打印手动激活步骤;可逆)。
10. 自查:参数化(无硬编码 `~/.claude`/`z`/`b`/本用户)、不读凭据内容、dry-run 零落盘 → 勾 DoD。

## 审计结果(Phase D:3 个视角并行)
3 个 agent 各自实跑复现,合计 **6 个阻塞 / 11 个重要**,已全部修掉并加了 50 条回归断言(116 → **166 条全绿**)。

### 阻塞(已修)
| # | 视角 | 问题 | 修法 |
|---|---|---|---|
| B1 | 安全 | `rollback <ts>` 的 `ts` 无校验 + `RESTORE` 分支的 `rm -rf` 无守卫 ⇒ **可诱导删任意目录树**(带 PoC) | `ts_check` 只收 `[0-9A-Za-z._-]`;`bk` 经 `realpath` 后强制 `is_under $ACCTS_DIR`;RESTORE 目标白名单(共享库/账号库/LEGACY_HOME_DIR);现存文件先挪进 `<备份>/pre-rollback/` 而非直接删 |
| B2 | 安全/正确性 | manifest 用 `cp` 就地覆写,**非原子** ⇒ 消费方(cc-monitor)会读到半截 JSON | 临时文件 + `mv` 原子落位 |
| B3 | 安全 | `path_sanitize` 允许 `'`,manifest 的 `configDir` 拼进 `export X='<dir>'` **可闭合引号注入** | `path_shell_safe` 从源头拒绝 `' " \ \` $ ; | & < > * ? ( ) !`;契约文档写明消费端**仍须**按不可信字符串处理 |
| B4 | 正确性 | `rollback` 逐条无错误边界,**第一条失败就 `set -e` 整体中止**,最该还原的那条永远轮不到 | 每条独立 `warn`+`continue`;顺序改成「先跑完所有 RESTORE(救数据)再跑 DELETE(清垃圾)」;末尾汇总并返回非 0 |
| B5 | 正确性 | `bk_save` 在操作**执行前**登记 undo ⇒ 操作失败后留下"假 RESTORE",rollback 会对没动过的文件做一次 `rm -rf`+`cp` 往返 | 拆成 `bk_copy`(只备份)+ 操作成功后才 `undo_restore`/`undo_delete`。不变式:undo.tsv 每条都对应一次真实发生过的改动 |
| B6 | 正确性 | 共享库里有**断链**时,`share_items`(算存在)与 sync 的退休判定(算不存在)不一致 ⇒ sync 一删一建**永不收敛**、每次 apply 多一个含凭据的备份目录、verify 恒 FAIL | 统一存在性谓词(`-e || -L`);verify 对「源头就断了」降级为 warn 并点名 |

### 重要(已修)
`umask 077` + 备份目录 700 + `chmod` 失败改 `die`(原来备份树 775) · `verify` 把非 600 凭据/非 700 目录判 FAIL + `sync` 修权限(原来只在创建时保证一次) · `json_esc` 补 C0 转义 + `--email` 校验(一个控制字符能把 manifest 变砖、工具自锁死) · 配置文件里写 `APPLY=1` 曾能废掉整套 dry-run → `cfg_load` 后强制 `APPLY=0` · `sync`/`run`/`verify` 都加 `acct_dir_ok` 计划期校验(原来只有执行期守卫,dry-run 会给出必然半途崩掉的计划) · `verify` 打错账号名曾返回 PASS → 改 die · 共享项 0 个曾报「全部就位」PASS → 改 FAIL · 混合 in-place+V2 时全局检查曾整体跳过 → 改成只要有隔离账号就仍检查 · `rm` 掉默认号后无人是默认 → 自动补选 + 提示 · 并发 `add` 会静默丢账号 → `flock` 提到 `manifest_load` **之前** · `init` 曾原样搬走 symlink 形态的凭据(旧软链切号方案)⇒ 隔离当场失效 → 拒绝并给解链命令。

### 计划符合度(Phase D 第 3 个视角)
DoD 8 条全达成、manifest 严格符合契约、迁移确为 V2、10 步无跳过、无范围蔓延。指出的缺口已补:`SHARE_SET` 配置键(原计划列了但没实现)、SKILL.md 三处漂移、`verify`「只读」措辞不准(它会写一个自动清理的探针)。

## 工程审计(Phase E)
- **账本影响**:manifest 契约(§MASTERPLAN)在 A2/A3 尚未开工时定型更省事 —— 加了 `updatedAt`(消费端判新鲜度)、`mode`(让 cc-monitor 能直接拒绝 in-place 账号),并补了「原子写 / 写者 flock、读者免锁 / 700 同 uid / configDir 不可信」四条消费方规则。这属于「账本预见的重叠现在就统一」,不留给 A2 打补丁。
- **新增 `list --json`**:审计指出「daemon 只能直接读 on-disk 格式,等于把『什么算已登录』的定义复制到第二处」。给一个进程级稳定接口,A2 可选用。
- **A2/A3 硬约束(已写进 SKILL.md 契约段)**:注入 `CLAUDE_CONFIG_DIR` 优先用不经 shell 的方式;必须拼 shell 时自己转义 + 白名单;不要自己写 `accounts.json`(要写就 shell out 到本工具)。
- 未引入对 cc-monitor 的任何依赖,A1 仍是跨域独立可交付。

## 签收
- [x] 过代码审计(D,3 视角并行 + 分诊修复 + 回归) · [x] 过工程审计(E) · [x] 主计划已更新(F) · [x] 沙盒测绿(166/166,shellcheck 干净)
