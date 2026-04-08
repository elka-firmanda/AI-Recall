// AI Recall UI - Vanilla JavaScript
// Handles authentication, file upload queue, and memory browsing

const API_BASE = '';

// Session state
let sessionId = localStorage.getItem('ai_recall_session');
let refreshInterval = null;

// DOM Elements
const loginScreen = document.getElementById('login-screen');
const mainScreen = document.getElementById('main-screen');
const loginForm = document.getElementById('login-form');
const passwordInput = document.getElementById('password');
const loginError = document.getElementById('login-error');
const logoutBtn = document.getElementById('logout-btn');
const dropZone = document.getElementById('drop-zone');
const fileInput = document.getElementById('file-input');
const queueList = document.getElementById('queue-list');
const memoriesList = document.getElementById('memories-list');
const searchInput = document.getElementById('search-input');
const searchBtn = document.getElementById('search-btn');

// Initialize
async function init() {
    if (sessionId) {
        const valid = await checkSession();
        if (valid) {
            showMainScreen();
        } else {
            showLoginScreen();
        }
    } else {
        showLoginScreen();
    }
}

// Auth Functions
async function login(password) {
    try {
        const response = await fetch(`${API_BASE}/api/login`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ password })
        });

        if (!response.ok) {
            throw new Error('Invalid password');
        }

        const data = await response.json();
        sessionId = data.session_id;
        localStorage.setItem('ai_recall_session', sessionId);
        showMainScreen();
    } catch (error) {
        loginError.textContent = error.message;
        passwordInput.value = '';
    }
}

async function logout() {
    if (sessionId) {
        try {
            await fetch(`${API_BASE}/api/logout`, {
                method: 'POST',
                headers: { 'X-Session-ID': sessionId }
            });
        } catch (e) {
            console.error('Logout error:', e);
        }
    }
    
    sessionId = null;
    localStorage.removeItem('ai_recall_session');
    if (refreshInterval) {
        clearInterval(refreshInterval);
    }
    showLoginScreen();
}

async function checkSession() {
    try {
        const response = await fetch(`${API_BASE}/api/session`, {
            headers: { 'X-Session-ID': sessionId }
        });
        return response.ok;
    } catch {
        return false;
    }
}

// UI Functions
function showLoginScreen() {
    loginScreen.classList.remove('hidden');
    mainScreen.classList.add('hidden');
    passwordInput.focus();
}

function showMainScreen() {
    loginScreen.classList.add('hidden');
    mainScreen.classList.remove('hidden');
    startRefreshInterval();
    refreshData();
}

// Upload Functions
async function uploadFiles(files) {
    for (const file of files) {
        if (file.type !== 'application/pdf' && !file.name.endsWith('.pdf')) {
            showToast(`${file.name} is not a PDF file`, 'error');
            continue;
        }

        const formData = new FormData();
        formData.append('file', file);
        formData.append('filename', file.name);

        try {
            const response = await fetch(`${API_BASE}/api/upload`, {
                method: 'POST',
                headers: { 'X-Session-ID': sessionId },
                body: formData
            });

            if (!response.ok) {
                throw new Error(`Upload failed: ${response.status}`);
            }

            const data = await response.json();
            showToast(`${file.name} added to queue`, 'success');
        } catch (error) {
            showToast(`Failed to upload ${file.name}: ${error.message}`, 'error');
        }
    }
    
    refreshQueue();
}

async function refreshQueue() {
    try {
        const response = await fetch(`${API_BASE}/api/upload/queue`, {
            headers: { 'X-Session-ID': sessionId }
        });
        
        if (!response.ok) throw new Error('Failed to fetch queue');
        
        const data = await response.json();
        updateQueueDisplay(data);
    } catch (error) {
        console.error('Queue refresh error:', error);
    }
}

async function refreshMemories() {
    try {
        const response = await fetch(`${API_BASE}/api/memories?limit=10`, {
            headers: { 'X-Session-ID': sessionId }
        });
        
        if (!response.ok) throw new Error('Failed to fetch memories');
        
        const data = await response.json();
        updateMemoriesDisplay(data);
    } catch (error) {
        console.error('Memories refresh error:', error);
    }
}

async function searchMemories(query) {
    try {
        const response = await fetch(`${API_BASE}/api/search?q=${encodeURIComponent(query)}`, {
            headers: { 'X-Session-ID': sessionId }
        });
        
        if (!response.ok) throw new Error('Search failed');
        
        const data = await response.json();
        updateMemoriesDisplay(data);
    } catch (error) {
        console.error('Search error:', error);
        showToast('Search failed', 'error');
    }
}

// Display Functions
function updateQueueDisplay(data) {
    const { pending, processing, completed, failed, items } = data;
    
    document.getElementById('pending-count').textContent = pending;
    document.getElementById('processing-count').textContent = processing;
    document.getElementById('completed-count').textContent = completed;
    document.getElementById('failed-count').textContent = failed;
    
    if (!items || items.length === 0) {
        queueList.innerHTML = '<p class="empty">No files in queue</p>';
        return;
    }
    
    queueList.innerHTML = items.map(item => `
        <div class="queue-item">
            <div class="queue-item-icon">📄</div>
            <div class="queue-item-info">
                <div class="queue-item-name">${escapeHtml(item.filename)}</div>
                <div class="queue-item-meta">${formatBytes(item.size)} • ${formatDate(item.created_at)}</div>
            </div>
            <div class="progress-bar">
                <div class="progress-fill" style="width: ${item.progress || 0}%"></div>
            </div>
            <span class="queue-item-status status-${item.status}">${item.status}</span>
        </div>
    `).join('');
}

function updateMemoriesDisplay(data) {
    const memories = data.data || data.memories || [];
    
    if (!memories || memories.length === 0) {
        memoriesList.innerHTML = '<p class="empty">No memories yet</p>';
        return;
    }
    
    memoriesList.innerHTML = memories.map(memory => `
        <div class="memory-item">
            <div class="memory-title">${escapeHtml(memory.title)}</div>
            <div class="memory-content">${escapeHtml(memory.content || memory.snippet?.text || '')}</div>
            <div class="memory-meta">
                <span>${formatDate(memory.created_at)}</span>
                <span class="memory-tag">${memory.type || memory.memory_type}</span>
                ${(memory.tags || []).map(tag => `<span class="memory-tag">${escapeHtml(tag)}</span>`).join('')}
            </div>
        </div>
    `).join('');
}

// Event Listeners
loginForm.addEventListener('submit', (e) => {
    e.preventDefault();
    login(passwordInput.value);
});

logoutBtn.addEventListener('click', logout);

// Drag and Drop
dropZone.addEventListener('click', () => fileInput.click());

dropZone.addEventListener('dragover', (e) => {
    e.preventDefault();
    dropZone.classList.add('drag-over');
});

dropZone.addEventListener('dragleave', () => {
    dropZone.classList.remove('drag-over');
});

dropZone.addEventListener('drop', (e) => {
    e.preventDefault();
    dropZone.classList.remove('drag-over');
    const files = Array.from(e.dataTransfer.files);
    uploadFiles(files);
});

fileInput.addEventListener('change', (e) => {
    const files = Array.from(e.target.files);
    uploadFiles(files);
    fileInput.value = ''; // Reset
});

searchBtn.addEventListener('click', () => {
    const query = searchInput.value.trim();
    if (query) {
        searchMemories(query);
    } else {
        refreshMemories();
    }
});

searchInput.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') {
        searchBtn.click();
    }
});

// Utilities
function startRefreshInterval() {
    if (refreshInterval) clearInterval(refreshInterval);
    refreshInterval = setInterval(refreshQueue, 2000); // Refresh queue every 2 seconds
}

function refreshData() {
    refreshQueue();
    refreshMemories();
}

function showToast(message, type = 'info') {
    const container = document.querySelector('.toast-container') || createToastContainer();
    const toast = document.createElement('div');
    toast.className = `toast ${type}`;
    toast.textContent = message;
    container.appendChild(toast);
    
    setTimeout(() => {
        toast.remove();
    }, 5000);
}

function createToastContainer() {
    const container = document.createElement('div');
    container.className = 'toast-container';
    document.body.appendChild(container);
    return container;
}

function escapeHtml(text) {
    if (!text) return '';
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function formatBytes(bytes) {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

function formatDate(dateStr) {
    if (!dateStr) return '';
    const date = new Date(dateStr);
    return date.toLocaleString();
}

// Start the app
init();
