#!/usr/bin/env python3
# ruff: noqa: E501
"""TAP 台架：真后端（进程内中转）× 假上游 × **真 claude CLI** —— 产出 T7 的夹具，并量「jsonl 何时落盘」。

住址：`<仓根>/tests/evidence/TAP-bench.py`
夹具：`<仓根>/tests/__fixtures__/tap-bench.json`（由 `tests/live-card.vitest.ts` 的 T7 读）
服务的要求：`设计/20 §8`（V24「SSE 确保快，落盘确保对」；对账键 `message.id`）· 设计与读数住仓外 `调研/第四波记录/TAP.md §7 T7 · §8 题 3 · §9`。

跑法（仓根下；要先 `cargo build`（`src/backend`）出后端二进制，要本机有 `claude`）：
    python3 tests/evidence/TAP-bench.py            # 跑一轮，写夹具，印读数
    python3 tests/evidence/TAP-bench.py --check    # 跑一轮，只比对不写夹具（夹具与这一轮的 T7 形状相同即可，不要求逐字）

为什么这样搭（异源在哪）：
- **jsonl 那一侧是 claude 自己写的**（真 `claude -p`，隔离 `CLAUDE_CONFIG_DIR`、假 key）—— 不是我们照着 SSE 造的，
  所以「SSE 拼出的正文 == jsonl 落盘的正文」不是两侧同源恒真。
- **tap 那一侧是生产接线**：后端以流模式（stdio 载体）起、交 `CCM_RELAY_PORT` ⇒ 进程内中转（`relay::host`）→ tap 口 →
  `writer_task` → stdout 上的 `tap` 帧。本脚本只从 stdout 收帧，不碰后端内部。
- **正文是假上游合成的**（固定的几段中文 ＋ 编号），不含任何真会话正文（`test-fixtures-no-real-transcript`）。夹具里的 jsonl
  只留 `type` · `apiBlockIndex` · `message.{id,content}`（去掉 cwd / 版本 / 时间戳这些机器相关的格）。
- 〔V141〕流标签 = claude 请求头 `x-claude-code-session-id`（== `--session-id` 给它的那个 UUID）⇒ `stream == sid`；地址里没有会话段。

同一趟还量一件事（`TAP.md §8` 题 3）：假上游在**思考块收尾之后、正文开始之前**停一拍，看那一刻 claude 有没有已经把
这一轮（同 `message.id`）的第一条记录写进 jsonl。写了 ⇒「同 id 第一条记录就整张覆盖」会在正文开始流之前撤掉活卡。
"""
from __future__ import annotations

import argparse
import http.server
import json
import os
import pathlib
import shutil
import socket
import subprocess
import sys
import threading
import time
import uuid

REPO = pathlib.Path(__file__).resolve().parents[2]
BACKEND = REPO / ".build" / "backend" / "debug" / "cc-monitor-backend"
FIXTURE = REPO / "tests" / "__fixtures__" / "tap-bench.json"
WORK = REPO / ".scratch" / "tap-bench"

TEXT_PARTS = ["台架", "合成的", "一段", "正文，", "第 {n} 轮。"]
PAUSE_AFTER_THINKING_S = 1.5


def sse(ev: dict) -> bytes:
    return f"event: {ev['type']}\ndata: {json.dumps(ev, ensure_ascii=False)}\n\n".encode()


class Upstream:
    """假上游：`HEAD /api/hello` 回 200；`POST /v1/messages…` 流式回一轮（思考块 ＋ 停一拍 ＋ 正文块）。"""

    def __init__(self, jsonl_glob: pathlib.Path) -> None:
        self.jsonl_root = jsonl_glob
        self.rounds = 0
        self.readings: list[dict] = []
        self.lock = threading.Lock()

    def landed(self, mid: str) -> bool:
        for p in self.jsonl_root.rglob("*.jsonl"):
            try:
                if mid in p.read_text(errors="replace"):
                    return True
            except OSError:
                pass
        return False

    def handler(self):
        up = self

        class H(http.server.BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *a):  # noqa: D401 - 安静
                pass

            def do_HEAD(self):
                self.send_response(200)
                self.send_header("Content-Length", "0")
                self.end_headers()

            def do_GET(self):
                self.do_HEAD()

            def do_POST(self):
                n = int(self.headers.get("Content-Length") or 0)
                body = json.loads(self.rfile.read(n) or b"{}")
                with up.lock:
                    up.rounds += 1
                    k = up.rounds
                mid = f"msg_tapbench_{k:02d}"
                text = "".join(TEXT_PARTS).replace("{n}", str(k))
                if not body.get("stream"):
                    out = json.dumps({
                        "id": mid, "type": "message", "role": "assistant", "model": body.get("model", "m"),
                        "content": [{"type": "text", "text": text}], "stop_reason": "end_turn", "stop_sequence": None,
                        "usage": {"input_tokens": 1, "output_tokens": 1},
                    }).encode()
                    self.send_response(200)
                    self.send_header("Content-Type", "application/json")
                    self.send_header("Content-Length", str(len(out)))
                    self.end_headers()
                    self.wfile.write(out)
                    return
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Connection", "close")
                self.end_headers()
                w = self.wfile
                model = body.get("model", "m")
                w.write(sse({"type": "message_start", "message": {
                    "id": mid, "type": "message", "role": "assistant", "model": model, "content": [],
                    "stop_reason": None, "stop_sequence": None, "usage": {"input_tokens": 1, "output_tokens": 1}}}))
                w.write(sse({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": "", "signature": ""}}))
                w.write(sse({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": f"台架思考 {k}"}}))
                w.write(sse({"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": "c2ln"}}))
                w.write(sse({"type": "content_block_stop", "index": 0}))
                w.flush()
                time.sleep(PAUSE_AFTER_THINKING_S)
                landed_early = up.landed(mid)
                w.write(sse({"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": ""}}))
                for part in TEXT_PARTS:
                    w.write(sse({"type": "ping"}))
                    w.write(sse({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": part.replace("{n}", str(k))}}))
                    w.flush()
                    time.sleep(0.05)
                w.write(sse({"type": "content_block_stop", "index": 1}))
                w.write(sse({"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": None}, "usage": {"output_tokens": 9}}))
                w.write(sse({"type": "message_stop"}))
                w.flush()
                # 正控：同一把尺子在 message_stop 之后再量一次 —— 量不到 ⇒ 上面那个 false 是空的（尺子瞎了）。
                landed_after = False
                for _ in range(40):
                    time.sleep(0.1)
                    if up.landed(mid):
                        landed_after = True
                        break
                with up.lock:
                    up.readings.append({"message_id": mid, "first_record_landed_before_text_started": landed_early,
                                        "landed_within_4s_after_message_stop": landed_after})
                self.close_connection = True

        return H


def run_interactive(claude, sid, cenv, cwd, cfg, _key_tail, up):
    """交互形：私有 tmux 服务器（`-L`，用完 kill-server）里起 claude，打一句话、回车，等假上游答完一轮再退。"""
    api_tail = cenv["ANTHROPIC_API_KEY"][-20:]
    (cfg / ".claude.json").write_text(json.dumps({
        "hasCompletedOnboarding": True,
        "theme": "dark",
        "customApiKeyResponses": {"approved": [api_tail], "rejected": []},
        "projects": {str(cwd): {"hasTrustDialogAccepted": True, "hasCompletedProjectOnboarding": True}},
    }))
    sock = f"tapbench-{os.getpid()}"
    envs = []
    for k, v in cenv.items():
        envs += ["-e", f"{k}={v}"]
    tm = ["tmux", "-L", sock]
    try:
        subprocess.run(tm + ["new-session", "-d", "-s", "t", "-x", "160", "-y", "50", "-c", str(cwd), *envs,
                             claude, "--session-id", sid], check=True)
        time.sleep(6)
        subprocess.run(tm + ["send-keys", "-t", "t", "说一句话"], check=True)
        time.sleep(0.5)
        subprocess.run(tm + ["send-keys", "-t", "t", "Enter"], check=True)
        deadline = time.time() + 40
        while time.time() < deadline and not up.readings:
            time.sleep(0.2)
        time.sleep(3)
        pane = subprocess.run(tm + ["capture-pane", "-p", "-t", "t"], capture_output=True, text=True).stdout
    finally:
        subprocess.run(tm + ["kill-server"], capture_output=True)

    class R:
        returncode = 0 if up.readings else 1
        stderr = pane[-600:] if "pane" in locals() else ""
    return R()


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--interactive", action="store_true",
                    help="只量不写夹具：claude 以交互式跑在一个私有 tmux 服务器里（量 §8 题 3 在交互形下的读数）")
    args = ap.parse_args()
    if not BACKEND.exists():
        print(f"没有后端二进制：{BACKEND}（先在 src/backend 下 cargo build）", file=sys.stderr)
        return 2
    claude = shutil.which("claude")
    if not claude:
        print("本机没有 claude CLI —— 本台架要真 claude 写 jsonl（异源），没有就不跑", file=sys.stderr)
        return 2

    if WORK.exists():
        shutil.rmtree(WORK)
    home = WORK / "home"
    cfg = home / ".claude"
    cwd = WORK / "proj"
    for d in (home / ".cc-monitor", cfg, cwd):
        d.mkdir(parents=True)
    creds = WORK / "apikey-credentials.json"
    creds.write_text('{\n  "accounts": {\n    "acctA": {}\n  }\n}\n')

    up = Upstream(cfg / "projects")
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), up.handler())
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    up_port = srv.server_address[1]

    relay_port = free_port()
    env = {
        "PATH": os.environ.get("PATH", ""),
        "HOME": str(home),
        "CLAUDE_CONFIG_DIR": str(cfg),
        "CCM_RELAY_PORT": str(relay_port),
        "CCM_AGENT_UPSTREAM_CLAUDE_CODE": f"http://127.0.0.1:{up_port}",
        "CCM_APIKEY_CREDENTIALS": str(creds),
    }
    be = subprocess.Popen([str(BACKEND), "--", "--tail-only"], env=env, stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    frames: list[dict] = []
    listening = threading.Event()

    def read_out():
        for ln in be.stdout:
            try:
                f = json.loads(ln)
            except json.JSONDecodeError:
                continue
            if f.get("kind") == "tap":
                frames.append(f)

    def read_err():
        for ln in be.stderr:
            if "[relay] listening on" in ln or "中转住本进程" in ln:
                listening.set()

    threading.Thread(target=read_out, daemon=True).start()
    threading.Thread(target=read_err, daemon=True).start()
    try:
        if not listening.wait(30):
            print("后端 30 s 内没说「中转在听」", file=sys.stderr)
            return 1
        key_file = home / ".cc-monitor" / "relay-key"
        for _ in range(100):
            if key_file.exists():
                break
            time.sleep(0.05)
        key = key_file.read_text().strip()
        sid = str(uuid.uuid4())
        base = f"http://127.0.0.1:{relay_port}/{key}/s/claude-code/acctA"
        cenv = {
            "PATH": os.environ.get("PATH", ""),
            "HOME": str(home),
            "CLAUDE_CONFIG_DIR": str(cfg),
            "ANTHROPIC_API_KEY": "sk-ant-tapbench-not-a-real-key",
            "ANTHROPIC_BASE_URL": base,
            "DISABLE_TELEMETRY": "1",
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
            "DISABLE_AUTOUPDATER": "1",
        }
        if args.interactive:
            r = run_interactive(claude, sid, cenv, cwd, cfg, key[-20:], up)
        else:
            r = subprocess.run([claude, "-p", "说一句话", "--session-id", sid], env=cenv, cwd=cwd,
                               capture_output=True, text=True, timeout=120)
        # tap 帧经写者最低优先出来 ⇒ 给它一点时间排完（claude 退出之前响应早已收尾）。
        deadline = time.time() + 5
        while time.time() < deadline and not any(f.get("end") for f in frames if f.get("stream") == sid):
            time.sleep(0.05)
        time.sleep(0.3)
    finally:
        be.terminate()
        try:
            be.wait(5)
        except subprocess.TimeoutExpired:
            be.kill()
        srv.shutdown()

    jsonls = list((cfg / "projects").rglob(f"{sid}.jsonl"))
    if r.returncode != 0 or not jsonls:
        print(f"claude 退出码 {r.returncode}，jsonl {len(jsonls)} 份；stderr 末尾：{r.stderr[-400:]}", file=sys.stderr)
        return 1
    recs = [json.loads(x) for x in jsonls[0].read_text().splitlines() if x.strip()]
    slim = []
    for rec in recs:
        if rec.get("type") != "assistant":
            slim.append({"type": rec.get("type")})
            continue
        m = rec.get("message") or {}
        slim.append({"type": "assistant", "apiBlockIndex": rec.get("apiBlockIndex"),
                     "message": {"id": m.get("id"), "content": m.get("content")}})
    taps = [{"origin": "<local>", **{k: f[k] for k in ("stream", "resp", "n", "data", "end") if k in f}}
            for f in frames if f.get("stream") == sid]
    # 响应号按本趟重编（进程级计数器的起点不进夹具）。
    first = {}
    for t in taps:
        first.setdefault(t["resp"], len(first))
    for t in taps:
        t["resp"] = first[t["resp"]]
    fixture = {"sid": sid, "taps": taps, "jsonl": slim,
               "readings": {"rounds": up.rounds, "per_round": up.readings,
                            "tap_frames": len(taps), "jsonl_records": len(slim)}}
    print(json.dumps(fixture["readings"], ensure_ascii=False, indent=2))
    if args.interactive:
        return 0
    if args.check:
        return 0 if taps and any(s.get("type") == "assistant" for s in slim) else 1
    FIXTURE.write_text(json.dumps(fixture, ensure_ascii=False, indent=1) + "\n")
    print(f"夹具已写：{FIXTURE.relative_to(REPO)}")
    shutil.rmtree(WORK, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
