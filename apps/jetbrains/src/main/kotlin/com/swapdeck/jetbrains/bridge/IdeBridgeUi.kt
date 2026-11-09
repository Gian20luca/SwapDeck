package com.swapdeck.jetbrains.bridge

import com.intellij.ide.actions.RevealFileAction
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.application.ApplicationManager
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
import com.swapdeck.jetbrains.SwapDeckBundle
import java.awt.datatransfer.StringSelection
import java.io.File
import java.nio.file.Path

/**
 * [BridgeUi] con i dialoghi dell'IDE, per il pannello della finestra di [project] (`null` = nessuna
 * finestra). Si chiama dai thread del plugin: dialoghi, selettori e appunti passano all'EDT con
 * `invokeAndWait` e il thread della richiesta aspetta la risposta.
 */
class IdeBridgeUi(private val project: Project?) : BridgeUi {
    private val owner: Project? get() = project?.takeUnless { it.isDisposed }

    override fun confirmAction(description: String, confirmation: Confirmation): Boolean = onEdt {
        val label = SwapDeckBundle.message("bridge.confirm.action", description)
        val choice = Messages.showOkCancelDialog(
            owner,
            label,
            TITLE,
            SwapDeckBundle.message("bridge.confirm.ok"),
            Messages.getCancelButton(),
            Messages.getWarningIcon(),
        )
        when {
            choice != Messages.OK -> false
            confirmation !is Confirmation.Typed -> true
            else -> {
                val word = confirmation.word
                val typed = Messages.showInputDialog(
                    owner,
                    SwapDeckBundle.message("bridge.confirm.typed", label, word),
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
        val label = SwapDeckBundle.message(if (kind == "install") "bridge.setup.install" else "bridge.setup.upgrade")
        Messages.showOkCancelDialog(
            owner,
            label,
            TITLE,
            SwapDeckBundle.message("bridge.confirm.ok"),
            Messages.getCancelButton(),
            Messages.getWarningIcon(),
        ) == Messages.OK
    }

    override fun pick(kind: PickKind, options: PickOptions): String? = onEdt {
        val extensions = options.filters.orEmpty().flatMap { it.extensions }.map { it.lowercase() }.toSet()
        val chosen: String? = when (kind) {
            PickKind.SAVE_FILE -> {
                val descriptor = FileSaverDescriptor(options.title ?: TITLE, "", *extensions.toTypedArray())
                val default = options.defaultPath?.let(::File)
                val baseDir = default?.takeIf { it.isAbsolute }?.parentFile?.let(::findLocal)
                FileChooserFactory.getInstance().createSaveFileDialog(descriptor, owner)
                    .save(baseDir, default?.name)?.file?.path
            }
            PickKind.OPEN_FILE, PickKind.FOLDER -> {
                var descriptor = if (kind == PickKind.FOLDER) {
                    FileChooserDescriptorFactory.createSingleFolderDescriptor()
                } else {
                    FileChooserDescriptorFactory.createSingleFileDescriptor()
                }
                options.title?.let { descriptor = descriptor.withTitle(it) }
                if (kind == PickKind.OPEN_FILE && extensions.isNotEmpty()) {
                    descriptor = descriptor.withFileFilter { file -> file.extension?.lowercase() in extensions }
                }
                val toSelect = options.defaultPath?.let(::File)?.takeIf { it.isAbsolute }?.let(::findLocal)
                FileChooser.chooseFile(descriptor, owner, toSelect)?.let { FileUtil.toSystemDependentName(it.path) }
            }
        }
        chosen
    }

    override fun copyText(text: String) = onEdt {
        CopyPasteManager.getInstance().setContents(StringSelection(text))
    }

    override fun openFolder(dir: String) {
        RevealFileAction.openDirectory(Path.of(dir))
    }

    override fun openCommand(program: String, args: List<String>) {
        onEdt {
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
        }
    }

    /** Senza plugin Terminal: il comando in un dialogo, con «Copia». */
    private fun showCommand(line: String) {
        val copy = SwapDeckBundle.message("bridge.command.copy")
        val choice = Messages.showDialog(
            owner,
            SwapDeckBundle.message("bridge.command.message", line),
            TITLE,
            arrayOf(copy, SwapDeckBundle.message("bridge.command.close")),
            0,
            Messages.getInformationIcon(),
        )
        if (choice == 0) CopyPasteManager.getInstance().setContents(StringSelection(line))
    }

    override fun warn(text: String) {
        NotificationGroupManager.getInstance().getNotificationGroup(NOTIFICATION_GROUP)
            .createNotification(text, NotificationType.WARNING)
            .notify(owner)
    }

    private fun findLocal(file: File): VirtualFile? = LocalFileSystem.getInstance().findFileByIoFile(file)

    /** Esegue [block] sull'EDT e ne restituisce l'esito (anche l'eccezione) al thread chiamante. */
    private fun <T> onEdt(block: () -> T): T {
        val app = ApplicationManager.getApplication()
        if (app.isDispatchThread) return block()
        var result: Result<T>? = null
        app.invokeAndWait({ result = runCatching(block) }, ModalityState.defaultModalityState())
        return result!!.getOrThrow()
    }

    private companion object {
        const val TITLE = "SwapDeck"
        const val NOTIFICATION_GROUP = "SwapDeck"
        val LOG = logger<IdeBridgeUi>()
    }
}
