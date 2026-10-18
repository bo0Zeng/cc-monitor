#!/usr/bin/env python3
# ruff: noqa: E501
"""SR1b：**SFTP 住本机常驻后端**（传输四条 ＋ 部署 `deploy-*`）—— 对着一台真 sshd 的现打。

跑法（仓根下，先 `cd src/backend && cargo build --release`）：
    python3 tests/evidence/SR1b-sftp-loopback.py [--monitor] [后端二进制路径]
后端二进制缺省用 release 构建：`--monitor` 那一半要把这份字节真部署到回环那一头（还要传一份暂存件去卸），
debug 构建约 500 MB，传暂存件那一步实测撞上期限（`deploy-uninstall` 回 `io_failed` · `Timeout`）。

`--monitor`：后端那几项之后再跑一趟**界面那一侧**（`sftp_tests::sr1b_loopback_deploy_and_transfer_through_the_resident_backend`，
平时 `#[ignore]`）—— 起一个真后端、用生产那个本机吸收点接上它，装 · 卸经 `deploy-install` · `deploy-uninstall`、传输经中继。

它起一台**临时的回环 sshd**（本用户身份、随机端口、临时 host key 与客户端钥匙、`UsePAM no`、`LogLevel VERBOSE`），
🔴 **远端那一侧的家整个换成一个临时目录 `rhome`**：sftp 子系统的起始目录钉在它（`sftp-server -d <rhome>`，日志 `-e` 接进临时文件），
exec 那一侧的 `HOME` 也设成它（`SetEnv HOME=<rhome>`；上传开单探同名 · 部署扫身份都经 shell 跑 `~/.cc-monitor/bin/ccm`）
⇒ 后端眼里的远端 home 是 `rhome`，读写不到这台机器上真的 `~/.cc-monitor/`；开跑前经一次 ssh 自证 `$HOME` 换过来了，没换过来 ⇒ 退 3。再起一个**流模式**的后端（stdio 载体；私有 HOME / TMUX_TMPDIR、摘掉 TMUX）。
**不进门禁**（门禁的沙箱里没有 sshd 可起）—— 它是一份读数，不是判据；交付报告里贴的是它的输出。

覆盖（每条都是「真 TCP ＋ 真 SSH 握手 ＋ 真鉴权 ＋ 真 sftp 子系统」）：
  ① 链路：ack 的 uses 里没有文件那一种用法（SFTP 只经帧命令用）；use=files ⇒ bad_args
  ② 落点上摆一份无身份戳的字节（给 ⑩ 用：部署要显式失败、不覆盖它）
  ③ 远端写围栏只在合成服务端上判（`dial_sftp_tests`）：这里的上传键由后端起，够不着暂存区外
  ④ 上传：只写 `~/.cc-monitor/staging/<key>.part`，逐字节等于本机那份；帧序 got 单调、最后一帧 done
  ⑤ 续传：暂存件先放前 N 字节 ⇒ sftp-server 记下的这一趟写入字节数 == 总长 − N
  ⑥ 撤：上传撤 ⇒ 暂存件删；下载撤 ⇒ `.part` 留
  ⑦ 下载：落地逐字节对、`.part` 不留；远端不存在 ⇒ failed、`.part` 不留；本机落点是会话文件 ⇒ 照样开单（没有数据围栏）
  ⑧ 长流一条、全部传输分道一条：sshd **恰好两次**鉴权、**恰好两条** TCP
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
DEFAULT_BIN = os.path.join(ROOT, ".build", "backend", "release", "cc-monitor-backend")


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def established(port):
    out = subprocess.run(["ss", "-Htn", "state", "established", f"( dport = :{port} )"], capture_output=True, text=True).stdout
    return len([ln for ln in out.splitlines() if ln.strip()])


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


def main():
    args = [a for a in sys.argv[1:] if a != "--monitor"]
    with_monitor = "--monitor" in sys.argv[1:]
    bin_path = os.path.abspath(args[0]) if args else DEFAULT_BIN  # 界面那一半在另一个目录里起它
    sshd = shutil.which("sshd") or "/usr/sbin/sshd"
    sftp_server = next((p for p in ("/usr/lib/openssh/sftp-server", "/usr/libexec/openssh/sftp-server", "/usr/libexec/sftp-server") if os.path.isfile(p)), "")
    if not os.path.isfile(bin_path) or not os.path.isfile(sshd) or not sftp_server or not shutil.which("ss"):
        print(f"环境不满足：后端 {bin_path} / sshd {sshd} / sftp-server {sftp_server or '找不到'} / ss")
        return 3
    user = os.environ.get("USER") or os.getlogin()
    d = os.path.realpath(tempfile.mkdtemp(prefix="sr1b."))
    home = os.path.join(d, "home")  # 后端自己的 HOME（本机那一侧）
    rhome = os.path.join(d, "rhome")  # sshd 给 sftp 的起始目录（远端那一侧）
    for p in (home, rhome, os.path.join(home, ".claude", "projects", "p"), os.path.join(home, ".claude-alt")):
        os.makedirs(p)
    with open(os.path.join(home, ".claude-alt", "accounts.json"), "w") as fh:
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
                f"SetEnv HOME={rhome}\n"
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
        # 台架自证：exec 那一侧的家是 rhome（不是就会经 shell 跑到真家目录里那份 `~/.cc-monitor/bin/ccm`）。
        said = subprocess.run(["ssh", "-F", "/dev/null", "-i", f"{d}/client_key", "-p", str(port), "-o", "BatchMode=yes", "-o", "IdentitiesOnly=yes",
                               "-o", f"UserKnownHostsFile={d}/known_hosts", "-o", "StrictHostKeyChecking=accept-new", f"{user}@127.0.0.1", 'printf %s "$HOME"'],
                              capture_output=True, text=True, timeout=30).stdout
        if said != rhome:
            print(f"远端 shell 的 HOME 不是临时目录（要 {rhome}，是 {said!r}）—— 不跑")
            return 3

        def slog_text():
            with open(slog, errors="replace") as fh:
                return fh.read()

        def auths():
            return slog_text().count("Accepted publickey")

        def upload(local):
            """开一单上传（一组一项，落点目录 = rhome）⇒ (id, 暂存件的键)；没开成 ⇒ (None, None) 与那一份回包。"""
            r = be.call("transfer-upload", {"dial": base, "items": [{"local_path": local}], "remote_dir": rhome, "home": rhome})
            it = (r or {}).get("data", {}).get("items", [{}])[0] if r and r.get("ok") else {}
            return (it.get("id"), it.get("staged"), r) if it.get("state") == "opened" else (None, None, r)

        def download(remote, local):
            """开一单下载（落点目录 = `local` 那一层、落名 = 它的名字）⇒ (id, 回包)。"""
            r = be.call("transfer-download", {"dial": base, "items": [{"remote_path": remote, "name": os.path.basename(local)}], "local_dir": os.path.dirname(local)})
            it = (r or {}).get("data", {}).get("items", [{}])[0] if r and r.get("ok") else {}
            return (it.get("id") if it.get("state") == "opened" else None), r

        def written(path):
            """sftp-server 记下的、关掉 `path` 那一次的写入字节数（最后一次）。它的日志经 `-e` 打到 stderr，再由子系统那行命令接进 `sftp.log`。"""
            n = None
            with open(f"{d}/sftp.log", errors="replace") as fh:
                log = fh.read()
            for ln in log.splitlines():
                if f'close "{path}"' in ln and " written " in ln:
                    n = int(ln.rsplit(" written ", 1)[1].split()[0])
            return n

        # 〔WIN3 §2〕拨号只收界面那一格原样的配置（MIG-1 之后 `ssh-dial/machine.rs::resolve`）；平铺 host/port 那一形会被回 `invalid_args`。
        base = {"machine": {"host": "127.0.0.1", "port": port, "user": user, "label": "sr1b", "keyPath": f"{d}/client_key"}}
        be = Backend(bin_path, home)
        cmds = be.hello.get("commands", [])
        check("后端的 hello 声明了传输四条", all(c in cmds for c in ("transfer-upload", "transfer-download", "transfer-start", "transfer-stop")), cmds)

        print("⑧（起点）先开一条长流，之后全部经它那条连接")
        a0 = auths()
        be.open("s", {**base, "command": "cat", "use": "stream"})
        be.wait(lambda: be.lines("s") or None, 15)
        check("长流 ack ok", bool(be.lines("s")) and json.loads(be.lines("s")[0]).get("ok"), be.lines("s"))

        print("① 链路上没有文件那一种用法")
        ack = json.loads(be.lines("s")[0]) if be.lines("s") else {}
        check("ack 的 uses == stream · capture · forward · shell · hold", ack.get("uses") == ["stream", "capture", "forward", "shell", "hold"], ack)
        r = be.open("f", {**base, "use": "files"})
        check("use=files ⇒ bad_args（拨号核心不认这种用法）", bool(r) and not r["ok"] and r.get("code") == "bad_args", r)

        print("② 落点上摆一份无身份戳的字节（⑩ 用）")
        os.makedirs(f"{rhome}/.cc-monitor/bin")
        with open(f"{rhome}/.cc-monitor/bin/ccm", "wb") as fh:
            fh.write(os.urandom(3 * 1024 * 1024 + 17))

        print("④ 上传")
        up = os.path.join(d, "up.bin")
        with open(up, "wb") as fh:
            fh.write(os.urandom(5 * 1024 * 1024 + 3))
        xid, key, r = upload(up)
        check("开单：回 id ＋ 32 位暂存键", bool(xid) and len(key or "") == 32, r)
        r = be.call("transfer-start", {"id": xid})
        last = be.end_of(xid)
        frames = be.xfer.get(xid, [])
        gots = [f["got"] for f in frames]
        # FW1 之后 done 帧带整份摘要（`sha256`，提交那一步要它）⇒ 摘要也要等于本机那份。
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
        xid2, key2, _ = upload(up2)
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
        xid3, key3, _ = upload(big_up)
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
        xid4, _ = download(rbig, dl_big)
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
        xid5, _ = download(rsrc, dl)
        be.call("transfer-start", {"id": xid5})
        last = be.end_of(xid5)
        with open(rsrc, "rb") as a:
            want = a.read()
        got = open(dl, "rb").read() if os.path.exists(dl) else b""
        check("下载 done、落地逐字节对、.part 不留", bool(last) and last["end"].get("state") == "done" and got == want and not os.path.exists(dl + ".part"), last)
        miss = os.path.join(d, "dl", "missing.bin")
        xid6, _ = download(f"{rhome}/no-such-file", miss)
        be.call("transfer-start", {"id": xid6})
        last = be.end_of(xid6)
        check("远端不存在 ⇒ failed、.part 不留", bool(last) and last["end"].get("state") == "failed" and not os.path.exists(miss + ".part") and not os.path.exists(miss), last)
        sess = os.path.join(home, ".claude", "projects", "p", "x.jsonl")
        xid7, r = download(rsrc, sess)
        check("本机落点是会话文件 ⇒ 照样开单（没有数据围栏）", bool(xid7), r)

        # 长流在时传输分道（`ssh-dial/pool.rs`：主连接上有长流 ⇒ 传输另开一条批量连接，
        #   被主连接托着、之后的传输都复用它）⇒ 长流一条、六趟传输一条：恰好两次鉴权、两条 TCP。
        #   分道本身的读数（下载期间长流回声 314 → 116 ms）住 `NT1-net-loopback.py` ①⑤。
        print("⑧ 两条连接（长流一条 · 传输分道一条）")
        check("sshd 鉴权恰好两次（长流 ＋ 六趟起跑的传输；传输那条只握一次手）", auths() - a0 == 2, auths() - a0)
        check("恰好两条 TCP", established(port) == 2, established(port))

        print("⑨ 子系统留口不开")
        r = be.open("sub", {**base, "use": "subsystem"})
        check("use=subsystem ⇒ unsupported_use", bool(r) and not r["ok"] and r.get("code") == "unsupported_use", r)

        if with_monitor:
            print("⑩ 界面那一侧（deploy-* · 中继 → 真后端 → 真 sshd）")
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
