# ─── Stage 1: Chef Setup ──────────────────────────────────────────────────────
FROM rust:1.75-bookworm AS chef
# We only pay the installation cost once, then cache the layer
RUN cargo install cargo-chef
WORKDIR /app

# ─── Stage 2: Planner ─────────────────────────────────────────────────────────
FROM chef AS planner
COPY . .
# Compute a lock-like recipe file for our project dependencies
RUN cargo chef prepare --recipe-path recipe.json

# ─── Stage 3: Builder ─────────────────────────────────────────────────────────
FROM chef AS builder
# Install system dependencies required for RocksDB and Tonic (gRPC/Protobufs)
RUN apt-get update && apt-get install -y \
    clang llvm libclang-dev pkg-config libssl-dev protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the massive caching layer!
# It drastically speeds up rebuilds if your Cargo.toml hasn't changed.
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
# The binary name should match your package name in Cargo.toml
RUN cargo build --release --bin axionvera-network-node

# ─── Stage 4: Production Runtime ──────────────────────────────────────────────
# We use the distroless C++ image because RocksDB dynamically links to libstdc++
# The :nonroot tag ensures we run as a restricted user from the start
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app

# Copy ONLY the compiled binary from the builder environment
COPY --from=builder --chown=nonroot:nonroot /app/target/release/axionvera-network-node /app/axionvera-network-node

# Expose gRPC (50051) and Prometheus Metrics (9090) ports
EXPOSE 50051 9090

# Set the binary as the entrypoint
ENTRYPOINT ["/app/axionvera-network-node"]