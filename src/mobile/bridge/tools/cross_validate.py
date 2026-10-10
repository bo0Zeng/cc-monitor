#!/usr/bin/env python3
"""流式消息与磁盘会话记录（JSONL）对账。

断线续读与「增量丢了不用补，块全文会覆盖」都建立在一个前提上：
流是快通道，可以丢；磁盘记录是权威副本。本工具回答三个问题：

1. 流里的助手文本与磁盘记录里的是否逐字一致。
2. 磁盘有而流里没有的：断线时要从磁盘补什么。
3. 流里有而磁盘没有的：哪些信息只能靠流拿，断线就丢了。

用法::

    <sdkvenv>/bin/python bridge/tools/cross_validate.py --case text-short
    <sdkvenv>/bin/python bridge/tools/cross_validate.py --all
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any

VECTORS = Path(__file__).resolve().parent.parent / "vectors"


def _find_jsonl(session_id: str) -> Path | None:
    """在 `$CLAUDE_CONFIG_DIR/projects/*/` 下找会话文件。

    注意：会话记录落在 `$CLAUDE_CONFIG_DIR` 下，不一定是 `~/.claude`。
    """
    root = Path(os.environ.get("CLAUDE_CONFIG_DIR") or (Path.home() / ".claude"))
    hits = list((root / "projects").glob(f"*/{session_id}.jsonl"))
    return hits[0] if hits else None


def _stream_text(case: str) -> tuple[str, list[str]]:
    """从 frames golden 取助手可见文本 + 收集流式独有的信息种类。

    取的是消息级 `at` 全文，不是增量拼接。两者逐字相等由
    `tests/test_encoder.py::test_delta_concat_equals_full_text` 单独钉住，那条不依赖外部文件。
    """
    text_parts: list[str] = []
    stream_only: list[str] = []
    for line in (VECTORS / f"{case}.frames.ndjson").read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        obj = json.loads(line)
        fr = obj.get("frame")
        if not fr:
            continue
        t = fr["t"]
        if t == "at":  # AssistantMessage 的 TextBlock，整块全文
            if fr.get("p"):
                continue  # 子 agent 输出，磁盘侧已按 isSidechain 排除，两侧保持同口径
            text_parts.append(fr.get("x", ""))
        elif t in ("d", "td", "ij"):
            stream_only.append(f"{t}(增量)")
        elif t == "rl":
            stream_only.append("rl(限流事件)")
        elif t == "ev" and fr.get("k", "").startswith(("stream:", "system:")):
            stream_only.append(fr["k"])
    return "".join(text_parts), sorted(set(stream_only))


def _jsonl_text(path: Path) -> tuple[str, list[str]]:
    """从 JSONL 拼出助手文本 + 收集磁盘独有的字段。"""
    text_parts: list[str] = []
    disk_only: set[str] = set()
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if not line.strip():
            continue
        try:
            rec = json.loads(line)
        except json.JSONDecodeError:
            continue
        # 磁盘上有而流里没有的顶层字段（血统/分支/元信息）
        for k in ("parentUuid", "logicalParentUuid", "forkedFrom", "isSidechain", "isMeta", "gitBranch"):
            if k in rec:
                disk_only.add(k)
        if rec.get("type") != "assistant":
            continue
        # 排除子 agent（Task 工具）的输出。流侧只取主流的 `at`，磁盘侧也要同口径，
        # 否则一有 Task 调用就会假红。
        if rec.get("isSidechain"):
            continue
        content = (rec.get("message") or {}).get("content")
        if isinstance(content, list):
            for blk in content:
                if isinstance(blk, dict) and blk.get("type") == "text":
                    text_parts.append(blk.get("text", ""))
    return "".join(text_parts), sorted(disk_only)


def validate(case: str) -> dict[str, Any]:
    meta = json.loads((VECTORS / f"{case}.ndjson").read_text(encoding="utf-8").splitlines()[0])
    sid = meta["__meta__"].get("session_id")
    if not sid:
        # 回退：`init` 帧里也带 sid。meta 里没有 session_id 的 golden 走这条，不必为对账重录。
        for line in (VECTORS / f"{case}.frames.ndjson").read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            fr = json.loads(line).get("frame")
            if fr and fr.get("t") == "init" and fr.get("sid"):
                sid = fr["sid"]
                break
    if not sid:
        return {"case": case, "skip": "golden 里既无 meta.session_id 也无 init.sid"}

    jsonl = _find_jsonl(sid)
    if jsonl is None:
        return {"case": case, "skip": f"找不到 {sid}.jsonl（会话可能已被清理）"}

    s_text, s_only = _stream_text(case)
    d_text, d_only = _jsonl_text(jsonl)

    return {
        "case": case,
        "sid": sid,
        "jsonl": str(jsonl),
        "stream_chars": len(s_text),
        "jsonl_chars": len(d_text),
        "text_identical": s_text == d_text,
        "stream_only": s_only,
        "disk_only": d_only,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--case")
    ap.add_argument("--all", action="store_true")
    args = ap.parse_args()

    cases = (
        sorted(p.stem for p in VECTORS.glob("*.frames.ndjson"))
        if args.all
        else [args.case]
    )
    cases = [c.replace(".frames", "") for c in cases]

    ok = True
    for c in cases:
        r = validate(c)
        if "skip" in r:
            print(f"⏭  {c}: {r['skip']}")
            continue
        mark = "✔" if r["text_identical"] else "✘"
        if not r["text_identical"]:
            ok = False
        print(
            f"{mark} {c}: 流式 {r['stream_chars']} 字符 / JSONL {r['jsonl_chars']} 字符"
            f" —— {'逐字一致' if r['text_identical'] else '★不一致'}"
        )
        print(f"    仅流式有: {', '.join(r['stream_only']) or '（无）'}")
        print(f"    仅磁盘有: {', '.join(r['disk_only']) or '（无）'}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
