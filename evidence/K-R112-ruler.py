#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R112` 的读数器：**这四条命令今天这一跳走哪条路**，以及两张申报表少了哪几行。

# 它是什么、**不是**什么

🔴 **它是读数器，不是第二副牙。** 判「有没有回落 / 有没有拼 shell」那副牙住在 Rust 判据里
（`cc_bus.rs::tests` 与 `tmux.rs::tests`，跑在门禁的 `cargo` 那一格）；判「退没退役」那副牙
住 `evidence/K-R111-ruler.py` 的 `BASELINE` 递减棘轮。
本文件**只出读数**：交回时那几个数（`EXEC_SITES` 16 → ?、逐行是哪几行出表、四条命令各走哪条路）
要能**现算**，而不是抄一份快照 —— `brief` 12：写「可重跑」就要给量具的住址。

⚠ 刻意**不给它第三副牙**（`K-R111 §H2` 那条理由的同族）：同一个事实两副牙，
其中一副会在阴性对照那一格替另一副挡枪，挡完之后「那张表到底有没有牙」就量不出来了。
⇒ 本文件**退出码恒 0**，除非它自己坏了（抽取器抓不到东西 ⇒ `exit 2`）。

# 量法（每一栏都写清「从哪派生」）

| 栏 | 从哪派生 |
|---|---|
| `EXEC_SITES` 条数与逐行 | 解析 `exec_site_registry.rs` 的 `const EXEC_SITES` 三元组表 |
| `SPAWNS` 条数与 cc-bus 那行 | 解析 `write_site_registry.rs` 的 `const SPAWNS` |
| 四条命令走哪条路 | 取那个函数**体**（按顶格行配边界，剥注释与串内容），看它命中哪一族标记 |
| 点名的帧原语在不在 | 现打 daemon 的 `main.rs::SUBCOMMANDS` ＋ `inbound.rs::COMMANDS`，**不抄名单** |
| 尺子B 的 `On` | 解析 `launch_wire.rs::TS_FALLBACK_REACH`（`KR112D3`：本件一个字节都不该动它） |

**剥法直接复用** `K-R111-ruler.py`（同一棵树上的同一件事不许有两份剥法 —— 那是本区判过的病）。

# 怎么跑

    python3 evidence/K-R112-ruler.py                 # 人读
    python3 evidence/K-R112-ruler.py --json          # 机器读
    python3 evidence/K-R112-ruler.py --src-root <树>/src-tauri/src --daemon-root <树>/remote-daemon-proto/src
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent


def _load_r111():
    """复用 `K-R111-ruler.py` 的剥法与表解析 —— 同一件事不许有两份实现。"""
    p = HERE / "K-R111-ruler.py"
    spec = importlib.util.spec_from_file_location("kr111_ruler", p)
    if spec is None or spec.loader is None:
        print(f"🔴 加载不了 {p} —— 本读数器的剥法住在那里，没有它一个数都不许信")
        sys.exit(2)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


R = _load_r111()

#: 本件动的那四条命令：`命令 → (住址文件, 被点名的函数, 它今天该走的那条 daemon 原语)`。
#: 🔴 **原语名字不在这里判死** —— 下面现打 daemon 两张表，不在表里就在读数里说出来。
FOUR: dict[str, tuple[str, str, str]] = {
    "cc_bus_kill": ("cc_bus.rs", "cc_bus_kill", "bus-kill"),
    "check_cc_bus_agent_online": ("cc_bus.rs", "check_cc_bus_agent_online", "bus-list"),
    "cc_bus_broadcast": ("cc_bus.rs", "cc_bus_broadcast", "bus-send"),
    "capture_remote_pane": ("tmux.rs", "capture_remote_pane", "capture-pane"),
}

#: 「这一跳走 shell」的标记。**认形态，不认某一个符号** —— 判「源码里还有没有
#: `Command::new`」是判写法：`K-R111` 现打过 `cc_bus.rs` 那 9 处里 6 处在文档注释里。
SHELL_MARKS: tuple[str, ...] = (
    "connect_and_exec_cmd(",
    "exec_read(",
    "local_shell_read(",
    "_cmd(",
    "shell_quote(",
    "cfg_of(",
    "load_remote_config_by_label(",
)

#: 「这一跳交给后端」的标记（转发口的名字）。
BACKEND_MARKS: tuple[str, ...] = (
    "kill_via_daemon",
    "online_via_daemon",
    "broadcast_via_daemon",
    "capture_via_daemon",
    "send_via_daemon",
    "client.call(",
)

#: 本件之前 `EXEC_SITES` 里属于这四条的行（`K-R111 §E2` 现打 16 条时的那张表）。
#: ⚠ 它是**上一次有人核过的答案**，不是今天的答案 —— 下面现打的与它的差就是本件的账。
WAS_16: tuple[tuple[str, str], ...] = (
    ("cc_bus.rs", "fetch_remote_cc_bus"),
    ("cc_bus.rs", "check_cc_bus_agent_online"),
    ("cc_bus.rs", "exec_read"),
    ("tmux.rs", "capture_remote_pane"),
    ("tmux.rs", "list_remote_tmux"),
    ("ccm_probe.rs", "probe_ccm_cli"),
    ("remote_history.rs", "run_list_query"),
    ("remote_history.rs", "stream_read_remote_session"),
)


def selftest(src: Path) -> list[str]:
    """★ 量具自检：抽取器真的抓得到东西。抓不到 ⇒ `exit 2`，**不出读数**。"""
    bad: list[str] = []
    lines = R.prod_lines((src / "cc_bus.rs").read_text(encoding="utf-8"))
    got = R.fn_body(lines, "cc_bus_kill")
    if got is None:
        bad.append("取不到 `cc_bus.rs::cc_bus_kill` 的函数体 —— 剥法或配对坏了")
    elif len("\n".join(got[1])) < 40:
        bad.append(f"`cc_bus_kill` 的体只切出 {len(''.join(got[1]))} 字 —— 抽取器坏了")
    # 反向：一段真拼 shell 的语料必须被认出来（不然「零命中」是空真）
    synth = "let cmd = build_x_cmd(&id)?;\nlet cfg = cfg_of(&origin)?;"
    if not [m for m in SHELL_MARKS if m in synth]:
        bad.append("`SHELL_MARKS` 对一份明摆着拼 shell 的语料都不响 —— 它此刻什么都没在认")
    if [m for m in BACKEND_MARKS if m in synth]:
        bad.append("`BACKEND_MARKS` 把一份拼 shell 的语料认成了走后端")
    return bad


def measure(src: Path, daemon: Path) -> dict:
    subs, frames = R.read_daemon_primitives(daemon)
    primitives = set(subs) | set(frames)
    execs = R.read_exec_sites(src)
    spawns = R.read_spawn_sites(src)
    _keepers, reach = R.read_ts_fallback(src)

    routes: dict[str, dict] = {}
    cache: dict[str, list[str]] = {}
    for cmd, (rel, fname, prim) in FOUR.items():
        if rel not in cache:
            cache[rel] = R.prod_lines((src / rel).read_text(encoding="utf-8"))
        got = R.fn_body(cache[rel], fname)
        if got is None:
            routes[cmd] = {"住址": f"{rel}::{fname}", "读数": "住址馊了：函数体取不到"}
            continue
        lineno, body = got
        blob = "\n".join(body)
        shell = [m for m in SHELL_MARKS if m in blob]
        backend = [m for m in BACKEND_MARKS if m in blob]
        routes[cmd] = {
            "住址": f"{rel}::{fname}:{lineno}",
            "走 shell 的痕迹": shell,
            "走后端的痕迹": backend,
            "这一跳": "后端帧" if (backend and not shell) else ("shell" if shell else "认不出"),
            "点名的原语": prim,
            "原语在 daemon 表里": prim in primitives,
        }

    now = {(f, fn) for f, fn in execs}
    gone = [f"{f}::{fn}" for f, fn in WAS_16 if (f, fn) not in now]
    stay = [f"{f}::{fn}" for f, fn in WAS_16 if (f, fn) in now]
    return {
        "EXEC_SITES": {
            "条数": len(execs),
            "本件之前": 16,
            "属于这四条那族里出表的": gone,
            "属于那族里留表的": stay,
            "逐行": [f"{f}::{fn}" for f, fn in execs],
        },
        "SPAWNS": {
            "条数": len(spawns),
            "cc_bus.rs::local_shell_read 还在不在": ("cc_bus.rs", "local_shell_read") in set(spawns),
        },
        "四条命令这一跳": routes,
        "daemon 分母": {"一次性子命令": len(subs), "流式帧": len(frames)},
        "尺子B": {
            "家数": len(reach),
            "On": sum(1 for _, r in reach if r == "OnProductionPath"),
        },
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="K-R112 读数器（只出读数，不判红）")
    ap.add_argument("--src-root", default=str(ROOT / "src-tauri" / "src"))
    ap.add_argument("--daemon-root", default=str(ROOT / "remote-daemon-proto" / "src"))
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()
    src, daemon = Path(a.src_root), Path(a.daemon_root)

    bad = selftest(src)
    if bad:
        print("🔴 量具自检没过（下面的读数一个都不许信）：")
        for b in bad:
            print("   ·", b)
        return 2

    m = measure(src, daemon)
    if a.json:
        print(json.dumps(m, ensure_ascii=False, indent=2))
        return 0

    print("═" * 78)
    print("K-R112 读数器 —— 四条命令这一跳走哪条路 · 两张申报表少了哪几行")
    print("═" * 78)
    print(f"被测对象：{src}")
    print(f"        ：{daemon}")
    print(f"daemon 分母：一次性子命令 {m['daemon 分母']['一次性子命令']} 条 ＋ 流式帧 "
          f"{m['daemon 分母']['流式帧']} 条（现打两张具名表，不抄名单）")
    print()
    print("── 四条命令" + "─" * 62)
    for cmd, d in m["四条命令这一跳"].items():
        if "读数" in d:
            print(f"  · {cmd:28} {d['读数']}  ⟨{d['住址']}⟩")
            continue
        ok = "✓" if d["原语在 daemon 表里"] else "✗ 不在 daemon 表里"
        print(f"  · {cmd:28} 这一跳：{d['这一跳']:6} ⟨{d['住址']}⟩")
        print(f"      点名原语 `{d['点名的原语']}` {ok}")
        print(f"      走 shell 的痕迹：{d['走 shell 的痕迹'] or '（一个都没有）'}")
        print(f"      走后端的痕迹：{d['走后端的痕迹'] or '（一个都没有）'}")
    print()
    e = m["EXEC_SITES"]
    print("── `exec_site_registry::EXEC_SITES`" + "─" * 42)
    print(f"  条数：{e['本件之前']} → **{e['条数']}**（分母 = 那张具名表现打的行数）")
    print(f"  出表：{e['属于这四条那族里出表的'] or '（一行都没出）'}")
    print(f"  留表（属于 `K-R111 §E2` 那 8 行里的）：{e['属于那族里留表的']}")
    print()
    s = m["SPAWNS"]
    print("── `write_site_registry::SPAWNS`" + "─" * 45)
    print(f"  条数：{s['条数']}")
    print(f"  `cc_bus.rs::local_shell_read` 还在不在：{'在' if s['cc_bus.rs::local_shell_read 还在不在'] else '不在'}")
    print()
    print("── 尺子B（`TS_FALLBACK_REACH`）—— `KR112D3`：本件一个字节都不该动它" + "─" * 6)
    print(f"  家数 {m['尺子B']['家数']} · On {m['尺子B']['On']}")
    print()
    print("（本读数器**不判红** —— 牙在 Rust 判据与 `K-R111-ruler.py` 的棘轮上，理由见头注）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
