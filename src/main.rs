mod api;
mod auth;
mod crdt;
mod signature;
mod storage;
mod sync;

use anyhow::Result;
use clap::Parser;
use silent::prelude::*;
use storage::Storage;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "silent-crdt")]
#[command(about = "Silent CRDT - A distributed CRDT implementation based on Silent framework")]
struct Args {
    /// 服务监听端口
    #[arg(long, default_value = "8080")]
    port: u16,

    /// 节点 ID
    #[arg(long)]
    node_id: Option<String>,

    /// 数据存储路径
    #[arg(long, default_value = "./data")]
    data_path: String,

    /// JWT 密钥
    #[arg(long, default_value = "silent-crdt-secret-key-change-in-production")]
    jwt_secret: String,

    /// 是否启用权限控制
    #[arg(long, default_value = "false")]
    auth_enabled: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "silent_crdt=info,silent=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 解析命令行参数
    let args = Args::parse();

    // 生成或使用提供的节点 ID
    let node_id = args.node_id.unwrap_or_else(|| {
        let id = scru128::new_string();
        tracing::info!("Generated node ID: {}", id);
        id
    });

    tracing::info!("Starting Silent CRDT node: {}", node_id);
    tracing::info!("Data path: {}", args.data_path);

    // 初始化存储
    let storage = Storage::new(&args.data_path)?;
    tracing::info!("Storage initialized");

    // 创建应用状态
    let app_state = api::AppState::new(
        node_id.clone(),
        storage,
        args.jwt_secret.clone(),
        args.auth_enabled,
    )?;
    tracing::info!("Application state created");
    tracing::info!("Auth enabled: {}", args.auth_enabled);

    // 构建路由
    let routes = api::build_routes(app_state);

    // 启动服务器
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", args.port)
        .parse()
        .expect("Invalid address");
    tracing::info!("Starting server on http://{}", addr);

    Server::new().bind(addr).serve(routes).await;

    Ok(())
}
