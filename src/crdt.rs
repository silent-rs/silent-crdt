use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// 节点 ID 类型
pub type NodeId = String;

/// 向量时钟，用于因果关系追踪
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VectorClock {
    pub clocks: HashMap<NodeId, u64>,
}

impl VectorClock {
    pub fn new() -> Self {
        Self {
            clocks: HashMap::new(),
        }
    }

    pub fn increment(&mut self, node_id: &str) {
        *self.clocks.entry(node_id.to_string()).or_insert(0) += 1;
    }

    #[allow(dead_code)]
    pub fn get(&self, node_id: &str) -> u64 {
        self.clocks.get(node_id).copied().unwrap_or(0)
    }

    pub fn merge(&mut self, other: &VectorClock) {
        for (node, &clock) in &other.clocks {
            let entry = self.clocks.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(clock);
        }
    }

    /// 判断是否发生在另一个向量时钟之前
    #[allow(dead_code)]
    pub fn happens_before(&self, other: &VectorClock) -> bool {
        let mut at_least_one_less = false;
        for (node, &clock) in &self.clocks {
            let other_clock = other.get(node);
            if clock > other_clock {
                return false;
            }
            if clock < other_clock {
                at_least_one_less = true;
            }
        }
        for node in other.clocks.keys() {
            if !self.clocks.contains_key(node) && other.get(node) > 0 {
                at_least_one_less = true;
            }
        }
        at_least_one_less
    }

    /// 判断是否并发
    #[allow(dead_code)]
    pub fn is_concurrent(&self, other: &VectorClock) -> bool {
        !self.happens_before(other) && !other.happens_before(self) && self != other
    }
}

impl Default for VectorClock {
    fn default() -> Self {
        Self::new()
    }
}

/// GCounter - 增长计数器
/// 只能递增的计数器，支持分布式环境下的最终一致性
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GCounter {
    pub counts: HashMap<NodeId, u64>,
}

impl GCounter {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    pub fn increment(&mut self, node_id: &str, delta: u64) {
        *self.counts.entry(node_id.to_string()).or_insert(0) += delta;
    }

    #[allow(dead_code)]
    pub fn value(&self) -> u64 {
        self.counts.values().sum()
    }

    pub fn merge(&mut self, other: &GCounter) {
        for (node, &count) in &other.counts {
            let entry = self.counts.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(count);
        }
    }

    pub fn state_hash(&self) -> String {
        let mut hasher = Sha256::new();
        let mut sorted: Vec<_> = self.counts.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        for (node, count) in sorted {
            hasher.update(node.as_bytes());
            hasher.update(count.to_le_bytes());
        }
        hex::encode(hasher.finalize())
    }
}

impl Default for GCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// PNCounter - 正负计数器
/// 支持递增和递减操作的计数器
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PNCounter {
    pub positive: GCounter,
    pub negative: GCounter,
}

impl PNCounter {
    pub fn new() -> Self {
        Self {
            positive: GCounter::new(),
            negative: GCounter::new(),
        }
    }

    pub fn increment(&mut self, node_id: &str, delta: u64) {
        self.positive.increment(node_id, delta);
    }

    pub fn decrement(&mut self, node_id: &str, delta: u64) {
        self.negative.increment(node_id, delta);
    }

    #[allow(dead_code)]
    pub fn value(&self) -> i64 {
        self.positive.value() as i64 - self.negative.value() as i64
    }

    pub fn merge(&mut self, other: &PNCounter) {
        self.positive.merge(&other.positive);
        self.negative.merge(&other.negative);
    }

    pub fn state_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"positive:");
        hasher.update(self.positive.state_hash().as_bytes());
        hasher.update(b"negative:");
        hasher.update(self.negative.state_hash().as_bytes());
        hex::encode(hasher.finalize())
    }
}

impl Default for PNCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// LWW-Register - 最后写入胜出寄存器
/// 使用时间戳来解决冲突，最新的写入胜出
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LWWRegister<T> {
    pub value: Option<T>,
    pub timestamp: i64,
    pub node_id: NodeId,
}

impl<T: Clone> LWWRegister<T> {
    pub fn new() -> Self {
        Self {
            value: None,
            timestamp: 0,
            node_id: String::new(),
        }
    }

    pub fn set(&mut self, value: T, timestamp: i64, node_id: &str) {
        self.value = Some(value);
        self.timestamp = timestamp;
        self.node_id = node_id.to_string();
    }

    pub fn get(&self) -> Option<&T> {
        self.value.as_ref()
    }

    pub fn merge(&mut self, other: &LWWRegister<T>) {
        if other.timestamp > self.timestamp
            || (other.timestamp == self.timestamp && other.node_id > self.node_id)
        {
            self.value = other.value.clone();
            self.timestamp = other.timestamp;
            self.node_id = other.node_id.clone();
        }
    }
}

impl<T: Clone> Default for LWWRegister<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// OR-Set - 观察移除集合
/// 使用唯一标识符来追踪每个元素的添加和删除
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ORSet<T: Eq + std::hash::Hash> {
    pub added: HashMap<T, HashSet<String>>, // 元素 -> 唯一标识符集合
    pub removed: HashSet<String>,           // 已删除的唯一标识符
}

// 手动实现 Serialize 和 Deserialize
impl<T: Eq + std::hash::Hash + Serialize> Serialize for ORSet<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ORSet", 2)?;
        state.serialize_field("added", &self.added)?;
        state.serialize_field("removed", &self.removed)?;
        state.end()
    }
}

impl<'de, T> Deserialize<'de> for ORSet<T>
where
    T: Deserialize<'de> + Eq + std::hash::Hash,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct ORSetVisitor<T> {
            marker: std::marker::PhantomData<T>,
        }

        impl<'de, T> Visitor<'de> for ORSetVisitor<T>
        where
            T: Deserialize<'de> + Eq + std::hash::Hash,
        {
            type Value = ORSet<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct ORSet")
            }

            fn visit_map<V>(self, mut map: V) -> Result<ORSet<T>, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut added = None;
                let mut removed = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "added" => {
                            if added.is_some() {
                                return Err(de::Error::duplicate_field("added"));
                            }
                            added = Some(map.next_value()?);
                        }
                        "removed" => {
                            if removed.is_some() {
                                return Err(de::Error::duplicate_field("removed"));
                            }
                            removed = Some(map.next_value()?);
                        }
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                let added = added.ok_or_else(|| de::Error::missing_field("added"))?;
                let removed = removed.ok_or_else(|| de::Error::missing_field("removed"))?;
                Ok(ORSet { added, removed })
            }
        }

        deserializer.deserialize_struct(
            "ORSet",
            &["added", "removed"],
            ORSetVisitor {
                marker: std::marker::PhantomData,
            },
        )
    }
}

impl<T: Clone + Eq + std::hash::Hash> ORSet<T> {
    pub fn new() -> Self {
        Self {
            added: HashMap::new(),
            removed: HashSet::new(),
        }
    }

    pub fn add(&mut self, value: T, unique_id: String) {
        self.added.entry(value).or_default().insert(unique_id);
    }

    pub fn remove(&mut self, value: &T) {
        if let Some(ids) = self.added.get(value) {
            for id in ids {
                self.removed.insert(id.clone());
            }
        }
    }

    #[allow(dead_code)]
    pub fn contains(&self, value: &T) -> bool {
        if let Some(ids) = self.added.get(value) {
            ids.iter().any(|id| !self.removed.contains(id))
        } else {
            false
        }
    }

    pub fn elements(&self) -> Vec<T> {
        self.added
            .iter()
            .filter_map(|(value, ids)| {
                if ids.iter().any(|id| !self.removed.contains(id)) {
                    Some(value.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn merge(&mut self, other: &ORSet<T>) {
        for (value, ids) in &other.added {
            self.added
                .entry(value.clone())
                .or_default()
                .extend(ids.clone());
        }
        self.removed.extend(other.removed.clone());
    }
}

impl<T: Clone + Eq + std::hash::Hash> Default for ORSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// CRDT Map - 支持多种 CRDT 类型的映射
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CRDTValue {
    GCounter(GCounter),
    PNCounter(PNCounter),
    LWWRegister(LWWRegister<String>),
    ORSet(ORSet<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CRDTMap {
    pub entries: HashMap<String, CRDTValue>,
    pub vector_clock: VectorClock,
}

impl CRDTMap {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            vector_clock: VectorClock::new(),
        }
    }

    #[allow(dead_code)]
    pub fn get(&self, key: &str) -> Option<&CRDTValue> {
        self.entries.get(key)
    }

    #[allow(dead_code)]
    pub fn set(&mut self, key: String, value: CRDTValue) {
        self.entries.insert(key, value);
    }

    pub fn merge(&mut self, other: &CRDTMap) {
        for (key, other_value) in &other.entries {
            match (self.entries.get_mut(key), other_value) {
                (Some(CRDTValue::GCounter(a)), CRDTValue::GCounter(b)) => a.merge(b),
                (Some(CRDTValue::PNCounter(a)), CRDTValue::PNCounter(b)) => a.merge(b),
                (Some(CRDTValue::LWWRegister(a)), CRDTValue::LWWRegister(b)) => a.merge(b),
                (Some(CRDTValue::ORSet(a)), CRDTValue::ORSet(b)) => a.merge(b),
                (None, _) => {
                    self.entries.insert(key.clone(), other_value.clone());
                }
                _ => {
                    // 类型不匹配，保持不变或采用其他策略
                }
            }
        }
        self.vector_clock.merge(&other.vector_clock);
    }

    pub fn state_hash(&self) -> String {
        let mut hasher = Sha256::new();
        let mut sorted: Vec<_> = self.entries.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        for (key, value) in sorted {
            hasher.update(key.as_bytes());
            match value {
                CRDTValue::GCounter(c) => hasher.update(c.state_hash().as_bytes()),
                CRDTValue::PNCounter(c) => hasher.update(c.state_hash().as_bytes()),
                CRDTValue::LWWRegister(r) => {
                    if let Some(v) = r.get() {
                        hasher.update(v.as_bytes());
                    }
                    hasher.update(r.timestamp.to_le_bytes());
                }
                CRDTValue::ORSet(s) => {
                    let mut elements = s.elements();
                    elements.sort();
                    for elem in elements {
                        hasher.update(elem.as_bytes());
                    }
                }
            }
        }
        hex::encode(hasher.finalize())
    }
}

impl Default for CRDTMap {
    fn default() -> Self {
        Self::new()
    }
}
