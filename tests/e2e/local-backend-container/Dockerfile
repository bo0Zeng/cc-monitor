# 干净 Linux 容器台架的镜像 —— `tests/e2e/local-backend-container/rig.sh` 唯一的被测容器镜像。
#
# ## 它要回答的问题
#
# 「用户在一台**干净 Linux 机**上装了我们的 `.deb`，敲第一条命令，能不能用。」
# 盘上此前**没有任何一条读数来自干净 Linux 机**（`K-R129` `§1.3` 逐字：`.deb`「没做」，
# SHA 对上了但没有任何机器装过它）⇒ 这台容器就是那台机。
#
# ## 🔴 为什么镜像里**预装** `.deb` 的依赖闭包，而 `.deb` 自己不进镜像
#
# 两件事要分开量，不许合成一格：
#   · **「一台裸 Ubuntu 上 `dpkg -i` 会怎样」** —— 那是 `rig.sh` 的 `P3`，它**另起一个
#     裸 `ubuntu:24.04`**、不用本镜像，专门把「依赖缺不缺」这一格量出来。
#   · **「依赖齐了之后，那一版能不能用」** —— 那是本镜像的用途。
# 合成一格的话，`dpkg` 因为缺依赖而失败会把后面每一格都变成「没跑过」，
# 而读起来像「装不上」——**那是两个不同的结论**。
#
# `.deb` 与 `monitor` **刻意不进镜像**：它们是**发版产物**（18MB / 43MB），
# 由 `rig.sh` 在跑的时候 `docker cp` 进去，并在进去之前逐字核 sha256。
# 进了镜像就等于把「测的是哪一版」焊死在镜像里，而镜像是会被复用的。
#
# ## 装了什么、为什么
#
# - `libwebkit2gtk-4.1-0` / `libgtk-3-0`：`.deb` 的 `Depends` 逐字那两条
#   （现打：`dpkg-deb -f cc-monitor_3.8.0_amd64.deb Depends`）。
# - `xvfb` / `dbus-x11`：`monitor` 是 Tauri GUI 应用，没有显示就起不来；
#   而**起一次 app 正是把 `~/.cc-monitor/bin/ccm` 放下来的那条生产路径**
#   （`local_backend.rs:1657` 的 `install_local_ccm_entry`）⇒ 不起它就测不到真落点。
# - `procps` / `iproute2`：看后端进程起没起、听没听口。**量具，不是被测对象。**
# - `ca-certificates`：`dpkg` 与 apt 的常规依赖，跑的时候用不到（跑的时候没有网）。
#
# ## 刻意**没**装什么
#
# - **没装 `gcc` / `build-essential`**：Debian 系上 `cc` 是 C 编译器的软链
#   （`/usr/bin/cc` → `/etc/alternatives/cc`）。装了它，`command -v cc` 会**恒真**，
#   而我们要量的正是「`cc` 这个名字通不通」⇒ 那会是一次**靠环境恒真的假读数**。
#   ⚠ 真实用户机器上大概率**是**装着 gcc 的 ⇒ 这一格 `rig.sh` 里单独有一条对照
#   （`P7b`），把「干净机上 `cc` 是什么」与「有 gcc 的机上 `cc` 是什么」分开报。
# - **没装 `sudo`**：`dpkg -i` 那一步由 `docker exec -u root` 做，不需要它。
FROM ubuntu:24.04

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      libwebkit2gtk-4.1-0 libgtk-3-0 \
      xvfb dbus-x11 \
      procps iproute2 ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# 一个**普通用户** —— 「用户敲第一条命令」这件事不许在 root 下量：
# root 的 PATH 与家目录都与普通用户不同，而我们要判的正是家目录下那两个落点。
RUN useradd -m -s /bin/bash tester

# 容器由 `rig.sh` 用 `docker exec` 驱动，自身不需要跑任何服务。
CMD ["sleep", "infinity"]
