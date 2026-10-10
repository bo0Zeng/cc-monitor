#!/usr/bin/env python3
"""录一份 CLI 原生 stream-json 的 golden。

与 `record_vectors.py` 录的是两个不同的形状：

| | `record_vectors.py` | 本文件 |
|---|---|---|
| 产出方 | `claude-agent-sdk`（Python 对象） | `claude` CLI 本体 |
| 原始行 | `{"msg": {"__type__": "StreamEvent", …}}` | `{"line": {"type": "stream_event", …}}` |
| 生产上谁产出它 | 无，只当规格参照 | 常驻管道 |

用真实的管道形态录（`--input-format stream-json` 喂一行），不用 `claude -p "…"`：
后者的输入路径不同，录出来的不保证是管道那条路上的东西。

脱敏：`init` 带 `cwd` / `claude_dir` / 用户名，落盘前把家目录与用户名替换掉
（`/home/USER/REDACTED`）。

用法：
    python3 bridge/tools/record_cli_vectors.py --case cli-text-short
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
import time

# 与 `record_vectors.py` 的 `text-short` 同一个 prompt，两份 golden 才能逐帧对照。
CASES: dict[str, dict[str, str]] = {
    "cli-text-short": {
        "prompt": "Reply with exactly: hello",
        "why": "CLI 原生 stream-json 的最小样本。与 SDK 侧 `text-short` 同 prompt，"
        "供逐帧对照『帧型是否真的对得上』（ARCHITECTURE §2.2 的那句声称）。",
    },
}

PIPE_FLAGS = [
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--include-partial-messages",
    "--verbose",
]


def _redact(text: str) -> str:
    """把家目录与用户名换掉。只替换不删除，结构原样保留。"""
    home = os.path.expanduser("~")
    user = os.path.basename(home)
    text = text.replace(home, "/home/USER/REDACTED")
    if user:
        text = re.sub(rf"\b{re.escape(user)}\b", "USER", text)
    return text


def record(case: str, out_dir: pathlib.Path) -> pathlib.Path:
    spec = CASES[case]
    # 管道的上行形状（`PROTOCOL.md` 3.1）：一行 user 消息喂 stdin
    stdin_line = json.dumps(
        {"type": "user", "message": {"role": "user", "content": spec["prompt"]}},
        ensure_ascii=False,
    )
    t0 = time.monotonic_ns()
    proc = subprocess.run(
        ["claude", *PIPE_FLAGS],
        input=stdin_line + "\n",
        capture_output=True,
        text=True,
        timeout=180,
    )
    if proc.returncode != 0 and not proc.stdout.strip():
        sys.stderr.write(f"claude 失败 rc={proc.returncode}\nstderr:\n{proc.stderr[:2000]}\n")
        return pathlib.Path()

    lines = []
    for raw in proc.stdout.splitlines():
        raw = raw.strip()
        if not raw:
            continue
        try:
            obj = json.loads(raw)
        except json.JSONDecodeError:
            # 解析不了也留原文，不丢
            obj = {"__unparsed__": raw}
        lines.append(
            json.dumps(
                {"t_ns": time.monotonic_ns() - t0, "line": obj},
                ensure_ascii=False,
                separators=(",", ":"),
            )
        )

    header = json.dumps(
        {
            "__meta__": {
                "case": case,
                "why": spec["why"],
                "prompt": spec["prompt"],
                "recorded_by": "bridge/tools/record_cli_vectors.py (G1)",
                "shape": "CLI 原生 stream-json（**不是** SDK 对象转储）—— "
                "每行 `line` 是 `claude` 直接吐到 stdout 的那一行 JSON，逐字节原样。",
                "invocation": "claude " + " ".join(PIPE_FLAGS) + "（stdin 喂一行 user 消息）",
            }
        },
        ensure_ascii=False,
        separators=(",", ":"),
    )
    out_path = out_dir / f"{case}.cli.ndjson"
    out_path.write_text(_redact(header + "\n" + "\n".join(lines) + "\n"), encoding="utf-8")
    return out_path


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--case", required=True, choices=sorted(CASES))
    args = ap.parse_args()
    out_dir = pathlib.Path(__file__).resolve().parent.parent / "vectors"
    path = record(args.case, out_dir)
    if not path.name:
        return 1
    n = sum(1 for _ in path.open(encoding="utf-8")) - 1
    print(f"录到 {path}（{n} 行，不含 __meta__）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
