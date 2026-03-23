const Database = require('better-sqlite3');
const path = require('path');
const fs = require('fs');
const os = require('os');

class ClaimDBService {
  constructor() {
    this.db = null;
    this.getDatabase = () => this.db;
    const defaultPath = process.env.DATABASE_PATH;
    if (defaultPath) {
      this.dbPath = path.resolve(defaultPath);
    } else {
      const dbDir = path.resolve(__dirname, 'database');
      this.dbPath = path.join(dbDir, 'claims.db');
    }
  }

  initialize() {
    try {
      // Ensure database directory exists
      const dbDir = path.dirname(this.dbPath);
      
      if (!fs.existsSync(dbDir)) {
        try {
          fs.mkdirSync(dbDir, { recursive: true, mode: 0o755 });
          console.log(`📁 Created database directory: ${dbDir}`);
        } catch (mkdirError) {
          if (mkdirError.code === 'EACCES') {
            const fallbackDir = path.join(os.homedir(), '.lensmint', 'claim-server', 'database');
            this.dbPath = path.join(fallbackDir, 'claims.db');
            const fallbackParent = path.dirname(this.dbPath);
            if (!fs.existsSync(fallbackParent)) {
              fs.mkdirSync(fallbackParent, { recursive: true, mode: 0o755 });
            }
            console.log(`⚠️  Permission denied, using fallback path: ${this.dbPath}`);
          } else {
            throw mkdirError;
          }
        }
      }

      this.db = new Database(this.dbPath);
      this.db.pragma('journal_mode = WAL');
      this.db.pragma('foreign_keys = ON');

      this.createTables();

      console.log(`✅ Claim database initialized automatically: ${this.dbPath}`);
    } catch (error) {
      console.error('❌ Failed to initialize database:', error);
      throw error;
    }
  }

  createTables() {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS claims (
        claim_id TEXT PRIMARY KEY,
        image_id INTEGER,
        cid TEXT NOT NULL,
        metadata_cid TEXT,
        device_id TEXT,
        camera_id TEXT,
        image_hash TEXT,
        signature TEXT,
        device_address TEXT,
        status TEXT DEFAULT 'pending',
        recipient_address TEXT,
        tx_hash TEXT,
        token_id INTEGER,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        claimed_at DATETIME,
        completed_at DATETIME,
        expires_at INTEGER DEFAULT NULL
      )
    `);

    const columnsToAdd = [
      { name: 'device_id', type: 'TEXT' },
      { name: 'camera_id', type: 'TEXT' },
      { name: 'image_hash', type: 'TEXT' },
      { name: 'signature', type: 'TEXT' },
      { name: 'device_address', type: 'TEXT' },
      { name: 'expires_at', type: 'INTEGER' }
    ];

    columnsToAdd.forEach(col => {
      try {
        this.db.exec(`ALTER TABLE claims ADD COLUMN ${col.name} ${col.type}`);
      } catch (e) {
      }
    });

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS edition_requests (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        claim_id TEXT NOT NULL,
        wallet_address TEXT NOT NULL,
        status TEXT DEFAULT 'pending',
        tx_hash TEXT,
        token_id TEXT,
        error_message TEXT,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        minted_at DATETIME,
        FOREIGN KEY (claim_id) REFERENCES claims(claim_id)
      )
    `);

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS proofs (
        claim_id TEXT PRIMARY KEY,
        token_id INTEGER,
        verification_status TEXT DEFAULT 'pending',
        proof_tx_hash TEXT,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        submitted_at DATETIME,
        verified_at DATETIME,
        FOREIGN KEY (claim_id) REFERENCES claims(claim_id)
      )
    `);

    this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_claims_status ON claims(status);
      CREATE INDEX IF NOT EXISTS idx_claims_cid ON claims(cid);
      CREATE INDEX IF NOT EXISTS idx_edition_requests_claim ON edition_requests(claim_id);
      CREATE INDEX IF NOT EXISTS idx_edition_requests_status ON edition_requests(status);
      CREATE INDEX IF NOT EXISTS idx_proofs_status ON proofs(verification_status);
    `);
  }

  createClaim(claim_id, image_id, cid, metadata_cid = null, device_id = null, camera_id = null, image_hash = null, signature = null, device_address = null) {
    try {
      const stmt = this.db.prepare(`
        INSERT INTO claims (claim_id, image_id, cid, metadata_cid, device_id, camera_id, image_hash, signature, device_address, status)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending')
      `);

      stmt.run(claim_id, image_id, cid, metadata_cid, device_id, camera_id, image_hash, signature, device_address);
      return this.getClaim(claim_id);
    } catch (error) {
      if (error.code === 'SQLITE_CONSTRAINT_UNIQUE') {
        return null;
      }
      throw error;
    }
  }

  getClaim(claim_id) {
    const stmt = this.db.prepare('SELECT * FROM claims WHERE claim_id = ?');
    return stmt.get(claim_id) || null;
  }

  updateClaim(claim_id, recipient_address) {
    const stmt = this.db.prepare(`
      UPDATE claims
      SET status = 'claimed',
          recipient_address = ?,
          claimed_at = CURRENT_TIMESTAMP
      WHERE claim_id = ?
    `);

    const result = stmt.run(recipient_address, claim_id);
    
    if (result.changes === 0) {
      return null;
    }

    return this.getClaim(claim_id);
  }

  updateClaimStatus(claim_id, status, token_id = null, tx_hash = null) {
    const updateFields = ['status = ?'];
    const values = [status];

    if (token_id !== null) {
      updateFields.push('token_id = ?');
      values.push(token_id);
    }

    if (tx_hash !== null) {
      updateFields.push('tx_hash = ?');
      values.push(tx_hash);
    }

    values.push(claim_id);

    const stmt = this.db.prepare(`
      UPDATE claims
      SET ${updateFields.join(', ')}
      WHERE claim_id = ?
    `);

    const result = stmt.run(...values);
    
    if (result.changes === 0) {
      return null;
    }

    return this.getClaim(claim_id);
  }

  createEditionRequest(claim_id, wallet_address) {
    try {
      const stmt = this.db.prepare(`
        INSERT INTO edition_requests (claim_id, wallet_address, status)
        VALUES (?, ?, 'pending')
      `);

      const result = stmt.run(claim_id, wallet_address);
      return this.getEditionRequest(result.lastInsertRowid);
    } catch (error) {
      console.error('Error creating edition request:', error);
      throw error;
    }
  }

  getEditionRequest(id) {
    const stmt = this.db.prepare('SELECT * FROM edition_requests WHERE id = ?');
    return stmt.get(id) || null;
  }

  getPendingEditionRequests(limit = 50) {
    const stmt = this.db.prepare(`
      SELECT e.*, c.token_id as original_token_id
      FROM edition_requests e
      JOIN claims c ON e.claim_id = c.claim_id
      WHERE e.status = 'pending' AND c.status = 'open' AND c.token_id IS NOT NULL
      ORDER BY e.created_at ASC
      LIMIT ?
    `);
    return stmt.all(limit);
  }

  updateEditionRequest(id, updates) {
    const updateFields = [];
    const values = [];

    if (updates.status) {
      updateFields.push('status = ?');
      values.push(updates.status);
    }

    if (updates.tx_hash) {
      updateFields.push('tx_hash = ?');
      values.push(updates.tx_hash);
    }

    if (updates.token_id) {
      updateFields.push('token_id = ?');
      values.push(updates.token_id);
    }

    if (updates.error_message) {
      updateFields.push('error_message = ?');
      values.push(updates.error_message);
    }

    if (updates.status === 'completed' || updates.status === 'minted') {
      updateFields.push('minted_at = CURRENT_TIMESTAMP');
    }

    if (updateFields.length === 0) {
      return this.getEditionRequest(id);
    }

    values.push(id);

    const stmt = this.db.prepare(`
      UPDATE edition_requests
      SET ${updateFields.join(', ')}
      WHERE id = ?
    `);

    stmt.run(...values);
    return this.getEditionRequest(id);
  }

  completeClaim(claim_id, tx_hash, token_id) {
    const stmt = this.db.prepare(`
      UPDATE claims
      SET status = 'completed',
          tx_hash = ?,
          token_id = ?,
          completed_at = CURRENT_TIMESTAMP
      WHERE claim_id = ?
    `);

    const result = stmt.run(tx_hash, token_id || null, claim_id);
    
    if (result.changes === 0) {
      return null;
    }

    return this.getClaim(claim_id);
  }

  getAllClaims(limit = 100) {
    const stmt = this.db.prepare(`
      SELECT * FROM claims
      ORDER BY created_at DESC
      LIMIT ?
    `);
    return stmt.all(limit);
  }

  getClaimsByStatus(status) {
    const stmt = this.db.prepare('SELECT * FROM claims WHERE status = ? ORDER BY created_at DESC');
    return stmt.all(status);
  }

  setClaimExpiry(claim_id, expires_at) {
  const stmt = this.db.prepare(
    'UPDATE claims SET expires_at = ? WHERE claim_id = ?'
  );
  stmt.run(expires_at, claim_id);
}

cleanupExpiredClaims() {
  const stmt = this.db.prepare(`
    UPDATE claims
    SET status = 'expired'
    WHERE expires_at IS NOT NULL
      AND expires_at < ?
      AND status = 'pending'
  `);

  const result = stmt.run(Date.now());

  if (result.changes > 0) {
    console.log(`[CLEANUP] Marked ${result.changes} expired claims`);
  }

  return result.changes;
}

  createOrUpdateProof(claimId, tokenId, verificationStatus, proofTxHash = null) {
    const existing = this.getProof(claimId);
    
    if (existing) {
      const updateFields = [];
      const values = [];
      
      if (tokenId !== null && tokenId !== undefined) {
        updateFields.push('token_id = ?');
        values.push(tokenId);
      }
      
      if (verificationStatus) {
        updateFields.push('verification_status = ?');
        values.push(verificationStatus);
      }
      
      if (proofTxHash) {
        updateFields.push('proof_tx_hash = ?');
        values.push(proofTxHash);
        updateFields.push('submitted_at = CURRENT_TIMESTAMP');
      }
      
      if (verificationStatus === 'verified') {
        updateFields.push('verified_at = CURRENT_TIMESTAMP');
      }
      
      if (updateFields.length > 0) {
        values.push(claimId);
        const stmt = this.db.prepare(`
          UPDATE proofs
          SET ${updateFields.join(', ')}
          WHERE claim_id = ?
        `);
        stmt.run(...values);
      }
    } else {
      const stmt = this.db.prepare(`
        INSERT INTO proofs (claim_id, token_id, verification_status, proof_tx_hash, created_at)
        VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)
      `);
      stmt.run(claimId, tokenId, verificationStatus, proofTxHash);
    }
    
    return this.getProof(claimId);
  }

  getProof(claimId) {
    const stmt = this.db.prepare('SELECT * FROM proofs WHERE claim_id = ?');
    return stmt.get(claimId);
  }

  close() {
    if (this.db) {
      this.db.close();
    }
  }
}

const dbService = new ClaimDBService();

module.exports = dbService;

