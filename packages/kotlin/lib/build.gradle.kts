import com.vanniktech.maven.publish.JavadocJar
import com.vanniktech.maven.publish.KotlinJvm
import com.vanniktech.maven.publish.SonatypeHost

plugins {
    id("org.jetbrains.kotlin.jvm") version "1.9.25"
    id("java-library")
    // Dokka generates HTML API documentation from KDoc comments.
    // Pinned to 1.9.20: the last release that supports Kotlin 1.9.x.
    // JavadocJar.Dokka("dokkaHtml") below packs the HTML output into
    // the *-javadoc.jar artifact required by Maven Central.
    id("org.jetbrains.dokka") version "1.9.20"
    // The Vanniktech maven-publish plugin auto-applies `maven-publish`
    // and `signing`, configures the Central Portal upload endpoint, and
    // wraps the close+release steps into a single Gradle task. This
    // replaces the legacy raw `maven-publish` + `signing` setup that
    // targeted the (sunset) `s01.oss.sonatype.org` OSSRH endpoint.
    // See: https://vanniktech.github.io/gradle-maven-publish-plugin/central
    //
    // Pinned to 0.30.0: 0.34.0+ requires Kotlin 2.2.0, but the JVM
    // target above is on Kotlin 1.9.25. 0.30.0 is the last release that
    // supports Kotlin 1.9.x while also supporting the Central Portal.
    id("com.vanniktech.maven.publish") version "0.30.0"
}

// Maven Central namespace verified for the koeda.me domain (DNS TXT
// proof). The GitHub org name `koedame` does not match a domain we
// own; `me.koeda` is the reverse-DNS of `koeda.me` and is the
// namespace registered on the Sonatype Central Portal.
group = "me.koeda"
// Read version from crates/ffi/Cargo.toml to keep in sync with Rust crate.
version = Regex("""^version\s*=\s*"([^"]+)"""", RegexOption.MULTILINE)
    .find(file("${rootDir}/../../crates/ffi/Cargo.toml").readText())
    ?.groupValues?.get(1)
    ?: error("Failed to parse version from crates/ffi/Cargo.toml — expected a line matching: version = \"...\"")


repositories {
    mavenCentral()
}

dependencies {
    implementation("net.java.dev.jna:jna:5.17.0")
    testImplementation("org.jetbrains.kotlin:kotlin-test")
    testImplementation("org.jetbrains.kotlin:kotlin-test-junit5")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

java {
    toolchain {
        languageVersion.set(JavaLanguageVersion.of(17))
    }
}

tasks.test {
    useJUnitPlatform()
}

mavenPublishing {
    // Override coordinates so the published artifactId is `chordsketch`
    // rather than the Gradle module name (`lib`).
    coordinates("me.koeda", "chordsketch", project.version.toString())

    // Maven Central requires every published artifact to ship a sources
    // jar and a javadoc jar. KotlinJvm() configures the plugin to build
    // both. JavadocJar.Dokka("dokkaHtml") runs the dokkaHtml task and
    // packs its output into the *-javadoc.jar, making the API docs
    // browsable at javadoc.io and central.sonatype.com.
    configure(
        KotlinJvm(
            javadocJar = JavadocJar.Dokka("dokkaHtml"),
            sourcesJar = true,
        ),
    )

    // Publish to the Central Portal (https://central.sonatype.com/), and
    // automatically progress the deployment from VALIDATED to RELEASED
    // so a successful CI run leaves the artifact actually consumable.
    publishToMavenCentral(SonatypeHost.CENTRAL_PORTAL, automaticRelease = true)

    // Maven Central requires every artifact to be GPG-signed. The key
    // material is supplied via Gradle properties (see kotlin.yml's
    // ORG_GRADLE_PROJECT_signingInMemoryKey* env vars).
    signAllPublications()

    pom {
        name.set("ChordSketch")
        description.set("ChordPro file format parser and renderer")
        inceptionYear.set("2025")
        url.set("https://github.com/koedame/chordsketch")

        licenses {
            license {
                name.set("MIT")
                url.set("https://github.com/koedame/chordsketch/blob/main/LICENSE")
                distribution.set("repo")
            }
        }

        developers {
            developer {
                id.set("koedame")
                name.set("koedame")
                url.set("https://github.com/koedame")
            }
        }

        scm {
            url.set("https://github.com/koedame/chordsketch")
            connection.set("scm:git:git://github.com/koedame/chordsketch.git")
            developerConnection.set("scm:git:ssh://git@github.com/koedame/chordsketch.git")
        }
    }
}
