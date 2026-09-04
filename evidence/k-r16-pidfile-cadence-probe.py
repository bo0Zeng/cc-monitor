#!/usr/bin/env python3
"""K-R16 D2 量具：观测 Claude Code 往 `sessions/<PID>.json` 写入的**节奏**。

尺子怎么切的（报数前先说清）：
  - 采样对象：`$SESSIONS_DIR`（默认 `~/.claude/sessions`）下所有 `<PID>.json`。
  - 采样频率：每 `--interval` 秒 stat 一遍全目录（默认 0.25s）。**轮询不是 inotify**
    —— 这台机器没装 inotifywait，且产品侧那条链用的是 notify crate 的 80ms debounce，
    轮询 0.25s 的分辨率对「秒级/分钟级」这个量纲够用，但**测不到 250ms 内的连写**。
    连写会被合并成一行，所以「写入次数」是**下界**。
  - 判「写了一次」的判据：`st_mtime_ns` 变了 **或** 文件字节变了。两者取并集，
    因为 CC 可能原子替换（inode 换、mtime 跳）也可能原地覆写同长度内容。
  - 每次变化落一行 JSONL，带上 `status` / `updatedAt` / `statusUpdatedAt` / `waitingFor`
    以及**距上一次写的间隔**，用来分「心跳」与「只在状态变化时写」。
  - 同时记 `/proc/<pid>` 在不在（进程活否）——这是产品侧心跳那一路用的同一个判据。

输出：`--out` 指定的 JSONL。每行一个事件；`kind` 取值：
  `seen`（首次看到该 pidfile）/ `write`（内容或 mtime 变了）/ `vanish`（文件没了）/
  `proc_gone`（文件还在但进程没了）/ `tick`（周期性存活打点，便于确认量具自己没死）。
"""

import argparse
import hashlib
import json
import os
import pathlib
import sys
import time


def read_state(p: pathlib.Path):
    try:
        st = p.stat()
        data = p.read_bytes()
    except OSError:
        return None
    try:
        obj = json.loads(data)
    except Exception:
        obj = {}
    return {
        "mtime_ns": st.st_mtime_ns,
        "size": st.st_size,
        "ino": st.st_ino,
        "sha": hashlib.sha256(data).hexdigest()[:12],
        "status": obj.get("status"),
        "waitingFor": obj.get("waitingFor"),
        "updatedAt": obj.get("updatedAt"),
        "statusUpdatedAt": obj.get("statusUpdatedAt"),
        "sessionId": obj.get("sessionId"),
        "pid": obj.get("pid"),
    }


def proc_alive(pid) -> bool:
    try:
        return os.path.isdir(f"/proc/{int(pid)}")
    except Exception:
        return False


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default=os.path.expanduser("~/.claude/sessions"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--interval", type=float, default=0.25)
    ap.add_argument("--seconds", type=float, default=3600.0)
    ap.add_argument("--tick", type=float, default=60.0)
    args = ap.parse_args()

    d = pathlib.Path(args.dir)
    out = open(args.out, "a", buffering=1)

    def emit(kind, name, **kw):
        row = {"t": round(time.time(), 3), "iso": time.strftime("%F %T"), "kind": kind, "file": name}
        row.update(kw)
        out.write(json.dumps(row, ensure_ascii=False) + "\n")

    prev: dict[str, dict] = {}
    last_write_t: dict[str, float] = {}
    started = time.time()
    last_tick = 0.0
    emit("start", "", dir=str(d), interval=args.interval, seconds=args.seconds)

    while time.time() - started < args.seconds:
        now = time.time()
        try:
            files = sorted(x.name for x in d.glob("*.json"))
        except OSError:
            files = []
        for name in files:
            cur = read_state(d / name)
            if cur is None:
                continue
            old = prev.get(name)
            if old is None:
                emit("seen", name, **cur, proc_alive=proc_alive(cur.get("pid")))
                prev[name] = cur
                last_write_t[name] = now
                continue
            if cur["mtime_ns"] != old["mtime_ns"] or cur["sha"] != old["sha"]:
                gap = round(now - last_write_t.get(name, now), 3)
                changed = [
                    k
                    for k in ("status", "waitingFor", "updatedAt", "statusUpdatedAt", "sessionId")
                    if cur.get(k) != old.get(k)
                ]
                emit(
                    "write",
                    name,
                    gap_s=gap,
                    changed=changed,
                    prev_status=old.get("status"),
                    **cur,
                    proc_alive=proc_alive(cur.get("pid")),
                )
                prev[name] = cur
                last_write_t[name] = now
            elif not proc_alive(cur.get("pid")):
                if not old.get("_proc_gone"):
                    emit("proc_gone", name, **cur, stale_s=round(now - last_write_t.get(name, now), 3))
                    cur["_proc_gone"] = True
                    prev[name] = cur
        for name in list(prev):
            if name not in files:
                emit("vanish", name, stale_s=round(now - last_write_t.get(name, now), 3))
                prev.pop(name, None)
        if now - last_tick >= args.tick:
            last_tick = now
            emit(
                "tick",
                "",
                tracked=len(prev),
                ages={
                    n: round(now - last_write_t.get(n, now), 1) for n in sorted(prev)
                },
            )
        time.sleep(args.interval)
    emit("end", "", tracked=len(prev))
    return 0


if __name__ == "__main__":
    sys.exit(main())
