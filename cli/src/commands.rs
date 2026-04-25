use crate::api;
use std::io::Write;

pub async fn new_note(server_url: &str, title: &str, content: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();

    let token = config.token.as_ref()
        .ok_or("Not logged in. Please run 'zt login' first.")?;

    let note = api::api_create_note(server_url, token, title, content).await?;

    println!("{}", serde_json::to_string_pretty(&api::CliNoteOutput::from(note))?);
    Ok(())
}

pub async fn list_notes(server_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();

    let token = config.token.as_ref()
        .ok_or("Not logged in. Please run 'zt login' first.")?;

    let notes = api::api_list_notes(server_url, token).await?;

    let output: Vec<api::CliNoteOutput> = notes.into_iter().map(|n| n.into()).collect();
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

pub async fn show_note(server_url: &str, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();

    let token = config.token.as_ref()
        .ok_or("Not logged in. Please run 'zt login' first.")?;

    let note = api::api_get_note(server_url, token, id).await?;
    println!("{}", serde_json::to_string_pretty(&api::CliNoteOutput::from(note))?);
    Ok(())
}

pub async fn edit_note(server_url: &str, id: &str, title: Option<&str>, content: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();

    let token = config.token.as_ref()
        .ok_or("Not logged in. Please run 'zt login' first.")?;

    let note = api::api_update_note(server_url, token, id, title, content).await?;
    println!("{}", serde_json::to_string_pretty(&api::CliNoteOutput::from(note))?);
    Ok(())
}

pub async fn delete_note(server_url: &str, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();

    let token = config.token.as_ref()
        .ok_or("Not logged in. Please run 'zt login' first.")?;

    api::api_delete_note(server_url, token, id).await?;
    println!("{{\"success\": true, \"message\": \"Note deleted\"}}");
    Ok(())
}

pub async fn search_notes(server_url: &str, keyword: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();

    let token = config.token.as_ref()
        .ok_or("Not logged in. Please run 'zt login' first.")?;

    let notes = api::api_list_notes(server_url, token).await?;

    let filtered: Vec<api::CliNoteOutput> = notes
        .into_iter()
        .filter(|n| n.title.contains(keyword) || n.content.as_ref().map(|c| c.contains(keyword)).unwrap_or(false))
        .map(|n| n.into())
        .collect();

    println!("{}", serde_json::to_string_pretty(&filtered)?);
    Ok(())
}

pub async fn sync_notes(server_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = api::get_config();

    let token = config.token.as_ref()
        .ok_or("Not logged in. Please run 'zt login' first.")?;

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
        .ok_or("Not logged in. Please run 'zt login' first.")?;

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
        _ => return Err("Unsupported format. Use 'json' or 'md'.".into()),
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
    let config = api::get_config();

    let token = config.token.as_ref()
        .ok_or("Not logged in. Please run 'zt login' first.")?;

    let content = std::fs::read_to_string(file)?;
    let notes: Vec<zhiati_shared::Note> = serde_json::from_str(&content)?;

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

    let response = api::api_login(server_url, &email, &password).await?;

    let mut config = api::get_config();
    config.token = Some(response.token);
    config.user_id = Some(response.user.id.to_string());
    api::save_config(&config)?;

    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "success": true,
        "message": "Logged in successfully",
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

    let response = api::api_register(server_url, &email, &password).await?;

    let mut config = api::get_config();
    config.token = Some(response.token);
    config.user_id = Some(response.user.id.to_string());
    api::save_config(&config)?;

    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "success": true,
        "message": "Registered and logged in successfully",
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
    api::save_config(&config)?;

    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "success": true,
        "message": "Logged out successfully"
    }))?);
    Ok(())
}
