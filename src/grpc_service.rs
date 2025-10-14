use crate::api::AppState;
use crate::sync::ChangeRequest;
use tonic::{Request, Response, Status};

// 引入生成的 protobuf 代码
pub mod crdt {
    tonic::include_proto!("crdt");
}

use crdt::crdt_service_server::{CrdtService, CrdtServiceServer};
use crdt::*;

/// gRPC 服务实现
pub struct CrdtServiceImpl {
    app_state: AppState,
}

impl CrdtServiceImpl {
    pub fn new(app_state: AppState) -> Self {
        Self { app_state }
    }

    pub fn into_server(self) -> CrdtServiceServer<Self> {
        CrdtServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl CrdtService for CrdtServiceImpl {
    /// 同步数据变更
    async fn sync(&self, request: Request<SyncRequest>) -> Result<Response<SyncResponse>, Status> {
        let req = request.into_inner();

        // 转换 gRPC 请求到内部格式
        let changes: Vec<crate::sync::Change> = req
            .changes
            .into_iter()
            .map(|c| crate::sync::Change {
                op: c.op,
                key: c.key,
                value: c.value,
                delta: c.delta.map(|d| d as u64),
            })
            .collect();

        let change_request = ChangeRequest { changes };

        // 应用变更
        let mut sync_state = self.app_state.sync_state.write().await;
        sync_state
            .apply_changes(change_request)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        // 保存状态
        self.app_state
            .storage
            .save_state(&self.app_state.node_id, &sync_state)
            .map_err(|e| Status::internal(format!("Failed to save state: {}", e)))?;

        let state_hash = sync_state.state_hash();
        drop(sync_state);

        Ok(Response::new(SyncResponse {
            success: true,
            state_hash,
            message: "Changes applied successfully".to_string(),
        }))
    }

    /// 合并状态
    async fn merge(
        &self,
        request: Request<MergeRequest>,
    ) -> Result<Response<MergeResponse>, Status> {
        let req = request.into_inner();

        // 解析状态数据
        let incoming_state: crate::sync::SyncState = serde_json::from_slice(&req.state_data)
            .map_err(|e| Status::invalid_argument(format!("Invalid state data: {}", e)))?;

        // 合并状态
        let mut sync_state = self.app_state.sync_state.write().await;
        sync_state.merge(&incoming_state);

        // 保存状态
        self.app_state
            .storage
            .save_state(&self.app_state.node_id, &sync_state)
            .map_err(|e| Status::internal(format!("Failed to save state: {}", e)))?;

        let state_hash = sync_state.state_hash();
        drop(sync_state);

        Ok(Response::new(MergeResponse {
            success: true,
            state_hash,
            message: format!("Merged state from node: {}", req.from_node),
        }))
    }

    /// 获取当前状态
    async fn get_state(
        &self,
        _request: Request<GetStateRequest>,
    ) -> Result<Response<GetStateResponse>, Status> {
        let sync_state = self.app_state.sync_state.read().await;

        let state_data = serde_json::to_vec(&*sync_state)
            .map_err(|e| Status::internal(format!("Failed to serialize state: {}", e)))?;

        Ok(Response::new(GetStateResponse {
            node_id: self.app_state.node_id.clone(),
            state_data,
        }))
    }

    /// 获取状态哈希
    async fn get_state_hash(
        &self,
        _request: Request<GetStateHashRequest>,
    ) -> Result<Response<GetStateHashResponse>, Status> {
        let sync_state = self.app_state.sync_state.read().await;
        let state_hash = sync_state.state_hash();

        Ok(Response::new(GetStateHashResponse { state_hash }))
    }

    /// 获取操作日志
    async fn get_op_log(
        &self,
        _request: Request<GetOpLogRequest>,
    ) -> Result<Response<GetOpLogResponse>, Status> {
        let sync_state = self.app_state.sync_state.read().await;

        let entries: Vec<OpLogEntry> = sync_state
            .op_log
            .ops
            .iter()
            .map(|entry| {
                let operation = format!("{:?}", entry.op);
                let causal_context = entry
                    .causal
                    .clocks
                    .iter()
                    .map(|(k, v)| (k.clone(), *v as i64))
                    .collect();

                OpLogEntry {
                    id: entry.id.clone(),
                    timestamp: entry.ts,
                    node_id: "".to_string(), // OpLogEntry 不包含 node_id，从 operation 中提取
                    operation,
                    causal_context,
                }
            })
            .collect();

        Ok(Response::new(GetOpLogResponse { entries }))
    }

    /// 获取操作历史
    async fn get_history(
        &self,
        _request: Request<GetHistoryRequest>,
    ) -> Result<Response<GetHistoryResponse>, Status> {
        let sync_state = self.app_state.sync_state.read().await;

        let entries: Vec<HistoryEntry> = sync_state
            .op_log
            .ops
            .iter()
            .map(|entry| {
                let (op_type, key, details, op_node_id) = match &entry.op {
                    crate::sync::Operation::GCounterIncrement {
                        key,
                        node_id,
                        delta,
                    } => (
                        "GCounter.Increment",
                        key.clone(),
                        format!("增加 {}", delta),
                        node_id.clone(),
                    ),
                    crate::sync::Operation::PNCounterIncrement {
                        key,
                        node_id,
                        delta,
                    } => (
                        "PNCounter.Increment",
                        key.clone(),
                        format!("增加 {}", delta),
                        node_id.clone(),
                    ),
                    crate::sync::Operation::PNCounterDecrement {
                        key,
                        node_id,
                        delta,
                    } => (
                        "PNCounter.Decrement",
                        key.clone(),
                        format!("减少 {}", delta),
                        node_id.clone(),
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
                        node_id.clone(),
                    ),
                    crate::sync::Operation::OrSetAdd {
                        key,
                        value,
                        unique_id,
                    } => (
                        "ORSet.Add",
                        key.clone(),
                        format!("添加元素 '{}' (id: {})", value, &unique_id[..8]),
                        "".to_string(),
                    ),
                    crate::sync::Operation::OrSetRemove { key, value } => (
                        "ORSet.Remove",
                        key.clone(),
                        format!("移除元素 '{}'", value),
                        "".to_string(),
                    ),
                };

                let causal_context = entry
                    .causal
                    .clocks
                    .iter()
                    .map(|(k, v)| (k.clone(), *v as i64))
                    .collect();

                HistoryEntry {
                    id: entry.id.clone(),
                    timestamp: entry.ts,
                    operation_type: op_type.to_string(),
                    key,
                    details,
                    node_id: op_node_id,
                    causal_context,
                }
            })
            .collect();

        Ok(Response::new(GetHistoryResponse { entries }))
    }

    /// 获取冲突信息
    async fn get_conflicts(
        &self,
        _request: Request<GetConflictsRequest>,
    ) -> Result<Response<GetConflictsResponse>, Status> {
        let sync_state = self.app_state.sync_state.read().await;

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

        Ok(Response::new(GetConflictsResponse { conflicts }))
    }

    /// 健康检查
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            status: "ok".to_string(),
            timestamp: chrono::Local::now()
                .naive_local()
                .and_utc()
                .timestamp_millis(),
        }))
    }
}
