package com.ccmonitor.mobile.core.claude.link

import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow

/** 给人看的那一句 ＋ 复制详情（核心写好，手机原样上屏）。 */
data class Said(
    val text: String,
    val detail: String,
)

/** 一台的常驻流此刻通不通（核心判，手机照画）。 */
sealed interface LinkState {
    data object Connecting : LinkState

    /** 通着。[notice] ＝ 通着但要人动手的那一句（那台后端旧了：照用对得上的那几条命令）；[fix] ＝ 核心点名的修法（旧了 ⇒ 更新）。 */
    data class Up(
        val notice: Said? = null,
        val fix: Fix? = null,
    ) : LinkState

    /** 断着：那一句；[needsHand] ⇒ 不会自己好（没装 · 起不了常驻 · 不是我们的后端），等［重试］；[fix] ⇒ 装一次会好、是哪一种。 */
    data class Down(
        val said: Said,
        val needsHand: Boolean,
        val fix: Fix? = null,
    ) : LinkState
}

/** 装一次会好的那一种（核心点名，手机只照它挑按钮字）：那台没有我们的后端 · 有、但旧了。 */
enum class Fix(
    val wire: String,
) {
    INSTALL("install"),
    UPDATE("update"),
    ;

    companion object {
        fun of(wire: String?): Fix? = entries.firstOrNull { it.wire == wire }
    }
}

/** 给一台装 / 更新后端这一趟走到哪了（话都是核心写好的）。 */
sealed interface Deploying {
    /** 在做：那一句；[bar] ⇒ 放字节那一步（屏上走一条进度条）。 */
    data class Step(
        val text: String,
        val bar: Boolean,
    ) : Deploying

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
