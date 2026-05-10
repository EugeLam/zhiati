mod commands;
mod api;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "zt")]
#[command(about = "纸条 - 备忘录 CLI 工具")]
#[command(long_about = "纸条（ZhiTiao）命令行工具，用于在终端中管理便签。\n\n常用命令:\n  zt list              列出所有便签\n  zt new \"标题\"        创建新便签\n  zt show 1            查看第一条便签\n  zt edit 1 --title X  编辑便签\n  zt sync              同步便签\n  zt export            导出便签")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, global = true, help = "指定服务器地址，例如 http://localhost:8080")]
    server: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "创建便签 | zt new \"标题\" [内容]", long_about = "创建一条新的便签到服务器。\n\n可以直接传入标题和内容作为位置参数，也可以用 --content 指定。\n\n示例:\n  zt new \"会议记录\"\n  zt new \"会议记录\" \"这是会议内容\"\n  zt new \"TODO\" --content \"1. 任务一\"")]
    New {
        #[arg(help = "便签标题")]
        title: String,

        #[arg(help = "便签内容")]
        content: Option<String>,

        #[arg(long = "content", value_name = "CONTENT", help = "便签内容")]
        content_flag: Option<String>,
    },

    #[command(about = "列出便签 | zt list [--json]", long_about = "列出当前用户的所有便签。\n\n便签会以编号表格形式展示，便于后续通过编号操作。\n\n示例:\n  zt list\n  zt list --json")]
    List {
        #[arg(long, help = "以 JSON 格式输出")]
        json: bool,
    },

    #[command(about = "查看便签 | zt show [编号/UUID]", long_about = "查看指定便签的详细信息。\n\n可以通过列表编号或 UUID 指定便签。不提供参数时显示可用便签列表。\n\n示例:\n  zt show            列出便签，提示选择\n  zt show 1          查看第一条便签\n  zt show <uuid>     通过 UUID 查看")]
    Show {
        #[arg(help = "便签编号（如 1）或 UUID，不指定则列出便签")]
        id: Option<String>,
    },

    #[command(about = "编辑便签 | zt edit [编号/UUID] --title X --content Y", long_about = "修改便签的标题和/或内容。\n\n必须至少指定 --title 或 --content 中的一个。不提供编号时显示可用便签列表。\n\n示例:\n  zt edit 1 --title \"新标题\"\n  zt edit 1 --content \"新内容\"\n  zt edit <uuid> --title X --content Y")]
    Edit {
        #[arg(help = "便签编号（如 1）或 UUID，不指定则列出便签")]
        id: Option<String>,

        #[arg(long, help = "新标题")]
        title: Option<String>,

        #[arg(long, help = "新内容")]
        content: Option<String>,
    },

    #[command(about = "删除便签 | zt delete [编号/UUID]", long_about = "删除指定的便签及其所有附件。\n\n可以通过列表编号或 UUID 指定便签。不提供参数时显示可用便签列表。\n\n示例:\n  zt delete 1\n  zt delete <uuid>")]
    Delete {
        #[arg(help = "便签编号（如 1）或 UUID，不指定则列出便签")]
        id: Option<String>,
    },

    #[command(about = "搜索便签 | zt search \"关键词\"", long_about = "在便签标题和内容中搜索关键词。\n\n示例:\n  zt search \"会议\"\n  zt search \"TODO\"")]
    Search {
        #[arg(help = "搜索关键词")]
        keyword: String,
    },

    #[command(about = "同步便签 | zt sync", long_about = "从服务器同步当前本地便签状态。\n\n示例:\n  zt sync")]
    Sync,

    #[command(about = "导出便签 | zt export [--format json|md] [--output 文件]", long_about = "将所有便签导出为 JSON 或 Markdown 格式。\n\n示例:\n  zt export\n  zt export --format md --output notes.md\n  zt export --format json --output notes.json")]
    Export {
        #[arg(long, default_value = "json", help = "导出格式: json 或 md")]
        format: String,

        #[arg(long, help = "输出文件路径，不指定则输出终端")]
        output: Option<String>,
    },

    #[command(about = "导入便签 | zt import --file 文件路径", long_about = "从 JSON 文件导入便签。\n\n便签 ID 需为有效的 UUID 格式。\n\n示例:\n  zt import --file notes.json")]
    Import {
        #[arg(long, help = "导入文件路径")]
        file: String,
    },

    #[command(about = "用户登录 | zt login [--email X --password Y]", long_about = "登录到纸条服务器。\n\n可以通过参数直接传入，也可以交互式输入。\n\n示例:\n  zt login --email user@example.com --password 123456\n  zt login")]
    Login {
        #[arg(long, help = "邮箱地址")]
        email: Option<String>,

        #[arg(long, help = "密码（建议不直接传入，使用交互式输入）")]
        password: Option<String>,
    },

    #[command(about = "用户注册 | zt register [--email X --password Y]", long_about = "注册新用户账号并自动登录。\n\n密码长度不能少于 6 个字符。\n\n示例:\n  zt register --email user@example.com --password 123456\n  zt register")]
    Register {
        #[arg(long, help = "邮箱地址")]
        email: Option<String>,

        #[arg(long, help = "密码（至少 6 个字符）")]
        password: Option<String>,
    },

    #[command(about = "退出登录 | zt logout", long_about = "清除本地登录凭证。\n\n示例:\n  zt logout")]
    Logout,

    #[command(about = "查看当前用户 | zt whoami", long_about = "显示当前登录用户的邮箱、ID 和服务器地址。\n\n示例:\n  zt whoami")]
    Whoami,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    let server_url = cli.server.unwrap_or_else(|| {
        api::get_config().server_url.unwrap_or_else(|| "http://localhost:8080".to_string())
    });

    match cli.command {
        Commands::New { title, content, content_flag } => {
            let content = content.or(content_flag);
            commands::new_note(&server_url, &title, content.as_deref()).await?;
        }
        Commands::List { json } => {
            commands::list_notes(&server_url, json).await?;
        }
        Commands::Show { id } => {
            if let Some(id) = id {
                commands::show_note(&server_url, &id).await?;
            } else {
                commands::prompt_note_select(&server_url).await?;
            }
        }
        Commands::Edit { id, title, content } => {
            if let Some(id) = id {
                commands::edit_note(&server_url, &id, title.as_deref(), content.as_deref()).await?;
            } else {
                commands::prompt_edit_select(&server_url, title.as_deref(), content.as_deref()).await?;
            }
        }
        Commands::Delete { id } => {
            if let Some(id) = id {
                commands::delete_note(&server_url, &id).await?;
            } else {
                commands::prompt_delete_select(&server_url).await?;
            }
        }
        Commands::Search { keyword } => {
            commands::search_notes(&server_url, &keyword).await?;
        }
        Commands::Sync => {
            commands::sync_notes(&server_url).await?;
        }
        Commands::Export { format, output } => {
            commands::export_notes(&server_url, &format, output.as_deref()).await?;
        }
        Commands::Import { file } => {
            commands::import_notes(&server_url, &file).await?;
        }
        Commands::Login { email, password } => {
            commands::login(&server_url, email.as_deref(), password.as_deref()).await?;
        }
        Commands::Register { email, password } => {
            commands::register(&server_url, email.as_deref(), password.as_deref()).await?;
        }
        Commands::Logout => {
            commands::logout().await?;
        }
        Commands::Whoami => {
            commands::whoami()?;
        }
    }

    Ok(())
}
