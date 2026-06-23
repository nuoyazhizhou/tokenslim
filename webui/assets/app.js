// TokenSlim Web UI - 主逻辑（原生 ES2017+, 零依赖）
"use strict";

const $ = (id) => document.getElementById(id);
const toast = (msg, isError = false) => {
    const t = $("toast");
    t.textContent = msg;
    t.className = "toast show" + (isError ? " error" : "");
    clearTimeout(t._h);
    t._h = setTimeout(() => (t.className = "toast"), 2200);
};

const I18N = {
    "zh-CN": {
        "ui.subtitle": "日志压缩 / 反向还原",
        "ui.lang": "语言",
        "ui.input": "输入",
        "ui.output": "输出",
        "ui.upload": "上传文件",
        "ui.sample": "示例",
        "ui.clear": "清空",
        "ui.drop": "松开鼠标以上传",
        "ui.reorder": "启用重排",
        "ui.ai_export": "AI 导出格式",
        "ui.stream": "SSE 流式",
        "ui.stream_progress": "SSE 流式压缩中...",
        "ui.tail": "实时 tail",
        "ui.tail_stop": "停止 tail",
        "ui.tail_prompt": "请输入要 tail 的相对路径：",
        "ui.tail_connected": "WebSocket tail 已连接",
        "ui.tail_disconnected": "WebSocket tail 已断开",
        "ui.compress": "压缩",
        "ui.decompress": "反向还原",
        "ui.copy": "复制",
        "ui.download": "下载",
        "ui.toggle_view": "切换 diff 视图",
        "ui.history": "历史记录",
        "ui.plugins": "插件命中",
        "ui.empty_history": "(暂无)",
        "ui.empty_plugins": "(等待压缩后显示)",
        "ui.history_restored": "已恢复历史输入",
        "ui.copy_ok": "已复制",
        "ui.copy_fail": "复制失败",
        "ui.compressing": "压缩中...",
        "ui.decompressing": "还原中...",
        "ui.compress_done": "压缩完成",
        "ui.compress_fail": "压缩失败",
        "ui.too_large": "文件超过 5MB 上限",
        "ui.file_read": "已读取文件",
        "ui.ai_text_label": "AI Export",
    },
    en: {
        "ui.subtitle": "Log Compress / Rehydrate",
        "ui.lang": "Lang",
        "ui.input": "Input",
        "ui.output": "Output",
        "ui.upload": "Upload",
        "ui.sample": "Sample",
        "ui.clear": "Clear",
        "ui.drop": "Drop to upload",
        "ui.reorder": "Enable Reorder",
        "ui.ai_export": "AI Export",
        "ui.stream": "SSE Stream",
        "ui.stream_progress": "SSE compressing...",
        "ui.tail": "Tail",
        "ui.tail_stop": "Stop tail",
        "ui.tail_prompt": "Relative path to tail:",
        "ui.tail_connected": "WebSocket tail connected",
        "ui.tail_disconnected": "WebSocket tail disconnected",
        "ui.compress": "Compress",
        "ui.decompress": "Rehydrate",
        "ui.copy": "Copy",
        "ui.download": "Download",
        "ui.toggle_view": "Toggle Diff View",
        "ui.history": "History",
        "ui.plugins": "Plugin Hits",
        "ui.empty_history": "(empty)",
        "ui.empty_plugins": "(after compress)",
        "ui.history_restored": "History restored",
        "ui.copy_ok": "Copied",
        "ui.copy_fail": "Copy failed",
        "ui.compressing": "Compressing...",
        "ui.decompressing": "Rehydrating...",
        "ui.compress_done": "Done",
        "ui.compress_fail": "Failed",
        "ui.too_large": "File exceeds 5MB limit",
        "ui.file_read": "File loaded",
        "ui.ai_text_label": "AI Export",
    },
};

const State = {
    lang: localStorage.getItem("ts.lang") || (navigator.language?.startsWith("zh") ? "zh-CN" : "en"),
    lastInput: "",
    lastOutput: null,
    lastAiText: null,
    history: JSON.parse(localStorage.getItem("ts.history") || "[]"),
    view: "json", // json | diff | ai
};

function t(key) {
    return I18N[State.lang]?.[key] || I18N["zh-CN"][key] || key;
}

function applyI18n() {
    document.documentElement.lang = State.lang;
    document.querySelectorAll("[data-i18n]").forEach((el) => {
        const key = el.getAttribute("data-i18n");
        el.textContent = t(key);
    });
    $("lang-select").value = State.lang;
}

function fmtBytes(n) {
    if (n < 1024) return n + " B";
    if (n < 1024 * 1024) return (n / 1024).toFixed(1) + " KB";
    return (n / 1024 / 1024).toFixed(2) + " MB";
}

function updateInputSize() {
    $("input-size").textContent = fmtBytes($("input-text").value.length);
}

$("input-text").addEventListener("input", updateInputSize);

// 拖拽
const dz = $("drop-zone");
dz.addEventListener("dragover", (e) => { e.preventDefault(); dz.classList.add("dragover"); });
dz.addEventListener("dragleave", () => dz.classList.remove("dragover"));
dz.addEventListener("drop", (e) => {
    e.preventDefault();
    dz.classList.remove("dragover");
    const f = e.dataTransfer.files?.[0];
    if (f) handleFile(f);
});
$("btn-upload").addEventListener("click", () => $("file-input").click());
$("file-input").addEventListener("change", (e) => {
    const f = e.target.files?.[0];
    if (f) handleFile(f);
    e.target.value = "";
});
function handleFile(f) {
    if (f.size > 5 * 1024 * 1024) { toast(t("ui.too_large"), true); return; }
    const r = new FileReader();
    r.onload = () => {
        $("input-text").value = String(r.result || "");
        updateInputSize();
        toast(`${t("ui.file_read")}: ${f.name} (${fmtBytes(f.size)})`);
    };
    r.readAsText(f);
}

// 示例
$("btn-sample").addEventListener("click", () => {
    $("input-text").value = `2026-05-13T08:13:39.939000+00:00 ecs/backend/f6bf6a1fb38440c992e4e33e1691b589 INFO:     10.0.1.94:0 - "GET /health HTTP/1.1" 200 OK
2026-05-13T08:13:40.150000+00:00 ecs/backend/f6bf6a1fb38440c992e4e33e1691b589 INFO:     10.0.0.231:0 - "GET /health HTTP/1.1" 200 OK
2026-05-13T08:13:41.302000+00:00 ecs/backend/f6bf6a1fb38440c992e4e33e1691b589 ERROR:    10.0.2.17:0 - "POST /api/users HTTP/1.1" 500 Internal Server Error
  at handler.UserController.create (UserController.java:42)
  at javax.servlet.http.HttpServlet.service (HttpServlet.java:622)
  at org.apache.catalina.core.ApplicationFilterChain.doFilter (ApplicationFilterChain.java:166)
2026-05-13T08:13:42.456000+00:00 ecs/backend/f6bf6a1fb38440c992e4e33e1691b589 INFO:     10.0.3.55:0 - "GET /api/users/1 HTTP/1.1" 200 OK
2026-05-13T08:13:43.601000+00:00 ecs/backend/f6bf6a1fb38440c992e4e33e1691b589 INFO:     10.0.4.88:0 - "GET /api/users/2 HTTP/1.1" 200 OK`;
    updateInputSize();
});

$("btn-clear").addEventListener("click", () => {
    $("input-text").value = "";
    $("output-json").textContent = "";
    $("output-diff").innerHTML = "";
    $("output-ai").textContent = "";
    $("output-stats").innerHTML = "";
    $("plugin-list").innerHTML = `<li class="empty">${t("ui.empty_plugins")}</li>`;
    updateInputSize();
});

// 视图切换
$("btn-toggle-view").addEventListener("click", () => {
    const order = ["json", "diff", "ai"];
    const idx = order.indexOf(State.view);
    State.view = order[(idx + 1) % order.length];
    document.querySelectorAll(".output-view").forEach((v) => v.classList.remove("active"));
    $("output-" + State.view).classList.add("active");
});

function renderOutput(json, aiText) {
    State.lastOutput = json;
    State.lastAiText = aiText;
    $("output-json").textContent = JSON.stringify(json, null, 2);
    $("output-ai").textContent = aiText || "";

    // diff
    const inputLines = State.lastInput.split("\n");
    const outputText = json.semantic_log || json.text || JSON.stringify(json, null, 2);
    const outputLines = outputText.split("\n");
    $("output-diff").innerHTML = renderSideBySide(inputLines, outputLines);

    // 统计
    const inSize = State.lastInput.length;
    const outSize = outputText.length;
    const ratio = inSize > 0 ? ((1 - outSize / inSize) * 100).toFixed(1) : "0.0";
    $("output-stats").innerHTML = `
    <span>输入: <strong>${fmtBytes(inSize)}</strong> (${inputLines.length} 行)</span>
    <span>输出: <strong>${fmtBytes(outSize)}</strong> (${outputLines.length} 行)</span>
    <span class="ratio">压缩比: ${ratio}%</span>
  `;

    // 插件命中
    const plugins = json.slices?.map(s => s.plugin).filter(Boolean) || [];
    const uniq = [...new Set(plugins)];
    if (uniq.length === 0) {
        $("plugin-list").innerHTML = `<li class="empty">${t("ui.empty_plugins")}</li>`;
    } else {
        $("plugin-list").innerHTML = uniq.map(p => `<li><span>${p}</span><span class="hit">${plugins.filter(x => x === p).length}</span></li>`).join("");
    }

    // 历史
    pushHistory({ ts: Date.now(), inSize, outSize, ratio, input: State.lastInput });
}

// 简化版 LCS diff (按行)
function renderDiff(a, b) {
    const m = a.length, n = b.length;
    if (m + n > 4000) {
        return `<div class="diff-line same">（行数过多 ${m} → ${n}，跳过 diff 视图）</div>`;
    }
    const dp = Array.from({ length: m + 1 }, () => new Uint32Array(n + 1));
    for (let i = 1; i <= m; i++) {
        for (let j = 1; j <= n; j++) {
            dp[i][j] = a[i - 1] === b[j - 1] ? dp[i - 1][j - 1] + 1 : Math.max(dp[i - 1][j], dp[i][j - 1]);
        }
    }
    const out = [];
    let i = m, j = n;
    while (i > 0 && j > 0) {
        if (a[i - 1] === b[j - 1]) { out.unshift(`<div class="diff-line same"><span class="marker"> </span>${esc(a[i - 1])}</div>`); i--; j--; }
        else if (dp[i - 1][j] >= dp[i][j - 1]) { out.unshift(`<div class="diff-line del"><span class="marker">-</span>${esc(a[i - 1])}</div>`); i--; }
        else { out.unshift(`<div class="diff-line add"><span class="marker">+</span>${esc(b[j - 1])}</div>`); j--; }
    }
    while (i > 0) { out.unshift(`<div class="diff-line del"><span class="marker">-</span>${esc(a[i - 1])}</div>`); i--; }
    while (j > 0) { out.unshift(`<div class="diff-line add"><span class="marker">+</span>${esc(b[j - 1])}</div>`); j--; }
    return out.join("");
}

// 双列 side-by-side 渲染：每行 = [leftCell, rightCell]
// 状态: 'same' | 'del' | 'add' | 'pad'(对齐占位)
function renderSideBySide(a, b) {
    const m = a.length, n = b.length;
    if (m + n > 4000) {
        return `<div class="diff-line same">（行数过多 ${m} → ${n}，跳过 side-by-side 视图）</div>`;
    }
    const dp = Array.from({ length: m + 1 }, () => new Uint32Array(n + 1));
    for (let i = 1; i <= m; i++) {
        for (let j = 1; j <= n; j++) {
            dp[i][j] = a[i - 1] === b[j - 1] ? dp[i - 1][j - 1] + 1 : Math.max(dp[i - 1][j], dp[i][j - 1]);
        }
    }
    const rows = [];
    let i = m, j = n;
    while (i > 0 && j > 0) {
        if (a[i - 1] === b[j - 1]) { rows.unshift({ l: { t: a[i - 1], k: "same" }, r: { t: b[j - 1], k: "same" } }); i--; j--; }
        else if (dp[i - 1][j] >= dp[i][j - 1]) { rows.unshift({ l: { t: a[i - 1], k: "del" }, r: { t: "", k: "pad" } }); i--; }
        else { rows.unshift({ l: { t: "", k: "pad" }, r: { t: b[j - 1], k: "add" } }); j--; }
    }
    while (i > 0) { rows.unshift({ l: { t: a[i - 1], k: "del" }, r: { t: "", k: "pad" } }); i--; }
    while (j > 0) { rows.unshift({ l: { t: "", k: "pad" }, r: { t: b[j - 1], k: "add" } }); j--; }
    const cell = (c) => `<div class="diff-cell ${c.k}"><span class="marker">${c.k === "del" ? "-" : c.k === "add" ? "+" : " "}</span><span class="content">${esc(c.t)}</span></div>`;
    return `<div class="diff-grid">${rows.map(r => `<div class="diff-row">${cell(r.l)}${cell(r.r)}</div>`).join("")}</div>`;
}

function esc(s) { return String(s).replace(/[&<>]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]); }

function pushHistory(entry) {
    State.history.unshift(entry);
    State.history = State.history.slice(0, 10);
    try {
        localStorage.setItem("ts.history", JSON.stringify(State.history));
    } catch (e) {
        // localStorage 满 / 禁用，静默失败（不影响主流程）
    }
    renderHistory();
}
function renderHistory() {
    if (State.history.length === 0) {
        $("history-list").innerHTML = `<li class="empty">${t("ui.empty_history")}</li>`;
        return;
    }
    $("history-list").innerHTML = State.history.map((h, i) => `
    <li data-i="${i}" title="点击恢复输入">
      <span>${fmtBytes(h.inSize)} → ${fmtBytes(h.outSize)} <em>${h.ratio}%</em></span>
      <span class="ts">${new Date(h.ts).toLocaleTimeString()}</span>
    </li>
  `).join("");
    $("history-list").querySelectorAll("li[data-i]").forEach((li) => {
        li.addEventListener("click", () => {
            const h = State.history[+li.dataset.i];
            if (!h) return;
            if (typeof h.input === "string") {
                $("input-text").value = h.input;
                State.lastInput = h.input;
                updateInputSize();
                toast(t("ui.history_restored"));
            }
        });
    });
}
function loadHistory() {
    try {
        const raw = localStorage.getItem("ts.history");
        if (!raw) return;
        const arr = JSON.parse(raw);
        if (Array.isArray(arr)) {
            // 兼容旧版仅含 {ts,inSize,outSize,ratio} 的条目：保留字段即可
            State.history = arr.slice(0, 10);
            renderHistory();
        }
    } catch (e) {
        // 损坏的 JSON 静默清空
        try { localStorage.removeItem("ts.history"); } catch (_) { }
    }
}

// 压缩 / 反向还原
async function callApi(path, body) {
    const res = await fetch(path, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Accept-Language": State.lang },
        body: JSON.stringify(body),
    });
    if (!res.ok) {
        const text = await res.text().catch(() => "");
        throw new Error(`HTTP ${res.status}: ${text}`);
    }
    return res.json();
}

$("btn-compress").addEventListener("click", async () => {
    const text = $("input-text").value;
    if (!text) return;
    State.lastInput = text;
    $("btn-compress").disabled = true;
    $("btn-compress").textContent = t("ui.compressing");

    const reorder = $("opt-reorder").checked;
    const aiExport = $("opt-ai-export").checked;
    const useStream = $("opt-stream").checked;

    try {
        if (useStream) {
            await compressStream(text, reorder, aiExport);
        } else {
            const data = await callApi("/compress", { text, reorder, ai_export: aiExport });
            renderCompressResponse(data);
        }
    } catch (e) {
        toast(t("ui.compress_fail") + ": " + e.message, true);
        console.error(e);
    } finally {
        $("btn-compress").disabled = false;
        $("btn-compress").textContent = t("ui.compress");
    }
});

function renderCompressResponse(data) {
    if (data.ai_text) {
        State.lastAiText = data.ai_text;
        $("output-ai").textContent = data.ai_text;
    }
    if (data.semantic_log !== undefined || data.dictionary !== undefined) {
        renderOutput(data, data.ai_text || null);
    } else if (data.ai_text) {
        $("output-json").textContent = JSON.stringify({ ai_text_length: data.ai_text.length }, null, 2);
    }
    toast(t("ui.compress_done"));
}

function compressStream(text, reorder, aiExport) {
    return new Promise((resolve, reject) => {
        let resolved = false;
        const body = JSON.stringify({ text, reorder, ai_export: aiExport });
        // EventSource 不支持 POST，因此用 fetch + ReadableStream 读取 SSE
        fetch("/compress/stream", {
            method: "POST",
            headers: { "Content-Type": "application/json", "Accept-Language": State.lang },
            body,
        }).then(async (res) => {
            if (!res.ok) {
                const txt = await res.text().catch(() => "");
                throw new Error(`HTTP ${res.status}: ${txt}`);
            }
            const reader = res.body.getReader();
            const decoder = new TextDecoder();
            let buffer = "";
            $("output-json").textContent = t("ui.stream_progress");
            while (true) {
                const { done, value } = await reader.read();
                if (done) break;
                buffer += decoder.decode(value, { stream: true });
                const lines = buffer.split("\n");
                buffer = lines.pop();
                for (const line of lines) {
                    if (line.startsWith("event: ")) {
                        // 事件名在下一行 data 中通过 stage 字段体现，这里仅消费 data 行
                    } else if (line.startsWith("data: ")) {
                        const data = line.slice(6);
                        try {
                            const payload = JSON.parse(data);
                            if (payload.stage === "start") {
                                $("output-json").textContent = t("ui.stream_progress");
                            } else if (payload.stage === "done") {
                                renderCompressResponse(payload.payload);
                                resolved = true;
                                resolve();
                                return;
                            } else if (payload.stage === "error") {
                                throw new Error(payload.message || "stream error");
                            }
                        } catch (e) {
                            if (!resolved) reject(e);
                            resolved = true;
                            return;
                        }
                    }
                }
            }
            if (!resolved) resolve();
        }).catch((e) => {
            if (!resolved) reject(e);
            resolved = true;
        });
    });
}

$("btn-decompress").addEventListener("click", async () => {
    const text = $("input-text").value;
    if (!text) return;
    State.lastInput = text;
    $("btn-decompress").disabled = true;
    $("btn-decompress").textContent = t("ui.decompressing");
    try {
        const data = await callApi("/decompress", { text });
        $("output-json").textContent = JSON.stringify(data, null, 2);
        toast(t("ui.compress_done"));
    } catch (e) {
        toast(t("ui.compress_fail") + ": " + e.message, true);
    } finally {
        $("btn-decompress").disabled = false;
        $("btn-decompress").textContent = t("ui.decompress");
    }
});

// 实时 tail（WebSocket）
let tailSocket = null;
$("btn-tail").addEventListener("click", () => {
    if (tailSocket) {
        tailSocket.close();
        tailSocket = null;
        $("btn-tail").textContent = t("ui.tail");
        toast(t("ui.tail_disconnected"));
        return;
    }
    const path = prompt(t("ui.tail_prompt") || "请输入要 tail 的相对路径：");
    if (!path) return;
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(`${proto}//${window.location.host}/ws/tail`);
    tailSocket = ws;
    $("btn-tail").textContent = t("ui.tail_stop") || "停止 tail";
    $("output-json").textContent = `// tail ${path}\n`;
    ws.onopen = () => {
        ws.send(JSON.stringify({ path, interval_ms: 1000, compress: true }));
        toast(t("ui.tail_connected"));
    };
    ws.onmessage = (ev) => {
        const data = JSON.parse(ev.data);
        if (data.error) {
            toast("tail error: " + data.error, true);
            ws.close();
            return;
        }
        const text = data.compressed ? (data.output?.semantic_log || JSON.stringify(data.output, null, 2)) : data.text;
        $("output-json").textContent += text + "\n";
    };
    ws.onerror = (e) => {
        toast("WebSocket error", true);
        console.error(e);
    };
    ws.onclose = () => {
        tailSocket = null;
        $("btn-tail").textContent = t("ui.tail");
        toast(t("ui.tail_disconnected"));
    };
});

// 复制 / 下载
$("btn-copy").addEventListener("click", async () => {
    let text = "";
    if (State.view === "ai" && State.lastAiText) text = State.lastAiText;
    else if (State.lastOutput) text = JSON.stringify(State.lastOutput, null, 2);
    if (!text) return;
    try {
        await navigator.clipboard.writeText(text);
        toast(t("ui.copy_ok"));
    } catch {
        toast(t("ui.copy_fail"), true);
    }
});
$("btn-download").addEventListener("click", () => {
    let text = "", name = "tokenslim-output.json";
    if (State.view === "ai" && State.lastAiText) { text = State.lastAiText; name = "tokenslim-ai.txt"; }
    else if (State.lastOutput) text = JSON.stringify(State.lastOutput, null, 2);
    if (!text) return;
    const blob = new Blob([text], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url; a.download = name; a.click();
    URL.revokeObjectURL(url);
});

// 切换语言
$("lang-select").addEventListener("change", (e) => {
    State.lang = e.target.value;
    localStorage.setItem("ts.lang", State.lang);
    applyI18n();
    renderHistory();
});

// 启动
applyI18n();
loadHistory();
renderHistory();
updateInputSize();

// 心跳：拉 server 统计
async function refreshStats() {
    try {
        const r = await fetch("/health");
        if (r.ok) {
            const d = await r.json();
            $("server-stats").textContent = `v${d.version} · ${d.uptime_seconds}s up`;
        }
    } catch { /* server 未运行时不报错 */ }
}
refreshStats();
setInterval(refreshStats, 5000);
