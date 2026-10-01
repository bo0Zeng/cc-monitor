#!/usr/bin/env python3
# ruff: noqa: E501
"""SR1a：**本机常驻后端经流上的链路（`link-*`）替界面拨 SSH、并复用连接** —— 对着一台真 sshd 的现打。

跑法（仓根下，先 `cd src/backend && cargo build`）：
    python3 tests/evidence/SR1a-link-loopback.py [--monitor] [后端二进制路径]

`--monitor`：后端那几项之后再跑一趟**界面那一侧**（`dial_host_tests::loopback_roundtrip_through_the_resident_backend`，
平时 `#[ignore]`）—— 起一个真后端、用生产那个本机吸收点接上它、宿主经它开链路。

它起一台**临时的回环 sshd**（本用户身份、随机端口、临时 host key 与客户端钥匙、`UsePAM no`、`LogLevel VERBOSE`），
再起一个**流模式**的后端（stdio 载体；私有 HOME / TMUX_TMPDIR、摘掉 TMUX —— `C7i` 红线，不碰用户真实的 tmux server），
在它的 stdin 上发 `link-open` / `link-data` / `link-credit` / `link-close`，从 stdout 收 `reply` / `link_data` / `link_end`。
**不进门禁**（门禁的沙箱里没有 sshd 可起）—— 它是一份读数，不是判据；交付报告里贴的是它的输出。

覆盖（每条都是「真 TCP ＋ 真 SSH 握手 ＋ 真鉴权」）：
  ① capture：链路上恰好两行（ack ＋ 结果），stdout / stderr / 退出码三样收全
  ② 复用：一条长流开着时再开三条 capture ⇒ sshd 那侧**恰好一次**鉴权、**恰好一条** TCP
  ③ 换身份 ⇒ 另拨一条（鉴权次数 +1）
  ④ 最后一条链路关掉 ⇒ 那条 TCP 断（「没有空闲定时器」的收尾）
  ⑤ 流控：窗口 64 KiB、不还信用 ⇒ 收到的字节**恰好** 65536；还 32768 ⇒ 再来**恰好** 32768
  ⑥ 测试连接（stages ＋ probe）不进池：另拨一条、阶段行照出
  ⑦ agent_sock：后端自己的环境里没有 SSH_AUTH_SOCK，界面交过去的那个套接字照样鉴权得过
  ⑧ forward：本地口经隧道到一台回环 HTTP；每接一条报一行 {"accepted":n}；link-close ⇒ 本地口释放
  ⑨ 子系统留口不开：use=subsystem ⇒ unsupported_use
  ⑪ 账号清单变了：改写 manifest ⇒ 恰好一帧 accounts_changed；同目录别的文件 ⇒ 零帧

退出码：0 = 全过 · 1 = 有一条不对 · 3 = 起不来 sshd / 找不到二进制（环境不满足，不是被测对象坏了）
"""

import base64
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEFAULT_BIN = os.path.join(ROOT, ".build", "backend", "debug", "cc-monitor-backend")


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def established(port):
    """此刻连着 sshd 那个口的 TCP 条数（客户端一侧看）。"""
    out = subprocess.run(["ss", "-Htn", "state", "established", f"( dport = :{port} )"], capture_output=True, text=True).stdout
    return len([ln for ln in out.splitlines() if ln.strip()])


class Backend:
    """一个流模式的后端（stdio 载体），说链路协议。"""

    def __init__(self, bin_path, home, env_extra=None):
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": home, "TMUX_TMPDIR": home, "RUST_LOG": "info"}
        env.update(env_extra or {})
        self.log = open(os.path.join(home, "backend.log"), "w")
        self.p = subprocess.Popen([bin_path, "--", "--stream"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log, env=env)
        self.hello = json.loads(self.p.stdout.readline())
        self.cv = threading.Condition()
        self.replies = {}
        self.data = {}
        self.ends = {}
        self.accounts_changed = 0
        self.n = 0
        threading.Thread(target=self._pump, daemon=True).start()

    def _pump(self):
        for raw in self.p.stdout:
            try:
                f = json.loads(raw)
            except ValueError:
                continue
            with self.cv:
                k = f.get("kind")
                if k == "reply":
                    self.replies[f["id"]] = f
                elif k == "link_data":
                    self.data.setdefault(f["link"], bytearray()).extend(base64.b64decode(f["data"]))
                elif k == "link_end":
                    self.ends[f["link"]] = f.get("error")
                elif k == "accounts_changed":
                    self.accounts_changed += 1
                self.cv.notify_all()

    def send(self, cmd, args):
        self.n += 1
        rid = f"r{self.n}"
        self.p.stdin.write((json.dumps({"id": rid, "cmd": cmd, "args": args}) + "\n").encode())
        self.p.stdin.flush()
        return rid

    def call(self, cmd, args, timeout=30):
        rid = self.send(cmd, args)
        return self.wait(lambda: self.replies.get(rid), timeout)

    def wait(self, pred, timeout=30):
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

    def open(self, link, dial, window=1 << 20):
        return self.call("link-open", {"link": link, "window": window, "dial": dial})

    def until_end(self, link, timeout=30):
        return self.wait(lambda: link in self.ends, timeout)

    def close(self):
        self.p.stdin.close()
        self.p.terminate()
        self.p.wait(timeout=10)
        self.log.close()


def main():
    args = [a for a in sys.argv[1:] if a != "--monitor"]
    with_monitor = "--monitor" in sys.argv[1:]
    bin_path = args[0] if args else DEFAULT_BIN
    sshd = shutil.which("sshd") or "/usr/sbin/sshd"
    if not os.path.isfile(bin_path) or not os.path.isfile(sshd) or not shutil.which("ss"):
        print(f"环境不满足：后端 {bin_path} / sshd {sshd} / ss")
        return 3
    user = os.environ.get("USER") or os.getlogin()
    d = tempfile.mkdtemp(prefix="sr1a.")
    home = os.path.join(d, "home")
    os.makedirs(home)
    accts = os.path.join(home, ".claude-alt")  # 账号 manifest 的缺省住址（`$HOME/.claude-alt/accounts.json`）
    os.makedirs(accts)
    with open(os.path.join(accts, "accounts.json"), "w") as fh:
        fh.write('{"version":1,"accounts":[]}\n')
    procs = []
    fails = []
    be = None

    def check(name, cond, detail):
        print(("  ok   " if cond else "  FAIL ") + name + ("" if cond else f"  —— {detail}"))
        if not cond:
            fails.append(name)

    try:
        for k in ("host_key", "client_key"):
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", f"{d}/{k}"], check=True)
        shutil.copyfile(f"{d}/client_key.pub", f"{d}/authorized_keys")
        os.chmod(f"{d}/authorized_keys", 0o600)
        port = free_port()
        slog = f"{d}/sshd.log"
        with open(f"{d}/sshd_config", "w") as f:
            f.write(
                f"Port {port}\nListenAddress 127.0.0.1\nHostKey {d}/host_key\n"
                f"AuthorizedKeysFile {d}/authorized_keys\nPidFile {d}/sshd.pid\nUsePAM no\n"
                "StrictModes no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\n"
                "PubkeyAuthentication yes\nAllowTcpForwarding yes\nLogLevel VERBOSE\nMaxSessions 64\n"
            )
        procs.append(subprocess.Popen([sshd, "-D", "-E", slog, "-f", f"{d}/sshd_config"]))
        for _ in range(50):
            try:
                socket.create_connection(("127.0.0.1", port), timeout=0.2).close()
                break
            except OSError:
                time.sleep(0.1)
        else:
            print("sshd 起不来")
            return 3

        def auths():
            with open(slog) as fh:
                return fh.read().count("Accepted publickey")

        fp = subprocess.run(["ssh-keygen", "-l", "-E", "sha256", "-f", f"{d}/host_key.pub"], capture_output=True, text=True).stdout.split()[1]
        base = {"host": "127.0.0.1", "port": port, "user": user, "key_path": f"{d}/client_key", "host_key_fingerprint": None}
        be = Backend(bin_path, home)
        check("后端的 hello 声明了链路四条", all(c in be.hello.get("commands", []) for c in ("link-open", "link-data", "link-credit", "link-close")), be.hello.get("commands"))

        print("① capture")
        r = be.open("c1", {**base, "command": "echo out; echo err >&2; exit 7", "use": "capture", "capture": {"max_bytes": 1000}})
        check("link-open 回 ok", bool(r and r["ok"]), r)
        be.until_end("c1")
        ls = be.lines("c1")
        ok = len(ls) == 2
        ack, res = (json.loads(ls[0]), json.loads(ls[1])) if ok else ({}, {})
        # uses 多了 files（部署那几问的链路，写只许 ~/.cc-monitor/{staging,bin}）。
        check("链路上恰好两行：ack（v=2 · uses）＋ 结果", ok and ack.get("ok") and ack.get("v") == 2 and ack.get("uses") == ["stream", "capture", "forward", "files"], ls)
        check("stdout/stderr/退出码", (res.get("stdout"), res.get("stderr"), res.get("exit_status")) == ("out\n", "err\n", 7), res)
        check("收尾：link_end 无 error", be.ends.get("c1", "missing") is None, be.ends.get("c1"))

        print("② 复用")
        # 第一条连接（①那条）只被 c1 用、c1 结束了 ⇒ 已断。等它断干净再数。
        be.wait(lambda: established(port) == 0 or None, 5)
        a0 = auths()
        be.open("s", {**base, "command": "cat", "use": "stream"})
        be.wait(lambda: len(be.lines("s")) >= 1 or None, 15)
        check("长流的 ack ok", be.lines("s")[:1] and json.loads(be.lines("s")[0]).get("ok"), be.lines("s"))
        for i in range(3):
            be.open(f"q{i}", {**base, "command": f"echo q{i}", "use": "capture", "capture": {"max_bytes": 100}})
            be.until_end(f"q{i}")
        qs = [json.loads(be.lines(f"q{i}")[1])["stdout"] for i in range(3) if len(be.lines(f"q{i}")) == 2]
        check("三条 capture 都跑通", qs == ["q0\n", "q1\n", "q2\n"], qs)
        check("sshd 只鉴权了一次（长流 ＋ 三条 capture 共用一条连接）", auths() - a0 == 1, auths() - a0)
        check("恰好一条 TCP", established(port) == 1, established(port))
        # 上行：长流是 cat，写进去什么回来什么。
        rid = be.send("link-data", {"link": "s", "data": base64.b64encode(b"ping\n").decode()})
        ackd = be.wait(lambda: be.replies.get(rid), 10)
        be.wait(lambda: "ping" in be.lines("s") or None, 10)
        check("上行：link-data 写进去、应答 ok、原样回来", bool(ackd and ackd["ok"]) and "ping" in be.lines("s"), (ackd, be.lines("s")))

        print("③ 换身份 ⇒ 另拨")
        a1 = auths()
        be.open("x", {**base, "host_key_fingerprint": fp, "command": "echo x", "use": "capture", "capture": {"max_bytes": 100}})
        be.until_end("x")
        check("指纹那一项不同 ⇒ 另拨一条（鉴权 +1）", auths() - a1 == 1, auths() - a1)

        print("④ 最后一条链路关掉 ⇒ 连接断")
        be.wait(lambda: established(port) == 1 or None, 5)
        r = be.call("link-close", {"link": "s"})
        gone = be.wait(lambda: established(port) == 0 or None, 10)
        check("link-close 回 ok、TCP 随之断", bool(r and r["ok"]) and bool(gone), (r, established(port)))

        print("⑤ 流控（窗口 64 KiB）")
        be.open("f", {**base, "command": "head -c 4000000 /dev/zero", "use": "stream"}, window=65536)
        be.wait(lambda: len(be.data.get("f", b"")) >= 65536 or None, 15)
        time.sleep(2)
        got0 = len(be.data.get("f", b""))
        ack_len = len(be.lines("f")[0]) + 1 if be.lines("f") else 0
        check("不还信用 ⇒ 收到的字节恰好等于窗口", got0 == 65536, (got0, "其中 ack 行", ack_len))
        be.call("link-credit", {"link": "f", "bytes": 32768})
        be.wait(lambda: len(be.data.get("f", b"")) >= got0 + 32768 or None, 10)
        time.sleep(1)
        got1 = len(be.data.get("f", b""))
        check("还 32768 ⇒ 再来恰好 32768", got1 - got0 == 32768, got1 - got0)
        t0 = time.time()
        while "f" not in be.ends and time.time() - t0 < 30:
            have = len(be.data.get("f", b""))
            be.call("link-credit", {"link": "f", "bytes": 1 << 20})
            be.wait(lambda: len(be.data.get("f", b"")) > have or "f" in be.ends or None, 5)
        check("一直还信用 ⇒ 全部收完（ack ＋ 4000000）", "f" in be.ends and len(be.data["f"]) == ack_len + 4000000, (len(be.data.get("f", b"")), ack_len))

        print("⑥ 测试连接不进池")
        be.open("keep", {**base, "command": "cat", "use": "stream"})
        be.wait(lambda: be.lines("keep") or None, 15)
        a2 = auths()
        be.open("p", {**base, "command": "true", "use": "capture", "capture": {"max_bytes": 10}, "stages": True, "probe": True})
        be.until_end("p")
        kinds = [json.loads(x)["stage"]["kind"] for x in be.lines("p") if "stage" in json.loads(x)]
        check("阶段行照出", kinds[:1] == ["dialing"] and kinds[-1:] == ["established"], kinds)
        check("池里已有同身份连接，测试连接照样另拨（鉴权 +1）", auths() - a2 == 1, auths() - a2)
        be.call("link-close", {"link": "keep"})

        print("⑦ agent_sock（后端环境里没有 SSH_AUTH_SOCK）")
        ag = tempfile.mkdtemp(prefix="sr1aag.", dir="/tmp")  # 套接字路径有长度上限
        sock = f"{ag}/s"
        procs.append(subprocess.Popen(["ssh-agent", "-D", "-a", sock], stdout=subprocess.DEVNULL))
        for _ in range(50):
            if os.path.exists(sock):
                break
            time.sleep(0.1)
        subprocess.run(["ssh-add", "-q", f"{d}/client_key"], env={**os.environ, "SSH_AUTH_SOCK": sock}, check=True, stderr=subprocess.DEVNULL)
        be.open("g", {**base, "key_path": None, "agent_sock": sock, "command": "echo via-agent", "use": "capture", "capture": {"max_bytes": 100}})
        be.until_end("g")
        gl = be.lines("g")
        check("不给私钥路径、走界面交过来的 agent 套接字", len(gl) == 2 and json.loads(gl[1])["stdout"] == "via-agent\n", gl)

        print("⑧ forward")
        web, lp = free_port(), free_port()
        procs.append(subprocess.Popen([sys.executable, "-m", "http.server", str(web), "--bind", "127.0.0.1", "--directory", d],
                                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        time.sleep(0.5)
        be.open("w", {**base, "use": "forward", "forward": {"local_port": lp, "remote_host": "127.0.0.1", "remote_port": web}})
        be.wait(lambda: be.lines("w") or None, 15)
        first = json.loads(be.lines("w")[0])
        codes = [urllib.request.urlopen(f"http://127.0.0.1:{lp}/sshd_config", timeout=5).status for _ in range(2)]
        be.wait(lambda: len(be.lines("w")) >= 3 or None, 5)
        rest = [json.loads(x) for x in be.lines("w")[1:]]
        check("ack 之后接两条连接、各报一行", first.get("ok") and codes == [200, 200] and rest == [{"accepted": 1}, {"accepted": 2}], (first, codes, rest))
        be.call("link-close", {"link": "w"})
        released = be.wait(lambda: socket.socket().connect_ex(("127.0.0.1", lp)) != 0 or None, 5)
        check("link-close ⇒ 本地口释放", bool(released), lp)

        print("⑨ 子系统留口不开")
        r = be.open("sub", {**base, "use": "subsystem"})
        check("use=subsystem ⇒ unsupported_use", bool(r) and not r["ok"] and r.get("code") == "unsupported_use", r)

        print("⑪ 账号清单变了")
        base_n = be.accounts_changed
        with open(os.path.join(accts, "accounts.json"), "w") as fh:
            fh.write('{"version":1,"accounts":[{"name":"x","configDir":"/tmp/x"}]}\n')
        be.wait(lambda: be.accounts_changed > base_n or None, 10)
        time.sleep(2)
        check("改写 manifest ⇒ 恰好一帧 accounts_changed", be.accounts_changed - base_n == 1, be.accounts_changed - base_n)
        mid = be.accounts_changed
        with open(os.path.join(accts, "notes.txt"), "w") as fh:
            fh.write("x\n")
        time.sleep(2)
        check("同目录别的文件 ⇒ 零帧", be.accounts_changed == mid, be.accounts_changed - mid)

        if with_monitor:
            print("⑩ 界面那一侧（dial_host → link_mux → 真后端 → 真 sshd）")
            env = {**os.environ,
                   "SR1A_LOOPBACK": json.dumps({"host": "127.0.0.1", "port": port, "user": user, "key_path": f"{d}/client_key",
                                                "backend": bin_path, "home": home})}
            r = subprocess.run(["cargo", "test", "-p", "monitor", "--lib", "loopback_roundtrip_through_the_resident_backend", "--", "--ignored", "--nocapture"],
                               cwd=os.path.join(ROOT, "src", "frontend", "shell"), env=env, capture_output=True, text=True, timeout=3000)
            check("字节流 · 收全 · 阶段 ＋ 指纹 全经本机常驻后端", "SR1A-LOOPBACK-MONITOR ok" in r.stdout and "1 passed" in r.stdout,
                  (r.stdout[-1200:], r.stderr[-800:]))
    finally:
        if be:
            be.close()
        for p in procs:
            p.terminate()
        shutil.rmtree(d, ignore_errors=True)

    print(f"\n{'全过' if not fails else '有不对的：' + ', '.join(fails)}")
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
