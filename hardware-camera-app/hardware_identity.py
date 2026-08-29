#!/usr/bin/env python3

import os
import hashlib
import subprocess
import stat
from pathlib import Path

try:
    from ecdsa import SigningKey, SECP256k1, VerifyingKey
    ECDSA_AVAILABLE = True
except ImportError:
    ECDSA_AVAILABLE = False
    print("Warning: ecdsa library not available. Install with: pip3 install ecdsa")
    print("   Hardware signing features will be disabled")

# Try to import keccak256 for Ethereum address calculation
KECCAK_AVAILABLE = False
try:
    # Try pysha3 first (most common)
    import sha3
    KECCAK_AVAILABLE = True
except ImportError:
    try:
        # Try pycryptodome
        from Crypto.Hash import keccak
        KECCAK_AVAILABLE = True
    except ImportError:
        KECCAK_AVAILABLE = False
        print("Warning: keccak256 not available. Install with: pip3 install pysha3")
        print("   Address calculation will use SHA256 (won't match Ethereum addresses)")

SALT_PATH = os.getenv('SALT_PATH', '/boot/.device_salt')
SALT_BACKUP_PATH = Path(os.getenv('SALT_BACKUP_PATH', str(Path.home() / ".lensmint" / ".device_salt_backup")))

class HardwareIdentity:
    
    def __init__(self, camera_id=None):
        self.salt = None
        self.private_key = None
        self.public_key = None
        self.address = None
        self.initialized = False
        self.camera_id = camera_id
        
        if not ECDSA_AVAILABLE:
            raise RuntimeError("ecdsa library required. Install with: pip3 install ecdsa")
        
        self._initialize()
    
    def _initialize(self):
        try:
            self.salt = self._get_or_create_salt()
            hw_id = self._get_hardware_id()
            self.private_key, self.public_key = self._derive_key(hw_id, self.salt)
            self.address = self._get_address()
            
            self.initialized = True
            print(f"Hardware identity initialized. Address: {self.address[:16]}...")
            
        except Exception as e:
            print(f"Error initializing hardware identity: {e}")
            raise
    
    def _get_or_create_salt(self):
        if os.path.exists(SALT_PATH):
            try:
                with open(SALT_PATH, "rb") as f:
                    salt = f.read()
                if len(salt) == 32:
                    print(f"✓ Salt loaded from {SALT_PATH}")
                    return salt
            except PermissionError:
                print(f"⚠ Permission denied reading {SALT_PATH}, trying backup...")
        
        if SALT_BACKUP_PATH.exists():
            try:
                with open(SALT_BACKUP_PATH, "rb") as f:
                    salt = f.read()
                if len(salt) == 32:
                    print(f"✓ Salt loaded from backup: {SALT_BACKUP_PATH}")
                    return salt
            except Exception as e:
                print(f"⚠ Error reading backup salt: {e}")
        
        print("Creating new device salt...")
        salt = os.urandom(32)
        
        try:
            with open(SALT_PATH, "wb") as f:
                f.write(salt)
            os.chmod(SALT_PATH, stat.S_IRUSR | stat.S_IWUSR)
            print(f"✓ Salt saved to {SALT_PATH} (read-only)")
        except (PermissionError, OSError) as e:
            print(f"⚠ Cannot write to {SALT_PATH}: {e}")
            print("   Saving to user directory instead...")
            SALT_BACKUP_PATH.parent.mkdir(parents=True, exist_ok=True)
            with open(SALT_BACKUP_PATH, "wb") as f:
                f.write(salt)
            os.chmod(SALT_BACKUP_PATH, stat.S_IRUSR | stat.S_IWUSR)
            print(f"✓ Salt saved to backup: {SALT_BACKUP_PATH}")
        
        return salt
    
    def _get_hardware_id(self):
        identifiers = []
        
        if self.camera_id:
            identifiers.append(f"camera:{self.camera_id}")
            print(f"✓ Camera ID: {self.camera_id}")
        
        try:
            result = subprocess.run(
                ["cat", "/proc/cpuinfo"],
                capture_output=True,
                text=True,
                timeout=2
            )
            for line in result.stdout.split('\n'):
                if 'Serial' in line:
                    serial = line.split(':')[1].strip()
                    identifiers.append(f"serial:{serial}")
                    print(f"✓ CPU Serial: {serial[:16]}...")
                    break
        except Exception as e:
            print(f"⚠ Could not read CPU serial: {e}")
        
        for interface in ['wlan0', 'eth0']:
            try:
                mac_path = f"/sys/class/net/{interface}/address"
                if os.path.exists(mac_path):
                    with open(mac_path, 'r') as f:
                        mac = f.read().strip()
                        identifiers.append(f"mac:{mac}")
                        print(f"✓ MAC Address ({interface}): {mac}")
                        break
            except Exception as e:
                continue
        
        try:
            if os.path.exists("/etc/machine-id"):
                with open("/etc/machine-id", 'r') as f:
                    machine_id = f.read().strip()
                    identifiers.append(f"machine:{machine_id}")
                    print(f"✓ Machine ID: {machine_id[:16]}...")
        except Exception as e:
            pass
        
        if not identifiers:
            raise RuntimeError("Could not collect any hardware identifiers")
        
        hw_string = "|".join(identifiers)
        print(f"Hardware ID components: {len(identifiers)} found")
        
        return hw_string.encode('utf-8')
    
    def _derive_key(self, hw_id, salt):
        combined = hw_id + salt
        seed = hashlib.sha256(combined).digest()
        private_key = SigningKey.from_string(seed, curve=SECP256k1)
        public_key = private_key.get_verifying_key()
        return private_key, public_key
    
    def _get_address(self):
        pub_key_bytes = self.public_key.to_string()
        
        if KECCAK_AVAILABLE:
            try:
                # Try pysha3
                try:
                    import sha3
                    k = sha3.keccak_256()
                    k.update(pub_key_bytes)
                    hash_bytes = k.digest()
                except:
                    from Crypto.Hash import keccak
                    k = keccak.new(digest_bits=256)
                    k.update(pub_key_bytes)
                    hash_bytes = k.digest()
                
                address = hash_bytes[-20:].hex()
                return f"0x{address}"
            except Exception as e:
                print(f"Warning: keccak256 calculation failed: {e}, using SHA256 fallback")
        
        address_hash = hashlib.sha256(pub_key_bytes).hexdigest()[:40]
        print("⚠️ Using SHA256 for address (install pysha3 for keccak256)")
        return f"0x{address_hash}"
    
    def _get_ethereum_message_hash(self, message):
        prefix = f"\x19Ethereum Signed Message:\n{len(message)}".encode('utf-8')
        prefixed_message = prefix + message
        
        if KECCAK_AVAILABLE:
            try:
                import sha3
                k = sha3.keccak_256()
                k.update(prefixed_message)
                return k.digest()
            except ImportError:
                from Crypto.Hash import keccak
                k = keccak.new(digest_bits=256)
                k.update(prefixed_message)
                return k.digest()
        else:
            return hashlib.sha256(prefixed_message).digest()

    def _get_v_value(self, r, s, message_hash):
        # Determine the recovery ID (v)
        # ecdsa doesn't give us v easily, so we try both to see which recovers our public key
        sig = r + s
        for v in [0, 1]:
            try:
                vk = VerifyingKey.from_public_key_recovery(sig, message_hash, SECP256k1, hashfunc=hashlib.sha256)[v]
                if vk.to_string() == self.public_key.to_string():
                    return v + 27
            except Exception:
                pass
        return 27 # fallback, though might fail validation

    def sign_data(self, data):
        if not self.initialized:
            raise RuntimeError("Hardware identity not initialized")
        
        if isinstance(data, str):
            data = data.encode('utf-8')
        
        message_hash = self._get_ethereum_message_hash(data)
        signature = self.private_key.sign_digest_deterministic(message_hash, hashfunc=hashlib.sha256)
        
        r, s = signature[:32], signature[32:]
        v = self._get_v_value(r, s, message_hash)
        
        # 65 bytes: r + s + v
        full_signature = signature + bytes([v])
        return full_signature
    
    def sign_hash(self, data_hash):
        if not self.initialized:
            raise RuntimeError("Hardware identity not initialized")
        
        if isinstance(data_hash, str):
            # Assume hex string
            data_hash = bytes.fromhex(data_hash.replace('0x', ''))
        
        message_hash = self._get_ethereum_message_hash(data_hash)
        signature = self.private_key.sign_digest_deterministic(message_hash, hashfunc=hashlib.sha256)
        
        r, s = signature[:32], signature[32:]
        v = self._get_v_value(r, s, message_hash)
        
        full_signature = signature + bytes([v])
        
        return {
            'signature': "0x" + full_signature.hex(),
            'address': self.address,
            'algorithm': 'ECDSA_SECP256k1_ETH',
            'salt_path': SALT_PATH if os.path.exists(SALT_PATH) else str(SALT_BACKUP_PATH)
        }
    
    def verify_signature(self, data, signature):
        if not self.initialized:
            raise RuntimeError("Hardware identity not initialized")
        
        if isinstance(data, str):
            data = data.encode('utf-8')
        
        if isinstance(signature, str):
            signature = bytes.fromhex(signature.replace('0x', ''))
        
        # Strip 'v' if it has 65 bytes, ecdsa only needs 64
        if len(signature) == 65:
            signature = signature[:64]
            
        try:
            message_hash = self._get_ethereum_message_hash(data)
            self.public_key.verify_digest(signature, message_hash)
            return True
        except Exception:
            return False
    
    def get_public_key_hex(self):
        if not self.initialized:
            return None
        return self.public_key.to_string().hex()
    
    def get_address(self):
        return self.address
    
    def get_camera_id(self):
        return self.camera_id
    
    def get_hardware_info(self):
        return {
            'address': self.address,
            'camera_id': self.camera_id,
            'public_key_hex': self.get_public_key_hex(),
            'salt_path': SALT_PATH if os.path.exists(SALT_PATH) else str(SALT_BACKUP_PATH),
            'initialized': self.initialized
        }

_hardware_identity = None

def get_hardware_identity(camera_id=None):
    global _hardware_identity
    if _hardware_identity is None:
        _hardware_identity = HardwareIdentity(camera_id=camera_id)
    elif camera_id is not None and _hardware_identity.camera_id != camera_id:
        _hardware_identity = HardwareIdentity(camera_id=camera_id)
    return _hardware_identity

if __name__ == '__main__':
    print("=" * 60)
    print("Hardware Identity Test")
    print("=" * 60)
    
    try:
        hw_id = HardwareIdentity()
        
        print(f"\n✓ Hardware Identity Initialized")
        print(f"  Address: {hw_id.get_address()}")
        print(f"  Camera ID: {hw_id.get_camera_id() or 'Not provided'}")
        print(f"  Public Key: {hw_id.get_public_key_hex()[:32]}...")
        
        test_data = b"Hello, LensMint!"
        signature = hw_id.sign_data(test_data)
        print(f"\n✓ Test Signature Generated")
        print(f"  Data: {test_data}")
        print(f"  Signature: {signature.hex()[:32]}...")
        
        is_valid = hw_id.verify_signature(test_data, signature)
        print(f"\n✓ Signature Verification: {'PASSED' if is_valid else 'FAILED'}")
        
        test_hash = hashlib.sha256(b"test image data").digest()
        sig_info = hw_id.sign_hash(test_hash)
        print(f"\n✓ Hash Signature Generated")
        print(f"  Algorithm: {sig_info['algorithm']}")
        print(f"  Address: {sig_info['address']}")
        
    except Exception as e:
        print(f"\n✗ Error: {e}")
        import traceback
        traceback.print_exc()

