use anyhow::Result;

// 引入生成的 protobuf 代码
pub mod crdt {
    tonic::include_proto!("crdt");
}

use crdt::crdt_service_client::CrdtServiceClient;
use crdt::*;

#[tokio::main]
async fn main() -> Result<()> {
    // 连接到 gRPC 服务器
    let mut client = CrdtServiceClient::connect("http://127.0.0.1:50051").await?;

    println!("✅ 已连接到 gRPC 服务器");

    // 1. 健康检查
    println!("\n📋 执行健康检查...");
    let health_response = client
        .health_check(HealthCheckRequest {})
        .await?
        .into_inner();
    println!(
        "   状态: {}, 时间戳: {}",
        health_response.status, health_response.timestamp
    );

    // 2. 同步数据变更
    println!("\n📝 同步数据变更...");
    let sync_response = client
        .sync(SyncRequest {
            changes: vec![
                Change {
                    op: "increment".to_string(),
                    key: "counter1".to_string(),
                    value: None,
                    delta: Some(5),
                },
                Change {
                    op: "set".to_string(),
                    key: "name".to_string(),
                    value: Some("Alice".to_string()),
                    delta: None,
                },
                Change {
                    op: "add".to_string(),
                    key: "tags".to_string(),
                    value: Some("rust".to_string()),
                    delta: None,
                },
            ],
        })
        .await?
        .into_inner();
    println!(
        "   成功: {}, 状态哈希: {}",
        sync_response.success, sync_response.state_hash
    );

    // 3. 获取状态哈希
    println!("\n🔍 获取状态哈希...");
    let hash_response = client
        .get_state_hash(GetStateHashRequest {})
        .await?
        .into_inner();
    println!("   状态哈希: {}", hash_response.state_hash);

    // 4. 获取当前状态
    println!("\n📊 获取当前状态...");
    let state_response = client.get_state(GetStateRequest {}).await?.into_inner();
    println!("   节点 ID: {}", state_response.node_id);
    println!("   状态数据大小: {} 字节", state_response.state_data.len());

    // 5. 获取操作日志
    println!("\n📜 获取操作日志...");
    let oplog_response = client.get_op_log(GetOpLogRequest {}).await?.into_inner();
    println!("   操作日志条目数: {}", oplog_response.entries.len());
    for (i, entry) in oplog_response.entries.iter().take(5).enumerate() {
        println!(
            "   [{}] ID: {}, 时间戳: {}",
            i + 1,
            &entry.id[..12],
            entry.timestamp
        );
    }

    // 6. 获取操作历史
    println!("\n📖 获取操作历史...");
    let history_response = client.get_history(GetHistoryRequest {}).await?.into_inner();
    println!("   历史条目数: {}", history_response.entries.len());
    for (i, entry) in history_response.entries.iter().take(5).enumerate() {
        println!(
            "   [{}] {}: {} - {}",
            i + 1,
            entry.operation_type,
            entry.key,
            entry.details
        );
    }

    // 7. 获取冲突信息
    println!("\n⚠️  获取冲突信息...");
    let conflicts_response = client
        .get_conflicts(GetConflictsRequest {})
        .await?
        .into_inner();
    if conflicts_response.conflicts.is_empty() {
        println!("   无冲突");
    } else {
        println!("   冲突数: {}", conflicts_response.conflicts.len());
        for (i, conflict) in conflicts_response.conflicts.iter().enumerate() {
            println!(
                "   [{}] 键: {}, 类型: {}",
                i + 1,
                conflict.key,
                conflict.conflict_type
            );
            println!("       解决方案: {}", conflict.resolution);
        }
    }

    println!("\n✅ gRPC 客户端测试完成！");

    Ok(())
}
