# AI Recall 🧠

> Self-hosted AI agent memory system with vector search for AI coding tools

[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![MCP](https://img.shields.io/badge/MCP-Protocol-green.svg)](https://modelcontextprotocol.io)

AI Recall is a self-hosted memory system that enables AI coding assistants to remember context across sessions. It uses vector search (Qdrant) and graph relationships to provide semantic memory retrieval for your AI tools.

Inspired by [Andrej Karpathy](https://karpathy.ai/)'s explorations of LLM-augmented wikis, this project brings structured, searchable, and persistent memory to your AI coding workflow.

## ✨ Features

- **🔍 Vector Search**: Semantic similarity search using OpenAI/OpenRouter embeddings
- **🕸️ Graph Relationships**: Wiki-style linking between memories with backlink tracking
- **📝 Markdown Storage**: Human-readable memory files with YAML frontmatter
- **🔐 Authentication**: Bearer token auth for secure remote access
- **🔄 MCP Protocol**: Native integration with Model Context Protocol
- **📊 Feedback System**: Track memory usefulness ratings
- **⚠️ Contradiction Detection**: Detect conflicting information across memories
- **🏷️ Versioning**: Git-style versioning for memory history
- **🚀 Multi-Tool Support**: Works with Claude Code, OpenCode, Codex, and more

## 🏗️ Architecture

```
┌─────────────────┐     ┌──────────────┐     ┌─────────────────┐
│   AI Coding     │────▶│  AI Recall   │────▶│   Qdrant        │
│   Tools         │ MCP │   Server     │     │  (Vector DB)    │
│                 │     │              │     │                 │
└─────────────────┘     └──────────────┘     └─────────────────┘
                               │
                               ▼
                        ┌──────────────┐
                        │   Markdown   │
                        │   Storage    │
                        │  (Wiki +     │
                        │   Graph)     │
                        └──────────────┘
```

## 🚀 Quick Start

### Docker Compose (Recommended)

```bash
# Clone the repository
git clone <your-repo-url>
cd ai-recall

# Set your API key
export OPENAI_API_KEY=sk-your-key-here

# Deploy
docker-compose up -d

# Check status
docker-compose ps
```

### Binary Deployment

```bash
# Build
cargo build --release

# Install
sudo cp target/release/ai-recall /usr/local/bin/

# Deploy with automated script
./scripts/deploy-homelab.sh

# Or run directly
ai-recall init
ai-recall serve
```

## 🔌 MCP Integration Guide

AI Recall integrates with AI coding tools via the **Model Context Protocol (MCP)**.

### Claude Code

**Configuration**: `~/.claude/config.json` or Claude Desktop settings

```json
{
  "mcpServers": {
    "ai-recall": {
      "command": "/usr/local/bin/ai-recall",
      "args": ["mcp"],
      "env": {
        "AI_RECALL_SERVER_AUTH_TOKEN": "arec_your_token",
        "AI_RECALL_STORAGE_DATA_DIR": "~/.ai-recall/data",
        "AI_RECALL_QDRANT_URL": "http://localhost:6334",
        "AI_RECALL_EMBEDDINGS_API_KEY": "sk-your-openai-key"
      }
    }
  }
}
```

**Features**:
- `memory_add` - Save code patterns, architecture decisions
- `memory_search` - Find related code examples
- `feedback_record` - Rate helpfulness of memories
- `contradictions_check` - Detect conflicting guidance

**Usage in Claude**:
```
Claude, remember this error pattern for Rust lifetimes
Claude, search for similar async handling patterns
Claude, check if this contradicts previous advice
```

---

### OpenCode

**Configuration**: `~/.opencode/mcp.json`

```json
{
  "mcp": {
    "servers": [
      {
        "name": "ai-recall",
        "transport": {
          "type": "stdio",
          "command": "/usr/local/bin/ai-recall",
          "args": ["mcp"],
          "env": {
            "AI_RECALL_SERVER_AUTH_TOKEN": "arec_your_token",
            "AI_RECALL_STORAGE_DATA_DIR": "~/.ai-recall/data",
            "AI_RECALL_QDRANT_URL": "http://localhost:6334",
            "AI_RECALL_EMBEDDINGS_API_KEY": "sk-your-openai-key"
          }
        }
      }
    ]
  }
}
```

**Features**:
- Persistent context across coding sessions
- Pattern matching for code review
- Knowledge base for team standards

**Usage in OpenCode**:
```
/opencode memory save "Common React hook patterns"
/opencode memory search "authentication flows"
/opencode memory contradict "Should I use useEffect or useLayoutEffect?"
```

---

### Codex (OpenAI)

**Configuration**: `~/.codex/config.yaml`

```yaml
mcp_servers:
  - name: ai-recall
    type: stdio
    command: /usr/local/bin/ai-recall
    args:
      - mcp
    env:
      AI_RECALL_SERVER_AUTH_TOKEN: arec_your_token
      AI_RECALL_STORAGE_DATA_DIR: ~/.ai-recall/data
      AI_RECALL_QDRANT_URL: http://localhost:6334
      AI_RECALL_EMBEDDINGS_API_KEY: sk-your-openai-key
```

**Features**:
- Store API integration patterns
- Remember project-specific conventions
- Build knowledge graph of codebase

**Usage in Codex**:
```
codex> Remember this GraphQL mutation pattern for future reference
codex> What did we decide about error handling last week?
codex> Search for all authentication-related patterns
```

---

### Droid (Kotlin/Android)

**Configuration**: `~/.droid/mcp_config.json`

```json
{
  "mcp": {
    "ai-recall": {
      "type": "stdio",
      "command": "/usr/local/bin/ai-recall",
      "args": ["mcp"],
      "environment": {
        "AI_RECALL_SERVER_AUTH_TOKEN": "arec_your_token",
        "AI_RECALL_STORAGE_DATA_DIR": "~/.ai-recall/droid-data",
        "AI_RECALL_QDRANT_URL": "http://localhost:6334",
        "AI_RECALL_EMBEDDINGS_API_KEY": "sk-your-openai-key"
      }
    }
  }
}
```

**Features for Android Development**:
- Store Jetpack Compose patterns
- Remember custom view implementations
- Track dependency version decisions

**Usage in Droid**:
```
@droid remember "Best practice for ViewModel with Hilt"
@droid search "room database migration patterns"
@droid contradict "Should I use LiveData or StateFlow?"
```

---

### Forge Code

**Configuration**: `~/.forge/mcp.yaml`

```yaml
servers:
  ai-recall:
    transport: stdio
    command: /usr/local/bin/ai-recall
    args:
      - mcp
    env:
      AI_RECALL_SERVER_AUTH_TOKEN: arec_your_token
      AI_RECALL_STORAGE_DATA_DIR: ~/.ai-recall/data
      AI_RECALL_QDRANT_URL: http://localhost:6334
      AI_RECALL_EMBEDDINGS_API_KEY: sk-your-openai-key
```

**Features**:
- Code review pattern storage
- Architecture decision records
- Team coding standards

**Usage in Forge**:
```
forge memory add "Microservice communication pattern"
forge memory search "circuit breaker implementation"
forge feedback record useful "Pattern helped solve timeout issue"
```

---

### OpenClaw

**Configuration**: `~/.openclaw/config/mcp.json`

```json
{
  "servers": [
    {
      "id": "ai-recall",
      "enabled": true,
      "type": "stdio",
      "command": "/usr/local/bin/ai-recall",
      "args": ["mcp"],
      "environment": {
        "AI_RECALL_SERVER_AUTH_TOKEN": "arec_your_token",
        "AI_RECALL_STORAGE_DATA_DIR": "~/.ai-recall/data",
        "AI_RECALL_QDRANT_URL": "http://localhost:6334",
        "AI_RECALL_EMBEDDINGS_API_KEY": "sk-your-openai-key"
      }
    }
  ]
}
```

**Features**:
- Cross-session context retention
- Project-specific knowledge base
- Integration pattern library

**Usage in OpenClaw**:
```
/openclaw memory save "CI/CD pipeline pattern for Rust"
/openclaw memory find "docker multi-stage build optimization"
/openclaw memory outdated "Check old deployment patterns"
```

---

### Hermes

**Configuration**: `~/.hermes/mcp.toml`

```toml
[[servers]]
name = "ai-recall"
type = "stdio"
command = "/usr/local/bin/ai-recall"
args = ["mcp"]

[servers.env]
AI_RECALL_SERVER_AUTH_TOKEN = "arec_your_token"
AI_RECALL_STORAGE_DATA_DIR = "~/.ai-recall/data"
AI_RECALL_QDRANT_URL = "http://localhost:6334"
AI_RECALL_EMBEDDINGS_API_KEY = "sk-your-openai-key"
```

**Features**:
- Persistent project context
- Code snippet library
- Architecture patterns

**Usage in Hermes**:
```
hermes> Save this regex pattern for email validation
hermes> Find patterns for parsing JSON in streaming mode
hermes> Check contradictions in error handling advice
```

---

## 🛠️ Available MCP Tools

### Memory Management

| Tool | Description | Usage |
|------|-------------|-------|
| `memory_add` | Create new memory | Store patterns, decisions |
| `memory_search` | Semantic search | Find related information |
| `memory_get` | Retrieve by ID | Get specific memory |
| `memory_list` | List memories | Browse all entries |
| `memory_update` | Update memory | Correct or enhance |
| `memory_delete` | Remove memory | Clean up |

### Feedback & Quality

| Tool | Description | Usage |
|------|-------------|-------|
| `feedback_record` | Rate usefulness | Mark helpful/outdated |
| `feedback_stats` | View ratings | Check memory quality |
| `contradictions_check` | Find conflicts | Detect stale advice |

### System

| Tool | Description |
|------|-------------|
| `system_status` | Health check |
| `initialize` | First-time setup |

---

## 📋 Configuration Reference

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `AI_RECALL_EMBEDDINGS_API_KEY` | ✅ | - | OpenAI/OpenRouter API key |
| `AI_RECALL_EMBEDDINGS_PROVIDER` | ❌ | `openai` | Provider name |
| `AI_RECALL_SERVER_AUTH_TOKEN` | ❌ | Auto | Auth token |
| `AI_RECALL_STORAGE_DATA_DIR` | ❌ | `./data` | Data directory |
| `AI_RECALL_QDRANT_URL` | ❌ | `localhost:6334` | Qdrant URL |
| `AI_RECALL_SERVER_HOST` | ❌ | `127.0.0.1` | Bind address |
| `AI_RECALL_SERVER_PORT` | ❌ | `8080` | HTTP port |

### Config File (`config.yaml`)

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  auth_token: "arec_your_secure_token"

storage:
  data_dir: "/data"
  max_file_size_mb: 10

qdrant:
  url: "http://qdrant:6334"
  collection_name: "memories"
  vector_size: 1536

embeddings:
  provider: "openai"  # or "openrouter"
  api_key: "sk-your-key"
  model: "text-embedding-3-small"
  dimension: 1536
```

---

## 🔒 Security Best Practices

1. **Use Auth Token**: Always set `AI_RECALL_SERVER_AUTH_TOKEN`
2. **HTTPS Only**: Use reverse proxy (nginx/traefik) for remote access
3. **API Key Security**: Store in environment, never commit to git
4. **Firewall**: Restrict port 8080 to localhost if not using reverse proxy
5. **Backups**: Regular backups of `data/` directory

### Quick Security Setup

```bash
# Generate secure token
export AI_RECALL_SERVER_AUTH_TOKEN="arec_$(openssl rand -base64 32)"

# Set restrictive permissions
chmod 600 .env
chmod 600 config.yaml

# Use Cloudflare Tunnel (no port opening)
cloudflared tunnel create ai-recall
```

---

## 📊 API Usage Examples

### HTTP API (with Auth)

```bash
# Health check
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/health

# Add memory
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Rust Error Handling Pattern",
    "content": "Use anyhow for application code, thiserror for libraries",
    "type": "semantic",
    "tags": ["rust", "error-handling"]
  }' \
  http://localhost:8080/mcp

# Search memories
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "rust error handling",
    "limit": 5
  }' \
  http://localhost:8080/mcp
```

### Check Contradictions

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "contradictions_check",
    "arguments": {}
  }' \
  http://localhost:8080/mcp
```

---

## 🐳 Docker Deployment

### Basic Docker

```bash
docker run -d \
  -p 8080:8080 \
  -v $(pwd)/data:/data \
  -e AI_RECALL_EMBEDDINGS_API_KEY=sk-your-key \
  -e AI_RECALL_QDRANT_URL=http://host.docker.internal:6334 \
  ai-recall:latest serve
```

### Docker Compose (Full Stack)

```bash
# Start everything
docker-compose up -d

# View logs
docker-compose logs -f

# Update
docker-compose pull && docker-compose up -d
```

---

## 🔧 Troubleshooting

| Issue | Solution |
|-------|----------|
| Qdrant connection failed | Check Qdrant running: `curl http://localhost:6333/healthz` |
| Auth errors | Verify token in environment or config |
| Embedding failures | Check API key validity and quota |
| Port conflicts | Change port in config: `AI_RECALL_SERVER_PORT=8081` |
| Permission denied | Fix data dir ownership: `sudo chown -R $USER:$USER ./data` |

### Debug Mode

```bash
RUST_LOG=debug ai-recall serve
```

---

## 🏠 Homelab Deployment

See [DEPLOYMENT.md](DEPLOYMENT.md) for detailed deployment options:

- **Docker Compose** (easiest)
- **Systemd Services** (best performance)
- **Nginx Reverse Proxy** (HTTPS)
- **Cloudflare Tunnel** (no port opening)
- **Automated Scripts** (one-command deploy)

Quick deploy script:
```bash
./scripts/deploy-homelab.sh
```

---

## 📁 Project Structure

```
ai-recall/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── lib.rs            # Library exports
│   ├── config/           # Configuration
│   ├── mcp/              # MCP server & handlers
│   ├── storage/          # Storage backends
│   │   ├── markdown.rs   # File storage
│   │   ├── qdrant.rs     # Vector DB
│   │   ├── versioning.rs # Git-style versions
│   │   └── feedback.rs   # Feedback system
│   ├── graph/            # Relationship graph
│   ├── analysis/         # Contradiction detection
│   ├── auth/             # Authentication
│   ├── embeddings/       # Embedding client
│   └── models/           # Data models
├── tests/                # Integration tests
├── scripts/              # Deployment scripts
├── docker-compose.yml    # Docker deployment
├── Dockerfile            # Container build
├── config.example.yaml   # Config template
└── DEPLOYMENT.md         # Detailed deployment guide
```

---

## 🙏 Acknowledgments

This project was inspired by the work of **[Andrej Karpathy](https://karpathy.ai/)** and his explorations of wikis and LLM-augmented knowledge management. His insights on leveraging structured memory systems with language models have been instrumental in shaping the design philosophy behind AI Recall.

- [Andrej's blog on LLM Wikis](https://karpathy.ai/) - Exploring the intersection of wikis and large language models
- The broader AI community for pushing the boundaries of agent memory systems

---

## 🤝 Contributing

Contributions welcome! Areas of interest:

- Additional MCP tool implementations
- New embedding providers
- Graph visualization
- Web UI for memory management
- Performance optimizations

---

## 📜 License

Apache 2.0 - See [LICENSE](LICENSE) for details.

---

## 🔗 Links

- [Model Context Protocol](https://modelcontextprotocol.io)
- [Qdrant Vector DB](https://qdrant.tech)
- [OpenAI Embeddings](https://platform.openai.com)
- [OpenRouter](https://openrouter.ai)

---

## 💡 Tips for AI Coding Tools

1. **Tag consistently**: Use consistent tags like `rust`, `async`, `testing`
2. **Rate memories**: Mark outdated info so contradictions can be detected
3. **Link memories**: Use `[[Memory Title]]` syntax to create relationships
4. **Regular cleanup**: Check for contradictions periodically
5. **Backup**: The `data/` directory contains all your memories

**Happy coding with persistent memory!** 🧠✨
