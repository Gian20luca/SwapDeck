package com.swapdeck.jetbrains.settings

import com.intellij.openapi.fileChooser.FileChooserDescriptorFactory
import com.intellij.openapi.options.ConfigurationException
import com.intellij.openapi.options.SearchableConfigurable
import com.intellij.openapi.ui.TextFieldWithBrowseButton
import com.intellij.ui.components.JBCheckBox
import com.intellij.ui.components.JBTextField
import com.intellij.util.ui.FormBuilder
import com.swapdeck.jetbrains.SwapDeckBundle
import javax.swing.JComponent

/**
 * Pagina Strumenti → SwapDeck: intervallo di lettura, percorso di `swapdeck-cli`, processo del
 * motore, soglie dei colori della barra. Salva con [SwapDeckSettings.update] (che avvisa motore e
 * stato); regole in [SettingsValidation].
 */
class SwapDeckConfigurable : SearchableConfigurable {
    private var interval: JBTextField? = null
    private var cliPath: TextFieldWithBrowseButton? = null
    private var engine: JBCheckBox? = null
    private var thresholds: JBTextField? = null

    override fun getId(): String = ID

    override fun getDisplayName(): String = "SwapDeck"

    override fun createComponent(): JComponent {
        val t = SwapDeckBundle.translate()
        val intervalField = JBTextField(6)
        val pathField = TextFieldWithBrowseButton().apply {
            addBrowseFolderListener(
                null,
                FileChooserDescriptorFactory.createSingleFileNoJarsDescriptor().withTitle(t.message("settings.cliPath.choose")),
            )
        }
        val engineBox = JBCheckBox(t.message("settings.engine"))
        val thresholdsField = JBTextField(12)
        interval = intervalField
        cliPath = pathField
        engine = engineBox
        thresholds = thresholdsField
        reset()
        return FormBuilder.createFormBuilder()
            .addLabeledComponent(t.message("settings.interval"), intervalField)
            .addTooltip(t.message("settings.interval.hint"))
            .addLabeledComponent(t.message("settings.cliPath"), pathField)
            .addTooltip(t.message("settings.cliPath.hint"))
            .addComponent(engineBox)
            .addTooltip(t.message("settings.engine.hint"))
            .addLabeledComponent(t.message("settings.thresholds"), thresholdsField)
            .addTooltip(t.message("settings.thresholds.hint"))
            .addComponentFillVertically(javax.swing.JPanel(), 0)
            .panel
    }

    private fun current(): SettingsCheck = SettingsValidation.check(
        interval?.text.orEmpty(),
        cliPath?.text.orEmpty(),
        engine?.isSelected ?: true,
        thresholds?.text.orEmpty(),
    )

    override fun isModified(): Boolean {
        val saved = SwapDeckSettings.getInstance().snapshot
        return when (val check = current()) {
            is SettingsCheck.Valid -> check.snapshot != saved
            is SettingsCheck.Invalid -> true
        }
    }

    override fun apply() {
        when (val check = current()) {
            is SettingsCheck.Invalid -> throw ConfigurationException(SwapDeckBundle.message(check.key, *check.params.toTypedArray()))
            is SettingsCheck.Valid -> SwapDeckSettings.getInstance().update { check.snapshot }
        }
    }

    override fun reset() {
        val s = SwapDeckSettings.getInstance().snapshot
        interval?.text = s.refreshIntervalSeconds.toString()
        cliPath?.text = s.cliPath
        engine?.isSelected = s.engineEnabled
        thresholds?.text = SettingsValidation.thresholdsText(s.thresholds)
    }

    override fun disposeUIResources() {
        interval = null
        cliPath = null
        engine = null
        thresholds = null
    }

    companion object {
        const val ID = "com.swapdeck.jetbrains.settings"
    }
}
