#!/usr/bin/env python3
"""K-R59 射程现打：**删过界也要红** 那一侧的正向读数（09-11，实现方）。

`KR59D1` 逐字：「`ssh_source` 的三块非本件能力（russh 数据源 · `ssh -G` 导入 · 测试连接）
与 `connect_and_exec_cmd`/`shell_quote` 的现有消费者**一个不少**」。
本量具**只出读数**，不判红 —— 判红那一侧是 `exec_site_registry` / `dial_move_judge`
/ `tmux_daemon_gate_guard`（死值验 `M4` 现打过它们真会咬）。

住址唯一：`evidence/K-R59-scope-census.py`；被测对象 = 本文件所在工作树（不写死路径）。
"""
import pathlib
import subprocess

WT = pathlib.Path(__file__).resolve().parent.parent


def count(pattern: str, *paths: str) -> int:
    r = subprocess.run(["grep", "-rn", "-e", pattern, *paths],
                       cwd=WT, capture_output=True, text=True)
    return len([l for l in r.stdout.splitlines() if l.strip()])


ROWS = [
    ("russh 数据源：`connect_session(` 的生产+测试处数", "connect_session(", "src-tauri/src/ssh_source.rs"),
    # ⚠ 第一版这一行的针写的是 `ssh_config`，读数是 **0** —— 那不是「没了」，是**针错了**：
    #    这一族的名字是 `list_ssh_host_aliases` / `resolve_ssh_host`（模块头注第 9 行逐字）。
    #    留着这段是因为它正是「量具作用域对不上事实」那一族最便宜的形状。
    ("`ssh -G` 导入：`resolve_ssh_host`", "resolve_ssh_host", "src-tauri/src/ssh_source.rs"),
    ("`ssh -G` 导入：`list_ssh_host_aliases`", "list_ssh_host_aliases", "src-tauri/src/ssh_source.rs"),
    ("测试连接：`test_remote_connection`", "test_remote_connection", "src-tauri/src"),
    ("`connect_and_exec_cmd(` 全树调用点（含定义行）", "connect_and_exec_cmd(", "src-tauri/src"),
    ("`shell_quote(` 全树处数", "shell_quote(", "src-tauri/src"),
    ("`connect_and_exec_capture(` 全树处数", "connect_and_exec_capture(", "src-tauri/src"),
]

if __name__ == "__main__":
    print(f"# 量于工作树 {WT}")
    print(subprocess.run(["git", "rev-parse", "HEAD"], cwd=WT,
                         capture_output=True, text=True).stdout.strip())
    for why, pat, path in ROWS:
        print(f"{count(pat, path):5d}  {why}   〔grep -rn '{pat}' {path}〕")
