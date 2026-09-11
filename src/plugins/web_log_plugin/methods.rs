use super::types::WebLogPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
struct WebAccessRecord {
    source: String,
    time: String,
    ip: String,
    method: String,
    path: String,
    status: String,
    bytes: String,
    referer: String,
    ua: String,
    reason: String,
    duration_ms: Option<u64>,
    raw: String,
}

#[derive(Debug, Clone)]
struct CsvState {
    headers: Vec<String>,
}

#[derive(Debug, Clone)]
struct W3cState {
    fields: Vec<String>,
}

#[derive(Debug, Default)]
struct RoutineBucket {
    kind: &'static str,
    status: String,
    method: String,
    route: String,
    count: usize,
    ips: BTreeSet<String>,
    uas: BTreeSet<String>,
    total_ms: u64,
    timed_count: usize,
}

#[derive(Debug, Default)]
struct AccessSummaryStats<'a> {
    status: BTreeMap<String, usize>,
    methods: BTreeMap<String, usize>,
    urls: BTreeMap<String, usize>,
    ips: BTreeMap<String, usize>,
    uas: BTreeMap<String, usize>,
    referers: BTreeMap<String, usize>,
    sources: BTreeMap<String, usize>,
    status_codes: BTreeMap<String, usize>,
    unique_urls: BTreeSet<String>,
    unique_ips: BTreeSet<String>,
    unique_uas: BTreeSet<String>,
    anomalies: BTreeMap<String, Vec<&'a WebAccessRecord>>,
    slow: Vec<&'a WebAccessRecord>,
    bytes_total: u64,
}

type AccessScanGroup<'a> = (Vec<&'a WebAccessRecord>, BTreeSet<String>);
type AccessBurstGroup<'a> = Vec<&'a WebAccessRecord>;

#[derive(Debug, Default)]
struct AccessV3Collection<'a> {
    ip_counts: BTreeMap<String, usize>,
    ua_counts: BTreeMap<String, usize>,
    status_counts: BTreeMap<String, usize>,
    routine_entries: Vec<RoutineBucket>,
    scans: Vec<AccessScanGroup<'a>>,
    bursts: Vec<AccessBurstGroup<'a>>,
    health_count: usize,
    static_count: usize,
    bot_count: usize,
    scan_ips: BTreeSet<String>,
}

/// 折叠连续空白为单空格。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 紧凑化时间：ISO 8601 T 分隔转空格，剥离小数秒/时区与 UTC 后缀。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_time(value: &str) -> String {
    let trimmed = value.trim().trim_matches('"');
    if let Some((date, rest)) = trimmed.split_once('T') {
        let time = rest.split(['.', '+', 'Z']).next().unwrap_or(rest);
        if !time.is_empty() {
            return format!("{date} {time}");
        }
    }
    trimmed
        .replace(" +0000", "")
        .replace(" +0800", "")
        .replace(" UTC", "")
}

/// 紧凑化 stream 名：路径截断保留前两段与尾段前 8 字符。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_stream(value: &str) -> String {
    let parts = value.split('/').collect::<Vec<_>>();
    if parts.len() >= 3 {
        let tail = parts[parts.len() - 1];
        let short_tail = tail.get(..8).unwrap_or(tail);
        return format!("{}/{}/{}", parts[0], parts[1], short_tail);
    }
    value.to_string()
}

/// 将文本截断至指定字节数并追加省略号。
#[tracing::instrument(level = "debug", skip_all)]
fn trim_to(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    format!("{}...", &value[..max_len])
}

/// 归一化路由：URL 去 scheme/query，UUID/长数字段替换为 :id。
#[tracing::instrument(level = "debug", skip_all)]
fn normalize_route(path: &str) -> String {
    let path_only = path_from_url(path).split('?').next().unwrap_or(path);
    let mut parts = Vec::new();
    for part in path_only.split('/') {
        if part.is_empty() {
            continue;
        }
        let is_uuid_like =
            part.len() >= 16 && part.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        let is_numeric = part.len() >= 4 && part.chars().all(|c| c.is_ascii_digit());
        if is_uuid_like || is_numeric {
            parts.push(":id".to_string());
        } else {
            parts.push(part.to_string());
        }
    }
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

/// 从 URL 提取路径部分（http(s) 去主机）。
#[tracing::instrument(level = "debug", skip_all)]
fn path_from_url(value: &str) -> &str {
    let trimmed = value.trim();
    if let Some(after_scheme) = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
    {
        if let Some(pos) = after_scheme.find('/') {
            return &after_scheme[pos..];
        }
        return "/";
    }
    trimmed
}

/// 将状态码映射为桶（2xx/3xx/4xx/5xx/other）。
#[tracing::instrument(level = "debug", skip_all)]
fn status_bucket(status: &str) -> &'static str {
    match status.chars().next().unwrap_or('0') {
        '2' => "2xx",
        '3' => "3xx",
        '4' => "4xx",
        '5' => "5xx",
        _ => "other",
    }
}

/// BTreeMap 计数自增。
#[tracing::instrument(level = "debug", skip_all)]
fn inc(map: &mut BTreeMap<String, usize>, key: impl Into<String>) {
    *map.entry(key.into()).or_insert(0) += 1;
}

/// 取 map 中计数最高的 N 项并格式化为 key:count 逗号串。
#[tracing::instrument(level = "debug", skip_all)]
fn top_entries(map: &BTreeMap<String, usize>, limit: usize) -> String {
    let mut entries = map.iter().collect::<Vec<_>>();
    entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    entries
        .into_iter()
        .take(limit)
        .map(|(key, count)| format!("{}:{}", trim_to(key, 80), count))
        .collect::<Vec<_>>()
        .join(",")
}

/// 取 map 中计数最高的 N 项，并以 token 引用其值（复用字典 token，避免重复嵌入完整 UA/IP 串）。
/// 无对应 token 时回退为截断原值。
#[tracing::instrument(level = "debug", skip_all)]
fn top_entries_tok(
    map: &BTreeMap<String, usize>,
    limit: usize,
    tokens: &BTreeMap<String, String>,
) -> String {
    let mut entries = map.iter().collect::<Vec<_>>();
    entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    entries
        .into_iter()
        .take(limit)
        .map(|(key, count)| format!("{}:{}", token_ref(key, tokens), count))
        .collect::<Vec<_>>()
        .join(",")
}

/// 取 map 中计数最高的 N 项返回排序列表。
#[tracing::instrument(level = "debug", skip_all)]
fn sorted_counts(map: &BTreeMap<String, usize>, limit: usize) -> Vec<(String, usize)> {
    let mut entries = map
        .iter()
        .map(|(key, count)| (key.clone(), *count))
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    entries.truncate(limit);
    entries
}

/// 从压缩文本解析 records= 计数。
#[tracing::instrument(level = "debug", skip_all)]
fn aggregate_record_count(compacted: &str) -> usize {
    compacted
        .split("records=")
        .nth(1)
        .and_then(|tail| {
            tail.chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
                .parse::<usize>()
                .ok()
        })
        .unwrap_or(0)
}

/// 将状态码映射为人类可读原因短语。
#[tracing::instrument(level = "debug", skip_all)]
fn status_reason(status: &str) -> &'static str {
    match status {
        "200" => "OK",
        "201" => "Created",
        "204" => "No Content",
        "301" => "Moved Permanently",
        "302" => "Found",
        "400" => "Bad Request",
        "401" => "Unauthorized",
        "403" => "Forbidden",
        "404" => "Not Found",
        "408" => "Request Timeout",
        "429" => "Too Many Requests",
        "500" => "Internal Server Error",
        "502" => "Bad Gateway",
        "503" => "Service Unavailable",
        "504" => "Gateway Timeout",
        _ => "",
    }
}

/// 判断 IP 是否为内网地址（127/10/192.168/fc/fd/172.16-31）。
#[tracing::instrument(level = "debug", skip_all)]
fn is_internal_ip(ip: &str) -> bool {
    ip == "127.0.0.1"
        || ip == "::1"
        || ip.starts_with("10.")
        || ip.starts_with("192.168.")
        || ip.starts_with("fc")
        || ip.starts_with("fd")
        || ip
            .strip_prefix("172.")
            .and_then(|rest| rest.split('.').next())
            .and_then(|octet| octet.parse::<u8>().ok())
            .is_some_and(|octet| (16..=31).contains(&octet))
}

/// 将 IP 分类为 Unknown/Internal/Documentation/External。
#[tracing::instrument(level = "debug", skip_all)]
fn ip_class(ip: &str) -> &'static str {
    if ip == "-" || ip.is_empty() {
        "Unknown"
    } else if is_internal_ip(ip) {
        "Internal"
    } else if ip.starts_with("203.0.113.") {
        "Documentation/Scanner"
    } else if ip.starts_with("198.51.100.") {
        "Documentation/Edge"
    } else {
        "External"
    }
}

/// 将 UA 分类为 Health/Bot/Browser/Other。
#[tracing::instrument(level = "debug", skip_all)]
fn ua_class(ua: &str) -> &'static str {
    let lower = ua.to_ascii_lowercase();
    if lower == "-" || lower.is_empty() {
        "Unknown"
    } else if lower.contains("kube-probe") {
        "Health/Kubernetes"
    } else if lower.contains("elb-healthchecker") {
        "Health/ALB"
    } else if lower.contains("googlebot") {
        "Bot/Google"
    } else if lower.contains("bingbot") {
        "Bot/Bing"
    } else if lower.contains("python-requests")
        || lower.contains("curl/")
        || lower.contains("wget/")
        || lower.contains("go-http-client")
    {
        "Bot/Script"
    } else if lower.contains("mozilla/") {
        "Browser"
    } else {
        "Other"
    }
}

/// 返回 UA 的基础 token 名（$UA_KUBE/$UA_BROWSER 等）。
#[tracing::instrument(level = "debug", skip_all)]
fn ua_token_base(ua: &str) -> &'static str {
    let lower = ua.to_ascii_lowercase();
    if lower.contains("kube-probe") {
        "$UA_KUBE"
    } else if lower.contains("elb-healthchecker") {
        "$UA_ELB"
    } else if lower.contains("googlebot") {
        "$UA_BOT_GOOGLE"
    } else if lower.contains("bingbot") {
        "$UA_BOT_BING"
    } else if lower.contains("python-requests") {
        "$UA_REQ"
    } else if lower.contains("curl/") || lower.contains("wget/") {
        "$UA_CLI"
    } else if lower.contains("mozilla/") {
        "$UA_BROWSER"
    } else {
        "$UA"
    }
}

/// 生成唯一 token：base 已用则追加序号。
#[tracing::instrument(level = "debug", skip_all)]
fn unique_token(base: &str, used: &mut BTreeSet<String>, counter: usize) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    let token = format!("{base}{counter}");
    used.insert(token.clone());
    token
}

/// 判断路由或 UA 是否为健康检查（/health//ready/kube-probe 等）。
#[tracing::instrument(level = "debug", skip_all)]
fn is_health_route(route: &str, ua: &str) -> bool {
    let route_lower = route.to_ascii_lowercase();
    let ua_lower = ua.to_ascii_lowercase();
    route_lower.contains("/health")
        || route_lower.contains("/ready")
        || route_lower.contains("/live")
        || route_lower == "/ping"
        || ua_lower.contains("kube-probe")
        || ua_lower.contains("elb-healthchecker")
}

/// 判断路由是否为静态资源（.js/.css/.png 等后缀）。
#[tracing::instrument(level = "debug", skip_all)]
fn is_static_route(route: &str) -> bool {
    let lower = route.to_ascii_lowercase();
    [
        ".js", ".css", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".map", ".woff", ".woff2",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
}

/// 判断路由是否为敏感探测目标（.env/wp-/admin/.git 等）。
#[tracing::instrument(level = "debug", skip_all)]
fn is_sensitive_probe(route: &str) -> bool {
    let lower = route.to_ascii_lowercase();
    [
        ".env",
        "wp-",
        "wp/",
        "admin",
        "config",
        "backup",
        "phpmyadmin",
        ".git",
        "passwd",
        "secret",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// 将记录分类为 health/static/bot/routine。
#[tracing::instrument(level = "debug", skip_all)]
fn routine_kind(record: &WebAccessRecord, route: &str) -> &'static str {
    if is_health_route(route, &record.ua) {
        "health"
    } else if is_static_route(route) {
        "static"
    } else if ua_class(&record.ua).starts_with("Bot/") {
        "bot"
    } else {
        "routine"
    }
}

/// 为 IP 计数构建 token 映射（扫描 IP 用 $IP_ATK，其余 $IP1..）。
#[tracing::instrument(level = "debug", skip_all)]
fn build_ip_tokens(
    counts: &BTreeMap<String, usize>,
    scanner_ips: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    let mut tokens = BTreeMap::new();
    let mut used = BTreeSet::new();
    if let Some(ip) = scanner_ips.iter().next() {
        tokens.insert(ip.clone(), "$IP_ATK".to_string());
        used.insert("$IP_ATK".to_string());
    }
    let mut ordinal = 1usize;
    for (ip, _) in sorted_counts(counts, 10) {
        if tokens.contains_key(&ip) {
            continue;
        }
        let token = format!("$IP{ordinal}");
        ordinal += 1;
        used.insert(token.clone());
        tokens.insert(ip, token);
    }
    tokens
}

/// 为 UA 计数构建 token 映射（按 UA 类生成基础 token）。
#[tracing::instrument(level = "debug", skip_all)]
fn build_ua_tokens(counts: &BTreeMap<String, usize>) -> BTreeMap<String, String> {
    let mut tokens = BTreeMap::new();
    let mut used = BTreeSet::new();
    for (idx, (ua, _)) in sorted_counts(counts, 10).into_iter().enumerate() {
        let base = ua_token_base(&ua);
        let token = unique_token(base, &mut used, idx + 1);
        tokens.insert(ua, token);
    }
    tokens
}

/// 引用 token 或截断原值。
#[tracing::instrument(level = "debug", skip_all)]
fn token_ref(value: &str, tokens: &BTreeMap<String, String>) -> String {
    tokens
        .get(value)
        .cloned()
        .unwrap_or_else(|| trim_to(value, 28))
}

/// 引用 token 集合（单值转 token，多值标 Mixed）。
#[tracing::instrument(level = "debug", skip_all)]
fn token_set_ref(values: &BTreeSet<String>, tokens: &BTreeMap<String, String>) -> String {
    if values.is_empty() {
        "-".to_string()
    } else if values.len() == 1 {
        token_ref(values.iter().next().unwrap(), tokens)
    } else {
        format!("Mixed({})", values.len())
    }
}

/// 收集访问记录的 v3 统计集合（IP/UA/状态计数、routine 桶、扫描组、突发组）。
#[tracing::instrument(level = "debug", skip_all)]
fn collect_access_v3_collection<'a>(records: &'a [WebAccessRecord]) -> AccessV3Collection<'a> {
    let mut ip_counts = BTreeMap::new();
    let mut ua_counts = BTreeMap::new();
    let mut status_counts = BTreeMap::new();
    let mut scan_groups: BTreeMap<String, Vec<&WebAccessRecord>> = BTreeMap::new();
    let mut burst_groups: BTreeMap<String, Vec<&WebAccessRecord>> = BTreeMap::new();
    let mut routines: BTreeMap<String, RoutineBucket> = BTreeMap::new();
    let mut health_count = 0usize;
    let mut static_count = 0usize;
    let mut bot_count = 0usize;

    for record in records {
        let route = normalize_route(&record.path);
        let kind = routine_kind(record, &route);
        inc(&mut ip_counts, record.ip.clone());
        inc(&mut ua_counts, record.ua.clone());
        inc(&mut status_counts, status_bucket(&record.status));
        match kind {
            "health" => health_count += 1,
            "static" => static_count += 1,
            "bot" => bot_count += 1,
            _ => {}
        }

        if record.status == "404" || record.status == "403" {
            let key = format!("{}|{}", record.ip, record.ua);
            scan_groups.entry(key).or_default().push(record);
        }

        if record.status.starts_with('5') {
            let key = format!("{}|{}|{}", record.status, record.method, route);
            burst_groups.entry(key).or_default().push(record);
        }

        if !record.status.starts_with('4') && !record.status.starts_with('5') {
            let key = format!("{kind}|{}|{}|{}", record.status, record.method, route);
            let entry = routines.entry(key).or_insert_with(|| RoutineBucket {
                kind,
                status: record.status.clone(),
                method: record.method.clone(),
                route: route.clone(),
                ..Default::default()
            });
            entry.count += 1;
            entry.ips.insert(record.ip.clone());
            entry.uas.insert(record.ua.clone());
            if let Some(ms) = record.duration_ms {
                entry.total_ms += ms;
                entry.timed_count += 1;
            }
        }
    }

    let scans = scan_groups
        .into_iter()
        .filter_map(|(_, items)| {
            let mut targets = BTreeSet::new();
            let mut sensitive = 0usize;
            for record in &items {
                let route = normalize_route(&record.path);
                if is_sensitive_probe(&route) {
                    sensitive += 1;
                }
                targets.insert(route);
            }
            if items.len() >= 5 && (targets.len() >= 4 || sensitive >= 2) {
                Some((items, targets))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let bursts = burst_groups
        .into_iter()
        .filter_map(|(_, items)| (items.len() >= 3).then_some(items))
        .collect::<Vec<_>>();

    // 全局扫描源统计：窗口内每个源 IP 对敏感探针路径（403/404）发起探测的次数。
    // 用于识别「分布式扫描」——攻击源分散在多个 IP，单个 IP|UA 组不足以触发单组扫描阈值，
    // 但整片窗口敏感探针总量显著且跨 ≥2 源，应归并为单一 SCAN 行而非保留逐路由 ANOMALY。
    let mut probe_ip_counts = BTreeMap::<String, usize>::new();
    let mut sensitive_targets = BTreeSet::new();
    let mut sensitive_probe_total = 0usize;
    for record in records {
        if record.status == "404" || record.status == "403" {
            let route = normalize_route(&record.path);
            if is_sensitive_probe(&route) {
                *probe_ip_counts.entry(record.ip.clone()).or_default() += 1;
                sensitive_targets.insert(route);
                sensitive_probe_total += 1;
            }
        }
    }
    let distributed_scan = probe_ip_counts.len() >= 2 && sensitive_probe_total >= 6;

    // 分布式扫描归并：将全部扫描源 IP 纳入 scan_ips（供 ANOMALY 抑制与 $IP_ATK 生成），
    // 并以单条聚合 SCAN 行覆盖整片探测，替代多组扫描与未抑制的逐路径 ANOMALY。
    let (scans, scan_ips) = if distributed_scan {
        let mut items = Vec::new();
        for record in records {
            if record.status == "404" || record.status == "403" {
                let route = normalize_route(&record.path);
                if is_sensitive_probe(&route) {
                    items.push(record);
                }
            }
        }
        let scan_ips = probe_ip_counts.into_keys().collect::<BTreeSet<_>>();
        (vec![(items, sensitive_targets)], scan_ips)
    } else {
        // 非分布式：scan_ips 仅收录命中单组扫描判定的源 IP（复用各组首条记录）。
        let scan_ips = scans
            .iter()
            .filter_map(|(items, _)| items.first().map(|record| record.ip.clone()))
            .collect::<BTreeSet<_>>();
        (scans, scan_ips)
    };

    let mut routine_entries = routines.into_values().collect::<Vec<_>>();
    routine_entries.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.kind.cmp(b.kind))
            .then_with(|| a.route.cmp(&b.route))
    });

    AccessV3Collection {
        ip_counts,
        ua_counts,
        status_counts,
        routine_entries,
        scans,
        bursts,
        health_count,
        static_count,
        bot_count,
        scan_ips,
    }
}

/// 追加 v3 IR 输出（字典行/诊断/例行/扫描/突发）。
#[tracing::instrument(level = "debug", skip_all)]
fn append_access_v3_ir(
    out: &mut String,
    records: &[WebAccessRecord],
    collection: &AccessV3Collection<'_>,
    ip_tokens: &BTreeMap<String, String>,
    ua_tokens: &BTreeMap<String, String>,
) {
    if !should_emit_access_v3(records, !collection.scans.is_empty()) {
        return;
    }

    if let Some(ip_dict_line) = render_access_ip_dict_line(&collection.ip_counts, ip_tokens) {
        out.push_str(&ip_dict_line);
    }
    if let Some(ua_dict_line) = render_access_ua_dict_line(&collection.ua_counts, ua_tokens) {
        out.push_str(&ua_dict_line);
    }
    // 小样本（records<24）走 SCAN 触发 v3 时，省略与 SUMMARY 冗余的 DIAG 诊断行。
    // 该场景的聚合 IR 若承载完整 DIAG/字典会逼近甚至超过原始输入，触发 ROI 门槛
    // 回退到逐行透传，丢失 SCAN/ROUTINE 等优质聚合信号；SUMMARY 已携带 4xx/5xx
    // 计数，err_rate 可推导，小样本省略 DIAG 收益大于损失。
    if records.len() >= 24 {
        emit_access_diag(
            out,
            records,
            &collection.status_counts,
            collection.health_count,
            collection.static_count,
            collection.bot_count,
        );
    }
    emit_access_routines(out, &collection.routine_entries, ip_tokens, ua_tokens);
    emit_access_scans(out, &collection.scans, ip_tokens, ua_tokens);
    emit_access_bursts(out, &collection.bursts, ip_tokens);
}

/// 渲染通用字典行（$W|DICT_xxx|entries）。
#[tracing::instrument(level = "debug", skip_all)]
fn render_access_dict_line(kind: &str, entries: Vec<String>) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    Some(format!("$W|{kind}|{}\n", entries.join(",")))
}

/// 渲染 IP 字典行（$W|DICT_IP|token=ip(类别)）。
#[tracing::instrument(level = "debug", skip_all)]
fn render_access_ip_dict_line(
    ip_counts: &BTreeMap<String, usize>,
    ip_tokens: &BTreeMap<String, String>,
) -> Option<String> {
    if ip_tokens.is_empty() {
        return None;
    }
    let entries = sorted_counts(ip_counts, 10)
        .into_iter()
        .filter_map(|(ip, _)| {
            ip_tokens
                .get(&ip)
                .map(|token| format!("{token}={ip}({})", ip_class(&ip)))
        })
        .collect::<Vec<_>>();
    render_access_dict_line("DICT_IP", entries)
}

/// 渲染 UA 字典行（$W|DICT_UA|token=ua(类别)）。
#[tracing::instrument(level = "debug", skip_all)]
fn render_access_ua_dict_line(
    ua_counts: &BTreeMap<String, usize>,
    ua_tokens: &BTreeMap<String, String>,
) -> Option<String> {
    if ua_tokens.is_empty() {
        return None;
    }
    let entries = sorted_counts(ua_counts, 10)
        .into_iter()
        .filter_map(|(ua, _)| {
            ua_tokens
                .get(&ua)
                .map(|token| format!("{token}={}({})", trim_to(&ua, 42), ua_class(&ua)))
        })
        .collect::<Vec<_>>();
    render_access_dict_line("DICT_UA", entries)
}

/// 判断是否应输出 v3 IR。
///
/// 触发条件为「记录数 ≥24」或「存在真实扫描组」。允许小样本但语义上确
/// 定为攻魔扫描的日志（如 BadBot 连续探测敏感路径）走 SCAN 聚合，将逐路
/// 径 ANOMALY 折叠为单条 SCAN 行，同时由 emit_access_anomaly_lines 的
/// scan_represented 门控在该场景下正确抑制冗余 ANOMALY。
#[tracing::instrument(level = "debug", skip_all)]
fn should_emit_access_v3(records: &[WebAccessRecord], has_scan: bool) -> bool {
    records.len() >= 24 || has_scan
}

/// 输出诊断行（错误率/4xx/5xx/噪音计数）。
#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_diag(
    out: &mut String,
    records: &[WebAccessRecord],
    status_counts: &BTreeMap<String, usize>,
    health_count: usize,
    static_count: usize,
    bot_count: usize,
) {
    let error_count = status_counts.get("4xx").copied().unwrap_or(0)
        + status_counts.get("5xx").copied().unwrap_or(0);
    let error_rate = error_count as f64 * 100.0 / records.len() as f64;
    let has_edge_mesh_format = records
        .iter()
        .any(|record| matches!(record.source.as_str(), "CloudFront" | "Envoy" | "IIS_W3C"));
    if has_edge_mesh_format {
        out.push_str(&format!(
            "$W|DIAG|err_rate={:.1}%|4xx={}|5xx={}|noise=health:{},static:{},bot:{}\n",
            error_rate,
            status_counts.get("4xx").copied().unwrap_or(0),
            status_counts.get("5xx").copied().unwrap_or(0),
            health_count,
            static_count,
            bot_count
        ));
    } else {
        out.push_str(&format!(
            "$W|DIAG|err_rate={:.1}%|4xx={}|5xx={}\n",
            error_rate,
            status_counts.get("4xx").copied().unwrap_or(0),
            status_counts.get("5xx").copied().unwrap_or(0)
        ));
    }
}

/// 输出例行桶行（count≥2 或非 routine，最多 8 条）。
#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_routines(
    out: &mut String,
    routine_entries: &[RoutineBucket],
    ip_tokens: &BTreeMap<String, String>,
    ua_tokens: &BTreeMap<String, String>,
) {
    for bucket in routine_entries
        .iter()
        .filter(|bucket| bucket.count >= 2 || bucket.kind != "routine")
        .take(8)
    {
        out.push_str(&render_access_routine_line(bucket, ip_tokens, ua_tokens));
    }
}

/// 渲染例行桶行（kind/状态/方法/路由/计数/ip/ua/avg_ms）。
#[tracing::instrument(level = "debug", skip_all)]
fn render_access_routine_line(
    bucket: &RoutineBucket,
    ip_tokens: &BTreeMap<String, String>,
    ua_tokens: &BTreeMap<String, String>,
) -> String {
    let avg_ms = render_routine_avg_ms(bucket);
    format!(
        "$W|ROUTINE|kind={}|{} {}|{} {}|count={}|ips={}|ua={}|avg_ms={}\n",
        bucket.kind,
        bucket.status,
        status_reason(&bucket.status),
        bucket.method,
        bucket.route,
        bucket.count,
        token_set_ref(&bucket.ips, ip_tokens),
        token_set_ref(&bucket.uas, ua_tokens),
        avg_ms
    )
}

/// 渲染例行桶平均耗时（无计时数据时输出 -）。
#[tracing::instrument(level = "debug", skip_all)]
fn render_routine_avg_ms(bucket: &RoutineBucket) -> String {
    if bucket.timed_count > 0 {
        (bucket.total_ms / bucket.timed_count as u64).to_string()
    } else {
        "-".to_string()
    }
}

/// 输出扫描组行（最多 5 条）。
#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_scans(
    out: &mut String,
    scans: &[(Vec<&WebAccessRecord>, BTreeSet<String>)],
    ip_tokens: &BTreeMap<String, String>,
    ua_tokens: &BTreeMap<String, String>,
) {
    for (items, targets) in scans.iter().take(5) {
        out.push_str(&render_access_scan_line(
            items, targets, ip_tokens, ua_tokens,
        ));
    }
}

/// 渲染扫描行（源/UA/窗口/目标数/样本）。
#[tracing::instrument(level = "debug", skip_all)]
fn render_access_scan_line(
    items: &[&WebAccessRecord],
    targets: &BTreeSet<String>,
    ip_tokens: &BTreeMap<String, String>,
    ua_tokens: &BTreeMap<String, String>,
) -> String {
    let (window_start, window_end) = render_scan_window(items);
    let sample = render_scan_target_sample(targets, 8);
    // 源/IP 可能不止一个（分布式扫描归并）：用 token_set_ref 表达（单源退化为单个 token）。
    let source_ips = items.iter().map(|r| r.ip.clone()).collect::<BTreeSet<_>>();
    let source_uas = items.iter().map(|r| r.ua.clone()).collect::<BTreeSet<_>>();
    format!(
        "!$W|SCAN|source={}|ua={}|window={}..{}|hits={}|targets={}|sample={}\n",
        token_set_ref(&source_ips, ip_tokens),
        token_set_ref(&source_uas, ua_tokens),
        window_start,
        window_end,
        items.len(),
        targets.len(),
        sample
    )
}

/// 渲染扫描目标样本（前 N 个目标）。
#[tracing::instrument(level = "debug", skip_all)]
fn render_scan_target_sample(targets: &BTreeSet<String>, limit: usize) -> String {
    targets
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .join(",")
}

/// 渲染扫描窗口（首尾时间）。
#[tracing::instrument(level = "debug", skip_all)]
fn render_scan_window(items: &[&WebAccessRecord]) -> (String, String) {
    let first = items.first().unwrap();
    let last = items.last().unwrap();
    (first.time.clone(), last.time.clone())
}

/// 输出突发组行（最多 5 条）。
#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_bursts(
    out: &mut String,
    bursts: &[Vec<&WebAccessRecord>],
    ip_tokens: &BTreeMap<String, String>,
) {
    for items in bursts.iter().take(5) {
        out.push_str(&render_access_burst_line(items, ip_tokens));
    }
}

/// 渲染突发行（状态/方法/路由/窗口/计数/IP 数）。
#[tracing::instrument(level = "debug", skip_all)]
fn render_access_burst_line(
    items: &[&WebAccessRecord],
    ip_tokens: &BTreeMap<String, String>,
) -> String {
    let first = items.first().unwrap();
    let (window_start, window_end) = render_burst_window(items);
    let burst_ips = collect_burst_ips(items);
    format!(
        "!$W|BURST|{} {}|{} {}|window={}..{}|count={}|ips={}|sample_ip={}\n",
        first.status,
        status_reason(&first.status),
        first.method,
        normalize_route(&first.path),
        window_start,
        window_end,
        items.len(),
        burst_ips.len(),
        token_set_ref(&burst_ips, ip_tokens)
    )
}

/// 收集突发组的去重 IP 集合。
#[tracing::instrument(level = "debug", skip_all)]
fn collect_burst_ips(items: &[&WebAccessRecord]) -> BTreeSet<String> {
    let mut burst_ips = BTreeSet::new();
    for record in items {
        burst_ips.insert(record.ip.clone());
    }
    burst_ips
}

/// 渲染突发窗口（首尾时间）。
#[tracing::instrument(level = "debug", skip_all)]
fn render_burst_window(items: &[&WebAccessRecord]) -> (String, String) {
    let first = items.first().unwrap();
    let last = items.last().unwrap();
    (first.time.clone(), last.time.clone())
}

/// 收集访问汇总统计（状态/方法/URL/IP/UA/引用/异常/慢请求）。
#[tracing::instrument(level = "debug", skip_all)]
fn collect_access_summary_stats<'a>(records: &'a [WebAccessRecord]) -> AccessSummaryStats<'a> {
    let mut stats = AccessSummaryStats::default();
    for record in records {
        let route = normalize_route(&record.path);
        inc(&mut stats.status, status_bucket(&record.status));
        inc(&mut stats.status_codes, record.status.clone());
        inc(&mut stats.methods, record.method.clone());
        inc(&mut stats.urls, format!("{} {}", record.method, route));
        inc(&mut stats.ips, record.ip.clone());
        inc(&mut stats.uas, record.ua.clone());
        if record.referer != "-" && !record.referer.is_empty() {
            inc(&mut stats.referers, record.referer.clone());
        }
        inc(&mut stats.sources, record.source.clone());
        stats.unique_urls.insert(route.clone());
        stats.unique_ips.insert(record.ip.clone());
        stats.unique_uas.insert(record.ua.clone());
        if let Ok(bytes) = record.bytes.parse::<u64>() {
            stats.bytes_total += bytes;
        }
        if record.status.starts_with('4') || record.status.starts_with('5') {
            let key = format!("{} {} {}", record.status, record.method, route);
            stats.anomalies.entry(key).or_default().push(record);
        }
        if record.duration_ms.unwrap_or(0) >= 1000 {
            stats.slow.push(record);
        }
    }
    stats
}

/// 输出透传行（无法解析的行原样保留）。
#[tracing::instrument(level = "debug", skip_all)]
fn emit_passthrough_lines(out: &mut String, passthrough: &[String]) {
    for line in passthrough {
        out.push_str(line);
        out.push('\n');
    }
}

/// 尝试输出紧凑健康汇总（仅健康路由且无异常时）。
#[tracing::instrument(level = "debug", skip_all)]
fn try_emit_compact_health_summary(
    out: &mut String,
    records: &[WebAccessRecord],
    stats: &AccessSummaryStats<'_>,
) -> bool {
    if !stats.anomalies.is_empty() || !stats.slow.is_empty() || stats.unique_urls.len() != 1 {
        return false;
    }
    out.push_str(&format!(
        "$W|SUMMARY|records={}|2xx={}|3xx={}|4xx=0|5xx=0|TOP_URL={}|TOP_IP={}|TOP_UA={}\n",
        records.len(),
        stats.status.get("2xx").copied().unwrap_or(0),
        stats.status.get("3xx").copied().unwrap_or(0),
        top_entries(&stats.urls, 3),
        top_entries(&stats.ips, 5),
        top_entries(&stats.uas, 3)
    ));
    true
}

/// 输出访问汇总块（记录数/窗口/状态分布/Top 列表）。
#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_summary_block(
    out: &mut String,
    records: &[WebAccessRecord],
    stats: &AccessSummaryStats<'_>,
    ip_tokens: &BTreeMap<String, String>,
    ua_tokens: &BTreeMap<String, String>,
    has_scan: bool,
) {
    let first_time = records
        .first()
        .map(|record| record.time.as_str())
        .unwrap_or("-");
    let last_time = records
        .last()
        .map(|record| record.time.as_str())
        .unwrap_or("-");
    out.push_str(&format!(
        "$W|SUMMARY|records={}|window={}..{}|2xx={}|3xx={}|4xx={}|5xx={}|other={}|ips={}|urls={}|ua={}|bytes={}|st={}|m={}|src={}\n",
        records.len(),
        first_time,
        last_time,
        stats.status.get("2xx").copied().unwrap_or(0),
        stats.status.get("3xx").copied().unwrap_or(0),
        stats.status.get("4xx").copied().unwrap_or(0),
        stats.status.get("5xx").copied().unwrap_or(0),
        stats.status.get("other").copied().unwrap_or(0),
        stats.unique_ips.len(),
        stats.unique_urls.len(),
        stats.unique_uas.len(),
        stats.bytes_total,
        top_entries(&stats.status_codes, 8),
        top_entries(&stats.methods, 6),
        top_entries(&stats.sources, 4)
    ));
    out.push_str(&format!("$W|TOP_URL|{}\n", top_entries(&stats.urls, 8)));
    // 仅当 v3 字典行实际输出（records≥24）时复用 token，否则回退完整原始串，避免未定义 token。
    let use_tokens = should_emit_access_v3(records, has_scan);
    out.push_str(&format!(
        "$W|TOP_IP|{}|$W|TOP_UA|{}",
        if use_tokens {
            top_entries_tok(&stats.ips, 8, ip_tokens)
        } else {
            top_entries(&stats.ips, 8)
        },
        if use_tokens {
            top_entries_tok(&stats.uas, 6, ua_tokens)
        } else {
            top_entries(&stats.uas, 6)
        }
    ));
    if !stats.referers.is_empty() {
        out.push_str(&format!("|$W|TOP_REF|{}", top_entries(&stats.referers, 6)));
    }
    out.push('\n');
}

/// 输出异常行（4xx/5xx 分组，最多 8 条）。
#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_anomaly_lines(
    out: &mut String,
    anomalies: &BTreeMap<String, Vec<&WebAccessRecord>>,
    scan_ips: &BTreeSet<String>,
    scan_represented: bool,
) {
    let mut anomaly_entries = anomalies.iter().collect::<Vec<_>>();
    anomaly_entries.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(b.0)));
    for (key, items) in anomaly_entries.into_iter().take(8) {
        // 跳过纯扫描源异常：仅当本轮确实输出了 SCAN 行（v3 分支激活、记录数 ≥24）且该探测组
        // 的所有命中 IP 均已在扫描判定中作为扫描源聚合时才抑制，避免既无 SCAN 替代表达又丢关键
        // ANOMALY 信息（例如记录数不足 24 的小样本扫描场景）。
        if scan_represented
            && !scan_ips.is_empty()
            && items.iter().all(|record| scan_ips.contains(&record.ip))
        {
            continue;
        }
        let mut anomaly_ips = BTreeMap::new();
        for record in items {
            inc(&mut anomaly_ips, record.ip.clone());
        }
        let sample = items
            .first()
            .map(|record| trim_to(&compact_spaces(&record.raw), 36))
            .unwrap_or_default();
        let reason = items
            .first()
            .map(|record| record.reason.as_str())
            .unwrap_or("");
        out.push_str(&format!(
            "!$W|ANOMALY|{}|hits={}|ips={}|r={}|sample=\"{}\"\n",
            key,
            items.len(),
            top_entries(&anomaly_ips, 5),
            reason,
            sample.replace('"', "'")
        ));
    }
}

/// 输出慢请求行（最多 5 条）。
#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_slow_lines(out: &mut String, slow: &[&WebAccessRecord]) {
    for record in sort_slow_access_records(slow).into_iter().take(5) {
        out.push_str(&render_access_slow_line(record));
    }
}

/// 按耗时降序排序慢请求。
#[tracing::instrument(level = "debug", skip_all)]
fn sort_slow_access_records<'a>(slow: &'a [&'a WebAccessRecord]) -> Vec<&'a WebAccessRecord> {
    let mut sorted = slow.to_vec();
    sorted.sort_by(|a, b| b.duration_ms.cmp(&a.duration_ms));
    sorted
}

/// 渲染慢请求行。
#[tracing::instrument(level = "debug", skip_all)]
fn render_access_slow_line(record: &WebAccessRecord) -> String {
    let (method, route) = render_slow_identity(record);
    format!(
        "!$W|SLOW|{} {}|status={}|ms={}|ip={}|ua={}\n",
        method,
        route,
        record.status,
        record.duration_ms.unwrap_or(0),
        record.ip,
        trim_to(&record.ua, 60)
    )
}

/// 渲染慢请求身份（方法 + 归一化路由）。
#[tracing::instrument(level = "debug", skip_all)]
fn render_slow_identity(record: &WebAccessRecord) -> (String, String) {
    (record.method.clone(), normalize_route(&record.path))
}

/// 解析耗时（支持秒后缀与秒/毫秒数值，转毫秒）。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_duration_ms(value: &str) -> Option<u64> {
    let trimmed = value.trim().trim_matches('"');
    if trimmed.is_empty() || trimmed == "-" {
        return None;
    }
    if let Some(seconds) = trimmed.strip_suffix('s') {
        return seconds
            .parse::<f64>()
            .ok()
            .map(|value| (value * 1000.0).round() as u64);
    }
    trimmed
        .parse::<f64>()
        .ok()
        .map(|value| (value * 1000.0).round() as u64)
}

/// 判断是否为 CloudWatch 表格噪音行（分隔线/表头）。
#[tracing::instrument(level = "debug", skip_all)]
fn is_cloudwatch_table_noise(line: &str) -> bool {
    if line.is_empty() {
        return true;
    }
    let compact = line.trim();
    if compact.chars().all(|c| c == '-' || c == '+') {
        return true;
    }
    if compact.starts_with("|---") {
        return true;
    }
    let lower = compact.to_ascii_lowercase();
    lower.contains("|") && lower.contains("timestamp") && lower.contains("message")
}

/// 拆分 CSV 行（支持引号与转义）。
#[tracing::instrument(level = "debug", skip_all)]
fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    fields.push(current.trim().to_string());
    fields
}

/// 按空白拆分 W3C 行。
#[tracing::instrument(level = "debug", skip_all)]
fn split_w3c_line(line: &str) -> Vec<String> {
    line.split_whitespace()
        .map(|value| value.trim().to_string())
        .collect()
}

/// 解析 W3C 表头（#Fields: 行）。
#[tracing::instrument(level = "debug", skip_all)]
fn w3c_header(line: &str) -> Option<W3cState> {
    let trimmed = line.trim_start();
    let fields = trimmed.strip_prefix("#Fields:")?;
    let parsed = fields
        .split_whitespace()
        .map(|field| field.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if parsed.iter().any(|field| field == "cs-method")
        && parsed.iter().any(|field| field == "sc-status")
    {
        Some(W3cState { fields: parsed })
    } else {
        None
    }
}

/// 按候选字段名取 W3C 值。
#[tracing::instrument(level = "debug", skip_all)]
fn w3c_value(fields: &[String], values: &[String], candidates: &[&str]) -> Option<String> {
    for candidate in candidates {
        if let Some(index) = fields.iter().position(|field| field == candidate) {
            return values.get(index).cloned();
        }
    }
    None
}

/// 解码 W3C URL 转义（%20/%2F 等）。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_w3c_text(value: &str) -> String {
    value
        .replace("%20", " ")
        .replace("%2F", "/")
        .replace("%3A", ":")
        .replace("%3F", "?")
        .replace("%3D", "=")
        .replace('+', " ")
}

/// 解析 CSV 表头（含 message/status/method 特征）。
#[tracing::instrument(level = "debug", skip_all)]
fn csv_header(line: &str) -> Option<CsvState> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return None;
    }
    let fields = split_csv_line(line);
    if fields.len() < 2 {
        return None;
    }
    let lower = fields
        .iter()
        .map(|field| field.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let has_message = lower.iter().any(|field| field.contains("message"));
    let has_status = lower.iter().any(|field| field.contains("status"));
    let has_method = lower
        .iter()
        .any(|field| field.contains("method") || field.contains("request"));
    if has_message || (has_status && has_method) {
        Some(CsvState { headers: lower })
    } else {
        None
    }
}

/// 按候选列名取 CSV 值（精确后模糊匹配）。
#[tracing::instrument(level = "debug", skip_all)]
fn csv_value(headers: &[String], fields: &[String], candidates: &[&str]) -> Option<String> {
    for candidate in candidates {
        if let Some(index) = headers.iter().position(|header| header == candidate) {
            return fields.get(index).cloned();
        }
    }
    for candidate in candidates {
        if let Some(index) = headers.iter().position(|header| header.contains(candidate)) {
            return fields.get(index).cloned();
        }
    }
    None
}

/// 沿路径从 JSON 取字符串。
#[tracing::instrument(level = "debug", skip_all)]
fn json_string<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_str()
}

/// 沿路径从 JSON 取状态（字符串或数字）。
#[tracing::instrument(level = "debug", skip_all)]
fn json_status(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    if let Some(text) = cursor.as_str() {
        return Some(text.to_string());
    }
    cursor.as_i64().map(|number| number.to_string())
}

impl WebLogPlugin {
    /// 创建 WebLogPlugin 实例（名称 web_log，优先级 170），预编译各日志格式正则。
    pub fn new() -> Self {
        Self {
            name: "web_log",
            priority: 170,
            combined_log_pattern: Arc::new(Regex::new(r#"^(?P<ip>[\da-fA-F:\.]+)\s+(?P<ident>\S+)\s+(?P<user>\S+)\s+\[(?P<time>[^\]]+)\]\s+"(?P<method>[A-Z]+)\s+(?P<path>[^\s]+)\s+(?P<proto>[^"]+)"\s+(?P<status>\d{3})\s+(?P<bytes>\d+|-)\s+"(?P<referer>[^"]*)"\s+"(?P<ua>[^"]*)"(?:\s+(?P<tail>.*))?$"#).unwrap()),
            common_log_pattern: Arc::new(Regex::new(r#"^(?P<ip>[\da-fA-F:\.]+)\s+(?P<ident>\S+)\s+(?P<user>\S+)\s+\[(?P<time>[^\]]+)\]\s+"(?P<method>[A-Z]+)\s+(?P<path>[^\s]+)\s+(?P<proto>[^"]+)"\s+(?P<status>\d{3})\s+(?P<bytes>\d+|-)(?:\s+(?P<tail>.*))?$"#).unwrap()),
            error_log_pattern: Arc::new(Regex::new(r#"^(?P<time>\d{4}/\d{2}/\d{2} \d{2}:\d{2}:\d{2})\s+\[(?P<level>[a-z]+)\]\s+(?P<pid>\d+)#\d+:\s+\*\d+\s+(?P<msg>.*)$"#).unwrap()),
            uvicorn_access_pattern: Arc::new(Regex::new(r#"^(?P<level>[A-Z]+):\s+(?P<ip>[\da-fA-F:\.]+):\d+\s+-\s+"(?P<method>[A-Z]+)\s+(?P<path>[^\s"]+)\s+HTTP/[0-9.]+"\s+(?P<status>\d{3})\s+(?P<reason>.*)$"#).unwrap()),
            envoy_access_pattern: Arc::new(Regex::new(r#"^\[(?P<time>[^\]]+)\]\s+"(?P<method>[A-Z]+)\s+(?P<path>[^\s"]+)\s+HTTP/[0-9.]+"\s+(?P<status>\d{3})\s+(?P<flags>\S+)\s+(?P<bytes_in>\d+|-)\s+(?P<bytes_out>\d+|-)\s+(?P<duration>\d+|-)\s+\S+\s+"(?P<xff>[^"]*)"\s+"(?P<ua>[^"]*)".*$"#).unwrap()),
            alb_access_pattern: Arc::new(Regex::new(r#"^(?P<kind>http|https|h2|ws|wss|grpcs)\s+(?P<time>\d{4}-\d{2}-\d{2}T\S+)\s+\S+\s+(?P<ip>[\da-fA-F:\.]+):\d+\s+\S+\s+(?P<request_time>-?\d+(?:\.\d+)?)\s+(?P<target_time>-?\d+(?:\.\d+)?)\s+(?P<response_time>-?\d+(?:\.\d+)?)\s+(?P<elb_status>\d{3}|-)\s+(?P<target_status>\d{3}|-)\s+(?P<received>\d+|-)\s+(?P<sent>\d+|-)\s+"(?P<method>[A-Z]+)\s+(?P<path>[^\s"]+)\s+HTTP/[0-9.]+"\s+"(?P<ua>[^"]*)".*$"#).unwrap()),
            aws_logs_tail_pattern: Arc::new(Regex::new(r#"^(?P<time>\d{4}-\d{2}-\d{2}T\S+)\s+(?P<stream>\S+)\s+(?P<message>.+)$"#).unwrap()),
            cloudwatch_table_row_pattern: Arc::new(Regex::new(r#"^\|\s*(?P<time>\d{10,}|\d{4}-\d{2}-\d{2}T[^|]+)\s*\|\s*(?P<message>.*?)\s*\|$"#).unwrap()),
        }
    }

    /// 解析访问消息（ALB/Envoy/uvicorn/combined/common 五种格式）。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_access_message(
        &self,
        message: &str,
        source: &str,
        wrapper_time: Option<&str>,
        raw: &str,
    ) -> Option<WebAccessRecord> {
        let trimmed = message.trim();
        if let Some(caps) = self.alb_access_pattern.captures(trimmed) {
            let status = caps
                .name("target_status")
                .filter(|m| m.as_str() != "-")
                .or_else(|| caps.name("elb_status"))?
                .as_str()
                .to_string();
            let duration_ms = caps
                .name("target_time")
                .and_then(|m| parse_duration_ms(m.as_str()))
                .or_else(|| {
                    caps.name("request_time")
                        .and_then(|m| parse_duration_ms(m.as_str()))
                });
            return Some(WebAccessRecord {
                source: format!("ALB:{}", caps.name("kind")?.as_str()),
                time: wrapper_time
                    .map(compact_time)
                    .unwrap_or_else(|| compact_time(caps.name("time").unwrap().as_str())),
                ip: caps.name("ip")?.as_str().to_string(),
                method: caps.name("method")?.as_str().to_string(),
                path: path_from_url(caps.name("path")?.as_str()).to_string(),
                status: status.clone(),
                bytes: caps.name("sent")?.as_str().to_string(),
                referer: "-".to_string(),
                ua: caps.name("ua")?.as_str().to_string(),
                reason: status_reason(&status).to_string(),
                duration_ms,
                raw: raw.to_string(),
            });
        }

        if let Some(caps) = self.envoy_access_pattern.captures(trimmed) {
            let status = caps.name("status")?.as_str().to_string();
            let duration_ms = caps
                .name("duration")
                .and_then(|m| m.as_str().parse::<u64>().ok());
            return Some(WebAccessRecord {
                source: "Envoy".to_string(),
                time: wrapper_time
                    .map(compact_time)
                    .unwrap_or_else(|| compact_time(caps.name("time").unwrap().as_str())),
                ip: caps.name("xff")?.as_str().to_string(),
                method: caps.name("method")?.as_str().to_string(),
                path: path_from_url(caps.name("path")?.as_str()).to_string(),
                status: status.clone(),
                bytes: caps.name("bytes_out")?.as_str().to_string(),
                referer: "-".to_string(),
                ua: caps.name("ua")?.as_str().to_string(),
                reason: caps.name("flags")?.as_str().to_string(),
                duration_ms,
                raw: raw.to_string(),
            });
        }

        if let Some(caps) = self.uvicorn_access_pattern.captures(trimmed) {
            let status = caps.name("status")?.as_str().to_string();
            return Some(WebAccessRecord {
                source: source.to_string(),
                time: wrapper_time
                    .map(compact_time)
                    .unwrap_or_else(|| "-".to_string()),
                ip: caps.name("ip")?.as_str().to_string(),
                method: caps.name("method")?.as_str().to_string(),
                path: caps.name("path")?.as_str().to_string(),
                status: status.clone(),
                bytes: "-".to_string(),
                referer: "-".to_string(),
                // uvicorn access 格式（`INFO: ip:port - "GET /x HTTP/1.1" 200 OK`）无 User-Agent 字段，
                // 日志级别（捕获组 level）只是日志前缀，不得当作 UA 计入统计。统一置为 "-"（无 UA），
                // 否则会把 "INFO" 这类级别串污染进 TOP_UA（见 case_027）。
                ua: "-".to_string(),
                reason: compact_spaces(caps.name("reason")?.as_str()),
                duration_ms: None,
                raw: raw.to_string(),
            });
        }

        if let Some(caps) = self.combined_log_pattern.captures(trimmed) {
            let tail = caps.name("tail").map(|m| m.as_str()).unwrap_or_default();
            return Some(WebAccessRecord {
                source: source.to_string(),
                time: wrapper_time
                    .map(compact_time)
                    .unwrap_or_else(|| compact_time(caps.name("time").unwrap().as_str())),
                ip: caps.name("ip")?.as_str().to_string(),
                method: caps.name("method")?.as_str().to_string(),
                path: caps.name("path")?.as_str().to_string(),
                status: caps.name("status")?.as_str().to_string(),
                bytes: caps.name("bytes")?.as_str().to_string(),
                referer: caps.name("referer")?.as_str().to_string(),
                ua: caps.name("ua")?.as_str().to_string(),
                reason: status_reason(caps.name("status")?.as_str()).to_string(),
                duration_ms: tail.split_whitespace().find_map(parse_duration_ms),
                raw: raw.to_string(),
            });
        }

        if let Some(caps) = self.common_log_pattern.captures(trimmed) {
            let tail = caps.name("tail").map(|m| m.as_str()).unwrap_or_default();
            return Some(WebAccessRecord {
                source: source.to_string(),
                time: wrapper_time
                    .map(compact_time)
                    .unwrap_or_else(|| compact_time(caps.name("time").unwrap().as_str())),
                ip: caps.name("ip")?.as_str().to_string(),
                method: caps.name("method")?.as_str().to_string(),
                path: caps.name("path")?.as_str().to_string(),
                status: caps.name("status")?.as_str().to_string(),
                bytes: caps.name("bytes")?.as_str().to_string(),
                referer: "-".to_string(),
                ua: "-".to_string(),
                reason: status_reason(caps.name("status")?.as_str()).to_string(),
                duration_ms: tail.split_whitespace().find_map(parse_duration_ms),
                raw: raw.to_string(),
            });
        }

        None
    }

    /// 解析 JSON 记录（message/log/textPayload/httpRequest 等形态）。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_json_record(&self, line: &str) -> Option<WebAccessRecord> {
        let value = serde_json::from_str::<Value>(line.trim()).ok()?;
        if let Some(message) = json_string(&value, &["message"])
            .or_else(|| json_string(&value, &["log"]))
            .or_else(|| json_string(&value, &["textPayload"]))
        {
            let source = json_string(&value, &["cloud"])
                .or_else(|| json_string(&value, &["provider"]))
                .or_else(|| json_string(&value, &["resource", "type"]))
                .unwrap_or("JSON");
            let time = json_string(&value, &["timestamp"])
                .or_else(|| json_string(&value, &["@timestamp"]))
                .or_else(|| json_string(&value, &["time"]));
            if let Some(record) = self.parse_access_message(message, source, time, line) {
                return Some(record);
            }
        }

        if value.get("httpRequest").is_some() {
            let http = value.get("httpRequest")?;
            let status = json_status(http, &["status"])?;
            let path = json_string(http, &["requestUrl"]).unwrap_or("-");
            return Some(WebAccessRecord {
                source: "GCP_HTTP".to_string(),
                time: json_string(&value, &["timestamp"])
                    .map(compact_time)
                    .unwrap_or_else(|| "-".to_string()),
                ip: json_string(http, &["remoteIp"]).unwrap_or("-").to_string(),
                method: json_string(http, &["requestMethod"])
                    .unwrap_or("GET")
                    .to_string(),
                path: path_from_url(path).to_string(),
                status: status.clone(),
                bytes: json_status(http, &["responseSize"]).unwrap_or_else(|| "-".to_string()),
                referer: json_string(http, &["referer"]).unwrap_or("-").to_string(),
                ua: json_string(http, &["userAgent"]).unwrap_or("-").to_string(),
                reason: status_reason(&status).to_string(),
                duration_ms: json_string(http, &["latency"]).and_then(parse_duration_ms),
                raw: line.to_string(),
            });
        }

        let method = json_string(&value, &["request_method"])
            .or_else(|| json_string(&value, &["method"]))
            .or_else(|| json_string(&value, &["ClientRequestMethod"]))?;
        let status = json_status(&value, &["status"])
            .or_else(|| json_status(&value, &["EdgeResponseStatus"]))
            .or_else(|| json_status(&value, &["response_status"]))?;
        let path = json_string(&value, &["request_uri"])
            .or_else(|| json_string(&value, &["path"]))
            .or_else(|| json_string(&value, &["uri"]))
            .or_else(|| json_string(&value, &["ClientRequestURI"]))
            .unwrap_or("-");
        let source = json_string(&value, &["source"])
            .or_else(|| json_string(&value, &["provider"]))
            .unwrap_or("JSON_ACCESS");
        Some(WebAccessRecord {
            source: source.to_string(),
            time: json_string(&value, &["time_local"])
                .or_else(|| json_string(&value, &["time"]))
                .or_else(|| json_string(&value, &["timestamp"]))
                .or_else(|| json_string(&value, &["EdgeStartTimestamp"]))
                .map(compact_time)
                .unwrap_or_else(|| "-".to_string()),
            ip: json_string(&value, &["remote_addr"])
                .or_else(|| json_string(&value, &["client_ip"]))
                .or_else(|| json_string(&value, &["ClientIP"]))
                .unwrap_or("-")
                .to_string(),
            method: method.to_string(),
            path: path_from_url(path).to_string(),
            status: status.clone(),
            bytes: json_status(&value, &["body_bytes_sent"])
                .or_else(|| json_status(&value, &["bytes"]))
                .or_else(|| json_status(&value, &["EdgeResponseBytes"]))
                .unwrap_or_else(|| "-".to_string()),
            referer: json_string(&value, &["http_referer"])
                .or_else(|| json_string(&value, &["referer"]))
                .unwrap_or("-")
                .to_string(),
            ua: json_string(&value, &["http_user_agent"])
                .or_else(|| json_string(&value, &["user_agent"]))
                .or_else(|| json_string(&value, &["ClientRequestUserAgent"]))
                .unwrap_or("-")
                .to_string(),
            reason: status_reason(&status).to_string(),
            duration_ms: json_string(&value, &["request_time"])
                .or_else(|| json_string(&value, &["upstream_response_time"]))
                .and_then(parse_duration_ms),
            raw: line.to_string(),
        })
    }

    /// 解析 CSV 记录（按表头取字段）。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_csv_record(&self, state: &CsvState, line: &str) -> Option<WebAccessRecord> {
        let fields = split_csv_line(line);
        if fields.len() != state.headers.len() {
            return None;
        }

        if let Some(message) =
            csv_value(&state.headers, &fields, &["message", "log", "textpayload"])
        {
            let source = csv_value(&state.headers, &fields, &["provider", "cloud", "source"])
                .unwrap_or_else(|| "CSV".to_string());
            let time = csv_value(&state.headers, &fields, &["timestamp", "time"]);
            return self.parse_access_message(&message, &source, time.as_deref(), line);
        }

        let method = csv_value(
            &state.headers,
            &fields,
            &["method", "request_method", "clientrequestmethod"],
        )?;
        let path = csv_value(
            &state.headers,
            &fields,
            &["path", "uri", "request_uri", "clientrequesturi", "url"],
        )?;
        let status = csv_value(
            &state.headers,
            &fields,
            &["status", "response_status", "edgeresponsestatus"],
        )?;
        Some(WebAccessRecord {
            source: csv_value(&state.headers, &fields, &["provider", "source"])
                .unwrap_or_else(|| "CSV_ACCESS".to_string()),
            time: csv_value(&state.headers, &fields, &["timestamp", "time"])
                .map(|value| compact_time(&value))
                .unwrap_or_else(|| "-".to_string()),
            ip: csv_value(
                &state.headers,
                &fields,
                &["ip", "remote_addr", "client_ip", "clientip"],
            )
            .unwrap_or_else(|| "-".to_string()),
            method,
            path: path_from_url(&path).to_string(),
            status: status.clone(),
            bytes: csv_value(&state.headers, &fields, &["bytes", "body_bytes_sent"])
                .unwrap_or_else(|| "-".to_string()),
            referer: csv_value(&state.headers, &fields, &["referer", "http_referer"])
                .unwrap_or_else(|| "-".to_string()),
            ua: csv_value(
                &state.headers,
                &fields,
                &[
                    "user_agent",
                    "ua",
                    "http_user_agent",
                    "clientrequestuseragent",
                ],
            )
            .unwrap_or_else(|| "-".to_string()),
            reason: status_reason(&status).to_string(),
            duration_ms: csv_value(
                &state.headers,
                &fields,
                &[
                    "request_time",
                    "duration",
                    "latency",
                    "upstream_response_time",
                ],
            )
            .and_then(|value| parse_duration_ms(&value)),
            raw: line.to_string(),
        })
    }

    /// 解析 W3C 记录（按字段名取值）。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_w3c_record(&self, state: &W3cState, line: &str) -> Option<WebAccessRecord> {
        let fields = split_w3c_line(line);
        if fields.len() < state.fields.len() {
            return None;
        }
        let method = w3c_value(&state.fields, &fields, &["cs-method"])?;
        let status = w3c_value(&state.fields, &fields, &["sc-status"])?;
        let stem = w3c_value(&state.fields, &fields, &["cs-uri-stem", "cs-uri"])?;
        let query = w3c_value(&state.fields, &fields, &["cs-uri-query"]).unwrap_or_default();
        let path = if query.is_empty() || query == "-" {
            stem
        } else {
            format!("{stem}?{query}")
        };
        let date = w3c_value(&state.fields, &fields, &["date"]).unwrap_or_default();
        let time = w3c_value(&state.fields, &fields, &["time"]).unwrap_or_default();
        let source = if state
            .fields
            .iter()
            .any(|field| field.starts_with("x-edge-"))
        {
            "CloudFront"
        } else if state.fields.iter().any(|field| field == "s-sitename") {
            "IIS_W3C"
        } else {
            "W3C"
        };
        Some(WebAccessRecord {
            source: source.to_string(),
            time: compact_spaces(&format!("{date} {time}")),
            ip: w3c_value(
                &state.fields,
                &fields,
                &["c-ip", "client-ip", "x-forwarded-for"],
            )
            .unwrap_or_else(|| "-".to_string()),
            method,
            path: path_from_url(&path).to_string(),
            status: status.clone(),
            bytes: w3c_value(&state.fields, &fields, &["sc-bytes", "bytes"])
                .unwrap_or_else(|| "-".to_string()),
            referer: w3c_value(&state.fields, &fields, &["cs(referer)"])
                .map(|value| compact_w3c_text(&value))
                .unwrap_or_else(|| "-".to_string()),
            ua: w3c_value(&state.fields, &fields, &["cs(user-agent)", "user-agent"])
                .map(|value| compact_w3c_text(&value))
                .unwrap_or_else(|| "-".to_string()),
            reason: status_reason(&status).to_string(),
            duration_ms: w3c_value(&state.fields, &fields, &["time-taken"]).and_then(|value| {
                if source == "IIS_W3C" {
                    value.parse::<u64>().ok()
                } else {
                    parse_duration_ms(&value)
                }
            }),
            raw: line.to_string(),
        })
    }

    /// 解析包裹记录（AWS logs tail/CloudWatch 表格行）。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_wrapped_record(&self, line: &str) -> Option<WebAccessRecord> {
        // 单行云 CSV 包装器：无表头时形如 `2026-05-13T08:34:03Z,azure,"10.0.0.3 - - [...] "GET /x HTTP/1.1" 500 ..."`
        // 的 `timestamp,provider,"message"`（消息字段用 CSV 双引号转义）。三重守卫避免误伤真实 raw 访问行：
        // 1) split_csv_line 恰好 3 字段；2) 字段0 形如时间戳（含 ':'）；3) 字段1 为短 provider 令牌。
        // 消息字段经 split_csv_line 的引号转义还原后再走标准访问解析，从而把包装行内的 4xx/5xx 信号聚合进 records。
        // 必须置于宽松的 aws_logs_tail 之前：否则该行会被其 `timestamp stream message` 误匹配，message 抽成
        // 垃圾串、内层解析失败即 return，导致本回退永不触发。
        {
            let fields = split_csv_line(line.trim());
            if fields.len() == 3
                && fields[0].contains(':')
                && !fields[1].is_empty()
                && fields[1].len() <= 32
                && fields[1]
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                if let Some(record) = self.parse_access_message(
                    &fields[2],
                    &fields[1],
                    Some(fields[0].as_str()),
                    line,
                ) {
                    return Some(record);
                }
            }
        }
        if let Some(caps) = self.aws_logs_tail_pattern.captures(line.trim()) {
            let message = caps.name("message")?.as_str();
            let source = format!("AWS:{}", compact_stream(caps.name("stream")?.as_str()));
            return self.parse_access_message(
                message,
                &source,
                Some(caps.name("time")?.as_str()),
                line,
            );
        }
        if let Some(caps) = self.cloudwatch_table_row_pattern.captures(line.trim()) {
            return self.parse_access_message(
                caps.name("message")?.as_str(),
                "CLOUD_TABLE",
                Some(caps.name("time")?.as_str()),
                line,
            );
        }
        None
    }

    /// 解析访问记录（按 w3c/csv/包裹/JSON/纯文本 依次尝试）。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_access_record(
        &self,
        line: &str,
        csv_state: Option<&CsvState>,
        w3c_state: Option<&W3cState>,
    ) -> Option<WebAccessRecord> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || is_cloudwatch_table_noise(trimmed) {
            return None;
        }
        if let Some(state) = w3c_state {
            if let Some(record) = self.parse_w3c_record(state, trimmed) {
                return Some(record);
            }
        }
        if let Some(state) = csv_state {
            if let Some(record) = self.parse_csv_record(state, trimmed) {
                return Some(record);
            }
        }
        self.parse_wrapped_record(trimmed)
            .or_else(|| self.parse_json_record(trimmed))
            .or_else(|| self.parse_access_message(trimmed, "WEB", None, line))
    }

    /// 压缩访问记录：解析后汇总输出（摘要/异常/慢请求/v3 IR），ROI 门控。
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_access_records(&self, text: &str) -> Option<String> {
        let (records, passthrough) = self.parse_access_records_and_passthrough(text);

        if records.len() < 2 {
            return None;
        }

        // 升级/重定向（1xx 与 3xx 非 304）是决策相关信号：WebSocket 升级、redirect 链的目标路径都在
        // 这两类状态码里。聚合摘要默认只把它们归入 st=/3xx= 计数，不保留代表性样本，会让语义门禁判定
        // "升级/重定向意图信息不足"（见 case_035）。此类切片回退到逐行 $W|A 输出，完整保留每条的状态码
        // 与路径，语义无损。
        if records
            .iter()
            .any(|r| r.status.starts_with('1') || (r.status.starts_with('3') && r.status != "304"))
        {
            return None;
        }

        let stats = collect_access_summary_stats(&records);
        let mut out = String::new();
        emit_passthrough_lines(&mut out, &passthrough);

        if try_emit_compact_health_summary(&mut out, &records, &stats) {
            return Some(out);
        }

        let collection = collect_access_v3_collection(&records);
        let ip_tokens = build_ip_tokens(&collection.ip_counts, &collection.scan_ips);
        let ua_tokens = build_ua_tokens(&collection.ua_counts);
        let has_scan = !collection.scans.is_empty();
        emit_access_summary_block(&mut out, &records, &stats, &ip_tokens, &ua_tokens, has_scan);
        append_access_v3_ir(&mut out, &records, &collection, &ip_tokens, &ua_tokens);
        // 仅当 v3 分支激活（会输出 SCAN 行）时才允许抑制纯扫描源 ANOMALY，见 emit_access_anomaly_lines。
        emit_access_anomaly_lines(
            &mut out,
            &stats.anomalies,
            &collection.scan_ips,
            should_emit_access_v3(&records, has_scan),
        );
        emit_access_slow_lines(&mut out, &stats.slow);

        Some(out)
    }

    /// 解析访问记录与透传行（识别表头与噪音）。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_access_records_and_passthrough(
        &self,
        text: &str,
    ) -> (Vec<WebAccessRecord>, Vec<String>) {
        let mut records = Vec::new();
        let mut passthrough = Vec::new();
        let mut csv_state: Option<CsvState> = None;
        let mut w3c_state: Option<W3cState> = None;

        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(state) = w3c_header(trimmed) {
                w3c_state = Some(state);
                continue;
            }
            if trimmed.starts_with('#') {
                continue;
            }
            if let Some(state) = csv_header(trimmed) {
                csv_state = Some(state);
                continue;
            }
            if is_cloudwatch_table_noise(trimmed) {
                continue;
            }
            if let Some(record) =
                self.parse_access_record(line, csv_state.as_ref(), w3c_state.as_ref())
            {
                records.push(record);
            } else if !trimmed.is_empty() {
                passthrough.push(line.to_string());
            }
        }
        (records, passthrough)
    }
}

impl Default for WebLogPlugin {
    /// Default 实现：等价于 new()。
    fn default() -> Self {
        Self::new()
    }
}

/// 去掉局部字典条目值末尾的 (class) 注解，如 `10.0.0.52(Internal)` -> `10.0.0.52`。
fn strip_dict_class(val: &str) -> String {
    match val.rfind('(') {
        Some(idx) if val[idx..].ends_with(')') => val[..idx].to_string(),
        _ => val.to_string(),
    }
}

/// 解析插件局部字典行（$W|DICT_IP / $W|DICT_UA），返回 token->值 映射。
/// 压缩侧把 IP/UA 写成局部协议行（不进全局 dictionary），解压侧需自行解析。
fn parse_web_log_local_dict(compressed: &str) -> BTreeMap<String, String> {
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for line in compressed.lines() {
        let rest = if let Some(r) = line.strip_prefix("$W|DICT_IP|") {
            r
        } else if let Some(r) = line.strip_prefix("$W|DICT_UA|") {
            r
        } else {
            continue;
        };
        for entry in rest.trim_end().split(',') {
            if let Some((tok, val)) = entry.split_once('=') {
                let tok = tok.trim();
                if !tok.is_empty() {
                    map.insert(tok.to_string(), strip_dict_class(val.trim()));
                }
            }
        }
    }
    map
}

impl Plugin for WebLogPlugin {
    /// 返回插件名称 "web_log"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级 170。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：前 12 行中可解析行占比 ≥25% 时命中。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let lines: Vec<&str> = slice.text.lines().take(12).collect();
        if lines.is_empty() {
            return None;
        }
        let mut matched = 0;
        let mut csv_state: Option<CsvState> = None;
        let mut w3c_state: Option<W3cState> = None;
        for line in &lines {
            if let Some(state) = w3c_header(line.trim()) {
                w3c_state = Some(state);
                matched += 1;
                continue;
            }
            if line.trim_start().starts_with('#') {
                continue;
            }
            if let Some(state) = csv_header(line.trim()) {
                csv_state = Some(state);
                matched += 1;
                continue;
            }
            if self
                .parse_access_record(line, csv_state.as_ref(), w3c_state.as_ref())
                .is_some()
                || self.error_log_pattern.is_match(line)
            {
                matched += 1;
            }
        }
        let ratio = matched as f32 / lines.len() as f32;
        if ratio >= 0.25 {
            Some((ratio + 0.2).min(1.0))
        } else {
            None
        }
    }

    /// 压缩切片：聚合摘要优先，否则逐行 token 化（$W|A/$W|E），ROI 门控。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let aggregate = self.compress_access_records(text);

        let mut tokens: Vec<Token<'a>> = Vec::new();
        for line in text.lines() {
            if let Some(caps) = self.combined_log_pattern.captures(line) {
                let ip_token = dict_engine.add_macro(caps.name("ip").unwrap().as_str());
                let path_token = dict_engine.add_path_layered(caps.name("path").unwrap().as_str());
                let ua_token = dict_engine.add_macro(caps.name("ua").unwrap().as_str());
                tokens.push(Token::Text(
                    format!(
                        "$W|A|{}|{}|{}|{}|{}|{}|{}|{}\n",
                        ip_token,
                        caps.name("time").unwrap().as_str(),
                        caps.name("method").unwrap().as_str(),
                        path_token,
                        caps.name("status").unwrap().as_str(),
                        caps.name("bytes").unwrap().as_str(),
                        caps.name("referer").unwrap().as_str(),
                        ua_token
                    )
                    .into(),
                ));
            } else if let Some(caps) = self.error_log_pattern.captures(line) {
                tokens.push(Token::Text(
                    format!(
                        "$W|E|{}|{}|{}\n",
                        caps.name("time").unwrap().as_str(),
                        caps.name("level").unwrap().as_str(),
                        caps.name("msg").unwrap().as_str()
                    )
                    .into(),
                ));
            } else {
                tokens.push(Token::Text(format!("{}\n", line).into()));
            }
        }

        let legacy: String = tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_ref(),
                _ => "",
            })
            .collect();
        let compacted = match aggregate {
            // 优先选择聚合摘要：它在不扩张原始输入的前提下，把包裹/JSON/原始访问行统一归一为结构化
            // SUMMARY + ANOMALY，语义更利于 LLM 消费。原阈值把 record_count>=8 作为小样本聚合门的近似，
            // 导致 case_036 这类仅数行的混合包装样本即便聚合更优（5xx 已折叠进摘要）也因 477 vs 491 的
            // 字节大小而回退到逐行透传，丢失聚合信号。改以"聚合后不超过原始输入"为准。
            Some(aggregate)
                if aggregate.len() < legacy.len()
                    || (aggregate.len() < text.len()
                        && aggregate_record_count(&aggregate) >= 2) =>
            {
                aggregate
            }
            _ => legacy,
        };
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, compacted);

        CompressResult {
            tokens: vec![Token::Text(final_text.into())],
            metadata: None,
            plugin_name: Some(self.name),
        }
    }

    /// 解压：将 $W|A/$W|E 行还原为访问/错误日志格式；
    /// 对聚合摘要行（ROUTINE/SCAN/BURST/ANOMALY/SLOW/DIAG 等）行内替换局部 IP/UA token；
    /// 局部字典行（$W|DICT_IP/$W|DICT_UA）禁止泄漏，直接丢弃（T-011 根治）。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        let local = parse_web_log_local_dict(compressed);
        // 预编译替换正则：按 key 长度降序，避免 $IP1 误匹配 $IP10 等长前缀 token。
        let mut keys: Vec<&String> = local.keys().collect();
        keys.sort_by(|a, b| b.len().cmp(&a.len()));
        let re = if keys.is_empty() {
            None
        } else {
            let pat = keys
                .iter()
                .map(|k| regex::escape(k))
                .collect::<Vec<_>>()
                .join("|");
            Regex::new(&format!("({})", pat)).ok()
        };

        let mut out = String::new();
        for line in compressed.lines() {
            // 局部字典行：禁止泄漏，直接丢弃。
            if line.starts_with("$W|DICT_IP|") || line.starts_with("$W|DICT_UA|") {
                continue;
            }
            if line.starts_with("$W|A|") {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 10 {
                    let ip = local
                        .get(parts[2])
                        .cloned()
                        .unwrap_or_else(|| dict.resolve_or_self(parts[2]).to_string());
                    let path = dict.resolve_or_self(parts[5]);
                    let ua_joined = parts[9..].join("|");
                    let ua = local
                        .get(&ua_joined)
                        .cloned()
                        .unwrap_or_else(|| dict.resolve_or_self(&ua_joined).to_string());
                    out.push_str(&format!(
                        "{} - - [{}] \"{} {} HTTP/1.1\" {} {} \"{}\" \"{}\"\n",
                        ip, parts[3], parts[4], path, parts[6], parts[7], parts[8], ua
                    ));
                    continue;
                }
            } else if line.starts_with("$W|E|") {
                let parts: Vec<&str> = line.splitn(5, '|').collect();
                if parts.len() == 5 {
                    out.push_str(&format!(
                        "{} [{}] 0#0: *0 {}\n",
                        parts[2], parts[3], parts[4]
                    ));
                    continue;
                }
            }
            // 其余行（聚合摘要行等）：行内替换局部 IP/UA token。
            let resolved = match &re {
                Some(re) => re
                    .replace_all(line, |caps: &regex::Captures| {
                        local
                            .get(caps.get(0).unwrap().as_str())
                            .cloned()
                            .unwrap_or_default()
                    })
                    .into_owned(),
                None => line.to_string(),
            };
            out.push_str(&resolved);
            out.push('\n');
        }
        out
    }

    /// 返回后续插件列表（smart_path）。
    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试辅助：构造样例 WebAccessRecord。
    fn sample_record() -> WebAccessRecord {
        WebAccessRecord {
            source: "WEB".to_string(),
            time: "2026-05-21 10:00:00".to_string(),
            ip: "10.0.0.1".to_string(),
            method: "GET".to_string(),
            path: "/api/orders/12345?x=1".to_string(),
            status: "200".to_string(),
            bytes: "128".to_string(),
            referer: "-".to_string(),
            ua: "curl/8.0".to_string(),
            reason: "OK".to_string(),
            duration_ms: Some(1530),
            raw: "raw".to_string(),
        }
    }

    /// 测试：空条目字典行返回 None。
    #[test]
    fn render_access_dict_line_returns_none_when_entries_empty() {
        assert!(render_access_dict_line("DICT_IP", Vec::new()).is_none());
    }

    /// 测试：无计时数据时 routine 行 avg_ms 为 -。
    #[test]
    fn render_access_routine_line_uses_dash_avg_when_no_timing() {
        let mut ips = BTreeSet::new();
        ips.insert("10.0.0.1".to_string());
        let mut uas = BTreeSet::new();
        uas.insert("curl/8.0".to_string());
        let bucket = RoutineBucket {
            kind: "routine",
            status: "200".to_string(),
            method: "GET".to_string(),
            route: "/api/orders/:id".to_string(),
            count: 2,
            ips,
            uas,
            total_ms: 0,
            timed_count: 0,
        };
        let line = render_access_routine_line(&bucket, &BTreeMap::new(), &BTreeMap::new());
        assert!(line.contains("avg_ms=-"), "line={line}");
    }

    /// 测试：扫描行包含窗口与样本目标。
    #[test]
    fn render_access_scan_line_includes_window_and_sample_targets() {
        let record = sample_record();
        let items = vec![&record, &record];
        let mut targets = BTreeSet::new();
        targets.insert("/.env".to_string());
        targets.insert("/wp-admin".to_string());
        let line = render_access_scan_line(&items, &targets, &BTreeMap::new(), &BTreeMap::new());
        assert!(line.contains("!$W|SCAN|"));
        assert!(line.contains("window=2026-05-21 10:00:00..2026-05-21 10:00:00"));
        assert!(line.contains("sample=/.env,/wp-admin") || line.contains("sample=/wp-admin,/.env"));
    }

    /// 契约测试：`render_access_ip_dict_line` 按 ip_tokens 过滤并按 ip 计数排序渲染 DICT_IP 字典行。
    #[test]
    fn render_access_ip_dict_line_filters_by_tokens() {
        let mut counts = BTreeMap::new();
        counts.insert("10.0.0.1".to_string(), 5usize);
        counts.insert("10.0.0.2".to_string(), 3usize);
        let mut tokens = BTreeMap::new();
        tokens.insert("10.0.0.1".to_string(), "ia".to_string());
        // 10.0.0.2 未在 tokens 中，应被过滤掉
        let line = render_access_ip_dict_line(&counts, &tokens).expect("tokens 非空应有字典行");
        assert!(line.starts_with("$W|DICT_IP|"), "line={line}");
        assert!(line.contains("ia=10.0.0.1"), "line={line}");
        assert!(
            !line.contains("10.0.0.2"),
            "未被打点 IP 不应出现在行内: {line}"
        );

        // 空 tokens 返回 None
        assert!(render_access_ip_dict_line(&counts, &BTreeMap::new()).is_none());
    }

    /// 契约测试：`render_access_ua_dict_line` 按 ua_tokens 过滤并按 ua 计数排序渲染 DICT_UA 字典行。
    #[test]
    fn render_access_ua_dict_line_filters_by_tokens() {
        let mut counts = BTreeMap::new();
        counts.insert("curl/8.0".to_string(), 5usize);
        counts.insert("python-requests".to_string(), 2usize);
        let mut tokens = BTreeMap::new();
        tokens.insert("curl/8.0".to_string(), "ub".to_string());
        let line = render_access_ua_dict_line(&counts, &tokens).expect("tokens 非空应有字典行");
        assert!(line.starts_with("$W|DICT_UA|"), "line={line}");
        assert!(line.contains("ub=curl/8.0("), "line={line}");
        assert!(
            !line.contains("python-requests"),
            "未被打点 UA 不应出现在行内: {line}"
        );

        // 空 tokens 返回 None
        assert!(render_access_ua_dict_line(&counts, &BTreeMap::new()).is_none());
    }

    /// 契约测试：`parse_web_log_local_dict` 解析 DICT_IP/DICT_UA 行并忽略其余行，剥离 (类别) 后缀并丢弃空 token。
    #[test]
    fn parse_web_log_local_dict_roundtrips_dict_lines() {
        let compressed = "$W|DICT_IP|ia=10.0.0.1(private),ib=10.0.0.2(private)\n\
                          $W|DICT_UA|ub=curl/8.0(cli)\n\
                          $W|DIAG|err_rate=0.0\n\
                          $W|DICT_IP|=emptytoken\n";
        let map = parse_web_log_local_dict(compressed);
        assert_eq!(map.get("ia").map(String::as_str), Some("10.0.0.1"));
        assert_eq!(map.get("ib").map(String::as_str), Some("10.0.0.2"));
        assert_eq!(map.get("ub").map(String::as_str), Some("curl/8.0"));
        assert!(!map.contains_key("err_rate"), "非字典行应被忽略");
        assert!(!map.contains_key(""), "空 token 条目应被丢弃");
    }

    /// 测试：突发行统计去重 IP 数。
    #[test]
    fn render_access_burst_line_counts_unique_ips() {
        let first = sample_record();
        let mut second = sample_record();
        second.ip = "10.0.0.2".to_string();
        second.status = "503".to_string();
        let items = vec![&first, &second];
        let line = render_access_burst_line(&items, &BTreeMap::new());
        assert!(line.contains("!$W|BURST|"));
        assert!(line.contains("ips=2"), "line={line}");
    }

    /// 测试：慢请求行归一化路由 ID。
    #[test]
    fn render_access_slow_line_normalizes_route_id() {
        let record = sample_record();
        let line = render_access_slow_line(&record);
        assert!(line.contains("!$W|SLOW|GET /api/orders/:id|"));
        assert!(line.contains("ms=1530"));
    }
}
