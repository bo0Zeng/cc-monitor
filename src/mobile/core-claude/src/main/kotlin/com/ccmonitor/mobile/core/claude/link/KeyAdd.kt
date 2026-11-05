package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.KeyAddReq

/** `authorized-keys-add`：把一行 OpenSSH 公钥并进那台这个用户的 `authorized_keys`（怎么写 · 权限 · 去重 · 那一句都在那台）。 */
object KeyAdd {
    const val COMMAND: String = Commands.AuthorizedKeysAdd.NAME

    fun args(key: String): Map<String, Any?> = Commands.AuthorizedKeysAdd.args(KeyAddReq(key))

    /** 那台写好的那一句（并进去了 · 本来就在），屏上照摆。 */
    fun of(data: Any?): String? = Commands.AuthorizedKeysAdd.reply(data)?.said
}
