FROM rust:1-alpine AS build
RUN apk add --no-cache build-base
WORKDIR /src

# Build the dependencies first so source changes don't redo them.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && touch src/lib.rs \
  && cargo build --release \
  && rm -rf src target/release/deps/mikrotik_dns_switcher* target/release/.fingerprint/mikrotik-dns-switcher*

COPY src ./src
COPY public ./public
RUN cargo build --release

# A static binary with the front end embedded: nothing else to ship.
FROM scratch
COPY --from=build /src/target/release/mikrotik-dns-switcher /mikrotik-dns-switcher
USER 10001:10001
ENV PORT=3000
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/mikrotik-dns-switcher", "healthcheck"]
ENTRYPOINT ["/mikrotik-dns-switcher"]
