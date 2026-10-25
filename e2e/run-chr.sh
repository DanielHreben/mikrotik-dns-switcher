#!/usr/bin/env bash
# End-to-end test against a real RouterOS: boots a MikroTik CHR in QEMU and runs
# tests/chr.rs against it. Needs qemu-system-x86_64, curl and unzip. Uses KVM when
# available; without it QEMU emulates the CPU, which still works but boots slowly.
set -euo pipefail

VERSION="${CHR_VERSION:-7.24.5}"
WORK="${CHR_WORKDIR:-${TMPDIR:-/tmp}/mikrotik-chr}"
API_PORT="${CHR_API_PORT:-18728}"
TLS_PORT="${CHR_TLS_PORT:-18729}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$WORK"
cd "$WORK"

if [ ! -f "chr-$VERSION.img" ]; then
  curl -fsSL -O "https://download.mikrotik.com/routeros/$VERSION/chr-$VERSION.img.zip"
  unzip -oq "chr-$VERSION.img.zip"
fi

# A fresh disk every run: the test provisions the router from scratch.
cp "chr-$VERSION.img" disk.img

accel=tcg,thread=multi
[ -w /dev/kvm ] && accel=kvm

qemu-system-x86_64 -accel "$accel" -cpu max -smp 2 -m 512 \
  -display none -serial "file:$WORK/serial.log" \
  -drive file=disk.img,format=raw,if=virtio \
  -nic "user,model=virtio-net-pci,hostfwd=tcp:127.0.0.1:$API_PORT-:8728,hostfwd=tcp:127.0.0.1:$TLS_PORT-:8729" &
QEMU=$!
trap 'kill $QEMU 2>/dev/null || true' EXIT
sleep 2
kill -0 $QEMU 2>/dev/null || { echo "QEMU did not start" >&2; exit 1; }

cd "$ROOT"
CHR_API_PORT=$API_PORT CHR_TLS_PORT=$TLS_PORT \
  cargo test --test chr -- --ignored --nocapture
