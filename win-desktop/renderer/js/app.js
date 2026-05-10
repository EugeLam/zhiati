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

async function checkAuth() {
  try {
    const userId = await invoke('get_current_user_id');
    if (userId) {
      const email = await invoke('get_current_user_email');
      currentUserEmail = email;
      loginOverlay.classList.add('hidden');
      document.getElementById('app').style.display = 'flex';
      userBar.classList.remove('hidden');
      userEmailDisplay.textContent = email || '';
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
}

async function handleLoginSubmit(e) {
  e.preventDefault();
  const email = document.getElementById('login-email').value.trim();
  const password = document.getElementById('login-password').value;
  loginError.textContent = '';
  console.log('[Login] Attempting login, email:', email, 'mode:', isRegisterMode ? 'register' : 'login');

  if (!email || !password) {
    loginError.textContent = '请输入邮箱和密码';
    return;
  }

  try {
    const cmd = isRegisterMode ? 'register' : 'login';
    console.log('[Login] Invoking:', cmd);
    const result = await invoke(cmd, { email, password });
    console.log('[Login] Success, email:', result.email);
    currentUserEmail = result.email;
    loginOverlay.classList.add('hidden');
    document.getElementById('app').style.display = 'flex';
    userBar.classList.remove('hidden');
    userEmailDisplay.textContent = result.email;
    document.getElementById('login-password').value = '';
    console.log('[Login] Loading notes...');
    await loadNotes();
    console.log('[Login] Emitting auth-changed');
    await emit('auth-changed', true);
  } catch (err) {
    console.error('[Login] Failed:', err);
    loginError.textContent = typeof err === 'string' ? err : '操作失败，请检查网络连接';
  }
}

function switchMode() {
  isRegisterMode = !isRegisterMode;
  loginForm.querySelector('button').textContent = isRegisterMode ? '注册' : '登录';
  switchLink.textContent = isRegisterMode ? '已有账号？登录' : '还没有账号？立即注册';
  loginError.textContent = '';
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
    if (String(e).includes('未登录')) {
      showLogin();
    } else {
      await showAlert('加载失败: ' + e);
    }
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
  switchLink.onclick = (e) => { e.preventDefault(); switchMode(); };

  // Setup logout
  logoutBtn.onclick = handleLogout;

  // Double-click preview to enter edit mode
  notePreview.addEventListener('dblclick', () => {
    if (!isEditMode) toggleEdit();
  });

  // Title bar controls
  const appWindow = getCurrentWebviewWindow();
  document.getElementById('titlebar-minimize').onclick = () => appWindow.minimize();
  document.getElementById('titlebar-toggle-maximize').onclick = async () => {
    const maximized = await appWindow.isMaximized();
    if (maximized) await appWindow.unmaximize();
    else await appWindow.maximize();
  };
  document.getElementById('titlebar-close').onclick = () => appWindow.close();

  await checkAuth();
}

window.addEventListener('resize', () => {
  if (isEditMode) fitEditorHeight();
});

newNoteBtn.onclick = createNewNote;
syncBtn.onclick = async () => {
  const icon = syncBtn.querySelector('.sync-icon');
  icon.classList.remove('spinning');
  void icon.offsetWidth;
  icon.classList.add('spinning');
  await loadNotes();
};
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
