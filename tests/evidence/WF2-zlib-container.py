#!/usr/bin/env python3
"""要求：「SSH 那一跳：按判准压」（压缩链路上 > 32000 字节不可压数据写失败）。

一次性容器 sshd（alpine 3.20 · OpenSSH · `Compression yes`，**只绑 127.0.0.1**、随机口）上跑 `dial_compress_tests` 里的两条
`#[ignore]` 读数（过滤串 `zr_real_sshd`）：ZR（可压载荷，强制压 vs 不压）· ZR2（经 `sftp::put_atomic` 放 33000 字节与 1 MiB 不可压字节）。
回环上判准答「不压」⇒ 两条都在测试里用 `config(_, true)` 强制本端压。再读 sshd 日志核（异源）：协商出过 `zlib@openssh.com`、
没有一行 `incomplete message`。钥匙现铸在工作树 `.scratch/wf2-zsshd/`（不碰 `~/.ssh`），容器与镜像用完即删。

跑法（仓根，ASCII 路径的工作树里）：python3 tests/evidence/WF2-zlib-container.py
退出码：0 = 全过 · 1 = 有一条不对 · 3 = 没有 docker / 起不来（环境不满足）
"""
import json
import os
import shutil
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
NAME = f"wf2-zsshd-{os.getpid()}"
DOCKERFILE = """FROM alpine:3.20
RUN apk add --no-cache openssh openssh-sftp-server coreutils \\
 && adduser -D -s /bin/sh u && sed -i 's/^u:!:/u:*:/' /etc/shadow && ssh-keygen -A \\
 && printf 'PasswordAuthentication no\\nKbdInteractiveAuthentication no\\nPermitRootLogin no\\nAllowUsers u\\nCompression yes\\nLogLevel DEBUG1\\n' >> /etc/ssh/sshd_config
COPY authorized_keys /home/u/.ssh/authorized_keys
RUN chown -R u:u /home/u && chmod 700 /home/u/.ssh && chmod 600 /home/u/.ssh/authorized_keys
CMD ["/usr/sbin/sshd", "-D", "-e"]
"""


def sh(*a, **k):
    return subprocess.run(list(a), capture_output=True, text=True, **k)


def main() -> int:
    if not shutil.which("docker"):
        print("环境不满足：没有 docker")
        return 3
    work = os.path.join(ROOT, ".scratch", "wf2-zsshd")
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work)
    key = os.path.join(work, "id")
    sh("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", key, "-C", "wf2")
    shutil.copyfile(key + ".pub", os.path.join(work, "authorized_keys"))
    with open(os.path.join(work, "Dockerfile"), "w") as f:
        f.write(DOCKERFILE)
    if sh("docker", "build", "-q", "-t", NAME, work).returncode != 0:
        print("环境不满足：镜像建不起来")
        return 3
    try:
        if sh("docker", "run", "-d", "--name", NAME, "-p", "127.0.0.1:0:22", NAME).returncode != 0:
            print("环境不满足：容器起不来")
            return 3
        port = int(sh("docker", "port", NAME, "22").stdout.split(":")[-1])
        for _ in range(50):
            if "Server listening" in sh("docker", "logs", NAME).stderr:
                break
            time.sleep(0.1)
        env = dict(os.environ, NT1_COMPRESS=json.dumps({"host": "127.0.0.1", "port": port, "user": "u", "key_path": key}),
                   CARGO_BUILD_JOBS="4")
        env.pop("TMUX", None)
        env.pop("TMUX_PANE", None)
        r = subprocess.run(["cargo", "test", "--lib", "zr_real_sshd", "--", "--ignored", "--nocapture"],
                           cwd=os.path.join(ROOT, "src", "backend"), env=env, capture_output=True, text=True)
        for ln in r.stdout.splitlines():
            if ln.startswith(("NT1-COMPRESS", "WF2-ZR2", "test ")):
                print("  " + ln)
        log = sh("docker", "logs", NAME).stderr
        kex = [ln.split("compression:")[1].split()[0] for ln in log.splitlines() if "kex: client->server" in ln]
        checks = [
            ("两条读数都过（cargo test 退出 0）", r.returncode == 0, r.returncode),
            # 两条读数各两趟（不压 · 压）⇒ 恰好两趟 none、两趟 zlib@openssh.com（并行跑，先后不定 ⇒ 比排序后的）。
            ("sshd 那一侧协商结果 == 两趟 none ＋ 两趟 zlib@openssh.com", sorted(kex) == ["none"] * 2 + ["zlib@openssh.com"] * 2, kex),
            ("sshd 日志里 `incomplete message` 零行", "incomplete message" not in log, log.count("incomplete message")),
        ]
        bad = 0
        for what, ok, got in checks:
            print(f"{'过' if ok else '不过'}  {what}  〔{got}〕")
            bad += not ok
        return 1 if bad else 0
    finally:
        sh("docker", "rm", "-f", NAME)
        sh("docker", "rmi", "-f", NAME)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
