# Multi-stage Dockerfile for MikroTik DNS Switcher
# Stage 1: Build stage
FROM rust:1.80-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
  musl-dev \
  pkgconfig \
  openssl-dev \
  openssl-libs-static

# Set working directory
WORKDIR /app

# Copy all source files
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/

# Build the application
RUN cargo build --release --bin mikrotik-dns-switcher

# Stage 2: Runtime stage
FROM alpine:3.19 AS runtime

# Install runtime dependencies
RUN apk add --no-cache \
  ca-certificates \
  openssl \
  wget

# Create non-root user for security
RUN addgroup -g 1000 appuser && \
  adduser -D -s /bin/sh -u 1000 -G appuser appuser

# Set working directory
WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/mikrotik-dns-switcher /app/

# Copy static files
COPY static/ ./static/

# Change ownership to non-root user
RUN chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Expose port
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD wget --no-verbose --tries=1 --spider http://localhost:3000/api || exit 1

# Run the application
CMD ["./mikrotik-dns-switcher"]
