//! LensMint Secure Identity Core
//! 
//! Fulfills GSoC Requirement: "Secure device cryptographic key generation using hardware entropy"
//! Uses `ed25519-dalek` v2 to generate uncloneable device identities via OS-level CSPRNG.

use ed25519_dalek::{Signer, Signature, Verifier, SigningKey};
use rand_core::OsRng; // Uses hardware entropy provided by the OS (e.g., /dev/urandom)

pub struct DeviceIdentity {
    signing_key: SigningKey,
}

impl DeviceIdentity {
    /// Generates a new secure identity using hardware-level entropy.
    /// This replaces the vulnerable Python-based hardware_identity.py
    pub fn generate_secure_hardware_key() -> Self {
        // OsRng uses the operating system's Cryptographically Secure Pseudo-Random Number Generator
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        
        DeviceIdentity { signing_key }
    }

    /// Returns the public key (Camera ID) as a hex string for blockchain registration
    pub fn get_camera_id_hex(&self) -> String {
        // In dalek v2, we extract the VerifyingKey (Public Key) from the SigningKey
        let verifying_key = self.signing_key.verifying_key();
        hex::encode(verifying_key.as_bytes())
    }

    /// Cryptographically signs the photo's metadata (or pHash)
    pub fn sign_capture_payload(&self, payload: &[u8]) -> Signature {
        self.signing_key.sign(payload)
    }

    /// Verifies a signature (can be used for sanity checks before uploading to Web3 service)
    pub fn verify_signature(&self, payload: &[u8], signature: &Signature) -> bool {
        let verifying_key = self.signing_key.verifying_key();
        verifying_key.verify(payload, signature).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_entropy_key_generation() {
        let device = DeviceIdentity::generate_secure_hardware_key();
        let camera_id = device.get_camera_id_hex();
        
        // Public key should be 32 bytes, which is 64 hex characters
        assert_eq!(camera_id.len(), 64);
        assert!(camera_id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_device_signature_verification() {
        let device = DeviceIdentity::generate_secure_hardware_key();
        
        // Mock payload (e.g., a perceptual hash + timestamp)
        let mock_phash_payload = b"c7c7383830c7c7c6_1716422400";
        
        // Sign and verify
        let signature = device.sign_capture_payload(mock_phash_payload);
        assert!(device.verify_signature(mock_phash_payload, &signature));
    }
}