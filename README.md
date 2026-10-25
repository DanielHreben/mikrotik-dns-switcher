# MikroTik DNS Switcher

A simple web app for self-hosting enthusiasts who want to selectively enable Pi-hole (or other DNS servers) on a per-device basis through their MikroTik router.

## Why This Exists

Like many self-hosters, I love Pi-hole for blocking ads on smart TVs and IoT devices where browser ad-blockers can't help. But running it network-wide breaks things unexpectedly and causes family complaints.

**The solution**: Make Pi-hole opt-in per device instead of network-wide.

- **Default**: Everyone uses normal DNS (ISP, Google, Cloudflare, whatever)
- **When needed**: Toggle Pi-hole on for specific devices that benefit from it
- **Smart TVs**: Always use Pi-hole (they're ad-spam machines anyway)
- **Your devices**: Switch Pi-hole on/off as needed
- **Quick fixes**: Temporarily disable Pi-hole when troubleshooting

It works by tweaking DHCP lease entries on your MikroTik router via the API, so each device can have its own DNS settings.

## Quick Start

### Docker (Recommended)

```bash
git clone https://github.com/DanielHreben/mikrotik-dns-switcher.git
cd mikrotik-dns-switcher

# Configure environment
cp .env.example .env
nano .env  # Edit with your MikroTik details

# Start
docker-compose up -d
```

Open `http://localhost:3000` or `http://your-server-ip:3000`

### nginx-proxy (Optional)

For nginx-proxy deployments with automatic reverse proxy:

```bash
# Add VIRTUAL_HOST to your .env
echo "VIRTUAL_HOST=dns-switcher.yourdomain.com" >> .env

# Use the nginx-proxy compose file
docker-compose -f docker-compose.nginx-proxy.yml up -d
```

The image is a single static binary (about 2 MB; roughly 2 MB of RAM idle) with the
front end embedded. It runs as a non-root user and needs no other files.

### Environment Variables

Edit `.env` with your setup:

```bash
MIKROTIK_HOST=192.168.88.1        # Your router's IP
MIKROTIK_USERNAME=admin           # Router username  
MIKROTIK_PASSWORD=your-password   # Router password
CUSTOM_DNS=192.168.1.100          # Your Pi-hole IP (or 8.8.8.8)

# Production (behind a reverse proxy):
TRUSTED_PROXY=172.19.0.100        # Trust X-Real-IP only from this proxy
ALLOWED_RANGE=192.168.88.20-192.168.88.99   # Only manage these addresses (the DHCP pool)
MIKROTIK_TLS_CA=/path/router-ca.crt         # Router CA: use API-SSL (8729), verify the router
ALLOWED_HOSTS=dns-switcher.home             # Answer only to this name (comma-separated, no port)
MAX_MANAGED_LEASES=32                       # Most leases the switcher may own at once (default 32)

# Local development only:
ALLOW_TARGET_IP=true              # Trust X-Real-IP from anyone (?ip= in the page)
```

Every enabled device holds a permanent static lease, so the switcher refuses new ones
(`429 LIMIT_REACHED`) once `MAX_MANAGED_LEASES` carry its comment. Without
`ALLOWED_HOSTS` any `Host` header is served; set it so that a web page cannot reach the
service under a name it controls (DNS rebinding). The health check is exempt. Responses
carry `X-Frame-Options: DENY` and a restrictive `Content-Security-Policy`.

`TRUSTED_PROXY` is an address: if it is a Docker address, pin the proxy's IP in its
compose file so no other container can take it.

Which device a request acts for: the `X-Real-IP` header when the request comes
from `TRUSTED_PROXY` (or `ALLOW_TARGET_IP=true`), otherwise the connection's own
address. Without `ALLOWED_RANGE` every address is allowed; with it, requests for
other addresses get `403 ADDRESS_NOT_ALLOWED` before the router is contacted.

### MikroTik Setup

Production: API-SSL with the router's own certificate, reachable only from the
switcher's host, and the plain API off:

```
/ip service set api-ssl disabled=no certificate=<your api-ssl certificate> available-from=<switcher host>/32
/ip service disable api
```

The API-SSL certificate must be an ordinary server certificate, not a CA: create it with
`key-usage=digital-signature,key-encipherment,tls-server` and sign it with your own CA.
Its subject-alt-name must match `MIKROTIK_HOST` (an `IP:` entry when that is an address),
and `MIKROTIK_TLS_CA` is the CA's PEM file.

The router user needs `read,write,api`. RouterOS can't limit that to DHCP, so
keep the account's `address` restricted to the switcher's host.

Local development can use the plain API (`/ip service enable api`, port 8728)
by leaving `MIKROTIK_TLS_CA` unset.

## Development

```bash
git clone https://github.com/DanielHreben/mikrotik-dns-switcher.git
cd mikrotik-dns-switcher

# Configure and run (the service reads its settings from the environment)
cp .env.example .env
nano .env
set -a; . ./.env; set +a
cargo run

# Tests, lints
cargo test
cargo clippy --all-targets

# End to end against a real RouterOS (a MikroTik CHR in QEMU; minutes without KVM)
e2e/run-chr.sh
```

The front end is in `public/` and is compiled into the binary, so changes there
need a rebuild.

### API Testing

With `ALLOW_TARGET_IP=true` (local development only) you can manage specific
devices by adding the `X-Real-IP` header, or `?ip=` in the page:

```bash
curl -H "X-Real-IP: 192.168.1.100" http://localhost:3000/api/dns
```

## API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/dns` | Check DNS status |
| `PUT` | `/api/dns` | Switch to custom DNS |
| `DELETE` | `/api/dns` | Reset to default DNS |
| `GET` | `/health` | Liveness (does not contact the router) |

## Troubleshooting

- **Can't connect to MikroTik**: Check IP/credentials, ensure API is enabled
- **DNS changes not sticking**: Try disconnecting/reconnecting WiFi, some devices cache DNS
- **Check logs**: `docker-compose logs -f`

## About

Built this for my own home network and figured others might find it useful. Just a hobby project - feel free to fork it or send PRs if you find bugs!

Apache License 2.0 - do whatever you want with it.
