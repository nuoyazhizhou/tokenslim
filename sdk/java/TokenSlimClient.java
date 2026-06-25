package com.tokenslim.sdk;

import java.io.IOException;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.util.List;
import java.util.concurrent.CompletableFuture;

/**
 * TokenSlim REST API Client for Java.
 * 需要 Java 11+，支持与 tokenslim-server 的 compress / decompress / compressFile / health 交互。
 *
 * <p>用法示例:
 * <pre>
 *   TokenSlimClient client = TokenSlimClient.builder().host("127.0.0.1").port(10086).build();
 *   String result = client.compress("some log text").get();
 *   String restored = client.decompress(result).get();
 * </pre>
 */
public class TokenSlimClient {
    private final String baseUrl;
    private final HttpClient httpClient;
    private final Duration requestTimeout;

    private TokenSlimClient(String host, int port, Duration timeout) {
        this.baseUrl = "http://" + host + ":" + port;
        this.requestTimeout = timeout;
        this.httpClient = HttpClient.newBuilder()
                .connectTimeout(Duration.ofSeconds(5))
                .build();
    }

    public TokenSlimClient(String host, int port) {
        this(host, port, Duration.ofSeconds(30));
    }

    public TokenSlimClient() {
        this("127.0.0.1", 10086);
    }

    /**
     * 创建 Builder 实例。
     */
    public static Builder builder() {
        return new Builder();
    }

    /**
     * 检测 server 是否在线。
     */
    public CompletableFuture<Boolean> isHealthy() {
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(baseUrl + "/health"))
                .GET()
                .timeout(Duration.ofSeconds(3))
                .build();
        return httpClient.sendAsync(request, HttpResponse.BodyHandlers.ofString())
                .thenApply(resp -> resp.statusCode() == 200)
                .exceptionally(ex -> false);
    }

    /**
     * 兼容旧接口，返回原始 JSON 字符串。
     */
    public CompletableFuture<String> health() {
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(baseUrl + "/health"))
                .GET()
                .timeout(Duration.ofSeconds(3))
                .build();
        return httpClient.sendAsync(request, HttpResponse.BodyHandlers.ofString())
                .thenApply(HttpResponse::body);
    }

    /**
     * 压缩文本，返回 server 的 JSON 响应。
     */
    public CompletableFuture<String> compress(String text) {
        String json = buildJsonText(text);
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(baseUrl + "/compress"))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(json))
                .timeout(requestTimeout)
                .build();
        return httpClient.sendAsync(request, HttpResponse.BodyHandlers.ofString())
                .thenApply(HttpResponse::body);
    }

    /**
     * 压缩文件内容，返回 server 的 JSON 响应。
     * @param filePath 文件路径
     */
    public CompletableFuture<String> compressFile(Path filePath) {
        return CompletableFuture.supplyAsync(() -> {
            try {
                return Files.readString(filePath);
            } catch (IOException e) {
                throw new RuntimeException("读取文件失败: " + filePath, e);
            }
        }).thenCompose(this::compress);
    }

    /**
     * 解压 TokenSlim JSON payload，返回 server 的 JSON 响应。
     * @param tokensJson tokens 数组的 JSON 字符串
     * @param dictionaryJson dictionary 对象的 JSON 字符串
     */
    public CompletableFuture<String> decompress(String tokensJson, String dictionaryJson) {
        String json = "{\"tokens\": " + tokensJson + ", \"dictionary\": " + dictionaryJson + "}";
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(baseUrl + "/decompress"))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(json))
                .timeout(requestTimeout)
                .build();
        return httpClient.sendAsync(request, HttpResponse.BodyHandlers.ofString())
                .thenApply(HttpResponse::body);
    }

    /**
     * 解压完整的 TokenSlim JSON（自动拆分 tokens 和 dictionary）。
     * @param fullPayload 完整的 JSON 字符串，包含 tokens 和 dictionary 字段
     */
    public CompletableFuture<String> decompress(String fullPayload) {
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(baseUrl + "/decompress"))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(fullPayload))
                .timeout(requestTimeout)
                .build();
        return httpClient.sendAsync(request, HttpResponse.BodyHandlers.ofString())
                .thenApply(HttpResponse::body);
    }

    /**
     * 批量压缩多段文本。
     */
    public CompletableFuture<List<String>> batchCompress(List<String> texts) {
        List<CompletableFuture<String>> futures = texts.stream()
                .map(this::compress)
                .toList();
        return CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]))
                .thenApply(v -> futures.stream()
                        .map(CompletableFuture::join)
                        .toList());
    }

    // ---------- 内部方法 ----------

    /**
     * 构造压缩请求的 JSON body。使用简单的转义以避免额外依赖。
     */
    private static String buildJsonText(String text) {
        StringBuilder sb = new StringBuilder("{\"text\": \"");
        for (char c : text.toCharArray()) {
            switch (c) {
                case '\\' -> sb.append("\\\\");
                case '"' -> sb.append("\\\"");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> sb.append(c);
            }
        }
        sb.append("\"}");
        return sb.toString();
    }

    // ---------- Builder ----------

    public static class Builder {
        private String host = "127.0.0.1";
        private int port = 10086;
        private Duration timeout = Duration.ofSeconds(30);

        public Builder host(String host) { this.host = host; return this; }
        public Builder port(int port) { this.port = port; return this; }
        public Builder timeout(Duration timeout) { this.timeout = timeout; return this; }

        public TokenSlimClient build() {
            return new TokenSlimClient(host, port, timeout);
        }
    }

    // ---------- Main ----------

    public static void main(String[] args) throws Exception {
        TokenSlimClient client = TokenSlimClient.builder()
                .host("127.0.0.1")
                .port(10086)
                .build();

        System.out.println("Health check: " + client.health().get());

        String log = "2024-12-26T07:47:22.609Z [ERROR] Failed to connect to database\n  at com.example.DB.connect(DB.java:42)\n  at com.example.Main.run(Main.java:15)";
        System.out.println("Compressing sample log...");
        String result = client.compress(log).get();
        System.out.println("Result: " + result);

        System.out.println("Decompressing...");
        String restored = client.decompress(result).get();
        System.out.println("Restored: " + restored);
    }
}
package com.tokenslim.sdk;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.util.concurrent.CompletableFuture;

/**
 * TokenSlim REST API Client for Java.
 * Requires Java 11 or higher.
 */
public class TokenSlimClient {
    private final String baseUrl;
    private final HttpClient httpClient;

    public TokenSlimClient(String host, int port) {
        this.baseUrl = "http://" + host + ":" + port;
        this.httpClient = HttpClient.newBuilder().build();
    }

    public TokenSlimClient() {
        this("127.0.0.1", 10086);
    }

    public CompletableFuture<String> health() {
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(baseUrl + "/health"))
                .GET()
                .build();
        return httpClient.sendAsync(request, HttpResponse.BodyHandlers.ofString())
                .thenApply(HttpResponse::body);
    }

    public CompletableFuture<String> compress(String text) {
        String json = "{\"text\": \"" + text.replace("\"", "\\\"").replace("\n", "\\n") + "\"}";
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(baseUrl + "/compress"))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(json))
                .build();
        return httpClient.sendAsync(request, HttpResponse.BodyHandlers.ofString())
                .thenApply(HttpResponse::body);
    }

    public CompletableFuture<String> decompress(String tokensJson, String dictionaryJson) {
        String json = "{\"tokens\": " + tokensJson + ", \"dictionary\": " + dictionaryJson + "}";
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(baseUrl + "/decompress"))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(json))
                .build();
        return httpClient.sendAsync(request, HttpResponse.BodyHandlers.ofString())
                .thenApply(HttpResponse::body);
    }

    public static void main(String[] args) throws Exception {
        TokenSlimClient client = new TokenSlimClient();
        System.out.println("Health check: " + client.health().get());
        
        String log = "2024-12-26T07:47:22.609Z [ERROR] Failed to connect to database";
        System.out.println("Compressing sample log...");
        String result = client.compress(log).get();
        System.out.println("Result: " + result);
    }
}
