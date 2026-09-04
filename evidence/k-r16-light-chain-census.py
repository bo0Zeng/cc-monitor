#!/usr/bin/env python3
"""K-R16 D1 量具：红绿灯那条链，**每一跳分别数「盘上有」与「被谁引用」**。

⚠ 尺子怎么切的（先说清，再报数）：

  - **分母**：下面 `HOPS` 里手工点名的那些符号/字面量。这是一张**手工清单**，
    只能回答「我点名的这几跳各自被谁引用」，**回答不了「链上有没有我没点到的跳」**。
  - 「引用」= 正则在源码文本里命中，**不是**调用图。宏、trait 分发、字符串反射三类会漏；
    `serde` 的字段消费（`SessionInfo.status`）更是**根本没有调用点**。
  - **每一跳带自己的作用域** `scope`（文件名/路径子串白名单）。没有 scope 的跳量全仓。
    加 scope 是因为第一版栽过：`updatedAt` 与 `mtime` 在 `history.rs` / `accounts.ts` /
    `search.rs` 里各有一个**同名但不同物**的用法，全仓量出来的 13 / 36 个「生产命中」
    **一个都不是 pidfile 那条链上的**。⇒ 报数前先问「这把尺子量的是不是我要的那件事」。
  - **测试段的切法**（第一版也栽过，这是订正后的）：
      * `.rs`：**花括号配对**找出每个 `#[cfg(test)]` 所辖块的真实行区间，只有落在
        这些区间里的命中才算测试段。
        第一版用的是「文件里第一个 `#[cfg(test)]` 往后全算测试」——
        `remote-daemon-proto/src/observe/watcher.rs` 的第一个 `#[cfg(test)]` 在 **:70**，
        全文 4889 行 ⇒ 那把尺子把 **4820 行生产代码误判成测试段**，
        于是 `SessionEntry` / `meta.status =` 被报成「生产段零引用」。**那是尺子的错，不是代码的病。**
      * `.ts`：文件名带 `.vitest.` / `.spec.` / `.e2e.`，或路径里有 `e2e/`。
      * 其余算生产段。
  - 🔴 **「生产段有引用」≠「今天真被走到」**。本脚本只量「盘上有」这一维；
    「被走到」要靠跑起来的读数（D2 探针 / 门禁里真跑过的测试 / 真机日志），
    在件文件 §7 的 D1 表里**单独一列**写，**不由本脚本给**。

用法：`python3 evidence/k-r16-light-chain-census.py [仓根]`
"""

import json
import pathlib
import re
import sys

# (跳号, 人话说明, 正则, 作用域文件子串白名单 or None=全仓)
CHAIN = [
    "session_map.rs",
    "observe/watcher.rs",
    "ssh_source.rs",
    "src-tauri/src/lib.rs",
    "bridge.rs",
    "src/events.ts",
    "src/tabs.ts",
    "src/session-status.ts",
    "src/views/grid-monitor.ts",
    "src/main.ts",
    "src/styles.css",
    "wire.rs",
]

HOPS: list[tuple[str, str, str, list[str] | None]] = [
    # --- 本地路 ---
    ("L2", "SessionInfo 结构体（serde 反序列化落点）", r"\bSessionInfo\b", None),
    ("L3", "scan_dir（重扫整个 sessions/ 目录）", r"\bscan_dir\s*\(", ["session_map.rs"]),
    ("L4", "diff_sessions（算 status_changed 的唯一实现点）", r"\bdiff_sessions\s*\(", None),
    ("L5", "run_watcher（notify 事件 + 2s 心跳双触发）", r"\brun_watcher\s*\(", ["session_map.rs"]),
    ("L6", "is_process_alive（心跳那一路的判活）", r"\bis_process_alive\s*\(", ["session_map.rs"]),
    ("L7", "SessionChange.status_changed（承载灯的那个字段）", r"\bstatus_changed\b", None),
    ("L8", "SessionMap::snapshot_activity（F5 快照源）", r"\bsnapshot_activity\s*\(", None),
    ("L9", "list_session_activity（快照 IPC）", r"\blist_session_activity\b", None),
    ("L10", "events::SESSION_ACTIVITY（Tauri 事件名常量）", r"\bSESSION_ACTIVITY\b", None),
    ("L11", "SessionActivityPayload（过线的那个结构）", r"\bSessionActivityPayload\b", None),
    # --- 远端路 ---
    ("R1", "daemon: Frame/InboundFrame::SessionStatus", r"Frame::SessionStatus", None),
    ("R2", "daemon: SessionEntry（差分用的上一次值住这里）", r"\bSessionEntry\b", None),
    ("R3", '线上 kind="session_status" 字面量', r'"session_status"', None),
    ("R4", "ssh_source: announced meta.status 回写（F28 重发）", r"meta\.status\s*=", ["ssh_source.rs"]),
    # --- 前端路 ---
    ("F1", 'events.ts 订阅 "session-activity"', r'"session-activity"', None),
    ("F2", "TabManager.updateActivity", r"\bupdateActivity\s*\(", None),
    ("F3", "TabManager.syncActivitySnapshot（F5 收敛）", r"\bsyncActivitySnapshot\s*\(", None),
    ("F4", "activityLightClass（灯色语义单一事实源）", r"\bactivityLightClass\s*\(", None),
    ("F6", "CSS 类 act-idle / act-waiting", r"\bact-idle\b|\bact-waiting\b", None),
    ("F7", "tmuxIdle / tmux-idle（灰灯，与红绿黄正交）", r"\btmuxIdle\b|\btmux-idle\b", None),
    ("F8", "markTmuxIdle（置灰入口）", r"\bmarkTmuxIdle\s*\(", None),
    # --- 从来没人读的那些字段（本件的钥匙）；作用域**必须**限到 pidfile 那条链 ---
    ("X1", "statusUpdatedAt —— CC 自己写的「这个 status 什么时候定的」", r"statusUpdatedAt", None),
    ("X2", "pidfile 的 updatedAt（限本链，避开 accounts/history 的同名字段）", r"updatedAt", CHAIN),
    ("X3", "messagingSocketPath / cc-socks —— CC 自己开的活体通道", r"messagingSocketPath|cc-socks", None),
    ("X4", "peerFeatures / notify_idle —— CC 宣告的能力位", r"peerFeatures|notify_idle", None),
    ("X5", "pidfile 的 mtime（限本链：灯链上有没有人读文件时间）", r"\bmtime\b|st_mtime|modified\s*\(\)", CHAIN),
    ("X6", "pidfile 的 tmux 字段（CC 自己记的 pane，§6 那条边界的对侧）", r'"tmux"\s*:|\btmux_pane\b', CHAIN),
]

SRC_DIRS = ["src", "src-tauri/src", "remote-daemon-proto/src", "shared", "e2e"]
# ⚠ 第一版这里漏了 `.sh` / `.mts` / `.mjs`，而**门禁跑的那四套 e2e 全是 `.sh`**
#   （`e2e/ccm-print-parity.sh` · `ccm-rbind-title.sh` · `ccm-cli.test.sh` · `ccm-contract-parity.sh`）
#   ⇒ 那一版的「e2e 零覆盖」是靠另跑一遍裸 `grep -r e2e/` 得出的，**不是本尺子量的**。
#   尺子要自洽：把它们收进来，让「e2e 零命中」这句话由同一把尺子负责。
EXT = {".rs", ".ts", ".js", ".css", ".html", ".py", ".sh", ".mts", ".mjs", ""}
SKIP_DIRS = {"node_modules", "target", ".git", "dist", "generated"}


def is_test_file_ts(p: pathlib.Path) -> bool:
    n = p.name
    if "e2e" in p.parts:  # e2e/ 下一律算测试段（含 .sh / .mts）
        return True
    return ".vitest." in n or ".spec." in n or ".e2e." in n or n.endswith(".test.ts")


def rust_test_ranges(lines: list[str]) -> list[tuple[int, int]]:
    """`.rs`：花括号配对求出每个 `#[cfg(test)]` 所辖块的行区间（1-based，闭区间）。

    做法：见到 `#[cfg(test)]` 后，从下一行起找第一个 `{`，然后按 `{`/`}` 计深度直到归零。
    字符串/注释里的花括号会算错——本仓测试段里 `r#"{...}"#` 很常见，故**额外剥掉**
    行内的 `//` 注释与最朴素的字符串字面量再计数（不做完整词法分析，够用即可）。
    """
    ranges: list[tuple[int, int]] = []
    n = len(lines)
    i = 0
    while i < n:
        if lines[i].lstrip().startswith("#[cfg(test)]"):
            j = i + 1
            depth = 0
            started = False
            start_line = i + 1
            while j < n:
                s = strip_noise(lines[j])
                for ch in s:
                    if ch == "{":
                        depth += 1
                        started = True
                    elif ch == "}":
                        depth -= 1
                if started and depth <= 0:
                    ranges.append((start_line, j + 1))
                    i = j
                    break
                j += 1
            else:
                ranges.append((start_line, n))
                break
        i += 1
    return ranges


_STR = re.compile(r'r?#*"(?:\\.|[^"\\])*"#*|\'(?:\\.|[^\'\\])*\'')


def strip_noise(line: str) -> str:
    line = _STR.sub("", line)
    k = line.find("//")
    if k >= 0:
        line = line[:k]
    return line


def in_ranges(i: int, ranges: list[tuple[int, int]]) -> bool:
    return any(a <= i <= b for a, b in ranges)


def main() -> int:
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    files: list[pathlib.Path] = []
    for d in SRC_DIRS:
        base = root / d
        if not base.exists():
            continue
        for p in ([base] if base.is_file() else base.rglob("*")):
            if not p.is_file() or SKIP_DIRS & set(p.parts):
                continue
            if p.suffix in EXT or p.name == "ccm":
                files.append(p)

    cache: dict[pathlib.Path, tuple[list[str], list[tuple[int, int]]]] = {}
    for p in files:
        try:
            lines = p.read_text(errors="replace").splitlines()
        except OSError:
            continue
        cache[p] = (lines, rust_test_ranges(lines) if p.suffix == ".rs" else [])

    rows = []
    for hop, desc, pat, scope in HOPS:
        rx = re.compile(pat)
        prod: list[str] = []
        test: list[str] = []
        for p, (lines, tranges) in cache.items():
            rel = str(p.relative_to(root))
            if scope and not any(s in rel for s in scope):
                continue
            # 非 .rs 的测试段判定（.rs 走花括号配对的 `tranges`）。
            # ⚠ 第一版这里写的是 `p.suffix in (".ts", ".js") and …`，
            #   于是 `e2e/*.sh` 即便被收进来也会被算成**生产段** —— 尺子的两半对不上。
            ts_test = p.suffix != ".rs" and is_test_file_ts(p)
            for i, line in enumerate(lines, 1):
                if not rx.search(line):
                    continue
                (test if (ts_test or in_ranges(i, tranges)) else prod).append(f"{rel}:{i}")
        rows.append(
            {
                "hop": hop,
                "desc": desc,
                "scope": scope or "全仓",
                "prod_hits": len(prod),
                "test_hits": len(test),
                "prod_sample": prod[:8],
                "test_sample": test[:4],
                "verdict": (
                    "盘上没有" if not prod and not test
                    else "只有测试段引用（生产段零引用）" if not prod
                    else "只有生产段引用（零测试）" if not test
                    else "生产+测试都有"
                ),
            }
        )

    print(json.dumps({"root": str(root), "files_scanned": len(cache), "hops": rows}, ensure_ascii=False, indent=2))
    print("\n=== 一屏表（只量「盘上有」；「被走到」另给）===", file=sys.stderr)
    for r in rows:
        print(f"{r['hop']:<4} prod={r['prod_hits']:<4} test={r['test_hits']:<4} {r['verdict']:<24} {r['desc']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
