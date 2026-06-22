package com.tokenslim.plugin.actions

import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.project.Project
import com.tokenslim.plugin.TokenSlimServerManager

class RestartServerAction : AnAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project: Project = e.project ?: return
        
        TokenSlimServerManager.startServer(project)
        
        NotificationGroupManager.getInstance()
            .getNotificationGroup("TokenSlim Notifications")
            .createNotification("TokenSlim", "Sidecar server restart requested.", NotificationType.INFORMATION)
            .notify(project)
    }
}
