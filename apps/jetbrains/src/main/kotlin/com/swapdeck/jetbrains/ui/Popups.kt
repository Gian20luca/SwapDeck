package com.swapdeck.jetbrains.ui

import com.intellij.icons.AllIcons
import com.intellij.openapi.project.Project
import com.intellij.openapi.ui.popup.JBPopupFactory
import com.intellij.openapi.ui.popup.ListSeparator
import com.intellij.openapi.ui.popup.PopupStep
import com.intellij.openapi.ui.popup.util.BaseListPopupStep
import javax.swing.Icon

/** Popup a lista dalle voci di [SwitchMenu] o [MapMenu]; [onChosen] parte dopo la chiusura. Dall'EDT. */
fun <T : Any> showMenuPopup(project: Project?, title: String, items: List<MenuItem<T>>, onChosen: (T) -> Unit) {
    val step = object : BaseListPopupStep<MenuItem<T>>(title, items) {
        override fun getTextFor(value: MenuItem<T>): String = swingPlain(value.text)

        override fun getIconFor(value: MenuItem<T>): Icon? = if (value.checked) AllIcons.Actions.Checked else null

        override fun isSelectable(value: MenuItem<T>): Boolean = value.selectable

        override fun getSeparatorAbove(value: MenuItem<T>): ListSeparator? = value.separatorAbove?.let { ListSeparator(it) }

        override fun onChosen(selectedValue: MenuItem<T>, finalChoice: Boolean): PopupStep<*>? {
            val value = selectedValue.value ?: return FINAL_CHOICE
            return doFinalStep { onChosen(value) }
        }
    }
    val popup = JBPopupFactory.getInstance().createListPopup(step)
    if (project != null && !project.isDisposed) popup.showCenteredInCurrentWindow(project) else popup.showInFocusCenter()
}
