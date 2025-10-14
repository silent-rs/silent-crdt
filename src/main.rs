mod api;
mod auth;
mod crdt;
mod grpc_service;
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

    /// gRPC 服务端口
    #[arg(long, default_value = "50051")]
    grpc_port: u16,

    /// 是否启用 gRPC 服务
    #[arg(long, default_value = "false")]
    grpc_enabled: bool,
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
    let routes = api::build_routes(app_state.clone());

    // 启动 HTTP 服务器
    let http_addr: std::net::SocketAddr = format!("127.0.0.1:{}", args.port)
        .parse()
        .expect("Invalid HTTP address");
    tracing::info!("Starting HTTP server on http://{}", http_addr);

    // 如果启用 gRPC，同时启动 gRPC 服务器
    if args.grpc_enabled {
        let grpc_addr: std::net::SocketAddr = format!("127.0.0.1:{}", args.grpc_port)
            .parse()
            .expect("Invalid gRPC address");
        tracing::info!("Starting gRPC server on {}", grpc_addr);

        let grpc_service = grpc_service::CrdtServiceImpl::new(app_state.clone());
        let grpc_server = grpc_service.into_server();

        // 并行运行 HTTP 和 gRPC 服务器
        tokio::select! {
            _ = Server::new().bind(http_addr).serve(routes) => {
                tracing::info!("HTTP server stopped");
                Ok(())
            }
            result = tonic::transport::Server::builder()
                .add_service(grpc_server)
                .serve(grpc_addr) => {
                tracing::info!("gRPC server stopped");
                result.map_err(|e| anyhow::anyhow!("gRPC server error: {}", e))
            }
        }
    } else {
        Server::new().bind(http_addr).serve(routes).await;
        Ok(())
    }
}
