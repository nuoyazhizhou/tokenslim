package com.tokenslim.plugin

import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.time.Duration
import java.util.concurrent.CompletableFuture

/**
 * TokenSlim REST API 客户端（JetBrains 插件用）。
 * 支持 compress / decompress / health 三个核心操作。
 */
class TokenSlimClient private constructor(
    private val baseUrl: String,
    private val httpClient: HttpClient
) {
    constructor(host: String = "127.0.0.1", port: Int = 10086) : this(
        "http://$host:$port",
        HttpClient.newBuilder()
            .connectTimeout(Duration.ofSeconds(5))
            .build()
    )

    /**
     * 检测 server 是否在线。
     */
    fun checkHealth(): CompletableFuture<Boolean> {
        val request = HttpRequest.newBuilder()
            .uri(URI.create("$baseUrl/health"))
            .GET()
            .timeout(Duration.ofSeconds(3))
            .build()
        return httpClient.sendAsync(request, HttpResponse.BodyHandlers.ofString())
            .thenApply { it.statusCode() == 200 }
            .exceptionally { false }
    }

    /**
     * 压缩文本，返回 server 的 JSON 响应字符串。
     */
    fun compress(text: String): CompletableFuture<String> {
        val escaped = text.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n").replace("\r", "\\r")
        val json = "{\"text\": \"$escaped\"}"

        val request = HttpRequest.newBuilder()
            .uri(URI.create("$baseUrl/compress"))
            .header("Content-Type", "application/json")
            .POST(HttpRequest.BodyPublishers.ofString(json))
            .timeout(Duration.ofSeconds(30))
            .build()
        return httpClient.sendAsync(request, HttpResponse.BodyHandlers.ofString())
            .thenApply { it.body() }
    }

    /**
     * 解压 TokenSlim JSON payload，返回恢复后的原始文本。
     * @param jsonPayload 完整的 TokenSlim JSON 字符串（包含 tokens + dictionary）
     */
    fun decompress(jsonPayload: String): CompletableFuture<String> {
        val request = HttpRequest.newBuilder()
            .uri(URI.create("$baseUrl/decompress"))
            .header("Content-Type", "application/json")
            .POST(HttpRequest.BodyPublishers.ofString(jsonPayload))
            .timeout(Duration.ofSeconds(30))
            .build()
        return httpClient.sendAsync(request, HttpResponse.BodyHandlers.ofString())
            .thenApply { it.body() }
    }

    /**
     * 使用 Builder 模式配置客户端。
     */
    class Builder {
        private var host = "127.0.0.1"
        private var port = 10086
        private var timeoutSeconds = 30L

        fun host(host: String) = apply { this.host = host }
        fun port(port: Int) = apply { this.port = port }
        fun timeout(seconds: Long) = apply { this.timeoutSeconds = seconds }

        fun build(): TokenSlimClient {
            val client = HttpClient.newBuilder()
                .connectTimeout(Duration.ofSeconds(5))
                .build()
            return TokenSlimClient("http://$host:$port", client)
        }
    }
}
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
