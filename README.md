# 纸条 (Zhiati)

拥有成熟 CLI 操作体验的备忘录软件，可由 AI Agent 调用，同时提供可视化桌面界面供人类使用。

## 架构

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│   CLI (zt)  │────▶│  后端服务     │◀────│  桌面应用    │
│  Rust+Clap  │     │  Rust+Axum   │     │  Tauri 2.0  │
└─────────────┘     └──────┬───────┘     └─────────────┘
                           │
                    ┌──────▼───────┐
                    │  PostgreSQL  │
                    └──────────────┘
```

- **CLI** 和 **桌面应用** 都通过 REST API 与后端通信，不直连数据库
- 鉴权使用 JWT Bearer Token
- 共享类型定义在 `shared/` crate 中

## 功能

| 功能 | CLI | 桌面 |
|------|:---:|:----:|
| 创建/查看/编辑/删除便签 | ✓ | ✓ |
| Markdown 编辑与预览 | - | ✓ |
| 搜索便签 | ✓ | ✓ |
| 置顶/归档 | ✓ | ✓ |
| 导入/导出 | ✓ | - |
| 系统托盘 + 全局快捷键 | - | ✓ |
| 悬浮小窗 | - | ✓ |

## 项目结构

```
zhiati/
├── docs/                # 文档
├── shared/              # 共享类型 (Rust crate)
├── server/              # 后端 API 服务
├── cli/                 # CLI 工具 (zt)
└── win-desktop/         # Windows 桌面应用
    ├── src/             # Rust 后端 (Tauri commands)
    └── renderer/        # 前端 (HTML/CSS/JS + Vite)
```

## 快速开始

### 环境要求

- Rust 1.70+
- Node.js 18+
- PostgreSQL 15+

### 1. 启动后端服务

```bash
cd server
cp .env.example .env
# 编辑 .env，填写 DATABASE_URL 和 JWT_SECRET
cargo run
# 服务启动在 http://localhost:8080
```

### 2. CLI 使用

```bash
cd cli
cargo install --path .

# 注册 / 登录
zt register --email test@example.com --password 123456
zt login --email test@example.com --password 123456

# 创建便签
zt new "购物清单" --content "- 牛奶\n- 面包"

# 查看列表
zt list

# 编辑
zt edit <id> --title "新标题" --content "新内容"

# 搜索
zt search "关键词"

# 导出 / 导入
zt export --format json --output notes.json
zt import --file notes.json

# 指定服务器地址
zt --server http://your-server:8080 list
```

### 3. 桌面应用

```bash
cd win-desktop/renderer
npm install

# 返回 win-desktop 根目录
cd ..
cargo tauri dev
```

## API 概览

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/auth/register` | 注册 |
| POST | `/api/auth/login` | 登录 |
| GET | `/api/notes` | 便签列表 |
| POST | `/api/notes` | 创建便签 |
| GET | `/api/notes/:id` | 查看便签 |
| PUT | `/api/notes/:id` | 更新便签 |
| DELETE | `/api/notes/:id` | 删除便签 |
| POST | `/api/notes/sync` | 批量同步 |

所有业务接口需携带 `Authorization: Bearer <token>` 请求头。

响应格式统一为：

```json
{
  "success": true,
  "data": { ... },
  "error": null
}
```

## 技术栈

- **后端**: Rust + Axum + SQLx + PostgreSQL
- **CLI**: Rust + Clap
- **桌面**: Tauri 2.0 + HTML/CSS/JS + EasyMDE + Vite
- **鉴权**: JWT (jsonwebtoken) + bcrypt

## 许可证

MIT
