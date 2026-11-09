package com.swapdeck.jetbrains.bridge

import com.intellij.ide.actions.RevealFileAction
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.application.ReadAction
import com.intellij.openapi.application.ModalityState
import com.intellij.openapi.diagnostic.logger
import com.intellij.openapi.fileChooser.FileChooser
import com.intellij.openapi.fileChooser.FileChooserDescriptorFactory
import com.intellij.openapi.fileChooser.FileChooserFactory
import com.intellij.openapi.fileChooser.FileSaverDescriptor
import com.intellij.openapi.ide.CopyPasteManager
import com.intellij.openapi.project.Project
import com.intellij.openapi.ui.InputValidator
import com.intellij.openapi.ui.Messages
import com.intellij.openapi.util.io.FileUtil
import com.intellij.openapi.vfs.LocalFileSystem
import com.intellij.openapi.vfs.VirtualFile
import com.swapdeck.jetbrains.IdeText
import com.swapdeck.jetbrains.SwapDeckBundle
import java.awt.datatransfer.StringSelection
import java.io.File
import java.nio.file.Path

/**
 * [BridgeUi] con i dialoghi dell'IDE, per il pannello della finestra di [project] (`null` = nessuna
 * finestra). Si chiama dai thread del plugin: dialoghi, selettori e appunti passano all'EDT con
 * `invokeAndWait` e il thread della richiesta aspetta la risposta; il comando di `run` invece con
 * `invokeLater`, senza trattenere la risposta. I testi della CLI o del pannello passano da
 * [IdeText] (notifiche e dialoghi sono HTML), le conferme hanno «Annulla» come predefinito.
 */
class IdeBridgeUi(private val project: Project?) : BridgeUi {
    private val owner: Project? get() = project?.takeUnless { it.isDisposed }

    override fun confirmAction(description: String, confirmation: Confirmation): Boolean = onEdt {
        val label = SwapDeckBundle.message("bridge.confirm.action", description)
        when {
            !confirmDialog(owner, label) -> false
            confirmation !is Confirmation.Typed -> true
            else -> {
                val word = confirmation.word
                val typed = Messages.showInputDialog(
                    owner,
                    IdeText.dialog(SwapDeckBundle.message("bridge.confirm.typed", label, word)),
                    TITLE,
                    Messages.getWarningIcon(),
                    "",
                    object : InputValidator {
                        override fun checkInput(inputString: String?): Boolean = inputString == word

                        override fun canClose(inputString: String?): Boolean = inputString == word
                    },
                )
                typed == word
            }
        }
    }

    override fun confirmSetup(kind: String): Boolean = onEdt {
        confirmDialog(owner, SwapDeckBundle.message(if (kind == "install") "bridge.setup.install" else "bridge.setup.upgrade"))
    }

    override fun pick(kind: PickKind, options: PickOptions): String? {
        val extensions = options.filters.orEmpty().flatMap { it.extensions }.map { it.lowercase() }.toSet()
        // Percorso iniziale solo se assoluto e locale, risolto qui (thread del plugin) e non sull'EDT.
        val default = LocalPaths.localAbsolute(options.defaultPath)?.let(::File)
        return when (kind) {
            PickKind.SAVE_FILE -> {
                val baseDir = default?.parentFile?.let(::findLocalDir)
                onEdt {
                    val descriptor = FileSaverDescriptor(options.title ?: TITLE, "", *extensions.toTypedArray())
                    FileChooserFactory.getInstance().createSaveFileDialog(descriptor, owner)
                        .save(baseDir, default?.name)?.file?.path
                }
            }
            PickKind.OPEN_FILE, PickKind.FOLDER -> {
                val toSelect = default?.let(::findLocal)
                onEdt {
                    var descriptor = if (kind == PickKind.FOLDER) {
                        FileChooserDescriptorFactory.createSingleFolderDescriptor()
                    } else {
                        FileChooserDescriptorFactory.createSingleFileDescriptor()
                    }
                    options.title?.let { descriptor = descriptor.withTitle(it) }
                    if (kind == PickKind.OPEN_FILE && extensions.isNotEmpty()) {
                        descriptor = descriptor.withFileFilter { file -> file.extension?.lowercase() in extensions }
                    }
                    FileChooser.chooseFile(descriptor, owner, toSelect)?.let { FileUtil.toSystemDependentName(it.path) }
                }
            }
        }
    }

    override fun copyText(text: String) = onEdt {
        CopyPasteManager.getInstance().setContents(StringSelection(text))
    }

    override fun openFolder(dir: String) {
        RevealFileAction.openDirectory(Path.of(dir))
    }

    /** Non blocca la richiesta: terminale o dialogo arrivano dopo, sull'EDT. */
    override fun openCommand(program: String, args: List<String>) {
        ApplicationManager.getApplication().invokeLater({
            val target = owner
            val opened = target != null && CommandRunner.EP.extensionList.any { runner ->
                try {
                    runner.run(target, program, args)
                } catch (e: Exception) {
                    LOG.warn("run: terminale non aperto", e)
                    false
                }
            }
            if (!opened) showCommand(commandLine(program, args))
        }, ModalityState.nonModal())
    }

    /** Senza plugin Terminal: il comando in un dialogo, con «Copia». */
    private fun showCommand(line: String) {
        val copy = SwapDeckBundle.message("bridge.command.copy")
        val choice = Messages.showDialog(
            owner,
            IdeText.dialog(SwapDeckBundle.message("bridge.command.message", line)),
            TITLE,
            arrayOf(copy, SwapDeckBundle.message("bridge.command.close")),
            0,
            Messages.getInformationIcon(),
        )
        if (choice == 0) CopyPasteManager.getInstance().setContents(StringSelection(line))
    }

    override fun warn(text: String) {
        NotificationGroupManager.getInstance().getNotificationGroup(NOTIFICATION_GROUP)
            .createNotification(IdeText.html(text), NotificationType.WARNING)
            .notify(owner)
    }

    /** File o cartella locale esistente (controllo fuori dall'EDT), altrimenti `null`. */
    private fun findLocal(file: File): VirtualFile? =
        if (file.exists()) ReadAction.compute<VirtualFile?, RuntimeException> { LocalFileSystem.getInstance().findFileByIoFile(file) } else null

    private fun findLocalDir(dir: File): VirtualFile? = if (dir.isDirectory) findLocal(dir) else null

    /** Esegue [block] sull'EDT e ne restituisce l'esito (anche l'eccezione) al thread chiamante. */
    private fun <T> onEdt(block: () -> T): T {
        val app = ApplicationManager.getApplication()
        if (app.isDispatchThread) return block()
        var result: Result<T>? = null
        app.invokeAndWait({ result = runCatching(block) }, ModalityState.defaultModalityState())
        return result!!.getOrThrow()
    }

    companion object {
        private const val TITLE = "SwapDeck"

        /**
         * Conferma modale con «Conferma» e «Annulla»: predefinito e con il fuoco è «Annulla», così un
         * Invio dato mentre si scriveva non conferma. Il testo passa da [IdeText.dialog]. Dall'EDT.
         */
        fun confirmDialog(project: Project?, message: String, confirmText: String = SwapDeckBundle.message("bridge.confirm.ok")): Boolean =
            Messages.showDialog(
                project,
                IdeText.dialog(message),
                TITLE,
                null,
                arrayOf(confirmText, Messages.getCancelButton()),
                1,
                1,
                Messages.getWarningIcon(),
            ) == 0

        private const val NOTIFICATION_GROUP = "SwapDeck"
        private val LOG = logger<IdeBridgeUi>()
    }
}
