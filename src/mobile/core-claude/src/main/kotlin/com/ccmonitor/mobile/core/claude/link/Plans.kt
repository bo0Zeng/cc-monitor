package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.PlanCellView as PlanCellViewReply

/** `plan-list`：这台的 pb 工作区与片（目录 ＝ 那台此刻活着的会话的工作目录；认过的不重问）。 */
object PlanList {
    const val COMMAND: String = "plan-list"

    fun args(): Map<String, Any?> = emptyMap()
}

/** `plan-read {workspace}`：一个工作区的成品。 */
object PlanRead {
    const val COMMAND: String = "plan-read"

    fun args(workspace: String): Map<String, Any?> = mapOf("workspace" to workspace)
}

/** `plan-cell-view {workspace, slice, id}`：pb 印给站在这一格的 agent 的那一段，原样。 */
object PlanCellView {
    const val COMMAND: String = "plan-cell-view"

    fun args(
        workspace: String,
        slice: String,
        id: String,
    ): Map<String, Any?> = mapOf("workspace" to workspace, "slice" to slice, "id" to id)

    fun of(data: Any?): String? = Wire.bind<PlanCellViewReply>(data)?.view
}

/** 一格的状态画哪一种（图标照它挑）：核心照后端的码交来，认不得 ⇒ [NONE]。 */
enum class PlanLook { DONE, OPEN, UNDECIDED, NONE }

/** 一行（大纲顶层 · 一格的里面）：「编号 标题」· 状态那一句 · 有没有子格。 */
data class PlanRowFace(
    val id: String,
    val label: String,
    val look: PlanLook,
    val end: String,
    val kids: Boolean,
)

data class PlanSliceRow(
    val slice: String,
    val line: String,
    val end: String?,
    val need: Boolean,
)

data class PlanSliceGroup(
    val workspace: String,
    val head: String,
    val rows: List<PlanSliceRow>,
)

/** 片那一屏：[said] ＝ pb 不在 · 无计划那一句（有片 ⇒ `null`）。 */
data class PlanSlicesFace(
    val said: String?,
    val groups: List<PlanSliceGroup>,
)

data class PlanNeedCard(
    val kind: String,
    val title: String,
    val lines: List<String>,
    val pos: String,
)

data class PlanKv(
    val k: String,
    val v: String,
)

data class PlanKvSection(
    val head: String,
    val rows: List<PlanKv>,
)

/** 大纲那一屏：需手动排最前 · 此会话接手的块 · 顶层一层。 */
data class PlanOutlineFace(
    val title: String,
    val meta: List<String>,
    val needs: List<PlanNeedCard>,
    val mine: PlanKvSection?,
    val head: String,
    val rows: List<PlanRowFace>,
)

data class PlanTextSection(
    val head: String,
    val body: String,
)

data class PlanRowsSection(
    val head: String,
    val rows: List<PlanRowFace>,
)

data class PlanFileRow(
    val path: String,
    val state: String?,
    val note: String?,
)

data class PlanFilesSection(
    val head: String,
    val rows: List<PlanFileRow>,
)

/** 一格详情那一屏。 */
data class PlanCellFace(
    val label: String,
    val kind: String?,
    val path: String?,
    val look: PlanLook,
    val state: String,
    val block: String?,
    val design: PlanTextSection?,
    val inside: PlanRowsSection?,
    val files: PlanFilesSection?,
    val agentHead: String,
    val agentHint: String,
)

/**
 * 计划三屏的字：交回包原样，内嵌核心写好那一屏（`core-rs/rust/plan.rs`），这里只读成小数据类。回包读不动 · 片或格不在 ⇒ `null`。
 * [face] 换得掉（判据交替身）。
 */
class Plans(
    private val face: (String) -> String = PlanFace::face,
) {
    fun slices(reply: Any?): PlanSlicesFace? =
        ask(mapOf("layer" to "slices", "reply" to reply))?.let { m ->
            PlanSlicesFace(
                said = m.str("said"),
                groups =
                    m.maps("groups").map { g ->
                        PlanSliceGroup(
                            g.str("workspace").orEmpty(),
                            g.str("head").orEmpty(),
                            g.maps("rows").map { PlanSliceRow(it.str("slice").orEmpty(), it.str("line").orEmpty(), it.str("end"), it["need"] == true) },
                        )
                    },
            )
        }

    fun outline(
        reply: Any?,
        slice: String,
        sid: String?,
    ): PlanOutlineFace? =
        ask(mapOf("layer" to "outline", "reply" to reply, "slice" to slice, "sid" to sid))?.let { m ->
            PlanOutlineFace(
                title = m.str("title").orEmpty(),
                meta = m.strs("meta"),
                needs = m.maps("needs").map { PlanNeedCard(it.str("kind").orEmpty(), it.str("title").orEmpty(), it.strs("lines"), it.str("pos").orEmpty()) },
                mine = m.map("mine")?.let { s -> PlanKvSection(s.str("head").orEmpty(), s.maps("rows").map { PlanKv(it.str("k").orEmpty(), it.str("v").orEmpty()) }) },
                head = m.str("head").orEmpty(),
                rows = m.maps("rows").map(::row),
            )
        }

    fun cell(
        reply: Any?,
        slice: String,
        id: String,
    ): PlanCellFace? =
        ask(mapOf("layer" to "cell", "reply" to reply, "slice" to slice, "id" to id))?.let { m ->
            PlanCellFace(
                label = m.str("label").orEmpty(),
                kind = m.str("kind"),
                path = m.str("path"),
                look = look(m.str("look")),
                state = m.str("state").orEmpty(),
                block = m.str("block"),
                design = m.map("design")?.let { PlanTextSection(it.str("head").orEmpty(), it.str("body").orEmpty()) },
                inside = m.map("inside")?.let { s -> PlanRowsSection(s.str("head").orEmpty(), s.maps("rows").map(::row)) },
                files =
                    m.map("files")?.let { s ->
                        PlanFilesSection(s.str("head").orEmpty(), s.maps("rows").map { PlanFileRow(it.str("path").orEmpty(), it.str("state"), it.str("note")) })
                    },
                agentHead = m.str("agentHead").orEmpty(),
                agentHint = m.str("agentHint").orEmpty(),
            )
        }

    private fun ask(want: Map<String, Any?>): Map<*, *>? = face(Json.line(want)).takeIf { it.isNotEmpty() }?.let { Json.value(it) as? Map<*, *> }

    private fun row(m: Map<*, *>): PlanRowFace = PlanRowFace(m.str("id").orEmpty(), m.str("label").orEmpty(), look(m.str("look")), m.str("end").orEmpty(), m["kids"] == true)

    private fun look(code: String?): PlanLook = PlanLook.entries.firstOrNull { it.name.lowercase() == code } ?: PlanLook.NONE

    private fun Map<*, *>.str(k: String): String? = this[k] as? String

    private fun Map<*, *>.map(k: String): Map<*, *>? = this[k] as? Map<*, *>

    private fun Map<*, *>.maps(k: String): List<Map<*, *>> = (this[k] as? List<*>).orEmpty().filterIsInstance<Map<*, *>>()

    private fun Map<*, *>.strs(k: String): List<String> = (this[k] as? List<*>).orEmpty().filterIsInstance<String>()

    companion object {
        /** 生产那一份（JNI 进内嵌核心）。 */
        val core: Plans by lazy { Plans() }

        fun slices(reply: Any?): PlanSlicesFace? = core.slices(reply)

        fun outline(
            reply: Any?,
            slice: String,
            sid: String?,
        ): PlanOutlineFace? = core.outline(reply, slice, sid)

        fun cell(
            reply: Any?,
            slice: String,
            id: String,
        ): PlanCellFace? = core.cell(reply, slice, id)
    }
}

/** `libmobcore.so` 里计划三屏那一个口（`core-rs/rust/lib.rs`）。 */
internal object PlanFace {
    init {
        System.loadLibrary("mobcore")
    }

    /** `{layer, reply, slice?, id?, sid?}` ⇒ 那一屏的字（JSON）；读不动 ⇒ 空串。 */
    external fun face(want: String): String
}
