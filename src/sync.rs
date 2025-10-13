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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oplog_add_operation() {
        let mut oplog = OpLog::new("node1".to_string());
        let mut vc = VectorClock::new();

        let op = Operation::GCounterIncrement {
            key: "counter1".to_string(),
            node_id: "node1".to_string(),
            delta: 5,
        };

        oplog.add_operation(op, &mut vc);

        assert_eq!(oplog.ops.len(), 1);
        assert_eq!(vc.get("node1"), 1);
    }

    #[test]
    fn test_oplog_merge() {
        let mut oplog1 = OpLog::new("node1".to_string());
        let mut oplog2 = OpLog::new("node2".to_string());
        let mut vc = VectorClock::new();

        let op1 = Operation::GCounterIncrement {
            key: "counter1".to_string(),
            node_id: "node1".to_string(),
            delta: 5,
        };
        oplog1.add_operation(op1, &mut vc);

        let op2 = Operation::GCounterIncrement {
            key: "counter2".to_string(),
            node_id: "node2".to_string(),
            delta: 3,
        };
        oplog2.add_operation(op2, &mut vc);

        oplog1.merge(&oplog2);

        assert_eq!(oplog1.ops.len(), 2);
    }

    #[test]
    fn test_sync_state_apply_gcounter_operation() {
        let mut state = SyncState::new("node1".to_string());

        let op = Operation::GCounterIncrement {
            key: "counter1".to_string(),
            node_id: "node1".to_string(),
            delta: 5,
        };

        state.apply_operation(op);

        if let Some(CRDTValue::GCounter(c)) = state.crdt_map.entries.get("counter1") {
            assert_eq!(c.value(), 5);
        } else {
            panic!("Counter not found or wrong type");
        }
    }

    #[test]
    fn test_sync_state_apply_pncounter_operations() {
        let mut state = SyncState::new("node1".to_string());

        let op1 = Operation::PNCounterIncrement {
            key: "counter1".to_string(),
            node_id: "node1".to_string(),
            delta: 10,
        };
        state.apply_operation(op1);

        let op2 = Operation::PNCounterDecrement {
            key: "counter1".to_string(),
            node_id: "node1".to_string(),
            delta: 3,
        };
        state.apply_operation(op2);

        if let Some(CRDTValue::PNCounter(c)) = state.crdt_map.entries.get("counter1") {
            assert_eq!(c.value(), 7);
        } else {
            panic!("Counter not found or wrong type");
        }
    }

    #[test]
    fn test_sync_state_apply_lww_register_operation() {
        let mut state = SyncState::new("node1".to_string());

        let op = Operation::LwwRegisterSet {
            key: "register1".to_string(),
            value: "test_value".to_string(),
            timestamp: 12345,
            node_id: "node1".to_string(),
        };

        state.apply_operation(op);

        if let Some(CRDTValue::LWWRegister(r)) = state.crdt_map.entries.get("register1") {
            assert_eq!(r.get(), Some(&"test_value".to_string()));
        } else {
            panic!("Register not found or wrong type");
        }
    }

    #[test]
    fn test_sync_state_apply_orset_operations() {
        let mut state = SyncState::new("node1".to_string());

        let op1 = Operation::OrSetAdd {
            key: "set1".to_string(),
            value: "item1".to_string(),
            unique_id: "id1".to_string(),
        };
        state.apply_operation(op1);

        let op2 = Operation::OrSetAdd {
            key: "set1".to_string(),
            value: "item2".to_string(),
            unique_id: "id2".to_string(),
        };
        state.apply_operation(op2);

        if let Some(CRDTValue::ORSet(s)) = state.crdt_map.entries.get("set1") {
            let elements = s.elements();
            assert_eq!(elements.len(), 2);
            assert!(elements.contains(&"item1".to_string()));
            assert!(elements.contains(&"item2".to_string()));
        } else {
            panic!("Set not found or wrong type");
        }
    }

    #[test]
    fn test_sync_state_merge() {
        let mut state1 = SyncState::new("node1".to_string());
        let mut state2 = SyncState::new("node2".to_string());

        let op1 = Operation::GCounterIncrement {
            key: "counter1".to_string(),
            node_id: "node1".to_string(),
            delta: 5,
        };
        state1.apply_operation(op1);

        let op2 = Operation::GCounterIncrement {
            key: "counter1".to_string(),
            node_id: "node2".to_string(),
            delta: 3,
        };
        state2.apply_operation(op2);

        state1.merge(&state2);

        if let Some(CRDTValue::GCounter(c)) = state1.crdt_map.entries.get("counter1") {
            assert_eq!(c.value(), 8);
        } else {
            panic!("Counter not found or wrong type");
        }
    }

    #[test]
    fn test_sync_state_state_hash() {
        let mut state = SyncState::new("node1".to_string());

        let op = Operation::GCounterIncrement {
            key: "counter1".to_string(),
            node_id: "node1".to_string(),
            delta: 5,
        };
        state.apply_operation(op);

        let hash1 = state.state_hash();
        let hash2 = state.state_hash();

        assert_eq!(hash1, hash2);
        assert!(!hash1.is_empty());
    }

    #[test]
    fn test_sync_state_export_oplog() {
        let mut state = SyncState::new("node1".to_string());

        let op = Operation::GCounterIncrement {
            key: "counter1".to_string(),
            node_id: "node1".to_string(),
            delta: 5,
        };
        state.apply_operation(op);

        let result = state.export_oplog();
        assert!(result.is_ok());

        let json = result.unwrap();
        assert!(json.contains("counter1"));
    }

    #[test]
    fn test_sync_state_apply_changes_increment() {
        let mut state = SyncState::new("node1".to_string());

        let change = Change {
            op: "increment".to_string(),
            key: "counter1".to_string(),
            value: None,
            delta: Some(5),
        };

        let request = ChangeRequest {
            changes: vec![change],
        };

        let result = state.apply_changes(request);
        assert!(result.is_ok());

        if let Some(CRDTValue::GCounter(c)) = state.crdt_map.entries.get("counter1") {
            assert_eq!(c.value(), 5);
        }
    }

    #[test]
    fn test_sync_state_apply_changes_decrement() {
        let mut state = SyncState::new("node1".to_string());

        let changes = vec![
            Change {
                op: "increment".to_string(),
                key: "counter1".to_string(),
                value: None,
                delta: Some(10),
            },
            Change {
                op: "decrement".to_string(),
                key: "counter1".to_string(),
                value: None,
                delta: Some(3),
            },
        ];

        let request = ChangeRequest { changes };

        let result = state.apply_changes(request);
        assert!(result.is_ok());

        if let Some(CRDTValue::PNCounter(c)) = state.crdt_map.entries.get("counter1") {
            assert_eq!(c.value(), 7);
        }
    }

    #[test]
    fn test_sync_state_apply_changes_add() {
        let mut state = SyncState::new("node1".to_string());

        let change = Change {
            op: "add".to_string(),
            key: "set1".to_string(),
            value: Some("item1".to_string()),
            delta: None,
        };

        let request = ChangeRequest {
            changes: vec![change],
        };

        let result = state.apply_changes(request);
        assert!(result.is_ok());

        if let Some(CRDTValue::ORSet(s)) = state.crdt_map.entries.get("set1") {
            assert!(s.contains(&"item1".to_string()));
        }
    }

    #[test]
    fn test_sync_state_apply_changes_set() {
        let mut state = SyncState::new("node1".to_string());

        let change = Change {
            op: "set".to_string(),
            key: "register1".to_string(),
            value: Some("test_value".to_string()),
            delta: None,
        };

        let request = ChangeRequest {
            changes: vec![change],
        };

        let result = state.apply_changes(request);
        assert!(result.is_ok());

        if let Some(CRDTValue::LWWRegister(r)) = state.crdt_map.entries.get("register1") {
            assert_eq!(r.get(), Some(&"test_value".to_string()));
        }
    }

    #[test]
    fn test_sync_state_apply_changes_remove() {
        let mut state = SyncState::new("node1".to_string());

        let changes = vec![
            Change {
                op: "add".to_string(),
                key: "set1".to_string(),
                value: Some("item1".to_string()),
                delta: None,
            },
            Change {
                op: "remove".to_string(),
                key: "set1".to_string(),
                value: Some("item1".to_string()),
                delta: None,
            },
        ];

        let request = ChangeRequest { changes };

        let result = state.apply_changes(request);
        assert!(result.is_ok());

        if let Some(CRDTValue::ORSet(s)) = state.crdt_map.entries.get("set1") {
            assert!(!s.contains(&"item1".to_string()));
        }
    }

    #[test]
    fn test_sync_state_apply_changes_error_missing_value() {
        let mut state = SyncState::new("node1".to_string());

        let change = Change {
            op: "add".to_string(),
            key: "set1".to_string(),
            value: None,
            delta: None,
        };

        let request = ChangeRequest {
            changes: vec![change],
        };

        let result = state.apply_changes(request);
        assert!(result.is_err());
    }

    #[test]
    fn test_sync_state_apply_changes_error_unknown_op() {
        let mut state = SyncState::new("node1".to_string());

        let change = Change {
            op: "unknown_op".to_string(),
            key: "test".to_string(),
            value: None,
            delta: None,
        };

        let request = ChangeRequest {
            changes: vec![change],
        };

        let result = state.apply_changes(request);
        assert!(result.is_err());
    }

    #[test]
    fn test_convergence_property() {
        // 测试 CRDT 的收敛性：两个节点以不同顺序合并应该得到相同结果
        let mut state1 = SyncState::new("node1".to_string());
        let mut state2 = SyncState::new("node2".to_string());
        let mut state3 = SyncState::new("node3".to_string());

        let op1 = Operation::GCounterIncrement {
            key: "counter".to_string(),
            node_id: "node1".to_string(),
            delta: 5,
        };
        state1.apply_operation(op1);

        let op2 = Operation::GCounterIncrement {
            key: "counter".to_string(),
            node_id: "node2".to_string(),
            delta: 3,
        };
        state2.apply_operation(op2);

        // state3 先合并 state1，再合并 state2
        state3.merge(&state1);
        state3.merge(&state2);

        // 创建另一个副本，以相反顺序合并
        let mut state4 = SyncState::new("node4".to_string());
        state4.merge(&state2);
        state4.merge(&state1);

        // 两者应该产生相同的状态哈希
        assert_eq!(state3.state_hash(), state4.state_hash());
    }
}
