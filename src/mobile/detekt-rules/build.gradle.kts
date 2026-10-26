// 本仓自己的 detekt 规则（出口扫描判据的 Kotlin 那一半，见 `ExitRuleSetProvider`）：纯 JVM，只在 detekt 跑的时候装进去，不进 app。
// 别的模块经根 `build.gradle.kts` 的 `detektPlugins` 用它。
plugins {
    alias(libs.plugins.kotlin.jvm)
}

java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile>().configureEach {
    // detekt 2 的 API 是 JVM 17 的字节码，内联它的函数要同一档。
    compilerOptions.jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
}

val detektVersion = libs.versions.detekt.get()

dependencies {
    compileOnly("dev.detekt:detekt-api:$detektVersion")
    testImplementation("dev.detekt:detekt-api:$detektVersion")
    // 只要把一段代码解成 KtFile 的那一份；detekt-test 发布的元数据要 detekt-api 的测试夹具变体，解析不出来，不用它。
    testImplementation("dev.detekt:detekt-test-utils:$detektVersion")
    testImplementation(libs.junit)
}
