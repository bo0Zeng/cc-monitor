#!/usr/bin/env python3
"""K-P6 量具 ②：**往外拨 SSH 的扼流点普查** —— `KP6D1` 的 `§0a④` 第三、四格的分母。

## 它答什么

件文件 `§0a①` 只点了**两个**入口（`ssh_source.rs` 的 `client::connect` 与
`client::connect_stream`）。那两个是**最底下**那两条，不是「谁在拨号」的分母 ——
真正决定「本件要搬多大一块」的是**上面那层扼流点被谁调**：

    client::connect / client::connect_stream        ← §0a① 点的两处（russh 原语）
      └── connect_session()                          ← 连+鉴权+指纹校验，**一个**
            ├── connect_and_exec()      → 长连接数据源（daemon 流）
            ├── connect_and_exec_cmd()  → 一次性 exec，**全仓最宽的那条**
            ├── connect_and_exec_capture()
            ├── sftp::connect_sftp()    （`§2.1` 划在写区外）
            └── port_forward 那条隧道    （`§2.1` 划在写区外）

本量具把**每一层的调用点逐处列出来**，并按文件归堆。**这是 `KP6D3①` 那句
「射程内的那几份上归零」真正的分母** —— 只看 `russh` 这个词会漏掉它们
（`tmux.rs` / `cc_bus.rs` / `remote_history.rs` … 一个 `russh` 字都没有，
却每一个都在往外拨）。

## 口径（逐条写死）

- **人群**：`git ls-files -z` 里跟踪的 `.rs`，作用域 `src-tauri/src/`。
  （`-z` 是必需的：本仓路径含中文。）
- **词法**：复用 `K-P6-russh-ruler.py` 的 Rust 词法状态机（`classify_bytes`）——
  **刻意 import 而不是手抄一份**：仓里 `local_backend::local_extract_name` 的头注
  逐字记着「判据自己抄一份就成了测自己的副本」，本仓也栽过「两份手抄语义漂移」。
- **只认代码态命中**：注释里提一句、判据里断言一句字符串，都不算调用点。
- **逐处分三档**（互斥，按顺序取第一档）：
  · `定义` —— 这一行是 `fn <名>(` / `async fn <名>(` / `pub(crate) async fn <名>(`…
  · `调用` —— 代码态里出现 `<名>(`，且不是定义行
  · `提及` —— 代码态里出现 `<名>` 但后面不跟 `(`（如 `use` 引入、文档指针）
- **`#[cfg(test)]` 不剥**：本量具报整份文件，并把「行号是否落在某个 `#[cfg(test)]`
  之后」这件事**留给读者**（剥法要靠 `guard_core::production_code` 那把尺子，
  两把尺子的分母不同，混用就是本仓「分母差」那一族）。⚠ 因此本量具的数
  **≥** `exec_site_registry` 那条判据的数（它剥了测试段）—— 两个数不该被拿来对等。

## ⚠ 保证不了什么

- 它按**名字**找调用点。有人 `let f = connect_and_exec_cmd; f(cfg, cmd)` 就数不到。
- 它**不**判「这一处是不是真的会拨号」——`connect_and_exec_cmd` 每次都新建连接
  （`cc_bus.rs:160` 逐字「**不复用连接**」），但那是读来的，不是本量具量的。

## 住址与被测对象

量具住址：`evidence/K-P6-dial-census.py`（工作树 `.claude/worktrees/k-p6`）。
被测对象默认取本脚本所在仓的根；`--tree` 可换。输出头一行印树的绝对路径 + HEAD sha。
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import pathlib
import re
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve()

_spec = importlib.util.spec_from_file_location(
    "kp6_russh_ruler", HERE.parent / "K-P6-russh-ruler.py"
)
assert _spec and _spec.loader
_ruler = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_ruler)
classify_bytes = _ruler.classify_bytes
word_hits = _ruler.word_hits
line_of = _ruler.line_of
tracked_rs_files = _ruler.tracked_rs_files
CODE = _ruler.CODE

# 分层：(层号, 名字, 一句话说明)
NEEDLES = [
    (0, "client::connect_stream", "russh 原语（经跳板那条）"),
    (0, "client::connect", "russh 原语（直连那条）—— 注意它是 `connect_stream` 的前缀，见下"),
    (1, "connect_session", "连+鉴权+host key 指纹校验，**唯一**一份"),
    (2, "connect_and_exec_cmd", "一次性 exec 任意命令，全仓最宽的那条"),
    (2, "connect_and_exec_capture", "一次性 exec + 抓输出"),
    (2, "connect_and_exec", "长连接数据源（exec 远端 daemon 起流）"),
    (2, "connect_sftp", "SFTP 会话（`sftp.rs`，§2.1 划在写区外）"),
    (2, "channel_open_session", "russh 原语：在一条已连的会话上开 channel（层0 之后那一步）"),
]

DEF_RE = re.compile(r"\b(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)\s*[(<]")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--tree", default=str(HERE.parent.parent))
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    tree = pathlib.Path(args.tree).resolve()
    head = subprocess.run(
        ["git", "-C", str(tree), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()

    files = tracked_rs_files(tree, "src-tauri")
    # name -> list of (file, line, kind, text)
    found: dict[str, list[tuple[str, int, str, str]]] = {n: [] for _, n, _ in NEEDLES}

    for rel in files:
        src = (tree / rel).read_text("utf-8", "surrogateescape")
        if not any(n in src for _, n, _ in NEEDLES):
            continue
        states = classify_bytes(src)
        lines = src.splitlines()
        # 长名先吃掉，短名不再在同一处重复计（`connect_and_exec` 是
        # `connect_and_exec_cmd` 的前缀；`client::connect` 是 `client::connect_stream` 的前缀）
        claimed: set[int] = set()
        for _, name, _ in sorted(NEEDLES, key=lambda t: -len(t[1])):
            for idx in _plain_hits(src, name):
                if states[idx] != CODE:
                    continue
                if any(idx == c for c in claimed):
                    continue
                claimed.add(idx)
                ln = line_of(src, idx)
                text = lines[ln - 1].strip()
                after = src[idx + len(name) : idx + len(name) + 1]
                m = DEF_RE.search(text)
                if m and m.group(1) == name.split("::")[-1]:
                    kind = "定义"
                elif after == "(":
                    kind = "调用"
                else:
                    kind = "提及"
                found[name].append((str(rel), ln, kind, text[:120]))

    if args.json:
        print(
            json.dumps(
                {"tree": str(tree), "head": head, "found": found},
                ensure_ascii=False,
                indent=2,
            )
        )
        return 0

    w = 92
    print("=" * w)
    print(f"K-P6 · 往外拨 SSH 的扼流点普查   树={tree}")
    print(f"                                 HEAD={head}")
    print("=" * w)
    print(f"【人群】`git ls-files -z` 里 `src-tauri/src/**.rs` 共 {len(files)} 份")
    print("-" * w)
    for layer, name, why in NEEDLES:
        rows = found[name]
        calls = [r for r in rows if r[2] == "调用"]
        defs = [r for r in rows if r[2] == "定义"]
        ment = [r for r in rows if r[2] == "提及"]
        by_file = sorted({r[0] for r in calls})
        print(
            f"层{layer} `{name}` —— {why}\n"
            f"     定义 {len(defs)} · **调用 {len(calls)}** · 提及 {len(ment)}"
            f" · 调用散在 {len(by_file)} 份文件"
        )
        for f, ln, kind, text in rows:
            print(f"       {kind}  {f}:{ln}   {text}")
        print("-" * w)

    call_files: set[str] = set()
    for _, name, _ in NEEDLES:
        if name in ("client::connect", "client::connect_stream"):
            continue
        for f, _ln, kind, _t in found[name]:
            if kind == "调用":
                call_files.add(f)
    print(
        f"【合计】层1+层2 的**调用点**散在 {len(call_files)} 份文件里："
        + "、".join(sorted(pathlib.Path(f).name for f in call_files))
    )
    print("=" * w)
    return 0


def _plain_hits(src: str, name: str) -> list[int]:
    """`name` 里带 `::` 时按纯子串找（`::` 不是标识符字符，边界规则不适用）；
    否则按标识符边界找。"""
    if "::" in name:
        out = []
        start = 0
        while True:
            i = src.find(name, start)
            if i < 0:
                return out
            after = i + len(name)
            if after >= len(src) or src[after] not in _ruler.WORD_CHARS:
                out.append(i)
            start = i + 1
    return word_hits(src, name)


if __name__ == "__main__":
    sys.exit(main())
