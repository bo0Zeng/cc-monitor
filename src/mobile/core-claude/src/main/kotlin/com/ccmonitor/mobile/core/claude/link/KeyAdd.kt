package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.KeyAddReq
import com.ccmonitor.mobile.core.wire.gen.PushOutcome

/** `authorized-keys-add`：把一行 OpenSSH 公钥并进那台这个用户的 `authorized_keys`（怎么写 · 权限 · 去重都在那台）。 */
object KeyAdd {
    const val COMMAND: String = Commands.AuthorizedKeysAdd.NAME

    fun args(key: String): Map<String, Any?> = Commands.AuthorizedKeysAdd.args(KeyAddReq(key))

    /** 并进去了（[PushOutcome.ADDED]）还是本来就在（[PushOutcome.ALREADY]）。 */
    fun of(data: Any?): PushOutcome? = Commands.AuthorizedKeysAdd.reply(data)?.outcome
}
