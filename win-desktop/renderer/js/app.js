import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import EasyMDE from 'easymde';
import { marked } from 'marked';

let notes = [];
let currentNote = null;
let currentUserEmail = null;
let isEditMode = false;
let isContentDirty = false;
let easyMDE = null;
let savedContent = '';

const notesList = document.getElementById('notes-list');
const editor = document.getElementById('editor');
const editorBody = document.getElementById('editor-body');
const welcome = document.getElementById('welcome');
const noteTitle = document.getElementById('note-title');
const noteContent = document.getElementById('note-content');
const notePreview = document.getElementById('note-preview');
const newNoteBtn = document.getElementById('new-note-btn');
const saveBtn = document.getElementById('save-btn');
const editBtn = document.getElementById('edit-btn');
const exitEditBtn = document.getElementById('exit-edit-btn');
const deleteBtn = document.getElementById('delete-btn');
const searchInput = document.getElementById('search-input');
const loginOverlay = document.getElementById('login-overlay');
const loginForm = document.getElementById('login-form');
const loginError = document.getElementById('login-error');
const switchLink = document.getElementById('switch-to-register');
const userBar = document.getElementById('user-bar');
const userEmailDisplay = document.getElementById('user-email-display');
const logoutBtn = document.getElementById('logout-btn');
const syncBtn = document.getElementById('sync-btn');
const reminderBtn = document.getElementById('reminder-btn');
const reminderPanel = document.getElementById('reminder-panel');
const reminderList = document.getElementById('reminder-list');
const reminderDatetime = document.getElementById('reminder-datetime');
const reminderAddBtn = document.getElementById('reminder-add-btn');
const reminderCloseBtn = document.getElementById('reminder-close-btn');

let isRegisterMode = false;

// --- Custom Dialog ---

const dialogOverlay = document.getElementById('app-dialog-overlay');
const dialogMessage = document.getElementById('dialog-message');
const dialogCancelBtn = document.getElementById('dialog-cancel-btn');
const dialogConfirmBtn = document.getElementById('dialog-confirm-btn');
let dialogResolve = null;

function showConfirm(message) {
  return new Promise((resolve) => {
    dialogResolve = resolve;
    dialogMessage.textContent = message;
    dialogCancelBtn.classList.remove('hidden');
    dialogOverlay.classList.remove('hidden');
    dialogConfirmBtn.focus();
  });
}

function showAlert(message) {
  return new Promise((resolve) => {
    dialogResolve = resolve;
    dialogMessage.textContent = message;
    dialogCancelBtn.classList.add('hidden');
    dialogOverlay.classList.remove('hidden');
    dialogConfirmBtn.focus();
  });
}

function closeDialog(result) {
  dialogOverlay.classList.add('hidden');
  if (dialogResolve) {
    dialogResolve(result);
    dialogResolve = null;
  }
}

dialogConfirmBtn.onclick = () => closeDialog(true);
dialogCancelBtn.onclick = () => closeDialog(false);

// --- Auth ---

let appMode = null; // { cloud_enabled, is_cloud_connected, local_account_exists }

async function checkAuth() {
  try {
    appMode = await invoke('get_app_mode');
    console.log('[Auth] App mode:', appMode);

    if (appMode.local_account_exists) {
      // Local account exists, enter app directly
      const email = await invoke('get_current_user_email');
      currentUserEmail = email;
      loginOverlay.classList.add('hidden');
      document.getElementById('app').style.display = 'flex';
      if (appMode.is_cloud_connected) {
        userBar.classList.remove('hidden');
        userEmailDisplay.textContent = email || '';
      } else {
        userBar.classList.add('hidden');
      }
      updateSyncButtonText();
      await loadNotes();
      return true;
    }
  } catch (e) {
    console.log('[Auth] Not authenticated:', e);
  }
  showLogin();
  return false;
}

function showLogin() {
  loginOverlay.classList.remove('hidden');
  document.getElementById('app').style.display = 'none';
  userBar.classList.add('hidden');
  notes = [];
  currentNote = null;
  renderNotes();
  editor.classList.add('hidden');
  welcome.classList.remove('hidden');
  // Change form to local account setup
  const formTitle = document.querySelector('#login-overlay h2');
  if (formTitle) formTitle.textContent = '设置本地账号';
  const submitBtn = loginForm.querySelector('button');
  if (submitBtn) submitBtn.textContent = '开始使用';
  switchLink.parentElement.innerHTML = '<span class="login-hint">账号信息仅保存在本地，安全加密存储</span>';
  updateSyncButtonText();
}

async function handleLoginSubmit(e) {
  e.preventDefault();
  const email = document.getElementById('login-email').value.trim();
  const password = document.getElementById('login-password').value;
  loginError.textContent = '';
  console.log('[Login] Attempting local account setup, email:', email);

  if (!email || !password) {
    loginError.textContent = '请输入邮箱和密码';
    return;
  }

  try {
    // First setup local account
    await invoke('setup_local_account', { email, password });
    console.log('[Login] Local account setup complete');

    // Now check if we got cloud connected during startup
    appMode = await invoke('get_app_mode');
    currentUserEmail = email;
    loginOverlay.classList.add('hidden');
    document.getElementById('app').style.display = 'flex';
    if (appMode.is_cloud_connected) {
      userBar.classList.remove('hidden');
      userEmailDisplay.textContent = email;
    } else {
      userBar.classList.add('hidden');
    }
    document.getElementById('login-password').value = '';
    updateSyncButtonText();
    await loadNotes();
    await emit('auth-changed', true);
  } catch (err) {
    console.error('[Login] Failed:', err);
    loginError.textContent = typeof err === 'string' ? err : '操作失败，请检查网络连接';
  }
}

function updateSyncButtonText() {
  const icon = syncBtn.querySelector('.sync-icon');
  if (appMode && appMode.cloud_enabled) {
    syncBtn.title = '同步便签';
    syncBtn.innerHTML = '<span class="sync-icon">&#x21bb;</span> 同步';
  } else {
    syncBtn.title = '刷新便签';
    syncBtn.innerHTML = '<span class="sync-icon">&#x21bb;</span> 刷新';
  }
}

async function handleLogout() {
  if (!await showConfirm('确定要退出登录吗？')) return;
  try {
    await invoke('logout');
    showLogin();
    await emit('notes-updated', { notes: [] });
  } catch (e) {
    console.error('[Main] Failed to logout:', e);
  }
}

async function loadNotes() {
  try {
    const result = await invoke('get_notes');
    notes = result || [];
    console.log('[Main] Loaded notes:', notes.length);
    renderNotes(searchInput.value);
  } catch (e) {
    console.error('[Main] Failed to load notes:', e);
    await showAlert('加载失败: ' + e);
  }
}

function renderNotes(filter = '') {
  const filtered = notes.filter(n =>
    n.title.toLowerCase().includes(filter.toLowerCase()) ||
    (n.content && n.content.toLowerCase().includes(filter.toLowerCase()))
  );

  notesList.innerHTML = '';

  if (filtered.length === 0) {
    notesList.innerHTML = '<div class="empty-state">暂无便签</div>';
    return;
  }

  filtered.sort((a, b) => {
    if (a.is_pinned && !b.is_pinned) return -1;
    if (!a.is_pinned && b.is_pinned) return 1;
    return new Date(b.updated_at) - new Date(a.updated_at);
  });

  filtered.forEach(note => {
    const div = document.createElement('div');
    div.className = 'note-item' + (note.is_pinned ? ' pinned' : '');
    if (currentNote && currentNote.id === note.id) {
      div.classList.add('active');
    }
    div.innerHTML = `
      <h3>${escapeHtml(note.title) || '无标题'}</h3>
      <p>${escapeHtml(note.content || '')}</p>
      <div class="note-date">${formatDate(note.updated_at)}</div>
    `;
    div.onclick = () => selectNote(note);
    notesList.appendChild(div);
  });
}

// --- Editor logic ---

function togglePreviewPanel() {
  const showingPreview = !editorBody.classList.contains('show-preview');
  if (showingPreview) {
    notePreview.innerHTML = marked.parse(easyMDE.value()) || '<p class="preview-empty">暂无内容</p>';
  }
  editorBody.classList.toggle('show-preview');
  if (!showingPreview) {
    setTimeout(() => {
      fitEditorHeight();
      easyMDE.codemirror.focus();
    }, 50);
  }
}

function initEasyMDE() {
  if (easyMDE) return;
  easyMDE = new EasyMDE({
    element: noteContent,
    spellChecker: false,
    placeholder: '写点什么...',
    toolbar: ['bold', 'italic', 'heading', '|', 'quote', 'unordered-list', 'ordered-list', '|', 'link',
      {
        name: 'upload-image',
        action: handleImageUpload,
        className: 'fa fa-picture-o',
        title: '上传图片',
      },
      'code', 'table', '|',
      {
        name: 'preview-toggle',
        action: togglePreviewPanel,
        className: 'fa fa-columns',
        title: '切换预览',
      },
      '|', 'guide'],
    status: false,
  });
  easyMDE.codemirror.on('change', () => {
    const currentContent = easyMDE.value();
    if (currentContent !== savedContent) {
      isContentDirty = true;
      updateSaveBtn();
    } else {
      isContentDirty = false;
      updateSaveBtn();
    }
    // Live update custom preview
    notePreview.innerHTML = marked.parse(currentContent) || '<p class="preview-empty">暂无内容</p>';
  });
  fitEditorHeight();
}

function fitEditorHeight() {
  if (!easyMDE) return;
  const toolbar = editorBody.querySelector('.editor-toolbar');
  const toolbarHeight = toolbar ? toolbar.offsetHeight : 0;
  const availableHeight = editorBody.clientHeight - toolbarHeight;
  if (availableHeight > 0) {
    easyMDE.codemirror.setSize(null, availableHeight);
  }
}

function destroyEasyMDE() {
  if (easyMDE) {
    easyMDE.toTextArea();
    easyMDE = null;
  }
}

async function handleImageUpload(editor) {
  if (!currentNote || !currentNote.id) {
    await showAlert('请先保存便签再上传图片');
    return;
  }

  const filePath = await open({
    multiple: false,
    filters: [{
      name: 'Images',
      extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'],
    }],
  });

  if (!filePath) return;

  try {
    const result = await invoke('upload_image', { filePath, noteId: currentNote.id });
    const markdown = `![${result.filename}](${result.url})`;
    if (easyMDE) {
      easyMDE.codemirror.replaceSelection(markdown + '\n');
    }
  } catch (err) {
    console.error('[Main] Image upload failed:', err);
    await showAlert('图片上传失败: ' + (typeof err === 'string' ? err : '未知错误'));
  }
}

async function selectNote(note) {
  if (isEditMode && isContentDirty) {
    if (!await showConfirm('有未保存的更改，是否放弃？')) return;
  }
  currentNote = note;
  noteTitle.value = note.title;
  noteContent.value = note.content || '';
  savedContent = note.content || '';
  isContentDirty = false;
  destroyEasyMDE();
  setPreviewMode();
  reminderBtn.classList.remove('hidden');
  reminderPanel.classList.add('hidden');
  renderNotes(searchInput.value);
  editor.classList.remove('hidden');
  welcome.classList.add('hidden');
}

async function createNewNote() {
  if (isEditMode && isContentDirty) {
    if (!await showConfirm('有未保存的更改，是否放弃？')) return;
  }
  currentNote = {
    id: null,
    title: '',
    content: '',
    is_pinned: false,
    color: '#FFFB00',
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString()
  };
  noteTitle.value = '';
  noteContent.value = '';
  savedContent = '';
  isContentDirty = false;
  destroyEasyMDE();
  setEditMode();
  reminderBtn.classList.add('hidden');
  reminderPanel.classList.add('hidden');
  editor.classList.remove('hidden');
  welcome.classList.add('hidden');
  noteTitle.focus();
}

function setEditMode() {
  isEditMode = true;
  editorBody.classList.remove('preview-mode');
  editorBody.classList.add('edit-mode');
  noteContent.classList.remove('hidden');
  editorBody.classList.remove('show-preview');
  initEasyMDE();
  easyMDE.value(savedContent);
  // Move preview into EasyMDE container so it sits below the toolbar
  const container = easyMDE.codemirror.getWrapperElement().parentElement;
  container.appendChild(notePreview);
  notePreview.innerHTML = marked.parse(savedContent) || '<p class="preview-empty">暂无内容</p>';
  editBtn.classList.add('hidden');
  exitEditBtn.classList.remove('hidden');
  updateSaveBtn();
  setTimeout(() => {
    fitEditorHeight();
    easyMDE.codemirror.focus();
  }, 100);
}

function setPreviewMode() {
  isEditMode = false;
  // Move preview back to editor body
  editorBody.appendChild(notePreview);
  destroyEasyMDE();
  editorBody.classList.remove('edit-mode', 'show-preview');
  editorBody.classList.add('preview-mode');
  noteContent.classList.add('hidden');
  notePreview.innerHTML = marked.parse(savedContent) || '<p class="preview-empty">暂无内容</p>';
  editBtn.classList.remove('hidden');
  saveBtn.classList.add('hidden');
  exitEditBtn.classList.add('hidden');
}

function updateSaveBtn() {
  if (isContentDirty) {
    saveBtn.classList.remove('hidden');
  } else {
    saveBtn.classList.add('hidden');
  }
}

async function exitEdit() {
  if (isContentDirty) {
    if (!await showConfirm('有未保存的更改，确定退出吗？')) return;
  }
  noteContent.value = savedContent;
  isContentDirty = false;
  destroyEasyMDE();
  setPreviewMode();
}

function toggleEdit() {
  if (isEditMode) return;
  noteContent.value = savedContent;
  setEditMode();
}

async function saveNote() {
  if (!noteTitle.value.trim()) {
    await showAlert('请输入标题');
    return;
  }

  const content = isEditMode && easyMDE ? easyMDE.value() : savedContent;

  try {
    if (currentNote.id) {
      const updated = await invoke('update_note', {
        id: currentNote.id,
        title: noteTitle.value,
        content: content
      });
      const idx = notes.findIndex(n => n.id === currentNote.id);
      if (idx !== -1) {
        notes[idx] = updated;
      }
    } else {
      const newNote = await invoke('create_note', {
        title: noteTitle.value,
        content: content
      });
      notes.unshift(newNote);
      currentNote = newNote;
    }

    savedContent = content;
    isContentDirty = false;
    updateSaveBtn();
    renderNotes(searchInput.value);
    showNotification('保存成功');
    // After save, switch to preview mode
    destroyEasyMDE();
    setPreviewMode();
    await emit('notes-updated', { notes: notes });
  } catch (e) {
    console.error('[Main] Failed to save note:', e);
    await showAlert('保存失败: ' + e);
  }
}

async function deleteCurrentNote() {
  if (!currentNote || !currentNote.id) {
    editor.classList.add('hidden');
    welcome.classList.remove('hidden');
    currentNote = null;
    return;
  }

  if (!await showConfirm('确定要删除这个便签吗？')) return;

  try {
    await invoke('delete_note', { id: currentNote.id });
    notes = notes.filter(n => n.id !== currentNote.id);
    editor.classList.add('hidden');
    welcome.classList.remove('hidden');
    currentNote = null;
    destroyEasyMDE();
    renderNotes();
    showNotification('已删除');
    await emit('notes-updated', { notes: notes });
  } catch (e) {
    console.error('[Main] Failed to delete note:', e);
    await showAlert('删除失败: ' + e);
  }
}

// --- Reminders ---

async function loadReminders() {
  if (!currentNote || !currentNote.id) return;
  try {
    const reminders = await invoke('get_reminders', { noteId: currentNote.id });
    renderReminders(reminders);
  } catch (e) {
    console.error('[Main] Failed to load reminders:', e);
  }
}

function renderReminders(reminders) {
  reminderList.innerHTML = '';
  if (!reminders || reminders.length === 0) {
    reminderList.innerHTML = '<div class="reminder-empty">暂无提醒</div>';
    return;
  }

  reminders.forEach(r => {
    const item = document.createElement('div');
    item.className = 'reminder-item';
    const time = new Date(r.remind_at);
    item.innerHTML = `
      <span class="reminder-time">${time.toLocaleString('zh-CN')}</span>
      <button class="reminder-delete" data-id="${r.id}" title="删除提醒">&times;</button>
    `;
    item.querySelector('.reminder-delete').onclick = async (e) => {
      e.stopPropagation();
      await deleteReminder(r.id);
    };
    reminderList.appendChild(item);
  });
}

async function addReminder() {
  if (!currentNote || !currentNote.id) return;
  const datetime = reminderDatetime.value;
  if (!datetime) {
    await showAlert('请选择提醒时间');
    return;
  }

  try {
    await invoke('add_reminder', {
      noteId: currentNote.id,
      remindAt: new Date(datetime).toISOString(),
      noteTitle: currentNote.title || '未命名便签',
      noteContent: currentNote.content || '(无内容)'
    });
    reminderDatetime.value = '';
    await loadReminders();
  } catch (e) {
    console.error('[Main] Failed to add reminder:', e);
    await showAlert('添加提醒失败: ' + e);
  }
}

async function deleteReminder(id) {
  try {
    await invoke('delete_reminder', { id });
    await loadReminders();
  } catch (e) {
    console.error('[Main] Failed to delete reminder:', e);
  }
}

function toggleReminderPanel() {
  reminderPanel.classList.toggle('hidden');
  if (!reminderPanel.classList.contains('hidden')) {
    loadReminders();
  }
}

function escapeHtml(text) {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

function formatDate(dateStr) {
  const date = new Date(dateStr);
  const now = new Date();
  const diff = now - date;

  if (diff < 60000) return '刚刚';
  if (diff < 3600000) return Math.floor(diff / 60000) + '分钟前';
  if (diff < 86400000) return Math.floor(diff / 3600000) + '小时前';
  if (diff < 604800000) return Math.floor(diff / 86400000) + '天前';

  return date.toLocaleDateString('zh-CN');
}

function showNotification(message) {
  if ('Notification' in window && Notification.permission === 'granted') {
    new Notification('纸条', { body: message });
  }
}

async function init() {
  console.log('[Main] Initializing...');

  await listen('notes-updated', (event) => {
    if (event.payload && event.payload.notes) {
      notes = event.payload.notes;
      renderNotes(searchInput.value);
    }
  });

  await listen('open-note', async (event) => {
    const noteId = event.payload;
    console.log('[Main] open-note received:', noteId);
    if (notes.length === 0) {
      console.log('[Main] Notes not loaded yet, reloading...');
      await loadNotes();
    }
    const note = notes.find(n => n.id === noteId);
    if (note) {
      console.log('[Main] Found note, selecting:', note.title);
      selectNote(note);
    } else {
      console.log('[Main] Note not found:', noteId);
    }
  });

  await listen('auth-changed', (event) => {
    if (!event.payload) {
      showLogin();
    }
  });

  // Setup login form
  loginForm.onsubmit = handleLoginSubmit;

  // Setup logout
  logoutBtn.onclick = handleLogout;

  // Double-click preview to enter edit mode
  notePreview.addEventListener('dblclick', () => {
    if (!isEditMode) toggleEdit();
  });

  // Title bar controls
  const appWindow = getCurrentWebviewWindow();
  document.getElementById('titlebar-minimize').onclick = () => {
    // Hide main window and show mini window
    appWindow.hide();
    invoke('show_mini_window');
  };
  document.getElementById('titlebar-toggle-maximize').onclick = async () => {
    const maximized = await appWindow.isMaximized();
    if (maximized) await appWindow.unmaximize();
    else await appWindow.maximize();
  };
  document.getElementById('titlebar-close').onclick = () => appWindow.close();

  // Settings panel
  const titlebarTabs = document.getElementById('titlebar-tabs');
  const tabSettings = document.getElementById('tab-settings');
  const settingsPanel = document.getElementById('settings-panel');
  const appBody = document.querySelector('.app-body');

  // Add close button to settings tab
  const closeBtn = document.createElement('span');
  closeBtn.className = 'tab-close-btn';
  closeBtn.innerHTML = '&times;';
  closeBtn.onclick = (e) => { e.stopPropagation(); closeSettings(); };
  tabSettings.appendChild(closeBtn);

  // Open settings — show tab and panel
  document.getElementById('titlebar-settings').onclick = () => {
    titlebarTabs.classList.remove('hidden');
    settingsPanel.classList.remove('hidden');
    appBody.classList.add('hidden');
    loadNetworkSettings();
  };

  function closeSettings() {
    titlebarTabs.classList.add('hidden');
    settingsPanel.classList.add('hidden');
    appBody.classList.remove('hidden');
  }

  // Settings navigation
  settingsPanel.querySelectorAll('.settings-nav-item').forEach(item => {
    item.onclick = () => {
      settingsPanel.querySelectorAll('.settings-nav-item').forEach(i => i.classList.remove('active'));
      settingsPanel.querySelectorAll('.settings-page').forEach(p => p.classList.remove('active'));
      item.classList.add('active');
      document.getElementById(`settings-${item.dataset.settings}`).classList.add('active');
    };
  });

  // Network settings
  const serverUrlInput = document.getElementById('settings-server-url');
  const serverSaveBtn = document.getElementById('settings-server-save');
  const connectionStatus = document.getElementById('settings-connection-status');

  async function loadNetworkSettings() {
    const currentUrl = await invoke('get_server_url');
    serverUrlInput.value = currentUrl;
    checkConnection();
    // Sync cloud settings section visibility
    const mode = await invoke('get_app_mode');
    cloudSettingsSection.classList.toggle('hidden', !mode.cloud_enabled);
  }

  async function checkConnection() {
    connectionStatus.innerHTML = '<span class="status-dot checking"></span><span class="status-text">检查中...</span>';
    try {
      const resp = await fetch(serverUrlInput.value + '/health', { method: 'GET' });
      if (resp.ok) {
        connectionStatus.innerHTML = '<span class="status-dot connected"></span><span class="status-text">连接正常</span>';
      } else {
        connectionStatus.innerHTML = '<span class="status-dot disconnected"></span><span class="status-text">连接失败 (HTTP ' + resp.status + ')</span>';
      }
    } catch (e) {
      connectionStatus.innerHTML = '<span class="status-dot disconnected"></span><span class="status-text">无法连接: ' + e.message + '</span>';
    }
  }

  serverSaveBtn.onclick = async () => {
    const url = serverUrlInput.value.trim();
    if (!url) {
      await showAlert('请输入后端地址');
      return;
    }
    try {
      await invoke('set_server_url', { url });
      await showAlert('保存成功，请重新登录');
      checkConnection();
    } catch (e) {
      await showAlert('保存失败: ' + e);
    }
  };

  // Local mode toggle
  const localModeToggle = document.getElementById('settings-local-mode-toggle');

  const cloudSettingsSection = document.getElementById('cloud-settings-section');

  async function loadLocalModeState() {
    const mode = await invoke('get_app_mode');
    // local_mode is the inverse of cloud_enabled
    localModeToggle.checked = !mode.cloud_enabled;
    cloudSettingsSection.classList.toggle('hidden', !mode.cloud_enabled);
  }

  localModeToggle.onchange = async () => {
    try {
      // When local mode is checked (true), cloud should be disabled (false)
      await invoke('toggle_cloud', { enabled: !localModeToggle.checked });
      cloudSettingsSection.classList.toggle('hidden', localModeToggle.checked);
      updateSyncButtonText();
      // If disabling local mode (enabling cloud), trigger sync
      if (!localModeToggle.checked) {
        await syncNotes();
      }
    } catch (e) {
      await showAlert('切换失败: ' + e);
      localModeToggle.checked = !localModeToggle.checked;
    }
  };

  loadLocalModeState();

  await checkAuth();
}

window.addEventListener('resize', () => {
  if (isEditMode) fitEditorHeight();
});

newNoteBtn.onclick = createNewNote;

async function syncNotes() {
  const icon = syncBtn.querySelector('.sync-icon');
  icon.classList.remove('spinning');
  void icon.offsetWidth;
  icon.classList.add('spinning');
  await loadNotes();
}

syncBtn.onclick = syncNotes;
saveBtn.onclick = saveNote;
editBtn.onclick = toggleEdit;
exitEditBtn.onclick = exitEdit;
deleteBtn.onclick = deleteCurrentNote;
reminderBtn.onclick = toggleReminderPanel;
reminderAddBtn.onclick = addReminder;
reminderCloseBtn.onclick = () => reminderPanel.classList.add('hidden');

const reminderTestBtn = document.getElementById('reminder-test-btn');
reminderTestBtn.onclick = async () => {
  try {
    await invoke('test_reminder');
    console.log('[Main] test_reminder invoked successfully');
  } catch (e) {
    console.error('[Main] test_reminder failed:', e);
    await showAlert('测试失败: ' + e);
  }
};

searchInput.oninput = (e) => {
  renderNotes(e.target.value);
};

document.addEventListener('keydown', (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key === 's') {
    e.preventDefault();
    saveNote();
  }
});

window.mainWindowNotes = {
  getNotes: () => notes,
  setNotes: (newNotes) => {
    notes = newNotes;
    renderNotes(searchInput.value);
  }
};

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
