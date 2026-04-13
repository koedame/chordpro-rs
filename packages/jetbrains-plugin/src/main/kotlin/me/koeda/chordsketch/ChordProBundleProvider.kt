package me.koeda.chordsketch

import com.intellij.ide.plugins.PluginManagerCore
import com.intellij.openapi.extensions.PluginId
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider

/// Provides the ChordPro TextMate grammar bundle to the IntelliJ TextMate plugin.
///
/// The bundle files (package.json, chordpro.tmLanguage.json, language-configuration.json)
/// are placed in the plugin's `textmate/chordpro/` directory during the Gradle build.
/// This provider locates them via the plugin's installation path at runtime.
class ChordProBundleProvider : TextMateBundleProvider {
    override fun getBundles(): List<TextMateBundleProvider.PluginBundle> {
        val plugin = PluginManagerCore.getPlugin(PluginId.getId("me.koeda.chordsketch"))
            ?: return emptyList()
        val bundlePath = plugin.pluginPath.resolve("textmate").resolve("chordpro")
        return listOf(TextMateBundleProvider.PluginBundle("ChordPro", bundlePath))
    }
}
