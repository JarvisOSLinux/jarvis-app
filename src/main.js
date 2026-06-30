'use strict';

// Tauri 2 globals injected via withGlobalTauri: true
const invoke = window.__TAURI__.core.invoke;
const listen  = window.__TAURI__.event.listen;

// ── State ──────────────────────────────────────────────────────────────────
let isConnected  = false;
let isListening  = false;
let hasMessages  = false;

// ── DOM refs ───────────────────────────────────────────────────────────────
const chatView     = document.getElementById('chat-view');
const emptyState   = document.getElementById('empty-state');
const messages     = document.getElementById('messages');
const messageInput = document.getElementById('message-input');
const sendBtn      = document.getElementById('send-btn');
const micBtn       = document.getElementById('mic-btn');
const statusDot    = document.getElementById('status-dot');
const statusLabel  = document.getElementById('status-label');

// ── Status ─────────────────────────────────────────────────────────────────
const STATE_COLOR = {
    idle:       '#00c8ff',
    listening:  '#00ff88',
    processing: '#ffaa00',
    speaking:   '#aa44ff',
    offline:    '#ff4455',
};

const STATE_LABEL = {
    idle:       'Idle',
    listening:  'Listening',
    processing: 'Processing',
    speaking:   'Speaking',
    offline:    'Offline',
};

function setStatus(state) {
    const color = STATE_COLOR[state] ?? STATE_COLOR.offline;
    statusDot.style.background   = color;
    statusDot.style.color        = color;
    statusLabel.style.color      = color;
    statusLabel.textContent      = STATE_LABEL[state] ?? 'Offline';

    const pulsing = state === 'listening' || state === 'processing' || state === 'speaking';
    statusDot.classList.toggle('pulsing', pulsing);

    isListening = state === 'listening';
    micBtn.classList.toggle('listening', isListening);
}

function setConnected(connected) {
    isConnected = connected;
    messageInput.disabled    = !connected;
    messageInput.placeholder = connected ? 'Message JARVIS…' : 'Connecting to daemon…';
    if (!connected) {
        micBtn.classList.remove('listening');
        isListening = false;
    }
    refreshSendBtn();
}

function refreshSendBtn() {
    sendBtn.disabled = !isConnected || messageInput.value.trim() === '';
}

// ── Messages ───────────────────────────────────────────────────────────────
function timestamp() {
    return new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
}

function showMessages() {
    if (!hasMessages) {
        hasMessages = true;
        emptyState.style.display = 'none';
    }
}

function addUserMessage(content) {
    showMessages();
    const el = document.createElement('div');
    el.className = 'message user-message';

    const col = document.createElement('div');
    col.className = 'bubble-col align-right';

    const bubble = document.createElement('div');
    bubble.className = 'bubble user-bubble';
    bubble.textContent = content;

    const ts = document.createElement('div');
    ts.className = 'timestamp right';
    ts.textContent = timestamp();

    col.append(bubble, ts);
    el.append(col);
    messages.append(el);
    requestAnimationFrame(() => el.classList.add('visible'));
    scrollBottom();
}

function appendJarvisChunk(content, done) {
    showMessages();
    const streaming = messages.querySelector('.jarvis-message.streaming');

    if (streaming) {
        const span = streaming.querySelector('.bubble-text');
        const accumulated = (span.dataset.raw ?? '') + content;
        span.dataset.raw   = accumulated;
        span.textContent   = accumulated + (done ? '' : ' ▌');
        if (done) streaming.classList.remove('streaming');
    } else {
        const el = document.createElement('div');
        el.className = 'message jarvis-message' + (done ? '' : ' streaming');

        const avatar = document.createElement('div');
        avatar.className = 'avatar';
        avatar.textContent = 'J';

        const col = document.createElement('div');
        col.className = 'bubble-col';

        const bubble = document.createElement('div');
        bubble.className = 'bubble jarvis-bubble';

        const span = document.createElement('span');
        span.className    = 'bubble-text';
        span.dataset.raw  = content;
        span.textContent  = content + (done ? '' : ' ▌');

        const ts = document.createElement('div');
        ts.className   = 'timestamp';
        ts.textContent = timestamp();

        bubble.append(span);
        col.append(bubble, ts);
        el.append(avatar, col);
        messages.append(el);
        requestAnimationFrame(() => el.classList.add('visible'));
    }
    scrollBottom();
}

function scrollBottom() {
    requestAnimationFrame(() => { chatView.scrollTop = chatView.scrollHeight; });
}

// ── Actions ────────────────────────────────────────────────────────────────
async function sendMessage() {
    const text = messageInput.value.trim();
    if (!text || !isConnected) return;
    messageInput.value = '';
    refreshSendBtn();
    addUserMessage(text);
    setStatus('processing');
    await invoke('send_message', { content: text });
}

async function toggleListening() {
    await invoke('toggle_listening');
}

// ── Boot ───────────────────────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', async () => {
    messageInput.addEventListener('input', refreshSendBtn);

    messageInput.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            sendMessage();
        }
    });

    sendBtn.addEventListener('click', sendMessage);
    micBtn.addEventListener('click', toggleListening);

    // IPC events from Rust backend
    await listen('ipc-connected',    ()      => setConnected(true));
    await listen('ipc-disconnected', ()      => { setConnected(false); setStatus('offline'); });
    await listen('ipc-state',        (e)     => setStatus(e.payload));
    await listen('ipc-chunk',        (e)     => appendJarvisChunk(e.payload.content, e.payload.done));
    await listen('ipc-wake',         ()      => setStatus('listening'));
    await listen('ipc-confirm',      async (e) => {
        const { id, description } = e.payload;
        const approved = window.confirm('JARVIS wants to:\n\n' + description + '\n\nAllow?');
        await invoke('send_confirmation_response', { id, approved });
    });
});
