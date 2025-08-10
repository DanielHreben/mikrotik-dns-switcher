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

### Environment Variables

Edit `.env` with your setup:

```bash
MIKROTIK_HOST=192.168.88.1        # Your router's IP
MIKROTIK_USERNAME=admin           # Router username  
MIKROTIK_PASSWORD=your-password   # Router password
CUSTOM_DNS=192.168.1.100         # Your Pi-hole IP (or 8.8.8.8)
```

### MikroTik Setup

Enable API on your router:

```
/ip service enable api
/ip service set api port=8728
```

## Development

```bash
# Clone and install
git clone https://github.com/DanielHreben/mikrotik-dns-switcher.git
cd mikrotik-dns-switcher
yarn install

# Configure and run
cp .env.example .env
nano .env
yarn start
```

### API Testing

You can manage specific devices by adding the `X-Real-IP` header:

```bash
curl -H "X-Real-IP: 192.168.1.100" http://localhost:3000/api/dns
```

## API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/dns` | Check DNS status |
| `PUT` | `/api/dns` | Switch to custom DNS |
| `DELETE` | `/api/dns` | Reset to default DNS |

## Troubleshooting

- **Can't connect to MikroTik**: Check IP/credentials, ensure API is enabled
- **DNS changes not sticking**: Try disconnecting/reconnecting WiFi, some devices cache DNS
- **Check logs**: `docker-compose logs -f`

## About

Built this for my own home network and figured others might find it useful. Just a hobby project - feel free to fork it or send PRs if you find bugs!

Apache License 2.0 - do whatever you want with it.
