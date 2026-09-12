#!/usr/bin/env python3
"""K-R63 变异台 —— 「申报 ↔ 现实」那条性质的死值验，一份可复跑的量具。

# 住址与被测对象（纪律 12：量具住址要能唯一定位到那一份）

  · 量具本体：本文件（`evidence/k-r63-claim-vs-reality-cuts.py`，随 `track/k-r63` 入库）
  · 被测对象：**本文件所在的那棵工作树**（`WT` 由本文件的位置现算，不写死路径 ——
    写死了，谁把它拷到另一棵树上跑出来的就是另一棵树的数，而输出长得一模一样）
  · 跑法：`python3 evidence/k-r63-claim-vs-reality-cuts.py`（在任意目录都行）
  · 台子：`ccmon-devbox:latest` 沙箱里的 `cargo test --lib -p monitor`
    （`DECISIONS.md#R21`：会执行被测代码的一律进沙箱）

# 每一刀都先断言锚点**恰好命中 N 次**再落刀，落刀后打印「变异已落地」，跑完**原样还原**
  并逐字节核一遍还原对不对（纪律 ⑬ / 第 7 条）。

# ⚠ 它买不到什么
  · 它只跑 `-p monitor` 一个包 —— 别的包的红它看不见（门禁那一格才是全量）；
  · 「最小面」这一栏是**这一刀下红了哪几条**，不是「这条判据只可能被这一刀打红」。
"""
import subprocess
import sys
from pathlib import Path

WT = Path(__file__).resolve().parent.parent
PROJ = WT.parent.parent.parent  # …/claudecode-frontend
SKILL = Path("/home/zbl/.claude-accts/z/skills/planned-build")
TARGET = PROJ / ".claude/pm-targets/k-r63"

# (标签, 相对路径, 锚点, 替换, 期望锚点命中次数)
CUTS = [
    (
        "M1 刀 C 重打：cc-acct-iso uninstallable false→true（多报方向 = 本件正题）",
        "src-tauri/src/tool_registry.rs",
        '        uninstallable: false,\n        touches: &[\n            TouchedFile {\n                path: "$ACCT_ISO_DEST",',
        '        uninstallable: true,\n        touches: &[\n            TouchedFile {\n                path: "$ACCT_ISO_DEST",',
        1,
    ),
    (
        "M2 remote-daemon uninstallable true→false（少报方向；这一刀同时证明本件改的那一格原先是红的）",
        "src-tauri/src/tool_registry.rs",
        '        uninstallable: true,\n        touches: &[TouchedFile {\n            path: "$DAEMON_PATH",',
        '        uninstallable: false,\n        touches: &[TouchedFile {\n            path: "$DAEMON_PATH",',
        1,
    ),
    (
        "M3 KR63D2：powershell-profile installable true→false（**不是 cc-bus** 的另一个工具）",
        "src-tauri/src/tool_registry.rs",
        '        installable: true,\n        uninstallable: true,\n        touches: &[TouchedFile {\n            path: "$PROFILE",',
        '        installable: false,\n        uninstallable: true,\n        touches: &[TouchedFile {\n            path: "$PROFILE",',
        1,
    ),
    (
        "M4 掏空 structural_scan::fn_names_starting_with 的读（签名留着，恒答空）",
        "src-tauri/src/structural_scan.rs",
        "    let prod = guard_core::production_code(text);",
        "    let prod = String::new();\n    let _ = text;",
        1,
    ),
    (
        "M5 FENCE_SHAPES 负向：account_aliases 里长出一个没人认领的卸口",
        "src-tauri/src/account_aliases.rs",
        "fn ensure_rc_source_line(home: &Path, rc_raw: &str, line: &str) -> Result<bool, String> {",
        "#[allow(dead_code)]\nfn uninstall_rc_source_line(_home: &Path) -> Result<bool, String> {\n    Ok(false)\n}\n\n"
        "fn ensure_rc_source_line(home: &Path, rc_raw: &str, line: &str) -> Result<bool, String> {",
        1,
    ),
    (
        "M6b TOOLS 负向：cc_bus_deploy 里长出一个没人认领的卸口"
        "（⚠ 锚点必须含 `#[tauri::command]` 那一行 —— M6 没含，把属性与它的 fn 劈开了，"
        "那一趟是 CRASH 不是读数）",
        "src-tauri/src/cc_bus_deploy.rs",
        "#[tauri::command]\npub async fn deploy_local_cc_bus() -> Result<CcBusDeployReport, String> {",
        "#[allow(dead_code)]\npub async fn uninstall_local_cc_bus() -> Result<(), String> {\n    Ok(())\n}\n\n"
        "#[tauri::command]\npub async fn deploy_local_cc_bus() -> Result<CcBusDeployReport, String> {",
        1,
    ),
    (
        "M7 被收掉那条专名判据的覆盖还在不在：cc-bus installable true→false",
        "src-tauri/src/tool_registry.rs",
        "        installable: true,\n        uninstallable: false,\n        touches: &[\n            TouchedFile {\n                // 🔴 **这一条是 `K-R60` 补的",
        "        installable: false,\n        uninstallable: false,\n        touches: &[\n            TouchedFile {\n                // 🔴 **这一条是 `K-R60` 补的",
        1,
    ),
    (
        "M8 失效方向：把那条性质改成名字里带工具 id `ccm`（全文逐字换名，定义与自守里的 NAME 一起换）",
        "src-tauri/src/tool_registry.rs",
        "every_tool_declares_install_and_uninstall_as_the_implementations_really_are",
        "every_ccm_and_friends_declare_install_and_uninstall_as_the_implementations_really_are",
        7,
    ),
]


def run_in_sandbox() -> str:
    cmd = [
        "docker", "run", "--rm", "--network", "none",
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={TARGET}",
        "-e", "HOME=/home/zbl",
        "-w", str(WT),
        "ccmon-devbox:latest",
        "bash", "-o", "pipefail", "-c",
        'mkdir -p "$HOME/.claude/projects" && cd src-tauri && cargo test --lib -p monitor 2>&1',
    ]
    return subprocess.run(cmd, capture_output=True, text=True).stdout


def report(out: str) -> None:
    saw_result = False
    for line in out.splitlines():
        if line.startswith("test result:"):
            saw_result = True
            print("  |", line)
        elif line.startswith("    ") and "::" in line and "Running" not in line:
            print("  | 红:", line.strip())
        elif line.startswith("error["):
            print("  | CRASH:", line)
    if not saw_result:
        print("  | ⚠ 一行 `test result:` 都没有 ⇒ 按 CRASH 记，不许报「新红 0」")


def main() -> int:
    print(f"被测对象：{WT}")
    print("== 基线（不落任何刀）==")
    report(run_in_sandbox())
    for label, rel, anchor, repl, want in CUTS:
        target = WT / rel
        orig = target.read_text(encoding="utf-8")
        n = orig.count(anchor)
        print(f"\n== [{label}]\n   文件 {rel} · 锚点命中 {n} 次（应为 {want}）")
        if n != want:
            print("   锚点命中数不对 —— 拒跑（纪律 ⑬：窗口别跨到下一条 ToolSpec）")
            continue
        target.write_text(orig.replace(anchor, repl), encoding="utf-8")
        print("   变异已落地：", target.read_text(encoding="utf-8") != orig)
        try:
            report(run_in_sandbox())
        finally:
            target.write_text(orig, encoding="utf-8")
            print("   已还原（逐字节回原文）:",
                  target.read_text(encoding="utf-8") == orig)
    return 0


if __name__ == "__main__":
    sys.exit(main())
