package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.AttachItem
import com.ccmonitor.mobile.core.wire.gen.AttachVerdict
import com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCheckReply
import com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCheckReq
import com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCommitReply
import com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCommitReq
import com.ccmonitor.mobile.core.wire.gen.SessionAttachmentDropReq
import com.ccmonitor.mobile.core.wire.gen.Sha256Expect

/*
 * 聊天里随一句发的附件（`IPC-COMMANDS.md`「session-attachment-*」）：收不收 · 落多大 · 落在哪全由会话那台判；手机只拼入参、解成品。
 * 字节走现成的分块通道（[CoreLink.transfer] 的 [Transfer.Stage]），落那一下是 [SessionAttachmentCommit]。
 */

/** 开传前问的一份：名字 · 大小（本机那一份的事实）。 */
data class AttachAsk(
    val name: String,
    val bytes: Long,
)

/** `session-attachment-check`：这一条草稿里的全部附件（按先后），回逐份收不收（同序同数）。 */
object SessionAttachmentCheck {
    const val COMMAND: String = "session-attachment-check"

    fun args(
        sid: String,
        items: List<AttachAsk>,
    ): Map<String, Any?> = Wire.args(SessionAttachmentCheckReq(sid, items.map { AttachItem(it.name, it.bytes) }))

    fun of(data: Any?): List<AttachVerdict>? = Wire.bind<SessionAttachmentCheckReply>(data)?.items
}

/** `session-attachment-commit`：暂存区里送完的那一份（块号连着）落进会话工作目录 ⇒ 编号（送那一句时只交它）。 */
object SessionAttachmentCommit {
    const val COMMAND: String = "session-attachment-commit"

    fun args(
        sid: String,
        name: String,
        staged: FilesCommitUpload.Staged,
    ): Map<String, Any?> = Wire.args(SessionAttachmentCommitReq(sid, staged.key, name, staged.bytes, staged.chunks, Sha256Expect(staged.sha256)))

    fun of(data: Any?): SessionAttachmentCommitReply? = Wire.bind<SessionAttachmentCommitReply>(data)
}

/** `session-attachment-drop`：草稿上撤掉一份，那台删掉那个编号目录（本来就不在不是错）。 */
object SessionAttachmentDrop {
    const val COMMAND: String = "session-attachment-drop"

    fun args(
        sid: String,
        id: String,
    ): Map<String, Any?> = Wire.args(SessionAttachmentDropReq(sid, id))
}
