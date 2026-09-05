# 弱网台架镜像 —— `e2e/weak-net/rig.sh` 唯一的被测容器镜像。
#
# ## 为什么住 `e2e/weak-net/` 而不是 `.claude/devbox/`
#
# 两条理由，都可核：
# 1. **消费者只有一个**：这个镜像除了本套件没有第二处用。仓里的惯例是「夹具跟着它唯一的
#    消费者住」——`e2e/restart-shims/`、`e2e/tier2/` 都是这个形状。
# 2. `.claude/devbox/` 是**门禁沙箱**（`gate` 跑整仓九格的地方），它的镜像是通用开发环境；
#    把一个只给一套 e2e 用的网络实验镜像塞进去，等于让每个跑门禁的人都背上它。
#
# ## 🔴 `--network host` 只许出现在 build 那一步（定框 `W3`）
#
# 这台开发机的容器**上不了外网**（实测：容器里访问 `172.17.0.1:7890` 超时），装包唯一可行路是
# `docker build --network host` + 宿主 `127.0.0.1:7890` 那个代理。那条命令与理由写死在
# 同目录的 `build-image.sh` 里。
#
# **跑的时候绝不用宿主网络**：那样 `tc`/`netem` 会打在用户这台机的真网卡上 —— 等于拿用户的
# 网络当试验台。跑的时候一律自建 docker 网络 + `--cap-add=NET_ADMIN`（**不给 `--privileged`**：
# 改本容器 netns 里的 qdisc 只需要 `CAP_NET_ADMIN` 这一项）。
# 这条边界有一道文本判据看着：`guard-run-netns.sh`（它的射程与绕过形态写在它自己的头注里）。
#
# ## 装了什么、刻意没装什么
#
# - `iproute2`：`tc`（netem/tbf）与 `ss`，台架的被测对象本身。
# - `iputils-ping`：延迟与丢包两维的量具。
# - `openssh-server` / `openssh-client`：`WF1D2` 那格 —— B 起 sshd 当远端，A 用密钥登进去。
#   仓里今天**没有任何一处自己起过 sshd**（定框 `W4`），这一格是自己建的。
# - `netcat-openbsd`：带宽那维的收发端。
# - **刻意没装 `bc` 与 `time`**：量具全程用 bash 的 64 位整数算（毫秒 / 微秒 / 百分点），
#   一处浮点都没有 ⇒ 少两个包，也少两处「镜像里恰好没有它」的失败面。
FROM ubuntu:24.04

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      iproute2 iputils-ping \
      openssh-server openssh-client \
      netcat-openbsd ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /run/sshd

# 容器由 `rig.sh` 用 `docker exec` 驱动，自身不需要跑任何服务。
CMD ["sleep", "infinity"]
