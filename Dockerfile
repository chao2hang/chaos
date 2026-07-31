# Multi-stage build for chaos (control plane + vendored dae data plane)
#
# Build:
#   docker build -t chaos .
#
# Run (requires privileges for eBPF):
#   docker run -d --privileged --network=host --pid=host \
#     -v /sys:/sys -v chaos-data:/var/lib/chaos \
#     -e CHAOS_AUTOSTART_DAE=1 \
#     --name chaos chaos

# ---------------------------------------------------------------------------
# Stage 1: Build SvelteKit web assets
# ---------------------------------------------------------------------------
FROM node:20-slim AS web

RUN corepack enable && corepack prepare pnpm@10 --activate

WORKDIR /app
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/web/package.json apps/web/
RUN pnpm install --frozen-lockfile

COPY apps/web/ apps/web/
COPY locales/ locales/
RUN pnpm --dir apps/web build

# ---------------------------------------------------------------------------
# Stage 2: Build chaos-prober (real proxy latency tester)
# ---------------------------------------------------------------------------
FROM golang:1.26-slim AS prober

WORKDIR /src/tools/chaos-prober
COPY tools/chaos-prober/go.mod tools/chaos-prober/go.sum ./
RUN go mod download

COPY tools/chaos-prober/ ./
RUN CGO_ENABLED=0 GOOS=linux go build -trimpath -ldflags="-s -w" -o /out/chaos-prober .

# ---------------------------------------------------------------------------
# Stage 3: Build Rust API
# ---------------------------------------------------------------------------
FROM rust:1.83-slim AS api

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/

# Build release binary
RUN cargo build --release -p chaos-api && \
    cp target/release/chaos-api /usr/local/bin/chaos-api

# ---------------------------------------------------------------------------
# Stage 4: Runtime image
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create chaos user and directories
RUN useradd -r -s /bin/false chaos && \
    mkdir -p /var/lib/chaos /usr/lib/chaos/bin /usr/share/chaos/web && \
    chown -R chaos:chaos /var/lib/chaos

# Copy API binary
COPY --from=api /usr/local/bin/chaos-api /usr/lib/chaos/bin/chaos-api

# Copy real proxy latency tester
COPY --from=prober /out/chaos-prober /usr/lib/chaos/bin/chaos-prober

# Copy web assets
COPY --from=web /app/apps/web/build /usr/share/chaos/web

# Copy dae binary if present (fetched via scripts/fetch-dae.sh before build)
# The build will succeed without it; runtime reports dae_binary_missing.
COPY third_party/dae/current/dae* /usr/lib/chaos/bin/dae

# Copy locales
COPY locales/ /usr/share/chaos/locales/

# Environment defaults
ENV CHAOS_BIND=0.0.0.0:2030 \
    CHAOS_DATABASE_URL=sqlite:/var/lib/chaos/chaos.db?mode=rwc \
    CHAOS_JWT_SECRET=/var/lib/chaos/jwt.secret \
    CHAOS_DAE_BIN=/usr/lib/chaos/bin/dae \
    CHAOS_PROBER_BIN=/usr/lib/chaos/bin/chaos-prober \
    CHAOS_DAE_WORK_DIR=/var/lib/chaos/dae \
    CHAOS_WEB_DIR=/usr/share/chaos/web

EXPOSE 2030

VOLUME ["/var/lib/chaos"]

# Run as root by default (dae needs CAP_NET_ADMIN + CAP_BPF for eBPF)
# For production, consider running with specific capabilities instead of --privileged.
ENTRYPOINT ["/usr/lib/chaos/bin/chaos-api"]
