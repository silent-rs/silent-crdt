use crate::storage::Storage;
use crate::sync::{ChangeRequest, SyncRequest, SyncResponse, SyncState};
use serde::{Deserialize, Serialize};
use silent::prelude::*;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    pub node_id: String,
    pub sync_state: Arc<RwLock<SyncState>>,
    pub storage: Arc<Storage>,
}

impl AppState {
    pub fn new(node_id: String, storage: Storage) -> anyhow::Result<Self> {
        let sync_state = if let Some(state) = storage.load_state(&node_id)? {
            Arc::new(RwLock::new(state))
        } else {
            Arc::new(RwLock::new(SyncState::new(node_id.clone())))
        };

        Ok(Self {
            node_id,
            sync_state,
            storage: Arc::new(storage),
        })
    }
}

// 实现中间件处理器，用于在所有请求中注入 AppState
#[async_trait::async_trait]
impl MiddleWareHandler for AppState {
    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        req.extensions_mut().insert(self.clone());
        next.call(req).await
    }
}

/// POST /sync - 接收变更请求
async fn sync_handler(mut req: Request) -> Result<Response> {
    let state = req.extensions().get::<AppState>().unwrap().clone();

    // 解析请求体
    let change_request: ChangeRequest = req.json_parse().await?;

    // 应用变更
    let mut sync_state = state.sync_state.write().await;
    sync_state
        .apply_changes(change_request)
        .map_err(|e| SilentError::business_error(StatusCode::BAD_REQUEST, e))?;

    // 保存状态
    state
        .storage
        .save_state(&state.node_id, &sync_state)
        .map_err(|e| {
            SilentError::business_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to save state: {}", e),
            )
        })?;

    let state_hash = sync_state.state_hash();
    drop(sync_state);

    let response = SyncResponse {
        success: true,
        state_hash,
        message: "Changes applied successfully".to_string(),
    };

    Ok(Response::json(&response))
}

/// POST /sync-peer - 触发与其他节点的同步
#[derive(Debug, Deserialize)]
struct SyncPeerRequest {
    peer: String,
}

async fn sync_peer_handler(mut req: Request) -> Result<Response> {
    let state = req.extensions().get::<AppState>().unwrap().clone();

    // 解析请求体
    let peer_req: SyncPeerRequest = req.json_parse().await?;

    // 获取当前状态
    let current_state = {
        let sync_state = state.sync_state.read().await;
        sync_state.clone()
    };

    // 构建同步请求
    let sync_request = SyncRequest {
        from_node: state.node_id.clone(),
        state: current_state,
    };

    // 发送同步请求到对等节点
    let client = reqwest::Client::new();
    let peer_url = format!("http://{}/merge", peer_req.peer);

    let response = client
        .post(&peer_url)
        .json(&sync_request)
        .send()
        .await
        .map_err(|e| {
            SilentError::business_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to sync with peer: {}", e),
            )
        })?;

    if response.status().is_success() {
        let sync_response: SyncResponse = response.json().await.map_err(|e| {
            SilentError::business_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to parse peer response: {}", e),
            )
        })?;

        Ok(Response::json(&sync_response))
    } else {
        Err(SilentError::business_error(
            StatusCode::BAD_GATEWAY,
            format!("Peer returned error: {}", response.status()),
        ))
    }
}

/// POST /merge - 接收来自其他节点的同步请求
async fn merge_handler(mut req: Request) -> Result<Response> {
    let state = req.extensions().get::<AppState>().unwrap().clone();

    // 解析请求体
    let sync_request: SyncRequest = req.json_parse().await?;

    // 合并状态
    let mut sync_state = state.sync_state.write().await;
    sync_state.merge(&sync_request.state);

    // 保存状态
    state
        .storage
        .save_state(&state.node_id, &sync_state)
        .map_err(|e| {
            SilentError::business_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to save state: {}", e),
            )
        })?;

    let state_hash = sync_state.state_hash();
    drop(sync_state);

    tracing::info!("Merged state from node: {}", sync_request.from_node);

    let response = SyncResponse {
        success: true,
        state_hash,
        message: format!("Merged state from {}", sync_request.from_node),
    };

    Ok(Response::json(&response))
}

/// GET /state - 获取当前状态
async fn get_state_handler(req: Request) -> Result<Response> {
    let state = req.extensions().get::<AppState>().unwrap().clone();

    let sync_state = state.sync_state.read().await;
    let state_json = serde_json::to_string_pretty(&*sync_state).map_err(|e| {
        SilentError::business_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to serialize state: {}", e),
        )
    })?;

    Ok(Response::text(&state_json))
}

/// GET /state-hash - 获取状态哈希
async fn get_state_hash_handler(req: Request) -> Result<Response> {
    let state = req.extensions().get::<AppState>().unwrap().clone();

    let sync_state = state.sync_state.read().await;
    let state_hash = sync_state.state_hash();

    #[derive(Serialize)]
    struct StateHashResponse {
        hash: String,
    }

    Ok(Response::json(&StateHashResponse { hash: state_hash }))
}

/// GET /oplog - 导出操作日志
async fn get_oplog_handler(req: Request) -> Result<Response> {
    let state = req.extensions().get::<AppState>().unwrap().clone();

    let sync_state = state.sync_state.read().await;
    let oplog_json = sync_state.export_oplog().map_err(|e| {
        SilentError::business_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to export oplog: {}", e),
        )
    })?;

    Ok(Response::text(&oplog_json))
}

/// GET /health - 健康检查
async fn health_handler(_req: Request) -> Result<Response> {
    #[derive(Serialize)]
    struct HealthResponse {
        status: String,
        timestamp: i64,
    }

    let response = HealthResponse {
        status: "ok".to_string(),
        timestamp: chrono::Local::now()
            .naive_local()
            .and_utc()
            .timestamp_millis(),
    };

    Ok(Response::json(&response))
}

/// 构建 API 路由
pub fn build_routes(app_state: AppState) -> Route {
    Route::new_root()
        .hook(app_state)
        .append(Route::new("sync").post(sync_handler))
        .append(Route::new("sync-peer").post(sync_peer_handler))
        .append(Route::new("merge").post(merge_handler))
        .append(Route::new("state").get(get_state_handler))
        .append(Route::new("state-hash").get(get_state_hash_handler))
        .append(Route::new("oplog").get(get_oplog_handler))
        .append(Route::new("health").get(health_handler))
}
