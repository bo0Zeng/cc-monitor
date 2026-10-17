package com.ccmonitor.mobile.ui.usage

import com.squareup.moshi.Moshi
import java.io.File

/** `quota-read` 跨语言金样（后端真出口写的）：按例名前缀取一例的回包，手机单测都从这里取，不另抄样本。 */
internal object QuotaGolden {
    @Suppress("UNCHECKED_CAST")
    private val cases: List<Map<String, Any?>> by lazy {
        val root = generateSequence(File("").absoluteFile) { it.parentFile }.first { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }
        val all =
            Moshi
                .Builder()
                .build()
                .adapter(Any::class.java)
                .fromJson(File(root, "tests/__fixtures__/quota-read.golden.json").readText()) as Map<String, Any?>
        all["cases"] as List<Map<String, Any?>>
    }

    @Suppress("UNCHECKED_CAST")
    fun reply(prefix: String): Map<String, Any?> = cases.first { (it["name"] as String).startsWith(prefix) }["reply"] as Map<String, Any?>
}
