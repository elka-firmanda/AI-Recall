#!/bin/bash
# Quick Docker Compose deployment for homelab

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}=== AI Recall Docker Deployment ===${NC}"
echo ""

# Check Docker
if ! command -v docker &> /dev/null; then
    echo "Docker not found. Install first:"
    echo "  curl -fsSL https://get.docker.com | sh"
    exit 1
fi

if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
    echo "Docker Compose not found. Install first."
    exit 1
fi

# Get API key
echo -e "${YELLOW}Enter your OpenAI/OpenRouter API key:${NC}"
read -p "API Key (sk-...): " API_KEY

if [[ -z "$API_KEY" ]]; then
    echo "Error: API key is required"
    exit 1
fi

# Create deployment directory
DEPLOY_DIR="$HOME/ai-recall"
mkdir -p "$DEPLOY_DIR"
cd "$DEPLOY_DIR"

# Download config files if not present
if [ ! -f "docker-compose.yml" ]; then
    echo -e "${YELLOW}Downloading configuration files...${NC}"
    
    # You should replace these URLs with your actual repo/raw URLs
    # For now, we'll create them
    
    cat > docker-compose.yml <<'EOF'
version: '3.8'

services:
  qdrant:
    image: qdrant/qdrant:latest
    container_name: ai-recall-qdrant
    restart: unless-stopped
    ports:
      - "6333:6333"
      - "6334:6334"
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

  ai-recall:
    build:
      context: .
      dockerfile: Dockerfile
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
      - RUST_LOG=info
    depends_on:
      qdrant:
        condition: service_healthy
    command: ["serve"]
EOF

    cat > config.yaml <<'EOF'
server:
  host: "0.0.0.0"
  port: 8080
  log_level: "info"

storage:
  data_dir: "/data"
  max_file_size_mb: 10

qdrant:
  url: "http://qdrant:6334"
  collection_name: "memories"
  graph_collection_name: "memory_graph"
  vector_size: 1536
  distance: "Cosine"

embeddings:
  provider: "openai"
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

    cat > Dockerfile <<'EOF'
FROM rust:1.75-slim-bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src
COPY src ./src
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates curl wget && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/ai-recall /usr/local/bin/ai-recall
RUN mkdir -p /data && chmod 755 /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost:8080/health || exit 1
ENTRYPOINT ["ai-recall"]
CMD ["serve"]
EOF

fi

# Create .env file
cat > .env <<EOF
OPENAI_API_KEY=$API_KEY
EMBEDDING_PROVIDER=openai
EMBEDDING_MODEL=text-embedding-3-small
EOF
chmod 600 .env

# Build and start
echo -e "${YELLOW}Building and starting services...${NC}"

if command -v docker-compose &> /dev/null; then
    docker-compose up -d --build
else
    docker compose up -d --build
fi

# Wait for health
echo "Waiting for services to be healthy..."
sleep 10

# Get auth token
TOKEN=$(docker-compose logs ai-recall 2>/dev/null | grep -i "token" | head -1 | awk '{print $NF}' || echo "Check logs with: docker-compose logs ai-recall")

echo ""
echo -e "${GREEN}=== Deployment Complete ===${NC}"
echo ""
echo "Services:"
if command -v docker-compose &> /dev/null; then
    docker-compose ps
else
    docker compose ps
fi

echo ""
echo "URLs:"
echo "  AI Recall: http://localhost:8080"
echo "  Qdrant UI: http://localhost:6333/dashboard"
if [ -n "$TOKEN" ]; then
    echo ""
    echo "Auth Token: $TOKEN"
fi
echo ""
echo "Commands:"
echo "  Logs:     docker-compose logs -f"
echo "  Stop:     docker-compose down"
echo "  Update:   docker-compose pull && docker-compose up -d"
