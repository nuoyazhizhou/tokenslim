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
}

#[tracing::instrument(level = "debug", skip_all)]
fn compact_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn trim_to(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    format!("{}...", &value[..max_len])
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn inc(map: &mut BTreeMap<String, usize>, key: impl Into<String>) {
    *map.entry(key.into()).or_insert(0) += 1;
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn unique_token(base: &str, used: &mut BTreeSet<String>, counter: usize) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    let token = format!("{base}{counter}");
    used.insert(token.clone());
    token
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn is_static_route(route: &str) -> bool {
    let lower = route.to_ascii_lowercase();
    [
        ".js", ".css", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".map", ".woff", ".woff2",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn token_ref(value: &str, tokens: &BTreeMap<String, String>) -> String {
    tokens
        .get(value)
        .cloned()
        .unwrap_or_else(|| trim_to(value, 28))
}

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
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn append_access_v3_ir(out: &mut String, records: &[WebAccessRecord]) {
    if !should_emit_access_v3(records) {
        return;
    }
    let collection = collect_access_v3_collection(records);

    let scanner_ips = collection
        .scans
        .iter()
        .filter_map(|(items, _)| items.first().map(|record| record.ip.clone()))
        .collect::<BTreeSet<_>>();
    let ip_tokens = build_ip_tokens(&collection.ip_counts, &scanner_ips);
    let ua_tokens = build_ua_tokens(&collection.ua_counts);

    if let Some(ip_dict_line) = render_access_ip_dict_line(&collection.ip_counts, &ip_tokens) {
        out.push_str(&ip_dict_line);
    }
    if let Some(ua_dict_line) = render_access_ua_dict_line(&collection.ua_counts, &ua_tokens) {
        out.push_str(&ua_dict_line);
    }
    emit_access_diag(
        out,
        records,
        &collection.status_counts,
        collection.health_count,
        collection.static_count,
        collection.bot_count,
    );
    emit_access_routines(out, collection.routine_entries, &ip_tokens, &ua_tokens);
    emit_access_scans(out, collection.scans, &ip_tokens, &ua_tokens);
    emit_access_bursts(out, collection.bursts, &ip_tokens);
}

#[tracing::instrument(level = "debug", skip_all)]
fn render_access_dict_line(kind: &str, entries: Vec<String>) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    Some(format!("$W|{kind}|{}\n", entries.join(",")))
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn should_emit_access_v3(records: &[WebAccessRecord]) -> bool {
    records.len() >= 24
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_routines(
    out: &mut String,
    routine_entries: Vec<RoutineBucket>,
    ip_tokens: &BTreeMap<String, String>,
    ua_tokens: &BTreeMap<String, String>,
) {
    for bucket in routine_entries
        .into_iter()
        .filter(|bucket| bucket.count >= 2 || bucket.kind != "routine")
        .take(8)
    {
        out.push_str(&render_access_routine_line(&bucket, ip_tokens, ua_tokens));
    }
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn render_routine_avg_ms(bucket: &RoutineBucket) -> String {
    if bucket.timed_count > 0 {
        (bucket.total_ms / bucket.timed_count as u64).to_string()
    } else {
        "-".to_string()
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_scans(
    out: &mut String,
    scans: Vec<(Vec<&WebAccessRecord>, BTreeSet<String>)>,
    ip_tokens: &BTreeMap<String, String>,
    ua_tokens: &BTreeMap<String, String>,
) {
    for (items, targets) in scans.into_iter().take(5) {
        out.push_str(&render_access_scan_line(
            &items, &targets, ip_tokens, ua_tokens,
        ));
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn render_access_scan_line(
    items: &[&WebAccessRecord],
    targets: &BTreeSet<String>,
    ip_tokens: &BTreeMap<String, String>,
    ua_tokens: &BTreeMap<String, String>,
) -> String {
    let first = items.first().unwrap();
    let (window_start, window_end) = render_scan_window(items);
    let sample = render_scan_target_sample(targets, 8);
    format!(
        "!$W|SCAN|source={}|ua={}|window={}..{}|targets={}|sample={}\n",
        token_ref(&first.ip, ip_tokens),
        token_ref(&first.ua, ua_tokens),
        window_start,
        window_end,
        items.len(),
        sample
    )
}

#[tracing::instrument(level = "debug", skip_all)]
fn render_scan_target_sample(targets: &BTreeSet<String>, limit: usize) -> String {
    targets
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .join(",")
}

#[tracing::instrument(level = "debug", skip_all)]
fn render_scan_window(items: &[&WebAccessRecord]) -> (String, String) {
    let first = items.first().unwrap();
    let last = items.last().unwrap();
    (first.time.clone(), last.time.clone())
}

#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_bursts(
    out: &mut String,
    bursts: Vec<Vec<&WebAccessRecord>>,
    ip_tokens: &BTreeMap<String, String>,
) {
    for items in bursts.into_iter().take(5) {
        out.push_str(&render_access_burst_line(&items, ip_tokens));
    }
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn collect_burst_ips(items: &[&WebAccessRecord]) -> BTreeSet<String> {
    let mut burst_ips = BTreeSet::new();
    for record in items {
        burst_ips.insert(record.ip.clone());
    }
    burst_ips
}

#[tracing::instrument(level = "debug", skip_all)]
fn render_burst_window(items: &[&WebAccessRecord]) -> (String, String) {
    let first = items.first().unwrap();
    let last = items.last().unwrap();
    (first.time.clone(), last.time.clone())
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn emit_passthrough_lines(out: &mut String, passthrough: &[String]) {
    for line in passthrough {
        out.push_str(line);
        out.push('\n');
    }
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_summary_block(
    out: &mut String,
    records: &[WebAccessRecord],
    stats: &AccessSummaryStats<'_>,
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
    out.push_str(&format!(
        "$W|TOP_IP|{}|$W|TOP_UA|{}",
        top_entries(&stats.ips, 8),
        top_entries(&stats.uas, 6)
    ));
    if !stats.referers.is_empty() {
        out.push_str(&format!("|$W|TOP_REF|{}", top_entries(&stats.referers, 6)));
    }
    out.push('\n');
}

#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_anomaly_lines(
    out: &mut String,
    anomalies: &BTreeMap<String, Vec<&WebAccessRecord>>,
) {
    let mut anomaly_entries = anomalies.iter().collect::<Vec<_>>();
    anomaly_entries.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(b.0)));
    for (key, items) in anomaly_entries.into_iter().take(8) {
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

#[tracing::instrument(level = "debug", skip_all)]
fn emit_access_slow_lines(out: &mut String, slow: &[&WebAccessRecord]) {
    for record in sort_slow_access_records(slow).into_iter().take(5) {
        out.push_str(&render_access_slow_line(record));
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn sort_slow_access_records<'a>(slow: &'a [&'a WebAccessRecord]) -> Vec<&'a WebAccessRecord> {
    let mut sorted = slow.to_vec();
    sorted.sort_by(|a, b| b.duration_ms.cmp(&a.duration_ms));
    sorted
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn render_slow_identity(record: &WebAccessRecord) -> (String, String) {
    (record.method.clone(), normalize_route(&record.path))
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn split_w3c_line(line: &str) -> Vec<String> {
    line.split_whitespace()
        .map(|value| value.trim().to_string())
        .collect()
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn w3c_value(fields: &[String], values: &[String], candidates: &[&str]) -> Option<String> {
    for candidate in candidates {
        if let Some(index) = fields.iter().position(|field| field == candidate) {
            return values.get(index).cloned();
        }
    }
    None
}

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

#[tracing::instrument(level = "debug", skip_all)]
fn json_string<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_str()
}

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
                ua: caps.name("level")?.as_str().to_string(),
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

    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_wrapped_record(&self, line: &str) -> Option<WebAccessRecord> {
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

    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_access_records(&self, text: &str) -> Option<String> {
        let (records, passthrough) = self.parse_access_records_and_passthrough(text);

        if records.len() < 2 {
            return None;
        }

        let stats = collect_access_summary_stats(&records);
        let mut out = String::new();
        emit_passthrough_lines(&mut out, &passthrough);

        if try_emit_compact_health_summary(&mut out, &records, &stats) {
            return Some(out);
        }

        emit_access_summary_block(&mut out, &records, &stats);
        append_access_v3_ir(&mut out, &records);
        emit_access_anomaly_lines(&mut out, &stats.anomalies);
        emit_access_slow_lines(&mut out, &stats.slow);

        Some(out)
    }

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
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for WebLogPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

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
            Some(aggregate)
                if aggregate.len() < legacy.len()
                    || (aggregate.len() < text.len()
                        && aggregate_record_count(&aggregate) >= 8) =>
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

    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        let mut out = String::new();
        for line in compressed.lines() {
            if line.starts_with("$W|A|") {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 10 {
                    let ip = dict.resolve_or_self(parts[2]);
                    let path = dict.resolve_or_self(parts[5]);
                    let ua = dict.resolve_or_self(&parts[9..].join("|"));
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
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn render_access_dict_line_returns_none_when_entries_empty() {
        assert!(render_access_dict_line("DICT_IP", Vec::new()).is_none());
    }

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

    #[test]
    fn render_access_slow_line_normalizes_route_id() {
        let record = sample_record();
        let line = render_access_slow_line(&record);
        assert!(line.contains("!$W|SLOW|GET /api/orders/:id|"));
        assert!(line.contains("ms=1530"));
    }
}
