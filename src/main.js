'use strict';

// Tauri 2 globals injected via withGlobalTauri: true
const invoke = window.__TAURI__.core.invoke;
const listen  = window.__TAURI__.event.listen;

// ── State ──────────────────────────────────────────────────────────────────
let isConnected     = false;
let isListening     = false;
let hasMessages     = false;
let currentSessionId = null;

// ── DOM refs ───────────────────────────────────────────────────────────────
const chatView        = document.getElementById('chat-view');
const emptyState      = document.getElementById('empty-state');
const messages        = document.getElementById('messages');
const messageInput    = document.getElementById('message-input');
const sendBtn         = document.getElementById('send-btn');
const micBtn          = document.getElementById('mic-btn');
const statusDot       = document.getElementById('status-dot');
const statusLabel     = document.getElementById('status-label');
const sessionsBtn     = document.getElementById('sessions-btn');
const sessionBackdrop = document.getElementById('session-backdrop');
const sessionSidebar  = document.getElementById('session-sidebar');
const sessionList     = document.getElementById('session-list');
const newSessionBtn   = document.getElementById('new-session-btn');

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

const STREAM_CURSOR = ' <span class="stream-cursor">▌</span>';

function appendJarvisChunk(content, done) {
    showMessages();
    const streaming = messages.querySelector('.jarvis-message.streaming');

    if (streaming) {
        const textEl = streaming.querySelector('.bubble-text');
        const accumulated = (textEl.dataset.raw ?? '') + content;
        textEl.dataset.raw = accumulated;
        textEl.innerHTML   = renderMarkdown(accumulated) + (done ? '' : STREAM_CURSOR);
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

        const textEl = document.createElement('div');
        textEl.className   = 'bubble-text markdown-content';
        textEl.dataset.raw = content;
        textEl.innerHTML   = renderMarkdown(content) + (done ? '' : STREAM_CURSOR);

        const ts = document.createElement('div');
        ts.className   = 'timestamp';
        ts.textContent = timestamp();

        bubble.append(textEl);
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

// ── Sessions ───────────────────────────────────────────────────────────────
function sessionLabel(session) {
    return session.title && session.title.trim() ? session.title : `Chat ${session.id.slice(0, 8)}`;
}

function formatSessionDate(iso) {
    if (!iso) return '';
    const d = new Date(iso);
    return Number.isNaN(d.getTime())
        ? iso
        : d.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

function openSidebar() {
    sessionSidebar.classList.add('visible');
    sessionBackdrop.classList.add('visible');
}

function closeSidebar() {
    sessionSidebar.classList.remove('visible');
    sessionBackdrop.classList.remove('visible');
}

function toggleSidebar() {
    sessionSidebar.classList.contains('visible') ? closeSidebar() : openSidebar();
}

function renderSessionList(sessions) {
    sessionList.innerHTML = '';
    if (!sessions.length) {
        const empty = document.createElement('div');
        empty.className = 'session-empty';
        empty.textContent = 'No sessions yet.';
        sessionList.append(empty);
        return;
    }
    for (const session of sessions) {
        sessionList.append(buildSessionItem(session));
    }
}

function buildSessionItem(session) {
    const item = document.createElement('div');
    item.className = 'session-item' + (session.id === currentSessionId ? ' active' : '');
    item.dataset.id = session.id;

    const main = document.createElement('div');
    main.className = 'session-item-main';

    const title = document.createElement('div');
    title.className = 'session-item-title';
    title.textContent = sessionLabel(session);

    const meta = document.createElement('div');
    meta.className = 'session-item-meta';
    const count = session.entry_count != null ? `${session.entry_count} msg` : '';
    meta.textContent = [formatSessionDate(session.updated_at || session.created_at), count]
        .filter(Boolean)
        .join(' · ');

    main.append(title, meta);

    const renameBtn = document.createElement('button');
    renameBtn.className = 'session-item-action';
    renameBtn.title = 'Rename';
    renameBtn.textContent = '✎';
    renameBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        startRename(title, session);
    });

    const deleteBtn = document.createElement('button');
    deleteBtn.className = 'session-item-action';
    deleteBtn.title = 'Delete';
    deleteBtn.textContent = '×';
    deleteBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        confirmDeleteSession(session);
    });

    const actions = document.createElement('div');
    actions.className = 'session-item-actions';
    actions.append(renameBtn, deleteBtn);

    item.append(main, actions);
    item.addEventListener('click', () => switchToSession(session.id));
    return item;
}

function startRename(titleEl, session) {
    const input = document.createElement('input');
    input.className = 'session-item-title-input';
    input.value = session.title || '';
    input.addEventListener('click', (e) => e.stopPropagation());

    let settled = false;
    const restore = () => { if (input.parentElement) input.replaceWith(titleEl); };

    const commit = async () => {
        if (settled) return;
        settled = true;
        const value = input.value.trim();
        if (value && value !== session.title) {
            // A successful rename broadcasts session_list, which rebuilds the
            // sidebar -- no need to manually restore titleEl in that case.
            await invoke('rename_session', { id: session.id, title: value });
        } else {
            restore();
        }
    };

    input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') { e.preventDefault(); commit(); }
        if (e.key === 'Escape') { settled = true; restore(); }
    });
    input.addEventListener('blur', commit);

    titleEl.replaceWith(input);
    input.focus();
    input.select();
}

async function confirmDeleteSession(session) {
    if (!window.confirm(`Delete "${sessionLabel(session)}"? This can't be undone.`)) return;
    await invoke('delete_session', { id: session.id });
}

async function switchToSession(id) {
    if (id === currentSessionId) { closeSidebar(); return; }
    await invoke('switch_session', { id });
    closeSidebar();
}

async function createNewSession() {
    await invoke('create_session', { title: null });
    closeSidebar();
}

function restoreSessionMessages(session, msgs) {
    currentSessionId = session.id;
    messages.innerHTML = '';
    hasMessages = false;
    emptyState.style.display = '';
    for (const m of msgs) {
        if (m.role === 'user') addUserMessage(m.content);
        else appendJarvisChunk(m.content, true);
    }
    sessionList.querySelectorAll('.session-item').forEach((el) => {
        el.classList.toggle('active', el.dataset.id === currentSessionId);
    });
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
    sessionsBtn.addEventListener('click', toggleSidebar);
    sessionBackdrop.addEventListener('click', closeSidebar);
    newSessionBtn.addEventListener('click', createNewSession);

    // IPC events from Rust backend
    await listen('ipc-connected',    ()      => { setConnected(true); invoke('list_sessions'); });
    await listen('ipc-disconnected', ()      => { setConnected(false); setStatus('offline'); });
    await listen('ipc-state',        (e)     => setStatus(e.payload));
    await listen('ipc-chunk',        (e)     => appendJarvisChunk(e.payload.content, e.payload.done));
    await listen('ipc-wake',         ()      => setStatus('listening'));
    await listen('ipc-confirm',      async (e) => {
        const { id, description } = e.payload;
        const approved = window.confirm('JARVIS wants to:\n\n' + description + '\n\nAllow?');
        await invoke('send_confirmation_response', { id, approved });
    });
    await listen('ipc-session-list', (e) => {
        renderSessionList(e.payload);
        if (currentSessionId === null && e.payload.length > 0) {
            switchToSession(e.payload[0].id);
        }
    });
    await listen('ipc-session-switched', (e) => {
        restoreSessionMessages(e.payload.session, e.payload.messages);
        // create_session/switch_session only reply with session_switched, not
        // a refreshed list -- pull one explicitly so a newly created session
        // (or updated_at reordering) shows up in the sidebar immediately.
        invoke('list_sessions');
    });
    await listen('ipc-session-error', (e) => window.alert('Session error: ' + e.payload));

    // Listeners are now registered, but the backend's IPC poll thread may have
    // already emitted ipc-connected/ipc-state before this point (Tauri does not
    // queue events for late listeners). Pull current truth to reconcile.
    const status = await invoke('get_status');
    setConnected(status.connected);
    setStatus(status.state);
    if (status.connected) await invoke('list_sessions');
});
