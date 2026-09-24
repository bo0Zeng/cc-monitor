# MC1+AL1 摸底：shim · `CCM_SELF` · 别名写路 · 机器卡 8 颗按钮（现打，基线 `2b2fde3b`）

> 只记**今天是什么**。量法：`git grep -n`（不截断），逐条回读过源码。
> 设计依据：`设计/01 §6.7b` · `设计/71 §12.5` / `§12.6` / `§13` · `设计/70 §3` / `§5`。

## 一、`CCM_SELF`：一个写者，一个读者

| 角色 | 住址 | 做什么 |
|---|---|---|
| **写者（唯一）** | `local_backend.rs::ccm_entry_shim` | 远端 shim 第三行 `CCM_SELF="${CCM_SELF:-$0}" exec <后端> ccm "$@"` |
| **读者（唯一）** | `src/backend/control/ccm/plan.rs::Env::from_process` | `self_path = CCM_SELF → 兜底 argv[0]` |
| 读者的下游 | `plan.rs` 容器路内层命令（`inner` 的第一个词）· `ccm/mod.rs::probe_output`（`self=` 那一行，只给人看，`ccm_probe` 不解析它） | |
| 测试夹具 | `tests/e2e/ccm-cli.test.sh`（7）· `ccm-contract-parity.sh`（2）· `ccm-print-parity.sh`（1）· `p3t-local-tmux.sh`（1） | 全是 `CCM_SELF=/usr/local/bin/ccm`：把 `--print` 的黄金串钉成与软链临时目录无关 |
| 判据 | `local_backend_tests.rs`（shim 必须带 `${CCM_SELF:-$0}`、落在 `exec` 之前）· `sftp_tests.rs`（注释） | |
| 证据脚本 | `tests/evidence/K-P5c-…mutations.py` · `K-R48-native-vs-bash-parity.py` | 只读历史，不进门禁 |

**它为什么存在**：shim `exec` 的是后端真身 ⇒ `argv[0]` 是 `cc-monitor-backend`，容器路内层命令缺了 `ccm` 这个词。
入口①（basename ＝ `ccm`）用不着它；只有入口②（`<后端> ccm …`）需要。

## 二、shim：一个生成器，一个推送点，一个落点

| | 住址 |
|---|---|
| 生成器 | `local_backend.rs::ccm_entry_shim(backend_path)` |
| 唯一推送点 | `sftp.rs::install_remote_ccm_helper` 的 ①（`upload_atomic` → 远端 `~/.local/bin/ccm`，`CCM_CLI_REMOTE_PATH`）|
| 声明 | `tool_registry.rs` `TOOLS` 里 id `ccm`（`RemoteHomeRelative(".local/bin/ccm")`）· 足迹表跟着它走 |
| 本机对应物 | **没有 shim**：`local_backend.rs::install_local_ccm_entry` 把后端**逐字节拷一份**改名 `ccm` 放 `~/.cc-monitor/bin/`（由 `resolve_or_extract` 每次解析后端时顺手放）。这份拷贝就是 `01 §6.7b` 说的「第二份字节」 |
| rc 里的 PATH 行 | `src/shared/ccm-aliases.sh` 同时把 `~/.local/bin`（远端 shim）与 `~/.cc-monitor/bin`（本机拷贝）加进 PATH |

## 三、🔴 「`ccm` 就是后端二进制本身」今天做不到的那一格（**写区外，停下报备**）

`01 §6.7b` 的目标：远端只有一个文件 `~/.cc-monitor/bin/ccm`，它就是后端。卡在：

1. monitor 起远端后端的两处 —— `ssh_source.rs::connect_and_exec`（流模式，`<backend_path> [--with-bg] [--tail-only] [--with-rbind-token]`）
   与 `ssh_source.rs::probe_backend`（**零参数** exec `backend_path`）—— 都直接 exec `cfg.backend_path`。
2. 后端 `main.rs` 最先做的是 `control::ccm::intercept(argv0, …)`：**basename ＝ `ccm` ⇒ 整条进一次性 ccm 模式**。
3. ⇒ 若 `backend_path` 指向一个叫 `ccm` 的文件：探针那一发（零参数）会变成「在当前目录起一个 agent」，
   流模式那一发会被 ccm 的 argv 解析器当未知 flag 拒掉（它「一条都不许静默忽略」）。

要走通只有两条路，**两条都在本路写区外**：
- 改 monitor 起后端的那一行（给后端一个显式的「我是后端」词）⇒ `ssh_source.rs`，**C1 独占**；
- 改后端的 argv 语法（`intercept` 学会在 basename ＝ `ccm` 时认出后端调用）⇒ 零参数那一发与 `ccm`（无参＝起会话）**正面冲突**，是设计题不是工单。

⇒ 本路**不删 shim 这个落点**（删了远端 PATH 上就没有 `ccm`），只做不依赖这一格的那几件（见交付报告）。

## 四、别名今天走哪几条写路（全在 monitor 进程里，**没有一条经后端**）

| 写什么 | 命令 | 落盘原语 | 校验 / 回滚 |
|---|---|---|---|
| 本机别名文件 `~/.cc-monitor/account-aliases.sh`（整份重写） | `write_account_aliases`（`dryRun` 布尔）→ `account_aliases::apply` → `write_alias_file` | `profile_installer::atomic_write_string`（写临时文件 → `rename` / Windows `ReplaceFileW`） | `verified_write::verify_and_rollback`，坏了就删 |
| 本机 rc 里那一行 `source` | 同上 → `account_aliases::ensure_rc_source_line` | `fenced_block::find_pair` ＋ **自己的一份拼接** → `std::fs::copy` 备份 → `atomic_write_string` | `verify_and_rollback`，从备份拷回 |
| 本机 rc / PowerShell profile 里的别名块 | `cc_integration_install` / `_uninstall` → `profile_installer::install_to_profile` / `uninstall_from_profile` | `plan_install`（POSIX 臂借 `sftp::merge_profile_block`，PS 臂 `replace_or_append_block`）→ `std::fs::copy` 备份 → `atomic_write_string` | `verify_and_rollback` |
| 远端 `~/.local/bin/ccm`（shim）＋ 远端 rc 别名块 | `install_remote_ccm_helper` / `uninstall_remote_ccm_helper` | monitor 这一侧的 SFTP 客户端 `sftp::upload_atomic`（备份也是一次 `upload_atomic`） | `read_optional` ＋ `verify_readback`，失败再 `upload_atomic` 原文 |

⇒ **规则写了三份**（`71 §12.5` 的读数成立）：围栏拼接三份（`sftp::merge/strip_profile_block` · `profile_installer::replace_or_append_block/strip_block` · `account_aliases::ensure_rc_source_line` 里内联的那一段），
「备份 → 写 → 回读 → 回滚」这个序列四份（本机三处各写一遍 ＋ 远端装/卸各写一遍，共五个函数体）。

⇒ 用户那条裁决（「只允许后端的文件管理部分写文件」）管的是**后端**里谁能写；今天别名的写全在 monitor 进程，
**本路不把任何一处写挪进后端**。`71 §12.6.3` 的「远端那条改走叫远端后端自己写」**要挪进后端 ⇒ 停下报备**，本路不做。

## 五、机器卡上的 8 颗按钮（`machine-card.ts` 动作区，全挤在「组件」栏一行里）

测试连接 · 文件 · 推送公钥 · 开新 Claude · 安装 backend · 卸载 backend · 装 ccm 启动器 · 卸载 ccm

其中后四颗是两件事的四个开关：部署后端（装 / 卸）＋「ccm 助手」（装 / 卸）；
「ccm 助手」本身又是两件事（`71 §13.1` 的 ① 推入口 ＋ ② 往 `.bashrc` 写别名块）。

## 六、「ccm 助手 / ccm 启动器」这个词今天在哪（界面文字）

| 住址 | 写区 |
|---|---|
| `machine-card.ts`（按钮、进度、安装位置说明） | 本路 |
| `remote-section.ts::buildWrapperSnippetRow` 的说明 | 本路 |
| `panel.ts` 远端 resume 命令的说明 | 公共文件（一行文案） |
| `launcher-diagnostics.ts` 别名块按钮的 title | 本路 |
| `tabs.ts::warnCwdFallbackAttach` · 杀会话确认那句（`91 §2.1` / `§2.3` 点名的就是它） | 🔴 **热文件，本路不许碰** |
