FROM lukemathwalker/cargo-chef:latest-rust-1.92-alpine AS chef
WORKDIR /app

# Planner stage: analyze dependencies and create recipe
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Builder stage: build dependencies then the application
FROM chef AS builder

# Build argument for feature selection (e.g., vec, hybrid, fullhash)
ARG FEATURES=""

# First, build dependencies (this layer is cached if Cargo.toml/Cargo.lock don't change)
COPY --from=planner /app/recipe.json recipe.json
RUN if [ -n "$FEATURES" ]; then \
        cargo chef cook --release --features "$FEATURES" --recipe-path recipe.json; \
    else \
        cargo chef cook --release --recipe-path recipe.json; \
    fi

# Copy source and build the application
COPY . .
RUN if [ -n "$FEATURES" ]; then \
        cargo build --release --bin server --features "$FEATURES"; \
    else \
        cargo build --release --bin server; \
    fi

# Runtime stage: minimal Alpine image with just the binary
FROM alpine:3.21 AS runtime

# Install runtime dependencies (ca-certificates for HTTPS in reqwest)
RUN apk add --no-cache ca-certificates

WORKDIR /app

# Copy the built binary
COPY --from=builder /app/target/release/server /usr/local/bin/server

# Create non-root user for security
RUN adduser -D -g '' appuser
USER appuser

# Default port for Rocket
EXPOSE 8000

ENTRYPOINT ["/usr/local/bin/server"]
