package com.tokenslim.plugin

import com.intellij.openapi.project.Project
import java.io.File
import java.nio.file.Paths

object TokenSlimServerManager {
    private var serverProcess: Process? = null

    fun ensureServerRunning(project: Project) {
        val client = TokenSlimClient()
        client.checkHealth().thenAccept { isRunning ->
            if (!isRunning) {
                startServer(project)
            }
        }
    }

    fun startServer(project: Project) {
        serverProcess?.destroy()
        
        // Strategy: Look for binary in the project root target folder (dev mode)
        // In production, we'd bundle this or allow user to configure path.
        val projectBase = project.basePath ?: return
        val binPath = Paths.get(projectBase, "target", "release", "tokenslim-server.exe").toFile()
        
        if (!binPath.exists()) {
            println("TokenSlim binary not found at ${binPath.absolutePath}")
            return
        }

        try {
            val processBuilder = ProcessBuilder(binPath.absolutePath)
            processBuilder.directory(File(projectBase))
            serverProcess = processBuilder.start()
            println("TokenSlim Sidecar Server started.")
        } catch (e: Exception) {
            e.printStackTrace()
        }
    }

    fun stopServer() {
        serverProcess?.destroy()
    }
}
