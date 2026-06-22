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

class CompressAction : AnAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project: Project = e.project ?: return
        val editor = e.getData(CommonDataKeys.EDITOR)
        val document = editor?.document
        
        val textToCompress = if (editor != null && editor.selectionModel.hasSelection()) {
            editor.selectionModel.selectedText
        } else {
            document?.text
        }

        if (textToCompress.isNullOrBlank()) {
            showNotification(project, "No text to compress.", NotificationType.WARNING)
            return
        }

        // Ensure server is running
        TokenSlimServerManager.ensureServerRunning(project)

        val client = TokenSlimClient()
        client.compress(textToCompress).thenAccept { response ->
            // Display result in a new scratch file / editor tab
            val resultFile = LightVirtualFile("tokenslim_output.json", response)
            FileEditorManager.getInstance(project).openFile(resultFile, true)
            
            showNotification(project, "Compression successful!", NotificationType.INFORMATION)
        }.exceptionally { ex ->
            showNotification(project, "Error: ${ex.message}", NotificationType.ERROR)
            null
        }
    }

    private fun showNotification(project: Project, content: String, type: NotificationType) {
        NotificationGroupManager.getInstance()
            .getNotificationGroup("TokenSlim Notifications")
            .createNotification("TokenSlim", content, type)
            .notify(project)
    }
}
