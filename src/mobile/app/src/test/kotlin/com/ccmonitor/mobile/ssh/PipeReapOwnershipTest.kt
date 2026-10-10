package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.UUID

/**
 * 收口的凭据闸：只认我们自己名字空间里的那些。
 *
 * 守的是 [PipeLauncher.isOurPipeSessionName] 与 [PipeLauncher.reapCommandFor]。
 *
 * 判别力边界：下面每一条都是纯函数的判据，钉的是「这个名字闸放不放行」「放行时给出哪条命令串」，
 * 没有一条证明远端真的少了一条 tmux 会话；自动收口也没有接扳机（理由写在 [PipeLauncher.reapCommandFor] 的头注里）。
 *
 * | 断言 | 它要防的 bug | 造出来会红吗 |
 * |---|---|---|
 * | [everyNameTheLauncherCanMintPassesTheGate] | 闸收得太紧 ⇒ 连我们自己起的都不认 ⇒ 收口永远收不到任何东西（一个「永远不干活」的闸，而且看起来很安全） | 会：把 ③ 换成 `== PIPE_SESSION_PREFIX` 当场红 |
 * | [theCcSuffixOverlapIsRefused] | 照抄起管道那条路的 `isValidSessionId` ⇒ `atermpipe-<X>-cc` 过闸 ⇒ 碰到 `cc-*` 那一层 | 会：把 ③ 换成 `isValidSessionId` 当场红 |
 * | [noNameThisGateAcceptsIsEverClaimedByTheCcNamespace] | 同上，但从正面扫：闸放行的集合与后端谓词认领的集合必须不相交 | 会 |
 * | [namesWeDidNotMintAreRefused] | 只判前缀 ⇒ 别人借我们前缀随手起的名也被当成自己的 | 会 |
 * | [reapCommandForYieldsNothingForEveryRefusedName] | 反向判据：闸拒了，却还是把命令发出去 | 会：把 `reapCommandFor` 改成无条件返回 `killCommand` 当场红 |
 * | [reapCommandForRoutesThroughTheBackendAndNamesOnlyThatSession] | 自己拼 `tmux kill-session`，或把 target 拼错成别的会话 | 会 |
 *
 * 刻意不写的：
 * - 「断言 `isOurPipeSessionName("atermpipe-<uuid>")` 为真」单独成立：把闸写成「只判前缀」它照样绿，
 *   无判别力。判别力全在 [namesWeDidNotMintAreRefused] 那一侧。
 * - 「断言 `reapCommandFor` 返回非 null」而不比命令串本身：返回一条指向别的会话的
 *   kill 命令时那个断言照样绿，而那正是最坏的 bug。
 */
class PipeReapOwnershipTest {
    /**
     * 闸必须放行我们自己起的那些，否则它是个永远不干活的闸。
     *
     * 名字只从 [PipeLauncher.tmuxNameFor] 来（造名的唯一产地），sid 用生产上真会出现的那两种来源的形状：
     * `ChatRoute.newConversationId()`（随机 UUID）与 Claude 自己的会话编号（也是 UUID）。
     */
    @Test
    fun everyNameTheLauncherCanMintPassesTheGate() {
        val sids =
            listOf(
                "0f9d3c2a-1111-4222-8333-444455556666",
                "deadbeef-dead-4eef-8eef-deadbeefdead",
                freshUuid(),
                freshUuid().uppercase(), // UUID 大小写都该认
            )
        for (sid in sids) {
            // 前提：这个 sid 真的是生产上交得出去的形状，不先证明这个，下面那条可能因别的原因通过
            assertTrue("前提：sid 得是合法 UUID，否则生产上根本起不来：$sid", ClaudeInvocation.isValidUuid(sid))
            val name = PipeLauncher.tmuxNameFor(sid)
            assertTrue("自己起的那条必须过闸，否则收口永远收不到东西：$name", PipeLauncher.isOurPipeSessionName(name))
        }
    }

    /**
     * 那个真实存在的交集名必须被拒。
     *
     * `atermpipe-proj-cc` 同时满足三件事：① 带我们的前缀、② `isValidSessionId` 为真、
     * ③ 被 `TmuxCommands.isCcmTmuxName`（后端那一层的谓词）认领。
     * 闸若照抄起管道那条路的 `isValidSessionId`，它就会放行一个 `cc-*` 名字，碰到主机上的 `cc-*` tmux 会话。
     *
     * 前两句断言是前提证明：不先证明这个交集真的存在，第三句就只是一条
     * 「随便一个怪名字被拒了」的弱断言，把闸写成 `isValidSessionId` 它也不会红。
     */
    @Test
    fun theCcSuffixOverlapIsRefused() {
        val overlap = "${PipeLauncher.PIPE_SESSION_PREFIX}proj-cc"

        // 前提①：照抄起管道那条路的写法会认为这个 sid 合法
        assertTrue("前提：`proj-cc` 过得了起管道那条路的白名单", ClaudeInvocation.isValidSessionId("proj-cc"))
        // 前提②：而这个名字确实被后端那一层认领：交集是真的
        assertTrue("前提：`$overlap` 真的被 `isCcmTmuxName` 认领（`<X>-cc` 后缀那一支）", TmuxCommands.isCcmTmuxName(overlap))

        assertFalse("它被后端那一层认领 ⇒ 闸必须拒；放行它就是在碰 `cc-*`", PipeLauncher.isOurPipeSessionName(overlap))
        assertNull("连命令都不许给", PipeLauncher.reapCommandFor(TmuxBackend, overlap))
    }

    /**
     * 从正面扫：闸放行的集合，与 `cc-*` 那一层认领的集合，不相交。
     *
     * [theCcSuffixOverlapIsRefused] 钉的是那一个已知的交集名；本条钉的是性质：
     * 「凡闸放行的，后端谓词一律不认领」。这条是「结构上不可能碰到 `cc-*`」那句话的判据。
     */
    @Test
    fun noNameThisGateAcceptsIsEverClaimedByTheCcNamespace() {
        val corpus =
            buildList {
                repeat(ACCEPT_SAMPLES) { add(PipeLauncher.tmuxNameFor(freshUuid())) }
                addAll(REFUSED_NAMES)
                addAll(listOf("proj-cc", "proj-cc-2", "cc-abc12345").map { PipeLauncher.PIPE_SESSION_PREFIX + it })
            }
        // 前提：语料里真的有被放行的，否则本条恒绿
        assertTrue("前提：语料里得有过闸的", corpus.any { PipeLauncher.isOurPipeSessionName(it) })
        // 前提：语料里真的有被后端认领的，否则本条也恒绿
        assertTrue("前提：语料里得有被 `isCcmTmuxName` 认领的", corpus.any { TmuxCommands.isCcmTmuxName(it) })

        val both = corpus.filter { PipeLauncher.isOurPipeSessionName(it) && TmuxCommands.isCcmTmuxName(it) }
        assertEquals("这两个名字空间必须不相交，交集里出现的每一个都是一次潜在的误杀", emptyList<String>(), both)
    }

    /**
     * 不是我们造得出来的名字，一律拒。
     *
     * 这一条承载本文件的全部判别力：少了它，闸写成「只判前缀」甚至「恒真」都全绿。
     */
    @Test
    fun namesWeDidNotMintAreRefused() {
        for (name in REFUSED_NAMES) {
            assertFalse("不该放行：`$name`", PipeLauncher.isOurPipeSessionName(name))
        }
    }

    /**
     * 反向判据：闸拒掉的每一个名字，`reapCommandFor` 必须一条命令都不给。
     *
     * 闸与命令分两步时的经典 bug 是「判了但没按判断走」。本条把它堵死：
     * 拒绝面上的返回值是 `null`，不是「一条无害的命令」，也不是「空串」。
     */
    @Test
    fun reapCommandForYieldsNothingForEveryRefusedName() {
        // 前提：拒绝面非空，否则本条恒绿
        assertTrue("前提：得真有被拒的名字", REFUSED_NAMES.isNotEmpty())
        for (name in REFUSED_NAMES) {
            assertNull("闸拒了却还给出命令 —— 这就是误杀的那一步：`$name`", PipeLauncher.reapCommandFor(TmuxBackend, name))
        }
    }

    /**
     * 放行时给出的那条命令：经 [SessionBackend]，且 target 恰是那条会话。
     *
     * 两半都要断：
     * - 「经后端」：tmux 命令族只经 `SessionBackend`/`TmuxBackend` 触达，自己拼 `tmux kill-session` 不行；
     * - 「target 是它自己」：返回一条指向别的会话的 kill 命令时，「非 null」那种断言照样绿。
     */
    @Test
    fun reapCommandForRoutesThroughTheBackendAndNamesOnlyThatSession() {
        val sid = "0f9d3c2a-1111-4222-8333-444455556666"
        val name = PipeLauncher.tmuxNameFor(sid)
        val cmd = PipeLauncher.reapCommandFor(TmuxBackend, name)

        assertEquals("必须逐字节等于后端那条 —— 不许在这里另拼一份", TmuxBackend.killCommand(name), cmd)
        assertTrue("target 必须是这条会话本身：$cmd", cmd!!.contains("-t '$name'"))
        // 反面：命令里不许出现别的会话名（防「拼错 target」那一族）
        assertFalse("命令里混进了别的会话名：$cmd", cmd.contains("atermpipe-other"))
    }

    /**
     * 判别力边界的判据：闸认的是「名字空间」，不是「谁起的」。
     *
     * 一个用户手敲出来的 `atermpipe-<合法 UUID>` 与我们自己起的那条，在闸眼里逐字节不可分。
     * 本条把这件事钉成常绿事实，防下一个人把 [PipeLauncher.isOurPipeSessionName] 读成「作者证明」，
     * 而那个误读的后果正好是杀错东西。真要能分辨，得有一份「起过哪些」的落盘账本。
     */
    @Test
    fun theGateCannotTellOurOwnSessionFromAHandMadeOneInOurNamespace() {
        val sid = freshUuid()
        val oursByUs = PipeLauncher.tmuxNameFor(sid)
        val oursByHand = PipeLauncher.PIPE_SESSION_PREFIX + sid // 想象用户手敲 `tmux new -s …`

        assertEquals("前提：两者本来就是同一个串 —— 这正是「分不开」的字面证据", oursByUs, oursByHand)
        assertTrue("闸放行它（因为它在我们的名字空间里）", PipeLauncher.isOurPipeSessionName(oursByHand))
        // ⇒ 结论：放行 ≠ 我们起的。这句话就是 `isOurPipeSessionName` 头注第一段要说的那件事。
    }

    private companion object {
        /** 连造多少个过闸名来扫「不相交」那条性质。 */
        private const val ACCEPT_SAMPLES = 64

        /**
         * 现铸一个 UUID：生产上新对话的编号就是这么来的
         * （`ChatRoute.newConversationId()` 逐字 `UUID.randomUUID().toString()`）。
         *
         * 链式调用逐行断开是本仓的 ktlint 规矩（`chain-method-continuation`），
         * 同 `UplinkIdentity.newOrigin()` 的写法。
         */
        private fun freshUuid(): String =
            UUID
                .randomUUID()
                .toString()

        /**
         * 拒绝面语料。每一类都对应一个真实的误杀方向，别当成随机怪字符串。
         *
         * | 名字 | 它代表谁 |
         * |---|---|
         * | `work` · `main` · `dev` | 用户自己手开的会话（`HostConnect.resolveTmuxSession` 那条透传路放进来的） |
         * | `cc-abc12345` | 终端 resume 的会话（`ClaudeInvocation.resumeSessionName`），里面跑着官方 TUI |
         * | `cx-abc12345` | Codex 那一侧的同形（`CodexInvocation`） |
         * | `cc-relay` · `cc-relayd` · `cc-relay-planner` | 主机上的 `cc-*` 会话：不许碰 |
         * | `myproj-cc` · `myproj-cc-2` | `cc-*` 那一层的新形态（`isCcmTmuxName` 的后缀支） |
         * | `atermpipe-` | 退化名：前缀本身，[PipeLauncher.tmuxNameFor] 造不出来 |
         * | `atermpipe-work` · `atermpipe-notauuid` | 借我们前缀、但 sid 不是 UUID ⇒ 不是我们造的 |
         * | `atermpipe-proj-cc` | 那个真实交集（见 [theCcSuffixOverlapIsRefused]） |
         * | `Atermpipe-<uuid>` | 前缀大小写不符：tmux 名区分大小写，它是另一条会话 |

         * | `xatermpipe-<uuid>` | 前缀只是子串而非开头 |
         * | `atermpipe-<uuid>-extra` | 过闸名后面被接了东西 ⇒ 另一条会话 |
         */
        private val REFUSED_NAMES =
            listOf(
                "work",
                "main",
                "dev",
                "",
                "cc-abc12345",
                "cx-abc12345",
                "cc-relay",
                "cc-relayd",
                "cc-relay-planner",
                "myproj-cc",
                "myproj-cc-2",
                "atermpipe-",
                "atermpipe-work",
                "atermpipe-notauuid",
                "atermpipe-proj-cc",
                "Atermpipe-0f9d3c2a-1111-4222-8333-444455556666",
                "xatermpipe-0f9d3c2a-1111-4222-8333-444455556666",
                "atermpipe-0f9d3c2a-1111-4222-8333-444455556666-extra",
                "atermpipe-0f9d3c2a-1111-4222-8333-44445555666",
            )
    }
}
