# 纸条 (Zhiati) - 备忘录软件需求说明书

## 1. 项目概述

**项目名称**: 纸条
**项目类型**: 备忘录/便签软件
**核心定位**: 拥有成熟CLI操作体验的备忘录软件，可由Agent调用，同时提供可视化界面供人类使用
**目标平台**: Windows首发，多端同步（未来扩展）
**技术栈**: PostgreSQL（云端）+ Tauri 2.0（桌面端）+ Rust（CLI/后端）

---

## 2. 功能清单

### 2.1 CLI命令 (P0)
| 命令 | 描述 | 输出格式 |
|------|------|----------|
| `zt new <title>` | 创建新便签 | JSON |
| `zt list` | 列出所有便签 | JSON |
| `zt show <id>` | 查看便签内容 | JSON |
| `zt edit <id>` | 编辑便签 | JSON |
| `zt delete <id>` | 删除便签 | JSON |
| `zt search <keyword>` | 搜索便签 | JSON |
| `zt sync` | 手动触发同步 | JSON |
| `zt export` | 导出便签 | JSON/MD |
| `zt import` | 导入便签 | JSON |
| `zt --help` | 帮助信息 | 文本 |

### 2.2 可视化界面 (P0)
- 主窗口便签列表（侧边栏）
- 便签编辑器（Markdown支持）
- 搜索栏
- 标签系统 (P1)
- 置顶功能 (P1)

### 2.3 Windows系统集成 (P0)
- 系统托盘
- 托盘菜单
- 悬浮窗口
- 全局快捷键
- 开机自启 (P1)

### 2.4 云端同步 (P0)
- 用户注册/登录
- 便签CRUD同步
- 增量同步
- 冲突处理 (P1)
- 离线支持 (P1)

### 2.5 Agent调用支持 (P0)
- CLI输出JSON格式
- 管道支持
- MCP协议 (P1)

---

## 3. 数据库设计

```sql
-- 用户表
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);

-- 便签表
CREATE TABLE notes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    content TEXT,
    is_pinned BOOLEAN DEFAULT FALSE,
    is_archived BOOLEAN DEFAULT FALSE,
    color VARCHAR(20) DEFAULT '#FFFB00',
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW(),
    synced_at TIMESTAMP
);

-- 标签表
CREATE TABLE tags (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(50) NOT NULL,
    color VARCHAR(20) DEFAULT '#808080',
    created_at TIMESTAMP DEFAULT NOW()
);

-- 便签-标签关联表
CREATE TABLE note_tags (
    note_id UUID REFERENCES notes(id) ON DELETE CASCADE,
    tag_id UUID REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (note_id, tag_id)
);

-- 提醒表
CREATE TABLE reminders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    note_id UUID NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    remind_at TIMESTAMPTZ NOT NULL,
    is_triggered BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

---

## 4. API设计

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | /api/auth/register | 用户注册 |
| POST | /api/auth/login | 用户登录 |
| GET | /api/notes | 获取所有便签 |
| POST | /api/notes | 创建便签 |
| GET | /api/notes/:id | 获取单个便签 |
| PUT | /api/notes/:id | 更新便签 |
| DELETE | /api/notes/:id | 删除便签 |
| POST | /api/notes/sync | 批量同步 |
| GET | /api/reminders | 获取提醒列表 |
| POST | /api/reminders | 创建提醒 |
| PUT | /api/reminders/:id | 更新提醒 |
| DELETE | /api/reminders/:id | 删除提醒 |
| POST | /api/reminders/:id/trigger | 标记提醒已触发 |

---

## 5. 项目结构

```
zhiati/
├── docs/
│   ├── SPEC.md
│   └── FUTURE_FEATURES.md
├── server/                  # Rust后端服务 (axum)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── routes/
│       │   ├── mod.rs
│       │   ├── auth.rs
│       │   ├── notes.rs
│       │   └── reminders.rs
│       └── middleware/
├── win-desktop/             # Tauri桌面应用 (Windows)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── commands.rs
│   │   ├── tray.rs
│   │   ├── scheduler.rs
│   │   ├── notification.rs
│   │   ├── auth.rs
│   │   └── config.rs
│   └── renderer/            # 前端 (Vite + vanilla JS)
│       ├── index.html
│       ├── js/
│       │   └── app.js
│       └── css/
│           └── style.css
├── shared/                  # 共享类型
│   └── src/
│       └── lib.rs
└── target/                  # 构建产物
```

---

## 6. MVP验证标准

1. CLI命令可在终端正常运行
2. Web界面可正常显示和编辑便签
3. 托盘图标可正常显示和交互
4. 数据可正确同步到PostgreSQL

---

*文档版本: v0.2*
*更新日期: 2026-04-26*
