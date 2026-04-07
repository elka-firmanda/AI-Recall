# AI Recall - Homelab Deployment Guide

## Overview

AI Recall requires:
- **AI Recall binary** (Rust application)
- **Qdrant** (vector database)
- **OpenAI/OpenRouter API key** (for embeddings)

## Option 1: Docker Compose (Recommended)

The easiest deployment method for homelabs.

### Directory Structure
```
~/ai-recall/
├── docker-compose.yml
├── config.yaml
├── .env
└── data/
```

### 1. Create docker-compose.yml

```yaml
version: '3.8'

services:
  qdrant:
    image: qdrant/qdrant:latest
    container_name: ai-recall-qdrant
    restart: unless-stopped
    ports:
      - "6333:6333"  # HTTP API
      - "6334:6334"  # gRPC API
    volumes:
      - ./data/qdrant:/qdrant/storage
    environment:
      - QDRANT__SERVICE__HTTP_PORT=6333
      - QDRANT__SERVICE__GRPC_PORT=6334
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:6333/healthz"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s

  ai-recall:
    # Build from source (recommended for now)
    build:
      context: ./src
      dockerfile: Dockerfile
    # Or use pre-built image when available:
    # image: ghcr.io/yourusername/ai-recall:latest
    container_name: ai-recall
    restart: unless-stopped
    ports:
      - "8080:8080"
    volumes:
      - ./data:/data
      - ./config.yaml:/app/config.yaml:ro
    environment:
      - AI_RECALL_SERVER_HOST=0.0.0.0
      - AI_RECALL_SERVER_PORT=8080
      - AI_RECALL_STORAGE_DATA_DIR=/data
      - AI_RECALL_QDRANT_URL=http://qdrant:6334
      - AI_RECALL_EMBEDDINGS_API_KEY=${OPENAI_API_KEY}
      - AI_RECALL_EMBEDDINGS_PROVIDER=${EMBEDDING_PROVIDER:-openai}
      - AI_RECALL_EMBEDDINGS_MODEL=${EMBEDDING_MODEL:-text-embedding-3-small}
      - RUST_LOG=info
    depends_on:
      qdrant:
        condition: service_healthy
    command: ["serve"]
    healthcheck:
      test: ["CMD", "wget", "--quiet", "--tries=1", "--spider", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  # Optional: Watchtower for automatic updates
  watchtower:
    image: containrrr/watchtower:latest
    container_name: ai-recall-watchtower
    restart: unless-stopped
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    environment:
      - WATCHTOWER_CLEANUP=true
      - WATCHTOWER_POLL_INTERVAL=86400  # Check once per day
    command: --interval 86400 qdrant ai-recall
```

### 2. Create config.yaml

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  # Auth token will be auto-generated on first run if not set
  # auth_token: "your-secure-token-here"
  log_level: "info"

storage:
  data_dir: "/data"
  max_file_size_mb: 10

qdrant:
  url: "http://qdrant:6334"
  api_key: null
  collection_name: "memories"
  graph_collection_name: "memory_graph"
  vector_size: 1536
  distance: "Cosine"

embeddings:
  provider: "openai"  # or "openrouter"
  api_key: ""  # Set via environment variable
  model: "text-embedding-3-small"
  dimension: 1536
  batch_size: 100
  timeout_secs: 30
  base_url: null  # Set for OpenRouter: "https://openrouter.ai/api/v1"

memory_defaults:
  default_confidence: 0.8
  min_confidence_threshold: 0.5
  auto_link: true
  auto_extract_wikilinks: true
```

### 3. Create .env file

```bash
# Required: OpenAI API Key
OPENAI_API_KEY=sk-your-key-here

# Optional: Embedding provider settings
EMBEDDING_PROVIDER=openai
EMBEDDING_MODEL=text-embedding-3-small

# Optional: OpenRouter alternative (uncomment to use)
# OPENAI_API_KEY=sk-or-v1-your-openrouter-key
# EMBEDDING_PROVIDER=openrouter
# EMBEDDING_MODEL=openai/text-embedding-3-small
```

### 4. Deploy

```bash
# Create directories
mkdir -p ~/ai-recall/data

# Start services
cd ~/ai-recall
docker-compose up -d

# Check logs
docker-compose logs -f ai-recall

# Generate auth token (if not set)
docker-compose exec ai-recall ai-recall token
```

---

## Option 2: Binary Deployment

For running directly on the host without Docker.

### 1. Build the Binary

On your development machine:

```bash
cd /Users/elka/Project/global-memory/ai-recall
cargo build --release

# Copy binary to server
scp target/release/ai-recall user@homelab:/usr/local/bin/
ssh user@homelab 'chmod +x /usr/local/bin/ai-recall'
```

Or build on the server:

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Clone and build
git clone <your-repo-url> /opt/ai-recall
cd /opt/ai-recall
cargo build --release

# Install binary
cp target/release/ai-recall /usr/local/bin/
```

### 2. Install Qdrant

```bash
# Download and install Qdrant
mkdir -p /opt/qdrant
cd /opt/qdrant

# Get latest release URL from https://github.com/qdrant/qdrant/releases
wget https://github.com/qdrant/qdrant/releases/download/v1.12.0/qdrant-x86_64-unknown-linux-gnu.tar.gz
tar -xzf qdrant-*.tar.gz

# Create systemd service
sudo tee /etc/systemd/system/qdrant.service > /dev/null <<EOF
[Unit]
Description=Qdrant Vector Database
After=network.target

[Service]
Type=simple
User=qdrant
Group=qdrant
WorkingDirectory=/opt/qdrant
ExecStart=/opt/qdrant/qdrant
Restart=always
RestartSec=5
Environment=QDRANT__SERVICE__HTTP_PORT=6333
Environment=QDRANT__SERVICE__GRPC_PORT=6334

[Install]
WantedBy=multi-user.target
EOF

# Create user and directories
sudo useradd -r -s /bin/false qdrant
sudo mkdir -p /var/lib/qdrant
sudo chown qdrant:qdrant /var/lib/qdrant

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable qdrant
sudo systemctl start qdrant
```

### 3. Create AI Recall systemd Service

```bash
# Create directories
sudo mkdir -p /opt/ai-recall/data
sudo mkdir -p /etc/ai-recall

# Create config
sudo tee /etc/ai-recall/config.yaml > /dev/null <<EOF
server:
  host: "127.0.0.1"  # Bind to localhost, use reverse proxy
  port: 8080
  log_level: "info"

storage:
  data_dir: "/opt/ai-recall/data"
  max_file_size_mb: 10

qdrant:
  url: "http://localhost:6334"
  collection_name: "memories"
  graph_collection_name: "memory_graph"
  vector_size: 1536
  distance: "Cosine"

embeddings:
  provider: "openai"
  api_key: ""  # Will be set via environment
  model: "text-embedding-3-small"
  dimension: 1536
  batch_size: 100
  timeout_secs: 30

memory_defaults:
  default_confidence: 0.8
  min_confidence_threshold: 0.5
  auto_link: true
  auto_extract_wikilinks: true
EOF

# Create environment file
sudo tee /etc/ai-recall/environment > /dev/null <<EOF
AI_RECALL_EMBEDDINGS_API_KEY=sk-your-openai-key-here
AI_RECALL_SERVER_AUTH_TOKEN=arec_$(openssl rand -base64 32 | tr -d '=+/')
RUST_LOG=info
EOF
sudo chmod 600 /etc/ai-recall/environment

# Create systemd service
sudo tee /etc/systemd/system/ai-recall.service > /dev/null <<EOF
[Unit]
Description=AI Recall Memory Server
After=network.target qdrant.service
Wants=qdrant.service

[Service]
Type=simple
User=ai-recall
Group=ai-recall
WorkingDirectory=/opt/ai-recall
ExecStart=/usr/local/bin/ai-recall serve --config /etc/ai-recall/config.yaml
Restart=always
RestartSec=5
EnvironmentFile=/etc/ai-recall/environment

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/ai-recall/data

[Install]
WantedBy=multi-user.target
EOF

# Create user
sudo useradd -r -s /bin/false -d /opt/ai-recall ai-recall
sudo chown -R ai-recall:ai-recall /opt/ai-recall

# Initialize data directory
sudo -u ai-recall /usr/local/bin/ai-recall init --config /etc/ai-recall/config.yaml

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable ai-recall
sudo systemctl start ai-recall
```

---

## Option 3: Nginx Reverse Proxy (SSL)

Add HTTPS with Let's Encrypt:

```bash
# Install nginx and certbot
sudo apt update
sudo apt install -y nginx certbot python3-certbot-nginx

# Create nginx config
sudo tee /etc/nginx/sites-available/ai-recall > /dev/null <<EOF
server {
    listen 80;
    server_name recall.yourdomain.com;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_cache_bypass \$http_upgrade;
        
        # Timeouts
        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
    }
}
EOF

# Enable site
sudo ln -s /etc/nginx/sites-available/ai-recall /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx

# Get SSL certificate
sudo certbot --nginx -d recall.yourdomain.com --non-interactive --agree-tos --email your@email.com
```

---

## Option 4: Cloudflare Tunnel (No Port Opening)

If you don't want to open ports or deal with SSL:

```bash
# Install cloudflared
wget -q https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64.deb
sudo dpkg -i cloudflared-linux-amd64.deb

# Authenticate
cloudflared tunnel login

# Create tunnel
cloudflared tunnel create ai-recall

# Get the tunnel ID
TUNNEL_ID=$(cloudflared tunnel list | grep ai-recall | awk '{print $1}')

# Create config
sudo mkdir -p /etc/cloudflared
sudo tee /etc/cloudflared/config.yml > /dev/null <<EOF
tunnel: ${TUNNEL_ID}
credentials-file: /root/.cloudflared/${TUNNEL_ID}.json

ingress:
  - hostname: recall.yourdomain.com
    service: http://localhost:8080
  - service: http_status:404
EOF

# Install as service
sudo cloudflared service install
sudo systemctl enable cloudflared
sudo systemctl start cloudflared
```

---

## Client Configuration

### For Claude Desktop / MCP Clients

Create `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or equivalent:

```json
{
  "mcpServers": {
    "ai-recall": {
      "command": "/usr/local/bin/ai-recall",
      "args": ["mcp"],
      "env": {
        "AI_RECALL_SERVER_AUTH_TOKEN": "your-token-here",
        "AI_RECALL_STORAGE_DATA_DIR": "/Users/elka/.ai-recall/data",
        "AI_RECALL_QDRANT_URL": "http://localhost:6334",
        "AI_RECALL_EMBEDDINGS_API_KEY": "sk-your-key-here"
      }
    }
  }
}
```

### For Remote HTTP Access

```bash
# Set environment variables for clients
export AI_RECALL_URL=https://recall.yourdomain.com
export AI_RECALL_TOKEN=your-token-here

# Test connection
curl -H "Authorization: Bearer $AI_RECALL_TOKEN" \
  $AI_RECALL_URL/health
```

---

## Maintenance

### Backup

```bash
#!/bin/bash
# backup-ai-recall.sh

BACKUP_DIR="/backups/ai-recall"
DATE=$(date +%Y%m%d_%H%M%S)

# Create backup
mkdir -p ${BACKUP_DIR}/${DATE}
rsync -av /opt/ai-recall/data/ ${BACKUP_DIR}/${DATE}/

# Keep only last 7 days
find ${BACKUP_DIR} -type d -mtime +7 -exec rm -rf {} + 2>/dev/null

echo "Backup completed: ${BACKUP_DIR}/${DATE}"
```

Add to crontab:
```bash
0 2 * * * /opt/ai-recall/backup-ai-recall.sh >> /var/log/ai-recall-backup.log 2>&1
```

### Updates

```bash
# Docker Compose
cd ~/ai-recall
docker-compose pull
docker-compose up -d

# Binary
cd /opt/ai-recall
git pull
cargo build --release
sudo systemctl restart ai-recall
```

### Monitoring

```bash
# Check status
docker-compose ps  # Docker
sudo systemctl status ai-recall  # Binary

# View logs
docker-compose logs -f ai-recall  # Docker
sudo journalctl -u ai-recall -f  # Binary

# Health check
curl http://localhost:8080/health
```

---

## Troubleshooting

| Issue | Solution |
|-------|----------|
| Qdrant connection failed | Check Qdrant is running: `curl http://localhost:6333/healthz` |
| Embedding errors | Verify API key with: `echo $AI_RECALL_EMBEDDINGS_API_KEY` |
| Auth failures | Check token: `ai-recall token` or `/etc/ai-recall/environment` |
| Permission denied | Fix ownership: `sudo chown -R ai-recall:ai-recall /opt/ai-recall` |
| Port already in use | Find process: `sudo lsof -i :8080` |

---

## Security Checklist

- [ ] Auth token generated and stored securely
- [ ] Environment file permissions set to 600
- [ ] Service running as non-root user
- [ ] Firewall configured (only 443/80 exposed if using reverse proxy)
- [ ] SSL certificate installed (Let's Encrypt or Cloudflare)
- [ ] Regular backups configured
- [ ] API key rotated periodically
