// 文案取文口（`copyText`）与格式化（大小 · 时长）：纯 JVM 叶子模块，不带 Android / Compose。
plugins {
    alias(libs.plugins.kotlin.jvm)
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}

// 文案：和桌面同一张表 `src/shared/copy/table.json`，Kotlin 侧不另写表（`架构.md` §7）。
// 取文口住这个纯 JVM 模块，各模块（app · core-ui · core-ssh · core-data · core-claude · core-remote）都依赖它：
// 下层模块自己写的上屏句子也从表里取（同桌面 Rust 那边把 copy_core 单做一个 crate）。
// 构建时扫手机全部模块生产源码里的 `copyText("<键>"` 调用点，只把用到的那几条抄进生成的 `CopyEntries`；
// 引了表里没有的键 ⇒ 构建失败。参数名与表里 `args` 两向相等由 tests/copy/copy-table.vitest.ts 一并判（与 TS · Rust 同一条）。
val copyTable = rootProject.file("../shared/copy/table.json")
val mobileSources =
    fileTree(rootProject.projectDir) {
        include("*/src/main/**/*.kt")
    }
val copyGenDir = layout.buildDirectory.dir("generated/copyEntries")
val genCopyEntries =
    tasks.register("genCopyEntries") {
        inputs.file(copyTable)
        inputs.files(mobileSources)
        outputs.dir(copyGenDir)
        doLast {
            @Suppress("UNCHECKED_CAST")
            val entries = (groovy.json.JsonSlurper().parse(copyTable) as Map<String, Any?>)["entries"] as Map<String, Map<String, Any?>>
            val call = Regex("""\bcopyText\(\s*"([A-Za-z0-9.]+)"""")
            val used =
                mobileSources.files
                    .flatMap { f -> call.findAll(f.readText()).map { it.groupValues[1] }.toList() }
                    .toSortedSet()
            val unknown = used.filterNot(entries::containsKey)
            if (unknown.isNotEmpty()) throw GradleException("手机代码引了文案表里没有的键：$unknown")

            fun lit(s: String) =
                "\"" +
                    s
                        .replace("\\", "\\\\")
                        .replace("\"", "\\\"")
                        .replace("\n", "\\n")
                        .replace("$", "\\$") + "\""
            val body =
                used.joinToString("\n") { k ->
                    val e = entries.getValue(k)

                    @Suppress("UNCHECKED_CAST")
                    val args = (e["args"] as List<String>).joinToString(", ") { lit(it) }
                    "            ${lit(k)} to CopyEntry(${lit(e["zh"] as String)}, listOf($args)),"
                }
            val out = copyGenDir.get().file("com/ccmonitor/mobile/core/copy/CopyEntries.kt").asFile
            out.parentFile.mkdirs()
            out.writeText(
                """
                |// 生成物：core-copy/build.gradle.kts 的 genCopyEntries 从 src/shared/copy/table.json 抄来（只抄手机代码引到的那几条），勿手改。
                |package com.ccmonitor.mobile.core.copy
                |
                |internal object CopyEntries {
                |    private val ENTRIES: Map<String, CopyEntry> =
                |        mapOf(
                |$body
                |        )
                |
                |    fun of(key: String): CopyEntry? = ENTRIES[key]
                |}
                |
                """.trimMargin(),
            )
        }
    }

// 生成物落在加进源码集的目录里：读源码集的任务（ktlint · detekt）都要排在生成之后。
tasks.matching { it.name.startsWith("runKtlint") || it.name.startsWith("ktlint") || it.name.startsWith("detekt") }.configureEach {
    dependsOn(genCopyEntries)
}

kotlin {
    sourceSets.named("main") {
        kotlin.srcDir(genCopyEntries.map { copyGenDir.get() })
    }
}

dependencies {
    testImplementation(libs.junit)
    testImplementation(libs.moshi)
}
