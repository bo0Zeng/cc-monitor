package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.core.claude.link.AttachOutcome
import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.GateVerdict
import com.ccmonitor.mobile.core.claude.link.LinkOutcome
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.OneShotOutcome
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.ui.copy.copyText

/**
 * 屏上那一句 ＋ 复制详情。核心写好的失败（[CoreFailure]）原样：`message` 上屏、`detail` 进详情；
 * 手机这一侧判出来的走不通（门槛 · 握手）用桌面同一个概念的那几条字（`rsRemoteResident.*` · `front.*`），不另写。
 */
data class Problem(
    val text: String,
    val detail: String,
)

private fun core(f: CoreFailure) = Problem(f.message, f.detail)

private fun unreadable(o: OneShotOutcome.Unreadable) =
    Problem(copyText("rsRemoteResident.hello.notOurs"), "exit: ${o.exitStatus}\nstdout: ${o.stdout.trim()}\nstderr: ${o.stderr.trim()}")

/** 接一台走不通 ⇒ 那一句。[machine] 是这台在手机上的名字。 */
fun LinkState.Down.problem(machine: String): Problem {
    transport?.let { return Problem(copyText("front.body.offline", "machine" to machine), it) }
    return when (val o = outcome) {
        null -> Problem(copyText("front.body.offline", "machine" to machine), "")
        is LinkOutcome.Gate -> gateProblem(o.verdict, machine)
        is LinkOutcome.Ensure -> ensureProblem(o.outcome)
        is LinkOutcome.Attach -> attachProblem(o.outcome, machine)
        is LinkOutcome.HelloDiffers -> Problem(copyText("front.title.update", "machine" to machine), o.theirs.orEmpty())
        is LinkOutcome.Up -> Problem(copyText("front.body.offline", "machine" to machine), "")
    }
}

private fun gateProblem(
    v: GateVerdict,
    machine: String,
): Problem =
    when (v) {
        GateVerdict.Same -> Problem(copyText("front.body.offline", "machine" to machine), "")
        is GateVerdict.Differs -> Problem(copyText("front.title.update", "machine" to machine), v.theirs.orEmpty())
        GateVerdict.Absent -> Problem(copyText("remoteHealth.head.notDeployed"), "")
        is GateVerdict.Failed -> core(v.failure)
        is GateVerdict.Unreadable -> unreadable(v.outcome)
    }

private fun ensureProblem(e: OneShotOutcome): Problem =
    when (e) {
        is OneShotOutcome.Failed -> core(e.failure)
        OneShotOutcome.Absent -> Problem(copyText("remoteHealth.head.notDeployed"), "")
        is OneShotOutcome.Unreadable -> Problem(copyText("rsRemoteResident.ensure.answerUnreadable"), e.stderr.trim())
        is OneShotOutcome.Ok -> Problem(copyText("rsRemoteResident.ensure.noReason"), "")
    }

private fun attachProblem(
    a: AttachOutcome,
    machine: String,
): Problem =
    when (a) {
        is AttachOutcome.Refused ->
            when (a.reason) {
                "absent" -> Problem(copyText("rsRemoteResident.relay.absent", "machine" to machine), "")
                "unreachable" -> Problem(copyText("rsRemoteResident.relay.unreachable", "machine" to machine), "")
                else -> Problem(copyText("rsRemoteResident.handshake.refused", "why" to a.reason.orEmpty()), "")
            }
        is AttachOutcome.NotOurs -> Problem(copyText("rsRemoteResident.hello.notOurs"), a.line)
        is AttachOutcome.Cut ->
            when (a.stage) {
                AttachOutcome.Stage.HELLO -> Problem(copyText("rsRemoteResident.handshake.helloCut"), a.stderr)
                AttachOutcome.Stage.ATTACH_REPLY -> Problem(copyText("rsRemoteResident.handshake.replyCut"), a.stderr)
            }
        is AttachOutcome.Attached -> Problem(copyText("front.body.offline", "machine" to machine), "")
    }

/** 一问一答没答成 ⇒ 那一句；答成了 ⇒ `null`。 */
fun Reply.problem(machine: String): Problem? =
    when (this) {
        is Reply.Ok -> null
        is Reply.Failed -> core(failure)
        Reply.TimedOut -> Problem(copyText("front.body.timeout", "machine" to machine), "")
        Reply.Cancelled, Reply.LinkDown -> Problem(copyText("front.body.offline", "machine" to machine), "")
        is Reply.Unreadable -> unreadableReply(raw)
    }

/** 应答缺必填格（帧上的 · 成品里的同一个说法）；[raw] 进详情。 */
fun unreadableReply(raw: String): Problem = Problem(copyText("rsRemoteResident.hello.notOurs"), raw)
