package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.BatchAnswer
import com.ccmonitor.mobile.core.wire.gen.BatchLine
import com.ccmonitor.mobile.core.wire.gen.ClashRow
import com.ccmonitor.mobile.core.wire.gen.Decide
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.LiveCards
import com.ccmonitor.mobile.core.wire.gen.MachineFix
import com.ccmonitor.mobile.core.wire.gen.MachineFixButton
import com.ccmonitor.mobile.core.wire.gen.MachineState
import com.ccmonitor.mobile.core.wire.gen.MachineStateKind
import com.ccmonitor.mobile.core.wire.gen.PhoneMenu
import com.ccmonitor.mobile.core.wire.gen.SessionBook
import com.ccmonitor.mobile.core.wire.gen.SessionBranch
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow

/** 正在装 / 更新那一句（成品写好的）；[bar] ⇒ 放字节那一段（屏上走一条进度条）。 */
data class Busy(
    val text: String,
    val bar: Boolean,
)

/*
 * 一台此刻怎样就是核心交的那一行（契约里的 [LinkLine]：通没通 ＋ 那台的状态成品 [MachineState]，后端 `deploy::machine` 定）；
 * 手机不另抄一份，下面几样只从成品里挑格。`null` ＝ 核心还没交过那一行（当正在连）。流自己在接（[LinkLine.Connecting]）时不另说。
 */

/** 部署那一趟走到哪（成品说正在问那台 · 正在装 · 正在更新）；没在装 ⇒ `null`。 */
fun LinkLine?.busy(): Busy? {
    val m = machineSaying() ?: return null
    return when (m.state) {
        MachineStateKind.INSTALLING, MachineStateKind.UPDATING -> Busy(m.said, bar = true)
        // 流还通着 / 断着、成品已在连 ⇒ 部署那一趟在问那台 · 在换常驻。
        MachineStateKind.CONNECTING -> Busy(m.said, bar = false)
        else -> null
    }
}

/** 装一次会好的那一颗（［安装］/［更新］：手机这一侧做得了的只有这两种）；成品没给 ⇒ `null`。 */
fun LinkLine?.deployFix(): MachineFixButton? = machineSaying()?.fixes?.firstOrNull { it.fix == MachineFix.INSTALL || it.fix == MachineFix.UPDATE }

/** 流通着或断着时那台的成品；正在连 · 还没交过 ⇒ `null`。 */
private fun LinkLine?.machineSaying(): MachineState? = if (this is LinkLine.Up || this is LinkLine.Down) machine else null

/** 给一台装 / 更新后端这一趟的结局（话都是核心写好的；走到哪一步在成品里，见 [busy]）。 */
sealed interface Deploying {
    /** 做完了：那一句（装上了 · 已是这一版 · 那台不比这一版旧）。 */
    data class Done(
        val text: String,
    ) : Deploying

    /** 没做成：交出去那一形（码 · 那一句 · 复制详情，核心写好、手机原样上屏）。 */
    data class Failed(
        val failed: com.ccmonitor.mobile.core.wire.gen.Failed,
    ) : Deploying
}

/** 一台的常驻流对外的那几样：会话簿 · 记录行 · 一问一答 · 接上第几回。聊天屏只经它（测试给假的）。 */
interface CoreLink {
    /** 核心交的那一行（通没通 ＋ 那台的状态成品）：断着 ⇒ 屏上那几颗要连着那台的按钮都灰；核心还没交过 ⇒ `null`。 */
    val state: StateFlow<LinkLine?>

    /** 那台离线那一句（那台状态成品里连接层写好的，出口照摆）；成品还没到 ⇒ `null`（未知不当离线）。 */
    val offline: StateFlow<String?>

    /** 这台的会话簿：核心折好的整份（`face_core::book`，桌面同一份折法）。 */
    val table: StateFlow<SessionBook>
    val lines: SharedFlow<RecordLine>

    /** 活卡：某会话某运行此刻整份的卡（核心折好，只有名单里的会话）。可丢：丢了一格，下一格就是那一处的整份。 */
    val live: SharedFlow<LiveCards>

    /** 流上的 `session_branch`：一个会话的主线外清单的一截（「前 `at` 条照旧，其后换成 `off`」，段照 `runs`），只有名单里要了清单的会话。 */
    val branches: SharedFlow<SessionBranch>

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
     * [Transfer.Stage]：送进那台的暂存区，成了 ⇒ `{key, chunks, bytes, sha256}`；
     * [Transfer.Pull]：读进内存（[fd] 不调），成了 ⇒ `{bytes, content, undecodable}`（[Transfer.Pull.of] 解）。
     */
    suspend fun transfer(
        how: Transfer,
        fd: () -> Int,
        withinMs: Long,
        moved: (Long, Long) -> Unit,
    ): Reply

    /** 一组一条命令走到哪（全部编号的都在这一条流上；按 [BatchLine.id] 认自己那一组）。 */
    val batches: SharedFlow<BatchLine>

    /** 核心里此刻有活在走（一组 · 一趟分块传，核心交的 `busy`）：保活服务照它判，切到后台活不断。 */
    val working: StateFlow<Boolean>

    /**
     * 落过盘、这一趟开起来接回来的那几组（[BatchLine.kept]）各自最后那一格：屏后开也照画。
     * 收场的那一格只交一次（交过 · 收场时 [batches] 上有人在看 ⇒ 不再交）。
     */
    fun resumed(): List<BatchLine>

    /**
     * 起一组（全过程与字在核心里，桌面同一份）：[want] 是要做什么 —— `delete {root, items [{rel, name, dir}]}` ·
     * `chmod {root, items [{rel, name}], mode}` · `copy {from, names, to}` ·
     * `download {folder, items [{path, name}], here}`（下到本机 [folder] 那一棵：`folder` 原样一串，每项要本机那一份时经 [LocalSaves] 开）·
     * `retry id` · `undo id`，入参由核心拼。回编号；这台不在 ⇒ 0。
     */
    suspend fun batch(want: Map<String, Any?>): Long

    /** 同名那一问的答（`null` ＝ 取消）。 */
    fun batchAnswer(
        id: Long,
        answer: BatchAnswer?,
    )

    /** 撤那一组（「停」）。 */
    fun batchStop(id: Long)

    /** 文件页选中这几项（[FilesLs.Entry.row]，那台交来的原行）此刻能做什么（核心照此刻那一台判：断着 · 那台做不到的命令）；核心那一行认不出 ⇒ `null`。 */
    fun picks(rows: List<Any?>): PhoneMenu?

    /** 一问一答撞了同名（码 `exists` 的 `data`）⇒ 抽屉那几行（说明由核心写，一组那一问同一份）；认不出 ⇒ 空。 */
    fun clashes(data: Any?): List<ClashRow>

    /** 手机这一侧问的同名（上传）那一答 ⇒ [keys] 那几行每个键怎么做（`overwrite` · `skip` · `keep_both`，核心认：逐项没点的照跳过）；取消 ⇒ `null`。 */
    fun decided(
        keys: List<String>,
        answer: BatchAnswer?,
    ): Map<String, Decide>?

    /** 断开这一条（有的话）、马上整份重来：停在要人动手那一步的（没装 · 起不来）也重拨。 */
    fun reconnect()
}

/**
 * 一台的［重试］（会话列表问题条 · 文件页那一屏走这同一条）：流断着 ⇒ 整份重接（停在要人动手那一步的也重拨；
 * 接上了 [CoreLink.epoch] · [CoreLink.state] 跟着变，看着它的那一处重问）；通着 ⇒ [ask] 重问。
 */
fun CoreLink.retry(ask: () -> Unit) = if (state.value is LinkLine.Down) reconnect() else ask()

/** 分块传哪一半（[CoreLink.transfer]）。 */
sealed interface Transfer {
    /** 上传那一半；[name] 是本机那一份在句子里的名字。 */
    data class Stage(
        val name: String,
    ) : Transfer

    /** 拉进内存那一半：那台的 [path] 逐块读回、过 [cap]（门给的上限）就停；不碰本机文件（[CoreLink.transfer] 的 `fd` 不开）。 */
    data class Pull(
        val path: RemotePath,
        val cap: Long,
    ) : Transfer {
        /** 读回的那一份：原样字节 ＋ 本机解不开时屏上那一句（核心写的）。 */
        class Got(
            val bytes: ByteArray,
            val undecodable: String,
        )

        companion object {
            /** 成了的回包 `{bytes, content: {b16}, undecodable}` ⇒ [Got]；形状不对 ⇒ `null`。 */
            fun of(data: Any?): Got? {
                val m = data as? Map<*, *> ?: return null
                val bytes = ((m["content"] as? Map<*, *>)?.get("b16") as? String)?.let(::unhex) ?: return null
                val undecodable = m["undecodable"] as? String ?: return null
                return Got(bytes, undecodable)
            }
        }
    }
}

/** 一问一答默认等多久。 */
const val DEFAULT_CALL_MS: Long = 15_000L

/** 一条会话「此刻有人在看」：看着的那一处拿一个 [hold]，不看了关掉；刚进名单那一格的 `from` 经 [from] 来。 */
interface SessionWatch {
    val from: SharedFlow<WatchFrom>

    fun hold(sid: String): AutoCloseable
}

/** 一条入方向命令的结局（核心交回来的那一行，`IPC-PROTOCOL.md` §4）。 */
sealed interface Reply {
    /** `ok: true`；`data` 是命令的返回值（无返回值 ⇒ `null`）。 */
    data class Ok(
        val data: Any?,
    ) : Reply

    /**
     * `ok: false`：核心写好的失败（线上那一形 [com.ccmonitor.mobile.core.wire.gen.Failed]）：`code` 给程序认，`said` 原样上屏，
     * `detail` 进复制详情（时刻 · 机器 · 命令 · 码 · 原话都在里面）。帧面 `reply` 与一次性 CLI 的 stderr 信封是同一份（`IPC-PROTOCOL.md` §4 · §8）。
     */
    data class Failed(
        val failure: com.ccmonitor.mobile.core.wire.gen.Failed,
    ) : Reply

    /** 到了期限（核心已补发撤单）：[said] 原样上屏、[detail] 进复制详情，都是核心照通道那一份写好的（桌面同一句）。 */
    data class TimedOut(
        val said: String,
        val detail: String,
    ) : Reply

    /** 撤了（后端回 `cancelled` · 这边不等了）：[said] · [detail] 同 [TimedOut]。 */
    data class Cancelled(
        val said: String,
        val detail: String,
    ) : Reply

    /** 流断了 · 没接上。 */
    data object LinkDown : Reply

    /** 回包缺必填格（两端契约对不上）：原样带回。 */
    data class Unreadable(
        val raw: String,
    ) : Reply
}
