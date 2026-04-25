mod commands;
mod api;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "zt")]
#[command(about = "纸条 - 备忘录软件", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, global = true)]
    server: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "创建新便签")]
    New {
        #[arg(help = "便签标题")]
        title: String,

        #[arg(long, help = "便签内容")]
        content: Option<String>,
    },

    #[command(about = "列出所有便签")]
    List,

    #[command(about = "查看便签内容")]
    Show {
        #[arg(help = "便签ID")]
        id: String,
    },

    #[command(about = "编辑便签")]
    Edit {
        #[arg(help = "便签ID")]
        id: String,

        #[arg(long, help = "新标题")]
        title: Option<String>,

        #[arg(long, help = "新内容")]
        content: Option<String>,
    },

    #[command(about = "删除便签")]
    Delete {
        #[arg(help = "便签ID")]
        id: String,
    },

    #[command(about = "搜索便签")]
    Search {
        #[arg(help = "搜索关键词")]
        keyword: String,
    },

    #[command(about = "同步便签")]
    Sync,

    #[command(about = "导出便签")]
    Export {
        #[arg(long, default_value = "json")]
        format: String,

        #[arg(long)]
        output: Option<String>,
    },

    #[command(about = "导入便签")]
    Import {
        #[arg(long)]
        file: String,
    },

    #[command(about = "用户登录")]
    Login {
        #[arg(long)]
        email: Option<String>,

        #[arg(long)]
        password: Option<String>,
    },

    #[command(about = "用户注册")]
    Register {
        #[arg(long)]
        email: Option<String>,

        #[arg(long)]
        password: Option<String>,
    },

    #[command(about = "退出登录")]
    Logout,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    let server_url = cli.server.unwrap_or_else(|| {
        api::get_config().server_url.unwrap_or_else(|| "http://localhost:8080".to_string())
    });

    match cli.command {
        Commands::New { title, content } => {
            commands::new_note(&server_url, &title, content.as_deref()).await?;
        }
        Commands::List => {
            commands::list_notes(&server_url).await?;
        }
        Commands::Show { id } => {
            commands::show_note(&server_url, &id).await?;
        }
        Commands::Edit { id, title, content } => {
            commands::edit_note(&server_url, &id, title.as_deref(), content.as_deref()).await?;
        }
        Commands::Delete { id } => {
            commands::delete_note(&server_url, &id).await?;
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
    }

    Ok(())
}
