# SSH 远端模式 Phase 0 — daemon 手动部署 runbook（issue #15）

Phase 0（walking skeleton）的远端 daemon **不自动分发**——在目标机器上**原生编译**后手动放到固定路径。这刻意绕开了设计稿里"build.rs 交叉编译双 musl 二进制 + SFTP 上传"的整套机制（那是 Phase 1）。本文给出在 **NanoPi(aarch64)** 或任意 Linux 机器（含 **WSL**）上把 `cc-monitor-remote` 跑起来的确切步骤。

> **为什么手动**：交叉编译 aarch64-musl 是被低估最严重的一块（Windows 主机上要 zig/cross/docker 工具链）。aarch64 机器自己编 aarch64 = 零交叉编译。Phase 0 先用最薄钢丝验证 russh + 实时渲染端到端可行，再投入自动分发。

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
#   scp -r "D:\Sync\文档\claudecode-frontend\cc-monitor\remote-daemon-proto" user@host:~/remote-daemon-proto
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

设置面板「远端 (SSH)」分组填（写进 `~/.claude/claudecode-frontend/config.json` 的 `remote` 对象）：

| 字段 | 例（WSL / 路径 A） | 例（NanoPi / 路径 B） |
|---|---|---|
| `enabled` | ✓ | ✓ |
| `host` | `localhost`（或 WSL IP） | `raspberrypi.local` / Pi 的 IP |
| `port` | 22 | 22 |
| `user` | 你的 WSL 用户名 | `pi` |
| `keyPath` | `C:\Users\<you>\.ssh\id_ed25519` | 同 |
| `daemonPath` | `/home/<you>/.cc-monitor/bin/cc-monitor-remote` | `/home/pi/.cc-monitor/bin/cc-monitor-remote` |
| `hostKeyFingerprint` | `SHA256:...`（第 6 步） | 同 |

> **认证**：仅支持 publickey（无密码框，anti-pattern）。确保 `keyPath` 对应的公钥在目标机 `~/.ssh/authorized_keys`。
> **WSL 装 sshd**（路径 A）：`sudo apt-get install -y openssh-server && sudo service ssh start`，并把你的公钥加进 WSL 用户的 `authorized_keys`。

填完 **重启 cc-monitor**（远端配置在启动时读取，改了要重启生效——设置面板有提示）。

---

## 8. 端到端验收

- **S8（路径 A，本地）**：cc-monitor 连 WSL/容器 → 在 WSL 里 `echo` 假 jsonl 或跑 `claude` → 本地出 Tab + 实时渲染；session 文件增删 → Tab 建/归档。
- **S9（路径 B，NanoPi 跨网络）**：cc-monitor 连 Pi → Pi 上真跑 `claude` → 本地 Tab 实时渲染、seq 顺序正确；会话结束 → Tab 归档。

### Phase 0 已知边界（**不是 bug**，是 scope）
- 断线**不自动重连**、无 catch-up（daemon/网络掉了要手动重启 cc-monitor）。
- 慢消费者**无 overflow 信号**回传（daemon 满了丢帧 + warn，客户端不感知）。
- 远端**历史浏览 / 搜索 / resume / 拉前**未接（Phase 1+）；Phase 0 只做实时渲染。
- daemon 重启后 seq 从 0 重来——Phase 0 客户端不处理 seq 跨重启续接。

这些都在 Phase 1 补（auto-deploy / 韧性 reconnect+catch-up / history RPC / bind）。

---

## 9. 更新 daemon（改了 wire/逻辑后）

```bash
cd ~/cc-monitor-src && git pull          # 或重新 scp remote-daemon-proto/
cd remote-daemon-proto && cargo build --release
cp target/release/cc-monitor-remote ~/.cc-monitor/bin/   # 覆盖
# 重启 cc-monitor（它会重新 exec daemon）
```

> Phase 0 没有 daemon/client 版本协商（hello 里有 `build_id:"phase0-proto"` 但不强校验）。wire 在 Phase 0 期间冻结；客户端解析器**忽略未知 kind/字段不 panic**，所以 Phase 1 加 `event_id` 等是向前兼容的。版本/sha 协商是 Phase 1（SFTP + build_id）。
