use crate::crdt::{
    CRDTMap, CRDTValue, GCounter, LWWRegister, NodeId, ORSet, PNCounter, VectorClock,
};
use serde::{Deserialize, Serialize};

/// 操作类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Operation {
    GCounterIncrement {
        key: String,
        node_id: NodeId,
        delta: u64,
    },
    PNCounterIncrement {
        key: String,
        node_id: NodeId,
        delta: u64,
    },
    PNCounterDecrement {
        key: String,
        node_id: NodeId,
        delta: u64,
    },
    LwwRegisterSet {
        key: String,
        value: String,
        timestamp: i64,
        node_id: NodeId,
    },
    OrSetAdd {
        key: String,
        value: String,
        unique_id: String,
    },
    OrSetRemove {
        key: String,
        value: String,
    },
}

/// 操作日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpLogEntry {
    pub id: String,          // 使用 scru128 生成的唯一 ID
    pub ts: i64,             // 时间戳
    pub causal: VectorClock, // 因果元数据
    pub op: Operation,       // 操作内容
}

/// 操作日志
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpLog {
    pub node_id: NodeId,
    pub ops: Vec<OpLogEntry>,
}

impl OpLog {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            ops: Vec::new(),
        }
    }

    pub fn add_operation(&mut self, op: Operation, vector_clock: &mut VectorClock) {
        let id = scru128::new_string();
        let ts = chrono::Local::now()
            .naive_local()
            .and_utc()
            .timestamp_millis();

        vector_clock.increment(&self.node_id);

        let entry = OpLogEntry {
            id,
            ts,
            causal: vector_clock.clone(),
            op,
        };

        self.ops.push(entry);
    }

    pub fn merge(&mut self, other: &OpLog) {
        for op in &other.ops {
            if !self.ops.iter().any(|e| e.id == op.id) {
                self.ops.push(op.clone());
            }
        }
        // 按时间戳排序
        self.ops
            .sort_by(|a, b| a.ts.cmp(&b.ts).then_with(|| a.id.cmp(&b.id)));
    }
}

/// 同步状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub node_id: NodeId,
    pub crdt_map: CRDTMap,
    pub op_log: OpLog,
}

impl SyncState {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id: node_id.clone(),
            crdt_map: CRDTMap::new(),
            op_log: OpLog::new(node_id),
        }
    }

    /// 应用操作到 CRDT Map
    pub fn apply_operation(&mut self, op: Operation) {
        self.op_log
            .add_operation(op.clone(), &mut self.crdt_map.vector_clock);

        match op {
            Operation::GCounterIncrement {
                key,
                node_id,
                delta,
            } => {
                let counter = self
                    .crdt_map
                    .entries
                    .entry(key)
                    .or_insert_with(|| CRDTValue::GCounter(GCounter::new()));

                if let CRDTValue::GCounter(c) = counter {
                    c.increment(&node_id, delta);
                }
            }
            Operation::PNCounterIncrement {
                key,
                node_id,
                delta,
            } => {
                let counter = self
                    .crdt_map
                    .entries
                    .entry(key)
                    .or_insert_with(|| CRDTValue::PNCounter(PNCounter::new()));

                if let CRDTValue::PNCounter(c) = counter {
                    c.increment(&node_id, delta);
                }
            }
            Operation::PNCounterDecrement {
                key,
                node_id,
                delta,
            } => {
                let counter = self
                    .crdt_map
                    .entries
                    .entry(key)
                    .or_insert_with(|| CRDTValue::PNCounter(PNCounter::new()));

                if let CRDTValue::PNCounter(c) = counter {
                    c.decrement(&node_id, delta);
                }
            }
            Operation::LwwRegisterSet {
                key,
                value,
                timestamp,
                node_id,
            } => {
                let register = self
                    .crdt_map
                    .entries
                    .entry(key)
                    .or_insert_with(|| CRDTValue::LWWRegister(LWWRegister::new()));

                if let CRDTValue::LWWRegister(r) = register {
                    r.set(value, timestamp, &node_id);
                }
            }
            Operation::OrSetAdd {
                key,
                value,
                unique_id,
            } => {
                let set = self
                    .crdt_map
                    .entries
                    .entry(key)
                    .or_insert_with(|| CRDTValue::ORSet(ORSet::new()));

                if let CRDTValue::ORSet(s) = set {
                    s.add(value, unique_id);
                }
            }
            Operation::OrSetRemove { key, value } => {
                if let Some(CRDTValue::ORSet(s)) = self.crdt_map.entries.get_mut(&key) {
                    s.remove(&value);
                }
            }
        }
    }

    /// 合并来自另一个节点的状态
    pub fn merge(&mut self, other: &SyncState) {
        // 合并操作日志
        self.op_log.merge(&other.op_log);

        // 合并 CRDT Map
        self.crdt_map.merge(&other.crdt_map);
    }

    /// 获取状态哈希
    pub fn state_hash(&self) -> String {
        self.crdt_map.state_hash()
    }

    /// 导出操作日志为 JSON
    pub fn export_oplog(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.op_log)
    }
}

/// 同步请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub from_node: NodeId,
    pub state: SyncState,
}

/// 同步响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub success: bool,
    pub state_hash: String,
    pub message: String,
}

/// 变更请求（用于 HTTP API）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRequest {
    pub changes: Vec<Change>,
}

/// 单个变更
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub op: String, // "add", "remove", "increment", "decrement", "set"
    pub key: String,
    pub value: Option<String>,
    pub delta: Option<u64>,
}

impl SyncState {
    /// 从变更请求应用操作
    pub fn apply_changes(&mut self, request: ChangeRequest) -> Result<(), String> {
        for change in request.changes {
            match change.op.as_str() {
                "add" => {
                    let value = change.value.ok_or("Missing value for add operation")?;
                    let unique_id = scru128::new_string();
                    let op = Operation::OrSetAdd {
                        key: change.key,
                        value,
                        unique_id,
                    };
                    self.apply_operation(op);
                }
                "remove" => {
                    let value = change.value.ok_or("Missing value for remove operation")?;
                    let op = Operation::OrSetRemove {
                        key: change.key,
                        value,
                    };
                    self.apply_operation(op);
                }
                "increment" => {
                    let delta = change.delta.unwrap_or(1);
                    let op = Operation::PNCounterIncrement {
                        key: change.key,
                        node_id: self.node_id.clone(),
                        delta,
                    };
                    self.apply_operation(op);
                }
                "decrement" => {
                    let delta = change.delta.unwrap_or(1);
                    let op = Operation::PNCounterDecrement {
                        key: change.key,
                        node_id: self.node_id.clone(),
                        delta,
                    };
                    self.apply_operation(op);
                }
                "set" => {
                    let value = change.value.ok_or("Missing value for set operation")?;
                    let timestamp = chrono::Local::now()
                        .naive_local()
                        .and_utc()
                        .timestamp_millis();
                    let op = Operation::LwwRegisterSet {
                        key: change.key,
                        value,
                        timestamp,
                        node_id: self.node_id.clone(),
                    };
                    self.apply_operation(op);
                }
                _ => return Err(format!("Unknown operation: {}", change.op)),
            }
        }
        Ok(())
    }
}
