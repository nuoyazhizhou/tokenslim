package com.tokenslim.plugin.actions

import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.fileEditor.FileEditorManager
import com.intellij.openapi.project.Project
import com.intellij.testFramework.LightVirtualFile
import com.tokenslim.plugin.TokenSlimClient
import com.tokenslim.plugin.TokenSlimServerManager

/**
 * 解压 TokenSlim 压缩的 JSON payload，恢复原始文本并在新标签页中显示。
 */
class DecompressAction : AnAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project: Project = e.project ?: return
        val editor = e.getData(CommonDataKeys.EDITOR)

        val textToDecompress = if (editor != null && editor.selectionModel.hasSelection()) {
            editor.selectionModel.selectedText
        } else {
            editor?.document?.text
        }

        if (textToDecompress.isNullOrBlank()) {
            showNotification(project, "No text to decompress.", NotificationType.WARNING)
            return
        }

        // 确保 server 在运行
        TokenSlimServerManager.ensureServerRunning(project)

        val client = TokenSlimClient()
        client.decompress(textToDecompress).thenAccept { response ->
            val resultFile = LightVirtualFile("tokenslim_decompressed.log", response)
            FileEditorManager.getInstance(project).openFile(resultFile, true)
            showNotification(project, "Decompression successful!", NotificationType.INFORMATION)
        }.exceptionally { ex ->
            showNotification(project, "Error: ${ex.message}", NotificationType.ERROR)
            null
        }
    }

    /**
     * 只在编辑器有内容时启用此 action。
     */
    override fun update(e: AnActionEvent) {
        val editor = e.getData(CommonDataKeys.EDITOR)
        e.presentation.isEnabledAndVisible = editor != null
    }

    private fun showNotification(project: Project, content: String, type: NotificationType) {
        NotificationGroupManager.getInstance()
            .getNotificationGroup("TokenSlim Notifications")
            .createNotification("TokenSlim", content, type)
            .notify(project)
    }
}
