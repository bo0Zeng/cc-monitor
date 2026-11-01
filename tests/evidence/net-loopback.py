#!/usr/bin/env python3
# ruff: noqa: E501
"""**SSH 压缩** —— 对着真 sshd 的现打（本机回环，零 root）。

跑法（仓根下）：
    python3 tests/evidence/net-loopback.py --compress

`--compress`：对着一台回环 sshd 跑 `dial_compress_tests::zr_…`（`#[ignore]`，强制压 / 强制不压各一趟），再读 sshd 日志核协商结果。

它起一台**临时的回环 sshd**（本用户身份、随机端口、临时 host key 与客户端钥匙、`UsePAM no`、`LogLevel DEBUG1`）。

覆盖：
  ⑥（`--compress`）强制压 vs 不压（真 sshd）：sshd 日志按先后 `none` · `zlib@openssh.com`。上游 russh 0.61 的 zlib 解压有缺陷
     （NT1：闸关着时压的那趟卡在第一条通道上、收不全）；后端链仓内补过的副本、闸开 ⇒ 压的那趟**收全、逐字节同、
     线上字节 < 不压那趟的一半**；上行那一半（客户端压、sshd 解）把一段会话 jsonl 样子的载荷喂给远端 `sha256sum`，摘要对、
     线上字节 < 一半（两个方向的压缩比由那一行 `NT1-COMPRESS` 读数给出）

原来那几段（① 分道 ② 预算满 ③ MaxSessions ④ 回环不压 ⑤ 弱网 ⑦ 断线续传 ⑧ 黑洞）经后端的 `link-*` 驱一条条长流与 capture，
随那一族一起删了：链路一删，它们量的是不存在的路。

退出码：0 = 全过 · 1 = 有一条不对 · 2 = 没给 `--compress` · 3 = 起不来 sshd（环境不满足，不是被测对象坏了）
"""

import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


class Sshd:
    def __init__(self, d, name, sshd):
        self.port = free_port()
        self.log = f"{d}/{name}.log"
        cfg = f"{d}/{name}_config"
        with open(cfg, "w") as f:
            f.write(
                f"Port {self.port}\nListenAddress 127.0.0.1\nHostKey {d}/host_key\n"
                f"AuthorizedKeysFile {d}/authorized_keys\nPidFile {d}/{name}.pid\nUsePAM no\n"
                "StrictModes no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\n"
                "PubkeyAuthentication yes\nLogLevel DEBUG1\n"
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


def main():
    if "--compress" not in sys.argv[1:]:
        print("用法：python3 tests/evidence/net-loopback.py --compress")
        return 2
    sshd = shutil.which("sshd") or "/usr/sbin/sshd"
    if not os.path.isfile(sshd):
        print(f"环境不满足：sshd {sshd}")
        return 3
    user = os.environ.get("USER") or os.getlogin()
    d = os.path.realpath(tempfile.mkdtemp(prefix="nt1."))
    fails, servers = [], []

    def check(name, cond, detail):
        print(("  ok   " if cond else "  FAIL ") + name + ("" if cond else f"  —— {detail}"))
        if not cond:
            fails.append(name)

    try:
        for k in ("host_key", "client_key"):
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", f"{d}/{k}"], check=True)
        shutil.copyfile(f"{d}/client_key.pub", f"{d}/authorized_keys")
        os.chmod(f"{d}/authorized_keys", 0o600)
        try:
            a = Sshd(d, "a", sshd)
        except RuntimeError as e:
            print(e)
            return 3
        servers.append(a)

        print("⑥ 压缩（强制，真 sshd）：dial_compress_tests::zr_…（#[ignore]）")
        env = {**os.environ, "NT1_COMPRESS": json.dumps({"host": "127.0.0.1", "port": a.port, "user": user, "key_path": f"{d}/client_key"})}
        r = subprocess.run(["cargo", "test", "--lib", "zr_real_sshd_negotiates_zlib_and_moves_fewer_bytes_when_forced", "--", "--ignored", "--nocapture"],
                           cwd=os.path.join(ROOT, "src", "backend"), env=env, capture_output=True, text=True, timeout=3000)
        line = next((ln for ln in r.stdout.splitlines() if ln.startswith("NT1-COMPRESS")), "")
        print(f"  read {line}")
        check("读数用例本身过（不压那趟收全；闸关着 ⇒ 压的那趟 30 s 内收不全 —— 上游 russh 0.61 解压缺陷；闸开着（上游 0.62.7 起修好）⇒ 收全、逐字节同、线上字节 < 一半）", "1 passed" in r.stdout, (r.stdout[-800:], r.stderr[-800:]))
        kv = dict(x.split("=", 1) for x in line.split()[1:] if "=" in x)
        if kv.get("wire_on", "0").isdigit() and int(kv.get("wire_on", "0")) > 0:
            print(f"  read 线上字节：不压 {int(kv['wire_off'])} · 压 {int(kv['wire_on'])} ⇒ {int(kv['wire_off']) / int(kv['wire_on']):.1f} 倍")
        if kv.get("up_on", "0").isdigit() and int(kv.get("up_on", "0")) > 0:
            print(f"  read 上行（会话 jsonl 样子的载荷 {int(kv['upload'])} 字节喂给远端 sha256sum，摘要两趟都对：{kv.get('up_off_ok')}/{kv.get('up_on_ok')}）："
                  f"不压 {int(kv['up_off'])} · 压 {int(kv['up_on'])} ⇒ {int(kv['up_off']) / int(kv['up_on']):.1f} 倍")
        kex = [ln.split("compression:")[1].split()[0] for ln in a.text().splitlines() if "kex: client->server" in ln and "compression:" in ln]
        check("sshd 那一侧协商结果：不压那趟 none、压的那趟 zlib@openssh.com（按先后）", kex == ["none", "zlib@openssh.com"], kex)
    finally:
        for s in servers:
            s.p.terminate()
        shutil.rmtree(d, ignore_errors=True)

    print(f"\n{'全过' if not fails else '有不对的：' + ', '.join(fails)}")
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
