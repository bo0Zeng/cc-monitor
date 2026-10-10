package com.ccmonitor.mobile.core.claude.link

import java.io.File

/**
 * 读本仓的跨语言金样（`tests/__fixtures__/`，后端真序列化器写的）。手机的解码读同一份，不另抄一份样本。
 * 测试的工作目录是模块目录（`src/mobile/<模块>`）。
 */
internal object Fixtures {
    val repoRoot: File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .firstOrNull { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }
            ?: error("从 ${File("").absolutePath} 往上找不到仓根（tests/__fixtures__ ＋ src/backend/lib.rs）")

    fun text(name: String): String = File(repoRoot, "tests/__fixtures__/$name").readText()

    fun json(name: String): Map<String, Any?> = Json.obj(text(name)) ?: error("$name 不是 JSON 对象")
}
