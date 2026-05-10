# zt CLI — AI Agent Skill

纸条（Zhiati）CLI 工具，用于在终端中管理便签。

## 前置条件

执行 `zt` 命令前，确保后端服务已运行：

```bash
cd server && cargo run
```

## 认证

首次使用时，**询问用户**是注册新账号还是已有账号：

- 如果用户选择 **注册**：收集邮箱和密码，执行 `zt register --email <邮箱> --password <密码>`
- 如果用户选择 **已有账号**：收集邮箱和密码，执行 `zt login --email <邮箱> --password <密码>`

> 密码至少 6 个字符。不要默认生成密码，应询问用户偏好。

后续命令无需重复认证。如需查看当前登录状态：

```bash
zt whoami
```

- `register` 注册账号并自动登录
- `login` 登录并保存 token 到本地配置
- `logout` 清除本地登录凭证
- `whoami` 查看当前登录用户（邮箱 / 用户ID / 服务器地址）

## 命令速查

### list — 列出便签

```bash
zt list            # 表格输出（带编号）
zt list --json     # JSON 输出
```

表格输出会显示编号（1, 2, 3...），后续命令可直接用编号代替 UUID。

### new — 创建便签

```bash
zt new "标题" "内容"                        # 位置参数
zt new "标题" --content "内容"              # --content 参数
zt new "纯标题"                             # 只有标题，无内容
```

两种内容写法等效，位置参数更便于 AI 直接传入。

### show — 查看便签

```bash
zt show 1           # 通过编号查看
zt show <uuid>      # 通过 UUID 查看
zt show             # 不带参数 → 列出便签供选择
```

### edit — 编辑便签

```bash
zt edit 1 --title "新标题"
zt edit 1 --content "新内容"
zt edit 1 --title "新标题" --content "新内容"
zt edit <uuid> --content "新内容"
```

必须至少指定 `--title` 或 `--content` 之一。

### delete — 删除便签

```bash
zt delete 1         # 通过编号删除
zt delete <uuid>    # 通过 UUID 删除
```

删除会同时移除关联的附件（S3 对象）。

### search — 搜索便签

```bash
zt search "关键词"
```

在标题和内容中进行模糊匹配。

### sync — 同步便签

```bash
zt sync
```

获取全量便签列表并标记同步时间。

### export — 导出便签

```bash
zt export                          # 默认 JSON 格式输出到终端
zt export --format json --output notes.json   # 导出到文件
zt export --format md --output notes.md       # Markdown 格式
```

### import — 导入便签

```bash
zt import --file notes.json
```

文件需为导出的 JSON 格式，每条记录需包含 `id`（UUID）、`user_id`、`title`、`is_pinned`、`is_archived` 等字段。

### whoami — 查看当前用户

```bash
zt whoami
```

显示邮箱、用户 ID、服务器地址。未登录时报错提示先运行 `zt login`。

### 全局参数

```bash
zt --server http://your-server:8080 list
```

`--server` 可加在任何命令中覆盖默认服务器地址。

## 常见工作流

### 编辑便签：先查看现有内容，再更新局部，然后全量写入

`zt edit --content` 是**覆盖写入**，不是追加。正确做法是先 `zt show` 获取现有内容，在内存中修改后，将完整内容写回：

```bash
# 1. 查看便签现有内容
zt show 1

# 2. 假设返回的 content 为:
# ## 会议记录
# - 讨论了下季度计划
#
# 3. AI 在现有内容基础上追加/修改（在内存中拼接）
#
# 4. 将完整的最终内容一次性写入
zt edit 1 --content "## 会议记录\n- 讨论了下季度计划\n- 新增：准备预算方案"
```

> 注意：如果只传入部分文本，原有内容会被覆盖丢失。务必将完整的最终内容传入 `--content`。

### 创建便签

```bash
zt new "会议记录" "讨论了下季度计划"
```

### 搜索后编辑

```bash
zt search "预算"
# 从结果中找到便签，记下编号
zt show 3
# 先查看现有内容，再追加/修改后全量写入
zt edit 3 --content "## 预算方案\n更新后的完整内容..."
```

### 删除后确认

```bash
zt delete 1       # 直接删除，无二次确认
zt list           # 确认已删除
```

### 导出备份

```bash
zt export --format json --output backup.json
```

## 错误处理

| 错误信息 | 原因 | 处理方式 |
|---------|------|---------|
| `未登录，请先运行 \`zt login\`` | 未登录或 token 过期 | 执行 `zt login` |
| `邮箱格式不正确` | email 不符合格式 | 使用正确邮箱格式 |
| `密码长度不能少于 6 个字符` | 密码太短 | 使用至少 6 位密码 |
| `标题不能为空` | 标题为空字符串 | 提供非空标题 |
| `请至少指定一个要修改的参数` | `zt edit` 未传 `--title` 或 `--content` | 至少传入一个 |
| `编号 N 超出范围` | 编号超出便签列表范围 | 先运行 `zt list` 刷新缓存 |
| `未找到便签列表缓存` | 尚未运行过 `zt list` | 先运行 `zt list` |
| `文件不存在` | import 指定的文件不存在 | 确认文件路径 |

## 注意事项

- 编号引用基于上次 `zt list` 的缓存，列表变动后需重新运行 `zt list`
- 删除操作无二次确认，不可恢复
- 导入的 JSON 需包含完整字段（id、user_id、title、content、is_pinned、is_archived、color、created_at、updated_at）
- 所有便签内容支持 Markdown 格式
