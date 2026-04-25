let invoke, emit, listen, getCurrentWindow, getWebviewWindow;

async function waitForTauri() {
  console.log('[Mini] Waiting for Tauri...');
  while (!window.__TAURI__) {
    await new Promise(r => setTimeout(r, 100));
  }
  console.log('[Mini] Tauri ready');
  invoke = window.__TAURI__.core.invoke;
  emit = window.__TAURI__.event.emit;
  listen = window.__TAURI__.event.listen;
  getCurrentWindow = window.__TAURI__.window.getCurrentWindow;
  getWebviewWindow = window.__TAURI__.window.getWebviewWindow;
}

let notes = [];
let isPinned = false;

window.miniWindowNotes = {
  getNotes: () => notes,
  setNotes: (newNotes) => {
    notes = newNotes;
    renderNotes();
    emit('notes-updated', { notes: notes });
  }
};

async function loadNotes() {
  try {
    const result = await invoke('get_notes');
    notes = result || [];
    console.log('[Mini] Loaded notes:', notes.length);
    renderNotes();
  } catch (e) {
    console.error('[Mini] Failed to load notes:', e);
  }
}

function renderNotes(filter = '') {
  const list = document.getElementById('mini-list');
  if (!list) return;

  const filtered = notes.filter(n =>
    n.title.toLowerCase().includes(filter.toLowerCase())
  );

  if (filtered.length === 0) {
    list.innerHTML = '<div class="mini-empty">暂无便签</div>';
    return;
  }

  list.innerHTML = filtered.map(n => `
    <div class="mini-item ${n.is_pinned ? 'pinned' : ''}" data-id="${n.id}">
      <div class="mini-item-title">${escapeHtml(n.title)}</div>
    </div>
  `).join('');

  list.querySelectorAll('.mini-item').forEach(item => {
    item.onclick = () => openMainWindow(item.dataset.id);
  });
}

function escapeHtml(text) {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

async function openMainWindow(noteId) {
  try {
    const mainWindow = await getWebviewWindow('main');
    if (mainWindow) {
      await mainWindow.show();
      await mainWindow.setFocus();
      await emit('open-note', noteId);
    }
  } catch (e) {
    console.error('[Mini] Failed to open main window:', e);
  }
}

async function hideMiniWindow() {
  console.log('[Mini] Hide button clicked');
  try {
    const cw = getCurrentWindow();
    await cw.hide();
    console.log('[Mini] Window hidden successfully');
  } catch (e) {
    console.error('[Mini] Failed to hide window:', e);
  }
}

async function togglePin() {
  console.log('[Mini] Pin button clicked, current state:', isPinned);
  try {
    isPinned = !isPinned;
    const btn = document.getElementById('btn-pin');
    btn.classList.toggle('active', isPinned);
    const result = await invoke('toggle_always_on_top', { windowLabel: 'mini' });
    console.log('[Mini] Toggle always on top result:', result);
  } catch (e) {
    console.error('[Mini] Failed to toggle pin:', e);
  }
}

async function setBottom() {
  console.log('[Mini] Bottom button clicked');
  try {
    await invoke('set_window_level', { windowLabel: 'mini', level: 'bottom' });
    console.log('[Mini] Set to bottom');
  } catch (e) {
    console.error('[Mini] Failed to set bottom:', e);
  }
}

document.getElementById('btn-close').onclick = hideMiniWindow;
document.getElementById('btn-pin').onclick = togglePin;
document.getElementById('btn-bottom').onclick = setBottom;
document.getElementById('mini-search-input').oninput = (e) => renderNotes(e.target.value);

const header = document.querySelector('.mini-header');
let isDragging = false;
let startX, startY;

header.addEventListener('mousedown', (e) => {
  if (e.target.classList.contains('mini-btn')) return;
  isDragging = true;
  startX = e.screenX;
  startY = e.screenY;
});

document.addEventListener('mousemove', (e) => {
  if (!isDragging) return;
  const cw = getCurrentWindow();
  cw.outerPosition({ x: e.screenX - startX, y: e.screenY - startY });
});

document.addEventListener('mouseup', () => {
  isDragging = false;
});

waitForTauri().then(() => {
  loadNotes();
  listen('notes-updated', (event) => {
    console.log('[Mini] Received notes-updated event:', event.payload);
    loadNotes();
  });
});

console.log('[Mini] Mini mode script loaded');
