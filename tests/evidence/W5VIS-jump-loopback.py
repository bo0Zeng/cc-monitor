#!/usr/bin/env python3
# ruff: noqa: E501
"""W5-VIS：**跳板连接的「替换」语义** —— 对着真 sshd 的现打（本机回环，零 root）。

要求住址：`设计/15 §3.2` 复用硬约束表第 5 条（逐字）「跳板连接的「替换」语义依赖通道先死 | 未核（跳板复用零读数；W5-VIS）」·
同表第 1、2 条「驱逐 ≠ 关掉」「在飞的通道持有发送端副本 ⇒ 「驱逐了」与「关掉了」是两件事，账要分开记」。

跑法（仓根下，先 `cd src/backend && cargo build`）：
    python3 tests/evidence/W5VIS-jump-loopback.py [后端二进制路径]

台架（借 `NT1-net-loopback.py` 的 `Sshd` · `Backend` · `ShapedProxy`，不抄第二份）：两台临时回环 sshd —— 甲台当**跳板**、乙台当**目标**；
后端经跳板拨目标（目标那一跳是甲台 sshd 替我们开的 direct-tcpip ⇒ 乙台看到的 TCP 来自甲台）。跳板那一跳前面挡一个用户态代理，
好把「此刻已有的 TCP」变黑洞（没有 RST，看着还活着）—— 那是池子**摘掉**一条连接的唯一现成触发（`pool::Watch`：等开通道被打断）。

数的是**后端这一侧**到代理口的已建立 TCP（`ss dport = :<代理口>`）= 后端手里还攥着几条**跳板**连接。

  ① 经跳板起一条长流 ⇒ 跳板 TCP == 1、甲 / 乙各鉴权 1 次；同一身份再来一条 capture ⇒ 复用（鉴权不变）
  ② 黑洞：冻住此刻的跳板 TCP，新的一条 capture 卡在开通道上、被关掉 ⇒ 池子把那条（目标 ＋ 它的跳板）摘掉；
     再起一条长流 ⇒ **拨新的**（甲 / 乙各多鉴权 1 次）⇒ 跳板 TCP == 2（旧的还被旧长流攥着 —— 摘掉 ≠ 关掉）
  ③ ★ 关掉旧长流（旧连接上最后一个用户）⇒ 旧跳板**真的关了**：跳板 TCP 回到 1（替换语义：旧跳板活到它上面的通道全死、然后就死）
  ④ 关掉新长流 ⇒ 跳板 TCP == 0（没有定时器，按事件收）

退出码：0 = 全过 · 1 = 有一条不对 · 3 = 起不来 sshd / 找不到二进制（环境不满足，不是被测对象坏了）
"""

import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("nt1_rig", os.path.join(HERE, "NT1-net-loopback.py"))
rig = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rig)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    bin_path = args[0] if args else rig.DEFAULT_BIN
    sshd = shutil.which("sshd") or "/usr/sbin/sshd"
    sftp_server = next((p for p in ("/usr/lib/openssh/sftp-server", "/usr/libexec/openssh/sftp-server", "/usr/libexec/sftp-server") if os.path.isfile(p)), "")
    if not os.path.isfile(bin_path) or not os.path.isfile(sshd) or not sftp_server or not shutil.which("ss"):
        print(f"环境不满足：后端 {bin_path} / sshd {sshd} / sftp-server {sftp_server or '找不到'} / ss")
        return 3
    user = os.environ.get("USER") or os.getlogin()
    d = os.path.realpath(tempfile.mkdtemp(prefix="w5vis-jump."))
    home, rhome = os.path.join(d, "home"), os.path.join(d, "rhome")
    for p in (home, rhome, os.path.join(home, ".claude-alt")):
        os.makedirs(p)
    with open(os.path.join(home, ".claude-alt", "accounts.json"), "w") as fh:
        fh.write('{"version":1,"accounts":[]}\n')
    fails, servers, be = [], [], None

    def check(name, cond, detail):
        print(("  ok   " if cond else "  FAIL ") + name + ("" if cond else f"  —— {detail}"))
        if not cond:
            fails.append(name)

    try:
        for k in ("host_key", "client_key"):
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", f"{d}/{k}"], check=True)
        shutil.copyfile(f"{d}/client_key.pub", f"{d}/authorized_keys")
        os.chmod(f"{d}/authorized_keys", 0o600)
        jump = rig.Sshd(d, "jump", 10, sshd, sftp_server, rhome)
        target = rig.Sshd(d, "target", 10, sshd, sftp_server, rhome)
        servers += [jump, target]
        px = rig.ShapedProxy(jump.port)
        key = f"{d}/client_key"

        def dial(**kw):
            return {
                "host": "127.0.0.1",
                "port": target.port,
                "user": user,
                "key_path": key,
                "host_key_fingerprint": None,
                "jump": {"host": "127.0.0.1", "port": px.port, "user": user, "key_path": key, "host_key_fingerprint": None, "label": "跳板"},
                **kw,
            }

        def jumps():
            return rig.established(px.port)

        be = rig.Backend(bin_path, home)

        print("① 经跳板起一条长流")
        j0, t0 = jump.auths(), target.auths()
        be.open("s1", dial(command="cat", use="stream"))
        be.wait(lambda: be.lines("s1") or None, 30)
        first = (jumps(), jump.auths() - j0, target.auths() - t0)
        check("跳板 TCP == 1、甲（跳板）鉴权 1 次、乙（目标）鉴权 1 次", first == (1, 1, 1), first)
        be.open("c1", dial(command="echo reuse", use="capture", capture={"max_bytes": 100}))
        be.wait(lambda: "c1" in be.ends or None, 30)
        reuse = (be.lines("c1")[1:] if len(be.lines("c1")) == 2 else be.lines("c1"), jump.auths() - j0, target.auths() - t0, jumps())
        check("同一身份的 capture 复用那一条（鉴权不变、跳板 TCP 仍 1）", reuse[1:] == (1, 1, 1) and "reuse" in str(reuse[0]), reuse)

        print("② 黑洞：冻住此刻的跳板 TCP ⇒ 卡在开通道上的那一条被关掉 ⇒ 池子摘掉那条连接")
        px.freeze()
        be.open("c2", dial(command="echo stuck", use="capture", capture={"max_bytes": 100}))
        stuck = not be.wait(lambda: "c2" in be.ends or None, 3)
        be.call("link-close", {"link": "c2"})  # 模拟界面的握手期限到点
        be.open("s2", dial(command="cat", use="stream"))
        fresh = be.wait(lambda: be.lines("s2") or None, 30)
        second = (stuck, bool(fresh), jump.auths() - j0, target.auths() - t0, jumps())
        check("第一条被黑洞吞掉；再起一条长流 ⇒ 拨新的（甲 / 乙各多鉴权 1 次）⇒ 跳板 TCP == 2（旧的还被旧长流攥着：摘掉 ≠ 关掉）",
              second == (True, True, 2, 2, 2), second)

        print("③ ★ 关掉旧长流（旧连接上最后一个用户）")
        be.call("link-close", {"link": "s1"})
        back = be.wait(lambda: jumps() == 1 or None, 20)
        check("旧跳板真的关了：跳板 TCP 回到 1（旧跳板活到它上面的通道全死，然后就死）", bool(back), jumps())

        print("④ 关掉新长流")
        be.call("link-close", {"link": "s2"})
        zero = be.wait(lambda: jumps() == 0 or None, 20)
        check("跳板 TCP == 0（按事件收，没有定时器）", bool(zero), jumps())
    finally:
        if be:
            be.close()
        for s in servers:
            s.p.terminate()
            try:
                s.p.wait(timeout=10)
            except subprocess.TimeoutExpired:
                s.p.kill()
        shutil.rmtree(d, ignore_errors=True)
    print("全过" if not fails else f"FAIL {len(fails)} 条：{fails}")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
