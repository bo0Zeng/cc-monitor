# G-C — graylight/restart/resume 三族 e2e 进 CI（**并解掉 BACKLOG E41**）

> 主计划：`../MASTERPLAN.md` §1 G-C（第 79 行）· §3 账本第 1/3 行 · §6 开放问题 1
> 前置：**G-A**（`620ae3a`，`assert-pass-floor.sh` + 覆盖面地板）

## 1. 开工复测：**我自己上一轮落盘的那份测量有两处错**

上一轮我把 G-C 的面测了一遍写进 STATUS。这轮逐条复核，两条不成立：

| 我上一轮写的 | 实际 |
|---|---|
| 「计划说 6 套，**实际 7 个文件**」 | **计划是对的，我数错了**。`gen-idle-tmux.sh` **0 条断言**，它是**生成器**（造一个可变灰的 `@ccm_sid` 会话），不是套件 |
| 「`合计 PASS=` 打印 **0 处** ⇒ 接不上 `assert-pass-floor.sh`」 | 字面为真但**误导**：6 套**早就有** `pass=0; fail=0` + `ok()`/`bad()` 计数器，收尾也打印 `== 结果:$pass 过 / $fail 败 ==`。**差的只是格式**，不是计数 ⇒ 每套只需改一行 |

（`-L` 隔离 0 处那条是对的，但它对 E41 的**归因**也不对——见 §2。）

**教训照旧**：连续十一轮开工复测都抓出不符，这轮抓的是**我自己上一轮的记录**。
省时间可以用它，结论必须自己再验一遍。

## 2. ★ E41 的实质不是「缺 `-L`」，是「继承了 `$TMUX`」

我原本打算用 `TMUX_TMPDIR` 做隔离（零调用点改动，比给 84 处裸 `tmux` 加 `-L` 干净得多）。
**第一次实测就失败了**——设了 `TMUX_TMPDIR` 建出的会话**仍然落在默认 socket 上**：

```
TMUX_TMPDIR=<tmp> tmux display-message -p '#{socket_path}'  →  /tmp/tmux-1000/default
env | grep ^TMUX                                            →  TMUX=/tmp/tmux-1000/default,2350,2
```

**根因**：跑套件的 shell **本身就在用户的 tmux 会话里**。`$TMUX` 一设，客户端就连那台 server
并**完全忽略 `TMUX_TMPDIR`**。

⇒ **E41 的真实机理**：那批套件危险，不是因为「没写 `-L`」这个表面特征，
而是因为**它们从一个 tmux 会话里被跑起来时会连到外层那台 server**。
（`ccm-cli.test.sh` 里其实早有这条知识：「**必须 `env -u TMUX`**：CLI 在 tmux 内会退化成就地起」。
只是没人把它和 E41 联系起来。）

**第二个实测坑**：`TMUX_TMPDIR` 指向 scratchpad 那种长路径时 tmux 报
`File name too long` —— unix socket 路径上限 108 字节。**必须短路径**。

⇒ 隔离 = **`unset TMUX` + 短 `TMUX_TMPDIR`**，两者缺一不可。实测确认：
隔离 socket 里只有 `iso3`，默认 socket 逐字未变。

## 3. 做了什么

### 3.1 6 套统一前导（零调用点改动）

```bash
unset TMUX TMUX_PANE
TMUX_TMPDIR="$(mktemp -d /tmp/e2e-sock.XXXXXX)"; export TMUX_TMPDIR
```

**好处是零调用点改动**：84 处裸 `tmux` 一个不用改，而且**自动覆盖套件 shell out 出去的东西**
（`ccm` / `cc-spawn` 内部也是裸调 tmux）。

收尾只用 **`-S <私有 socket>`** 收自己那台，**绝不裸 `kill-server`**——
万一隔离没生效，裸的那个会打到用户的 server 上。原 `trap cleanup EXIT` 扩成
`trap 'cleanup; _gc_sock_cleanup' EXIT`。

### 3.2 收尾行改成与另 8 套逐字一致

`===== 合计 PASS=$pass FAIL=$fail =====` —— 好让 G-A 的 `assert-pass-floor.sh`
用**同一条正则**抓，不为这批另造一套。

### 3.3 npm scripts + CI 接线

6 个 `test:*` 进 `package.json`；**按开放问题 1 的建议，不进 `npm test`**
（`npm test` 应保持「不需要 tmux/daemon 就能跑」）。

### 3.4 主计划记的「唯一技术不确定性」被绕开了

主计划 §4 写：「G-C 最后，因为它要 debug daemon（CI 的 `daemon` job 已在编译
`remote-daemon-proto`，**要把产物传给 e2e job——这是 G-C 唯一的技术不确定性**）」。

**不用传**：那三套 daemon-frames 直接放进 `e2e-tmux-rust` job，那儿**本来就有 Rust 工具链**，
加一句 `cargo build --manifest-path remote-daemon-proto/Cargo.toml` 就行——
daemon crate 比该 job 已经在编的 src-tauri 轻得多。**跨 job 传产物这件事整个不需要发生。**

按依赖分两处：

| job | 套件 | 为什么 |
|---|---|---|
| `e2e-tmux`（有 `npm ci`） | `restart` 24 · `resume` 17 | 只要 node（tsx driver）+ tmux，不碰 daemon |
| `e2e-tmux-rust`（有 Rust） | `graylight-frames` 5 · `restart-frames` 5 · `resume-frames` 7 | 要真 daemon 二进制 |

## 4. ★ 第 6 套（`graylight-suite`）拿到了隔离，但**不进 CI**

它跑出 **1 过 / 2 败**。**查清了：与我的隔离改动无关，是环境。**

三条证据：

1. 它是**全链级**套件——断言的是**正在跑的 dev app** 写的
   `~/.claude/claudecode-frontend/logs/monitor.*.log`，脚本自己的报错文案就是「dev 实例在跑吗?」
2. 本机**没有 dev app 在跑**，最新那份日志是 **07-26 的**（4 天前）⇒ 它在等永远不会出现的新行
3. **对照组**：它的 daemon 级兄弟 `graylight-daemon-frames`（`0` 处引用 app 日志）
   在**同样的隔离**下 **5/5 通过** ⇒ 隔离本身工作正常

⇒ **计划说「6 套进 CI」，实际 5 套能进。** 第 6 套要 GUI runner + 跑起整个 app，
这与 `ci.yml` 里既有的那条论证**同源**（「DOM e2e 需 Linux GUI runner + 5min+ app 构建，
且 app 生产仅 Windows → 大投入低 ROI」）。

**但 E41 对它同样解了**：它现在钉在私有 socket 上，**可以安全地在有活会话的机器上本地跑**
——那正是 E41 要的东西。差的只是「进 CI」。

## 5. 门禁

| 门禁 | 前 | 后 |
|---|---|---|
| **带地板的真机套件** | 8 套 / 152 条 | **13 套 / 210 条**（+5 套 +58 条：24/17/5/5/7） |
| 覆盖面地板 | ≥8 + 8 对逐个校验 | **≥13 + 13 对** |
| shellcheck | 37 文件零告警 | **37**（这 6 套本就在 `e2e/*.sh` glob 里，**覆盖面不变**——实测过，没口算） |
| `ci.yml` 触发条件 | — | **零改动** |
| 其余（vendored 294 / cargo 620 / daemon 149 / vitest 866 / tsc 0） | | **不变** |

## 6. tmux 纪律（本轮真跑了 6 套，逐条核过）

- shim 重建（**这次加了第三条规则**：`TMUX_TMPDIR` 已设时放行，未设且无 `-L`/`-S` 一律 rc=97）
  + canary 三向自检
- **中途踩到一次真实污染并当场清理**：验证 `TMUX_TMPDIR` 时建出的 `iso3`/`iso1` 落到了默认
  socket 上（就是 §2 那个发现）⇒ 立刻 `kill-session` 并与本会话最早的快照 `diff` 确认逐字复原。
  **这正是隔离必须先验证再使用的理由。**
- 收尾：`/usr/bin/tmux ls` 与起飞前快照 **`diff` 逐字未变** ✓ · hook **57 → 57** ✓
- **无孤儿**：`pgrep -a -f '^/usr/bin/tmux'` 只剩用户自己那台（pid 2350）· `/tmp/e2e-sock.*` 零残留
- 全程**未起真实 claude**（用 `e2e/fake-claude`）

## 7. Phase D 的账

**如实标注**：铁律 8 要求并行多 agent 审计；本会话常驻指令「除非用户要求不开 agent」
⇒ 主线程 + 全门禁 + **6 套真机实跑**代替。**这是欠账，不是强度裁剪。**

本功能的「变异」形态特殊：**隔离是否生效本身就是可判色的**——
`TMUX_TMPDIR` 单独用（不 `unset TMUX`）就是一次天然的反例，它**当场红**（会话落到默认 socket），
这比人为构造的变异更强，因为它是我原本打算采用的方案。

## 8. 本轮没做的

- `graylight-suite` **不进 CI**（§4，需 GUI runner + 跑起 app）
- 6 套**不进 `npm test`**（开放问题 1 的建议；代价「本地改 `shared/ccm` 不会自动触发」
  **待写进 `e2e/README.md`** —— 本轮未写，登记）
- 未给任何一套**加**断言（本功能是隔离 + 进 CI，不是扩覆盖）

## 9. 签收

- [x] **BACKLOG E41 已解**：6 套全部钉在私有 socket 上，可安全在有活会话的机器上跑（实测默认 socket 逐字未变）
- [x] 5 套进 CI 并**自带地板**（G-A 的形态直接复用，未另造）
- [x] **订正我自己上一轮的两处记录**（`gen-idle-tmux.sh` 是生成器；6 套早有计数器、只差格式）
- [x] **绕开了主计划记的「唯一技术不确定性」**（daemon 在 `e2e-tmux-rust` 就地编，不跨 job 传产物）
- [x] 第 6 套不进 CI 的理由**查到证据**（对照组 + 陈旧日志 + 无 app 进程），不是猜的
- [ ] `e2e/README.md` 补「不进 `npm test` 的代价」一句 —— **登记，未做**
