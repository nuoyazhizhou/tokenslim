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
