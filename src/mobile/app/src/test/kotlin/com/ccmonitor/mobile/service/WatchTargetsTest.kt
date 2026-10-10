package com.ccmonitor.mobile.service

import com.ccmonitor.mobile.core.claude.catalog.LiveSessionScan
import com.ccmonitor.mobile.core.claude.catalog.SessionRef
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * turn-end watcher 目标选择的纯逻辑：可信活动集全部入选（多并发会话不漏 turn-end）；
 * 探测降级 → 不撤 tail；超上限截断并报告丢了几条；无活会话退回最新单条。
 */
class WatchTargetsTest {
    private fun ref(
        path: String,
        live: Boolean,
    ) = SessionRef(path = path, sessionId = path.substringAfterLast('/').removeSuffix(".jsonl"), live = live, pidfileName = null)

    /** catalog `scanLiveSessions` 契约：refs 全 live=true（有活会话）或全 live=false（可信零活→兜底列表），从不混合。 */
    private fun scan(
        refs: List<SessionRef>,
        probeOk: Boolean = true,
    ) = LiveSessionScan(refs, probeOk)

    // === 可信活动集：全部入选、保序、允许撤旧 ===

    @Test
    fun allLiveSessionsAreWatched() {
        // 两个并发活会话（attach tab + resume tab）都被 tail，而非只挑最新 mtime 单条。
        val plan = watchTargets(scan(listOf(ref("/p/a/s1.jsonl", live = true), ref("/p/b/s2.jsonl", live = true))))
        assertEquals(listOf("/p/a/s1.jsonl", "/p/b/s2.jsonl"), plan.targets)
        assertTrue("可信活动集 → 允许撤已死会话 tail", plan.cancelStale)
        assertEquals(0, plan.droppedLive)
    }

    @Test
    fun liveSetCappedAtMaxTailsAndReportsDropped() {
        // 超上限截断 + 报告被丢弃条数（供调用方记日志，不静默丢）。
        val plan = watchTargets(scan((1..5).map { ref("/p/a/s$it.jsonl", live = true) }), maxTails = 3)
        assertEquals(listOf("/p/a/s1.jsonl", "/p/a/s2.jsonl", "/p/a/s3.jsonl"), plan.targets)
        assertEquals(2, plan.droppedLive)
    }

    @Test
    fun defaultCapCoversTypicalMultiSessionLoadNoDrop() {
        // 上限内（典型 2-3 会话 ≤ MAX_WATCH_TAILS=4）不截断、不丢；恰好达上限也全收、droppedLive=0。
        val plan = watchTargets(scan((1..MAX_WATCH_TAILS).map { ref("/p/a/s$it.jsonl", live = true) }))
        assertEquals(MAX_WATCH_TAILS, plan.targets.size)
        assertEquals(0, plan.droppedLive)
    }

    // === 探测降级：不可信 live 标记 → 空 targets + 绝不撤 tail ===

    @Test
    fun probeDegradedNeverCancelsTails() {
        // pidfile/proc 探测瞬时失败 → refs 全 live=false（兜底列表），但不得据此撤活 tail。
        val degraded = scan((1..3).map { ref("/p/a/s$it.jsonl", live = false) }, probeOk = false)
        val plan = watchTargets(degraded)
        assertTrue("探测降级本轮无可信活路径可加", plan.targets.isEmpty())
        assertFalse("探测降级绝不撤 tail（additive-only）", plan.cancelStale)
        assertEquals(0, plan.droppedLive)
    }

    // === 可信零活会话 → 最新 mtime 单条兜底（refs 最近在前，取首条 = `ls -t|head -1` 语义），允许撤旧 ===

    @Test
    fun trustworthyZeroLiveFallsBackToNewestSingle() {
        val plan = watchTargets(scan(listOf(ref("/p/a/newest.jsonl", live = false), ref("/p/a/older.jsonl", live = false))))
        assertEquals(listOf("/p/a/newest.jsonl"), plan.targets)
        assertTrue("可信零活会话 → 允许收敛到最新单条（撤旧 tail）", plan.cancelStale)
    }

    @Test
    fun trustworthyEmptyRefsYieldNoTargetsButAllowsCancel() {
        // 所有 jsonl 消失（ls 成功、空列表）→ 空 targets + cancelStale=true → 调用方撤掉全部现有 tail（含已删文件的 tail -F）。
        val plan = watchTargets(scan(emptyList()))
        assertTrue(plan.targets.isEmpty())
        assertTrue("可信空 → 允许撤全部 tail", plan.cancelStale)
    }

    // === 判活非权威（Codex mtime，无 pidfile）→ tail top-N 最近 additive，不单条+cancelStale 致 thrash ===

    @Test
    fun nonAuthoritativeLivenessTailsTopNRecentNotSingle() {
        // Codex 全 live=false 若走「可信零活单条」→ 交替写会来回撤/建 tail。所以盯 top-N 最近（都在内→稳）。
        val s = LiveSessionScan((1..3).map { ref("/s/r$it.jsonl", live = false) }, probeOk = true, authoritativeLiveness = false)
        val plan = watchTargets(s, maxTails = 4)
        assertEquals("盯 top-N 最近全部（非单条兜底）", listOf("/s/r1.jsonl", "/s/r2.jsonl", "/s/r3.jsonl"), plan.targets)
        assertTrue("允许撤跌出 top-N 的旧会话", plan.cancelStale)
        assertEquals(0, plan.droppedLive)
    }

    @Test
    fun nonAuthoritativeCapsAtMaxTails() {
        val s = LiveSessionScan((1..6).map { ref("/s/r$it.jsonl", live = false) }, probeOk = true, authoritativeLiveness = false)
        val plan = watchTargets(s, maxTails = 4)
        assertEquals(4, plan.targets.size)
        assertEquals(2, plan.droppedLive)
    }

    @Test
    fun nonAuthoritativeProbeFailStillAdditiveOnly() {
        // probeOk=false（连接抖动）优先短路 → 空 targets + 不撤，即便判活非权威。
        val s = LiveSessionScan((1..2).map { ref("/s/r$it.jsonl", live = false) }, probeOk = false, authoritativeLiveness = false)
        val plan = watchTargets(s)
        assertTrue(plan.targets.isEmpty())
        assertFalse("探测失败绝不撤 tail", plan.cancelStale)
    }

    // === 后端流在供货：tail 一条都不留，`MAX_WATCH_TAILS` 那条上限随之不适用 ===

    /**
     * 后端流（α）真的在供货 ⇒ 空 targets ＋ `cancelStale=true`。
     *
     * `cancelStale=true` 是本档的要点：靠它去撤掉交接之前那几条 tail（β）。
     * 写成 `false` 的话 α 接上了而 β 的 tail 一条都不会撤，两条路一直并行搬字节
     * （通知不会重：两条路投同一个槽；但那笔流量白付）。
     */
    @Test
    fun whenTheDaemonPathIsServingTheOldPathKeepsNoTailAtAll() {
        val live = scan((1..3).map { ref("/p/a/s$it.jsonl", live = true) })
        val plan = watchTargets(live, alphaCoversTurnEnd = true)
        assertTrue("α 一条流覆盖全部会话 ⇒ β 一条 tail 都不该有", plan.targets.isEmpty())
        assertTrue("必须允许撤 —— 否则交接之前那几条 β tail 会在 α 身边赖着不走", plan.cancelStale)
        assertEquals("α 档不存在「超上限」这回事 ⇒ 那句 N 条未监视 turn-end 的日志结构上发不出来", 0, plan.droppedLive)
    }

    /**
     * α 供货档排在 `probeOk` 短路之前。
     *
     * α 在供货时 β 的探测结果整个不相干，而 `probeOk=false` 那支会返回 `cancelStale=false`
     * ⇒ 顺序写反的话，一次探测抖动就能让 β 的 tail 在 α 身边赖着不走。
     * 负控形状：把 `watchTargets` 里那两个 `if` 换个位置 ⇒ 本条当场红。
     */
    @Test
    fun theDaemonPathTierWinsOverAProbeHiccup() {
        val degraded = scan((1..3).map { ref("/p/a/s$it.jsonl", live = false) }, probeOk = false)
        val plan = watchTargets(degraded, alphaCoversTurnEnd = true)
        assertTrue(plan.targets.isEmpty())
        assertTrue("α 供货时 β 的探测抖动不许挡住撤 tail", plan.cancelStale)
    }

    /**
     * 默认 `false` ⇒ 其余各档的行为不受这个参数影响。
     *
     * 上面那些判据全是不传这个参数跑的，本条再从正面断一次「显式传 false ＝ 不传」。
     */
    @Test
    fun theDefaultIsTheOldBehaviourByteForByte() {
        val cases =
            listOf(
                scan((1..2).map { ref("/p/a/s$it.jsonl", live = true) }),
                scan(listOf(ref("/p/a/newest.jsonl", live = false), ref("/p/a/older.jsonl", live = false))),
                scan(emptyList()),
                scan((1..3).map { ref("/p/a/s$it.jsonl", live = false) }, probeOk = false),
                LiveSessionScan((1..3).map { ref("/s/r$it.jsonl", live = false) }, probeOk = true, authoritativeLiveness = false),
            )
        for (s in cases) {
            assertEquals("显式 false 必须与不传逐字同结果（β 一档都没动）", watchTargets(s), watchTargets(s, alphaCoversTurnEnd = false))
        }
    }
}
