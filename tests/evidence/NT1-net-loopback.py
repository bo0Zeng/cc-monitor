#!/usr/bin/env python3
# ruff: noqa: E501
"""NT1：**SSH 按需多开 · 压缩 · sftp 通道开销** —— 对着真 sshd 的现打（本机回环，零 root）。

跑法（仓根下，先 `cd src/backend && cargo build`）：
    python3 tests/evidence/NT1-net-loopback.py [--readings-only] [--only 1,2,…] [后端二进制路径]

`--readings-only`：只印读数、不判（给**旧**二进制跑 A/B 基线用：旧的不该多开，判了必红）。

它起两台**临时的回环 sshd**（本用户身份、随机端口、临时 host key 与客户端钥匙、`UsePAM no`、`LogLevel DEBUG1`，
sftp 子系统起始目录钉在临时目录 —— 同 `SR1b-sftp-loopback.py`）：甲台 `MaxSessions 10`（默认值），乙台 `MaxSessions 2`
（「远端把会话数调低了」那一形）。再起一个**流模式**的后端（stdio 载体；私有 HOME / TMUX_TMPDIR）。

🔴 **弱网是用户态整形代理模拟的，不是 `tc netem`**（题面：要 root 的不许用）：甲台前面挡一个 Python 代理，
两个方向各一条**共享的瓶颈**（所有经它的 TCP 共用一个 FIFO 队列，队列上限 `QCAP`、按 `BPS` 出队、再延迟 `DELAY` 交付）——
与真路由器的「一条瓶颈、一个队列」同形：同一条 TCP 里的交互字节排在批量字节后面（发送方 socket 缓冲里的队头阻塞），
分开的 TCP 只排瓶颈那一个短队列。买不到：真丢包 · 真乱序 · 拥塞控制对真 RTT 的反应（代理本机那一跳的 RTT 是 0，
内核看到的 `tcpi_rtt` 也是回环的 —— 所以**压缩的判准在这台代理上读不出「远」**，那一格的真读数在 Rust 那条 `#[ignore]` 读数里）。

覆盖：
  ① 分道：长流在时起一趟下载 ⇒ 传输期间 sshd 鉴权 **恰好 2**、TCP **恰好 2**；下载完 TCP 仍 **恰好 2**（批量连接被主连接托着）；
     长流关掉 ⇒ **恰好 0**（主连接没了，托着的那条随之断 —— 按事件收，没有定时器）
  ② 预算满：长流 ＋ 8 条重叠的 capture（每条连接的通道闸 8 格）⇒ 全部跑通、鉴权 **恰好 2**
  ③ 远端 MaxSessions=2（乙台）：长流 ＋ 3 条重叠的 capture ⇒ 全部跑通（被拒的那一条挪到新连接上）、鉴权 **恰好 2**；sshd 的拒绝原话
  ④ 压缩（回环）：sshd 日志里我们这几条连接的协商结果全是 `compression: none`（回环不开，判准只有 `connect.rs::compression_for`）
  ⑤ 弱网读数（甲台经整形代理，DELAY 单程 · BPS 每方向）：下载 16 MiB 期间长流上的回声延迟 · 顺序小文件下载每件耗时 ·
     首条 / 复用 capture 耗时（握手成本）—— 只印、不判（墙钟读数）；顺序 6 趟小下载的 sftp 子系统请求次数 **恰好 1**（判，计数）

退出码：0 = 全过 · 1 = 有一条不对 · 3 = 起不来 sshd / 找不到二进制（环境不满足，不是被测对象坏了）
"""

import base64
import collections
import json
import os
import shutil
import socket
import statistics
import subprocess
import sys
import tempfile
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEFAULT_BIN = os.path.join(ROOT, ".build", "backend", "debug", "cc-monitor-backend")

DELAY = 0.040  # 单程延迟（秒）⇒ RTT 80 ms
BPS = 2 * 1024 * 1024  # 每方向瓶颈（字节/秒）
QCAP = 64 * 1024  # 瓶颈队列上限（字节）


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def established(port):
    out = subprocess.run(["ss", "-Htn", "state", "established", f"( dport = :{port} )"], capture_output=True, text=True).stdout
    return len([ln for ln in out.splitlines() if ln.strip()])


def sleep_until(t):
    d = t - time.monotonic()
    if d > 0:
        time.sleep(d)


class Bottleneck:
    """一个方向上的共享瓶颈：FIFO 队列（上限 QCAP 字节）→ 按 BPS 出队 → 延迟 DELAY 交付。"""

    def __init__(self, bps, delay, qcap):
        self.bps, self.delay, self.qcap = bps, delay, qcap
        self.q = collections.deque()
        self.qbytes = 0
        self.cv = threading.Condition()
        self.out = collections.deque()
        self.ocv = threading.Condition()
        threading.Thread(target=self._drain, daemon=True).start()
        threading.Thread(target=self._deliver, daemon=True).start()

    def put(self, sock, data):
        with self.cv:
            while self.qbytes > 0 and self.qbytes + len(data or b"") > self.qcap:
                self.cv.wait()
            self.q.append((sock, data))
            self.qbytes += len(data or b"")
            self.cv.notify_all()

    def _drain(self):
        t = time.monotonic()
        while True:
            with self.cv:
                while not self.q:
                    self.cv.wait()
                sock, data = self.q.popleft()
                self.qbytes -= len(data or b"")
                self.cv.notify_all()
            if data and self.bps:
                t = max(t, time.monotonic()) + len(data) / self.bps
                sleep_until(t)
            with self.ocv:
                self.out.append((time.monotonic() + self.delay, sock, data))
                self.ocv.notify_all()

    def _deliver(self):
        while True:
            with self.ocv:
                while not self.out:
                    self.ocv.wait()
                at, sock, data = self.out[0]
                now = time.monotonic()
                if at > now:
                    self.ocv.wait(at - now)
                    continue
                self.out.popleft()
            try:
                if data is None:
                    sock.shutdown(socket.SHUT_WR)
                else:
                    sock.sendall(data)
            except OSError:
                pass


class ShapedProxy:
    """127.0.0.1:port → 127.0.0.1:upstream，两个方向各经一条共享瓶颈。"""

    def __init__(self, upstream):
        self.up = Bottleneck(BPS, DELAY, QCAP)
        self.down = Bottleneck(BPS, DELAY, QCAP)
        self.upstream = upstream
        self.ls = socket.socket()
        self.ls.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.ls.bind(("127.0.0.1", 0))
        self.ls.listen(64)
        self.port = self.ls.getsockname()[1]
        threading.Thread(target=self._accept, daemon=True).start()

    def _accept(self):
        while True:
            c, _ = self.ls.accept()
            s = socket.create_connection(("127.0.0.1", self.upstream))
            for a, b, neck in ((c, s, self.up), (s, c, self.down)):
                threading.Thread(target=self._pump, args=(a, b, neck), daemon=True).start()

    @staticmethod
    def _pump(src, dst, neck):
        while True:
            try:
                data = src.recv(4096)
            except OSError:
                data = b""
            if not data:
                neck.put(dst, None)
                return
            neck.put(dst, data)


class Backend:
    """一个流模式的后端（stdio 载体）：链路协议 ＋ 传输四条。链路字节带到达时刻（回声延迟要它）。"""

    def __init__(self, bin_path, home):
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": home, "TMUX_TMPDIR": home, "RUST_LOG": "info"}
        self.log = open(os.path.join(home, "backend.log"), "w")
        self.p = subprocess.Popen([bin_path], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log, env=env)
        self.hello = json.loads(self.p.stdout.readline())
        self.cv = threading.Condition()
        self.replies, self.data, self.ends, self.xfer, self.arrivals = {}, {}, {}, {}, {}
        self.n = 0
        self.wlock = threading.Lock()
        threading.Thread(target=self._pump, daemon=True).start()

    def _pump(self):
        for raw in self.p.stdout:
            try:
                f = json.loads(raw)
            except ValueError:
                continue
            now = time.monotonic()
            with self.cv:
                k = f.get("kind")
                if k == "reply":
                    self.replies[f["id"]] = f
                elif k == "link_data":
                    chunk = base64.b64decode(f["data"])
                    self.data.setdefault(f["link"], bytearray()).extend(chunk)
                    for ln in chunk.split(b"\n"):
                        if ln.startswith(b"echo-"):
                            self.arrivals[ln.decode()] = now
                elif k == "link_end":
                    self.ends[f["link"]] = f.get("error")
                elif k == "transfer":
                    self.xfer.setdefault(f["id"], []).append(f)
                self.cv.notify_all()

    def send(self, cmd, args):
        with self.wlock:
            self.n += 1
            rid = f"r{self.n}"
            self.p.stdin.write((json.dumps({"id": rid, "cmd": cmd, "args": args}) + "\n").encode())
            self.p.stdin.flush()
        return rid

    def call(self, cmd, args, timeout=60):
        rid = self.send(cmd, args)
        return self.wait(lambda: self.replies.get(rid), timeout)

    def wait(self, pred, timeout=60):
        end = time.time() + timeout
        with self.cv:
            while True:
                v = pred()
                if v:
                    return v
                left = end - time.time()
                if left <= 0:
                    return None
                self.cv.wait(left)

    def lines(self, link):
        return [ln for ln in bytes(self.data.get(link, b"")).decode("utf-8", "replace").split("\n") if ln]

    def open(self, link, dial, window=1 << 24):
        return self.call("link-open", {"link": link, "window": window, "dial": dial})

    def end_of(self, xid, timeout=300):
        return self.wait(lambda: next((f for f in self.xfer.get(xid, []) if f.get("end")), None), timeout)

    def download(self, dial, remote, local):
        r = self.call("transfer-download", {"dial": dial, "remote_path": remote, "local_path": local})
        xid = r["data"]["id"]
        self.call("transfer-start", {"id": xid})
        return xid

    def close(self):
        self.p.stdin.close()
        self.p.terminate()
        self.p.wait(timeout=10)
        self.log.close()


class Sshd:
    def __init__(self, d, name, max_sessions, sshd, sftp_server, rhome):
        self.port = free_port()
        self.log = f"{d}/{name}.log"
        cfg = f"{d}/{name}_config"
        with open(cfg, "w") as f:
            f.write(
                f"Port {self.port}\nListenAddress 127.0.0.1\nHostKey {d}/host_key\n"
                f"AuthorizedKeysFile {d}/authorized_keys\nPidFile {d}/{name}.pid\nUsePAM no\n"
                "StrictModes no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\n"
                f"PubkeyAuthentication yes\nLogLevel DEBUG1\nMaxSessions {max_sessions}\n"
                f"Subsystem sftp {sftp_server} -d {rhome}\n"
            )
        self.p = subprocess.Popen([sshd, "-D", "-E", self.log, "-f", cfg])
        for _ in range(50):
            try:
                socket.create_connection(("127.0.0.1", self.port), timeout=0.2).close()
                return
            except OSError:
                time.sleep(0.1)
        raise RuntimeError("sshd 起不来")

    def text(self):
        with open(self.log, errors="replace") as fh:
            return fh.read()

    def auths(self):
        return self.text().count("Accepted publickey")


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(round(p / 100 * (len(xs) - 1))))] if xs else float("nan")


def main():
    argv = sys.argv[1:]
    readings_only = "--readings-only" in argv
    only = None
    if "--only" in argv:
        only = {int(x) for x in argv[argv.index("--only") + 1].split(",")}
        del argv[argv.index("--only"): argv.index("--only") + 2]
    args = [a for a in argv if not a.startswith("--")]
    bin_path = args[0] if args else DEFAULT_BIN
    sshd = shutil.which("sshd") or "/usr/sbin/sshd"
    sftp_server = next((p for p in ("/usr/lib/openssh/sftp-server", "/usr/libexec/openssh/sftp-server", "/usr/libexec/sftp-server") if os.path.isfile(p)), "")
    if not os.path.isfile(bin_path) or not os.path.isfile(sshd) or not sftp_server or not shutil.which("ss"):
        print(f"环境不满足：后端 {bin_path} / sshd {sshd} / sftp-server {sftp_server or '找不到'} / ss")
        return 3
    user = os.environ.get("USER") or os.getlogin()
    d = os.path.realpath(tempfile.mkdtemp(prefix="nt1."))
    home, rhome = os.path.join(d, "home"), os.path.join(d, "rhome")
    for p in (home, rhome, os.path.join(home, ".claude-alt"), os.path.join(d, "dl")):
        os.makedirs(p)
    with open(os.path.join(home, ".claude-alt", "accounts.json"), "w") as fh:
        fh.write('{"version":1,"accounts":[]}\n')
    fails, servers, be = [], [], None

    def check(name, cond, detail):
        if readings_only:
            print(f"  read {name}  —— {detail}")
            return
        print(("  ok   " if cond else "  FAIL ") + name + ("" if cond else f"  —— {detail}"))
        if not cond:
            fails.append(name)

    def want(k):
        return only is None or k in only

    try:
        for k in ("host_key", "client_key"):
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", f"{d}/{k}"], check=True)
        shutil.copyfile(f"{d}/client_key.pub", f"{d}/authorized_keys")
        os.chmod(f"{d}/authorized_keys", 0o600)
        a = Sshd(d, "a", 10, sshd, sftp_server, rhome)
        b = Sshd(d, "b", 2, sshd, sftp_server, rhome)
        servers += [a, b]

        def dial(srv, port=None, **kw):
            return {"host": "127.0.0.1", "port": port or srv.port, "user": user, "key_path": f"{d}/client_key", "host_key_fingerprint": None, **kw}

        be = Backend(bin_path, home)
        big = os.path.join(rhome, "big.bin")
        with open(big, "wb") as fh:
            fh.write(os.urandom(16 * 1024 * 1024))

        if want(1):
            print("① 分道：长流在时起一趟下载")
            a0 = a.auths()
            be.open("s1", dial(a, command="cat", use="stream"))
            be.wait(lambda: be.lines("s1") or None, 15)
            xid = be.download(dial(a), big, os.path.join(d, "dl", "big1.bin"))
            be.wait(lambda: any(f["got"] > 0 for f in be.xfer.get(xid, [])) or None, 30)
            during = (a.auths() - a0, established(a.port))
            last = be.end_of(xid)
            after = established(a.port)
            check("传输期间：鉴权 == 2、TCP == 2（长流一条、传输另一条）", during == (2, 2), during)
            check("下载 done", bool(last) and last["end"].get("state") == "done", last)
            check("下载完 ⇒ TCP 仍 == 2（批量连接被主连接托着，下一趟不再握手）", after == 2, after)
            be.call("link-close", {"link": "s1"})
            gone = be.wait(lambda: established(a.port) == 0 or None, 10)
            check("长流关掉 ⇒ 主连接没了、托着的批量连接随之断 ⇒ TCP == 0（按事件收，没有定时器）", bool(gone), established(a.port))

        if want(2):
            print("② 预算满（甲台 MaxSessions 10，每条连接的通道闸 8 格）")
            a0 = a.auths()
            be.open("s2", dial(a, command="cat", use="stream"))
            be.wait(lambda: be.lines("s2") or None, 15)
            for i in range(8):
                be.open(f"c2{i}", dial(a, command=f"sleep 2; echo c{i}", use="capture", capture={"max_bytes": 100}))
            for i in range(8):
                be.wait(lambda i=i: f"c2{i}" in be.ends or None, 30)
            outs = [json.loads(be.lines(f"c2{i}")[1])["stdout"] if len(be.lines(f"c2{i}")) == 2 else be.lines(f"c2{i}") for i in range(8)]
            check("8 条 capture 全部跑通", outs == [f"c{i}\n" for i in range(8)], outs)
            check("鉴权 == 2（第 8 条 capture 时这条连接的 8 格已满 ⇒ 多开一条）", a.auths() - a0 == 2, a.auths() - a0)
            be.call("link-close", {"link": "s2"})
            be.wait(lambda: established(a.port) == 0 or None, 10)

        if want(3):
            print("③ 远端 MaxSessions=2（乙台）")
            b0 = b.auths()
            be.open("s3", dial(b, command="cat", use="stream"))
            be.wait(lambda: be.lines("s3") or None, 15)
            for i in range(3):
                be.open(f"c3{i}", dial(b, command=f"sleep 2; echo m{i}", use="capture", capture={"max_bytes": 100}))
            for i in range(3):
                be.wait(lambda i=i: f"c3{i}" in be.ends or None, 30)
            outs = [json.loads(be.lines(f"c3{i}")[1]) if len(be.lines(f"c3{i}")) == 2 else be.lines(f"c3{i}") for i in range(3)]
            ok = all(isinstance(o, dict) and o.get("stdout") == f"m{i}\n" for i, o in enumerate(outs))
            check("3 条 capture 全部跑通", ok, outs)
            check("鉴权 == 2（被拒那一刻学到这条连接只有 2 格，第 3、4 个通道去新连接）", b.auths() - b0 == 2, b.auths() - b0)
            refusals = [ln for ln in b.text().splitlines() if "no more sessions" in ln.lower() or "open failed" in ln.lower()]
            print(f"  read sshd 那一侧的拒绝原话：{refusals[:2]}")
            be.call("link-close", {"link": "s3"})
            be.wait(lambda: established(b.port) == 0 or None, 10)

        if want(4):
            print("④ 压缩（回环）")
            kex = [ln.split("compression:")[1].split()[0] for ln in (a.text() + b.text()).splitlines() if "kex: client->server" in ln and "compression:" in ln]
            check("我们这几条连接协商出的压缩全是 none（回环不开）", bool(kex) and set(kex) == {"none"}, collections.Counter(kex))

        if want(5):
            print(f"⑤ 弱网读数（整形代理：单程 {DELAY * 1000:.0f} ms · 每方向 {BPS // 1024} KiB/s · 队列 {QCAP // 1024} KiB）")
            px = ShapedProxy(a.port)
            t0 = time.monotonic()
            be.open("q0", dial(a, port=px.port, command="true", use="capture", capture={"max_bytes": 10}))
            be.wait(lambda: "q0" in be.ends or None, 60)
            t_cold = time.monotonic() - t0
            be.open("s5", dial(a, port=px.port, command="cat", use="stream"))
            be.wait(lambda: be.lines("s5") or None, 60)
            t0 = time.monotonic()
            be.open("q1", dial(a, port=px.port, command="true", use="capture", capture={"max_bytes": 10}))
            be.wait(lambda: "q1" in be.ends or None, 60)
            t_warm = time.monotonic() - t0
            print(f"  read 一条 capture：新拨（握手 ＋ 鉴权 ＋ 通道）{t_cold * 1000:.0f} ms · 复用（只开通道）{t_warm * 1000:.0f} ms")

            def echo_series(tag, n, gap):
                sent = {}
                for i in range(n):
                    key = f"echo-{tag}-{i}"
                    sent[key] = time.monotonic()
                    be.call("link-data", {"link": "s5", "data": base64.b64encode((key + "\n").encode()).decode()})
                    time.sleep(gap)
                be.wait(lambda: all(k in be.arrivals for k in sent) or None, 120)
                return [(be.arrivals[k] - t) * 1000 for k, t in sent.items() if k in be.arrivals]

            idle = echo_series("idle", 10, 0.2)
            print(f"  read 空闲时长流回声：p50 {pct(idle, 50):.0f} ms · max {max(idle):.0f} ms")
            xid = be.download(dial(a, port=px.port), big, os.path.join(d, "dl", "big5.bin"))
            be.wait(lambda: any(f["got"] > 256 * 1024 for f in be.xfer.get(xid, [])) or None, 120)
            busy = echo_series("busy", 20, 0.25)
            t0 = time.monotonic()
            last = be.end_of(xid, 600)
            print(f"  read 下载 16 MiB 期间长流回声：p50 {pct(busy, 50):.0f} ms · p95 {pct(busy, 95):.0f} ms · max {max(busy):.0f} ms（下载收场 {last and last['end'].get('state')}，余下 {time.monotonic() - t0:.1f} s）")
            smalls = []
            sub0 = a.text().count("subsystem request for sftp")
            for i in range(6):
                src = os.path.join(rhome, f"small{i}.txt")
                with open(src, "w") as fh:
                    fh.write("x" * 1000)
                t0 = time.monotonic()
                xid = be.download(dial(a, port=px.port), src, os.path.join(d, "dl", f"small{i}.txt"))
                be.end_of(xid, 60)
                smalls.append((time.monotonic() - t0) * 1000)
            print(f"  read 顺序下载 6 个 1 KB 小文件（长流在）：每件 {', '.join(f'{x:.0f}' for x in smalls)} ms（中位 {statistics.median(smalls):.0f} ms；RTT {DELAY * 2000:.0f} ms）")
            subs = a.text().count("subsystem request for sftp") - sub0
            check("顺序 6 趟小下载 ⇒ sshd 记下的 sftp 子系统请求恰好 1 次（空闲会话复用；基线 6 次）", subs == 1, subs)
            be.call("link-close", {"link": "s5"})
    finally:
        if be:
            be.close()
        for s in servers:
            s.p.terminate()
        shutil.rmtree(d, ignore_errors=True)

    if readings_only:
        print("\n（只印读数）")
        return 0
    print(f"\n{'全过' if not fails else '有不对的：' + ', '.join(fails)}")
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
