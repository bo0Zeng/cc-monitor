"""中转压测台架：本机假上游 ＋ 沙箱里的真后端 ＋ 一档档并发的流式请求，量后端进程。读数不进判据。

  python3 -I tests/benches/relay-load/rig.py run <后端二进制> <工作目录> [--levels 100,200,400] [--seconds 20] [--events 20] [--gap-ms 100]
             [--nofile 1024] [--nofile-hard <n>] [--min-avail-mb 4096] [--max-load <核数×3>] [--answer sse|429-bare]
  rig.py upstream <端口文件> <事件数> <间隔毫秒> <答法>     （run 自己起它，单独一个进程）

一个真请求都不发：上游是本机回环上的这个脚本自己。后端起在 <工作目录>/home 这个沙箱家目录里，环境从空起（只给 PATH · HOME ·
几个隔离用的目录），中转端口与默认上游由环境交；fd 软上限照常驻后端被桌面会话起来时那样给 --nofile（硬上限不动；要看 fd 用尽时的样子，--nofile-hard 把硬上限也压低，后端抬不上去）。
每档：起 N 个并发的「会话」，每个会话一条连接一发请求、读完整条流再开下一条，跑满 --seconds 秒。
量：首字节延迟（发完请求 → 回包第一个字节）· 首个事件延迟（→ 第一行 `data:`）· 整条完成 · 每秒完成条数 · 回包状态各几条 ·
后端进程（/proc/<pid>，每 100 ms 采一次）fd · 线程 · RSS 的峰值与 CPU 占用。机器余量不够（可用内存低于 --min-avail-mb、
一分钟负载高于 --max-load）⇒ 这一档不开、后面的也不开，照实印出来。先小档再大档、看着机器往上加。
起的两个进程按 PID 收；沙箱家目录（<工作目录>/home 等）现建、跑完删，工作目录里只留后端的 stderr。
"""

import asyncio
import json
import os
import resource
import signal
import socket
import subprocess
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))


# ── 假上游 ─────────────────────────────────────────────────────────────────────

QUOTA_HEADERS = (
    "anthropic-ratelimit-unified-status: allowed\r\n"
    "anthropic-ratelimit-unified-5h-status: allowed\r\n"
    "anthropic-ratelimit-unified-5h-utilization: 0.25\r\n"
    "anthropic-ratelimit-unified-5h-reset: 1800003600\r\n"
    "anthropic-ratelimit-unified-representative-claim: five_hour\r\n"
)


def sse_event(kind, data):
    return f"event: {kind}\ndata: {json.dumps(data, separators=(',', ':'))}\n\n".encode()


def chunk(b):
    return f"{len(b):x}\r\n".encode() + b + b"\r\n"


async def upstream_conn(reader, writer, events, gap, answer):
    try:
        head = await reader.readuntil(b"\r\n\r\n")
        clen = 0
        for line in head.split(b"\r\n"):
            k, _, v = line.partition(b":")
            if k.strip().lower() == b"content-length":
                clen = int(v.strip() or 0)
        if clen:
            await reader.readexactly(clen)
        if answer == "429-bare":
            body = b'{"type":"error","error":{"type":"rate_limit_error","message":"busy"}}'
            writer.write(
                b"HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\n"
                + f"content-length: {len(body)}\r\nconnection: close\r\n\r\n".encode()
                + body
            )
            await writer.drain()
            return
        writer.write(
            (
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\n"
                + QUOTA_HEADERS
                + "transfer-encoding: chunked\r\nconnection: close\r\n\r\n"
            ).encode()
        )
        writer.write(chunk(sse_event("message_start", {"type": "message_start", "message": {"id": "msg_rig", "usage": {"input_tokens": 10}}})))
        await writer.drain()
        for i in range(events):
            await asyncio.sleep(gap)
            delta = {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": f"字{i} " * 8}}
            writer.write(chunk(sse_event("content_block_delta", delta)))
            await writer.drain()
        writer.write(chunk(sse_event("message_stop", {"type": "message_stop"})))
        writer.write(b"0\r\n\r\n")
        await writer.drain()
    except (asyncio.IncompleteReadError, ConnectionError, asyncio.LimitOverrunError):
        pass
    finally:
        writer.close()


async def upstream_main(port_file, events, gap_ms, answer):
    server = await asyncio.start_server(
        lambda r, w: upstream_conn(r, w, events, gap_ms / 1000.0, answer),
        "127.0.0.1",
        0,
        backlog=4096,
    )
    port = server.sockets[0].getsockname()[1]
    with open(port_file + ".tmp", "w") as f:
        f.write(str(port))
    os.replace(port_file + ".tmp", port_file)
    async with server:
        await server.serve_forever()


# ── 后端进程的读数 ─────────────────────────────────────────────────────────────

TICK = os.sysconf("SC_CLK_TCK")


def proc_sample(pid):
    try:
        fds = len(os.listdir(f"/proc/{pid}/fd"))
        rss_kb = threads = 0
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                if line.startswith("VmRSS:"):
                    rss_kb = int(line.split()[1])
                elif line.startswith("Threads:"):
                    threads = int(line.split()[1])
        with open(f"/proc/{pid}/stat") as f:
            after = f.read().rsplit(")", 1)[1].split()
        cpu = (int(after[11]) + int(after[12])) / TICK
        return fds, threads, rss_kb, cpu
    except (FileNotFoundError, ProcessLookupError, IndexError, ValueError):
        return None


class Sampler(threading.Thread):
    def __init__(self, pid):
        super().__init__(daemon=True)
        self.pid, self.stop_at, self.peaks = pid, threading.Event(), [0, 0, 0]
        self.cpu0 = self.cpu1 = None

    def run(self):
        while not self.stop_at.is_set():
            s = proc_sample(self.pid)
            if s:
                for i in range(3):
                    self.peaks[i] = max(self.peaks[i], s[i])
                if self.cpu0 is None:
                    self.cpu0 = (time.monotonic(), s[3])
                self.cpu1 = (time.monotonic(), s[3])
            self.stop_at.wait(0.1)

    def cpu_pct(self):
        if not self.cpu0 or not self.cpu1 or self.cpu1[0] <= self.cpu0[0]:
            return 0.0
        return 100.0 * (self.cpu1[1] - self.cpu0[1]) / (self.cpu1[0] - self.cpu0[0])


# ── 打压 ──────────────────────────────────────────────────────────────────────


async def one_request(port, key, session, ttfb, tfe, total, statuses):
    body = b'{"model":"rig","stream":true,"max_tokens":16,"messages":[{"role":"user","content":"hi"}]}'
    req = (
        f"POST /{key}/t/claude-code/rig/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n"
        f"authorization: Bearer rig-not-a-credential\r\nanthropic-version: 2023-06-01\r\n"
        f"x-claude-code-session-id: rig-{session}\r\ncontent-type: application/json\r\n"
        f"content-length: {len(body)}\r\nconnection: close\r\n\r\n"
    ).encode() + body
    t0 = time.monotonic()
    try:
        reader, writer = await asyncio.open_connection("127.0.0.1", port)
    except OSError as e:
        statuses[f"connect:{e.errno}"] = statuses.get(f"connect:{e.errno}", 0) + 1
        return
    try:
        writer.write(req)
        await writer.drain()
        sent = time.monotonic()
        first = await reader.read(1)
        if not first:
            statuses["eof"] = statuses.get("eof", 0) + 1
            return
        ttfb.append(time.monotonic() - sent)
        buf = bytearray(first)
        seen_event = False
        while True:
            more = await reader.read(65536)
            if not more:
                break
            buf += more
            if not seen_event and b"data:" in buf:
                seen_event = True
                tfe.append(time.monotonic() - sent)
        status = buf.split(b" ", 2)[1].decode(errors="replace") if b" " in buf[:16] else "?"
        if status == "200" and b"message_stop" not in buf:
            status = "200-cut"
        statuses[status] = statuses.get(status, 0) + 1
        if status == "200":
            total.append(time.monotonic() - t0)
    except (ConnectionError, OSError) as e:
        statuses[f"io:{getattr(e, 'errno', '?')}"] = statuses.get(f"io:{getattr(e, 'errno', '?')}", 0) + 1
    finally:
        writer.close()


async def level(port, key, n, seconds):
    ttfb, tfe, total, statuses = [], [], [], {}
    until = time.monotonic() + seconds

    async def session(i):
        while time.monotonic() < until:
            before = len(total)
            await one_request(port, key, i, ttfb, tfe, total, statuses)
            if len(total) == before:
                # 没拿到一条完整的 200（503 · 连接失败 · 断了）：退 0.2 s 再来（拿到 503 的客户端本来就退避）。
                await asyncio.sleep(0.2)

    t0 = time.monotonic()
    await asyncio.gather(*(session(i) for i in range(n)))
    return ttfb, tfe, total, statuses, time.monotonic() - t0


def pct(v, p):
    if not v:
        return float("nan")
    v = sorted(v)
    return v[min(len(v) - 1, int(p / 100.0 * len(v)))]


def room(min_avail_mb, max_load):
    avail = 0
    with open("/proc/meminfo") as f:
        for line in f:
            if line.startswith("MemAvailable:"):
                avail = int(line.split()[1]) // 1024
    load = os.getloadavg()[0]
    return avail >= min_avail_mb and load <= max_load, avail, load


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def wait_for(path, what, proc, limit=60.0):
    t = time.monotonic() + limit
    while time.monotonic() < t:
        if os.path.exists(path):
            return
        if proc.poll() is not None:
            sys.exit(f"{what}没起来（退出码 {proc.returncode}）")
        time.sleep(0.05)
    sys.exit(f"{what} {limit:.0f}s 内没起来")


def run(argv):
    import argparse

    ap = argparse.ArgumentParser(prog="rig.py run")
    ap.add_argument("backend")
    ap.add_argument("work")
    ap.add_argument("--levels", default="100,200,400")
    ap.add_argument("--seconds", type=float, default=20)
    ap.add_argument("--events", type=int, default=20)
    ap.add_argument("--gap-ms", type=int, default=100)
    ap.add_argument("--nofile", type=int, default=1024)
    ap.add_argument("--nofile-hard", type=int, default=0)
    ap.add_argument("--min-avail-mb", type=int, default=4096)
    ap.add_argument("--max-load", type=float, default=3.0 * (os.cpu_count() or 1))
    ap.add_argument("--answer", choices=["sse", "429-bare"], default="sse")
    a = ap.parse_args(argv)

    work = os.path.abspath(a.work)
    home = os.path.join(work, "home")
    sandbox = [os.path.join(work, d) for d in ("home", "tmux", "run")]
    if any(os.path.exists(d) for d in sandbox):
        sys.exit(f"{work} 里已经有沙箱目录（上一趟没收干净？）—— 换一个工作目录")
    for d in sandbox:
        os.makedirs(d)
    os.chmod(os.path.join(work, "run"), 0o700)
    # 打压那一侧自己也要几百个 fd（每条会话一条连接）：抬到自己的硬上限。
    soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
    resource.setrlimit(resource.RLIMIT_NOFILE, (hard, hard))

    procs = []

    def reap(*_):
        for p in procs:
            if p.poll() is None:
                p.send_signal(signal.SIGTERM)
        for p in procs:
            try:
                p.wait(timeout=10)
            except subprocess.TimeoutExpired:
                p.kill()
                p.wait()

    signal.signal(signal.SIGTERM, lambda *_: (reap(), sys.exit(143)))
    try:
        port_file = os.path.join(work, "upstream.port")
        if os.path.exists(port_file):
            os.remove(port_file)
        up = subprocess.Popen(
            [sys.executable, "-I", os.path.abspath(__file__), "upstream", port_file, str(a.events), str(a.gap_ms), a.answer],
            stdin=subprocess.DEVNULL,
        )
        procs.append(up)
        wait_for(port_file, "假上游", up)
        with open(port_file) as f:
            up_port = int(f.read())

        relay_port = free_port()
        env = {
            "PATH": "/usr/bin:/bin",
            "HOME": home,
            "USERPROFILE": home,
            "LANG": "C.UTF-8",
            "TZ": "UTC",
            "TMUX_TMPDIR": os.path.join(work, "tmux"),
            "XDG_RUNTIME_DIR": os.path.join(work, "run"),
            "CLAUDE_CONFIG_DIR": os.path.join(home, ".claude"),
            # 中转那几行 `[relay]` 在 warn 档下印不出来（「在听」那一句台架要等它）。
            "RUST_LOG": "info",
            "CCM_RELAY_PORT": str(relay_port),
            "CCM_AGENT_UPSTREAM_CLAUDE_CODE": f"http://127.0.0.1:{up_port}",
        }
        nofile, nofile_hard = a.nofile, a.nofile_hard

        def limit_fds():
            _, h = resource.getrlimit(resource.RLIMIT_NOFILE)
            if nofile_hard:
                h = min(h, nofile_hard)
            resource.setrlimit(resource.RLIMIT_NOFILE, (min(nofile, h), h))

        err_path = os.path.join(work, "backend.stderr")
        err = open(err_path, "wb")
        be = subprocess.Popen(
            [a.backend, "--", "--stream"],
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.DEVNULL,
            stderr=err,
            preexec_fn=limit_fds,
        )
        procs.append(be)
        key_path = os.path.join(home, ".cc-monitor", "relay-key")
        t = time.monotonic() + 60
        while b"[relay] listening on" not in open(err_path, "rb").read():
            if be.poll() is not None or time.monotonic() > t:
                why = f"退出码 {be.returncode}" if be.poll() is not None else "60 s 内没说在听"
                sys.exit(f"后端的中转没起来（{why}）：\n" + open(err_path, errors="replace").read()[-2000:])
            time.sleep(0.05)
        # 沙箱里后端现铸的那一把（不是任何号的凭据）：中转门上认它。
        with open(key_path) as f:
            key = f.read().strip()
        with open(f"/proc/{be.pid}/limits") as f:
            lim = next(line for line in f if line.startswith("Max open files")).split()
        print(f"后端 pid {be.pid} · fd 上限 软 {lim[3]} 硬 {lim[4]} · 起来时 {proc_sample(be.pid)[:3]}（fd · 线程 · RSS kB）")
        print(f"答法 {a.answer} · 每条 {a.events} 个事件 × {a.gap_ms} ms · 每档 {a.seconds:.0f} s")
        hdr = "并发  完成/s  首字节p50  p99   首事件p50  p99   整条p50  p99    回包                         fd峰 线程峰 RSS峰MB CPU%"
        print(hdr)
        for n in [int(x) for x in a.levels.split(",") if x.strip()]:
            ok, avail, load = room(a.min_avail_mb, a.max_load)
            if not ok:
                print(f"{n:>4}  不开：可用内存 {avail} MB · 一分钟负载 {load:.1f}（门槛 {a.min_avail_mb} MB · {a.max_load:.0f}）")
                break
            s = Sampler(be.pid)
            s.start()
            ttfb, tfe, total, statuses, took = asyncio.run(level(relay_port, key, n, a.seconds))
            s.stop_at.set()
            s.join()
            done = sum(1 for _ in total)
            st = " ".join(f"{k}:{v}" for k, v in sorted(statuses.items()))
            ms = lambda v, p: pct(v, p) * 1000  # noqa: E731
            print(
                f"{n:>4}  {done / took:>6.1f}  {ms(ttfb, 50):>8.1f} {ms(ttfb, 99):>6.1f}  {ms(tfe, 50):>8.1f} {ms(tfe, 99):>6.1f}"
                f"  {ms(total, 50):>7.0f} {ms(total, 99):>6.0f}   {st:<28} {s.peaks[0]:>4} {s.peaks[1]:>5} {s.peaks[2] / 1024:>7.1f} {s.cpu_pct():>5.1f}"
            )
            if be.poll() is not None:
                print(f"后端退出了（{be.returncode}）")
                break
            time.sleep(2)
        tail = [line for line in open(err_path, errors="replace").read().splitlines() if "[relay]" in line or "WARN" in line or "ERROR" in line]
        if tail:
            print(f"后端 stderr 里中转 · WARN · ERROR 的行 {len(tail)} 条，按出现次数（数字抹成 N）：")
            import re
            from collections import Counter

            for line, c in Counter(re.sub(r"\d+", "N", x)[:160] for x in tail).most_common(8):
                print(f"  {c:>6} × {line}")
    finally:
        reap()
        import shutil

        for d in sandbox + [os.path.join(work, "upstream.port")]:
            if os.path.isdir(d):
                shutil.rmtree(d, ignore_errors=True)
            elif os.path.exists(d):
                os.remove(d)


def main():
    if len(sys.argv) >= 2 and sys.argv[1] == "upstream":
        port_file, events, gap_ms, answer = sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), sys.argv[5]
        resource.setrlimit(resource.RLIMIT_NOFILE, (resource.getrlimit(resource.RLIMIT_NOFILE)[1],) * 2)
        signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
        asyncio.run(upstream_main(port_file, events, gap_ms, answer))
    elif len(sys.argv) >= 2 and sys.argv[1] == "run":
        run(sys.argv[2:])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
