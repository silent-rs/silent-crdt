use anyhow::{Result, anyhow};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// 密钥对
#[derive(Clone)]
#[allow(dead_code)]
pub struct KeyPair {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

#[allow(dead_code)]
impl KeyPair {
    /// 生成新的密钥对
    pub fn generate() -> Self {
        // 生成随机的 32 字节密钥
        let mut secret_bytes = [0u8; 32];
        rand::Rng::fill(&mut rand::thread_rng(), &mut secret_bytes);

        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();

        Self {
            signing_key,
            verifying_key,
        }
    }

    /// 从字节创建签名密钥
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self> {
        let signing_key = SigningKey::from_bytes(bytes);
        let verifying_key = signing_key.verifying_key();

        Ok(Self {
            signing_key,
            verifying_key,
        })
    }

    /// 导出公钥字节
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    /// 导出私钥字节
    pub fn secret_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }
}

/// 签名操作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SignedOperation {
    pub id: String,
    pub timestamp: i64,
    pub node_id: String,
    pub operation_type: String,
    pub operation_data: String,
    pub causal_context: String,
    pub signature: String,  // Base64 编码的签名
    pub public_key: String, // Base64 编码的公钥
}

#[allow(dead_code)]
impl SignedOperation {
    /// 创建签名操作
    pub fn new(
        id: String,
        timestamp: i64,
        node_id: String,
        operation_type: String,
        operation_data: String,
        causal_context: String,
        keypair: &KeyPair,
    ) -> Result<Self> {
        // 构造待签名的消息
        let message = Self::construct_message(
            &id,
            timestamp,
            &node_id,
            &operation_type,
            &operation_data,
            &causal_context,
        );

        // 对消息进行哈希
        let hash = Self::hash_message(&message);

        // 签名
        let signature = keypair.signing_key.sign(&hash);
        let signature_base64 = BASE64.encode(signature.to_bytes());
        let public_key_base64 = BASE64.encode(keypair.public_key_bytes());

        Ok(Self {
            id,
            timestamp,
            node_id,
            operation_type,
            operation_data,
            causal_context,
            signature: signature_base64,
            public_key: public_key_base64,
        })
    }

    /// 验证签名
    pub fn verify(&self) -> Result<()> {
        // 解码公钥
        let public_key_bytes = BASE64
            .decode(&self.public_key)
            .map_err(|e| anyhow!("Failed to decode public key: {}", e))?;

        let public_key_array: [u8; 32] = public_key_bytes
            .try_into()
            .map_err(|_| anyhow!("Invalid public key length"))?;

        let verifying_key = VerifyingKey::from_bytes(&public_key_array)
            .map_err(|e| anyhow!("Invalid public key: {}", e))?;

        // 解码签名
        let signature_bytes = BASE64
            .decode(&self.signature)
            .map_err(|e| anyhow!("Failed to decode signature: {}", e))?;

        let signature_array: [u8; 64] = signature_bytes
            .try_into()
            .map_err(|_| anyhow!("Invalid signature length"))?;

        let signature = Signature::from_bytes(&signature_array);

        // 重新构造消息
        let message = Self::construct_message(
            &self.id,
            self.timestamp,
            &self.node_id,
            &self.operation_type,
            &self.operation_data,
            &self.causal_context,
        );

        // 对消息进行哈希
        let hash = Self::hash_message(&message);

        // 验证签名
        verifying_key
            .verify(&hash, &signature)
            .map_err(|e| anyhow!("Signature verification failed: {}", e))
    }

    /// 构造待签名的消息
    fn construct_message(
        id: &str,
        timestamp: i64,
        node_id: &str,
        operation_type: &str,
        operation_data: &str,
        causal_context: &str,
    ) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}",
            id, timestamp, node_id, operation_type, operation_data, causal_context
        )
    }

    /// 对消息进行哈希
    fn hash_message(message: &str) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(message.as_bytes());
        hasher.finalize().to_vec()
    }
}

/// 签名管理器
#[allow(dead_code)]
pub struct SignatureManager {
    keypair: KeyPair,
    node_id: String,
}

#[allow(dead_code)]
impl SignatureManager {
    /// 创建新的签名管理器
    pub fn new(node_id: String) -> Self {
        let keypair = KeyPair::generate();
        Self { keypair, node_id }
    }

    /// 从现有密钥创建签名管理器
    pub fn from_keypair(node_id: String, keypair: KeyPair) -> Self {
        Self { keypair, node_id }
    }

    /// 签名操作
    pub fn sign_operation(
        &self,
        id: String,
        timestamp: i64,
        operation_type: String,
        operation_data: String,
        causal_context: String,
    ) -> Result<SignedOperation> {
        SignedOperation::new(
            id,
            timestamp,
            self.node_id.clone(),
            operation_type,
            operation_data,
            causal_context,
            &self.keypair,
        )
    }

    /// 获取公钥（Base64 编码）
    pub fn public_key_base64(&self) -> String {
        BASE64.encode(self.keypair.public_key_bytes())
    }

    /// 获取私钥（Base64 编码）
    pub fn secret_key_base64(&self) -> String {
        BASE64.encode(self.keypair.secret_key_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let keypair = KeyPair::generate();
        let public_key = keypair.public_key_bytes();
        let secret_key = keypair.secret_key_bytes();

        assert_eq!(public_key.len(), 32);
        assert_eq!(secret_key.len(), 32);
    }

    #[test]
    fn test_sign_and_verify() {
        let manager = SignatureManager::new("node1".to_string());

        let signed_op = manager
            .sign_operation(
                "op1".to_string(),
                1234567890,
                "LWWRegister.Set".to_string(),
                "key=value".to_string(),
                "{}".to_string(),
            )
            .unwrap();

        // 验证签名应该成功
        assert!(signed_op.verify().is_ok());
    }

    #[test]
    fn test_tampered_signature_fails() {
        let manager = SignatureManager::new("node1".to_string());

        let mut signed_op = manager
            .sign_operation(
                "op1".to_string(),
                1234567890,
                "LWWRegister.Set".to_string(),
                "key=value".to_string(),
                "{}".to_string(),
            )
            .unwrap();

        // 篡改操作数据
        signed_op.operation_data = "key=tampered".to_string();

        // 验证签名应该失败
        assert!(signed_op.verify().is_err());
    }

    #[test]
    fn test_keypair_from_bytes() {
        let keypair1 = KeyPair::generate();
        let secret_bytes = keypair1.secret_key_bytes();

        let keypair2 = KeyPair::from_bytes(&secret_bytes).unwrap();

        assert_eq!(keypair1.public_key_bytes(), keypair2.public_key_bytes());
    }
}
