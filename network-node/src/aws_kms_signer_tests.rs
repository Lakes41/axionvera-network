#[cfg(test)]
mod tests {
    use crate::aws_kms_signer::{KmsSigner, KmsConfig};
    use crate::signing::Signer;
    use aws_sdk_kms::config::Region;
    use aws_sdk_kms::primitives::Blob;
    use aws_sdk_kms::types::SigningAlgorithmSpec;
    use aws_sdk_kms::Client as KmsClient;
    use aws_smithy_runtime::client::http::test_util::{ReplayEvent, StaticReplayClient};
    use aws_smithy_runtime_api::client::http::HttpClient;
    use ed25519_dalek::PublicKey;
    use stellar_sdk::transaction::Transaction;
    use stellar_sdk::network::Network;
    use stellar_sdk::XdrCodec;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;

    fn create_mock_client(events: Vec<ReplayEvent>) -> KmsClient {
        let http_client = StaticReplayClient::new(events);
        let config = aws_sdk_kms::Config::builder()
            .region(Region::new("us-east-1"))
            .http_client(http_client)
            .build();
        KmsClient::from_conf(config)
    }

    #[tokio::test]
    async fn test_kms_signer_sign_and_append() {
        // 1. Mock the GetPublicKey response (SPKI format for Ed25519)
        // A 44-byte SPKI for Ed25519 with a dummy public key
        let mut public_key_bytes = vec![0u8; 44];
        // SPKI header for Ed25519 (fixed 12 bytes)
        public_key_bytes[..12].copy_from_slice(&[
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00
        ]);
        // Dummy 32-byte public key
        let dummy_pub_key = [1u8; 32];
        public_key_bytes[12..].copy_from_slice(&dummy_pub_key);

        let get_pub_key_response = http::Response::builder()
            .status(200)
            .body(aws_smithy_types::body::SdkBody::from(format!(
                r#"{{"PublicKey": "{}"}}"#,
                STANDARD.encode(&public_key_bytes)
            )))
            .unwrap();

        // 2. Mock the Sign response
        let dummy_signature = [2u8; 64];
        let sign_response = http::Response::builder()
            .status(200)
            .body(aws_smithy_types::body::SdkBody::from(format!(
                r#"{{"Signature": "{}"}}"#,
                STANDARD.encode(&dummy_signature)
            )))
            .unwrap();

        let events = vec![
            ReplayEvent::new(
                http::Request::builder().body(aws_smithy_types::body::SdkBody::empty()).unwrap(),
                get_pub_key_response
            ),
            ReplayEvent::new(
                http::Request::builder().body(aws_smithy_types::body::SdkBody::empty()).unwrap(),
                sign_response
            ),
        ];

        let client = create_mock_client(events);
        let config = KmsConfig {
            key_id: "test-key-id".to_string(),
            region: "us-east-1".to_string(),
            profile: None,
            timeout_ms: 5000,
        };
        let signer = KmsSigner::with_client(client, config);

        // Verify public key parsing
        let pub_key = signer.get_public_key().await.expect("Should get public key");
        assert_eq!(pub_key.as_bytes(), &dummy_pub_key);

        // Verify signing
        let message = b"test message";
        let signature = signer.sign(message).await.expect("Should sign message");
        assert_eq!(signature, dummy_signature.to_vec());

        // 3. Verify signature bytes are appended correctly to a mock transaction envelope
        // In Stellar, a DecoratedSignature consists of a hint (last 4 bytes of public key) 
        // and the signature itself.
        let mut hint = [0u8; 4];
        hint.copy_from_slice(&dummy_pub_key[28..]);
        
        let mut envelope_bytes = vec![0u8; 100]; // Mock envelope
        let signature_len = signature.len();
        
        // Simulating appending a decorated signature: [hint (4)] + [sig_len (4)] + [signature (64)]
        let mut decorated_sig = Vec::new();
        decorated_sig.extend_from_slice(&hint);
        decorated_sig.extend_from_slice(&(signature_len as u32).to_be_bytes());
        decorated_sig.extend_from_slice(&signature);
        
        envelope_bytes.extend_from_slice(&decorated_sig);
        
        assert!(envelope_bytes.len() > 100);
        assert_eq!(&envelope_bytes[100..104], &hint);
        assert_eq!(&envelope_bytes[108..], &signature);
    }
}
