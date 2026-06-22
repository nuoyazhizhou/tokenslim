package com.tokenslim.plugin

import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.util.concurrent.CompletableFuture

class TokenSlimClient(private val host: String = "127.0.0.1", private val port: Int = 10086) {
    private val baseUrl = "http://$host:$port"
    private val client = HttpClient.newBuilder().build()

    fun checkHealth(): CompletableFuture<Boolean> {
        val request = HttpRequest.newBuilder()
            .uri(URI.create("$baseUrl/health"))
            .GET()
            .build()
        return client.sendAsync(request, HttpResponse.BodyHandlers.ofString())
            .thenApply { it.statusCode() == 200 }
            .exceptionally { false }
    }

    fun compress(text: String): CompletableFuture<String> {
        // Simple JSON escape for the MVP
        val escaped = text.replace("\"", "\\\"").replace("\n", "\\n").replace("\r", "\\r")
        val json = "{\"text\": \"$escaped\"}"
        
        val request = HttpRequest.newBuilder()
            .uri(URI.create("$baseUrl/compress"))
            .header("Content-Type", "application/json")
            .POST(HttpRequest.BodyPublishers.ofString(json))
            .build()
        return client.sendAsync(request, HttpResponse.BodyHandlers.ofString())
            .thenApply { it.body() }
    }
}
