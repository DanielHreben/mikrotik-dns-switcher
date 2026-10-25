# Cross-compiled on the build machine's own architecture for each target, so a
# multi-arch image needs no emulation.
ARG TARGETARCH
FROM --platform=$BUILDPLATFORM ghcr.io/rust-cross/rust-musl-cross:x86_64-musl AS build-amd64
FROM --platform=$BUILDPLATFORM ghcr.io/rust-cross/rust-musl-cross:aarch64-musl AS build-arm64

FROM build-${TARGETARCH} AS build
WORKDIR /home/rust/src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY public ./public
RUN cargo build --release --locked \
  && mkdir /home/rust/out \
  && cp target/*-unknown-linux-musl/release/mikrotik-dns-switcher /home/rust/out/

# A static binary with the front end embedded: nothing else to ship.
FROM scratch
COPY --from=build /home/rust/out/mikrotik-dns-switcher /mikrotik-dns-switcher
USER 10001:10001
ENV PORT=3000
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/mikrotik-dns-switcher", "healthcheck"]
ENTRYPOINT ["/mikrotik-dns-switcher"]
