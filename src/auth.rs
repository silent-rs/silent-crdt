use anyhow::{Result, anyhow};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// 用户角色
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Writer,
    Reader,
}

impl Role {
    /// 检查是否有足够的权限
    pub fn has_permission(&self, required: &Role) -> bool {
        matches!(
            (self, required),
            (Role::Admin, _)
                | (Role::Writer, Role::Writer)
                | (Role::Writer, Role::Reader)
                | (Role::Reader, Role::Reader)
        )
    }
}

/// JWT Claims
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,     // 主体（用户ID或节点ID）
    pub role: Role,      // 角色
    pub exp: u64,        // 过期时间
    pub iat: u64,        // 签发时间
    pub node_id: String, // 节点ID
}

/// JWT 管理器
pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validation: Validation,
}

impl JwtManager {
    /// 创建新的 JWT 管理器
    pub fn new(secret: &str) -> Self {
        let encoding_key = EncodingKey::from_secret(secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(secret.as_bytes());
        let validation = Validation::default();

        Self {
            encoding_key,
            decoding_key,
            validation,
        }
    }

    /// 生成 JWT token
    pub fn generate_token(
        &self,
        node_id: String,
        role: Role,
        expires_in_secs: u64,
    ) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = Claims {
            sub: node_id.clone(),
            role,
            exp: now + expires_in_secs,
            iat: now,
            node_id,
        };

        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| anyhow!("Failed to generate token: {}", e))
    }

    /// 验证并解析 JWT token
    pub fn verify_token(&self, token: &str) -> Result<Claims> {
        decode::<Claims>(token, &self.decoding_key, &self.validation)
            .map(|data| data.claims)
            .map_err(|e| anyhow!("Invalid token: {}", e))
    }

    /// 从 Authorization header 中提取 token
    pub fn extract_token(auth_header: &str) -> Result<&str> {
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            Ok(token)
        } else {
            Err(anyhow!("Invalid authorization header format"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_permissions() {
        assert!(Role::Admin.has_permission(&Role::Admin));
        assert!(Role::Admin.has_permission(&Role::Writer));
        assert!(Role::Admin.has_permission(&Role::Reader));

        assert!(!Role::Writer.has_permission(&Role::Admin));
        assert!(Role::Writer.has_permission(&Role::Writer));
        assert!(Role::Writer.has_permission(&Role::Reader));

        assert!(!Role::Reader.has_permission(&Role::Admin));
        assert!(!Role::Reader.has_permission(&Role::Writer));
        assert!(Role::Reader.has_permission(&Role::Reader));
    }

    #[test]
    fn test_jwt_generation_and_verification() {
        let manager = JwtManager::new("test_secret_key");
        let token = manager
            .generate_token("node1".to_string(), Role::Writer, 3600)
            .unwrap();

        let claims = manager.verify_token(&token).unwrap();
        assert_eq!(claims.node_id, "node1");
        assert_eq!(claims.role, Role::Writer);
    }

    #[test]
    fn test_token_extraction() {
        let header = "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...";
        let token = JwtManager::extract_token(header).unwrap();
        assert_eq!(token, "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...");

        let invalid_header = "InvalidFormat token";
        assert!(JwtManager::extract_token(invalid_header).is_err());
    }
}
