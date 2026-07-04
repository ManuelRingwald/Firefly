# Multi-stage build for firefly-server (M6.4).
#
# Stage 1: Build the binary and all dependencies.
FROM rust:1.87-bookworm AS builder

WORKDIR /build

# Copy the entire workspace.
COPY . .

# Build in release mode (optimised for production).
RUN cargo build --release -p firefly-server

# Stage 2: Runtime image with only the binary and static assets.
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from the builder stage.
COPY --from=builder /build/target/release/firefly-server /app/

# Copy static assets (HTML, GeoJSON, etc.).
COPY --from=builder /build/crates/firefly-server/static /app/static/

# Health check: server listens on port 8080 and has a /health endpoint.
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

EXPOSE 8080

# The server reads its config from the environment (12-Factor, NFR-CLOUD-001):
# sources via FIREFLY_SOURCES (ADR 0023) or FIREFLY_OPENSKY_*/_FLARM_*/_RADAR_*;
# with no source configured it serves an empty sky + CAT065 heartbeat (ADR 0030).

ENTRYPOINT ["/app/firefly-server"]
