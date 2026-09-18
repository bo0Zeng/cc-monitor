#!/usr/bin/env python3
"""K-R78：`connect_sftp` 调用点普查 + 三类分工的读数。

🔴 **这是第二条腿，不是真相源**。真相源是 `src-tauri/src/sftp_move_ledger.rs` 里那张
`DIAL_CENSUS`（它由 Rust 判据从源码派生再逐格比对，改坏当场红）。
本脚本存在的理由只有一个：让 PM 不进沙箱也能复算一遍同一个数。

⚠ 量具的洞（与 `dial_move_judge::call_sites` 同一个，**不声称堵住**）：
`use` 别名 · 函数指针 · 宏里拼出来的调用，一律数不到。

⚠ 剥测试段用的是**粗剥**（按 `#[cfg(test)]` 行首锚点截到文件尾），
与 Rust 侧 `guard_core::production_code`（按花括号配平逐块剥）**不是同一把尺子**。
本仓这四份文件的测试段都在文件末尾，所以两把尺子今天同值 —— 逐份读数在下面印出来，
对不上就是这条前提破了。

用法：python3 evidence/K-R78-dial-census.py   （在仓根跑）
"""

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
SRC = ROOT / "src-tauri" / "src"

FILES = ["sftp.rs", "sftp_pool.rs", "mcp.rs", "acct_iso_deploy.rs"]


def production(text: str) -> str:
    """粗剥：第一个行首 `#[cfg(test)]` 之后全丢。"""
    for i, line in enumerate(text.splitlines(keepends=True)):
        if line.startswith("#[cfg(test)]"):
            return "".join(text.splitlines(keepends=True)[:i])
    return text


def call_sites(code: str, name: str) -> list[int]:
    """回调用点的行号（1 基）—— 剔掉定义行本身（`fn <name>(`）。"""
    hits = []
    needle = name + "("
    for n, line in enumerate(code.splitlines(), 1):
        start = 0
        while True:
            at = line.find(needle, start)
            if at < 0:
                break
            start = at + len(needle)
            if line[:at].endswith("fn "):
                continue
            hits.append(n)
    return hits


def main() -> int:
    total = 0
    print("== `connect_sftp` 调用点普查（生产段）==")
    for f in FILES:
        raw = (SRC / f).read_text(encoding="utf-8")
        prod = production(raw)
        hits = call_sites(prod, "connect_sftp")
        total += len(hits)
        print(f"  {f:22s} 生产段 {len(prod):7d} B / 全文 {len(raw):7d} B"
              f"  ·  connect_sftp 调用点 {len(hits)} 处  行号 {hits}")
    print(f"  ── 合计 **{total} 处 / {len(FILES)} 份文件**")
    print("     （派工单写的是「9 处 / 4 文件」—— 那张分类表自己加起来是 5+4+3=12，")
    print("       而 `sftp.rs` 里另有 2 处它一类都没归：ensure_daemon_deployed · remove_remote_file）")

    print()
    print("== 乙（`sftp_pool.rs`）的形状 ==")
    pool = production((SRC / "sftp_pool.rs").read_text(encoding="utf-8"))
    print(f"  #[tauri::command] {pool.count('#[tauri::command]')} 条")
    print(f"  connect_sftp 调用点 {len(call_sites(pool, 'connect_sftp'))} 处")
    for what, pin in [
        ("进度通道", "Channel<TransferProgress>"),
        ("per-origin 连接池", "static POOL: std::sync::OnceLock<Mutex<HashMap<String, Slot>>>"),
        ("死连接重建重试", "Err(e) if looks_like_dead_conn(&e) =>"),
    ]:
        print(f"  {what:18s} 校验位命中 {pool.count(pin)} 处  ←  {pin}")

    print()
    print("== 甲那四条命令住哪 ==")
    sftp = production((SRC / "sftp.rs").read_text(encoding="utf-8"))
    print(f"  sftp.rs 的 #[tauri::command] {sftp.count('#[tauri::command]')} 条")
    for fn in [
        "deploy_remote_daemon",
        "uninstall_remote_daemon",
        "install_remote_ccm_helper",
        "uninstall_remote_ccm_helper",
    ]:
        print(f"    pub async fn {fn}  定义 {sftp.count('pub async fn ' + fn)} 处")

    print()
    print("== 挡路石的逐字校验位（甲）==")
    for pin, where in [
        ("daemon 不许改动用户既有数据", "doc/INVARIANTS.md"),
        ('vec!["sftp.rs".to_string()]', "src-tauri/src/profile_installer.rs"),
        ("crate::sftp::daemon_binary(std::env::consts::ARCH)", "src-tauri/src/local_daemon.rs"),
        ("pub(crate) async fn upload_atomic_verified", "src-tauri/src/sftp.rs"),
    ]:
        text = (ROOT / where).read_text(encoding="utf-8")
        print(f"  {where:38s} 命中 {text.count(pin)} 处  ←  {pin}")

    print()
    print("== daemon 那一侧（**本脚本只是 grep，Rust 侧刻意没有机检 —— 不长跨半边的编译期边**）==")
    dmn = (ROOT / "remote-daemon-proto" / "Cargo.toml").read_text(encoding="utf-8")
    mon = (ROOT / "src-tauri" / "Cargo.toml").read_text(encoding="utf-8")
    for name in ["russh =", "russh-sftp =", "tauri ="]:
        print(f"  {name:14s} daemon {dmn.count(chr(10) + name)} 处 / monitor {mon.count(chr(10) + name)} 处")
    guard = (ROOT / "remote-daemon-proto" / "src" / "readonly_guard.rs").read_text(encoding="utf-8")
    print(f"  readonly_guard 的 FS_MUTATION_PATTERNS 全在 `fs::`/`File::`/`OpenOptions` 命名空间："
          f"{'sftp' not in guard.split('FS_MUTATION_PATTERNS')[1].split(']')[0]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
