"""Generate generic batch-processing logs for case_046 (table) and case_047/048 (CSV).

Strips out the previous crawler/price-scraping business.
Replaces with generic ETL/image/document/log-batch jobs that still exercise
the cloud_log_plugin compression logic (repeated patterns, long param lines,
progress, warnings, retries, checkpoints, START/END/REPORT).
"""

import os


def _split_ms(ms_total):
    """Convert a ms-since-minute-start value to (minute_offset, sec_ms)."""
    return ms_total // 1000, ms_total % 1000


# ----------------------------------------------------------------------
# case_046: AWS CloudWatch Logs Insights TABLE format (wide table)
# Generic CSV -> Parquet ETL batch
# ----------------------------------------------------------------------

def emit_case_046_table(path, width=1408):
    sep = '-' * width

    def row(ts, msg):
        ts_field = ts.center(15)
        msg_field = msg + ' ' * max(0, width - 15 - 6 - len(msg))
        return f"| {ts_field} | {msg_field} |"

    events = []
    req_id = "196a221f-ccf1-4231-9769-f0e4e8b5db42"

    # Header / lifecycle
    events.append(("1770000000000", f"START RequestId: {req_id} Version: $LATEST"))
    events.append(("1770000000100", f"[2026-06-05 01:01:00,100] INFO: 下载 S3 -> 本地: s3://sample-batch-processor/input/orders-2026-06-04.csv  ->  /tmp/orders-2026-06-04.csv"))
    events.append(("1770000000100", f"[INFO] 2026-06-05T01:01:00.100Z {req_id} 下载 S3 -> 本地: s3://sample-batch-processor/input/orders-2026-06-04.csv  ->  /tmp/orders-2026-06-04.csv"))
    events.append(("1770000000300", f"[2026-06-05 01:01:00,300] INFO: 校验 CSV 表头 expected=12 actual=12 status=ok"))
    events.append(("1770000000300", f"[INFO] 2026-06-05T01:01:00.300Z {req_id} 校验 CSV 表头 expected=12 actual=12 status=ok"))
    events.append(("1770000000500", f"[2026-06-05 01:01:00,500] INFO: 推断 schema: [order_id:str, customer_id:str, sku:str, qty:int, unit_price:decimal, currency:str, region:str, channel:str, status:str, created_at:ts, updated_at:ts, notes:str]"))
    events.append(("1770000000500", f"[INFO] 2026-06-05T01:01:00.500Z {req_id} 推断 schema: [order_id:str, customer_id:str, sku:str, qty:int, unit_price:decimal, currency:str, region:str, channel:str, status:str, created_at:ts, updated_at:ts, notes:str]"))

    cmd = "python etl_worker.py --input /tmp/orders-2026-06-04.csv --output-prefix s3://sample-batch-processor/parquet/orders/2026-06-05/ --target-format parquet --compression snappy --partition-by region --target-file-size 134217728 --max-concurrency 8 --batch-size 5000 --request-timeout 60 --log-level INFO --db-host db-primary.example.local --db-port 3306 --db-name sample_app --db-user app --db-pass REDACTED --on-conflict overwrite --manifest /tmp/orders-2026-06-04.csv.manifest.json"
    events.append(("1770000000600", f"[2026-06-05 01:01:00,600] INFO: 启动批处理进程：{cmd}"))
    events.append(("1770000000600", f"[INFO] 2026-06-05T01:01:00.600Z {req_id} 启动批处理进程：{cmd}"))

    events.append(("1770000000800", f"[2026-06-05 01:01:00,800] INFO: [worker stdout] [2026-06-05 01:01:00,800] INFO: 共加载 1024 个 batch，shards: ['shard-0', 'shard-1', 'shard-2', 'shard-3']"))
    events.append(("1770000000800", f"[INFO] 2026-06-05T01:01:00.800Z {req_id} [worker stdout] [2026-06-05 01:01:00,800] INFO: 共加载 1024 个 batch，shards: ['shard-0', 'shard-1', 'shard-2', 'shard-3']"))

    # Worker progress + write events
    regions_upper = ["EU", "NA", "APAC", "LATAM"]
    regions_lower = ["eu", "na", "apac", "latam"]
    total = 1024
    base_minute = 20  # anchor minute inside the hour
    ts_base = 1770000010000
    for shard in range(4):
        for i in range(64):  # 64 * 4 = 256 events per shard, 1024 total
            item_idx = shard * 256 + i + 1
            ms_a = (shard * 60000) + (i * 200)
            minute_a = base_minute + shard * 2 + ms_a // 60000
            sec_a_ms = ms_a % 60000
            sec_a_min, sec_a = _split_ms(sec_a_ms)
            minute_a += sec_a_min
            ts = ts_base + ms_a
            events.append((str(ts), f"[2026-06-05 01:{minute_a:02d}:{sec_a_min:02d},{sec_a:03d}] INFO: [worker stdout] [2026-06-05 01:{minute_a:02d}:{sec_a_min:02d},{sec_a:03d}] INFO: [进度] {item_idx}/{total} | shard=shard-{shard} | partition=region={regions_upper[shard]}"))
            events.append((str(ts), f"[INFO] 2026-06-05T01:{minute_a:02d}:{sec_a_min:02d}.{sec_a:03d}Z {req_id} [worker stdout] [2026-06-05 01:{minute_a:02d}:{sec_a_min:02d},{sec_a:03d}] INFO: [进度] {item_idx}/{total} | shard=shard-{shard} | partition=region={regions_upper[shard]}"))

            ms_b = ms_a + 50
            minute_b = base_minute + shard * 2 + ms_b // 60000
            sec_b_ms = ms_b % 60000
            sec_b_min, sec_b = _split_ms(sec_b_ms)
            minute_b += sec_b_min
            ts2 = ts + 50
            events.append((str(ts2), f"[2026-06-05 01:{minute_b:02d}:{sec_b_min:02d},{sec_b:03d}] INFO: [worker stdout] [2026-06-05 01:{minute_b:02d}:{sec_b_min:02d},{sec_b:03d}] INFO: 写出 batch | shard=shard-{shard} | rows=5000 | bytes=4194304 | path=s3://sample-batch-processor/parquet/orders/2026-06-05/region={regions_lower[shard]}/part-{i:05d}.snappy.parquet | engine=pyarrow"))
            events.append((str(ts2), f"[INFO] 2026-06-05T01:{minute_b:02d}:{sec_b_min:02d}.{sec_b:03d}Z {req_id} [worker stdout] [2026-06-05 01:{minute_b:02d}:{sec_b_min:02d},{sec_b:03d}] INFO: 写出 batch | shard=shard-{shard} | rows=5000 | bytes=4194304 | path=s3://sample-batch-processor/parquet/orders/2026-06-05/region={regions_lower[shard]}/part-{i:05d}.snappy.parquet | engine=pyarrow"))

            if i == 30 and shard == 1:
                ms_c = ms_a + 100
                minute_c = base_minute + shard * 2 + ms_c // 60000
                sec_c_ms = ms_c % 60000
                sec_c_min, sec_c = _split_ms(sec_c_ms)
                minute_c += sec_c_min
                ts3 = ts + 100
                events.append((str(ts3), f"[2026-06-05 01:{minute_c:02d}:{sec_c_min:02d},{sec_c:03d}] INFO: [worker stdout] [2026-06-05 01:{minute_c:02d}:{sec_c_min:02d},{sec_c:03d}] INFO: WARN  | shard=shard-{shard} | batch={i} | reason=schema_coerce | detail=unit_price rounded to 2 decimals for 12 rows"))
                events.append((str(ts3), f"[INFO] 2026-06-05T01:{minute_c:02d}:{sec_c_min:02d}.{sec_c:03d}Z {req_id} [worker stdout] [2026-06-05 01:{minute_c:02d}:{sec_c_min:02d},{sec_c:03d}] INFO: WARN  | shard=shard-{shard} | batch={i} | reason=schema_coerce | detail=unit_price rounded to 2 decimals for 12 rows"))
            if i == 50 and shard == 2:
                ms_d = ms_a + 150
                minute_d = base_minute + shard * 2 + ms_d // 60000
                sec_d_ms = ms_d % 60000
                sec_d_min, sec_d = _split_ms(sec_d_ms)
                minute_d += sec_d_min
                ts4 = ts + 150
                events.append((str(ts4), f"[2026-06-05 01:{minute_d:02d}:{sec_d_min:02d},{sec_d:03d}] INFO: [worker stdout] [2026-06-05 01:{minute_d:02d}:{sec_d_min:02d},{sec_d:03d}] INFO: ERROR | shard=shard-{shard} | batch={i} | reason=s3_throttle | detail=RequestLimitExceeded will retry 1/3"))
                events.append((str(ts4), f"[INFO] 2026-06-05T01:{minute_d:02d}:{sec_d_min:02d}.{sec_d:03d}Z {req_id} [worker stdout] [2026-06-05 01:{minute_d:02d}:{sec_d_min:02d},{sec_d:03d}] INFO: ERROR | shard=shard-{shard} | batch={i} | reason=s3_throttle | detail=RequestLimitExceeded will retry 1/3"))
                ms_e = ms_a + 450
                minute_e = base_minute + shard * 2 + ms_e // 60000
                sec_e_ms = ms_e % 60000
                sec_e_min, sec_e = _split_ms(sec_e_ms)
                minute_e += sec_e_min
                ts5 = ts + 450
                events.append((str(ts5), f"[2026-06-05 01:{minute_e:02d}:{sec_e_min:02d},{sec_e:03d}] INFO: [worker stdout] [2026-06-05 01:{minute_e:02d}:{sec_e_min:02d},{sec_e:03d}] INFO: 重试成功 | shard=shard-{shard} | batch={i} | ms=300"))
                events.append((str(ts5), f"[INFO] 2026-06-05T01:{minute_e:02d}:{sec_e_min:02d}.{sec_e:03d}Z {req_id} [worker stdout] [2026-06-05 01:{minute_e:02d}:{sec_e_min:02d},{sec_e:03d}] INFO: 重试成功 | shard=shard-{shard} | batch={i} | ms=300"))

    # Checkpoints
    cp_ts_base = 1770000200000
    for cp, processed in [(1, 256), (2, 768), (3, 1280), (4, 1792)]:
        ts_cp = cp_ts_base + cp * 30000
        events.append((str(ts_cp), f"[2026-06-05 01:04:{10+cp:02d},000] INFO: 心跳检查 (checkpoint {cp}/4) processed={processed} failed=0 throughput=42.7 batch/s"))
        events.append((str(ts_cp), f"[INFO] 2026-06-05T01:04:{10+cp:02d}.000Z {req_id} 心跳检查 (checkpoint {cp}/4) processed={processed} failed=0 throughput=42.7 batch/s"))

    # End
    end_ts = 1770000400000
    events.append((str(end_ts), f"[2026-06-05 01:06:40,000] INFO: 批处理完成 summary=batches={total} rows={total*5000} failed=0 elapsed=335000ms"))
    events.append((str(end_ts), f"[INFO] 2026-06-05T01:06:40.000Z {req_id} 批处理完成 summary=batches={total} rows={total*5000} failed=0 elapsed=335000ms"))
    events.append((str(end_ts+500), f"END RequestId: {req_id}"))
    events.append((str(end_ts+500), f"REPORT RequestId: {req_id} Duration: 335312.50 ms Billed Duration: 336000 ms Memory Size: 1024 MB Max Memory Used: 612 MB Init Duration: 1320.41 ms"))

    out = []
    out.append(sep)
    out.append(row("timestamp", "message"))
    out.append(sep)
    for ts, msg in events:
        out.append(row(ts, msg))
    out.append("")
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(out))
    print(f"Wrote {path} ({len(events)} events)")


# ----------------------------------------------------------------------
# case_047: CSV format - Generic document parsing/indexing batch
# ----------------------------------------------------------------------

def _format_clock(anchor_min, ms_offset):
    """Format 01:MM:SS,mmm from anchor minute + ms offset, properly handling carries."""
    minute = anchor_min + ms_offset // 60000
    sec_ms = ms_offset % 60000
    sec = sec_ms // 1000
    ms = sec_ms % 1000
    minute += sec // 60
    sec = sec % 60
    return minute, sec, ms


def emit_case_047_csv(path):
    """AWS Lambda function execution log (CSV format) - generic document parsing/indexing batch."""
    req_id = "b2c3d4e5-f6a7-8901-bcde-f23456789012"
    out = []
    out.append("timestamp,message")

    base_ts = 1775437264228

    def emit(ts, msg):
        safe = msg.replace('"', '""')
        out.append(f"{ts},\"{safe}\n\"")

    # START
    emit(base_ts, f"START RequestId: {req_id} Version: $LATEST")
    emit(base_ts + 10, f"[2026-04-06 01:01:04,238] INFO: 下载 S3 -> 本地: s3://sample-batch-processor/input/docs-manifest-2026-04-06.json  ->  /tmp/docs-manifest-2026-04-06.json")
    emit(base_ts + 10, f"[INFO] 2026-04-06T01:01:04.238Z {req_id} 下载 S3 -> 本地: s3://sample-batch-processor/input/docs-manifest-2026-04-06.json  ->  /tmp/docs-manifest-2026-04-06.json")
    emit(base_ts + 30, f"[2026-04-06 01:01:04,258] INFO: 加载清单 200 个文档来源，类别: [manual, report, spec, contract, faq]")
    emit(base_ts + 30, f"[INFO] 2026-04-06T01:01:04.258Z {req_id} 加载清单 200 个文档来源，类别: [manual, report, spec, contract, faq]")

    # Worker start command
    cmd = "python doc_indexer.py --input /tmp/docs-manifest-2026-04-06.json --output-index sample-search-prod --parser auto --max-concurrency 6 --request-timeout 60 --chunk-size 1024 --chunk-overlap 128 --log-level INFO --db-host db-primary.example.local --db-port 3306 --db-name sample_app --db-user app --db-pass REDACTED --on-conflict upsert"
    emit(base_ts + 50, f"[2026-04-06 01:01:04,278] INFO: 启动批处理进程：{cmd}")
    emit(base_ts + 50, f"[INFO] 2026-04-06T01:01:04.278Z {req_id} 启动批处理进程：{cmd}")

    # Worker emits progress + per-doc processing
    docs = [
        ("manual",   "doc-0001",  12, 148),
        ("manual",   "doc-0002",  18, 226),
        ("report",   "doc-0003",   8,  92),
        ("spec",     "doc-0004",  24, 312),
        ("contract", "doc-0005",  16, 184),
        ("faq",      "doc-0006",   4,  48),
    ]
    for shard in range(4):
        anchor_min = 6 + shard * 8
        for i in range(50):  # 200 total docs
            idx = shard * 50 + i
            cat, doc_id, pages, chunks = docs[idx % len(docs)]
            ms_a = i * 400
            m_a, s_a, ss_a = _format_clock(anchor_min, ms_a)
            ts = base_ts + 2000 + (shard * 30000) + ms_a
            emit(ts, f"[2026-04-06 01:01:{m_a:02d}:{s_a:02d},{ss_a:03d}] INFO: [worker stdout] [2026-04-06 01:01:{m_a:02d}:{s_a:02d},{ss_a:03d}] INFO: [进度] {idx+1}/200 | shard=shard-{shard} | category={cat} | doc={doc_id}.pdf")
            emit(ts, f"[INFO] 2026-04-06T01:01:{m_a:02d}:{s_a:02d}.{ss_a:03d}Z {req_id} [worker stdout] [2026-04-06 01:01:{m_a:02d}:{s_a:02d},{ss_a:03d}] INFO: [进度] {idx+1}/200 | shard=shard-{shard} | category={cat} | doc={doc_id}.pdf")

            ms_b = ms_a + 80
            m_b, s_b, ss_b = _format_clock(anchor_min, ms_b)
            ts2 = ts + 80
            emit(ts2, f"[2026-04-06 01:01:{m_b:02d}:{s_b:02d},{ss_b:03d}] INFO: [worker stdout] [2026-04-06 01:01:{m_b:02d}:{s_b:02d},{ss_b:03d}] INFO: 解析中 | doc={doc_id}.pdf | pages={pages} | parser=pdfplumber | size_bytes=524288")
            emit(ts2, f"[INFO] 2026-04-06T01:01:{m_b:02d}:{s_b:02d}.{ss_b:03d}Z {req_id} [worker stdout] [2026-04-06 01:01:{m_b:02d}:{s_b:02d},{ss_b:03d}] INFO: 解析中 | doc={doc_id}.pdf | pages={pages} | parser=pdfplumber | size_bytes=524288")

            ms_c = ms_a + 220
            m_c, s_c, ss_c = _format_clock(anchor_min, ms_c)
            ts3 = ts + 220
            emit(ts3, f"[2026-04-06 01:01:{m_c:02d}:{s_c:02d},{ss_c:03d}] INFO: [worker stdout] [2026-04-06 01:01:{m_c:02d}:{s_c:02d},{ss_c:03d}] INFO: 索引中 | doc={doc_id}.pdf | chunks={chunks} | embedding_model=text-emb-3s | index=sample-search-prod")
            emit(ts3, f"[INFO] 2026-04-06T01:01:{m_c:02d}:{s_c:02d}.{ss_c:03d}Z {req_id} [worker stdout] [2026-04-06 01:01:{m_c:02d}:{s_c:02d},{ss_c:03d}] INFO: 索引中 | doc={doc_id}.pdf | chunks={chunks} | embedding_model=text-emb-3s | index=sample-search-prod")

            ms_d = ms_a + 280
            m_d, s_d, ss_d = _format_clock(anchor_min, ms_d)
            ts4 = ts + 280
            emit(ts4, f"[2026-04-06 01:01:{m_d:02d}:{s_d:02d},{ss_d:03d}] INFO: [worker stdout] [2026-04-06 01:01:{m_d:02d}:{s_d:02d},{ss_d:03d}] INFO: 完成 | doc={doc_id}.pdf | ms=280 | chunks_indexed={chunks}")
            emit(ts4, f"[INFO] 2026-04-06T01:01:{m_d:02d}:{s_d:02d}.{ss_d:03d}Z {req_id} [worker stdout] [2026-04-06 01:01:{m_d:02d}:{s_d:02d},{ss_d:03d}] INFO: 完成 | doc={doc_id}.pdf | ms=280 | chunks_indexed={chunks}")
            if i == 25 and shard == 1:
                ms_e = ms_a + 320
                m_e, s_e, ss_e = _format_clock(anchor_min, ms_e)
                ts5 = ts + 320
                emit(ts5, f"[2026-04-06 01:01:{m_e:02d}:{s_e:02d},{ss_e:03d}] INFO: [worker stdout] [2026-04-06 01:01:{m_e:02d}:{s_e:02d},{ss_e:03d}] INFO: WARN  | doc={doc_id}.pdf | reason=ocr_low_conf | detail=page=3 confidence=0.42")
                emit(ts5, f"[INFO] 2026-04-06T01:01:{m_e:02d}:{s_e:02d}.{ss_e:03d}Z {req_id} [worker stdout] [2026-04-06 01:01:{m_e:02d}:{s_e:02d},{ss_e:03d}] INFO: WARN  | doc={doc_id}.pdf | reason=ocr_low_conf | detail=page=3 confidence=0.42")
            if i == 40 and shard == 3:
                ms_f = ms_a + 350
                m_f, s_f, ss_f = _format_clock(anchor_min, ms_f)
                ts6 = ts + 350
                emit(ts6, f"[2026-04-06 01:01:{m_f:02d}:{s_f:02d},{ss_f:03d}] INFO: [worker stdout] [2026-04-06 01:01:{m_f:02d}:{s_f:02d},{ss_f:03d}] INFO: ERROR | doc={doc_id}.pdf | reason=index_throttle | detail=429 Too Many Requests will retry 1/3")
                emit(ts6, f"[INFO] 2026-04-06T01:01:{m_f:02d}:{s_f:02d}.{ss_f:03d}Z {req_id} [worker stdout] [2026-04-06 01:01:{m_f:02d}:{s_f:02d},{ss_f:03d}] INFO: ERROR | doc={doc_id}.pdf | reason=index_throttle | detail=429 Too Many Requests will retry 1/3")
                ms_g = ms_a + 700
                m_g, s_g, ss_g = _format_clock(anchor_min, ms_g)
                ts7 = ts + 700
                emit(ts7, f"[2026-04-06 01:01:{m_g:02d}:{s_g:02d},{ss_g:03d}] INFO: [worker stdout] [2026-04-06 01:01:{m_g:02d}:{s_g:02d},{ss_g:03d}] INFO: 重试成功 | doc={doc_id}.pdf | ms=350 | chunks_indexed={chunks}")
                emit(ts7, f"[INFO] 2026-04-06T01:01:{m_g:02d}:{s_g:02d}.{ss_g:03d}Z {req_id} [worker stdout] [2026-04-06 01:01:{m_g:02d}:{s_g:02d},{ss_g:03d}] INFO: 重试成功 | doc={doc_id}.pdf | ms=350 | chunks_indexed={chunks}")

    # Checkpoints
    for cp, processed in [(1, 50), (2, 100), (3, 150), (4, 200)]:
        ts_cp = base_ts + 120000 + cp * 30000
        emit(ts_cp, f"[2026-04-06 01:03:{cp*1:02d},000] INFO: 心跳检查 (checkpoint {cp}/4) processed={processed} failed=0 throughput=15.2 docs/s")
        emit(ts_cp, f"[INFO] 2026-04-06T01:03:{cp*1:02d}.000Z {req_id} 心跳检查 (checkpoint {cp}/4) processed={processed} failed=0 throughput=15.2 docs/s")

    # End
    end_ts = base_ts + 300000
    emit(end_ts, f"[2026-04-06 01:06:04,228] INFO: 批处理完成 summary=docs=200 chunks=2896 failed=0 elapsed=282000ms")
    emit(end_ts, f"[INFO] 2026-04-06T01:06:04.228Z {req_id} 批处理完成 summary=docs=200 chunks=2896 failed=0 elapsed=282000ms")
    emit(end_ts + 500, f"END RequestId: {req_id}")
    emit(end_ts + 500, f"REPORT RequestId: {req_id} Duration: 282312.50 ms Billed Duration: 283000 ms Memory Size: 1024 MB Max Memory Used: 768 MB Init Duration: 1142.83 ms")

    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(out) + "\n")
    print(f"Wrote {path}")


# ----------------------------------------------------------------------
# case_048: CSV format - Generic log analytics batch
# ----------------------------------------------------------------------

def emit_case_048_csv(path):
    """AWS Lambda function execution log (CSV format) - generic log analytics batch (parse logs, compute stats, write aggregated metrics)."""
    req_id = "196a221f-ccf1-4231-9769-f0e4e8b5db42"
    out = []
    out.append("timestamp,message")

    base_ts = 1780621264700

    def emit(ts, msg):
        safe = msg.replace('"', '""')
        out.append(f"{ts},\"{safe}\n\"")

    # START
    emit(base_ts, f"START RequestId: {req_id} Version: $LATEST")
    emit(base_ts + 10, f"[2026-06-05 01:01:04,710] INFO: 下载 S3 -> 本地: s3://sample-batch-processor/input/app-logs-2026-06-04.tar.gz  ->  /tmp/app-logs-2026-06-04.tar.gz")
    emit(base_ts + 10, f"[INFO] 2026-06-05T01:01:04.710Z {req_id} 下载 S3 -> 本地: s3://sample-batch-processor/input/app-logs-2026-06-04.tar.gz  ->  /tmp/app-logs-2026-06-04.tar.gz")
    emit(base_ts + 30, f"[2026-06-05 01:01:04,730] INFO: 解压 -> /tmp/app-logs-2026-06-04/ (files=24, total_size=157286400)")
    emit(base_ts + 30, f"[INFO] 2026-06-05T01:01:04.730Z {req_id} 解压 -> /tmp/app-logs-2026-06-04/ (files=24, total_size=157286400)")

    cmd = "python log_analytics.py --input-dir /tmp/app-logs-2026-06-04/ --output-bucket s3://sample-batch-processor/metrics/2026-06-05/ --rollup-window 60 --max-concurrency 8 --request-timeout 60 --log-level INFO --db-host db-primary.example.local --db-port 3306 --db-name sample_app --db-user app --db-pass REDACTED --on-conflict overwrite"
    emit(base_ts + 50, f"[2026-06-05 01:01:04,750] INFO: 启动批处理进程：{cmd}")
    emit(base_ts + 50, f"[INFO] 2026-06-05T01:01:04.750Z {req_id} 启动批处理进程：{cmd}")

    # Worker emits per-file processing
    services = ["api-gateway", "auth-svc", "billing-svc", "notif-svc", "ingest-svc", "report-svc", "search-svc", "media-svc"]
    for shard in range(4):
        anchor_min = 6 + shard * 8
        for i in range(28):  # 112 windows total
            idx = shard * 28 + i
            svc = services[idx % len(services)]
            ms_a = i * 350
            m_a, s_a, ss_a = _format_clock(anchor_min, ms_a)
            ts = base_ts + 2000 + (shard * 30000) + ms_a
            emit(ts, f"[2026-06-05 01:01:{m_a:02d}:{s_a:02d},{ss_a:03d}] INFO: [worker stdout] [2026-06-05 01:01:{m_a:02d}:{s_a:02d},{ss_a:03d}] INFO: [进度] {idx+1}/112 | shard=shard-{shard} | service={svc} | window=2026-06-04T{(idx % 24):02d}:00:00Z/PT60S")
            emit(ts, f"[INFO] 2026-06-05T01:01:{m_a:02d}:{s_a:02d}.{ss_a:03d}Z {req_id} [worker stdout] [2026-06-05 01:01:{m_a:02d}:{s_a:02d},{ss_a:03d}] INFO: [进度] {idx+1}/112 | shard=shard-{shard} | service={svc} | window=2026-06-04T{(idx % 24):02d}:00:00Z/PT60S")

            ms_b = ms_a + 80
            m_b, s_b, ss_b = _format_clock(anchor_min, ms_b)
            ts2 = ts + 80
            emit(ts2, f"[2026-06-05 01:01:{m_b:02d}:{s_b:02d},{ss_b:03d}] INFO: [worker stdout] [2026-06-05 01:01:{m_b:02d}:{s_b:02d},{ss_b:03d}] INFO: 聚合中 | service={svc} | window=2026-06-04T{(idx % 24):02d}:00:00Z | count={1000+idx*47} | p50_ms={(40+idx*3)%200} | p99_ms={(180+idx*7)%500} | err_rate={((idx*13) % 100) / 1000.0:.3f}")
            emit(ts2, f"[INFO] 2026-06-05T01:01:{m_b:02d}:{s_b:02d}.{ss_b:03d}Z {req_id} [worker stdout] [2026-06-05 01:01:{m_b:02d}:{s_b:02d},{ss_b:03d}] INFO: 聚合中 | service={svc} | window=2026-06-04T{(idx % 24):02d}:00:00Z | count={1000+idx*47} | p50_ms={(40+idx*3)%200} | p99_ms={(180+idx*7)%500} | err_rate={((idx*13) % 100) / 1000.0:.3f}")

            ms_c = ms_a + 200
            m_c, s_c, ss_c = _format_clock(anchor_min, ms_c)
            ts3 = ts + 200
            emit(ts3, f"[2026-06-05 01:01:{m_c:02d}:{s_c:02d},{ss_c:03d}] INFO: [worker stdout] [2026-06-05 01:01:{m_c:02d}:{s_c:02d},{ss_c:03d}] INFO: 写入指标 | service={svc} | window=2026-06-04T{(idx % 24):02d}:00:00Z | out=s3://sample-batch-processor/metrics/2026-06-05/service={svc}/2026-06-04T{(idx % 24):02d}:00:00Z.json | engine=duckdb")
            emit(ts3, f"[INFO] 2026-06-05T01:01:{m_c:02d}:{s_c:02d}.{ss_c:03d}Z {req_id} [worker stdout] [2026-06-05 01:01:{m_c:02d}:{s_c:02d},{ss_c:03d}] INFO: 写入指标 | service={svc} | window=2026-06-04T{(idx % 24):02d}:00:00Z | out=s3://sample-batch-processor/metrics/2026-06-05/service={svc}/2026-06-04T{(idx % 24):02d}:00:00Z.json | engine=duckdb")
            if i == 14 and shard == 1:
                ms_d = ms_a + 250
                m_d, s_d, ss_d = _format_clock(anchor_min, ms_d)
                ts4 = ts + 250
                emit(ts4, f"[2026-06-05 01:01:{m_d:02d}:{s_d:02d},{ss_d:03d}] INFO: [worker stdout] [2026-06-05 01:01:{m_d:02d}:{s_d:02d},{ss_d:03d}] INFO: WARN  | service={svc} | reason=missing_data | detail=log file truncated, using partial window")
                emit(ts4, f"[INFO] 2026-06-05T01:01:{m_d:02d}:{s_d:02d}.{ss_d:03d}Z {req_id} [worker stdout] [2026-06-05 01:01:{m_d:02d}:{s_d:02d},{ss_d:03d}] INFO: WARN  | service={svc} | reason=missing_data | detail=log file truncated, using partial window")
            if i == 20 and shard == 2:
                ms_e = ms_a + 300
                m_e, s_e, ss_e = _format_clock(anchor_min, ms_e)
                ts5 = ts + 300
                emit(ts5, f"[2026-06-05 01:01:{m_e:02d}:{s_e:02d},{ss_e:03d}] INFO: [worker stdout] [2026-06-05 01:01:{m_e:02d}:{s_e:02d},{ss_e:03d}] INFO: ERROR | service={svc} | reason=db_lock | detail=deadlock detected will retry 1/3")
                emit(ts5, f"[INFO] 2026-06-05T01:01:{m_e:02d}:{s_e:02d}.{ss_e:03d}Z {req_id} [worker stdout] [2026-06-05 01:01:{m_e:02d}:{s_e:02d},{ss_e:03d}] INFO: ERROR | service={svc} | reason=db_lock | detail=deadlock detected will retry 1/3")
                ms_f = ms_a + 600
                m_f, s_f, ss_f = _format_clock(anchor_min, ms_f)
                ts6 = ts + 600
                emit(ts6, f"[2026-06-05 01:01:{m_f:02d}:{s_f:02d},{ss_f:03d}] INFO: [worker stdout] [2026-06-05 01:01:{m_f:02d}:{s_f:02d},{ss_f:03d}] INFO: 重试成功 | service={svc} | ms=300 | out=s3://sample-batch-processor/metrics/2026-06-05/service={svc}/2026-06-04T{(idx % 24):02d}:00:00Z.json")
                emit(ts6, f"[INFO] 2026-06-05T01:01:{m_f:02d}:{s_f:02d}.{ss_f:03d}Z {req_id} [worker stdout] [2026-06-05 01:01:{m_f:02d}:{s_f:02d},{ss_f:03d}] INFO: 重试成功 | service={svc} | ms=300 | out=s3://sample-batch-processor/metrics/2026-06-05/service={svc}/2026-06-04T{(idx % 24):02d}:00:00Z.json")

    # Checkpoints
    for cp, processed in [(1, 28), (2, 56), (3, 84), (4, 112)]:
        ts_cp = base_ts + 200000 + cp * 30000
        emit(ts_cp, f"[2026-06-05 01:04:{20+cp:02d},000] INFO: 心跳检查 (checkpoint {cp}/4) processed={processed} failed=0 throughput=8.4 windows/s")
        emit(ts_cp, f"[INFO] 2026-06-05T01:04:{20+cp:02d}.000Z {req_id} 心跳检查 (checkpoint {cp}/4) processed={processed} failed=0 throughput=8.4 windows/s")

    # End
    end_ts = base_ts + 380000
    emit(end_ts, f"[2026-06-05 01:07:24,700] INFO: 批处理完成 summary=windows=112 services=8 failed=0 elapsed=360000ms")
    emit(end_ts, f"[INFO] 2026-06-05T01:07:24.700Z {req_id} 批处理完成 summary=windows=112 services=8 failed=0 elapsed=360000ms")
    emit(end_ts + 500, f"END RequestId: {req_id}")
    emit(end_ts + 500, f"REPORT RequestId: {req_id} Duration: 360142.00 ms Billed Duration: 361000 ms Memory Size: 2048 MB Max Memory Used: 1408 MB Init Duration: 1184.55 ms")

    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(out) + "\n")
    print(f"Wrote {path}")


if __name__ == "__main__":
    import sys
    base = sys.argv[1] if len(sys.argv) > 1 else r"samples\cloud_log_plugin"
    emit_case_046_table(os.path.join(base, "case_046_aws_logs_insights_table.log"), width=1408)
    emit_case_047_csv(os.path.join(base, "case_047_aws_logs_insights_table.log"))
    emit_case_048_csv(os.path.join(base, "case_048_aws_logs_insights_table.log"))
