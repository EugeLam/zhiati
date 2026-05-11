import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import EasyMDE from 'easymde';
import { marked } from 'marked';

// Global error handler - visible in webview devtools console
window.addEventListener('error', (e) => {
  console.error('[Global Error]', e.message, 'at', e.filename + ':' + e.lineno + ':' + e.colno, e.error);
});

window.addEventListener('unhandledrejection', (e) => {
  console.error('[Unhandled Rejection]', e.reason);
});

// --- Image path resolution ---

/** Convert markdown content: resolve 'attachments/...' relative paths to asset:// URLs.
    S3 URLs (http/https) pass through unchanged. */
async function resolveMarkdownImages(markdown) {
  const imagePattern = /!\[([^\]]*)\]\((attachments\/[^)]+)\)/g;
  const replacements = [];
  let match;

  while ((match = imagePattern.exec(markdown)) !== null) {
    const relativePath = match[2];
    try {
      const absPath = await invoke('resolve_attachment_path', { path: relativePath });
      const assetUrl = convertFileSrc(absPath);
      replacements.push({ full: match[0], alt: match[1], url: assetUrl });
    } catch (e) {
      // File not found — keep original, error handler will try to download
    }
  }

  let resolved = markdown;
  for (const r of replacements) {
    resolved = resolved.replace(r.full, `![${r.alt}](${r.url})`);
  }
  return resolved;
}

/** Post-render: fix any <img> tags with 'attachments/' src in the DOM */
/** Check if a src attribute is a relative attachment path */
function isAttachmentPath(src) {
  return src && src.startsWith('attachments/');
}

async function fixPreviewImages() {
  const images = notePreview.querySelectorAll('img');
  for (const img of images) {
    const src = img.getAttribute('src');
    if (src && isAttachmentPath(src)) {
      try {
        const dataUrl = await invoke('read_attachment_as_data_url', { path: src });
        img.src = dataUrl;
      } catch (e) {
        // Not found — handleImageError will attempt cloud download
      }
    }
  }
}

/** Render markdown into notePreview, then resolve image paths */
async function renderPreview(markdown) {
  notePreview.innerHTML = marked.parse(markdown) || '<p class="preview-empty">暂无内容</p>';
  await fixPreviewImages();
}

let notes = [];
let currentNote = null;
let currentUserEmail = null;
let isEditMode = false;
let isContentDirty = false;
let easyMDE = null;
let savedContent = '';
let isActionLoading = false;

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

// Bind dialog elements
const bindDialogOverlay = document.getElementById('bind-dialog-overlay');
const bindDialogCancelBtn = document.getElementById('bind-dialog-cancel-btn');
const bindDialogBindBtn = document.getElementById('bind-dialog-bind-btn');
const bindDialogRegisterBtn = document.getElementById('bind-dialog-register-btn');

// Settings panel elements (needed by handleLoginSubmit after binding)
let localModeToggle = null;
let cloudSettingsSection = null;

let isRegisterMode = false;
let loginMode = 'local_setup'; // 'local_setup' | 'cloud_bind' | 'cloud_register'

// --- Image Cache & Fallback ---

// Track images already processed to avoid redundant downloads
const imageCache = new Set();

/** Check if URL is a cloud/remote URL (http/https) */
function isCloudUrl(url) {
  return url && (url.startsWith('http://') || url.startsWith('https://'));
}

/** Check if URL is a local file:// URL */
function isLocalFileUrl(url) {
  return url && url.startsWith('file://');
}

/** Extract note_id and filename from an S3 URL like
    http://127.0.0.1:9000/bucket/attachments/{user_id}/{uuid}.ext
    Returns { noteId: null, filename: 'uuid.ext' } (note_id from note context) */
function extractImageInfo(url) {
  try {
    const urlObj = new URL(url);
    const parts = urlObj.pathname.split('/').filter(Boolean);
    const filename = parts[parts.length - 1] || 'image.png';
    // note_id comes from current note context, not the URL
    return { filename };
  } catch {
    return { filename: 'image.png' };
  }
}

/** Handle a failed <img> load — download from cloud to local cache */
async function handleImageError(img) {
  const src = img.getAttribute('src');
  if (!src || !isCloudUrl(src)) return;
  if (imageCache.has(src)) return;
  imageCache.add(src);

  const { filename } = extractImageInfo(src);
  const noteId = currentNote?.id;
  if (!noteId) return;

  try {
    console.log('[Image] Downloading to cache:', src);
    const rawPath = await invoke('download_attachment', {
      url: src,
      noteId,
      filename,
    });
    img.src = convertFileSrc(rawPath);
    console.log('[Image] Cached locally:', img.src);
  } catch (err) {
    console.warn('[Image] Failed to download:', src, err);
  }
}

/** Set up image error interception on the preview container (capture phase) */
function initImageFallback() {
  notePreview.addEventListener('error', (e) => {
    if (e.target.tagName === 'IMG') {
      handleImageError(e.target);
    }
  }, true); // capture phase
}

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
  loginMode = 'local_setup';
  loginOverlay.classList.remove('hidden');
  document.getElementById('app').style.display = 'none';
  userBar.classList.add('hidden');
  notes = [];
  currentNote = null;
  renderNotes();
  editor.classList.add('hidden');
  welcome.classList.remove('hidden');
  const formTitle = document.querySelector('#login-overlay h2');
  if (formTitle) formTitle.textContent = '设置本地账号';
  const submitBtn = loginForm.querySelector('button');
  if (submitBtn) submitBtn.textContent = '开始使用';
  const loginHint = document.getElementById('login-hint-text');
  if (loginHint) loginHint.style.display = '';
  if (switchLink) switchLink.style.display = 'none';
  loginError.textContent = '';
  document.getElementById('login-email').value = '';
  document.getElementById('login-password').value = '';
  updateSyncButtonText();
}

async function handleLoginSubmit(e) {
  e.preventDefault();
  const email = document.getElementById('login-email').value.trim();
  const password = document.getElementById('login-password').value;
  loginError.textContent = '';
  console.log('[Login] Mode:', loginMode, 'email:', email);

  if (!email || !password) {
    loginError.textContent = '请输入邮箱和密码';
    return;
  }

  if (isActionLoading) return;
  isActionLoading = true;
  const submitBtn = loginForm.querySelector('button');
  const originalText = submitBtn.textContent;
  submitBtn.disabled = true;
  submitBtn.textContent = '处理中...';

  try {
    if (loginMode === 'cloud_bind') {
      await invoke('bind_cloud_account', { email, password });
      // Enable cloud mode since user was trying to switch to cloud
      await invoke('toggle_cloud', { enabled: true });
      loginOverlay.classList.add('hidden');
      bindDialogOverlay.classList.add('hidden');
      document.getElementById('app').style.display = 'flex';
      appMode = await invoke('get_app_mode');
      currentUserEmail = email;
      userBar.classList.remove('hidden');
      userEmailDisplay.textContent = email;
      updateSyncButtonText();
      await syncNotes();
      await emit('auth-changed', true);
      // Sync local mode toggle to reflect cloud enabled
      if (localModeToggle) localModeToggle.checked = false;
      if (cloudSettingsSection) cloudSettingsSection.classList.toggle('hidden', false);
      return;
    }

    if (loginMode === 'cloud_register') {
      await invoke('register_and_bind', { email, password });
      // Enable cloud mode since user was trying to switch to cloud
      await invoke('toggle_cloud', { enabled: true });
      loginOverlay.classList.add('hidden');
      bindDialogOverlay.classList.add('hidden');
      document.getElementById('app').style.display = 'flex';
      appMode = await invoke('get_app_mode');
      currentUserEmail = email;
      userBar.classList.remove('hidden');
      userEmailDisplay.textContent = email;
      updateSyncButtonText();
      await syncNotes();
      await emit('auth-changed', true);
      // Sync local mode toggle to reflect cloud enabled
      if (localModeToggle) localModeToggle.checked = false;
      if (cloudSettingsSection) cloudSettingsSection.classList.toggle('hidden', false);
      return;
    }

    await invoke('setup_local_account', { email, password });
    console.log('[Login] Local account setup complete');

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
    let msg = typeof err === 'string' ? err : '操作失败，请检查网络连接';
    if (msg.includes('Invalid credentials') || msg.includes('Unauthorized')) {
      msg = '邮箱或密码错误';
    } else if (msg.includes('Failed to connect') || msg.includes('无法连接')) {
      msg = '无法连接到服务器，请检查网络';
    } else if (msg.includes('email')) {
      msg = '该邮箱已被注册';
    }
    loginError.textContent = msg;
  } finally {
    isActionLoading = false;
    submitBtn.disabled = false;
    submitBtn.textContent = originalText;
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

// --- Cloud Account Binding ---

function showBindDialog() {
  bindDialogOverlay.classList.remove('hidden');
}

function hideBindDialog() {
  bindDialogOverlay.classList.add('hidden');
}

function showCloudLogin() {
  hideBindDialog();
  loginMode = 'cloud_bind';
  const formTitle = document.querySelector('#login-overlay h2');
  if (formTitle) formTitle.textContent = '绑定云端账户';
  const submitBtn = loginForm.querySelector('button');
  if (submitBtn) submitBtn.textContent = '绑定';
  const loginHint = document.getElementById('login-hint-text');
  if (loginHint) loginHint.style.display = 'none';
  if (switchLink) {
    switchLink.style.display = '';
    switchLink.textContent = '立即注册';
    switchLink.id = 'switch-to-register';
    switchLink.onclick = (e) => { e.preventDefault(); showCloudRegister(); };
  }
  loginError.textContent = '';
  document.getElementById('login-email').value = '';
  document.getElementById('login-password').value = '';
  loginOverlay.classList.remove('hidden');
  document.getElementById('login-email').focus();
}

function showCloudRegister() {
  hideBindDialog();
  loginMode = 'cloud_register';
  const formTitle = document.querySelector('#login-overlay h2');
  if (formTitle) formTitle.textContent = '注册云端账户';
  const submitBtn = loginForm.querySelector('button');
  if (submitBtn) submitBtn.textContent = '注册并绑定';
  const loginHint = document.getElementById('login-hint-text');
  if (loginHint) loginHint.style.display = 'none';
  if (switchLink) {
    switchLink.style.display = '';
    switchLink.textContent = '立即登录';
    switchLink.id = 'switch-to-login';
    switchLink.onclick = (e) => { e.preventDefault(); showCloudLogin(); };
  }
  document.getElementById('login-email').value = '';
  document.getElementById('login-password').value = '';
  loginOverlay.classList.remove('hidden');
  document.getElementById('login-email').focus();
}

async function loadNotes() {
  try {
    const result = await invoke('get_notes');
    notes = result || [];
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

async function togglePreviewPanel() {
  const showingPreview = !editorBody.classList.contains('show-preview');
  if (showingPreview) {
    await renderPreview(easyMDE.value());
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
    // Live update custom preview (fire-and-forget for image resolution)
    renderPreview(currentContent);
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
    // Always use local relative path in markdown — renderPreview resolves to data URL
    const mdPath = result.local_path || result.url;
    const markdown = `![${result.filename}](${mdPath})`;
    if (easyMDE) {
      easyMDE.codemirror.replaceSelection(markdown + '\n');
    }
    // Re-render preview to resolve the new image
    await renderPreview(easyMDE.value());
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

async function setEditMode() {
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
  await renderPreview(savedContent);
  editBtn.classList.add('hidden');
  exitEditBtn.classList.remove('hidden');
  updateSaveBtn();
  setTimeout(() => {
    fitEditorHeight();
    easyMDE.codemirror.focus();
  }, 100);
}

async function setPreviewMode() {
  isEditMode = false;
  // Move preview back to editor body
  editorBody.appendChild(notePreview);
  destroyEasyMDE();
  editorBody.classList.remove('edit-mode', 'show-preview');
  editorBody.classList.add('preview-mode');
  noteContent.classList.add('hidden');
  await renderPreview(savedContent);
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
  if (isActionLoading) return;
  if (!noteTitle.value.trim()) {
    await showAlert('请输入标题');
    return;
  }

  const content = isEditMode && easyMDE ? easyMDE.value() : savedContent;

  isActionLoading = true;
  const originalText = saveBtn.textContent;
  saveBtn.disabled = true;
  saveBtn.textContent = '保存中...';

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
    destroyEasyMDE();
    setPreviewMode();
    await emit('notes-updated', { notes: notes });
  } catch (e) {
    console.error('[Main] Failed to save note:', e);
    await showAlert('保存失败: ' + e);
  } finally {
    isActionLoading = false;
    saveBtn.disabled = false;
    saveBtn.textContent = originalText;
  }
}

async function deleteCurrentNote() {
  if (isActionLoading) return;
  if (!currentNote || !currentNote.id) {
    editor.classList.add('hidden');
    welcome.classList.remove('hidden');
    currentNote = null;
    return;
  }

  if (!await showConfirm('确定要删除这个便签吗？')) return;

  isActionLoading = true;
  const originalText = deleteBtn.textContent;
  deleteBtn.disabled = true;
  deleteBtn.textContent = '删除中...';

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
  } finally {
    isActionLoading = false;
    deleteBtn.disabled = false;
    deleteBtn.textContent = originalText;
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
  try {

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

  // Setup bind dialog buttons
  bindDialogCancelBtn.onclick = hideBindDialog;
  bindDialogBindBtn.onclick = showCloudLogin;
  bindDialogRegisterBtn.onclick = showCloudRegister;

  // Double-click preview to enter edit mode
  notePreview.addEventListener('dblclick', () => {
    if (!isEditMode) toggleEdit();
  });

  // Image error fallback — download failed cloud images to local cache
  initImageFallback();

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
      // Load storage settings when storage tab is selected
      if (item.dataset.settings === 'storage') {
        loadStorageSettings();
      }
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

  // Storage settings
  const attachmentsRootInput = document.getElementById('settings-attachments-root');
  const attachmentsChangeBtn = document.getElementById('settings-attachments-change');
  const storageInfoEl = document.getElementById('settings-storage-info');

  async function loadStorageSettings() {
    const root = await invoke('get_attachments_root');
    attachmentsRootInput.value = root;
    loadStorageInfo(root);
  }

  async function loadStorageInfo(root) {
    try {
      const info = await invoke('get_attachments_storage_info', { root });
      const sizeStr = formatBytes(info.total_size);
      storageInfoEl.textContent = `附件数量: ${info.file_count} | 占用空间: ${sizeStr}`;
    } catch (e) {
      storageInfoEl.textContent = `无法获取存储信息: ${e}`;
    }
  }

  function formatBytes(bytes) {
    if (bytes === 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0) + ' ' + units[i];
  }

  attachmentsChangeBtn.onclick = async () => {
    const newRoot = await open({
      directory: true,
      multiple: false,
      title: '选择新的附件保存位置',
    });
    if (!newRoot) return;

    try {
      const confirmed = await showConfirm(`确定要将附件保存位置更改为:\n${newRoot}\n\n已有附件将自动迁移到新位置。`);
      if (!confirmed) return;

      attachmentsChangeBtn.disabled = true;
      attachmentsChangeBtn.textContent = '迁移中...';
      await invoke('set_attachments_root', { newRoot });
      await showAlert('更改成功，附件已迁移到新位置');
      await loadStorageSettings();
    } catch (e) {
      await showAlert('更改失败: ' + e);
    } finally {
      attachmentsChangeBtn.disabled = false;
      attachmentsChangeBtn.textContent = '更改';
    }
  };

  // Local mode toggle
  localModeToggle = document.getElementById('settings-local-mode-toggle');
  cloudSettingsSection = document.getElementById('cloud-settings-section');

  async function loadLocalModeState() {
    const mode = await invoke('get_app_mode');
    // local_mode is the inverse of cloud_enabled
    localModeToggle.checked = !mode.cloud_enabled;
    cloudSettingsSection.classList.toggle('hidden', !mode.cloud_enabled);
  }

  localModeToggle.onchange = async () => {
    try {
      if (!localModeToggle.checked) {
        // Enabling cloud mode - check if cloud account is bound
        const currentMode = await invoke('get_app_mode');
        if (!currentMode.cloud_account_bound) {
          // Not bound, show bind dialog and revert toggle
          localModeToggle.checked = true;
          showBindDialog();
          return;
        }
        // Already bound - enable cloud and sync
        await invoke('toggle_cloud', { enabled: true });
        cloudSettingsSection.classList.toggle('hidden', false);
        updateSyncButtonText();
        await syncNotes();
      } else {
        // Disabling cloud mode (enabling local mode)
        await invoke('toggle_cloud', { enabled: false });
        cloudSettingsSection.classList.toggle('hidden', true);
        updateSyncButtonText();
      }
    } catch (e) {
      await showAlert('切换失败: ' + e);
      localModeToggle.checked = !localModeToggle.checked;
    }
  };

  loadLocalModeState();

  await checkAuth();
  } catch (e) {
    console.error('[Main] Init error:', e);
  }
}

window.addEventListener('resize', () => {
  if (isEditMode) fitEditorHeight();
});

newNoteBtn.onclick = createNewNote;

async function syncNotes() {
  if (isActionLoading) return;
  isActionLoading = true;
  const icon = syncBtn.querySelector('.sync-icon');
  icon.classList.remove('spinning');
  void icon.offsetWidth;
  icon.classList.add('spinning');
  syncBtn.disabled = true;
  try {
    const result = await invoke('sync_notes');
    notes = result || [];
    renderNotes(searchInput.value);
  } finally {
    isActionLoading = false;
    syncBtn.disabled = false;
  }
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
