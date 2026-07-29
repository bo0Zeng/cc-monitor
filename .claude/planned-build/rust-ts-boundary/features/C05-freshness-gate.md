# C05 — 门禁：生成物必须最新

> 主计划：`../MASTERPLAN.md` · 前置：C01 已闭环（`20c9dd7` + `8abd489`）
> **本功能由 C01 的 Phase D 审计从执行顺序 #4 提到 #2**，理由见 §1。

## 1. 为什么必须在 C02/C03/C04 之前

C01 的 Phase D 审计变异实测（我已独立复现）：

> serde-rename 一个 Rust 字段而**不重新生成** ⇒ `cargo build` 绿 · `tsc` 绿 ·
> 5 条守卫绿 · `npm test` 819 全绿，**而生产里 IPC 发新字段名、TS 读旧名拿 `undefined`、
> UI 标签变空**。

**失效模式不是「没门禁」，而是「所有现有门禁都保持绿色」。** 根因：CI 的 `rust` job
（跑 `cargo test`，会重新生成）与 `frontend` job（跑 `tsc`）是**两次独立 checkout**——
前者重新生成的产物被丢掉，后者对着**已提交的**生成物做类型检查。

若先做 C02/C03/C04，这个洞会被复制到 **127 个 struct**。

## 2. Phase B 实测：没有任何 job 同时有 Rust 和 node

| job | 平台 | Rust | node |
|---|---|---|---|
| `rust` | windows-latest（`working-directory: src-tauri`） | 有 | **无** |
| `frontend` | windows-latest | **无** | 有 |
| `daemon` | ubuntu-latest | 有（但是 `remote-daemon-proto` 另一个 crate） | 无 |
| `e2e-scripts` | ubuntu-latest | 无 | 无（注释明写「不该为一条纯 shell 检查引入 npm 依赖」） |
| `e2e-tmux` | ubuntu-latest | **无**（job 名就叫「no Rust」） | 有 |
| `e2e-tmux-rust` | ubuntu-latest | 有 | 有 |

⇒ **`npm run check:types`（C01 加的那条 `gen:types && tsc && git diff`）不能直接进 CI**：
- 放 `rust` job 要加 `setup-node` + `npm ci`（windows 上的 `npm ci` 是分钟级）；
- 放 `frontend` job 要加整套 Tauri 编译 —— **那是 CI 里最贵的东西，复制一份不可接受**；
- `e2e-tmux-rust` 两者都有，但它是 ubuntu + 真机 tmux 套件，把类型门禁挂在真机 job 上
  会让一个环境问题看起来像类型问题。

## 3. 设计：把性质拆成两半，各在已有 job 里查

```
rust job（已有 Rust，cargo test 本来就会重新生成）
  └─ git diff --exit-code -- ../src/generated/
     ⇒ 性质 A：已提交的生成物 == 从当前 Rust 源生成的

frontend job（已有 node）
  └─ tsc --noEmit（已存在）
     ⇒ 性质 B：TS 消费方 == 已提交的生成物

A ∧ B ⇒ TS 消费方 == Rust 源   ← 这正是要守的东西
```

**为什么这比串联更好**：`git diff --exit-code` **不需要 node**，所以性质 A 的代价
≈ 一条 git 命令；而串联版要在某个 job 里多装一整套工具链。
**两半各自在自己已有工具链的 job 里，总代价近乎零。**

**组合是否严密**（逐格检查）：

| 情形 | 谁红 |
|---|---|
| 改了 Rust、**没**重新生成、没提交生成物 | **性质 A 红**（cargo test 重新生成后 diff 非空） |
| 改了 Rust、重新生成并提交、TS 消费方没跟上 | **性质 B 红**（tsc 看到新类型） |
| **已提交的**手改（Rust 源没动） | **性质 A 红**——CI 从 commit checkout ⇒ 树里是伪造版；`cargo test` 重新生成出正确版 ⇒ diff 非空 |
| **未提交的**手改（只在本地工作树里） | **不红，但也无害**——`cargo test` 直接把它冲掉了。**它从来到不了仓库**，开发者会看到自己的改动消失 |
| 只改 TS 消费方 | 性质 B 红 |

⇒ **顺带把 C01 那个已登记的盲区（手改生成物抓不到）也堵上了**。
C01 的守卫头注与 `.gitattributes` 里那两处「目前只靠人工纪律」的如实说明，
本功能落地后要一起改准。

## 4. DoD

- [x] `rust` job 在 `cargo test --all` 之后加一步 `git diff --exit-code -- ../src/generated/`
- [x] 该步骤的失败信息**说清了怎么修**（跑 `npm run gen:types` 并提交），
      否则下一个人看到一个裸的 diff 退出码不知道该干什么
- [x] **未改任何 workflow 触发条件**（`yaml.safe_load` 核对：`push{main,v*} / pull_request{main}` 未变）（红线）
- [x] **变异验收**：serde-rename 一个字段而不重新生成 → **本地复刻 CI 那一步会红**
      （`cargo test --all` 后 `git diff --exit-code`）。先 diff 确认变异落位 + 确认编译得过再判色
- [x] **反向自检**：不变异时该步骤退出码 0（实测）
- [x] **已提交的**手改生成物被抓到（Rust 源未动）—— C01 登记盲区的闭合。
      **措辞订正**：本条初稿写成「手改生成物也被抓到」，**不准**。第一次测法也错了——
      我在工作树里手改后直接跑检查，得到**绿**，因为 `cargo test` 先把手改冲掉了、
      再 diff 就与 HEAD 一致。**那不是「没抓到」，是测错了场景**：
      CI 面对的永远是**已提交**的内容。改用 index 模拟已提交状态后 ⇒ **红，确实抓到**。
      未提交的手改会被静默冲掉，**从来到不了仓库**，所以不构成漏洞（但值得写下来，
      否则下一个人会像我一样测出一个假的绿灯）
- [x] C01 的守卫头注两条 + `.gitattributes` 那段改准——**「已知盲区」这个标题也换了**
      （改成「本文件抓不到什么、谁来抓」），因为那两条已经不是敞着的洞了
- [x] 全门禁绿且数字不降：cargo **538** · code-picture-core **25** · npm **819/54** ·
      clippy 0 error · tsc 0 · fmt 干净 · npm audit rc=0 · shellcheck 0 · exec-bit rc=0
- [x] 8 套真机套件全绿、条数与基线一致（26/44/12/15/13/-/14/7）· 默认 socket 4 会话逐字未变 ·
      `git diff -- e2e/` 0 行（本功能不碰 e2e）

**明确不做**：不给 `rust` job 加 node · 不给 `frontend` job 加 Rust ·
不改触发条件 · 不动 `e2e-*` 任何 job · 不碰 C02/C03/C04 的范围

## 5. 冲突协议：本功能先改 `ci.yml`

主计划 §3 共享面 6 与 `planned-build/README.md` 的协议原文是
「`gate-integrity` 最先改 `ci.yml`」。**但 `gate-integrity` 还没开工**，而 C05 已被提到 #2。

→ **本功能先改**，并遵守一条约束：**只在 `rust` job 末尾追加一步，不重排、不改动任何既有步骤**。
`gate-integrity`（G-A/G-B/G-C）与 `local-as-remote`（L0/L4）后到时同样只追加。
协议实质不变（后到者不重排先到者），只是先后顺序按实际开工调整。**已回写两处文档。**

## 6. 测试策略

- **主判据是变异**（DoD 第 4/6 条），在本地复刻 CI 的那一步来判色 —— 因为
  「CI 会不会红」这件事没法在本地直接观察，只能复刻它的命令。
- **不加新的守卫测试**：本功能的守卫就是 CI 那一步本身，再写一个「断言 ci.yml 里有那一步」
  的文本扫描测试属于代理指标（源码文本扫描 ≠ 行为），而它保护的东西
  （那一步真的会红）已经由变异证明。

## 7. 代码审计结果（Phase D）

**低风险档，不开审计 agent。** 判断依据（planned-build 的强度裁剪）：
本功能**零生产代码改动**——只加了一步 CI 检查 + 改准三处文档措辞。
攻击面就是「那一步会不会红」，而那已经由**两个变异**直接证明（见 §4），
不是靠推理。再开一个 agent 去读一条 `git diff --exit-code` 收益极低。

**但我自己在 DoD 里写错过一条，如实记在这儿**（比审计报出来更该记）：

初稿写「手改生成物也被抓到」，且我第一次的测法也错了——在**工作树**里手改后直接跑检查，
得到**绿**，一度以为设计有洞。实际是**测错了场景**：`cargo test` 会先把工作树里的手改冲掉，
再 diff 自然与 HEAD 一致。而 CI 面对的永远是**已提交**的内容。
用 index 模拟已提交状态后 ⇒ **红，确实抓到**。

**这一条的教训与「编译失败不等于测试有牙」同族**：
**判色之前先确认自己造的场景就是要防的那个场景。** 我造的是一个到不了仓库的场景。

## 8. 工程审计结果（Phase E）

- **主计划自洽**：C05 未引入新耦合。它改的是 `ci.yml` 的 `rust` job 末尾一步。
- **共享面 6（`ci.yml`）的协议按实际开工顺序调整**：原文「`gate-integrity` 最先改」，
  但它还没开工而 C05 已提到 #2 ⇒ **本功能先改，且只在末尾追加、不重排任何既有步骤**。
  后到的 `gate-integrity`（G-A/G-B/G-C）与 `local-as-remote`（L0/L4）同样只追加。
  **协议实质不变**（后到者不重排先到者），已回写主计划 §3 与 `planned-build/README.md`。
- **对 C02/C03/C04 的意义**：它们从此**在一个已经会红的门禁下工作**。
  C01 那个「所有现有门禁都保持绿色」的失效模式不会被复制到 127 个 struct——
  这正是把 C05 提前的全部目的，现在兑现了。
- **一条留给 Windows 的未验**：C05 那一步跑在 `windows-latest` 上，
  而我在 Linux 上验的。`.gitattributes` 有 `* text=auto eol=lf`（审计已核实覆盖生成物），
  所以 CRLF 不该造成假红——**但没在 Windows 上实证**。如实登记，
  首次 CI 跑到这一步就能看出来。

## 9. 签收

- [x] **通过代码审计** —— 低风险档、零生产代码，判据是两个变异（§4/§7），不开 agent。
      §7 如实记了我自己 DoD 写错的一条 + 第一次测错场景的教训。
- [x] **通过工程审计** —— 见 §8。含一条 Windows 未验的如实登记。
- [x] **主计划已据此更新** —— §3 共享面 6 的协议注明按实际开工顺序调整；变更记录 05。
