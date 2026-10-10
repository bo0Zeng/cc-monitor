package com.ccmonitor.mobile.core.claude.transport

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * [TurnEndRoute]：把 daemon 流的帧折成「哪条会话、哪个文件完成了一轮」。
 * 证明的是「帧按后端契约到达时，配住址与排下一次该什么时候问的逻辑是对的」。
 */
class TurnEndRouteTest {
    private fun te(
        sid: String,
        uuid: String,
    ) = JsonlFrame.TurnEnd(sid, uuid)

    private fun added(
        sid: String,
        path: String?,
        agentKind: String? = null,
    ) = JsonlFrame.SessionAdded(sessionId = sid, path = path, agentKind = agentKind)

    /** `TurnEnd` 帧没有 path，住址得从 `session_added` 攒。 */
    @Test
    fun theRoundIsRoutedToThePathLearnedFromSessionAdded() {
        val r = TurnEndRoute()
        r.onFrame(added("s1", "/p/enc/s1.jsonl", agentKind = "claude"), 0)
        r.onFrame(te("s1", "u1"), 10)
        assertEquals("窗口内不结算", emptyList<TurnEndRoute.Done>(), r.settle(900))
        val done = r.settle(1_300)
        assertEquals(1, done.size)
        assertEquals("/p/enc/s1.jsonl", done[0].path)
        assertEquals("认得出 path 就以 path 为通知槽（与逐行跟尾那条路同槽，交接重叠期才不会重复可见）", "/p/enc/s1.jsonl", done[0].slotKey)
        assertEquals("claude", done[0].agentKindWire)
        assertEquals("u1", done[0].uuid)
    }

    /**
     * 后端在会话刚起、还没写首行时 path 为空。拿不到 path 也必须通知（漏通知是最坏的失败），
     * 只是投到一个按 sid 的槽。
     */
    @Test
    fun aRoundWithNoKnownPathStillGetsASlotInsteadOfBeingDropped() {
        val r = TurnEndRoute()
        r.onFrame(added("s1", null), 0) // 对端说了这条会话，但 path 那一格是空的
        r.onFrame(te("s1", "u1"), 10)
        val done = r.settle(1_300)
        assertEquals(1, done.size)
        assertNull("帧里没说 path", done[0].path)
        assertEquals("退回按 sid 的槽，不许因为不知道 path 就不出事件", "sid:s1", done[0].slotKey)
        assertNull("帧里没说哪一家 ⇒ 不许替它编一个（调用方要能退回主机自己配的那一家）", done[0].agentKindWire)
    }

    /** 连 `session_added` 都没见过（`overflow` 把它丢了）也要出事件。 */
    @Test
    fun aRoundFromAnEntirelyUnknownSessionStillGetsASlot() {
        val r = TurnEndRoute()
        r.onFrame(te("ghost", "u1"), 0)
        assertEquals(listOf("sid:ghost"), r.settle(1_300).map { it.slotKey })
    }

    /**
     * 同一个 sid 会收到不止一条 `session_added`（一个 sid 可以有两份 pidfile），
     * 第二条可能把 path 省了 ⇒ 必须并入而不是整条重建，否则住址会被抹掉、通知投到 `sid:…` 去。
     */
    @Test
    fun aSecondSessionAddedWithFewerFieldsDoesNotEraseTheKnownPath() {
        val r = TurnEndRoute()
        r.onFrame(added("s1", "/p/enc/s1.jsonl", agentKind = "codex"), 0)
        r.onFrame(added("s1", null, agentKind = null), 10) // 第二条宣告，两格都省了
        r.onFrame(te("s1", "u1"), 20)
        val done = r.settle(1_300)
        assertEquals("path 不许被第二条 added 抹掉", "/p/enc/s1.jsonl", done[0].path)
        assertEquals("agentKind 同理", "codex", done[0].agentKindWire)
    }

    /** 住址后到也算（`turn_end` 先到、`session_added` 后到；后端是事件驱动的，顺序不保证）。 */
    @Test
    fun aPathLearnedAfterTheFrameStillRoutesTheRound() {
        val r = TurnEndRoute()
        r.onFrame(te("s1", "u1"), 0)
        r.onFrame(added("s1", "/p/enc/s1.jsonl"), 100)
        assertEquals(listOf("/p/enc/s1.jsonl"), r.settle(1_300).map { it.slotKey })
    }

    /** 会话没了 ⇒ 两份状态都清掉（住址 + 待结算），与逐行跟尾那条路撤 tail 同行为，理由在类头注。 */
    @Test
    fun aRemovedSessionLoosesBothItsAddressAndItsPendingRound() {
        val r = TurnEndRoute()
        r.onFrame(added("s1", "/p/enc/s1.jsonl"), 0)
        r.onFrame(te("s1", "u1"), 10)
        assertEquals(1, r.knownSessionCount())
        r.onFrame(JsonlFrame.SessionRemoved("s1"), 20)
        assertEquals("住址清了", 0, r.knownSessionCount())
        assertEquals("待结算那一帧也清了", emptyList<TurnEndRoute.Done>(), r.settle(1_300))
        assertNull("清干净了 ⇒ 没有待结算 ⇒ 调用方该一直阻塞、不该空转", r.msUntilNextRipe(1_300))
    }

    /** `line` / `session_status` / `hello` / `overflow` 一律跳过：这里只管轮次与住址。 */
    @Test
    fun framesThatAreNotRoundsOrAddressesAreIgnored() {
        val r = TurnEndRoute()
        r.onFrame(JsonlFrame.Hello(), 0)
        r.onFrame(JsonlFrame.Line("s1", "/p/enc/s1.jsonl", 0, "{}", 2), 0)
        r.onFrame(JsonlFrame.SessionStatus("s1", status = "idle"), 0)
        r.onFrame(JsonlFrame.Overflow(dropped = 3), 0)
        assertEquals("这几种都不建住址", 0, r.knownSessionCount())
        assertNull("也不许让调度以为有待结算的", r.msUntilNextRipe(0))
        assertEquals(emptyList<TurnEndRoute.Done>(), r.settle(10_000))
    }

    // ===== 调度镜像（msUntilNextRipe）：只决定「什么时候问」，不决定「答案是什么」 =====

    /**
     * 下次唤醒取 per-session 最小值，不是「最后一帧之后 1200ms」。
     *
     * 写成后者会让一条话多的会话把另一条安静会话的窗口无限续上：去抖器判对了，但没人去问它。
     * 把 [TurnEndRoute.msUntilNextRipe] 换成取最后一帧 ⇒ 这条红。
     */
    @Test
    fun theNextWakeupFollowsTheEarliestPendingSessionNotTheLatestFrame() {
        val r = TurnEndRoute()
        r.onFrame(te("s1", "u1"), 0)
        r.onFrame(te("s2", "u2"), 1_000)
        assertEquals("s1 在 t=1200 就成熟了，不许被 s2 推到 t=2200", 200L, r.msUntilNextRipe(1_000))
        assertEquals("已经过点了 ⇒ 立刻问（不许返回负数让 withTimeout 抛）", 0L, r.msUntilNextRipe(5_000))
    }

    /**
     * 被 uuid 去重挡掉的那一次结算，调度镜像也必须跟着清。
     *
     * [TurnEndDebouncer.settle] 会把成熟的一律从 `pending` 里摘掉，哪怕因 uuid 与上次相同而不返回。
     * 镜像只按「返回了什么」来清的话，那个 sid 永远留在镜像里 ⇒ `msUntilNextRipe` 恒 0 ⇒
     * 调用方 `withTimeoutOrNull(0)` 空转烧 CPU。删掉 `settle` 里那行 `removeAll` ⇒ 这条红。
     */
    @Test
    fun aRoundSwallowedByUuidDedupeStillClearsTheSchedulingMirror() {
        val r = TurnEndRoute()
        r.onFrame(te("s1", "u1"), 0)
        assertEquals(1, r.settle(1_300).size)
        assertNull("结算过 ⇒ 镜像空", r.msUntilNextRipe(1_300))

        r.onFrame(te("s1", "u1"), 5_000) // 重连重读，同一轮再来一遍
        assertEquals("uuid 去重 ⇒ 不出事件", emptyList<TurnEndRoute.Done>(), r.settle(6_300))
        assertNull("但镜像必须也清了，否则调用方会以 0ms 超时空转", r.msUntilNextRipe(6_300))
    }

    /**
     * daemon 流要断了 ⇒ 把没结算的全倒出来。
     *
     * 不倒的话会漏通知：流断 ⇒ 回落逐行跟尾 ⇒ 它第一轮吞历史 ⇒ 静默窗口里那一轮两条路都不发。
     */
    @Test
    fun flushAllRipensEverythingPendingSoNothingIsLostWhenTheStreamDies() {
        val r = TurnEndRoute()
        r.onFrame(added("s1", "/p/a.jsonl"), 0)
        r.onFrame(added("s2", "/p/b.jsonl"), 0)
        r.onFrame(te("s1", "u1"), 0)
        r.onFrame(te("s2", "u2"), 100) // 两条都还在窗口里
        assertEquals("正常结算这一刻一条都不该出", emptyList<TurnEndRoute.Done>(), r.settle(200))
        assertEquals(
            "flushAll 把两条都倒出来（顺序按 sid，判据要可复现）",
            listOf("/p/a.jsonl", "/p/b.jsonl"),
            r.flushAll(200).map { it.slotKey },
        )
        assertNull("倒完就没有待结算的了", r.msUntilNextRipe(200))
    }

    /** 静默窗口默认值与逐行跟尾那条路用的相同（两条路感知的延迟一样）。 */
    @Test
    fun theDefaultSettleWindowIsTheDebouncerOne() {
        val r = TurnEndRoute()
        r.onFrame(te("s1", "u1"), 0)
        assertEquals(TurnEndDebouncer.SETTLE_MS, r.msUntilNextRipe(0))
    }
}
