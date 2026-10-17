package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.MachineFix
import com.ccmonitor.mobile.core.claude.link.wire.MachineFixButton
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow

/** 给人看的那一句 ＋ 复制详情（核心写好，手机原样上屏）。 */
data class Said(
    val text: String,
    val detail: String,
)

/** 一台的常驻流此刻通不通（核心判，手机照画）；那一句与修法取那台的状态成品（后端 `deploy::machine` 定）。 */
sealed interface LinkState {
    /** 部署那一趟走到哪（成品说正在问那台 · 正在装 · 正在更新）；没在装 ⇒ `null`。 */
    val busy: Busy? get() = null

    data object Connecting : LinkState

    /** 通着。[notice] ＝ 通着但要人动手的那一句（那台旧了 · 较新）；[fixes] ＝ 成品给的修法。 */
    data class Up(
        val notice: Said? = null,
        val fixes: List<MachineFixButton> = emptyList(),
        override val busy: Busy? = null,
    ) : LinkState

    /** 断着：那一句 ＋ 成品给的修法。 */
    data class Down(
        val said: Said,
        val fixes: List<MachineFixButton> = emptyList(),
        override val busy: Busy? = null,
    ) : LinkState
}

/** 正在装 / 更新那一句（成品写好的）；[bar] ⇒ 放字节那一段（屏上走一条进度条）。 */
data class Busy(
    val text: String,
    val bar: Boolean,
)

/** 装一次会好的那一颗（［安装］/［更新］：手机这一侧做得了的只有这两种）；成品没给 ⇒ `null`。 */
fun LinkState.deployFix(): MachineFixButton? {
    val fixes =
        when (this) {
            is LinkState.Up -> fixes
            is LinkState.Down -> fixes
            LinkState.Connecting -> emptyList()
        }
    return fixes.firstOrNull { it.fix == MachineFix.INSTALL || it.fix == MachineFix.UPDATE }
}

/** 给一台装 / 更新后端这一趟的结局（话都是核心写好的；走到哪一步在成品里，见 [LinkState.busy]）。 */
sealed interface Deploying {
    /** 做完了：那一句（装上了 · 已是这一版 · 那台不比这一版旧）。 */
    data class Done(
        val text: String,
    ) : Deploying

    /** 没做成：那一句 ＋ 复制详情。 */
    data class Failed(
        val said: Said,
    ) : Deploying
}

/** 一台的常驻流对外的那几样：会话表 · 记录行 · 一问一答 · 接上第几回。聊天屏只经它（测试给假的）。 */
interface CoreLink {
    /** 此刻通不通（核心判的）：断着 ⇒ 那台离线，屏上那几颗要连着那台的按钮都灰。 */
    val state: StateFlow<LinkState>
    val table: StateFlow<SessionTable>
    val lines: SharedFlow<RecordLine>

    /** 流上的活卡帧（只有名单里的会话）。可丢：丢了一帧那张卡接不上、先不画，等那台下一张整卡。 */
    val live: SharedFlow<LiveFrame>

    /**
     * 接上第几回（还没接上过 ⇒ 0）。值变了 ＝ 换了一条流：之前在路上的问答作废（回过 [Reply.LinkDown]），
     * 断着那一段的 `line` 帧没收到 ⇒ 读过的正文要从断处补读。
     */
    val epoch: StateFlow<Long>

    /**
     * 一问一答。流没通（在建 · 在重接 · SSH 还没通）⇒ 先等它接上（至多 [withinMs]，等的那一段也算在里面）再问；
     * 走不通、要人动手的那几种 ⇒ 当场 [Reply.LinkDown]。期限到了核心补发撤单（那条命令撤不动的，那台握手时说过，核心照它不补）。
     */
    suspend fun call(
        cmd: String,
        args: Map<String, Any?>? = null,
        withinMs: Long = DEFAULT_CALL_MS,
    ): Reply

    /**
     * 分块传（循环在核心里，桌面同一份）：[fd] 现开本机那一份、交出描述符（核心用完就关；开不了就抛 [java.io.IOException]）。
     * 每一块等至多 [withinMs]，走到哪经 [moved]（已过字节 · 总数，不知道 ⇒ `-1`）；结局同 [call]。
     * [Transfer.Stage]：送进那台的暂存区，成了 ⇒ `{key, chunks, bytes, sha256}`；[Transfer.Pull]：读回写进本机那一份，成了 ⇒ `{bytes}`。
     */
    suspend fun transfer(
        how: Transfer,
        fd: () -> Int,
        withinMs: Long,
        moved: (Long, Long) -> Unit,
    ): Reply

    /** 一组一条命令走到哪（全部编号的都在这一条流上；按 [BatchEvent.id] 认自己那一组）。 */
    val batches: SharedFlow<BatchEvent>

    /**
     * 落过盘、这一趟开起来接回来的那几组（[BatchEvent.kept]）各自最后那一格：屏后开也照画。
     * 收场的那一格只交一次（交过 · 收场时 [batches] 上有人在看 ⇒ 不再交）。
     */
    fun resumed(): List<BatchEvent>

    /**
     * 起一组（全过程与字在核心里，桌面同一份）：[want] 是要做什么 —— `delete {root, items [{rel, name, dir}]}` ·
     * `chmod {root, items [{rel, name}], mode}` · `copy {from, names, to}` · `retry id` · `undo id`，入参由核心拼。
     * 回编号；这台不在 ⇒ 0。
     */
    suspend fun batch(want: Map<String, Any?>): Long

    /** 同名那一问的答（`null` ＝ 取消）。 */
    fun batchAnswer(
        id: Long,
        answer: BatchAnswer?,
    )

    /** 撤那一组（「停」）。 */
    fun batchStop(id: Long)

    /** 文件页选中这几项（[FilesLs.Entry.row]，那台交来的原行）此刻能做什么（核心照此刻那一台判：断着 · 那台做不到的命令）。 */
    fun picks(rows: List<Any?>): PickMenu

    /** 一问一答撞了同名（码 `exists` 的 `data`）⇒ 抽屉那几行（说明由核心写，一组那一问同一份）；认不出 ⇒ 空。 */
    fun clashes(data: Any?): List<BatchEvent.Clash>

    /** 手机这一侧问的同名（下载 · 上传）那一答 ⇒ [keys] 那几行每个键怎么做（`overwrite` · `skip` · `keep_both`，核心认：逐项没点的照跳过）；取消 ⇒ `null`。 */
    fun decided(
        keys: List<String>,
        answer: BatchAnswer?,
    ): Map<String, String>?

    /**
     * 下载一组那几格（逐项拉在手机这一圈里走，字在核心）：`{id, machine, names, phase, …}` ⇒ 一格（[BatchEvent]）；读不动 ⇒ `null`。
     * `phase`：`asking`（`here` 本机文件夹里已有的名字）· `running`（`done` · `part` · `current`）· `cut` · `ended`（`outcomes` 逐项结局）。
     */
    fun downloadEvent(want: Map<String, Any?>): BatchEvent?
}

/** 分块传哪一半（[CoreLink.transfer]）。 */
sealed interface Transfer {
    /** 上传那一半；[name] 是本机那一份在句子里的名字。 */
    data class Stage(
        val name: String,
    ) : Transfer

    /** 下载：那台上的 [path] 读回；[shown] 是本机落点在句子里的名字。 */
    data class Pull(
        val path: RemotePath,
        val shown: String,
    ) : Transfer
}

/** 一问一答默认等多久。 */
const val DEFAULT_CALL_MS: Long = 15_000L

/** 一条会话「此刻有人在看」：看着的那一处拿一个 [hold]，不看了关掉；刚进名单那一格的 `from` 经 [from] 来。 */
interface SessionWatch {
    val from: SharedFlow<WatchFrom>

    fun hold(sid: String): AutoCloseable
}

/**
 * 核心写好的失败：`code` 给程序认，`message` 原样上屏，`detail` 进复制详情（时刻 · 机器 · 命令 · 码 · 原话都在里面）。
 * 帧面 `reply` 与一次性 CLI 的 stderr 信封是同一份（`IPC-PROTOCOL.md` §4 · §8）。
 */
data class CoreFailure(
    val code: String,
    val message: String,
    val detail: String,
    val data: Any? = null,
)

/** 一条入方向命令的结局（核心交回来的那一行，`IPC-PROTOCOL.md` §4）。 */
sealed interface Reply {
    /** `ok: true`；`data` 是命令的返回值（无返回值 ⇒ `null`）。 */
    data class Ok(
        val data: Any?,
    ) : Reply

    /** `ok: false`：核心写好的失败（`message` 上屏，`detail` 进复制详情）。 */
    data class Failed(
        val failure: CoreFailure,
    ) : Reply

    /** 到了期限（核心已补发撤单）。 */
    data object TimedOut : Reply

    /** 后端回 `cancelled`。 */
    data object Cancelled : Reply

    /** 流断了 · 没接上。 */
    data object LinkDown : Reply

    /** 回包缺必填格（两端契约对不上）：原样带回。 */
    data class Unreadable(
        val raw: String,
    ) : Reply
}
