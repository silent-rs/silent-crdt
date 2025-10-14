use crate::sync::SyncState;
use anyhow::{Context, Result};
use sled::Db;

/// 存储管理器
pub struct Storage {
    db: Db,
}

impl Storage {
    /// 创建或打开存储
    pub fn new(path: &str) -> Result<Self> {
        let db =
            sled::open(path).with_context(|| format!("Failed to open database at {}", path))?;
        Ok(Self { db })
    }

    /// 保存同步状态
    pub fn save_state(&self, node_id: &str, state: &SyncState) -> Result<()> {
        let key = format!("state:{}", node_id);
        let value = serde_json::to_vec(state).context("Failed to serialize sync state")?;

        self.db
            .insert(key.as_bytes(), value)
            .context("Failed to insert state into database")?;

        self.db.flush().context("Failed to flush database")?;

        tracing::info!("Saved state for node: {}", node_id);
        Ok(())
    }

    /// 加载同步状态
    pub fn load_state(&self, node_id: &str) -> Result<Option<SyncState>> {
        let key = format!("state:{}", node_id);

        if let Some(value) = self
            .db
            .get(key.as_bytes())
            .context("Failed to get state from database")?
        {
            let state =
                serde_json::from_slice(&value).context("Failed to deserialize sync state")?;
            tracing::info!("Loaded state for node: {}", node_id);
            Ok(Some(state))
        } else {
            tracing::info!("No saved state found for node: {}", node_id);
            Ok(None)
        }
    }

    /// 保存快照（用于版本记录）
    #[allow(dead_code)]
    pub fn save_snapshot(&self, node_id: &str, version: u64, state: &SyncState) -> Result<()> {
        let key = format!("snapshot:{}:{}", node_id, version);
        let value = serde_json::to_vec(state).context("Failed to serialize snapshot")?;

        self.db
            .insert(key.as_bytes(), value)
            .context("Failed to insert snapshot into database")?;

        self.db.flush().context("Failed to flush database")?;

        tracing::info!("Saved snapshot for node: {} version: {}", node_id, version);
        Ok(())
    }

    /// 加载快照
    #[allow(dead_code)]
    pub fn load_snapshot(&self, node_id: &str, version: u64) -> Result<Option<SyncState>> {
        let key = format!("snapshot:{}:{}", node_id, version);

        if let Some(value) = self
            .db
            .get(key.as_bytes())
            .context("Failed to get snapshot from database")?
        {
            let state = serde_json::from_slice(&value).context("Failed to deserialize snapshot")?;
            tracing::info!("Loaded snapshot for node: {} version: {}", node_id, version);
            Ok(Some(state))
        } else {
            tracing::info!(
                "No snapshot found for node: {} version: {}",
                node_id,
                version
            );
            Ok(None)
        }
    }

    /// 列出节点的所有快照版本
    #[allow(dead_code)]
    pub fn list_snapshots(&self, node_id: &str) -> Result<Vec<u64>> {
        let prefix = format!("snapshot:{}:", node_id);
        let mut versions = Vec::new();

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, _) = item.context("Failed to scan database")?;
            let key_str = String::from_utf8_lossy(&key);

            if let Some(version_str) = key_str.split(':').nth(2)
                && let Ok(version) = version_str.parse::<u64>()
            {
                versions.push(version);
            }
        }

        versions.sort();
        Ok(versions)
    }

    /// 删除旧快照（保留最新的 N 个）
    #[allow(dead_code)]
    pub fn cleanup_old_snapshots(&self, node_id: &str, keep: usize) -> Result<()> {
        let mut versions = self.list_snapshots(node_id)?;

        if versions.len() <= keep {
            return Ok(());
        }

        versions.sort();
        let to_delete = &versions[..versions.len() - keep];

        for version in to_delete {
            let key = format!("snapshot:{}:{}", node_id, version);
            self.db
                .remove(key.as_bytes())
                .context("Failed to remove old snapshot")?;
            tracing::info!("Deleted old snapshot: node={} version={}", node_id, version);
        }

        self.db.flush().context("Failed to flush database")?;

        Ok(())
    }

    /// 导出操作日志到文件
    #[allow(dead_code)]
    pub fn export_oplog(&self, node_id: &str, output_path: &str) -> Result<()> {
        if let Some(state) = self.load_state(node_id)? {
            let oplog_json = state
                .export_oplog()
                .context("Failed to export operation log")?;

            std::fs::write(output_path, oplog_json)
                .with_context(|| format!("Failed to write operation log to {}", output_path))?;

            tracing::info!("Exported operation log to: {}", output_path);
        } else {
            tracing::warn!("No state found for node: {}", node_id);
        }
        Ok(())
    }

    /// 清空所有数据
    #[allow(dead_code)]
    pub fn clear_all(&self) -> Result<()> {
        self.db.clear().context("Failed to clear database")?;
        self.db.flush().context("Failed to flush database")?;
        tracing::info!("Cleared all data from storage");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::SyncState;

    #[test]
    fn test_storage_basic() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let storage = Storage::new(temp_dir.path().to_str().unwrap())?;

        let node_id = "test-node";
        let state = SyncState::new(node_id.to_string());

        // 保存状态
        storage.save_state(node_id, &state)?;

        // 加载状态
        let loaded = storage.load_state(node_id)?;
        assert!(loaded.is_some());

        Ok(())
    }

    #[test]
    fn test_snapshot_management() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let storage = Storage::new(temp_dir.path().to_str().unwrap())?;

        let node_id = "test-node";
        let state = SyncState::new(node_id.to_string());

        // 保存多个快照
        for i in 1..=5 {
            storage.save_snapshot(node_id, i, &state)?;
        }

        // 列出快照
        let versions = storage.list_snapshots(node_id)?;
        assert_eq!(versions.len(), 5);

        // 清理旧快照
        storage.cleanup_old_snapshots(node_id, 3)?;

        let versions = storage.list_snapshots(node_id)?;
        assert_eq!(versions.len(), 3);

        Ok(())
    }

    #[test]
    fn test_load_snapshot() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let storage = Storage::new(temp_dir.path().to_str().unwrap())?;

        let node_id = "test-node";
        let mut state = SyncState::new(node_id.to_string());

        // 应用一些操作
        use crate::sync::{Change, ChangeRequest};
        state
            .apply_changes(ChangeRequest {
                changes: vec![Change {
                    op: "increment".to_string(),
                    key: "counter".to_string(),
                    value: None,
                    delta: Some(5),
                }],
            })
            .map_err(|e| anyhow::anyhow!(e))?;

        // 保存快照
        storage.save_snapshot(node_id, 1, &state)?;

        // 加载快照
        let loaded = storage.load_snapshot(node_id, 1)?;
        assert!(loaded.is_some());

        // 尝试加载不存在的快照
        let not_found = storage.load_snapshot(node_id, 999)?;
        assert!(not_found.is_none());

        Ok(())
    }

    #[test]
    fn test_state_persistence() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let storage = Storage::new(temp_dir.path().to_str().unwrap())?;

        let node_id = "test-node";
        let mut state = SyncState::new(node_id.to_string());

        // 添加一些数据
        use crate::sync::{Change, ChangeRequest};
        state
            .apply_changes(ChangeRequest {
                changes: vec![Change {
                    op: "increment".to_string(),
                    key: "counter".to_string(),
                    value: None,
                    delta: Some(10),
                }],
            })
            .map_err(|e| anyhow::anyhow!(e))?;

        // 保存状态
        storage.save_state(node_id, &state)?;

        // 确认状态存在
        let loaded = storage.load_state(node_id)?;
        assert!(loaded.is_some());

        // 验证状态哈希相同
        let loaded_state = loaded.unwrap();
        assert_eq!(state.state_hash(), loaded_state.state_hash());

        Ok(())
    }

    #[test]
    fn test_clear_all() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let storage = Storage::new(temp_dir.path().to_str().unwrap())?;

        // 保存多个节点的状态
        for i in 1..=3 {
            let node_id = format!("node-{}", i);
            let state = SyncState::new(node_id.clone());
            storage.save_state(&node_id, &state)?;
        }

        // 清空所有数据
        storage.clear_all()?;

        // 验证所有状态都已清空
        for i in 1..=3 {
            let node_id = format!("node-{}", i);
            assert!(storage.load_state(&node_id)?.is_none());
        }

        Ok(())
    }

    #[test]
    fn test_load_nonexistent_state() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let storage = Storage::new(temp_dir.path().to_str().unwrap())?;

        // 尝试加载不存在的状态
        let result = storage.load_state("nonexistent-node")?;
        assert!(result.is_none());

        Ok(())
    }

    #[test]
    fn test_list_snapshots_empty() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let storage = Storage::new(temp_dir.path().to_str().unwrap())?;

        let node_id = "test-node";

        // 列出空快照列表
        let versions = storage.list_snapshots(node_id)?;
        assert_eq!(versions.len(), 0);

        Ok(())
    }
}
