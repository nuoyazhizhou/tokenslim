# syntax=docker/dockerfile:1
# ── Stage 1: 编译阶段 ──────────────────────────────────────────────────────
FROM rust:slim-bookworm AS builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 先复制依赖清单，利用 Docker 缓存加速
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# 复制源码与配置
COPY src/ src/
COPY config/ config/
COPY webui/ webui/
COPY resources/ resources/
COPY plugins_registry.md ./

# 编译 release（CLI + Server + 独立工具）
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --bins && \
    cp target/release/tokenslim /app/tokenslim-bin && \
    cp target/release/tokenslim-server /app/tokenslim-server-bin

# ── Stage 2: 最小运行时镜像 ────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    useradd -m -u 1000 tokenslim

WORKDIR /app

# 复制编译产物
COPY --from=builder /app/tokenslim-bin ./tokenslim
COPY --from=builder /app/tokenslim-server-bin ./tokenslim-server

# 复制运行时配置（plugins/frameworks/languages）
COPY --from=builder /app/config/ ./config/

# 设置可执行权限
RUN chmod +x ./tokenslim ./tokenslim-server

USER tokenslim

# Server 默认端口
EXPOSE 10086

# 默认启动 server 模式
CMD ["./tokenslim-server", "--host", "0.0.0.0"]
