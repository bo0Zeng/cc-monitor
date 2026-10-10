package com.ccmonitor.mobile.core.terminal

import androidx.compose.ui.graphics.toArgb
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.getAndUpdate
import kotlinx.coroutines.flow.update
import org.connectbot.terminal.TerminalEmulator
import org.connectbot.terminal.TerminalEmulatorFactory
import java.io.ByteArrayOutputStream
import java.io.Closeable
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.Executors
import kotlin.concurrent.thread

/**
 * 粘性修饰键状态（extra-keys 行点 Ctrl/Alt → 修饰下一个软键盘按键；termlib 软键盘本身无 Ctrl/Alt）。
 * [shift] 只给自定义按钮组合键用，软键盘粘性路径恒为 false。
 */
data class KeyModifiers(
    val ctrl: Boolean = false,
    val alt: Boolean = false,
    val shift: Boolean = false,
) {
    val any: Boolean get() = ctrl || alt || shift
}

/**
 * 把粘性修饰应用到一次软键盘输入（纯函数）。只对单字节按键生效（不破坏 IME 组字/emoji/粘贴多字节）。
 * Shift = 小写字母→大写（'a'→'A'，非字母不动）；Ctrl+key = 标准控制码（仅字母与 `@ [ \ ] ^ _ 空格 ?`，见
 * [ctrlControlCode]，其余不变）；Alt/Meta = 前缀 ESC(0x1B)。叠加序：Shift→Ctrl→Alt。无修饰或多字节 → 原样。
 */
fun applyKeyModifiers(
    raw: ByteArray,
    mods: KeyModifiers,
): ByteArray {
    if (!mods.any || raw.size != 1) return raw
    var b = raw[0].toInt() and 0xFF
    if (mods.shift && b in 'a'.code..'z'.code) b -= 0x20 // Shift：小写字母→大写
    if (mods.ctrl) b = ctrlControlCode(b) ?: b
    return if (mods.alt) byteArrayOf(0x1B, b.toByte()) else byteArrayOf(b.toByte())
}

/**
 * Ctrl 组合的标准控制码映射（xterm 语义）：只有字母（大小写同码）与 `@ [ \ ] ^ _`（0x40..0x5F
 * 控制语义区）取 `& 0x1F`；`空格`→NUL（同 Ctrl+@，emacs set-mark 常用）、`?`→DEL(0x7F)。
 * 其余（数字/标点）没有标准 Ctrl 码 → null（原字节不变）。
 * 注意：不能对任意字节盲取 `& 0x1F`：Ctrl+1→0x11(XON)、Ctrl+3→0x13(XOFF) 会触发终端流控、无提示地冻结输出。
 */
private fun ctrlControlCode(b: Int): Int? =
    when {
        b in 'a'.code..'z'.code || b in 'A'.code..'Z'.code -> b and 0x1F
        b == '@'.code || b in '['.code..'_'.code -> b and 0x1F // @ [ \ ] ^ _
        b == ' '.code -> 0x00 // Ctrl+Space = NUL（与 Ctrl+@ 同码）
        b == '?'.code -> 0x7F // Ctrl+? = DEL
        else -> null
    }

/** `\e`/`\\`/`\n`/`\r`/`\t` 等简单转义 → 字节。 */
private val SIMPLE_ESCAPES = mapOf('e' to 0x1B, 'n' to 0x0A, 'r' to 0x0D, 't' to 0x09, '\\' to 0x5C)

/** 是否位置 i 起是一个 `\X` 转义（反斜杠后还有字符）。 */
private fun String.isEscapeAt(i: Int) = this[i] == '\\' && i + 1 < length

/** 处理位置 i 的 `\X` 转义，写入 [out]，返回新索引。识别不了 → 反斜杠字面（前进 1，X 后续按普通字符处理）。 */
private fun writeEscape(
    out: ByteArrayOutputStream,
    seq: String,
    i: Int,
): Int {
    val c = seq[i + 1]
    SIMPLE_ESCAPES[c]?.let {
        out.write(it)
        return i + 2
    }
    if (c == 'x') {
        val hex = seq.substring(i + 2, minOf(i + 4, seq.length))
        val v = hex.toIntOrNull(16)
        if (hex.length == 2 && v != null) {
            out.write(v)
            return i + 4
        }
    }
    out.write('\\'.code)
    return i + 1
}

/**
 * 转义序列 → 原始字节（转义类型按钮用，纯函数）。识别 `\e`/`\x1b`=ESC、`\xNN`=单字节 hex、
 * `\n`/`\r`/`\t`、`\\`；未识别的 `\X`、无反斜杠的字面串（如 `[D`）原样按 UTF-8 透传。
 */
fun escapeSequenceBytes(seq: String): ByteArray {
    val out = ByteArrayOutputStream(seq.length)
    var i = 0
    while (i < seq.length) {
        if (seq.isEscapeAt(i)) {
            i = writeEscape(out, seq, i)
        } else {
            val start = i
            while (i < seq.length && !seq.isEscapeAt(i)) i++ // 非转义运行整段按 UTF-8 透传，保代理对不劈半
            out.write(seq.substring(start, i).toByteArray(Charsets.UTF_8))
        }
    }
    return out.toByteArray()
}

/**
 * 自定义按钮的出线字节（纯函数）。
 * - 无修饰 → `command` 作命令行文本，末尾补 `\n`。
 * - 有修饰 → `command` 作单个基准键编码：命名键 tab/enter/esc/space（+ shift+Tab=backtab CSI Z），
 *   否则取首字符，经 [applyKeyModifiers] 加 Ctrl/Shift/Alt。组合键不补 `\n`（是按键，不是命令行）。
 * 基准键取首个码点、按 UTF-8 编码。非 ASCII 首字符没有单字节 chord 语义，多字节经 [applyKeyModifiers]
 * 的多字节守卫原样透传（不做 Ctrl/Alt 变换、不补回车，与 IME 多字节同规则）。
 */
fun customButtonBytes(
    command: String,
    mods: KeyModifiers,
): ByteArray {
    if (!mods.any) return (command + "\n").toByteArray(Charsets.UTF_8)
    resolveChordBytes(command, mods)?.let { return it }
    if (command.isEmpty()) return ByteArray(0)
    val first = command.substring(0, command.offsetByCodePoints(0, 1)) // 整码点（代理对不劈半）
    return applyKeyModifiers(first.toByteArray(Charsets.UTF_8), mods)
}

/** 命名键 → 基准字节（+ shift+Tab 的 backtab CSI Z 特例）。识别不了返回 null（交回首字符路径）。 */
private fun resolveChordBytes(
    command: String,
    mods: KeyModifiers,
): ByteArray? {
    val name = command.trim().lowercase()
    // 「仅 Shift」用 data class 相等表达。
    if (name == "tab" && mods == KeyModifiers(shift = true)) {
        return byteArrayOf(0x1B, '['.code.toByte(), 'Z'.code.toByte()) // Shift+Tab = CSI Z（backtab）
    }
    val base =
        when (name) {
            "tab" -> 0x09
            "enter", "return" -> 0x0D
            "esc", "escape" -> 0x1B
            "space" -> 0x20
            else -> return null
        }
    return applyKeyModifiers(byteArrayOf(base.toByte()), mods)
}

/**
 * SSH 交互 shell 流 ↔ termlib emulator 的桥。只收原始流 + resize lambda，不依赖 core-ssh。
 *
 * 线程模型（termlib 回调可能持非重入 native mutex）：
 * - reader 线程：阻塞 `input.read` → `emulator.writeInput`。独立线程、不在回调内，可安全入 native。
 * - onKeyboardInput 回调：只把字节投到单线程 `writeExecutor`，绝不在回调内写流或再入 native。
 * - onResize 回调：投到 writeExecutor → `onResizeRemote`（`changeWindowDimensions` 是网络写，可能阻塞）。
 */
class TerminalSession(
    private val input: InputStream,
    private val output: OutputStream,
    private val onResizeRemote: (cols: Int, rows: Int) -> Unit,
    rows: Int = 24,
    cols: Int = 80,
    palette: TerminalPalette = OneHalfDarkPalette,
    // 远端 shell 通道（core-ssh ShellChannel，以 [Closeable] 传入，core-terminal 不依赖 core-ssh）。
    // 由这个会话持有，[close] 时后台关闭。只关 input 不够：sshj ChannelInputStream.close 只是本地 EOF，
    // 不关通道、不通知远端，会留下孤儿远端 shell 与僵尸 tmux attach client。
    private val remoteChannel: Closeable? = null,
) : Closeable {
    @Volatile private var closed = false

    // SSH shell 流是否已断：reader 线程因 EOF/断开/close 退出即置 true，终端 pane 据此就地显示断线/重连。
    // 静默半开 TCP 靠 SSH keepalive 令 read 最终抛错 → reader 退出 → 这里翻 true。
    private val _streamClosed = MutableStateFlow(false)
    val streamClosed: StateFlow<Boolean> = _streamClosed.asStateFlow()

    // 粘性修饰键（Ctrl/Alt）：UI 点亮后修饰下一个软键盘按键，用后即清（one-shot）。StateFlow 供行高亮同步。
    private val _modifiers = MutableStateFlow(KeyModifiers())
    val modifiers: StateFlow<KeyModifiers> = _modifiers.asStateFlow()

    /** 切换粘性 Ctrl（点亮=下一个软键盘键组合 Ctrl）。 */
    fun toggleCtrl() = _modifiers.update { it.copy(ctrl = !it.ctrl) }

    /** 切换粘性 Alt（点亮=下一个软键盘键前缀 ESC/Meta）。 */
    fun toggleAlt() = _modifiers.update { it.copy(alt = !it.alt) }

    private val writeExecutor =
        Executors.newSingleThreadExecutor { r ->
            Thread(r, "ssh-write").apply { isDaemon = true }
        }

    /** 异步派发写任务。外层 runCatching 吞 close 竞态下的 RejectedExecutionException（不冒到主线程）。 */
    private fun submit(block: () -> Unit) {
        if (closed) return
        runCatching { writeExecutor.execute { runCatching(block) } }
    }

    val emulator: TerminalEmulator =
        TerminalEmulatorFactory
            .create(
                initialRows = rows,
                initialCols = cols,
                defaultForeground = palette.foreground,
                defaultBackground = palette.background,
                onKeyboardInput = { raw ->
                    // 原子地取当前粘性修饰并清空（getAndUpdate=CAS，不与 UI 线程 toggle 的 .update{} 竞争丢失一次点亮）；
                    // 用取到的旧值组合本次按键（one-shot）。
                    val mods = _modifiers.getAndUpdate { KeyModifiers() }
                    val bytes = applyKeyModifiers(raw, mods)
                    submit {
                        output.write(bytes)
                        output.flush()
                    }
                },
                onResize = { dims ->
                    submit { onResizeRemote(dims.columns, dims.rows) }
                },
            ).apply {
                applyColorScheme(
                    ansiColors = palette.toAnsiIntArray(),
                    defaultForeground = palette.foreground.toArgb(),
                    defaultBackground = palette.background.toArgb(),
                )
            }

    // reader 在 emulator 就绪后启动
    private val reader: Thread =
        thread(name = "ssh-reader", isDaemon = true) {
            val buf = ByteArray(8192)
            try {
                while (!closed) {
                    val n = input.read(buf)
                    if (n < 0) break
                    if (n > 0) emulator.writeInput(buf, 0, n)
                }
            } catch (_: Exception) {
                // 流关闭/断开 → 退出 reader
            } finally {
                _streamClosed.value = true // reader 退出（EOF/断开/被 close）→ 通知 UI 掉线
            }
        }

    /**
     * 发原始字节到 SSH（extra-keys / 粘贴）。与 onKeyboardInput 同路径：异步经 writeExecutor，
     * 绝不在调用线程同步写流/入 native。
     */
    fun send(bytes: ByteArray) {
        if (bytes.isEmpty()) return
        submit {
            output.write(bytes)
            output.flush()
        }
    }

    override fun close() {
        closed = true
        _streamClosed.value = true // 立即置断（reader finally 也会置，这里保证即时）
        // 关远端 shell 通道：sshj 的 channel close 是网络写（CHANNEL_CLOSE），而这个方法在 onCleared/retry 的主线程被调
        // （NetworkOnMainThreadException）→ 派发到守护线程。不走 writeExecutor：下方 shutdownNow() 会丢弃队列任务。
        // 关两次安全：ShellChannel.close 内部 runCatching，sshj close 幂等。
        closeRemoteChannelInBackground(remoteChannel)
        runCatching { input.close() } // 解阻塞 reader 的 read()
        reader.interrupt()
        writeExecutor.shutdownNow()
        // TerminalEmulator 无公开 close()（native 资源由 termlib impl 管理）；停流/线程即可。
    }
}

/**
 * 在后台守护线程关一个远端通道（fire-and-forget，异常吞掉，close 尽力而为）。
 * 抽成顶层函数以便 JVM 直测（[TerminalSession] 本体含 termlib 原生 emulator，纯 JVM 不可构造）。
 * @return 派发的线程（null=无通道），供测试 join 后断言 close 确已执行且不在调用线程。
 */
internal fun closeRemoteChannelInBackground(channel: Closeable?): Thread? =
    channel?.let { ch ->
        thread(name = "ssh-shell-close", isDaemon = true) { runCatching { ch.close() } }
    }
