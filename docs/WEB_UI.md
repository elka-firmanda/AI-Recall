# AI Recall Web UI

## Overview

AI Recall now includes a built-in web interface for uploading PDF files and managing memories through a browser. The UI is embedded directly in the Rust binary, requiring no separate web server.

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

## Getting Started

### Access the UI

Once the server is running, open your browser:

```
http://<your-server-ip>:8080/
```

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
| `AI_RECALL_SERVER_PORT` | `8080` | HTTP server port |
| `AI_RECALL_SERVER_HOST` | `0.0.0.0` | HTTP server bind address |

### Rate Limiting

Login attempts are rate-limited to prevent brute force:
- 5 failed attempts per 5-minute window
- 15-minute lockout after exceeding limit
- Automatic reset after successful login

## Security Considerations

1. **Change Default Password**: Always set `AI_RECALL_UI_PASSWORD` in production
2. **Network Access**: The UI is available on all interfaces by default (0.0.0.0)
3. **Session Storage**: Sessions are stored in memory and lost on restart
4. **File Uploads**: Temporary files are stored in `data/.temp/` and cleaned up
5. **HTTPS**: Consider using a reverse proxy (nginx, Caddy) for HTTPS in production

## Troubleshooting

### Can't Access UI
- Check firewall rules for port 8080
- Verify server is running: `curl http://localhost:8080/health`
- Check logs: `journalctl -u ai-recall -f`

### Login Issues
- Verify `AI_RECALL_UI_PASSWORD` is set correctly
- Check for rate limiting in logs
- Clear browser cookies/localStorage

### Upload Failures
- Check file size (large PDFs may take time)
- Verify PDF is not corrupted or password-protected
- Check disk space in data directory
- Review server logs for extraction errors

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
