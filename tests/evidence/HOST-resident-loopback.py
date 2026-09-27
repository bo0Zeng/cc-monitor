#!/usr/bin/env python3
# ruff: noqa: E501
"""HOST · V139：**远端常驻后端**（起 · 找 · 只升不降 · 停 · 多客户 · 经 SSH 隧道接监听口）对着一台真回环 sshd 的现打。

跑法（仓根下，先 `cd src/backend && cargo build`）：
    python3 tests/evidence/HOST-resident-loopback.py [后端二进制路径]

台架：临时回环 sshd（本用户、随机端口、临时 host key 与客户端钥匙、`UsePAM no`）；「远端」那一侧的每条命令前缀
`env -u TMUX HOME=<临时远端家> TMUX_TMPDIR=<同> CLAUDE_CONFIG_DIR=<同>/.claude` —— 不碰用户真 `~/.cc-monitor`、真 tmux。
「monitor」那一侧 = 一个 stdio 载体的本机后端（私有 HOME），脚本在它的 stdin 上发 `link-*`（与 `SR1a-link-loopback.py` 同一个驱动）。
临时目录放 `<仓根>/.scratch/`（不放 /tmp）；起过的进程收尾时逐个收掉。**不进门禁**，是一份读数。

退出码：0 = 全过 · 1 = 有一条不对 · 3 = 环境不满足
"""

import base64
import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEFAULT_BIN = os.path.join(ROOT, ".build", "backend", "debug", "cc-monitor-backend")


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


class Backend:
    """monitor 那一侧：stdio 载体的本机后端，说链路协议。"""

    def __init__(self, bin_path, home):
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": home, "TMUX_TMPDIR": home, "RUST_LOG": "info"}
        self.log = open(os.path.join(home, "local-backend.log"), "a")
        self.p = subprocess.Popen([bin_path], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log, env=env)
        self.hello = json.loads(self.p.stdout.readline())
        self.cv = threading.Condition()
        self.replies, self.data, self.ends, self.n = {}, {}, {}, 0
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
                self.cv.notify_all()

    def call(self, cmd, args, timeout=30):
        self.n += 1
        rid = f"r{self.n}"
        self.p.stdin.write((json.dumps({"id": rid, "cmd": cmd, "args": args}) + "\n").encode())
        self.p.stdin.flush()
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

    def send(self, link, text):
        return self.call("link-data", {"link": link, "data": base64.b64encode(text.encode()).decode()})

    def capture(self, link, base, cmd):
        r = self.call("link-open", {"link": link, "window": 1 << 20, "dial": {**base, "command": cmd, "use": "capture", "capture": {"max_bytes": 65536}}})
        if not (r and r["ok"]):
            return None
        self.wait(lambda: link in self.ends, 30)
        ls = self.lines(link)
        return json.loads(ls[1]) if len(ls) == 2 and json.loads(ls[0]).get("ok") else None

    def tunnel(self, link, base, port):
        """开隧道 → 读 ack 与 hello。回 (ack, hello) 或 None。"""
        r = self.call("link-open", {"link": link, "window": 1 << 20, "dial": {**base, "use": "tunnel", "tunnel_port": port}})
        if not (r and r["ok"]):
            return None
        got = self.wait(lambda: len(self.lines(link)) >= 2 or link in self.ends, 15)
        ls = self.lines(link)
        if not got or len(ls) < 2:
            return (json.loads(ls[0]) if ls else None, None)
        return json.loads(ls[0]), json.loads(ls[1])

    def close(self):
        try:
            self.p.stdin.close()
        except OSError:
            pass
        self.p.terminate()
        self.p.wait(timeout=10)
        self.log.close()


def pid_alive(pid):
    try:
        os.kill(pid, 0)
    except OSError:
        return False
    with open(f"/proc/{pid}/stat") as fh:
        return fh.read().split(")")[-1].split()[0] != "Z"


def wait_for(pred, timeout=10):
    end = time.time() + timeout
    while time.time() < end:
        if pred():
            return True
        time.sleep(0.1)
    return False


def main():
    bin_path = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_BIN
    sshd = shutil.which("sshd") or "/usr/sbin/sshd"
    if not os.path.isfile(bin_path) or not os.path.isfile(sshd):
        print(f"环境不满足：后端 {bin_path} / sshd {sshd}")
        return 3
    user = os.environ.get("USER") or os.getlogin()
    os.makedirs(os.path.join(ROOT, ".scratch"), exist_ok=True)
    d = tempfile.mkdtemp(prefix="host-resident.", dir=os.path.join(ROOT, ".scratch"))
    lhome, rhome = os.path.join(d, "lhome"), os.path.join(d, "rhome")
    for h in (lhome, rhome):
        os.makedirs(os.path.join(h, ".claude", "sessions"))
        os.makedirs(os.path.join(h, ".claude", "projects"))
    procs, fails, bes, residents = [], [], [], set()

    def check(name, cond, detail=""):
        print(("  ok   " if cond else "  FAIL ") + name + ("" if cond else f"  —— {detail}"))
        if not cond:
            fails.append(name)

    # 中转口 8788 是这台机器上真会话在用的门牌：台架起的常驻后端会去 bind 它（V139）。口空着时先由脚本占住（不 accept），
    # 让台架那一位的中转「起不来、出声」而不是真去答话 —— 不替这台机器上的真会话接请求。
    hold = socket.socket()
    try:
        hold.bind(("127.0.0.1", 8788))
        hold.listen(1)
    except OSError:
        hold.close()
        hold = None
    remote = f"env -u TMUX HOME={rhome} TMUX_TMPDIR={rhome} CLAUDE_CONFIG_DIR={rhome}/.claude '{bin_path}'"
    try:
        for k in ("host_key", "client_key"):
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", f"{d}/{k}"], check=True)
        shutil.copyfile(f"{d}/client_key.pub", f"{d}/authorized_keys")
        os.chmod(f"{d}/authorized_keys", 0o600)
        port = free_port()
        with open(f"{d}/sshd_config", "w") as f:
            f.write(
                f"Port {port}\nListenAddress 127.0.0.1\nHostKey {d}/host_key\nAuthorizedKeysFile {d}/authorized_keys\n"
                f"PidFile {d}/sshd.pid\nUsePAM no\nStrictModes no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\n"
                "PubkeyAuthentication yes\nAllowTcpForwarding yes\nLogLevel VERBOSE\nMaxSessions 64\n"
            )
        procs.append(subprocess.Popen([sshd, "-D", "-E", f"{d}/sshd.log", "-f", f"{d}/sshd_config"]))
        if not wait_for(lambda: socket.socket().connect_ex(("127.0.0.1", port)) == 0, 5):
            print("sshd 起不来")
            return 3
        base = {"host": "127.0.0.1", "port": port, "user": user, "key_path": f"{d}/client_key", "host_key_fingerprint": None}

        be = Backend(bin_path, lhome)
        bes.append(be)
        print("① 起：--resident-ensure（经链路 capture）")
        c = be.capture("e1", base, f"{remote} --resident-ensure")
        ans = json.loads(c["stdout"]) if c and c.get("exit_status") == 0 else {}
        check("退出 0、答 {port,token,pid}", {"port", "token", "pid"} <= set(ans), c)
        rport, token, pid1 = ans.get("port"), ans.get("token", ""), ans.get("pid")
        tok_path = os.path.join(rhome, ".cc-monitor", "listen-token")
        check("钥匙文件 0600、内容就是答的那一把", os.path.exists(tok_path) and (os.stat(tok_path).st_mode & 0o777) == 0o600 and open(tok_path).read().strip() == token)
        check("子进程活着", bool(pid1) and wait_for(lambda: pid_alive(pid1), 3), pid1)
        if pid1:
            residents.add(pid1)
        env_raw = open(f"/proc/{pid1}/environ", "rb").read() if pid1 and pid_alive(pid1) else b""
        cmd_raw = open(f"/proc/{pid1}/cmdline", "rb").read() if pid1 and pid_alive(pid1) else b""
        check("钥匙不在子进程的 env / argv 里（env 里只有钥匙文件路径）", bool(token) and token.encode() not in env_raw and token.encode() not in cmd_raw and b"CCM_LISTEN_TOKEN_FILE=" in env_raw)
        check("子进程被交了中转口（V139：远端中转进程内起）", b"CCM_RELAY_PORT=8788" in env_raw)
        log = os.path.join(rhome, ".cc-monitor", "logs", "backend", "stderr.log")  # 〔GAP1〕与本机同一层级
        check("它的诊断落进远端家目录下的 stderr 文件、里面有中转那一句", wait_for(lambda: os.path.exists(log) and "relay" in open(log, errors="replace").read(), 5))

        print("② 接：tunnel → hello → attach（多客户：两条同时）")
        got = None
        for _ in range(30):
            got = be.tunnel("t1", base, rport)
            if got and got[1]:
                break
            be.call("link-close", {"link": "t1"})
            be.data.pop("t1", None)
            be.ends.pop("t1", None)
            time.sleep(0.2)
        check("隧道 ack ok、第一行是 hello", bool(got and got[0] and got[0].get("ok") and got[1] and got[1].get("kind") == "hello"), got)
        be.send("t1", json.dumps({"attach": token, "flags": ["--tail-only"]}) + "\n")
        check("第一条 attach ⇒ ok", be.wait(lambda: len(be.lines("t1")) >= 3, 10) and json.loads(be.lines("t1")[2]) == {"attach": "ok"}, be.lines("t1"))
        got2 = be.tunnel("t2", base, rport)
        be.send("t2", json.dumps({"attach": token}) + "\n")
        check("第二条同时 attach ⇒ 也 ok（不再 stream-busy）", be.wait(lambda: len(be.lines("t2")) >= 3, 10) and json.loads(be.lines("t2")[2]) == {"attach": "ok"}, be.lines("t2"))
        check("两条都有帧流过来（sessions_replayed）", be.wait(lambda: any('"sessions_replayed"' in ln for ln in be.lines("t1")) and any('"sessions_replayed"' in ln for ln in be.lines("t2")), 10))
        got3 = be.tunnel("t3", base, rport)
        be.send("t3", json.dumps({"attach": "0" * 32}) + "\n")
        check("钥匙不对 ⇒ 出声拒（bad-token）", be.wait(lambda: len(be.lines("t3")) >= 3, 10) and json.loads(be.lines("t3")[2]).get("reason") == "bad-token", be.lines("t3"))

        print("③ SSH 断了它还在（独立生命周期，默认「不结束」）")
        be.close()
        bes.remove(be)
        time.sleep(1.0)
        check("本机后端（持 SSH 的那一个）退了之后，远端常驻后端还活着", pid_alive(pid1))
        s = socket.create_connection(("127.0.0.1", rport), timeout=3)
        s.settimeout(3)
        first = s.makefile("rb").readline()
        s.close()
        check("它的口还在听、照样先说 hello", b'"kind":"hello"' in first, first[:80])

        print("④ 找：再 ensure 一次 ⇒ 同一个常驻后端（新起的子进程绑不上口、退 3）")
        be = Backend(bin_path, lhome)
        bes.append(be)
        c2 = be.capture("e2", base, f"{remote} --resident-ensure")
        ans2 = json.loads(c2["stdout"]) if c2 and c2.get("exit_status") == 0 else {}
        pid2 = ans2.get("pid")
        if pid2:
            residents.add(pid2)
        check("同一个口、同一把钥匙", ans2.get("port") == rport and ans2.get("token") == token, ans2)
        check("第二次起的子进程自己退了", bool(pid2) and wait_for(lambda: not pid_alive(pid2), 5), pid2)
        owner = open(os.path.join(rhome, ".cc-monitor", f"listen-{rport}.pid")).read().split()
        check("pid 文件记的仍是第一个", owner and int(owner[0]) == pid1, owner)

        print("⑤ 只升不降：--replace ⇒ 旧的收 SIGTERM 退，再 ensure 起新的")
        c3 = be.capture("e3", base, f"{remote} --resident-ensure --replace")
        ans3 = json.loads(c3["stdout"]) if c3 and c3.get("exit_status") == 0 else {}
        if ans3.get("pid"):
            residents.add(ans3["pid"])
        check("旧的那一个退了", wait_for(lambda: not pid_alive(pid1), 10))
        pid_new = None
        for i in range(20):
            c4 = be.capture(f"e4-{i}", base, f"{remote} --resident-ensure")
            a4 = json.loads(c4["stdout"]) if c4 and c4.get("exit_status") == 0 else {}
            if a4.get("pid"):
                residents.add(a4["pid"])
            if wait_for(lambda: os.path.exists(os.path.join(rhome, ".cc-monitor", f"listen-{rport}.pid")) and int(open(os.path.join(rhome, ".cc-monitor", f"listen-{rport}.pid")).read().split()[0]) not in (pid1,), 2):
                pid_new = int(open(os.path.join(rhome, ".cc-monitor", f"listen-{rport}.pid")).read().split()[0])
                break
        check("换上了一个新的常驻后端", bool(pid_new) and pid_alive(pid_new), pid_new)

        print("⑥ 停：--resident-stop")
        c5 = be.capture("s1", base, f"{remote} --resident-stop")
        ans5 = json.loads(c5["stdout"]) if c5 and c5.get("exit_status") == 0 else {}
        # 〔STOP〕结局三个词：那台自己等到它退了才答 ⇒ 答回来的那一刻它已经不在（不用再等）。
        check("答 graceful ＋ 那个 pid", ans5 == {"stopped": "graceful", "pid": pid_new}, ans5)
        check("答回来时它已经退了、口关了", not pid_alive(pid_new) and socket.socket().connect_ex(("127.0.0.1", rport)) != 0)
        c6 = be.capture("s2", base, f"{remote} --resident-stop")
        check("再停一次 ⇒ not_running（本来就没在跑）", c6 and json.loads(c6["stdout"]) == {"stopped": "not_running", "pid": None}, c6)
    finally:
        for be in bes:
            try:
                be.close()
            except Exception:  # noqa: BLE001
                pass
        for pid in residents:
            if pid_alive(pid):
                os.kill(pid, signal.SIGKILL)
        for p in procs:
            p.terminate()
            p.wait(timeout=10)
        if hold:
            hold.close()
        shutil.rmtree(d, ignore_errors=True)
    print("全过" if not fails else f"不对 {len(fails)} 条：{fails}")
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
