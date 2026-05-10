use crate::api;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

// --- Validation helpers ---

fn validate_email(email: &str) -> Result<(), String> {
    if email.is_empty() {
        return Err("邮箱不能为空".to_string());
    }
    // Basic email check: must contain exactly one @ with something before and after,
    // and the part after @ must contain a dot.
    let parts: Vec<&str> = email.splitn(2, '@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err("邮箱格式不正确，请使用类似 user@example.com 的格式".to_string());
    }
    if !parts[1].contains('.') {
        return Err("邮箱格式不正确，请使用类似 user@example.com 的格式".to_string());
    }
    Ok(())
}

fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < 6 {
        return Err(format!("密码长度不能少于 6 个字符（当前 {} 个字符）", password.len()));
    }
    Ok(())
}

fn validate_title(title: &str) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("标题不能为空".to_string());
    }
    Ok(())
}

// --- Note ID resolution (supports numbered index or UUID) ---

fn last_list_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("zhiati");
    if !config_dir.exists() {
        let _ = fs::create_dir_all(&config_dir);
    }
    config_dir.join("last_list.json")
}

pub fn resolve_note_id(input: &str) -> Result<String, String> {
    // If it looks like a UUID, return as-is
    if input.len() == 36 && input.contains('-') {
        if uuid::Uuid::parse_str(input).is_ok() {
            return Ok(input.to_string());
        }
        return Err(format!("便签 ID 格式不正确: '{}'，请输入有效的 UUID 或列表编号", input));
    }

    // If it's a number, resolve from cached list
    if let Ok(n) = input.parse::<usize>() {
        let path = last_list_path();
        if !path.exists() {
            return Err("未找到便签列表缓存，请先运行 `zt list`".to_string());
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("无法读取缓存: {}", e))?;
        let notes: Vec<api::Note> = serde_json::from_str(&content)
            .map_err(|_| "缓存文件已损坏，请重新运行 `zt list`".to_string())?;

        if n == 0 || n > notes.len() {
            return Err(format!("编号 {} 超出范围，当前共有 {} 条便签。请先运行 `zt list` 刷新缓存", n, notes.len()));
        }
        return Ok(notes[n - 1].id.to_string());
    }

    Err(format!("无法识别 '{}'，请输入便签 UUID 或列表编号（数字）", input))
}

// --- Interactive note selection ---

fn print_select_list(notes: &[api::Note]) {
    if notes.is_empty() {
        println!("当前没有便签，请先使用 `zt new \"标题\"` 创建");
        return;
    }

    let mut title_width = 6;
    for note in notes.iter() {
        let len = note.title.chars().count();
        if len > title_width {
            title_width = std::cmp::min(len, 40);
        }
    }
    let num_width = std::cmp::max(4, notes.len().to_string().len());

    let header = format!(" {:>w$} | {:<tw$} | {}", "#", "标题", "ID",
        w = num_width, tw = title_width);
    let uuid_w = 36;
    let separator = format!("{:-<w$}-+-{:-<tw$}-+-{:-<uuid_w$}", "", "", "",
        w = num_width, tw = title_width, uuid_w = uuid_w);
    println!("{}", header);
    println!("{}", separator);

    for (i, note) in notes.iter().enumerate() {
        let mut title_display = note.title.clone();
        if title_display.chars().count() > title_width {
            title_display = title_display.chars().take(title_width - 2).collect::<String>() + "…";
        }
        println!(" {:>w$} | {:<tw$} | {}", i + 1, title_display, note.id,
            w = num_width, tw = title_width);
    }
    println!();
}

pub async fn prompt_note_select(server_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();
    let token = config.token.as_ref()
        .ok_or("未登录，请先运行 `zt login`")?;

    let notes = api::api_list_notes(server_url, token).await?;
    let _ = fs::write(last_list_path(), serde_json::to_string(&notes)?);
    print_select_list(&notes);

    if !notes.is_empty() {
        println!("请输入编号或 UUID 查看便签，例如: zt show 1");
    }
    Ok(())
}

pub async fn prompt_edit_select(server_url: &str, title: Option<&str>, content: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    if title.is_none() && content.is_none() {
        return Err("请至少指定一个要修改的参数：--title 或 --content".to_string().into());
    }

    let config = api::get_config();
    let token = config.token.as_ref()
        .ok_or("未登录，请先运行 `zt login`")?;

    let notes = api::api_list_notes(server_url, token).await?;
    let _ = fs::write(last_list_path(), serde_json::to_string(&notes)?);
    print_select_list(&notes);

    if !notes.is_empty() {
        println!("请输入编号或 UUID 编辑便签，例如: zt edit 1 --title \"新标题\"");
    }
    Ok(())
}

pub async fn prompt_delete_select(server_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();
    let token = config.token.as_ref()
        .ok_or("未登录，请先运行 `zt login`")?;

    let notes = api::api_list_notes(server_url, token).await?;
    let _ = fs::write(last_list_path(), serde_json::to_string(&notes)?);
    print_select_list(&notes);

    if !notes.is_empty() {
        println!("请输入编号或 UUID 删除便签，例如: zt delete 1");
    }
    Ok(())
}

// --- Commands ---

pub async fn new_note(server_url: &str, title: &str, content: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    validate_title(title)?;

    let config = api::get_config();
    let token = config.token.as_ref()
        .ok_or("未登录，请先运行 `zt login`")?;

    let note = api::api_create_note(server_url, token, title, content).await?;
    println!("{}", serde_json::to_string_pretty(&api::CliNoteOutput::from(note))?);
    Ok(())
}

pub async fn list_notes(server_url: &str, as_json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();
    let token = config.token.as_ref()
        .ok_or("未登录，请先运行 `zt login`")?;

    let notes = api::api_list_notes(server_url, token).await?;

    // Cache for numbered index resolution
    let _ = fs::write(last_list_path(), serde_json::to_string(&notes)?);

    if as_json {
        let output: Vec<api::CliNoteOutput> = notes.into_iter().map(|n| n.into()).collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        if notes.is_empty() {
            println!("没有便签");
            return Ok(());
        }

        // Calculate column widths
        let num_width = std::cmp::max(4, notes.len().to_string().len());
        let mut title_width = 6; // "标题" width
        let uuid_width = 36;

        for (i, note) in notes.iter().enumerate() {
            let idx = (i + 1).to_string().len();
            title_width = std::cmp::max(title_width, note.title.chars().count());
            let _ = (idx, note);
        }
        // Cap title width at 40 for readability
        title_width = std::cmp::min(title_width, 40);

        let header = format!(" {:>w$} | {:<tw$} | {}", "#", "标题", "ID",
            w = num_width, tw = title_width);
        let separator = format!("{:-<w$}-+-{:-<tw$}-+-{:-<uuid_width$}", "", "", "",
            w = num_width, tw = title_width, uuid_width = uuid_width);

        println!("{}", header);
        println!("{}", separator);

        for (i, note) in notes.iter().enumerate() {
            let num = format!("{}", i + 1);
            let mut title_display = note.title.clone();
            let title_chars = title_display.chars().count();
            if title_chars > title_width {
                title_display = title_display.chars().take(title_width - 2).collect::<String>() + "…";
            }
            println!(" {:>w$} | {:<tw$} | {}", num, title_display, note.id,
                w = num_width, tw = title_width);
        }

        println!("\n共 {} 条便签。使用 `zt show <编号>` 查看内容，例如: zt show 1", notes.len());
    }

    Ok(())
}

pub async fn show_note(server_url: &str, id_input: &str) -> Result<(), Box<dyn std::error::Error>> {
    let id = resolve_note_id(id_input)?;

    let config = api::get_config();
    let token = config.token.as_ref()
        .ok_or("未登录，请先运行 `zt login`")?;

    let note = api::api_get_note(server_url, token, &id).await?;
    println!("{}", serde_json::to_string_pretty(&api::CliNoteOutput::from(note))?);
    Ok(())
}

pub async fn edit_note(server_url: &str, id_input: &str, title: Option<&str>, content: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let id = resolve_note_id(id_input)?;

    if let Some(t) = title {
        validate_title(t)?;
    }
    if title.is_none() && content.is_none() {
        return Err("请至少指定一个要修改的参数：--title 或 --content".to_string().into());
    }

    let config = api::get_config();
    let token = config.token.as_ref()
        .ok_or("未登录，请先运行 `zt login`")?;

    let note = api::api_update_note(server_url, token, &id, title, content).await?;
    println!("{}", serde_json::to_string_pretty(&api::CliNoteOutput::from(note))?);
    Ok(())
}

pub async fn delete_note(server_url: &str, id_input: &str) -> Result<(), Box<dyn std::error::Error>> {
    let id = resolve_note_id(id_input)?;

    let config = api::get_config();
    let token = config.token.as_ref()
        .ok_or("未登录，请先运行 `zt login`")?;

    api::api_delete_note(server_url, token, &id).await?;
    println!("{{\"success\": true, \"message\": \"便签已删除\"}}");
    Ok(())
}

pub async fn search_notes(server_url: &str, keyword: &str) -> Result<(), Box<dyn std::error::Error>> {
    if keyword.trim().is_empty() {
        return Err("搜索关键词不能为空".to_string().into());
    }

    let config = api::get_config();
    let token = config.token.as_ref()
        .ok_or("未登录，请先运行 `zt login`")?;

    let notes = api::api_list_notes(server_url, token).await?;

    let filtered: Vec<api::CliNoteOutput> = notes
        .into_iter()
        .filter(|n| {
            n.title.contains(keyword)
                || n.content.as_ref().map(|c| c.contains(keyword)).unwrap_or(false)
        })
        .map(|n| n.into())
        .collect();

    if filtered.is_empty() {
        println!("未找到包含 \"{}\" 的便签", keyword);
    } else {
        println!("{}", serde_json::to_string_pretty(&filtered)?);
    }
    Ok(())
}

pub async fn sync_notes(server_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();
    let token = config.token.as_ref()
        .ok_or("未登录，请先运行 `zt login`")?;

    let notes = api::api_list_notes(server_url, token).await?;
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "success": true,
        "synced_count": notes.len(),
        "synced_at": chrono::Utc::now().to_rfc3339()
    }))?);
    Ok(())
}

pub async fn export_notes(server_url: &str, format: &str, output: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();
    let token = config.token.as_ref()
        .ok_or("未登录，请先运行 `zt login`")?;

    let notes = api::api_list_notes(server_url, token).await?;

    let content = match format {
        "json" => serde_json::to_string_pretty(&notes)?,
        "md" => {
            let mut md = String::from("# 纸条导出\n\n");
            for note in &notes {
                md.push_str(&format!("## {}\n\n{}\n\n---\n\n", note.title, note.content.as_deref().unwrap_or("")));
            }
            md
        }
        _ => return Err("不支持的导出格式，请使用 'json' 或 'md'".into()),
    };

    if let Some(output_path) = output {
        std::fs::write(output_path, &content)?;
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "success": true,
            "exported": notes.len(),
            "file": output_path
        }))?);
    } else {
        println!("{}", content);
    }

    Ok(())
}

pub async fn import_notes(server_url: &str, file: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(file);
    if !path.exists() {
        return Err(format!("文件不存在: {}", file).into());
    }
    if !path.is_file() {
        return Err(format!("路径不是文件: {}", file).into());
    }

    let config = api::get_config();
    let token = config.token.as_ref()
        .ok_or("未登录，请先运行 `zt login`")?;

    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("无法读取文件 {}: {}", file, e))?;
    let notes: Vec<zhiati_shared::Note> = serde_json::from_str(&content)
        .map_err(|e| format!("JSON 解析失败: {}", e))?;

    if notes.is_empty() {
        return Err("导入文件中没有便签数据".into());
    }

    let mut imported = 0;
    for note in notes {
        api::api_create_note(server_url, token, &note.title, note.content.as_deref()).await?;
        imported += 1;
    }

    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "success": true,
        "imported": imported
    }))?);
    Ok(())
}

pub async fn login(server_url: &str, email: Option<&str>, password: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let email = match email {
        Some(e) => e.to_string(),
        None => {
            print!("Email: ");
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        }
    };
    validate_email(&email)?;

    let password = match password {
        Some(p) => p.to_string(),
        None => {
            print!("Password: ");
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        }
    };
    validate_password(&password)?;

    let response = api::api_login(server_url, &email, &password).await
        .map_err(|e| {
            if e.to_string().contains("401") {
                "登录失败，请检查邮箱和密码是否正确".to_string()
            } else {
                format!("登录失败: {}", e)
            }
        })?;

    let mut config = api::get_config();
    config.token = Some(response.token);
    config.user_id = Some(response.user.id.to_string());
    config.server_url = Some(server_url.to_string());
    config.email = Some(email.to_string());
    api::save_config(&config)?;

    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "success": true,
        "message": "登录成功",
        "user": {
            "id": response.user.id,
            "email": response.user.email
        }
    }))?);
    Ok(())
}

pub async fn register(server_url: &str, email: Option<&str>, password: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let email = match email {
        Some(e) => e.to_string(),
        None => {
            print!("Email: ");
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        }
    };
    validate_email(&email)?;

    let password = match password {
        Some(p) => p.to_string(),
        None => {
            print!("Password: ");
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        }
    };
    validate_password(&password)?;

    let response = api::api_register(server_url, &email, &password).await
        .map_err(|e| {
            if e.to_string().contains("409") || e.to_string().contains("already") {
                "该邮箱已被注册".to_string()
            } else {
                format!("注册失败: {}", e)
            }
        })?;

    let mut config = api::get_config();
    config.token = Some(response.token);
    config.user_id = Some(response.user.id.to_string());
    config.server_url = Some(server_url.to_string());
    config.email = Some(email.to_string());
    api::save_config(&config)?;

    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "success": true,
        "message": "注册并登录成功",
        "user": {
            "id": response.user.id,
            "email": response.user.email
        }
    }))?);
    Ok(())
}

pub async fn logout() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = api::get_config();
    config.token = None;
    config.user_id = None;
    config.email = None;
    api::save_config(&config)?;

    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "success": true,
        "message": "已退出登录"
    }))?);
    Ok(())
}

pub fn whoami() -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();

    if config.token.is_none() && config.user_id.is_none() && config.email.is_none() {
        return Err("未登录，请先运行 `zt login`".to_string().into());
    }

    println!("邮箱:     {}", config.email.as_deref().unwrap_or("未知"));
    println!("用户 ID:  {}", config.user_id.as_deref().unwrap_or("未知"));
    println!("服务器:   {}", config.server_url.as_deref().unwrap_or("未配置"));
    Ok(())
}
