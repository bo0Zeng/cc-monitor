#!/usr/bin/env python3
# ruff: noqa: E501
"""SR1b：**SFTP 住本机常驻后端**（传输四条 ＋ `files` 链路）—— 对着一台真 sshd 的现打。

跑法（仓根下，先 `cd src/backend && cargo build`）：
    python3 tests/evidence/SR1b-sftp-loopback.py [--monitor] [后端二进制路径]

`--monitor`：后端那几项之后再跑一趟**界面那一侧**（`sftp_tests::sr1b_loopback_deploy_and_transfer_through_the_resident_backend`，
平时 `#[ignore]`）—— 起一个真后端、用生产那个本机吸收点接上它，部署那几问经 `RemoteFs`、传输经中继。

它起一台**临时的回环 sshd**（本用户身份、随机端口、临时 host key 与客户端钥匙、`UsePAM no`、`LogLevel VERBOSE`），
🔴 **sftp 子系统的起始目录钉在一个临时目录**（`sftp-server -d <rhome>`，日志 `-e` 接进临时文件）⇒ 后端眼里的远端 home 是 `rhome`，
写不到这台机器上真的 `~/.cc-monitor/`。再起一个**流模式**的后端（stdio 载体；私有 HOME / TMUX_TMPDIR、摘掉 TMUX）。
**不进门禁**（门禁的沙箱里没有 sshd 可起）—— 它是一份读数，不是判据；交付报告里贴的是它的输出。

覆盖（每条都是「真 TCP ＋ 真 SSH 握手 ＋ 真鉴权 ＋ 真 sftp 子系统」）：
  ① files 链路：ack 的 uses 含 files；home == sshd 给的起始目录
  ② 部署那几问：建目录 · 原子上传 ＋ 读回比对 · stat · read · remove（两次，第二次 removed=false）；盘上逐字节对（sshd 那侧的文件系统，异源）
  ③ 远端写围栏：两个根之外 / 绝对路径出 home / `..` / 根里一条指向外面的链接 ⇒ fenced，盘上零改动（整棵 rhome 前后快照相等）
  ④ 上传：只写 `~/.cc-monitor/staging/<key>.part`，逐字节等于本机那份；帧序 got 单调、最后一帧 done
  ⑤ 续传：暂存件先放前 N 字节 ⇒ sftp-server 记下的这一趟写入字节数 == 总长 − N
  ⑥ 撤：上传撤 ⇒ 暂存件删；下载撤 ⇒ `.part` 留
  ⑦ 下载：落地逐字节对、`.part` 不留；远端不存在 ⇒ failed、`.part` 不留；本机落点是会话文件 ⇒ 照样开单（〔WF2 跟上〕V119 之后没有数据围栏，`设计/60 §3.5`）
  ⑧ 〔NT1 改〕长流 ＋ files 链路一条、全部传输分道一条：sshd **恰好两次**鉴权、**恰好两条** TCP
  ⑨ 子系统留口仍不开：use=subsystem ⇒ unsupported_use

退出码：0 = 全过 · 1 = 有一条不对 · 3 = 起不来 sshd / 找不到二进制（环境不满足，不是被测对象坏了）
"""

import base64
import hashlib
import json
import os
import shutil
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


def established(port):
    out = subprocess.run(["ss", "-Htn", "state", "established", f"( dport = :{port} )"], capture_output=True, text=True).stdout
    return len([ln for ln in out.splitlines() if ln.strip()])


def tree(root):
    """整棵目录此刻的样子：{相对路径: (类型, sha256 或链接目标)}。"""
    out = {}
    for dp, dns, fns in os.walk(root, followlinks=False):
        for n in dns + fns:
            p = os.path.join(dp, n)
            rel = os.path.relpath(p, root)
            if os.path.islink(p):
                out[rel] = ("link", os.readlink(p))
            elif os.path.isdir(p):
                out[rel] = ("dir", None)
            else:
                with open(p, "rb") as fh:
                    out[rel] = ("file", hashlib.sha256(fh.read()).hexdigest())
    return out


class Backend:
    """一个流模式的后端（stdio 载体）：链路协议 ＋ 传输四条。"""

    def __init__(self, bin_path, home):
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": home, "TMUX_TMPDIR": home, "RUST_LOG": "info"}
        self.log = open(os.path.join(home, "backend.log"), "w")
        self.p = subprocess.Popen([bin_path, "--", "--stream"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log, env=env)
        self.hello = json.loads(self.p.stdout.readline())
        self.cv = threading.Condition()
        self.replies, self.data, self.ends, self.xfer = {}, {}, {}, {}
        self.n = 0
        self.wlock = threading.Lock()
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

    def open(self, link, dial, window=1 << 24):
        return self.call("link-open", {"link": link, "window": window, "dial": dial})

    def end_of(self, xid, timeout=120):
        """这一趟的最后一帧（带 end 的那一帧）。"""
        return self.wait(lambda: next((f for f in self.xfer.get(xid, []) if f.get("end")), None), timeout)

    def close(self):
        self.p.stdin.close()
        self.p.terminate()
        self.p.wait(timeout=10)
        self.log.close()


class Files:
    """一条 files 链路：上行一行请求（put 后跟原始字节，按 32 KiB 一块 link-data 送），下行一行应答。"""

    def __init__(self, be, link):
        self.be, self.link, self.seen = be, link, 1  # 第 0 行是 ack

    def _up(self, raw):
        for i in range(0, len(raw), 32768):
            r = self.be.call("link-data", {"link": self.link, "data": base64.b64encode(raw[i:i + 32768]).decode()})
            assert r and r["ok"], r

    def ask(self, req, body=b""):
        self._up((json.dumps(req) + "\n").encode() + body)
        want = self.seen + 1
        self.be.wait(lambda: len(self.be.lines(self.link)) >= want or None, 60)
        ls = self.be.lines(self.link)
        self.seen = want
        return json.loads(ls[want - 1]) if len(ls) >= want else {"code": "timeout", "message": str(ls[-2:])}


def main():
    args = [a for a in sys.argv[1:] if a != "--monitor"]
    with_monitor = "--monitor" in sys.argv[1:]
    bin_path = args[0] if args else DEFAULT_BIN
    sshd = shutil.which("sshd") or "/usr/sbin/sshd"
    sftp_server = next((p for p in ("/usr/lib/openssh/sftp-server", "/usr/libexec/openssh/sftp-server", "/usr/libexec/sftp-server") if os.path.isfile(p)), "")
    if not os.path.isfile(bin_path) or not os.path.isfile(sshd) or not sftp_server or not shutil.which("ss"):
        print(f"环境不满足：后端 {bin_path} / sshd {sshd} / sftp-server {sftp_server or '找不到'} / ss")
        return 3
    user = os.environ.get("USER") or os.getlogin()
    d = os.path.realpath(tempfile.mkdtemp(prefix="sr1b."))
    home = os.path.join(d, "home")  # 后端自己的 HOME（本机那一侧）
    rhome = os.path.join(d, "rhome")  # sshd 给 sftp 的起始目录（远端那一侧）
    outside = os.path.join(d, "outside")  # 链接逃逸的目标
    for p in (home, rhome, outside, os.path.join(home, ".claude", "projects", "p"), os.path.join(home, ".claude-accts")):
        os.makedirs(p)
    with open(os.path.join(home, ".claude-accts", "accounts.json"), "w") as fh:
        fh.write('{"version":1,"accounts":[]}\n')
    procs, fails = [], []
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
                "PubkeyAuthentication yes\nLogLevel VERBOSE\nMaxSessions 10\n"
                f"Subsystem sftp {sftp_server} -e -l VERBOSE -d {rhome} 2>>{d}/sftp.log\n"
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

        def slog_text():
            with open(slog, errors="replace") as fh:
                return fh.read()

        def auths():
            return slog_text().count("Accepted publickey")

        def written(path):
            """sftp-server 记下的、关掉 `path` 那一次的写入字节数（最后一次）。它的日志经 `-e` 打到 stderr，再由子系统那行命令接进 `sftp.log`。"""
            n = None
            with open(f"{d}/sftp.log", errors="replace") as fh:
                log = fh.read()
            for ln in log.splitlines():
                if f'close "{path}"' in ln and " written " in ln:
                    n = int(ln.rsplit(" written ", 1)[1].split()[0])
            return n

        # 〔WF2 · WIN3 §2〕拨号只收界面那一格原样的配置（MIG-1 之后 `dial/machine.rs::resolve`）；平铺 host/port 那一形会被回 `invalid_args`。
        base = {"machine": {"host": "127.0.0.1", "port": port, "user": user, "label": "sr1b", "keyPath": f"{d}/client_key"}}
        be = Backend(bin_path, home)
        cmds = be.hello.get("commands", [])
        check("后端的 hello 声明了传输四条", all(c in cmds for c in ("transfer-upload", "transfer-download", "transfer-start", "transfer-stop")), cmds)

        print("⑧（起点）先开一条长流，之后全部经它那条连接")
        a0 = auths()
        be.open("s", {**base, "command": "cat", "use": "stream"})
        be.wait(lambda: be.lines("s") or None, 15)
        check("长流 ack ok", bool(be.lines("s")) and json.loads(be.lines("s")[0]).get("ok"), be.lines("s"))

        print("① files 链路")
        r = be.open("f", {**base, "use": "files"})
        be.wait(lambda: be.lines("f") or None, 15)
        ack = json.loads(be.lines("f")[0]) if be.lines("f") else {}
        check("link-open ok、ack ok、uses 含 files", bool(r and r["ok"]) and ack.get("ok") and "files" in (ack.get("uses") or []), (r, ack))
        fs = Files(be, "f")
        h = fs.ask({"op": "home"})
        check("home == sshd 给的起始目录", h.get("home") == rhome, (h, rhome))

        print("② 部署那几问")
        blob = os.urandom(3 * 1024 * 1024 + 17)
        bp = f"{rhome}/.cc-monitor/bin/ccm"
        st0 = fs.ask({"op": "stat", "path": bp})
        check("〔MIG-3b 续 · V41〕stat 那一问删了 ⇒ unknown_op", st0.get("code") == "unknown_op", st0)
        r = fs.ask({"op": "mkdirs", "path": f"{rhome}/.cc-monitor/bin"})
        check("mkdirs ok", r == {}, r)
        r = fs.ask({"op": "put", "path": bp, "size": len(blob), "mode": 0o700, "verify": True}, blob)
        check("put ＋ 读回：len 相等、first_diff=null", r == {"readback": {"len": len(blob), "first_diff": None}}, r)
        with open(bp, "rb") as fh:
            on_disk = fh.read()
        check("sshd 那侧盘上逐字节等于送去的（异源）", on_disk == blob, len(on_disk))
        check("权限 0700", (os.stat(bp).st_mode & 0o777) == 0o700, oct(os.stat(bp).st_mode))
        check("没留临时件 / .bak", sorted(os.listdir(f"{rhome}/.cc-monitor/bin")) == ["ccm"], os.listdir(f"{rhome}/.cc-monitor/bin"))
        fs.ask({"op": "put", "path": f"{rhome}/.cc-monitor/bin/.build_id", "size": 6, "mode": 0o600, "verify": False}, b"sr1b-x")
        rd = fs.ask({"op": "read", "path": f"{rhome}/.cc-monitor/bin/.build_id", "max": 65536})
        check("read 回 base64 原文", base64.b64decode(rd.get("data") or "") == b"sr1b-x", rd)
        big = fs.ask({"op": "read", "path": bp, "max": 1024})
        check("read 超过这一问的上限 ⇒ too_big", big.get("code") == "too_big", big)
        r1 = fs.ask({"op": "remove", "path": f"{rhome}/.cc-monitor/bin/.build_id"})
        r2 = fs.ask({"op": "remove", "path": f"{rhome}/.cc-monitor/bin/.build_id"})
        check("remove：第一次 true、第二次 false", (r1, r2) == ({"removed": True}, {"removed": False}), (r1, r2))

        print("③ 远端写围栏")
        os.symlink(outside, f"{rhome}/.cc-monitor/bin/esc")
        os.makedirs(f"{rhome}/.ssh")
        before, before_out = tree(rhome), tree(outside)
        tries = [
            ("put", f"{rhome}/.ssh/authorized_keys"),
            ("put", f"{rhome}/.cc-monitor/other/x"),
            ("put", f"{outside}/x"),
            ("put", f"{rhome}/.cc-monitor/bin/../../.bashrc"),
            ("put", f"{rhome}/.cc-monitor/bin/esc/x"),
            ("put", ".bashrc"),
            ("mkdirs", f"{rhome}/.cc-monitor/other"),
            ("remove", f"{rhome}/.ssh/authorized_keys"),
        ]
        codes = []
        for op, path in tries:
            req = {"op": op, "path": path}
            if op == "put":
                req.update({"size": 3, "mode": 0o600, "verify": False})
            codes.append(fs.ask(req, b"bad" if op == "put" else b"").get("code"))
        check(f"{len(tries)} 形越界全部 fenced", codes == ["fenced"] * len(tries), list(zip([t[1] for t in tries], codes)))
        check("rhome 整棵前后相等（零改动）", tree(rhome) == before, set(tree(rhome)) ^ set(before))
        check("链接逃逸的目标目录零改动", tree(outside) == before_out, tree(outside))
        os.unlink(f"{rhome}/.cc-monitor/bin/esc")

        print("④ 上传")
        up = os.path.join(d, "up.bin")
        with open(up, "wb") as fh:
            fh.write(os.urandom(5 * 1024 * 1024 + 3))
        r = be.call("transfer-upload", {"dial": base, "local_path": up})
        ok = bool(r and r["ok"])
        xid, key = (r["data"]["id"], r["data"]["key"]) if ok else (None, None)
        check("开单：回 id ＋ 32 位键", ok and len(key) == 32, r)
        r = be.call("transfer-start", {"id": xid})
        last = be.end_of(xid)
        frames = be.xfer.get(xid, [])
        gots = [f["got"] for f in frames]
        # 〔WF2 跟上〕FW1 之后 done 帧带整份摘要（`sha256`，提交那一步要它）⇒ 摘要也要等于本机那份。
        with open(up, "rb") as fh:
            up_sha = hashlib.sha256(fh.read()).hexdigest()
        check("终局 done、bytes == 本机大小、sha256 == 本机那份", bool(last) and last["end"] == {"state": "done", "bytes": os.path.getsize(up), "sha256": up_sha}, last)
        check("got 单调不减", gots == sorted(gots), gots[:20])
        part = f"{rhome}/.cc-monitor/staging/{key}.part"
        with open(up, "rb") as a, open(part, "rb") as b:
            check("暂存件逐字节等于本机那份", a.read() == b.read(), part)
        again = be.call("transfer-start", {"id": xid})
        check("收场之后票摘掉：再起跑 ⇒ no_such_transfer", bool(again) and again.get("code") == "no_such_transfer", again)

        print("⑤ 续传")
        up2 = os.path.join(d, "up2.bin")
        body = os.urandom(4 * 1024 * 1024 + 11)
        with open(up2, "wb") as fh:
            fh.write(body)
        r = be.call("transfer-upload", {"dial": base, "local_path": up2})
        xid2, key2 = r["data"]["id"], r["data"]["key"]
        have = 1536 * 1024
        part2 = f"{rhome}/.cc-monitor/staging/{key2}.part"
        with open(part2, "wb") as fh:
            fh.write(body[:have])
        be.call("transfer-start", {"id": xid2})
        last = be.end_of(xid2)
        w = written(f".cc-monitor/staging/{key2}.part") or written(part2)
        with open(part2, "rb") as fh:
            same = fh.read() == body
        check("续传收场 done、暂存件等于整份", bool(last) and last["end"].get("state") == "done" and same, last)
        check(f"sftp-server 记下的写入字节 == 总长 − 已有（{len(body)} − {have} = {len(body) - have}）", w == len(body) - have, w)

        print("⑥ 撤")
        big_up = os.path.join(d, "big.bin")
        with open(big_up, "wb") as fh:
            fh.truncate(256 * 1024 * 1024)
        r = be.call("transfer-upload", {"dial": base, "local_path": big_up})
        xid3, key3 = r["data"]["id"], r["data"]["key"]
        be.call("transfer-start", {"id": xid3})
        be.wait(lambda: any(f["got"] > 0 for f in be.xfer.get(xid3, [])) or None, 30)
        be.call("transfer-stop", {"id": xid3})
        last = be.end_of(xid3)
        check("上传撤 ⇒ cancelled", bool(last) and last["end"] == {"state": "cancelled"}, last)
        check("上传撤 ⇒ 暂存件删了", not os.path.exists(f"{rhome}/.cc-monitor/staging/{key3}.part"), os.listdir(f"{rhome}/.cc-monitor/staging"))
        rbig = f"{rhome}/big-remote.bin"
        with open(rbig, "wb") as fh:
            fh.truncate(256 * 1024 * 1024)
        dl_big = os.path.join(d, "dl", "big.bin")
        os.makedirs(os.path.dirname(dl_big))
        r = be.call("transfer-download", {"dial": base, "remote_path": rbig, "local_path": dl_big})
        xid4 = r["data"]["id"]
        be.call("transfer-start", {"id": xid4})
        be.wait(lambda: any(f["got"] > 0 for f in be.xfer.get(xid4, [])) or None, 30)
        be.call("transfer-stop", {"id": xid4})
        last = be.end_of(xid4)
        check("下载撤 ⇒ cancelled、.part 留着、落点不在", bool(last) and last["end"] == {"state": "cancelled"} and os.path.exists(dl_big + ".part") and not os.path.exists(dl_big),
              (last, os.listdir(os.path.dirname(dl_big))))
        os.unlink(rbig)

        print("⑦ 下载")
        rsrc = f"{rhome}/data.bin"
        with open(rsrc, "wb") as fh:
            fh.write(os.urandom(2 * 1024 * 1024 + 5))
        dl = os.path.join(d, "dl", "data.bin")
        r = be.call("transfer-download", {"dial": base, "remote_path": rsrc, "local_path": dl})
        xid5 = r["data"]["id"]
        be.call("transfer-start", {"id": xid5})
        last = be.end_of(xid5)
        with open(rsrc, "rb") as a:
            want = a.read()
        got = open(dl, "rb").read() if os.path.exists(dl) else b""
        check("下载 done、落地逐字节对、.part 不留", bool(last) and last["end"].get("state") == "done" and got == want and not os.path.exists(dl + ".part"), last)
        miss = os.path.join(d, "dl", "missing.bin")
        r = be.call("transfer-download", {"dial": base, "remote_path": f"{rhome}/no-such-file", "local_path": miss})
        xid6 = r["data"]["id"]
        be.call("transfer-start", {"id": xid6})
        last = be.end_of(xid6)
        check("远端不存在 ⇒ failed、.part 不留", bool(last) and last["end"].get("state") == "failed" and not os.path.exists(miss + ".part") and not os.path.exists(miss), last)
        sess = os.path.join(home, ".claude", "projects", "p", "x.jsonl")
        r = be.call("transfer-download", {"dial": base, "remote_path": rsrc, "local_path": sess})
        check("本机落点是会话文件 ⇒ 照样开单（V119：没有数据围栏）", bool(r) and r["ok"], r)

        # 〔NT1 · 2026-09-24〕长流在时传输分道（`dial/pool.rs`：主连接上有长流 ⇒ 传输另开一条批量连接，
        #   被主连接托着、之后的传输都复用它）⇒ 长流 ＋ files 一条、六趟传输一条：恰好两次鉴权、两条 TCP。
        #   分道本身的读数（下载期间长流回声 314 → 116 ms）住 `NT1-net-loopback.py` ①⑤。
        print("⑧ 两条连接（长流 ＋ files 一条 · 传输分道一条）")
        check("sshd 鉴权恰好两次（长流 ＋ files ＋ 六趟起跑的传输；传输那条只握一次手）", auths() - a0 == 2, auths() - a0)
        check("恰好两条 TCP", established(port) == 2, established(port))

        print("⑨ 子系统留口不开")
        r = be.open("sub", {**base, "use": "subsystem"})
        check("use=subsystem ⇒ unsupported_use", bool(r) and not r["ok"] and r.get("code") == "unsupported_use", r)

        if with_monitor:
            print("⑩ 界面那一侧（RemoteFs · 中继 → 真后端 → 真 sshd）")
            up_m = os.path.join(d, "up-monitor.bin")
            with open(up_m, "wb") as fh:
                fh.write(os.urandom(1024 * 1024 + 9))
            env = {**os.environ,
                   "SR1B_LOOPBACK": json.dumps({"host": "127.0.0.1", "port": port, "user": user, "key_path": f"{d}/client_key",
                                                "backend": bin_path, "home": home, "rhome": rhome, "up": up_m,
                                                "dl_remote": rsrc, "dl_local": os.path.join(d, "dl", "monitor.bin")})}
            r = subprocess.run(["cargo", "test", "--offline", "-p", "monitor", "--lib", "sr1b_loopback_deploy_and_transfer_through_the_resident_backend", "--", "--ignored", "--nocapture"],
                               cwd=os.path.join(ROOT, "src", "frontend", "shell"), env=env, capture_output=True, text=True, timeout=3000)
            check("部署判定三形 · 入口 · 卸载 · 围栏原话 · 上传 / 下载经中继 全经本机常驻后端", "SR1B-LOOPBACK-MONITOR ok" in r.stdout and "1 passed" in r.stdout,
                  (r.stdout[-1500:], r.stderr[-1500:]))
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
