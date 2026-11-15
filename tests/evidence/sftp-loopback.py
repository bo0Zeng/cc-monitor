#!/usr/bin/env python3
# ruff: noqa: E501
"""**SFTP 住本机常驻后端**（部署 `deploy-*` ＋ 两台之间上下传 `transfer-copy`）—— 对着一台真 sshd 的现打。

跑法（仓根下，先 `cd src/backend && cargo build --release`）：
    python3 tests/evidence/sftp-loopback.py [后端二进制路径]
后端二进制缺省用 release 构建：要把这份字节真部署到回环那一头（还要传一份暂存件去卸），
debug 构建约 500 MB，传暂存件那一步实测撞上期限（`deploy-uninstall` 回 `io_failed` · `Timeout`）。

它起一台**临时的回环 sshd**（本用户身份、随机端口、临时 host key 与客户端钥匙、`UsePAM no`、`LogLevel VERBOSE`），
🔴 **远端那一侧的家整个换成一个临时目录 `rhome`**：sftp 子系统的起始目录钉在它（`sftp-server -d <rhome>`，日志 `-e` 接进临时文件），
exec 那一侧的 `HOME` 也设成它（`SetEnv HOME=<rhome>`；部署扫身份 · 那台后端都经 shell 跑 `~/.cc-monitor/bin/ccm`）
⇒ 后端眼里的远端 home 是 `rhome`，读写不到这台机器上真的 `~/.cc-monitor/`；开跑前经一次 ssh 自证 `$HOME` 换过来了，没换过来 ⇒ 退 3。
然后跑**界面那一侧**（`deploy_tests::sr1b_loopback_deploy_and_transfer_through_the_resident_backend`，平时 `#[ignore]`）——
起一个真后端、用生产那个本机吸收点接上它，装 · 卸经 `deploy-install` · `deploy-uninstall`、上传 / 下载各一条寻址本机的 `transfer-copy`。
**不进门禁**（门禁的沙箱里没有 sshd 可起）—— 它是一份读数，不是判据；交付报告里贴的是它的输出。

覆盖（每条都是「真 TCP ＋ 真 SSH 握手 ＋ 真鉴权 ＋ 真 sftp 子系统」）：
  ② 落点上摆一份无身份戳的字节（部署要显式失败、不覆盖它）
  ⑩ 部署判定 · 入口 · 卸载 · 围栏原话 · 上传 / 下载各一条 `transfer-copy`、落地逐字节对

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


ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEFAULT_BIN = os.path.join(ROOT, ".build", "backend", "release", "cc-monitor-backend")
SHIM = os.path.join(ROOT, "tests", "e2e", "tmux-shim.sh")
# 界面那一侧起真后端照 `C7i` 走 shim：先装本趟私有的那一份（装不上 tmux-shim.sh 自己退 2），目录经 `CCM_E2E_TMUX_SHIM_BIN`
# 交给判据（拿不到就停），后端裸调的 tmux 被强插 `-L`；收尾只收本趟那一台私有 tmux。
WITH_SHIM = ["bash", "-c", '. "$1" e2eSr1b || exit 2; trap tmux_shim_cleanup EXIT; shift; CCM_E2E_TMUX_SHIM_BIN="$TMUX_SHIM_BIN" "$@"', "_", SHIM]


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def main():
    bin_path = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_BIN  # 界面那一半在另一个目录里起它
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

        print("② 落点上摆一份无身份戳的字节")
        os.makedirs(f"{rhome}/.cc-monitor/bin")
        with open(f"{rhome}/.cc-monitor/bin/ccm", "wb") as fh:
            fh.write(os.urandom(3 * 1024 * 1024 + 17))
        rsrc = f"{rhome}/data.bin"
        with open(rsrc, "wb") as fh:
            fh.write(os.urandom(2 * 1024 * 1024 + 5))
        os.makedirs(os.path.join(d, "dl"))

        print("⑩ 界面那一侧（deploy-* · transfer-copy → 真后端 → 真 sshd）")
        up_m = os.path.join(d, "up-monitor.bin")
        with open(up_m, "wb") as fh:
            fh.write(os.urandom(1024 * 1024 + 9))
        env = {**os.environ,
               "SR1B_LOOPBACK": json.dumps({"host": "127.0.0.1", "port": port, "user": user, "key_path": f"{d}/client_key",
                                            "backend": bin_path, "home": home, "rhome": rhome, "up": up_m,
                                            "dl_remote": rsrc, "dl_local": os.path.join(d, "dl", "monitor.bin")})}
        r = subprocess.run(WITH_SHIM + ["cargo", "test", "--offline", "-p", "monitor", "--lib", "sr1b_loopback_deploy_and_transfer_through_the_resident_backend", "--", "--ignored", "--nocapture"],
                           cwd=os.path.join(ROOT, "src", "frontend", "shell"), env=env, capture_output=True, text=True, timeout=3000)
        check("部署判定三形 · 入口 · 卸载 · 围栏原话 · 上传 / 下载各一条 transfer-copy 全经本机常驻后端", "SR1B-LOOPBACK-MONITOR ok" in r.stdout and "1 passed" in r.stdout,
              (r.stdout[-1500:], r.stderr[-1500:]))
    finally:
        for p in procs:
            p.terminate()
        shutil.rmtree(d, ignore_errors=True)

    print(f"\n{'全过' if not fails else '有不对的：' + ', '.join(fails)}")
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
