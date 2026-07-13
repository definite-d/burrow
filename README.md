# Burrow

A file-sharing server that exposes local folders over the internet via an embedded [Cloudflare tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/). Point it at a folder, get a public URL.

## Features

- **Cloudflare tunnel integration** -- automatic public URL via `cloudflared`, no port forwarding or DNS config needed
- **Per-share access modes** -- download-only, upload-only, or both
- **Admin GUI** -- web-based dashboard for creating, editing, and deleting shares (htmx + Askama SSR)
- **Archive downloads** -- ZIP export of entire share directories
- **File type and size filters** -- restrict uploads/downloads by MIME type, glob pattern, or size
- **Token-based access** -- each share gets a unique secret URL
- **Expiry** -- shares can auto-expire after a configurable duration
- **Range requests** -- partial content / resume support for large file downloads
- **TOML config** -- declarative share definitions in `burrow.toml`

## Prerequisites

**cloudflared** must be installed and available on your `PATH` if you want tunnel support. Download it from:

<https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/>

## Install

### From releases

Download the latest binary for your platform from the [Releases](https://github.com/definite-d/burrow/releases) page.

### From source

```
cargo install --git https://github.com/definite-d/burrow.git
```

## Usage

```
burrow [OPTIONS]

Options:
  -c, --config <PATH>   Path to config file [default: burrow.toml]
  -d, --dir <PATH>      Directory to serve files from
      --host <HOST>     Server host [default: 127.0.0.1]
  -p, --port <PORT>     Server port [default: 8080]
      --no-tunnel       Disable tunnel (local-only mode)
  -h, --help            Print help
```

### Quick start

```
burrow -d ~/my-files
```

This serves `~/my-files` on `http://127.0.0.1:8080` and opens a Cloudflare tunnel for public access.

### Local-only mode

```
burrow -d ~/my-files --no-tunnel
```

## Configuration

Create a `burrow.toml` in your working directory (or pass `-c` to specify a path):

```toml
[server]
host = "127.0.0.1"
port = 8080

[tunnel]
enabled = true

[admin]
enabled = true
# token = "optional-admin-secret"

# Pre-defined shares
[[shares]]
id = "docs"
path = "./documents"
mode = "download"
# token = "my-secret-token"
# expires = "24h"
# max_size = "100MB"
# allowed_types = ["*.pdf", "*.png", "*.jpg"]
# allow_archive = true

[[shares]]
id = "dropbox"
mode = "upload"
path = "./uploads"
max_size = "50MB"
```

CLI flags override the corresponding config values.

### Share options

| Field | Description |
|---|---|
| `id` | Unique identifier for the share |
| `path` | Local directory to expose |
| `mode` | `download`, `upload`, or `both` (default: `download`) |
| `token` | Optional secret token (auto-generated if omitted) |
| `expires` | Expiry duration: `30s`, `10m`, `24h`, `7d` |
| `max_size` | Max upload size: `10MB`, `500KB`, `1GB` |
| `allowed_types` | Glob patterns: `["*.pdf", "*.png"]` |
| `allow_archive` | Enable ZIP download for this share |

## License

MIT
