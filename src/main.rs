use std::sync::Arc;

use clap::Parser;
use color_eyre::eyre::Result;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use privchat::application::dto::TokenSource;
use privchat::infrastructure::{AppConfig, FileTokenStorage, PrivchatService};
use privchat::presentation::{App, Theme};
use std::path::PathBuf;

fn init_logging(config: &AppConfig) -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.log_level.to_string()));

    if let Some(log_path) = config.effective_log_path() {
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        let file_layer = fmt::layer()
            .with_writer(file)
            .with_ansi(false)
            .with_target(true)
            .with_thread_ids(false);

        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .init();

        info!(path = %log_path.display(), "Logging initialized");
    } else {
        tracing_subscriber::registry().with(filter).init();
    }

    Ok(())
}

fn create_app() -> Result<(App, Option<(String, TokenSource)>)> {
    let config = AppConfig::parse();

    let external_token = if let Ok(env_token) = std::env::var("PRIVCHAT_TOKEN") {
        if let Some(ref token) = config.token {
            if token == &env_token {
                Some((token.clone(), TokenSource::Environment))
            } else {
                Some((token.clone(), TokenSource::CommandLine))
            }
        } else {
            Some((env_token, TokenSource::Environment))
        }
    } else {
        config.token.clone().map(|t| (t, TokenSource::CommandLine))
    };

    init_logging(&config)?;

    info!(version = privchat::VERSION, "Starting Privchat");

    let data_dir = std::env::var("PRIVCHAT_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            directories::ProjectDirs::from("io", "privchat", "privchat-cli")
                .map(|d| d.data_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from("./data"))
        });
    let server_url = std::env::var("PRIVCHAT_SERVER_URL").unwrap_or_else(|_| "quic://127.0.0.1:8082".to_string());
    let privchat = Arc::new(PrivchatService::new(data_dir.clone(), server_url));

    let token_storage = Arc::new(FileTokenStorage::new(data_dir));
    let theme = Theme::new(&config.theme.accent_color);

    let app_config = privchat::presentation::AppConfig {
        disable_user_colors: config.disable_user_colors,
        group_guilds: config.ui.group_guilds,
        enable_desktop_notifications: config.enable_desktop_notifications,
        use_display_name: config.ui.use_display_name,
        theme,
    };

    let app = App::new(
        privchat.clone(),
        privchat.clone(),
        privchat,
        token_storage,
        app_config,
    );

    Ok((app, external_token))
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let (app, external_token) = create_app()?;

    let mut terminal = ratatui::init();

    let result = app.run(&mut terminal, external_token).await;

    ratatui::restore();

    result
}
