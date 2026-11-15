# 帧形一览

由 `devsched-wire` 的类型生成，别手改；改了帧之后重写：`DEVSCHED_WIRE_BLESS=1 cargo test -p devsched-wire --test frames_md`。

协议指纹：`696a8f6e4e907102`

一条连接（标准输入 / 输出）上一行一个 JSON 对象，`t` 是帧名。类型里 `T|null` 是可以不填的格。

## hello

节点 → 调度端。连上先发的一帧；`proto` 与调度端的指纹不同 ⇒ 调度端不交作业。

| 键 | 类型 | 说明 |
|---|---|---|
| `caps` | array | 能力：名字固定（cargo · tmux · tmuxnew · node · gui · jdk · android · rsync） |
| `caps[]` | object | 每一项 |
| `caps[].name` | str | 能力名 |
| `caps[].version` | str | 探到的版本，原样 |
| `cargo_config` | array | 从 `<根>/projects` 往上各层会被 cargo 读到的 `.cargo/config.toml` · `.cargo/config`（根在家目录下时那台家目录的配置会生效） |
| `cargo_config[]` | str | 每一项 |
| `heartbeat_timeout_s` | int | 心跳时限，秒 |
| `programs` | array | PATH 里查到的程序（sccache · cc 等），供调度端核指向程序的变量 |
| `programs[]` | str | 每一项 |
| `proto` | str | 协议指纹（本文件头上那个） |
| `root` | str | 节点的登记根（`--root`） |
| `version` | str | 节点版本 |

## facts

节点 → 调度端。资源事实：hello 之后发一次，此后每收到一次 beat 回一次；量不到的格是 null。

| 键 | 类型 | 说明 |
|---|---|---|
| `disk` | int\|null | 根目录所在盘剩余，MiB |
| `jobs` | int | 这台此刻在跑几趟 |
| `mem` | int\|null | 可用内存，MiB |
| `nproc` | int\|null | RLIMIT_NPROC |
| `ours` | int\|null | 节点名下（连节点自己）的线程数 |
| `psi` | float\|null | 内存压力 some avg10，百分数 |
| `threads` | int\|null | 本账号此刻的线程数 |
| `tmp` | int\|null | /tmp 剩余，MiB |

## job

调度端 → 节点。交作业：节点在 `<根>/projects/<project>/<tree>/<rel>` 起 `argv`，环境 ＝ 节点自己设的三类 ＋ `env`。

| 键 | 类型 | 说明 |
|---|---|---|
| `argv` | array | 命令数组 |
| `argv[]` | str | 每一项 |
| `env` | map<str,str> | 带过去的环境（名 → 值，空值照设） |
| `job` | int | 号（一条连接上不重） |
| `nice` | int\|null | nice 值 |
| `project` | str | 项目名 |
| `rel` | str | 树里的相对目录（空 ＝ 树根） |
| `stop` | object | 停线；没写的那条不看 |
| `stop.disk` | int\|null | 根目录所在盘剩余低于它（MiB） |
| `stop.mem` | int\|null | 可用内存低于它（MiB） |
| `stop.psi` | float\|null | 内存压力不低于它（百分数） |
| `tree` | str | 树名 |

## out

节点 → 调度端。作业的一截输出（不是 UTF-8 的字节按替换字符写）。

| 键 | 类型 | 说明 |
|---|---|---|
| `data` | str | 这一截 |
| `job` | int | 号 |
| `stream` | str：stderr · stdout | 哪一路 |

## exit

节点 → 调度端。作业收尾：只有 `stop` = done 时 `code` 是命令自己的退出码。

| 键 | 类型 | 说明 |
|---|---|---|
| `breach` | null\|str：disk · mem · psi | stop = breach 时撞的哪条，与 job.stop 的键逐字一样 |
| `code` | int\|null | 退出码；命令被信号打死的折成 128 ＋ 信号；stop ≠ done 的是 null |
| `job` | int | 号 |
| `skipped` | array | 作业自己报的没跑的段 |
| `skipped[]` | str | 每一项 |
| `stop` | str：breach · done · heartbeat · spawn · stopped · threads · tmp_path | 停因：done 跑完 · stopped 停作业 · breach 撞停线 · threads 线程上限 · spawn 起不了进程 · heartbeat 心跳断 · tmp_path 临时目录路径超长 |

## stop

调度端 → 节点。停作业：整组停掉，回 exit（stop = stopped）。

| 键 | 类型 | 说明 |
|---|---|---|
| `job` | int | 号 |

## beat

调度端 → 节点。心跳，调度端每 5 秒一次；节点超过 hello 报的时限收不到 ⇒ 停掉这条连接上的作业。

没有别的键。

## clean

调度端 → 节点。删 `<根>/projects/<project>/<tree>` 与这棵树的产物目录。

| 键 | 类型 | 说明 |
|---|---|---|
| `project` | str | 项目名 |
| `tree` | str | 树名 |

## error

两头都发。说不成一帧回话的错；说得上是哪一趟的带 `job`。

| 键 | 类型 | 说明 |
|---|---|---|
| `job` | int\|null | 号；说不上的是 null |
| `msg` | str | 说明 |
