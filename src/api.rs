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

/// GET /history - 获取操作历史（带详细信息）
async fn get_history_handler(req: Request) -> Result<Response> {
    let state = req.extensions().get::<AppState>().unwrap().clone();
    let sync_state = state.sync_state.read().await;

    #[derive(Serialize)]
    struct HistoryEntry {
        id: String,
        timestamp: i64,
        operation_type: String,
        key: String,
        details: String,
        node_id: String,
        causal_context: std::collections::HashMap<String, i64>,
    }

    let oplog = &sync_state.op_log;
    let mut history: Vec<HistoryEntry> = Vec::new();

    for entry in &oplog.ops {
        let (op_type, key, details) = match &entry.op {
            crate::sync::Operation::GCounterIncrement {
                key,
                node_id,
                delta,
            } => (
                "GCounter.Increment",
                key.clone(),
                format!("节点 {} 增加 {}", node_id, delta),
            ),
            crate::sync::Operation::PNCounterIncrement {
                key,
                node_id,
                delta,
            } => (
                "PNCounter.Increment",
                key.clone(),
                format!("节点 {} 增加 {}", node_id, delta),
            ),
            crate::sync::Operation::PNCounterDecrement {
                key,
                node_id,
                delta,
            } => (
                "PNCounter.Decrement",
                key.clone(),
                format!("节点 {} 减少 {}", node_id, delta),
            ),
            crate::sync::Operation::LwwRegisterSet {
                key,
                value,
                timestamp,
                node_id,
            } => (
                "LWWRegister.Set",
                key.clone(),
                format!("节点 {} 设置为 '{}' (ts: {})", node_id, value, timestamp),
            ),
            crate::sync::Operation::OrSetAdd {
                key,
                value,
                unique_id,
            } => (
                "ORSet.Add",
                key.clone(),
                format!("添加元素 '{}' (id: {})", value, &unique_id[..8]),
            ),
            crate::sync::Operation::OrSetRemove { key, value } => {
                ("ORSet.Remove", key.clone(), format!("移除元素 '{}'", value))
            }
        };

        history.push(HistoryEntry {
            id: entry.id.clone(),
            timestamp: entry.ts,
            operation_type: op_type.to_string(),
            key,
            details,
            node_id: oplog.node_id.clone(),
            causal_context: entry
                .causal
                .clocks
                .iter()
                .map(|(k, v)| (k.clone(), *v as i64))
                .collect(),
        });
    }

    Ok(Response::json(&history))
}

/// GET /conflicts - 检测并返回可能的冲突
async fn get_conflicts_handler(req: Request) -> Result<Response> {
    let state = req.extensions().get::<AppState>().unwrap().clone();
    let sync_state = state.sync_state.read().await;

    #[derive(Serialize)]
    struct Conflict {
        key: String,
        conflict_type: String,
        operations: Vec<ConflictOperation>,
        resolution: String,
    }

    #[derive(Serialize)]
    struct ConflictOperation {
        id: String,
        timestamp: i64,
        node_id: String,
        details: String,
    }

    let mut conflicts: Vec<Conflict> = Vec::new();
    let oplog = &sync_state.op_log;

    // 检测 LWWRegister 的并发写入
    let mut lww_writes: std::collections::HashMap<String, Vec<&crate::sync::OpLogEntry>> =
        std::collections::HashMap::new();

    for entry in &oplog.ops {
        if let crate::sync::Operation::LwwRegisterSet { key, .. } = &entry.op {
            lww_writes.entry(key.clone()).or_default().push(entry);
        }
    }

    for (key, entries) in lww_writes {
        if entries.len() > 1 {
            // 检查是否有并发写入（向量时钟无法比较）
            let mut concurrent_writes = Vec::new();
            for i in 0..entries.len() {
                for j in (i + 1)..entries.len() {
                    let clock1 = &entries[i].causal;
                    let clock2 = &entries[j].causal;

                    if !clock1.happens_before(clock2) && !clock2.happens_before(clock1) {
                        if concurrent_writes.is_empty()
                            && let crate::sync::Operation::LwwRegisterSet {
                                value,
                                timestamp,
                                node_id,
                                ..
                            } = &entries[i].op
                        {
                            concurrent_writes.push(ConflictOperation {
                                id: entries[i].id.clone(),
                                timestamp: *timestamp,
                                node_id: node_id.clone(),
                                details: format!("设置为 '{}'", value),
                            });
                        }

                        if let crate::sync::Operation::LwwRegisterSet {
                            value,
                            timestamp,
                            node_id,
                            ..
                        } = &entries[j].op
                        {
                            concurrent_writes.push(ConflictOperation {
                                id: entries[j].id.clone(),
                                timestamp: *timestamp,
                                node_id: node_id.clone(),
                                details: format!("设置为 '{}'", value),
                            });
                        }
                    }
                }
            }

            if !concurrent_writes.is_empty() {
                // 找出最终胜出的值
                let winner_node = concurrent_writes
                    .iter()
                    .max_by(|a, b| {
                        a.timestamp
                            .cmp(&b.timestamp)
                            .then_with(|| a.node_id.cmp(&b.node_id))
                    })
                    .map(|w| w.node_id.clone())
                    .unwrap();

                conflicts.push(Conflict {
                    key: key.clone(),
                    conflict_type: "LWWRegister 并发写入".to_string(),
                    operations: concurrent_writes,
                    resolution: format!(
                        "根据 LWW 规则，时间戳较大的操作胜出 (节点: {})",
                        winner_node
                    ),
                });
            }
        }
    }

    Ok(Response::json(&conflicts))
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
        // API 路由
        .append(Route::new("sync").post(sync_handler))
        .append(Route::new("sync-peer").post(sync_peer_handler))
        .append(Route::new("merge").post(merge_handler))
        .append(Route::new("state").get(get_state_handler))
        .append(Route::new("state-hash").get(get_state_hash_handler))
        .append(Route::new("oplog").get(get_oplog_handler))
        .append(Route::new("history").get(get_history_handler))
        .append(Route::new("conflicts").get(get_conflicts_handler))
        .append(Route::new("health").get(health_handler))
        // 静态文件服务（使用 Silent 内置的静态资源处理器）
        .with_static("./static")
}
