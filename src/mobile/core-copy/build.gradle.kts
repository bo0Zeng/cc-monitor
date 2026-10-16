// 文案取文口（`copyText`）与格式化（大小 · 时长）：纯 JVM 叶子模块，不带 Android / Compose。
// 写法都在手机那份 `.so` 里的 `copy-core`（经 JNI，见 `CopyCore`）；上屏有字的模块都依赖本模块。
plugins {
    alias(libs.plugins.kotlin.jvm)
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}

dependencies {
    testImplementation(libs.junit)
    testImplementation(libs.moshi)
}
