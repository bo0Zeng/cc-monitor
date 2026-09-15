# `K-R136` 死值验 —— 收窄之后牙还在吗（`KR136D2`）

> 量于 **2026-09-15**。被测树 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r136`
> （`track/k-r136`，基线 `5b915daa`）。**全部在沙箱里跑**：
> `docker run --rm --network none -v <scratchpad>:/s -w /s/kr136-fx ccmon-devbox:latest`
> —— 与 `.claude/devbox/gate` **同镜像、同隔离档（`--network none`）**，
> 🔴 **只换了最后那条命令**（`gate` 最后一行写死 `bash scripts/gate.sh`，跑不了单把台架；
> 缺口已立 `K-R130`）。**如实上报，前几件也是这么做的。**

## 0 · 🔴 跑不动的那一半，先写清楚

**弱网台架整趟（`bash e2e/weak-net/rig.sh`）我没跑。** 两条理由：

1. `ccmon-weaknet:latest` 镜像本机没有（`docker images` 现打：无此条目），建它要联网；
2. 更要紧的是 **`K31`**：整趟跑会在**宿主**上真建 bridge + 2 条 veth。
   那是台架设计上就要做的事，但它不是「在沙箱里跑测试」。

⇒ 本件把判定**抽成一个纯文本进、分类出的函数**（`rig.sh:137` `host_link_classify`），
并给它一个喂夹具的口子（`rig.sh:212` `--link-residue`）。**死值验全部打在这个函数上。**
`KR136D2` 的字面也正是这么留的出路（「造不出来就说造不出来 —— 那时至少要把
`enP20511s1` 那一行原始输出当夹具喂进判定函数」）。

**因此本文买不到的东西，逐条**：
- 买不到「整趟跑完 PASS=29」这个读数 —— 那要 PM 在有镜像的机器上跑一趟。
- 买不到「`link.mid` 那一格在真跑动中确实数出 3 个」—— 本文用夹具替代（`§3`）。
- **买得到**的是：判定函数在十二形输入上的逐格行为、五刀变异、以及「牙还在」。

## 1 · 夹具怎么造的（可重跑；夹具名一律中性，判定读的是设备名与旗标，不读路径）

```bash
#!/usr/bin/env bash
# K-R136 死值验夹具生成器。夹具名一律中性（判定读的是设备名与旗标，不读路径）。
set -eu
D="${1:?用法: gen.sh <输出目录>}"
mkdir -p "$D"

# ── base：GitHub runner（Azure）跑前那一份。**构成不是编的**：
#    事故 diff 段是 `6a7,9` ⇒ 前一份恰好 6 行 ＝ 3 个设备 × 2 行，
#    而追加的是第 7-9 行 ＝ VF 那一块 3 行（头行 + link/ether + altname）。
cat > "$D/base" <<'EOF'
1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN mode DEFAULT group default qlen 1000
    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00
2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc mq state UP mode DEFAULT group default qlen 1000
    link/ether 60:45:bd:cd:1a:2f brd ff:ff:ff:ff:ff:ff
3: docker0: <NO-CARRIER,BROADCAST,MULTICAST,UP> mtu 1500 qdisc noqueue state DOWN mode DEFAULT group default
    link/ether 02:42:6c:9f:3b:11 brd ff:ff:ff:ff:ff:ff
EOF

# ── A 干净：跑完与跑前逐字相同
cp "$D/base" "$D/clean"

# ── B 事故复现：`enP20511s1` 那一块，逐字取自件 K-R136 §0 的 CI 输出。
#    ⚠ 件文件里 `state UP` 之后是 `…`（截断）；补的是标准尾部。
#    判别式只读 `<…>` 里的 `SLAVE` 与 `master eth0` —— 两样都在**逐字**那一段里。
cp "$D/base" "$D/vf-incident"
cat >> "$D/vf-incident" <<'EOF'
7: enP20511s1: <BROADCAST,MULTICAST,SLAVE,UP,LOWER_UP> mtu 1500 qdisc mq master eth0 state UP mode DEFAULT group default qlen 1000
    link/ether 60:45:bd:cd:1a:2f brd ff:ff:ff:ff:ff:ff
    altname enP20511p0s2
EOF

# ── B2 第二趟那个名字（`main` 7fb79638）—— 证「换个名字判别式照样成立」
sed 's/enP20511s1/enP29037s1/; s/enP20511p0s2/enP29037p0s2/' "$D/vf-incident" > "$D/vf-incident-2"

# ── B3 内核现造的 SLAVE 行（不是手打的；来自隔离 netns 里的 bond+slave）
cp "$D/base" "$D/vf-realkernel"
cat >> "$D/vf-realkernel" <<'EOF'
9: vfdev: <BROADCAST,NOARP,SLAVE,UP,LOWER_UP> mtu 1500 qdisc noqueue master eth0 state UNKNOWN mode DEFAULT group default qlen 1000
    link/ether 4e:89:27:2c:08:33 brd ff:ff:ff:ff:ff:ff
EOF

# ── C 真残留①：自建网络没删（bridge + 两条 veth 全留着）
cp "$D/base" "$D/residue-net"
cat >> "$D/residue-net" <<'EOF'
11: br-9f2a1c4d8e05: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue state UP mode DEFAULT group default
    link/ether 02:42:aa:bb:cc:dd brd ff:ff:ff:ff:ff:ff
13: veth3c91f0a@if2: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue master br-9f2a1c4d8e05 state UP mode DEFAULT group default
    link/ether 7a:11:22:33:44:55 brd ff:ff:ff:ff:ff:ff link-netnsid 0
15: veth8b22e71@if2: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue master br-9f2a1c4d8e05 state UP mode DEFAULT group default
    link/ether 7a:66:77:88:99:aa brd ff:ff:ff:ff:ff:ff link-netnsid 1
EOF

# ── D 真残留②：一条**没有 master** 的野 veth（手动留下的那一个）
cp "$D/base" "$D/residue-stray"
cat >> "$D/residue-stray" <<'EOF'
17: vethstray0@vethstray1: <BROADCAST,MULTICAST,M-DOWN> mtu 1500 qdisc noop state DOWN mode DEFAULT group default qlen 1000
    link/ether 5e:01:02:03:04:05 brd ff:ff:ff:ff:ff:ff
EOF

# ── E 最难的一格：真残留**与**热插拔同一趟一起出现 ⇒ 必须红，且只点名残留那一个
cat "$D/residue-stray" > "$D/residue-plus-vf"
tail -3 "$D/vf-incident" >> "$D/residue-plus-vf"

# ── F 台架把宿主设备弄没了（T2）
grep -v -A1 'docker0' "$D/base" | grep -v '^--$' > "$D/gone-dev" || true
{ sed -n '1,4p' "$D/base"; } > "$D/gone-dev"

# ── G tc 打到了宿主真网卡上（T3）：eth0 那一行 qdisc mq → qdisc netem
sed 's/^2: eth0: \(.*\) qdisc mq /2: eth0: \1 qdisc netem /' "$D/base" > "$D/tc-on-host"

# ── H 空快照（awk NR==FNR 那个坑的活体）
: > "$D/empty"
```

🔴 **`base` 的构成不是编的**：件 `K-R136` `§0` 记的 CI `diff` 段是 **`6a7,9`**
⇒ 前一份恰好 **6 行 ＝ 3 个设备 × 2 行**，追加的是第 **7-9** 行 ＝ VF 那一块 3 行
（头行 + `link/ether` + `altname`）。照这个反推出 `lo` / `eth0` / `docker0` 三个设备。
**现打验证**：拿老写法（`diff -q base vf-incident`）跑，输出的 `diff` 段逐字是

```
6a7,9
> 7: enP20511s1: <BROADCAST,MULTICAST,SLAVE,UP,LOWER_UP> mtu 1500 qdisc mq master eth0 state UP mode DEFAULT group default qlen 1000
>     link/ether 60:45:bd:cd:1a:2f brd ff:ff:ff:ff:ff:ff
>     altname enP20511p0s2
```

—— **与事故那一趟的 `6a7,9` 逐字同形** ⇒ 夹具是复原，不是发明。

⚠ **一处诚实边界**：件文件里 `state UP` 之后是 `…`（截断），夹具补的是标准尾部
`mode DEFAULT group default qlen 1000`。判别式只读 `<…>` 里的 `SLAVE` 与 `master eth0`
**两样，两样都在逐字那一段里** ⇒ 补的那一段进不了判定。

`vf-realkernel` 那一行**不是手打的**，是隔离 netns 里 `ip link add bond0 type bond` +
`ip link set vfdev master bond0` 之后内核现印的：

```
3: vfdev: <BROADCAST,NOARP,SLAVE,UP,LOWER_UP> mtu 1500 qdisc noqueue master bond0 state UNKNOWN mode DEFAULT group default qlen 1000
```

## 2 · 本件这一版：十二形输入逐格真实输出

「红」＝ `NEW` + `GONE` + `CHG` + `ERR` 四档的条数之和（`NEWX`/`GONEX` 是**排除**，不判红）。
**分母 = 我造的这 12 形**，不是「全部可能的输入」——那个分母我数不出来。

| 格 | 输入（前→后） | 红 | 逐项 | 判定 |
|---|---|---|---|---|
| A | `base`→`clean` | **0** | —— | 干净一趟不红 |
| B | `base`→`vf-incident`（**事故那一行**） | **0** | `NEWX=1 enP20511s1` | 🔴 **`KR136D2②` 热插拔不红** |
| B2 | `base`→`vf-incident-2`（`enP29037s1`） | **0** | `NEWX=1 enP29037s1` | 换个名字照样不红 ⇒ 不是白名单 |
| B3 | `base`→`vf-realkernel`（内核现造） | **0** | `NEWX=1 vfdev` | 同上 |
| C | `base`→`residue-net`（自建网络没删） | **3** | `NEW=3`：`br-9f2a1c4d8e05` · `veth3c91f0a` · `veth8b22e71` | 🔴 **`KR136D2①` 真残留仍红且点名** |
| D | `base`→`residue-stray`（野 veth） | **1** | `NEW=1 vethstray0` | 同上 |
| **E** | `base`→`residue-plus-vf`（**残留与热插拔同一趟**） | **1** | `NEW=1 vethstray0` ＋ `NEWX=1 enP20511s1` | 🔴 **最要害的一格**：只点名残留，VF 排除 |
| F | `base`→`gone-dev`（宿主设备没了） | **1** | `GONE=1 docker0` | T2 |
| G | `base`→`tc-on-host`（`eth0` `qdisc mq`→`netem`） | **1** | `CHG=1 eth0`（前后两行都印） | T3：**tc 打到宿主那颗牙还在** |
| H | `empty`→`base`（空快照） | **1** | `ERR=1`，退出码 **2** | 不静默绿 |
| I | `base`→`residue-on-docker0` | **1** | `NEW=1 vethleak9` | 挂在既有 bridge 上的残留也逮得住 |
| J | `base`→`mid-clean`（跑动中） | **3** | `NEW=3` | P2 分母格：分类器**够得着**台架自己的设备 |

格 B 的逐字输出（`NEWX` 那一行印出被排除的是谁 —— **排除不许静默**）：

```
NEWX	enP20511s1	7: enP20511s1: <BROADCAST,MULTICAST,SLAVE,UP,LOWER_UP> mtu 1500 qdisc mq master eth0 state UP mode DEFAULT group default qlen 1000 | link/ether 60:45:bd:cd:1a:2f brd ff:ff:ff:ff:ff:ff | altname enP20511p0s2
```

格 E 的逐字输出：

```
NEW	vethstray0	17: vethstray0@vethstray1: <BROADCAST,MULTICAST,M-DOWN> mtu 1500 qdisc noop state DOWN mode DEFAULT group default qlen 1000 | link/ether 5e:01:02:03:04:05 brd ff:ff:ff:ff:ff:ff
NEWX	enP20511s1	7: enP20511s1: <BROADCAST,MULTICAST,SLAVE,UP,LOWER_UP> mtu 1500 qdisc mq master eth0 state UP mode DEFAULT group default qlen 1000 | link/ether 60:45:bd:cd:1a:2f brd ff:ff:ff:ff:ff:ff | altname enP20511p0s2
```

格 G 的逐字输出：

```
CHG	eth0	2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc mq state UP mode DEFAULT group default qlen 1000 | link/ether 60:45:bd:cd:1a:2f brd ff:ff:ff:ff:ff:ff ||| 2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc netem state UP mode DEFAULT group default qlen 1000 | link/ether 60:45:bd:cd:1a:2f brd ff:ff:ff:ff:ff:ff
```

## 3 · 变异表 —— 五刀，逐行带锚点与命中数

**锚点命中数都是切之前断言过的**，命中数不等于 1 就不切（`brief` 第 7 条）。
五刀全部切在 **`e2e/weak-net/rig.sh` 的 `host_link_classify` 里那个 `kernel_enslaved`
函数体**，切在**副本**上（`<scratchpad>/kr136-fx/rig-cut*.sh`），**工作树一个字节没动**。

| 刀 | 切在哪 / 换成什么 | 锚点命中 | 输入 | 实得 | 读作 |
|---|---|---|---|---|---|
| **①** | `kernel_enslaved` 函数体 5 行 → `return 0`（＝逐字退回收窄之前） | **1** | `base`→`vf-incident` | **红 1**（`NEW=1 enP20511s1`） | 🔴 **复现今天这一趟** |
| ①b | 同上 | 1 | `base`→`clean` | **红 0** | **阴性对照（实质版）**：刀①不是一台恒红机器 |
| ①c | 同上 | 1 | `base`→`residue-net` | 红 3（`NEW=3`） | 刀①只改 VF 那一格的判词，别的没动 |
| **②** | **本件新判据整个不在**（`git show HEAD:e2e/weak-net/rig.sh`）＋ 刀① | **0** | `base`→`vf-incident` | **刀①无落点**；HEAD 版里 `kernel_enslaved` 0 处、`--link-residue` 0 处 | **阴性对照（字面版）**：刀①那一红是本件的代码买的 |
| **④** | 只掉 `SLAVE` 那条腿：删 `      if (fl(rec) !~ /,SLAVE,/) return 0` | **1** | 全部 12 形逐形比 | **活着**：**分母 12 形 · 同答 11 形 · 不同答 1 形** | 逼出了一格新夹具，见 `§4` |
| **⑤** | 两条腿都掉：「有 `master` 就排除」 | **1** | `base`→`mid-clean` | `NEW=1`（`NEWX=2` ＝ **两条 veth 被吞了**）⇒ **P2 分母格红** | 「排除写宽」当场被逮 |

⚠ **CRASH：0 次。** 五刀都保住了类型契约（`kernel_enslaved` 恒返回 0/1，被三元运算符消费）。
判定行（每格印出的分类行数）五刀全程有，没有一刀出现「判定行掉了」。

### 刀③ · 假红方向

| 跑法 | 遍数 | 红 |
|---|---|---|
| 固定夹具 `base`→`clean` 连跑 | **200** | **0** |
| **真实宿主** `ip link` 快照对（只读连拍，机上 4 个容器在跑） | **60 对** | **0** |

读数稳不稳（`references/testing.md` 六）：同一份数据（`base`→`residue-plus-vf`）连跑三次，
输出 md5 三次全同 `d200434c1d55655045df0179d859d232`。

⚠ **「60 对真实快照 0 红」买不到「整趟跑 60 遍 0 假红」** —— 它量的是**判定函数**在
真实宿主快照上的假红率，不是整趟台架的。整趟那个数要 PM 在有镜像的机器上跑。

## 4 · 刀④ 活着 —— 这是本轮最值钱的一条读数，单列

刀④ ＝ 掉 `SLAVE` 那条腿，只剩「`master` 在前一份快照里」。
现打逐形比（**分母 = 本文 `§2` 那 12 形，按格认不按字面认**）：**同答 11 形 · 不同答 1 形**
（VF 照样排除、真残留照样逮住）⇒ 一刀活着的变异。

**逼出来的那一格新夹具**：残留的 veth 故意挂在**跑前就有**的 `docker0` 上
（`residue-on-docker0`）。

| | 本版 | 刀④ |
|---|---|---|
| `base`→`residue-on-docker0` | `NEW=1 vethleak9` **逮住** | `NEW=0`（`NEWX=1`）**漏掉** |

⇒ **`SLAVE` 那条腿是承重的**，刀④被这一格杀掉。
⇒ 同时这也是「为什么不采『新设备的 master 在前一份里就有』那条更宽的排除」的**实测理由**
（那条能顺手挡住第三方 docker 活动的假红，但代价就是上面这一格）。

## 5 · 把实现整个退掉，还有多少条新断言仍绿

「退掉实现」＝ **刀①**（收窄那一步退回去），四条新断言逐条：

| 新断言 | 住址 | 退掉之后 | 仍绿的理由 |
|---|---|---|---|
| 1 · P2 分母 ≥3 | `rig.sh:288` `if [ "$RIG_DEV_N" -ge 3 ]; then` | **仍绿**（`NEW=4`） | 它守的是**反方向**（排除写**宽**）⇒ 刀①是把排除写**窄**到没有，它够不着。它的牙在**刀⑤**上（`NEW=1` ⇒ 红）。 |
| 2 · T1 没留新设备 | `rig.sh:492` `    ok "宿主 ip link：台架没留下新设备（它跑动中造过 $RIG_DEV_N 个，见 P2 那一格）"` | **红**（`NEW=1`） | 🔴 这条就是本件的牙 |
| 3 · T2 设备没少 | `rig.sh:497` `    ok "宿主 ip link：跑前有的设备一个都没少"` | **仍绿**（`GONE=0`） | 事故夹具是纯追加，这条本就不该红；它的牙在格 F（`GONE=1 docker0`） |
| 4 · T3 行没变 | `rig.sh:502` `    ok "宿主 ip link：两份都有的设备，每一行逐字相同（tc 没打到宿主设备上）"` | **仍绿**（`CHG=0`） | 同上；它的牙在格 G（`CHG=1 eth0`，tc 打到宿主） |

⇒ **4 条里 3 条仍绿，但没有一条是仪式**：每条都有自己的一刀（分别是刀⑤ · 刀① · 格 F · 格 G）。

## 6 · 断言数地板

CI 那条地板住 `.github/workflows/ci.yml:1102` `        run: bash e2e/weak-net/assert-floor.sh 26`，
判法是 `n < FLOOR` 才失败（`e2e/weak-net/assert-floor.sh:53`）⇒ **加格安全，减格危险**。

`ok "` / `chk "` 调用点现打（分母 = 行首缩进后逐字以 `ok "` 或 `chk "` 打头的行）：

| 版本 | `ok` | `chk` |
|---|---|---|
| `HEAD`（`5b915daa`） | 12 | 9 |
| 本件 | **15** | 9 |

净 **+3**（新增 4 条，删掉 1 条「宿主 ip link 跑前跑后逐字相同」）
⇒ 干净一趟 PASS **26 → 29** ≥ 地板 26。**`ci.yml` 不用改。**
⚠ 这个 29 是**算出来的**，不是跑出来的（整趟跑不了，理由见 `§0`）。
