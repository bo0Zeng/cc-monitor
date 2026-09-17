#!/usr/bin/env python3
"""K-P6 量具 ③：**本轮所有 `路径:行号` 的校验位** —— `brief` 13c 的机器形态。

`brief` 13c 逐字：「**指进本树的行号，要么带校验位，要么不写。**
校验位 = 把那一行的**逐字内容**抄在旁边，或者把住址**钉在一个 sha 上。**」
而同一条的诚实边界也写着：`F43-D1` 报的 **5 个行号今天全部 +49**，照抄进去五个全指错。

⇒ 本量具把 `evidence/K-P6-readings.md` 里**每一个**指进本树的行号，
连同它旁边抄的那一行逐字内容，做成一张表，**跑一遍就知道有没有漂**。
不符一处就非零退出并逐处点名 —— 下一轮谁引用那份读数，先跑这一条。

## 口径

- 比较用 `rstrip()`（行尾空白不算漂），其余**逐字节**。
- 越界（文件变短了）单独记成一档，不与「内容不符」混。
- 分母 = 下面 `PINS` 表的长度，**现算**（不写死一个基数 —— `brief` 13b）。
- 被测对象 = 本脚本所在仓的根；`--tree` 可换。输出印树路径 + HEAD sha。

## ⚠ 它保证不了什么

它证明的是「**这个行号在这棵树上指的就是这一行**」，**不**证明那一行支撑得起读数里的结论。
结论对不对要人读；本量具只挡「行号漂了而读数照抄」这一形。
"""

from __future__ import annotations

import argparse
import pathlib
import subprocess
import sys

# (相对路径, 行号, 那一行的逐字内容)
PINS: list[tuple[str, int, str]] = [
    # ── §0a① 的两个拨号入口 + 全文行数那份文件 ─────────────────────
    ("src-tauri/src/ssh_source.rs", 573, "                match client::connect(config, (ep.host.as_str(), ep.port), handler).await {"),
    ("src-tauri/src/ssh_source.rs", 687, "    let session = client::connect_stream(config, stream, handler)"),
    ("remote-daemon-proto/src/inbound.rs", 22, "//! 载体是现成的：monitor 那头拿的是 `russh::ChannelStream`，**双工**，"),
    # ── §0a② 里「不是解析」的那两处 ────────────────────────────────
    ("src-tauri/src/local_daemon.rs", 865, "                crate::ssh_source::record_tmux_raw(crate::inbound_client::LOCAL_ORIGIN, raw.clone());"),
    ("src-tauri/src/local_daemon.rs", 871, "        crate::ssh_source::forget_tmux_raw(crate::inbound_client::LOCAL_ORIGIN);"),
    # ── §0a③ 分母校正的三处 ────────────────────────────────────────
    ("src-tauri/src/sftp.rs", 31, "use russh::client;"),
    ("src-tauri/src/port_forward.rs", 162, "            russh::Disconnect::ByApplication,"),
    ("src-tauri/src/port_forward.rs", 94, "    let session = Arc::new(session); // russh Handle 不 Clone → Arc 共享"),
    ("src-tauri/src/remote_write_registry.rs", 260, "                || prod.contains(\"russh_sftp\")"),
    ("src-tauri/src/structural_scan.rs", 1449, "            (\"lib.rs\", \"russh-sftp-2.3.0/src/protocol/file_attrs.rs\", 29),"),
    ("src-tauri/Cargo.toml", 90, "russh = { version = \"0.61.1\", default-features = false, features = [\"ring\", \"flate2\", \"rsa\"] }"),
    ("src-tauri/Cargo.toml", 95, "russh-sftp = \"2\""),
    # ── §0a④ 甲：协议面 ────────────────────────────────────────────
    ("remote-daemon-proto/src/inbound.rs", 78, "pub const COMMANDS: &[&str] ="),
    ("remote-daemon-proto/src/inbound.rs", 79, "    &[\"bus-kill\", \"bus-list\", \"bus-send\", \"cancel\", \"kill\", \"launch\", \"ping\", \"resolve\"];"),
    ("src-tauri/src/ssh_source.rs", 1304, "pub async fn connect_and_exec("),
    ("src-tauri/src/ssh_source.rs", 3763, "async fn stream_loop("),
    # ── §0a④ 乙：两条 Linux 闸 + 第三个答案那条路 ────────────────
    ("src-tauri/src/local_daemon.rs", 1352, "    // ★★ **内嵌的那两份是 musl LINUX 二进制，本机不是 Linux 就一份都不能用**"),
    ("src-tauri/src/local_daemon.rs", 1372, "    let embedded = if cfg!(target_os = \"linux\") {"),
    ("src-tauri/src/local_daemon.rs", 902, "        cfg!(target_os = \"linux\"),"),
    ("src-tauri/src/local_daemon.rs", 667, "pub(crate) fn detach_wanted(is_linux: bool, no_detach_env: Option<&str>) -> bool {"),
    ("src-tauri/src/local_daemon.rs", 669, "    is_linux && !no_detach_env.is_some_and(|v| !v.trim().is_empty())"),
    ("src-tauri/src/backend/control/local_backend.rs", 578, "pub fn resolve_beside_this_exe(target_triple: &str) -> Resolved {"),
    (".github/workflows/release.yml", 151, "      - name: Build local backend sidecar (native)"),
    (".github/workflows/release.yml", 152, "        working-directory: remote-daemon-proto"),
    (".github/workflows/release.yml", 153, "        run: cargo build --release"),
    # ── §0a④ 丙丁：SFTP / 端口转发的连带 ──────────────────────────
    ("src-tauri/src/sftp.rs", 47, "pub async fn connect_sftp(cfg: &RemoteConfig) -> Result<SftpConn, String> {"),
    ("src-tauri/src/sftp.rs", 48, "    let (session, _fp) = connect_session(cfg, None, None).await?;"),
    ("src-tauri/src/port_forward.rs", 91, "    let (session, _fp) = ssh_source::connect_session(&cfg, None, None)"),
    ("src-tauri/src/port_forward.rs", 2, "//! cc-monitor 已有 SSH 连接隧道(复用 `connect_session` → 自动继承 F45 竞速/F56 跳板 +"),
    # ── ⑤ 扼流点 ───────────────────────────────────────────────────
    ("src-tauri/src/ssh_source.rs", 701, "pub(crate) async fn connect_session("),
    ("src-tauri/src/ssh_source.rs", 2078, "pub async fn connect_and_exec_cmd("),
    ("src-tauri/src/ssh_source.rs", 2252, "pub async fn connect_and_exec_capture("),
    # ── ⑥甲 会合面那四处 ──────────────────────────────────────────
    ("src-tauri/src/local_daemon.rs", 169, "fn token_path(dir: &std::path::Path) -> std::path::PathBuf {"),
    ("src-tauri/src/local_daemon.rs", 174, "fn pid_path(dir: &std::path::Path, port: u16) -> std::path::PathBuf {"),
    ("src-tauri/src/local_daemon.rs", 336, "fn read_listen_owner(dir: &std::path::Path, port: u16) -> Option<(u32, std::path::PathBuf)> {"),
    ("src-tauri/src/local_daemon.rs", 1165, "    let exe = std::fs::read_link(format!(\"/proc/{pid}/exe\"))"),
    # ── ⑦ 文案面 ──────────────────────────────────────────────────
    ("src-tauri/src/ssh_source.rs", 422, "pub enum ConnectStage {"),
    ("src-tauri/src/ssh_source.rs", 442, "pub fn classify_stage(err: &str) -> &'static str {"),
    ("src/settings/machine-card.ts", 121, "export function describeStage(st: ConnectStage): {"),
]


def main() -> int:
    here = pathlib.Path(__file__).resolve()
    ap = argparse.ArgumentParser()
    ap.add_argument("--tree", default=str(here.parent.parent))
    args = ap.parse_args()
    tree = pathlib.Path(args.tree).resolve()
    head = subprocess.run(
        ["git", "-C", str(tree), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()

    print(f"K-P6 · 行号校验位   树={tree}")
    print(f"                    HEAD={head}")
    print(f"分母（现算）：{len(PINS)} 处")

    drift: list[str] = []
    oob: list[str] = []
    for rel, ln, want in PINS:
        p = tree / rel
        if not p.exists():
            oob.append(f"{rel}:{ln}  ← 文件不在了")
            continue
        lines = p.read_text("utf-8", "surrogateescape").splitlines()
        if ln - 1 >= len(lines):
            oob.append(f"{rel}:{ln}  ← 越界（该文件只有 {len(lines)} 行）")
            continue
        got = lines[ln - 1]
        if got.rstrip() != want.rstrip():
            drift.append(f"{rel}:{ln}\n    抄的：{want}\n    盘上：{got}")

    if not drift and not oob:
        print(f"✅ {len(PINS)} 处逐字相符，一处没漂")
        return 0
    print(f"❌ 漂了 {len(drift)} 处 · 越界/文件不在 {len(oob)} 处")
    for d in drift:
        print("  " + d)
    for o in oob:
        print("  " + o)
    return 1


if __name__ == "__main__":
    sys.exit(main())
