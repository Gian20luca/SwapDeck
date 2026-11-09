import org.jetbrains.intellij.platform.gradle.IntelliJPlatformType
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.jetbrains.kotlin.gradle.dsl.KotlinVersion

plugins {
    id("org.jetbrains.kotlin.jvm") version "2.4.21"
    id("org.jetbrains.kotlin.plugin.serialization") version "2.4.21"
    id("org.jetbrains.intellij.platform") version "2.20.0"
}

group = "com.swapdeck"
version = providers.gradleProperty("pluginVersion").get()

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
    }
}

dependencies {
    intellijPlatform {
        create(
            IntelliJPlatformType.fromCode(providers.gradleProperty("platformType").get()),
            providers.gradleProperty("platformVersion"),
        )
        // Facoltativo nel plugin.xml (`swapdeck-terminal.xml`): «run» apre un terminale dell'IDE.
        bundledPlugin("org.jetbrains.plugins.terminal")
    }
    // kotlinx.serialization (JSON) è già nell'IDE (1.7.x nella 2024.3, dentro `lib/util-8.jar`) e non
    // si impacchetta. La copia dell'IDE non dichiara la versione, che il plugin del compilatore vuole:
    // si compila contro la 1.7.3 (stessa API), a runtime vale quella dell'IDE.
    compileOnly("org.jetbrains.kotlinx:kotlinx-serialization-json-jvm:1.7.3") {
        exclude(group = "org.jetbrains.kotlin")
    }
    testImplementation(platform("org.junit:junit-bom:5.14.4"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
    // Il classpath di test della piattaforma carica comunque classi di JUnit 4 (`org.junit.rules.TestRule`).
    testRuntimeOnly("junit:junit:4.13.2")
}

kotlin {
    jvmToolchain(21)
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_21)
        // L'IDE 2024.3 porta la libreria standard 2.0.21: niente API più nuove.
        // Il compilatore la segnala come deprecata: si tace l'avviso finché il minimo resta 243.
        apiVersion.set(KotlinVersion.KOTLIN_2_0)
        freeCompilerArgs.add("-Xsuppress-version-warnings")
    }
}

intellijPlatform {
    pluginConfiguration {
        version = providers.gradleProperty("pluginVersion")
        ideaVersion {
            sinceBuild = providers.gradleProperty("pluginSinceBuild")
            untilBuild = provider { null }
        }
    }
    buildSearchableOptions = false
    // Strumentazione di @NotNull e dei form Swing: serve solo al codice Java, qui non c'è.
    instrumentCode = false
}

// Pannello React (apps/jetbrains/webview) costruito con esbuild: serve Node con le dipendenze di
// `pnpm install`. Finisce nel jar del plugin sotto `webview/`.
val webviewOut = layout.projectDirectory.dir("out/webview")
val buildWebview = tasks.register<Exec>("buildWebview") {
    group = "build"
    description = "Costruisce il pannello per JCEF (node scripts/build.mjs --production)."
    workingDir = layout.projectDirectory.asFile
    commandLine("node", "scripts/build.mjs", "--production")
    inputs.dir("webview")
    inputs.file("scripts/build.mjs")
    inputs.dir("../../packages/ui/src")
    inputs.dir("../../packages/host/src")
    inputs.dir("../../packages/types/src")
    outputs.dir(webviewOut)
}

tasks.processResources {
    from(buildWebview) {
        include("index.html", "main.js", "main.css")
        into("webview")
    }
    // Elenchi e limiti del protocollo del pannello, fonte unica in packages/host.
    from("../../packages/host/protocol.json") {
        into("swapdeck")
    }
}

// La versione del plugin segue quella del Rust (`[workspace.package]` del Cargo.toml alla radice).
val checkVersion = tasks.register("checkVersion") {
    group = "verification"
    description = "Controlla che pluginVersion sia uguale alla versione in Cargo.toml."
    val cargoToml = layout.projectDirectory.file("../../Cargo.toml")
    val pluginVersion = providers.gradleProperty("pluginVersion")
    inputs.file(cargoToml)
    inputs.property("pluginVersion", pluginVersion)
    doLast {
        val text = cargoToml.asFile.readText()
        val cargo = Regex("""(?m)^version = "([^"]+)"""").find(text)?.groupValues?.get(1)
            ?: throw GradleException("versione non trovata in Cargo.toml")
        if (cargo != pluginVersion.get()) {
            throw GradleException(
                "pluginVersion (${pluginVersion.get()}) diversa da Cargo.toml ($cargo): allinea gradle.properties",
            )
        }
    }
}

tasks.check {
    dependsOn(checkVersion)
}

tasks.buildPlugin {
    dependsOn(checkVersion)
}

tasks.test {
    useJUnitPlatform()
}
