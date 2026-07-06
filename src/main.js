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

const permissionsBtn     = document.getElementById('permissions-btn');
const permissionIndicator = document.getElementById('permission-indicator');
const permissionBackdrop = document.getElementById('permission-backdrop');
const permissionPanel    = document.getElementById('permission-panel');
const permissionPanelTitle = document.getElementById('permission-panel-title');
const permissionList     = document.getElementById('permission-list');
const approveAllBtn      = document.getElementById('approve-all-btn');

let permissionFlashTimer = null;

// ── Status ─────────────────────────────────────────────────────────────────
// woken/capturing come from the daemon's formal voice state machine
// (Project-JARVIS#141) -- both render like the existing "listening" state
// since, from the user's point of view, the mic is active either way.
const STATE_COLOR = {
    idle:       '#00c8ff',
    woken:      '#00ff88',
    capturing:  '#00ff88',
    listening:  '#00ff88',
    processing: '#ffaa00',
    speaking:   '#aa44ff',
    offline:    '#ff4455',
};

const STATE_LABEL = {
    idle:       'Idle',
    woken:      'Listening',
    capturing:  'Listening',
    listening:  'Listening',
    processing: 'Processing',
    speaking:   'Speaking',
    offline:    'Offline',
};

const LISTENING_LIKE_STATES = new Set(['listening', 'woken', 'capturing']);

function setStatus(state) {
    const color = STATE_COLOR[state] ?? STATE_COLOR.offline;
    statusDot.style.background   = color;
    statusDot.style.color        = color;
    statusLabel.style.color      = color;
    statusLabel.textContent      = STATE_LABEL[state] ?? 'Offline';

    const pulsing = LISTENING_LIKE_STATES.has(state) || state === 'processing' || state === 'speaking';
    statusDot.classList.toggle('pulsing', pulsing);

    isListening = LISTENING_LIKE_STATES.has(state);
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

// ── Permission requests ────────────────────────────────────────────────────
function openPermissionPanel() {
    permissionPanel.classList.add('visible');
    permissionBackdrop.classList.add('visible');
    invoke('list_confirmations');
}

function closePermissionPanel() {
    permissionPanel.classList.remove('visible');
    permissionBackdrop.classList.remove('visible');
}

function togglePermissionPanel() {
    permissionPanel.classList.contains('visible') ? closePermissionPanel() : openPermissionPanel();
}

function flashPermissionIndicator() {
    clearTimeout(permissionFlashTimer);
    permissionIndicator.classList.remove('flash');
    void permissionIndicator.offsetWidth; // restart the fade if it's already flashing
    permissionIndicator.classList.add('flash');
    permissionFlashTimer = setTimeout(() => permissionIndicator.classList.remove('flash'), 2500);
}

function formatPermissionDate(epochSeconds) {
    if (!epochSeconds) return '';
    const d = new Date(epochSeconds * 1000);
    return Number.isNaN(d.getTime())
        ? ''
        : d.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

function buildPermissionItem(item) {
    const el = document.createElement('div');
    el.className = 'permission-item';
    el.dataset.id = item.id;

    const tools = document.createElement('div');
    tools.className = 'permission-item-tools';
    tools.textContent = item.tool_names && item.tool_names.length
        ? item.tool_names.join(', ')
        : 'Unknown tool';

    const meta = document.createElement('div');
    meta.className = 'permission-item-meta';
    meta.textContent = formatPermissionDate(item.created_at);

    const actions = document.createElement('div');
    actions.className = 'permission-item-actions';

    const approveBtn = document.createElement('button');
    approveBtn.className = 'permission-action approve';
    approveBtn.textContent = 'Approve';
    approveBtn.addEventListener('click', () => resolveConfirmation(item.id, true));

    const denyBtn = document.createElement('button');
    denyBtn.className = 'permission-action deny';
    denyBtn.textContent = 'Deny';
    denyBtn.addEventListener('click', () => resolveConfirmation(item.id, false));

    actions.append(approveBtn, denyBtn);
    el.append(tools, meta, actions);
    return el;
}

function renderPermissionList(items) {
    permissionList.innerHTML = '';
    permissionPanelTitle.textContent = items.length
        ? `Permission Requests (${items.length})`
        : 'Permission Requests';
    approveAllBtn.disabled = items.length === 0;

    if (!items.length) {
        const empty = document.createElement('div');
        empty.className = 'permission-empty';
        empty.textContent = 'No pending requests.';
        permissionList.append(empty);
        return;
    }
    for (const item of items) permissionList.append(buildPermissionItem(item));
}

async function resolveConfirmation(id, approved) {
    await invoke(approved ? 'approve_confirmation' : 'deny_confirmation', { id });
    await invoke('list_confirmations');
}

async function approveAllConfirmations() {
    await invoke('approve_all_confirmations');
    await invoke('list_confirmations');
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
    permissionsBtn.addEventListener('click', togglePermissionPanel);
    permissionBackdrop.addEventListener('click', closePermissionPanel);
    approveAllBtn.addEventListener('click', approveAllConfirmations);

    // IPC events from Rust backend
    await listen('ipc-connected',    ()      => { setConnected(true); invoke('list_sessions'); invoke('list_confirmations'); });
    await listen('ipc-disconnected', ()      => { setConnected(false); setStatus('offline'); });
    await listen('ipc-state',        (e)     => setStatus(e.payload));
    await listen('ipc-chunk',        (e)     => appendJarvisChunk(e.payload.content, e.payload.done));
    await listen('ipc-wake',         ()      => setStatus('listening'));
    await listen('ipc-confirm', () => {
        // Non-blocking by design: no window.confirm(), no toast. Just a
        // subtle cue that the Permission Requests panel has something new.
        flashPermissionIndicator();
        invoke('list_confirmations');
    });
    await listen('ipc-confirmation-list', (e) => renderPermissionList(e.payload));
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
    if (status.connected) {
        await invoke('list_sessions');
        await invoke('list_confirmations');
    }
});
