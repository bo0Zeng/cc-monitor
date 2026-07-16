# SSH 远端模式 — daemon 部署 runbook（issue #15 / #29）

> **更新（issue #29 F08b 已实现自动部署）**：cc-monitor.exe **内嵌**交叉编译好的 aarch64/x86_64
> musl daemon 二进制；连接远端时**自动**探测远端 arch（`uname -m`）、按 build_id 版本门控经 SFTP
> 把对应二进制推到 `cfg.daemon_path`（默认 `~/.cc-monitor/bin/cc-monitor-remote`）并 exec——用户**零手动步骤**。
> 自动部署失败（无内嵌该 arch / daemon_path 含 `~` / SFTP 失败）会优雅降级到下面的手动部署。
>
> **daemon_path 必须是绝对路径**（如 `/home/pi/.cc-monitor/bin/cc-monitor-remote`）：SFTP 无 shell、
> 不展开 `~`，含 `~` 时自动部署会跳过（手动部署仍可用 `~`，因为那走 shell exec）。
>
> 下面的**手动部署**仍然有效，作为：① 自动部署不可用时的回退；② Phase 0 钢丝验证的原始步骤。

---

## 发版构建：交叉编译 + 内嵌 daemon 二进制（F08b）

打包 cc-monitor.exe 前，需把 daemon 交叉编译成两份 musl 二进制放进 `src-tauri/embedded-daemons/`
（该目录已 gitignore——是构建产物；缺失时 `build.rs` 优雅降级，自动部署变 no-op，dev/CI 仍可编译）：

```powershell
# 一次性装工具链（Windows 主机；cross+Docker 在 Windows+Scoop-rustup 下踩坑，用 cargo-zigbuild）
rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
scoop install zig                 # 或官网下 zig
cargo install cargo-zigbuild

# 交叉编译（在 remote-daemon-proto/ 下）
cd remote-daemon-proto
cargo zigbuild --release --target x86_64-unknown-linux-musl
cargo zigbuild --release --target aarch64-unknown-linux-musl

# 放进内嵌目录（build.rs 会 include_bytes 进 exe）
mkdir ..\src-tauri\embedded-daemons
copy target\x86_64-unknown-linux-musl\release\cc-monitor-remote   ..\src-tauri\embedded-daemons\cc-monitor-remote-x86_64
copy target\aarch64-unknown-linux-musl\release\cc-monitor-remote  ..\src-tauri\embedded-daemons\cc-monitor-remote-aarch64
```

> **纪律（build.rs 有 staleness 警告兜底）**：每次改了 daemon（尤其 bump `main.rs::BUILD_ID`）都要**重跑
> zigbuild + 重新放二进制**，否则内嵌的旧二进制 build_id 与源码/期望不符 → 永不收敛的重复部署。
> `build.rs` 在内嵌二进制比 daemon 源码旧时会 `cargo:warning` 提示。

---

## 手动部署（自动部署的回退 / Phase 0 原始步骤）

在目标机器上**原生编译**后手动放到固定路径——零交叉编译（aarch64 机器自己编 aarch64）。本文给出在
**NanoPi(aarch64)** 或任意 Linux 机器（含 **WSL**）上把 `cc-monitor-remote` 跑起来的确切步骤。

---

## 0. 两条路径

| 目标 | 用途 | 需要 |
|---|---|---|
| **A. WSL / 本地 Linux 容器**（推荐先做） | S8 本地端到端 de-risk，不碰 NanoPi | Windows 上一个 WSL Ubuntu 或 Docker Linux |
| **B. NanoPi(aarch64)** | S9 真实里程碑（跨网络） | NanoPi 可 SSH 登录 |

两条路径的 daemon 构建/安装步骤**完全相同**（都是目标机原生 `cargo build`）。区别只在 cc-monitor 设置里填的 host（A 填 `localhost`/WSL IP，B 填 Pi 的地址）。

---

## 1. 前置：目标机上装 Rust 工具链

在 **目标机器**（WSL 里的 Ubuntu，或 SSH 进 NanoPi 之后）：

```bash
# 装 rustup + stable（已装可跳过）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustc --version    # 确认 stable 工具链可用
```

NanoPi 上首次编译 Rust 可能需要几分钟，且需要一些系统包（一般已具备）：

```bash
sudo apt-get update && sudo apt-get install -y build-essential pkg-config
```

> daemon 当前依赖：`tokio` / `notify` / `notify-debouncer-mini` / `walkdir` / `serde` / `tracing`。**纯 Rust，不需要 OpenSSL / NASM**（russh 在本地端，daemon 不含 SSH 库）。

---

## 2. 把 daemon 源码弄到目标机

只需要仓库里的 `remote-daemon-proto/` 这一个目录（它是独立 crate，不依赖 src-tauri）。任选其一：

```bash
# 方式 1：在目标机 git clone（推荐——以后好更新）
git clone <your-cc-monitor-repo-url> ~/cc-monitor-src
cd ~/cc-monitor-src/remote-daemon-proto

# 方式 2：从 Windows scp 过去（只传那一个目录）
#   在 Windows 上：
#   scp -r "<cc-monitor 仓库根>\remote-daemon-proto" user@host:~/remote-daemon-proto
#   然后目标机：cd ~/remote-daemon-proto
```

---

## 3. 原生编译

```bash
cd remote-daemon-proto       # 进到 crate 目录
cargo build --release        # aarch64 编 aarch64 / x86_64 编 x86_64，零交叉编译
```

产物：`target/release/cc-monitor-remote`（二进制名由 `Cargo.toml` 的 `[[bin]] name` 决定）。

---

## 4. 安装到固定路径

cc-monitor 默认 exec 的路径是 `~/.cc-monitor/bin/cc-monitor-remote`（可在设置里改 `daemonPath`）：

```bash
mkdir -p ~/.cc-monitor/bin
cp target/release/cc-monitor-remote ~/.cc-monitor/bin/
chmod 700 ~/.cc-monitor/bin/cc-monitor-remote
~/.cc-monitor/bin/cc-monitor-remote --help 2>/dev/null || true   # 可执行性自检
```

---

## 5. 本机 smoke（确认 daemon 自己 OK，不连 cc-monitor）

daemon 启动后**第一行**必是 `hello`，之后监听 `~/.claude/projects/` 与 `~/.claude/sessions/`：

```bash
# 准备一个 scratch claude 目录（或直接用真实 ~/.claude）
export CLAUDE_CONFIG_DIR=/tmp/ccm-smoke/.claude
mkdir -p "$CLAUDE_CONFIG_DIR/projects/proj" "$CLAUDE_CONFIG_DIR/sessions"

# 前台跑 daemon，stdout 是 wire（JSON Lines），stderr 是日志
~/.cc-monitor/bin/cc-monitor-remote
# 另开一个终端，往 jsonl 追加一行：
echo '{"type":"user","uuid":"u1","timestamp":"2026-01-01T00:00:00Z","sessionId":"s1","message":{"role":"user","content":"hi from remote"}}' \
  >> "$CLAUDE_CONFIG_DIR/projects/proj/s1.jsonl"
```

预期 daemon stdout 依次出现（一行一条 JSON）：
```
{"kind":"hello","v":1,"build_id":"phase0-proto","host_arch":"aarch64","claude_dir":"..."}
{"kind":"line","session_id":"s1","path":".../s1.jsonl","seq":0,"raw":"{...}"}
```
> 日志（tracing）走 **stderr**，不会污染 stdout 的 wire 流——这是 daemon 的硬约束。

---

## 6. 拿 host key 指纹（填进 cc-monitor 严格校验）

cc-monitor 的 host-key 策略：填了 `hostKeyFingerprint` 就严格校验（不匹配拒连）；不填则**首连 TOFU**（接受 + 大声 warn）。建议填上：

```bash
# 在能访问目标机的任意机器上：
ssh-keyscan -t ed25519 <host> 2>/dev/null | ssh-keygen -lf - | awk '{print $2}'
# 输出形如：SHA256:abc123...   ← 复制这一串填到设置的 hostKeyFingerprint
```

---

## 7. 在 cc-monitor 里配置（设置面板 → 远端模式，S6）

设置面板「远端 (SSH)」分组里**点「+ 添加机器」**为每台远端各填一组字段（多机 #30，写进 `~/.claude/claudecode-frontend/config.json` 的 `remote.hosts[]` 数组；旧的单 `remote` 对象仍兼容读、保存时自动升级成数组）。下表是**单台**字段，多台就重复填多张卡片：

| 字段 | 例（WSL / 路径 A） | 例（NanoPi / 路径 B） |
|---|---|---|
| `enabled`（全局开关，非每台） | ✓ | ✓ |
| `label`（可选，每台标识，作 Tab/历史的 `[机器]` 前缀；留空用 host） | `wsl` | `pi` |
| `host` | `localhost`（或 WSL IP） | `raspberrypi.local` / Pi 的 IP |
| `port` | 22 | 22 |
| `user` | 你的 WSL 用户名 | `pi` |
| `keyPath` | `C:\Users\<you>\.ssh\id_ed25519` | 同 |
| `daemonPath` | `/home/<you>/.cc-monitor/bin/cc-monitor-remote` | `/home/pi/.cc-monitor/bin/cc-monitor-remote` |
| `hostKeyFingerprint` | `SHA256:...`（第 6 步） | 同 |
| `addresses`（可选，F45 备用地址数组，每项 `host`/`host:port`/`[IPv6]:port`；与 `host` 竞速故障切换，首个成功者胜；设置卡「备用地址」多行输入即写此字段） | `[]` | `["10.0.0.9","pi.公网:2222"]` |

> **认证**：仅支持 publickey（无密码框，anti-pattern）。确保 `keyPath` 对应的公钥在目标机 `~/.ssh/authorized_keys`。
> **WSL 装 sshd**（路径 A）：`sudo apt-get install -y openssh-server && sudo service ssh start`，并把你的公钥加进 WSL 用户的 `authorized_keys`。

填完 **重启 cc-monitor**（远端配置在启动时读取，改了要重启生效——设置面板有提示）。

---

## 8. 端到端验收

- **S8（路径 A，本地）**：cc-monitor 连 WSL/容器 → 在 WSL 里 `echo` 假 jsonl 或跑 `claude` → 本地出 Tab + 实时渲染；session 文件增删 → Tab 建/归档。
- **S9（路径 B，NanoPi 跨网络）**：cc-monitor 连 Pi → Pi 上真跑 `claude` → 本地 Tab 实时渲染、seq 顺序正确；会话结束 → Tab 归档。

### Phase 0 已知边界（**不是 bug**，是 scope）
- ~~断线不自动重连~~ → 已补：断线**自动重连**（指数退避 2→30s，issue #17）；重连后 catch-up：旧 daemon 重扫活跃会话全量重放、客户端按 seq 去重；**p1f tail-only 起**改为 tail 从当前行数续 + monitor 旁路快照重拉（INVARIANTS §25a）。
- ~~慢消费者无 overflow 信号~~ → 已补：daemon 管道满时回传 `overflow` 哨兵帧，前端弹拥塞 toast（issue #32）。
- ~~远端历史浏览未接~~ → 已补：远端**历史浏览**已实现（#16，多机**分组 / 来源筛选** #30/#31）。~~全文搜索~~ → 已补：远端**全文搜索**已实现（#28，daemon `--search`）。~~resume~~ → 已补：远端 `↺` 提供**resume 命令助手**（复制 `claude --resume` 到远端终端粘贴；monitor 无法在远端开交互 TTY）。~~拉前~~ → 已补：远端 ↗ 拉前已实现（#18，设置面板每台机器卡片「装 ccm 助手」一键装）。**Batch7 起注册与启动分离**：块内装 `__ccm_rbind` **注册原语**（只挂 marker watcher + tmux 直通，不设环境不启动）+ 可选 `ccm` 便捷启动器（**不覆盖用户同名函数**——旧版曾无条件覆盖清掉用户自己的启动器）。自有启动器在 `( __ccm_rbind; exec claude ... )` 形态下调原语即可。**tmux 启动器注意**（真机踩坑）：`tmux new-session ... "ccm"` 的命令串走**非交互 shell**——bashrc 顶部的交互 guard 直接 return，函数不存在（报 `ccm: 未找到命令`）；应改用 `tmux new-session -d` + `tmux send-keys "ccm" Enter`（往交互 shell 里敲）。**tmux 自适配**：tmux 默认 `set-titles off` 会把 marker 截在 pane title 层——原语自动对当前 session 开标题直通（session 级选项，不写 tmux.conf）。旧版 ccm 块需重装一次才升级到此形态。**F74 起原语还写 tmux user option `@ccm_sid`**（当前 sid，随 `/branch` 实时更新；pane title 会被 Claude 活动标题抢写、不可靠，user option Claude 碰不到）——cc-monitor 靠它精确认「哪个 tmux 跑目标 sid」，修 resume/attach 撞进漂移/同目录别的会话（#63，见 INVARIANTS §30）。**旧 ccm 块需再重装一次才升级到带 `@ccm_sid` 形态**；未重装则 cc-monitor 退回按 cwd 匹配 = 旧行为，不变砖。**WT 多 tab 限制**：多个 ssh 会话开在同一 WT 窗口的不同 tab 时，↗ 只能拉起该窗口、无法切到具体 tab（建议每会话单独开窗）。
- ~~daemon 重启后 seq 从 0 重来（**p1f tail-only 起不再成立**：seq=行号、从当前行数起算，见 INVARIANTS §25a；本句仅描述旧 daemon 全量推流路径），客户端不处理~~ → 已补：客户端用 per-Tab `seenSeqs` 去重消化重放（issue #17），不重复、不丢新行。
- ~~远端探活仅 /proc 存在性~~ → 已补两层：①add-time 冒名判定（Batch5-F20）：**主证据 = pidfile 自带的 `procStart` 与当前占用者的 /proc starttime ticks 逐位相等 → 身份确认**（免疫全部时钟域问题）；不等/缺字段时 fallback 启发式——proc 启动时刻晚于 pidfile mtime+60s 或 cmdline 非 claude/node → 拒（防 daemon 启动前 PID 已被复用的 tmux 残留场景）；②运行期 procStart 双校验（issue #34，基线在 ① 把关后捕获才可信）。
- ~~bg 后台任务被当会话~~ → 已补 **kind 交互性门**（Batch6-F21，与"活性/作者身份"正交的第三维度——作者≠交互会话）：`kind` 存在且非 `interactive` 不宣告（缺失=旧 CC=放行），双端一字一致。**Batch7-F24 起改配置门**：monitor 按 `showBgSessions`（默认开）exec 时传 `--with-bg` → daemon 放行 bg 并在 `session_added` 帧附 `session_kind/cwd/name`；开关关 = F21 行为。旧 daemon（&lt; p1e）不识 `--with-bg` 会误入一次性查询模式——monitor 仅对 **auto-deploy 确认为 p1e+** 的远端加该参数（手动部署的旧 daemon 自动降级为不带参数连接）。
- ~~同 pidfile 原地换 sid 旧 tab 假 live / 同 sid 多 PID 误杀~~ → 已补（Batch6-F22）：sid 变更走 removed 路径 + `retire_sid_if_unreferenced` 引用计数（sid 退休唯一出口）。

**Phase 1/2 远端能力均已完成**：auto-deploy（#29，本文顶部）+ 全文搜索（#28）+ overflow 信号（#32）+ 版本协商（#33）+ 探活精确化（#34）+ 删除/resume/ccm 安装 + reconnect/seq 去重（#17）+ ↗ 拉前（#18）+ 历史浏览（#16）+ 多机聚合/分组/筛选（#30/#31）。剩仅真机 e2e 实测。

---

## 9. 更新 daemon（改了 wire/逻辑后）

```bash
cd ~/cc-monitor-src && git pull          # 或重新 scp remote-daemon-proto/
cd remote-daemon-proto && cargo build --release
cp target/release/cc-monitor-remote ~/.cc-monitor/bin/   # 覆盖
# 重启 cc-monitor（它会重新 exec daemon）
```

> Phase 0 没有 daemon/client 版本协商（hello 里有 `build_id:"phase0-proto"` 但不强校验）。wire 在 Phase 0 期间冻结；客户端解析器**忽略未知 kind/字段不 panic**，所以 Phase 1 加 `event_id` 等是向前兼容的。版本/sha 协商是 Phase 1（SFTP + build_id）。
