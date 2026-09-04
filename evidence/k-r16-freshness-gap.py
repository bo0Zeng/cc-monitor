#!/usr/bin/env python3
"""K-R16 D2/D3 量具：把「灯读的那个值有多老」与「这个会话其实多久前还在写字」**并排量**。

问题形状：红绿灯读 `sessions/<PID>.json` 里的 `status`。CC **只在状态转换时**重写那个文件
（D2 实测），所以那个文件的年龄**与会话健不健康无关**。但同一个会话的 **jsonl** 是另一
回事——`busy` 期间它一直在长，而 monitor 本来就在盯 jsonl（tab 内容整条流水线靠它）。

⚠ 尺子怎么切的：
  - **分母**：`--dir`（默认 `~/.claude/sessions`）下所有 `<PID>.json`，**不区分账号**
    （本机 `~/.claude-accts/b|z/sessions` 都是这个目录的软链，`hn|hn-u` 是空的独立目录，
    量不到；跨账号的会话本脚本**看不见**）。
  - `pidfile_age` = now − mtime。`status_age` = now − `statusUpdatedAt`（CC 自己写的时刻，
    毫秒 epoch）。两者一般只差几毫秒（写文件与写字段同一拍），差很多说明有人碰过文件。
  - `jsonl_age` = now − 该 sid **最新那个** jsonl 的 mtime。找法：`~/.claude/projects/*/`
    下文件名以 sid 开头的 `.jsonl`，取 mtime 最大者。
    🔴 **找不到不等于没有**：`CLAUDE_CONFIG_DIR` 重定位过的会话、或 projects 目录换了地方，
    这里会报 `jsonl_age=None`，那是**尺子够不着**，不是「这个会话没写字」。
  - `alive` = `/proc/<pid>` 在不在。**它只答「进程还在」，不答「进程还在动」** ——
    卡死的进程这一格照样是 True，这正是本件治不了的那一半。

读法：`gap` = `pidfile_age − jsonl_age`。gap 越大，说明「灯看到的陈旧」越是假象——
会话其实一直在动，只是没换状态所以没人重写 pidfile。
"""

import argparse
import json
import os
import pathlib
import time


def newest_jsonl_mtime(projects: pathlib.Path, sid: str) -> float | None:
    best: float | None = None
    if not projects.exists():
        return None
    for p in projects.glob(f"*/{sid}*.jsonl"):
        try:
            m = p.stat().st_mtime
        except OSError:
            continue
        if best is None or m > best:
            best = m
    return best


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default=os.path.expanduser("~/.claude/sessions"))
    ap.add_argument("--projects", default=os.path.expanduser("~/.claude/projects"))
    ap.add_argument("--json", action="store_true", help="输出 JSON 而不是表")
    args = ap.parse_args()

    now = time.time()
    d = pathlib.Path(args.dir)
    projects = pathlib.Path(args.projects)
    rows = []
    for p in sorted(d.glob("*.json")):
        try:
            st = p.stat()
            obj = json.loads(p.read_text())
        except Exception:
            continue
        pid = obj.get("pid")
        sid = obj.get("sessionId") or ""
        sua = obj.get("statusUpdatedAt")
        jm = newest_jsonl_mtime(projects, sid)
        rows.append(
            {
                "file": p.name,
                "pid": pid,
                "sid": sid[:8],
                "status": obj.get("status"),
                "waitingFor": obj.get("waitingFor"),
                "alive": os.path.isdir(f"/proc/{pid}") if isinstance(pid, int) else None,
                "pidfile_age_s": round(now - st.st_mtime, 1),
                "status_age_s": round(now - sua / 1000.0, 1) if isinstance(sua, (int, float)) else None,
                "jsonl_age_s": round(now - jm, 1) if jm else None,
                "gap_s": round((now - st.st_mtime) - (now - jm), 1) if jm else None,
                "灯": {"idle": "红", "shell": "红", "waiting": "黄"}.get(obj.get("status") or "", "绿"),
            }
        )

    if args.json:
        print(json.dumps({"now": round(now, 3), "iso": time.strftime("%F %T"), "rows": rows}, ensure_ascii=False, indent=2))
        return 0

    print(f"# 量于 {time.strftime('%F %T')}  分母={len(rows)} 个 pidfile（只数 {d}）")
    print(f"{'pid':>8} {'sid':<9} {'status':<8} {'灯':<3} {'活':<4} {'pidfile龄':>10} {'status龄':>10} {'jsonl龄':>9} {'gap':>9}")
    for r in rows:
        print(
            f"{str(r['pid']):>8} {r['sid']:<9} {str(r['status']):<8} {r['灯']:<3} "
            f"{str(r['alive']):<4} {str(r['pidfile_age_s']):>10} {str(r['status_age_s']):>10} "
            f"{str(r['jsonl_age_s']):>9} {str(r['gap_s']):>9}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
