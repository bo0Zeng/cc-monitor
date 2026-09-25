#!/usr/bin/env python3
# ruff: noqa: E501
"""C2：**拨号代理（`<后端> --dial`）对着一台真 sshd 的现打**（`设计/05 §13`）。

〔SR1a · 2026-09-24〕🔴 **本脚本只对 C2 那一版的后端有效**（BUILD_ID `p2t-commit-upload-dial-v2` 及之前）：
`--dial` 子命令在 SR1a 删了（拨号挪进本机那一个常驻后端，经流上的 `link-*` 链路做），
下面 `--monitor` 那一趟点名的界面侧判据也随之改名〔散文墓碑〕。**今天的读数脚本是 `tests/evidence/SR1a-link-loopback.py`**；
本文件留着是因为 `IPC-PROTOCOL.md` §10 的 C2 段与 `设计/05 §13.10` 的读数出自它（那是当时的读数，不改）。

跑法（仓根下，先 `cd src/backend && cargo build`）：
    python3 tests/evidence/C2-dial-loopback.py [--monitor] [后端二进制路径]

`--monitor`：八项之后再跑一趟**界面那一侧**（`dial_host_tests::loopback_roundtrip_through_the_proxy`，
平时 `#[ignore]`）—— 宿主起真代理、成员读真应答，界面进程里零 russh。

它起一台**临时的回环 sshd**（本用户身份、随机端口、临时 host key 与客户端钥匙、`UsePAM no`），
对拨号代理逐条下请求，核它回的每一行。**不进门禁**（门禁的沙箱里没有 sshd 可起）——
它是一份读数，不是判据；交付报告里贴的是它的输出。

覆盖（每条都是「真 TCP ＋ 真 SSH 握手 ＋ 真鉴权」）：
  ① capture：stdout / stderr / 退出码三样收全（退出码 7 原样回来）
  ② 竞速：一个死端口排在前面，活的那个胜出；阶段行六种按到达顺序出、kind 是 camelCase
  ③ 严格指纹：给错指纹 ⇒ 拒绝、ack 里带上实得指纹
  ④ stream：远端输出原样回来 · 界面走了（stdin EOF）代理就收工，哪怕远端那头还活着（`D3③`）
  ⑤ 跳板：经跳板开隧道、在隧道上握手（跳板与目标是同一台，形状是真的）
  ⑥ ssh-agent（Unix）：不给私钥路径，走 SSH_AUTH_SOCK
  ⑦ 没有 agent 也没给私钥 ⇒ 明说，不回落
  ⑧ forward：本地口经隧道到一台回环 HTTP；每接一条连接一行 {"accepted":n}；stdin EOF 就收工

退出码：0 = 全过 · 1 = 有一条不对 · 3 = 起不来 sshd / 找不到二进制（环境不满足，不是被测对象坏了）
"""

import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
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


def dial(bin_path, req, stdin=b"", env_extra=None, env_drop=()):
    env = dict(os.environ)
    for k in env_drop:
        env.pop(k, None)
    env.update(env_extra or {})
    env["CCM_DIAL_REQUEST"] = json.dumps(req)
    r = subprocess.run([bin_path, "--dial"], input=stdin, capture_output=True, env=env, timeout=30)
    return r.returncode, [ln for ln in r.stdout.decode("utf-8", "replace").split("\n") if ln]


def main():
    args = [a for a in sys.argv[1:] if a != "--monitor"]
    with_monitor = "--monitor" in sys.argv[1:]
    bin_path = args[0] if args else DEFAULT_BIN
    sshd = shutil.which("sshd") or "/usr/sbin/sshd"
    if not os.path.isfile(bin_path) or not os.path.isfile(sshd):
        print(f"环境不满足：后端 {bin_path} / sshd {sshd}")
        return 3
    user = os.environ.get("USER") or os.getlogin()
    d = tempfile.mkdtemp(prefix="c2dial.")
    procs = []
    fails = []

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
        with open(f"{d}/sshd_config", "w") as f:
            f.write(
                f"Port {port}\nListenAddress 127.0.0.1\nHostKey {d}/host_key\n"
                f"AuthorizedKeysFile {d}/authorized_keys\nPidFile {d}/sshd.pid\nUsePAM no\n"
                "StrictModes no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\n"
                "PubkeyAuthentication yes\nAllowTcpForwarding yes\n"
            )
        procs.append(subprocess.Popen([sshd, "-D", "-e", "-f", f"{d}/sshd_config"], stderr=subprocess.DEVNULL))
        for _ in range(50):
            try:
                socket.create_connection(("127.0.0.1", port), timeout=0.2).close()
                break
            except OSError:
                time.sleep(0.1)
        else:
            print("sshd 起不来")
            return 3
        fp = subprocess.run(["ssh-keygen", "-l", "-E", "sha256", "-f", f"{d}/host_key.pub"], capture_output=True, text=True).stdout.split()[1]
        base = {"host": "127.0.0.1", "port": port, "user": user, "key_path": f"{d}/client_key", "host_key_fingerprint": None}

        print("① capture")
        rc, out = dial(bin_path, {**base, "command": "echo out; echo err >&2; exit 7", "use": "capture", "capture": {"max_bytes": 1000}})
        ack, res = json.loads(out[0]), json.loads(out[1])
        check("ack ok · v=2 · uses", ack["ok"] and ack["v"] == 2 and ack["uses"] == ["stream", "capture", "forward"], out[0])
        check("stdout/stderr/退出码", (res["stdout"], res["stderr"], res["exit_status"]) == ("out\n", "err\n", 7), out[1])

        print("② 竞速 ＋ 阶段行")
        dead = free_port()
        rc, out = dial(bin_path, {**base, "command": "true", "use": "capture", "capture": {"max_bytes": 10}, "stages": True,
                                  "endpoints": [{"host": "127.0.0.1", "port": dead}, {"host": "127.0.0.1", "port": port}]})
        rows = [json.loads(x) for x in out]
        kinds = [r["stage"]["kind"] for r in rows if "stage" in r]
        ack = next(r for r in rows if "ok" in r)
        check("活的那个胜出", ack["ok"] and ack["endpoint"] == f"127.0.0.1:{port}", ack)
        check("六种阶段都出、死端口报 failed", set(kinds) == {"dialing", "failed", "hostKey", "won", "auth", "established"} and kinds[-1] == "established", kinds)
        check("指纹就是 host key 的 SHA256", ack["fingerprint"] == fp, (ack["fingerprint"], fp))

        print("③ 严格指纹")
        rc, out = dial(bin_path, {**base, "host_key_fingerprint": "SHA256:bogus", "command": "true"})
        ack = json.loads(out[0])
        check("错指纹被拒、退出 3、带实得指纹", (not ack["ok"]) and rc == 3 and ack["fingerprint"] == fp, (rc, out))

        print("④ stream")
        # 界面的写半边一直开着（真实用法：一次性查询从不关它），只看下行
        env = {**os.environ, "CCM_DIAL_REQUEST": json.dumps({**base, "host_key_fingerprint": fp, "command": "echo hello; echo world"})}
        pr = subprocess.Popen([bin_path, "--dial"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env)
        out = [ln for ln in pr.stdout.read().decode().split("\n") if ln]
        rc = pr.wait(timeout=10)
        pr.stdin.close()
        check("远端的输出原样回来、远端退了代理就收工（写半边一直开着）", out[1:] == ["hello", "world"] and rc == 0, (rc, out))
        t0 = time.time()
        rc, out = dial(bin_path, {**base, "host_key_fingerprint": fp, "command": "cat"}, stdin=b"")
        check("界面走了（stdin EOF）代理就收工 —— 哪怕远端那头还活着", rc == 0 and time.time() - t0 < 10, (rc, out, time.time() - t0))

        print("⑤ 跳板")
        rc, out = dial(bin_path, {**base, "command": "echo via-jump", "use": "capture", "capture": {"max_bytes": 100}, "stages": True,
                                  "jump": {**base, "label": "J"}})
        rows = [json.loads(x) for x in out]
        res = rows[-1]
        check("经跳板跑通", res.get("stdout") == "via-jump\n", rows)
        check("阶段里有「跳板 J」与「（经跳板）」", any(r.get("stage", {}).get("endpoint") == "跳板 J" for r in rows)
              and any("经跳板" in r.get("stage", {}).get("endpoint", "") for r in rows), rows)

        print("⑥ ssh-agent（Unix）")
        ag = tempfile.mkdtemp(prefix="c2ag.", dir="/tmp")  # 套接字路径有长度上限，scratchpad 太深
        sock = f"{ag}/s"
        agent = subprocess.Popen(["ssh-agent", "-D", "-a", sock], stdout=subprocess.DEVNULL)
        procs.append(agent)
        for _ in range(50):
            if os.path.exists(sock):
                break
            time.sleep(0.1)
        subprocess.run(["ssh-add", "-q", f"{d}/client_key"], env={**os.environ, "SSH_AUTH_SOCK": sock}, check=True, stderr=subprocess.DEVNULL)
        rc, out = dial(bin_path, {**base, "key_path": None, "command": "echo via-agent", "use": "capture", "capture": {"max_bytes": 100}},
                       env_extra={"SSH_AUTH_SOCK": sock})
        check("不给私钥路径、走 agent", len(out) == 2 and json.loads(out[1])["stdout"] == "via-agent\n", out)

        print("⑦ 没 agent 也没私钥")
        rc, out = dial(bin_path, {**base, "key_path": None, "command": "true"}, env_drop=("SSH_AUTH_SOCK",))
        ack = json.loads(out[0])
        check("明说 agent 连不上、不回落", (not ack["ok"]) and "ssh-agent" in ack["error"] and rc == 3, out)

        print("⑧ forward")
        web, lp = free_port(), free_port()
        procs.append(subprocess.Popen([sys.executable, "-m", "http.server", str(web), "--bind", "127.0.0.1", "--directory", d],
                                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        time.sleep(0.5)
        env = {**os.environ, "CCM_DIAL_REQUEST": json.dumps({**base, "use": "forward", "forward": {"local_port": lp, "remote_host": "127.0.0.1", "remote_port": web}})}
        fw = subprocess.Popen([bin_path, "--dial"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env)
        first = json.loads(fw.stdout.readline())
        codes = [urllib.request.urlopen(f"http://127.0.0.1:{lp}/sshd_config", timeout=5).status for _ in range(2)]
        fw.stdin.close()
        rest = [json.loads(x) for x in fw.stdout.read().decode().split("\n") if x]
        fw.wait(timeout=10)
        check("ack 之后接两条连接、各报一行", first["ok"] and codes == [200, 200] and rest == [{"accepted": 1}, {"accepted": 2}], (first, codes, rest))
        check("stdin EOF 就收工、本地口释放", fw.returncode == 0 and socket.socket().connect_ex(("127.0.0.1", lp)) != 0, fw.returncode)

        if with_monitor:
            print("⑨ 界面那一侧（dial_host → 真代理 → 真 sshd）")
            env = {**os.environ, "CCM_DIAL_PROXY": bin_path,
                   "C2_LOOPBACK": json.dumps({"host": "127.0.0.1", "port": port, "user": user, "key_path": f"{d}/client_key", "proxy": bin_path})}
            r = subprocess.run(["cargo", "test", "-p", "monitor", "--lib", "loopback_roundtrip_through_the_proxy", "--", "--ignored", "--nocapture"],
                               cwd=os.path.join(ROOT, "src", "bridge"), env=env, capture_output=True, text=True, timeout=1200)
            check("字节流 · 收全 · 阶段 ＋ 指纹 全经宿主", "C2-LOOPBACK-MONITOR ok" in r.stdout and "1 passed" in r.stdout,
                  (r.stdout[-800:], r.stderr[-800:]))
    finally:
        for p in procs:
            p.terminate()
        shutil.rmtree(d, ignore_errors=True)

    print(f"\n{'全过' if not fails else '有不对的：' + ', '.join(fails)}")
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
