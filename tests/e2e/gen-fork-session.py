#!/usr/bin/env python3
"""Batch13-F40c:生成含 ESC 分叉的合成会话 jsonl(fold E2E fixture)。

形态:u1→a1 后分叉——u2a/a2a(被弃分支,off-main,应折叠)与 u2b/a2b(胜者)。
每次生成用全新 session/记录 uuid(防 processedUuids 去重吃掉重复注入)。
用法:gen-fork-session.py <输出目录> [sid]  → 打印 session id。
sid 可由调用方预生成(套件需要先写宿主 pidfile 再落 jsonl——watcher 的
process_file 可能抢在 pidfile 之前跑,判非活跃后整文件静默跳过)。
"""
import json
import sys
import uuid
from datetime import datetime, timedelta, timezone
from pathlib import Path

out_dir = Path(sys.argv[1])
out_dir.mkdir(parents=True, exist_ok=True)
sid = sys.argv[2] if len(sys.argv) > 2 else str(uuid.uuid4())
t0 = datetime.now(timezone.utc)
cwd = "/tmp/e2e-fork"


def rec(i, typ, u, parent, text):
    return {
        "type": typ,
        "uuid": u,
        "parentUuid": parent,
        "timestamp": (t0 + timedelta(seconds=i)).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z",
        "sessionId": sid,
        "cwd": cwd,
        "message": {
            "role": "user" if typ == "user" else "assistant",
            "content": text if typ == "user" else [{"type": "text", "text": text}],
        },
    }


u1, a1, u2a, a2a, u2b, a2b = (str(uuid.uuid4()) for _ in range(6))
records = [
    rec(0, "user", u1, None, "e2e-fork:第一问"),
    rec(1, "assistant", a1, u1, "第一答。"),
    rec(2, "user", u2a, a1, "e2e-fork:被 ESC 回退的追问"),
    rec(3, "assistant", a2a, u2a, "被回退分支的回答(应折叠)。"),
    rec(4, "user", u2b, a1, "e2e-fork:重写后的追问(主线)"),
    rec(5, "assistant", a2b, u2b, "主线回答。"),
]
path = out_dir / f"{sid}.jsonl"
with path.open("w", encoding="utf-8") as f:
    for r in records:
        f.write(json.dumps(r, ensure_ascii=False) + "\n")
print(sid)
