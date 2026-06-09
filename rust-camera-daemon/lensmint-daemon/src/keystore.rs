use ed25519_dalek::{pkcs8::DecodePrivateKey, pkcs8::EncodePrivateKey, Signature, Signer, SigningKey};
use rand::rngs::OsRng;
use std::fs::{self, OpenOptions};
use std::io::Write;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub struct LocalKeystore {
    pub signing_key: SigningKey,
}

impl LocalKeystore {
    /// Loads the Ed25519 Keypair from the standard config directory (e.g. `~/.local/share/lensmint/keystore.pem`).
    /// Generates a new identity if it doesn't exist and enforces 0400 Unix file locks.
    pub fn load_or_generate() -> Result<Self, Box<dyn std::error::Error>> {
        // Use directory struct leveraging XDG standards
        let proj_dirs = directories::ProjectDirs::from("", "", "lensmint")
            .ok_or("Could not resolve local App directories")?;
            
        let config_dir = proj_dirs.data_dir();

        if !config_dir.exists() {
            fs::create_dir_all(config_dir)?;
        }

        let key_path = config_dir.join("keystore.pem");

        if key_path.exists() {
            let pem_str = fs::read_to_string(&key_path)?;
            let signing_key = SigningKey::from_pkcs8_pem(&pem_str)?;
            Ok(Self { signing_key })
        } else {
            let mut csprng = OsRng;
            let signing_key = SigningKey::generate(&mut csprng);

            // Encode to PEM Format
            let pem_str = signing_key.to_pkcs8_pem(Default::default())?;

            // File FD creation specifically utilizing exclusive write locking.
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&key_path)?;

            #[cfg(unix)]
            {
                // Enforce POSIX mask locking
                let mut perms = file.metadata()?.permissions();
                perms.set_mode(0o400); // 0400: strict read-only for current owner 
                file.set_permissions(perms)?;
            }

            file.write_all(pem_str.as_bytes())?;

            Ok(Self { signing_key })
        }
    }

    /// Sign off-chain event data before network transit
    pub fn sign_payload(&self, payload: &[u8]) -> Signature {
        self.signing_key.sign(payload)
    }
}

// =====================================
// TDD: Unit Tests Lifecycle 
// =====================================
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Verifier;

    #[test]
    fn test_sign_and_verify_success() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let keystore = LocalKeystore {
            signing_key: signing_key.clone(),
        };

        let mock_hash = b"0x21a4f00d23..."; // Hypothetical perceptual hashing payload
        let signature = keystore.sign_payload(mock_hash);

        // A valid validating node public key MUST pass
        let verifying_key = signing_key.verifying_key();
        assert!(
            verifying_key.verify(mock_hash, &signature).is_ok(),
            "Signature payload mismatch with hardware identity."
        );
    }
    
    #[test]
    fn test_sign_and_verify_forged_failure() {
        let mut csprng = OsRng;
        let original_key = SigningKey::generate(&mut csprng);
        let keystore = LocalKeystore {
            signing_key: original_key,
        };
        
        let signature = keystore.sign_payload(b"Valid Data");
        
        // Mock a hacker forged or different camera node verifying key
        let hacker_key = SigningKey::generate(&mut csprng);
        let hacker_pub = hacker_key.verifying_key();
        
        assert!(
            hacker_pub.verify(b"Valid Data", &signature).is_err(),
            "Forged payload authentication should fail!"
        );
    }
}
