package com.ccmonitor.mobile.core.terminal

import androidx.compose.ui.graphics.toArgb
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.getAndUpdate
import kotlinx.coroutines.flow.update
import org.connectbot.terminal.TerminalEmulator
import org.connectbot.terminal.TerminalEmulatorFactory
import java.io.Closeable
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.Executors
import kotlin.concurrent.thread

/** 一次按下实际带上的修饰（软键盘与键条取同一份，见 [ModifierLatch.take]）。 */
data class KeyModifiers(
    val ctrl: Boolean = false,
    val alt: Boolean = false,
    val shift: Boolean = false,
) {
    val any: Boolean get() = ctrl || alt || shift
}

enum class Mod { CTRL, ALT, SHIFT }

/** 修饰键的样子：点亮一次 · 锁住 · 按住期间。 */
enum class ModState { OFF, ONCE, LOCKED, HELD }

/** 键条上修饰键的手势：单击 · 双击锁住 · 长按按下 · 长按松开。 */
enum class ModGesture { TAP, LOCK, HOLD, RELEASE }

/** 三颗修饰键各自的底态（[base]：OFF / ONCE / LOCKED）与是否按住着。 */
data class ModStates(
    private val base: Map<Mod, ModState> = emptyMap(),
    private val held: Set<Mod> = emptySet(),
) {
    fun of(m: Mod): ModState =
        when {
            base[m] == ModState.LOCKED -> ModState.LOCKED
            m in held -> ModState.HELD
            else -> base[m] ?: ModState.OFF
        }

    internal fun on(
        m: Mod,
        g: ModGesture,
    ): ModStates =
        when (g) {
            ModGesture.TAP -> copy(base = base + (m to if ((base[m] ?: ModState.OFF) == ModState.OFF) ModState.ONCE else ModState.OFF))
            ModGesture.LOCK -> copy(base = base + (m to ModState.LOCKED))
            ModGesture.HOLD -> copy(held = held + m)
            ModGesture.RELEASE -> copy(held = held - m)
        }

    internal val effective: KeyModifiers
        get() = KeyModifiers(ctrl = of(Mod.CTRL) != ModState.OFF, alt = of(Mod.ALT) != ModState.OFF, shift = of(Mod.SHIFT) != ModState.OFF)

    /** 带过一次：点亮一次的熄掉，锁住与按住的不动。 */
    internal fun used(): ModStates = copy(base = base.filterValues { it != ModState.ONCE })
}

/**
 * 修饰键三态：单击点亮、下一次按下带上后自己熄（再单击一下也熄）；双击锁住、再单击一下解开；长按按住期间一直带。
 * 软键盘的输入与键条上的键都经 [take] 取，所以两边都算。
 */
class ModifierLatch {
    private val _states = MutableStateFlow(ModStates())
    val states: StateFlow<ModStates> = _states.asStateFlow()

    fun on(
        m: Mod,
        g: ModGesture,
    ) = _states.update { it.on(m, g) }

    /** 这一下带上的修饰；点亮一次的随之熄掉（原子地取并清，不与界面线程的 [on] 互相丢）。 */
    fun take(): KeyModifiers = _states.getAndUpdate { it.used() }.effective
}

/**
 * 把修饰应用到一次软键盘输入（纯函数）。只对单字节按键生效（不破坏 IME 组字/emoji/粘贴多字节）。
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
    palette: TerminalPalette,
    // 远端 shell 通道（core-ssh ShellChannel，以 [Closeable] 传入，core-terminal 不依赖 core-ssh）。
    // 由这个会话持有，[close] 时后台关闭。只关 input 不够：输入流 close 只是本地 EOF，
    // 不关通道、不通知远端，会留下孤儿远端 shell 与僵尸 tmux attach client。
    private val remoteChannel: Closeable? = null,
) : Closeable {
    @Volatile private var closed = false

    // SSH shell 流是否已断：reader 线程因 EOF/断开/close 退出即置 true，终端 pane 据此就地显示断线/重连。
    // 静默半开 TCP 靠 SSH keepalive 令 read 最终抛错 → reader 退出 → 这里翻 true。
    private val _streamClosed = MutableStateFlow(false)
    val streamClosed: StateFlow<Boolean> = _streamClosed.asStateFlow()

    /** 修饰键（Ctrl · Alt · Shift）：键条上点亮 / 锁住 / 按住，软键盘与键条的下一下都带上。 */
    val modifiers = ModifierLatch()

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
                    val bytes = applyKeyModifiers(raw, modifiers.take())
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
        // 关远端 shell 通道：channel close 是网络写（CHANNEL_CLOSE），而这个方法在 onCleared/retry 的主线程被调
        // （NetworkOnMainThreadException）→ 派发到守护线程。不走 writeExecutor：下方 shutdownNow() 会丢弃队列任务。
        // 关两次安全：ShellChannel.close 幂等。
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
