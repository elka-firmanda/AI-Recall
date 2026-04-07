#!/bin/bash
# AI Recall Homelab Deployment Script
# Run this on your homelab server

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== AI Recall Homelab Deployment ===${NC}"
echo ""

# Check if running as root
if [[ $EUID -eq 0 ]]; then
   echo -e "${RED}Error: Do not run this script as root${NC}"
   exit 1
fi

# Configuration
INSTALL_DIR="/opt/ai-recall"
DATA_DIR="$INSTALL_DIR/data"
CONFIG_DIR="/etc/ai-recall"
USER="ai-recall"
QDRANT_VERSION="1.12.0"

# Detect architecture
ARCH=$(uname -m)
if [[ "$ARCH" == "x86_64" ]]; then
    QDRANT_ARCH="x86_64-unknown-linux-gnu"
elif [[ "$ARCH" == "aarch64" ]]; then
    QDRANT_ARCH="aarch64-unknown-linux-gnu"
else
    echo -e "${RED}Unsupported architecture: $ARCH${NC}"
    exit 1
fi

# Check dependencies
echo -e "${YELLOW}Checking dependencies...${NC}"
MISSING_DEPS=()

for cmd in curl wget systemctl; do
    if ! command -v $cmd &> /dev/null; then
        MISSING_DEPS+=($cmd)
    fi
done

if [ ${#MISSING_DEPS[@]} -ne 0 ]; then
    echo -e "${RED}Missing dependencies: ${MISSING_DEPS[*]}${NC}"
    echo "Install with: sudo apt update && sudo apt install -y curl wget systemd"
    exit 1
fi

echo -e "${GREEN}Dependencies OK${NC}"

# Get API key
echo ""
echo -e "${YELLOW}Embedding Provider Configuration${NC}"
echo "AI Recall requires an API key for embeddings."
echo "1) OpenAI: https://platform.openai.com/api-keys"
echo "2) OpenRouter: https://openrouter.ai/keys"
echo ""
read -p "Enter your API key (sk-...): " API_KEY

if [[ -z "$API_KEY" ]]; then
    echo -e "${RED}Error: API key is required${NC}"
    exit 1
fi

# Get provider
read -p "Provider (openai/openrouter) [openai]: " PROVIDER
PROVIDER=${PROVIDER:-openai}

# Create user
echo ""
echo -e "${YELLOW}Creating system user...${NC}"
if ! id "$USER" &>/dev/null; then
    sudo useradd -r -s /bin/false -d "$INSTALL_DIR" "$USER"
    echo -e "${GREEN}User created: $USER${NC}"
else
    echo -e "${GREEN}User exists: $USER${NC}"
fi

# Create directories
echo ""
echo -e "${YELLOW}Creating directories...${NC}"
sudo mkdir -p "$INSTALL_DIR" "$DATA_DIR" "$CONFIG_DIR"
sudo chown -R "$USER:$USER" "$INSTALL_DIR"

# Install Qdrant
echo ""
echo -e "${YELLOW}Installing Qdrant vector database...${NC}"

if [ ! -f "/opt/qdrant/qdrant" ]; then
    sudo mkdir -p /opt/qdrant
    cd /opt/qdrant
    
    QDRANT_URL="https://github.com/qdrant/qdrant/releases/download/v${QDRANT_VERSION}/qdrant-${QDRANT_ARCH}.tar.gz"
    echo "Downloading Qdrant from $QDRANT_URL"
    
    sudo curl -L -o qdrant.tar.gz "$QDRANT_URL"
    sudo tar -xzf qdrant.tar.gz
    sudo rm qdrant.tar.gz
    
    # Create qdrant user
    if ! id "qdrant" &>/dev/null; then
        sudo useradd -r -s /bin/false qdrant
    fi
    
    sudo mkdir -p /var/lib/qdrant
    sudo chown qdrant:qdrant /var/lib/qdrant
    
    echo -e "${GREEN}Qdrant installed${NC}"
else
    echo -e "${GREEN}Qdrant already installed${NC}"
fi

# Create Qdrant systemd service
echo ""
echo -e "${YELLOW}Creating Qdrant service...${NC}"
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
Environment=QDRANT__STORAGE__STORAGE_PATH=/var/lib/qdrant

[Install]
WantedBy=multi-user.target
EOF

# Install AI Recall binary
echo ""
echo -e "${YELLOW}Installing AI Recall...${NC}"

# Check if binary exists locally or build it
if [ -f "./target/release/ai-recall" ]; then
    echo "Found local binary, copying..."
    sudo cp ./target/release/ai-recall /usr/local/bin/
elif [ -f "./ai-recall" ]; then
    echo "Found binary in current directory, copying..."
    sudo cp ./ai-recall /usr/local/bin/
else
    echo -e "${YELLOW}Binary not found. Building from source...${NC}"
    
    # Check for Rust
    if ! command -v cargo &> /dev/null; then
        echo "Installing Rust..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi
    
    # Build
    cargo build --release
    sudo cp ./target/release/ai-recall /usr/local/bin/
fi

sudo chmod +x /usr/local/bin/ai-recall
echo -e "${GREEN}AI Recall installed${NC}"

# Generate auth token
AUTH_TOKEN=$(/usr/local/bin/ai-recall token 2>/dev/null | grep "Generated" | awk '{print $4}' || echo "arec_$(openssl rand -base64 32 | tr -d '=+/' | cut -c1-43)")

# Create environment file
echo ""
echo -e "${YELLOW}Creating configuration...${NC}"
sudo tee "$CONFIG_DIR/environment" > /dev/null <<EOF
AI_RECALL_EMBEDDINGS_API_KEY=$API_KEY
AI_RECALL_EMBEDDINGS_PROVIDER=$PROVIDER
AI_RECALL_SERVER_AUTH_TOKEN=$AUTH_TOKEN
RUST_LOG=info
EOF
sudo chmod 600 "$CONFIG_DIR/environment"

# Create config.yaml
cd "$INSTALL_DIR"
sudo -u "$USER" tee config.yaml > /dev/null <<EOF
server:
  host: "127.0.0.1"
  port: 8080
  auth_token: "$AUTH_TOKEN"
  log_level: "info"

storage:
  data_dir: "$DATA_DIR"
  max_file_size_mb: 10

qdrant:
  url: "http://localhost:6334"
  collection_name: "memories"
  graph_collection_name: "memory_graph"
  vector_size: 1536
  distance: "Cosine"

embeddings:
  provider: "$PROVIDER"
  api_key: ""
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

# Initialize data directory
echo ""
echo -e "${YELLOW}Initializing data directory...${NC}"
sudo -u "$USER" /usr/local/bin/ai-recall init --config "$INSTALL_DIR/config.yaml"

# Create AI Recall systemd service
echo ""
echo -e "${YELLOW}Creating AI Recall service...${NC}"
sudo tee /etc/systemd/system/ai-recall.service > /dev/null <<EOF
[Unit]
Description=AI Recall Memory Server
After=network.target qdrant.service
Wants=qdrant.service

[Service]
Type=simple
User=$USER
Group=$USER
WorkingDirectory=$INSTALL_DIR
ExecStart=/usr/local/bin/ai-recall serve --config $INSTALL_DIR/config.yaml
Restart=always
RestartSec=5
EnvironmentFile=$CONFIG_DIR/environment

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=$DATA_DIR

[Install]
WantedBy=multi-user.target
EOF

# Enable and start services
echo ""
echo -e "${YELLOW}Starting services...${NC}"
sudo systemctl daemon-reload
sudo systemctl enable qdrant
sudo systemctl enable ai-recall
sudo systemctl start qdrant

# Wait for Qdrant to be ready
echo "Waiting for Qdrant to start..."
for i in {1..30}; do
    if curl -s http://localhost:6333/healthz > /dev/null 2>&1; then
        echo -e "${GREEN}Qdrant is ready${NC}"
        break
    fi
    sleep 1
done

sudo systemctl start ai-recall

# Wait for AI Recall
echo "Waiting for AI Recall to start..."
for i in {1..30}; do
    if curl -s http://localhost:8080/health > /dev/null 2>&1; then
        echo -e "${GREEN}AI Recall is ready${NC}"
        break
    fi
    sleep 1
done

# Display summary
echo ""
echo -e "${GREEN}=== Deployment Complete ===${NC}"
echo ""
echo "Service Status:"
sudo systemctl status ai-recall --no-pager -l

echo ""
echo -e "${YELLOW}Configuration:${NC}"
echo "  Install Directory: $INSTALL_DIR"
echo "  Data Directory: $DATA_DIR"
echo "  Config: $INSTALL_DIR/config.yaml"
echo "  Auth Token: $AUTH_TOKEN"
echo ""
echo -e "${YELLOW}Useful Commands:${NC}"
echo "  Check status:  sudo systemctl status ai-recall"
echo "  View logs:     sudo journalctl -u ai-recall -f"
echo "  Health check:  curl http://localhost:8080/health"
echo "  Test auth:     curl -H \"Authorization: Bearer $AUTH_TOKEN\" http://localhost:8080/health"
echo ""
echo -e "${YELLOW}Next Steps:${NC}"
echo "1. Set up reverse proxy (nginx/traefik) for HTTPS"
echo "2. Configure firewall: sudo ufw allow 8080/tcp"
echo "3. Set up backups (see DEPLOYMENT.md)"
echo ""
echo -e "${GREEN}Done!${NC}"
