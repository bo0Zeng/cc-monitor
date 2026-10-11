package com.ccmonitor.mobile.core.claude.link

/**
 * 门槛：那台的后端和这一版手机是**同一个 `BUILD_ID`** 才接（同仓之后手机内嵌本仓后端的身份，不比大小、不留回落）。
 * 读那台 `ccm -- --backend-probe`（回 `{proto, buildId, commands}`，不读 stdin）。
 * 只核这一处：改了冻结的格同加子命令一样要打版本号（核心那道判据管），所以同一个 `BUILD_ID` 就是两端的格对得上。
 */
sealed interface GateVerdict {
    /** 同一个 `BUILD_ID`。 */
    data object Same : GateVerdict

    /** 那台是另一版（`theirs` 是它自报的身份；说不出 ⇒ `null`）⇒ 走部署（换成同一版）。 */
    data class Differs(
        val theirs: String?,
    ) : GateVerdict

    /** 那台没装后端 ⇒ 走部署。 */
    data object Absent : GateVerdict

    /** 问了但没问成：核心写好的失败。 */
    data class Failed(
        val failure: CoreFailure,
    ) : GateVerdict

    /** 问了但回话对不上协议（原样带回，进复制详情）。 */
    data class Unreadable(
        val outcome: OneShotOutcome.Unreadable,
    ) : GateVerdict
}

object BackendGate {
    const val PROBE: String = "--backend-probe"

    suspend fun check(oneShot: OneShot): GateVerdict = verdict(oneShot.run(PROBE))

    /** 判法只这一处。 */
    fun verdict(
        outcome: OneShotOutcome,
        ours: String = EmbeddedBuild.ID,
    ): GateVerdict =
        when (outcome) {
            is OneShotOutcome.Ok -> outcome.data.str("buildId").let { if (it == ours) GateVerdict.Same else GateVerdict.Differs(it) }
            is OneShotOutcome.Failed -> GateVerdict.Failed(outcome.failure)
            OneShotOutcome.Absent -> GateVerdict.Absent
            is OneShotOutcome.Unreadable -> GateVerdict.Unreadable(outcome)
        }
}
