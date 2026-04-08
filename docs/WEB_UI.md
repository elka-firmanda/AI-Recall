# AI Recall Web UI

## Overview

AI Recall includes a built-in web interface for uploading PDF files and managing memories through a browser. The UI runs on a **separate port** from the API, allowing you to expose the API to the internet while keeping the UI internal-only.

## Features

### 📄 PDF Upload
- Drag and drop PDF files onto the upload zone
- Multiple file support with queue management
- Background processing
- Real-time status updates

### 🔐 Authentication
- Simple password-based login
- Rate limiting (5 attempts, 15-minute lockout)
- Session-based authentication (1-hour TTL)
- Secure memory-only session storage

### 📊 Queue Management
- Visual queue status display
- Track pending, processing, completed, and failed uploads
- Individual job status checking
- Automatic cleanup of old jobs

### 🔍 Memory Browser
- List recent memories
- Search memories by content
- View memory metadata (tags, type, date)

## Architecture

```
┌─────────────────────┐     ┌─────────────────────┐
│   API Server        │     │   UI Server         │
│   Port 8080         │     │   Port 8081         │
│                     │     │                     │
│ • /health           │     │ • / (Web UI)        │
│ • /mcp              │     │ • /static/*         │
│ • /feedback         │     │ • /api/upload       │
│ • /contradictions   │     │ • /api/memories     │
│                     │     │ • /api/search       │
└─────────────────────┘     └─────────────────────┘
         │                            │
         ▼                            ▼
    Internet (via              Tailscale/Internal
    Cloudflare tunnel)         Network Only
```

## Getting Started

### Access the UI

By default, the UI runs on port **8081**:

```
http://<your-server-ip>:8081/
```

The API runs separately on port **8080**.

### Default Login

- **Password**: `admin` (change this!)

### Setting a Custom Password

Set the `AI_RECALL_UI_PASSWORD` environment variable:

```bash
# In your systemd service
Environment="AI_RECALL_UI_PASSWORD=your-secure-password"

# Or when running manually
AI_RECALL_UI_PASSWORD=your-password ./ai-recall serve
```

## Architecture

### Frontend
- **Technology**: Vanilla HTML5, CSS3, JavaScript
- **Design**: Light theme, minimalist, responsive
- **Files**: Stored in `/ui/` directory, embedded at compile time

### Backend Integration
- Static files served via `tower-http::fs`
- Session auth using custom middleware
- Queue system using tokio channels
- PDF extraction using `pdf-extract` crate

### New API Endpoints

The following endpoints are available for the UI:

| Endpoint | Method | Auth Required | Description |
|----------|--------|---------------|-------------|
| `/api/login` | POST | No | Authenticate and get session ID |
| `/api/logout` | POST | Yes | End session |
| `/api/session` | GET | Yes | Check if session is valid |
| `/api/upload` | POST | Yes | Upload PDF file(s) |
| `/api/upload/queue` | GET | Yes | Get queue status |
| `/api/upload/status/:id` | GET | Yes | Get specific job status |
| `/api/memories` | GET | Yes | List memories |
| `/api/search?q=query` | GET | Yes | Search memories |

## File Processing

### Upload Flow

1. **Upload**: Files are uploaded via multipart form data
2. **Queue**: Jobs are added to an in-memory queue
3. **Processing**: Background worker extracts text from PDF
4. **Storage**: Extracted text is saved as a memory
5. **Cleanup**: Temporary files are deleted

### PDF Extraction

The system uses the `pdf-extract` library to:
- Extract text from all PDF pages
- Detect simple tables (formatted as markdown)
- Create searchable memories

**Note**: Image extraction from PDFs is not currently supported.

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AI_RECALL_UI_PASSWORD` | `admin` | Password for web UI login |
| `AI_RECALL_UI_HOST` | `127.0.0.1` | UI server bind address |
| `AI_RECALL_UI_PORT` | `8081` | UI server port |
| `AI_RECALL_UI_ENABLED` | `true` | Enable/disable UI server |
| `AI_RECALL_SERVER_HOST` | `0.0.0.0` | API server bind address |
| `AI_RECALL_SERVER_PORT` | `8080` | API server port |

### Network Security Setup

#### Scenario 1: API on Internet, UI Internal Only

```bash
# API exposed to internet (via Cloudflare tunnel)
AI_RECALL_SERVER_HOST=0.0.0.0
AI_RECALL_SERVER_PORT=8080

# UI only on Tailscale/internal network
AI_RECALL_UI_HOST=100.64.0.1  # Your Tailscale IP
AI_RECALL_UI_PORT=8081
```

#### Scenario 2: Both on Same Network

```bash
# Bind both to all interfaces
AI_RECALL_SERVER_HOST=0.0.0.0
AI_RECALL_SERVER_PORT=8080
AI_RECALL_UI_HOST=0.0.0.0
AI_RECALL_UI_PORT=8081
```

#### Scenario 3: Disable UI

```bash
AI_RECALL_UI_ENABLED=false
```

### Systemd Service Example

```ini
[Unit]
Description=AI Recall Memory Server
After=network.target

[Service]
Type=simple
# API settings (exposed via Cloudflare)
Environment="AI_RECALL_SERVER_HOST=0.0.0.0"
Environment="AI_RECALL_SERVER_PORT=8080"

# UI settings (Tailscale only)
Environment="AI_RECALL_UI_HOST=100.64.0.1"
Environment="AI_RECALL_UI_PORT=8081"

# Security
Environment="AI_RECALL_UI_PASSWORD=your-secure-password"
Environment="AI_RECALL_SERVER_AUTH_TOKEN=your-api-token"
Environment="AI_RECALL_EMBEDDINGS_API_KEY=sk-..."

ExecStart=/usr/local/bin/ai-recall serve
Restart=always

[Install]
WantedBy=multi-user.target
```

### Rate Limiting

Login attempts are rate-limited to prevent brute force:
- 5 failed attempts per 5-minute window
- 15-minute lockout after exceeding limit
- Automatic reset after successful login

## Security Considerations

1. **Change Default Password**: Always set `AI_RECALL_UI_PASSWORD` in production
2. **Network Isolation**: UI runs on separate port - bind to internal/Tailscale IP only
3. **API Security**: API port can be safely exposed to internet (uses bearer token auth)
4. **Session Storage**: Sessions are stored in memory and lost on restart
5. **File Uploads**: Temporary files are stored in `data/.temp/` and cleaned up
6. **HTTPS**: Consider using a reverse proxy (nginx, Caddy) for HTTPS in production

## Troubleshooting

### Can't Access UI
- UI runs on port **8081** by default (not 8080)
- Check firewall rules for port 8081
- Verify UI server is enabled: `AI_RECALL_UI_ENABLED=true`
- Check bind address: UI might be bound to Tailscale IP only
- Check logs: `journalctl -u ai-recall -f`

### Can't Access API
- API runs on port **8080** by default
- Check firewall/cloudflare tunnel configuration
- Verify bearer token is set: `AI_RECALL_SERVER_AUTH_TOKEN`
- Test health endpoint: `curl http://localhost:8080/health`

### Login Issues
- Verify `AI_RECALL_UI_PASSWORD` is set correctly
- Check for rate limiting in logs (5 attempts, 15min lockout)
- Clear browser cookies/localStorage
- Make sure you're accessing UI port (8081), not API port (8080)

### Upload Failures
- Check file size (large PDFs may take time)
- Verify PDF is not corrupted or password-protected
- Check disk space in data directory
- Review server logs for extraction errors

### Port Conflicts
If you see "Address already in use":
```bash
# Check what's using the port
sudo lsof -i :8080
sudo lsof -i :8081

# Use different ports
AI_RECALL_SERVER_PORT=8082
AI_RECALL_UI_PORT=8083
```

### Session Expired
- Sessions expire after 1 hour of inactivity
- Simply log in again

## Development

### Modifying the UI

UI files are in the `/ui/` directory:
- `index.html` - Main page structure
- `style.css` - Styles (light theme)
- `app.js` - JavaScript functionality

After modifying, rebuild the binary:
```bash
cargo build --release
```

### UI Technology Stack

- No build step required
- No frontend framework (vanilla JS)
- Fetch API for HTTP requests
- CSS Grid/Flexbox for layout

## Future Enhancements

- [ ] Image extraction from PDFs
- [ ] Better table detection
- [ ] Upload progress bars
- [ ] Memory editing interface
- [ ] Dark mode toggle
- [ ] Batch operations
- [ ] Memory export
