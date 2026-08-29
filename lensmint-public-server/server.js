const express = require('express');
const cors = require('cors');
const path = require('path');
const { v4: uuidv4 } = require('uuid');
require('dotenv').config();

const dbService = require('./dbService');

const app = express();
const PORT = process.env.PORT || 5001;

// Middleware setup
app.use(express.json());

const CLAIM_SERVER_URL = process.env.CLAIM_SERVER_URL || 'https://lensmint.onrender.com';

let FRONTEND_URL = process.env.FRONTEND_URL || CLAIM_SERVER_URL;
if (FRONTEND_URL.includes('localhost') || FRONTEND_URL.includes('127.0.0.1')) {
  console.log(`⚠️  Ignoring localhost FRONTEND_URL (${FRONTEND_URL}), using Render URL instead`);
  FRONTEND_URL = CLAIM_SERVER_URL;
}

app.use(cors({
  origin: process.env.CORS_ORIGIN || '*',
  methods: ['GET', 'POST', 'PUT', 'DELETE', 'OPTIONS'],
  allowedHeaders: ['Content-Type', 'Authorization', 'X-Requested-With'],
  credentials: false
}));

app.use((req, res, next) => {
  res.header('Access-Control-Allow-Origin', '*');
  res.header('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS');
  res.header('Access-Control-Allow-Headers', 'Content-Type, Authorization, X-Requested-With');
  if (req.method === 'OPTIONS') {
    res.sendStatus(200);
  } else {
    next();
  }
});

app.use(express.json());
app.use(express.urlencoded({ extended: true }));
app.use(express.static('public'));

let servicesInitialized = false;

async function initializeServices() {
  try {
    console.log('🔄 Initializing claim server services...');

    dbService.initialize();
    console.log('✅ Database initialized');

    servicesInitialized = true;
    console.log('✅ All services initialized');
  } catch (error) {
    console.error('❌ Error initializing services:', error);
    servicesInitialized = false;
  }
}

initializeServices();

// Middleware to protect internal API routes
const requireAuth = (req, res, next) => {
  const apiSecret = process.env.API_SECRET;

  if (!apiSecret) {
    console.error('⚠️ API_SECRET environment variable is missing.');
    return res.status(500).json({ success: false, error: 'Server misconfiguration: missing API_SECRET' });
  }

  const authHeader = req.headers['authorization'] || req.headers['Authorization'];
  if (!authHeader || !authHeader.startsWith('Bearer ')) {
    return res.status(401).json({ success: false, error: 'Authentication required' });
  }

  const token = authHeader.split(' ')[1];
  if (token !== apiSecret) {
    return res.status(401).json({ success: false, error: 'Invalid authentication token' });
  }

  next();
};

app.post('/create-claim', requireAuth, (req, res) => {
  try {
    const {
      claim_id,
      cid,
      metadata_cid,
      device_id,
      camera_id,
      image_hash,
      signature,
      device_address
    } = req.body;

    if (!claim_id || !cid) {
      return res.status(400).json({
        success: false,
        error: 'claim_id and cid are required'
      });
    }

    const claim = dbService.createClaim(
      claim_id,
      null,
      cid,
      metadata_cid || null,
      device_id || null,
      camera_id || null,
      image_hash || null,
      signature || null,
      device_address || null
    );

    if (!claim) {
      return res.status(400).json({
        success: false,
        error: 'Claim ID already exists'
      });
    }

    const claimUrl = `${FRONTEND_URL}/claim/${claim_id}`;

    console.log(`✅ Claim created: ${claim_id} for CID: ${cid}`);

    res.json({
      success: true,
      claim_id: claim.claim_id,
      claim_url: claimUrl,
      status: claim.status
    });

  } catch (error) {
    console.error('❌ Error creating claim:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.options('/api/metadata/:claim_id', (req, res) => {
  res.header('Access-Control-Allow-Origin', '*');
  res.header('Access-Control-Allow-Methods', 'GET, OPTIONS');
  res.header('Access-Control-Allow-Headers', 'Content-Type');
  res.sendStatus(200);
});

app.get('/api/metadata/:claim_id', (req, res) => {
  try {
    const { claim_id } = req.params;

    const claim = dbService.getClaim(claim_id);

    if (!claim) {
      return res.status(404).json({
        success: false,
        error: 'Claim not found'
      });
    }

    const metadata = {
      name: `LensMint Photo ${claim.token_id ? `#${claim.token_id}` : ''}`,
      description: 'Captured by LensMint Camera',
      image: `ipfs://${claim.cid}`,
      external_url: `${FRONTEND_URL}/claim/${claim_id}`,
      attributes: [
        ...(claim.device_id ? [{
          trait_type: 'Device ID',
          value: claim.device_id
        }] : []),
        ...(claim.camera_id ? [{
          trait_type: 'Camera ID',
          value: claim.camera_id
        }] : []),
        ...(claim.device_address ? [{
          trait_type: 'Device Address',
          value: claim.device_address
        }] : []),
        ...(claim.image_hash ? [{
          trait_type: 'Image Hash',
          value: claim.image_hash
        }] : []),
        ...(claim.signature ? [{
          trait_type: 'Hardware Signature',
          value: claim.signature
        }] : []),
        ...(claim.token_id ? [{
          trait_type: 'Token ID',
          value: claim.token_id.toString()
        }] : []),
        {
          trait_type: 'Status',
          value: claim.status
        }
      ],
      properties: {
        ...(claim.device_id && { deviceId: claim.device_id }),
        ...(claim.camera_id && { cameraId: claim.camera_id }),
        ...(claim.device_address && { deviceAddress: claim.device_address }),
        ...(claim.image_hash && { imageHash: claim.image_hash }),
        ...(claim.signature && { signature: claim.signature }),
        ...(claim.cid && { filecoinCid: claim.cid }),
        ...(claim.metadata_cid && { metadataCid: claim.metadata_cid }),
        ...(claim.token_id && { tokenId: claim.token_id }),
        ...(claim.tx_hash && { txHash: claim.tx_hash })
      }
    };

    res.header('Content-Type', 'application/json');
    res.header('Access-Control-Allow-Origin', '*');
    res.header('Access-Control-Allow-Methods', 'GET, OPTIONS');
    res.header('Access-Control-Allow-Headers', 'Content-Type');

    res.json(metadata);

  } catch (error) {
    console.error('❌ Error getting metadata:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.post('/update-proof-status', requireAuth, (req, res) => {
  try {
    const {
      claim_id,
      token_id,
      verification_status,
      proof_tx_hash
    } = req.body;

    if (!claim_id) {
      return res.status(400).json({
        success: false,
        error: 'claim_id is required'
      });
    }

    dbService.createOrUpdateProof(
      claim_id,
      token_id || null,
      verification_status || 'pending',
      proof_tx_hash || null
    );

    res.json({
      success: true,
      message: 'Proof status updated'
    });
  } catch (error) {
    console.error('❌ Error updating proof status:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.get('/verify/:claim_id', async (req, res) => {
  try {
    const { claim_id } = req.params;

    const claim = dbService.getClaim(claim_id);

    if (!claim) {
      return res.status(404).send(`
        <!DOCTYPE html>
        <html>
        <head>
          <title>Claim Not Found</title>
          <meta name="viewport" content="width=device-width, initial-scale=1.0">
          <style>
            body { font-family: Arial, sans-serif; text-align: center; padding: 50px; background: #f5f5f5; }
            .error { color: #d32f2f; }
          </style>
        </head>
        <body>
          <h1 class="error">Claim Not Found</h1>
          <p>The claim ID "${claim_id}" does not exist.</p>
        </body>
        </html>
      `);
    }

    const metadata = {
      name: `LensMint Photo ${claim.token_id ? `#${claim.token_id}` : ''}`,
      description: 'Captured by LensMint Camera',
      image: `ipfs://${claim.cid}`,
      external_url: `${FRONTEND_URL}/claim/${claim_id}`,
      attributes: [
        ...(claim.device_id ? [{ trait_type: 'Device ID', value: claim.device_id }] : []),
        ...(claim.camera_id ? [{ trait_type: 'Camera ID', value: claim.camera_id }] : []),
        ...(claim.device_address ? [{ trait_type: 'Device Address', value: claim.device_address }] : []),
        ...(claim.image_hash ? [{ trait_type: 'Image Hash', value: claim.image_hash }] : []),
        ...(claim.signature ? [{ trait_type: 'Hardware Signature', value: claim.signature }] : []),
        ...(claim.token_id ? [{ trait_type: 'Token ID', value: claim.token_id.toString() }] : []),
        { trait_type: 'Status', value: claim.status }
      ],
      properties: {
        ...(claim.device_id && { deviceId: claim.device_id }),
        ...(claim.camera_id && { cameraId: claim.camera_id }),
        ...(claim.device_address && { deviceAddress: claim.device_address }),
        ...(claim.image_hash && { imageHash: claim.image_hash }),
        ...(claim.signature && { signature: claim.signature }),
        ...(claim.cid && { filecoinCid: claim.cid }),
        ...(claim.metadata_cid && { metadataCid: claim.metadata_cid }),
        ...(claim.token_id && { tokenId: claim.token_id }),
        ...(claim.tx_hash && { txHash: claim.tx_hash })
      }
    };

    let proofData = dbService.getProof(claim_id);

    if (proofData) {
      console.log(`[VERIFY] Found proof in local DB: status=${proofData.verification_status}, tx=${proofData.proof_tx_hash || 'none'}`);
    } else {
      console.log(`[VERIFY] No proof found in local DB for claim: ${claim_id}`);
    }

    res.send(`
      <!DOCTYPE html>
      <html>
      <head>
        <title>Verify Proof - LensMint</title>
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <style>
          * { margin: 0; padding: 0; box-sizing: border-box; }
          body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            padding: 20px;
          }
          .container {
            max-width: 1200px;
            margin: 0 auto;
            background: white;
            border-radius: 20px;
            padding: 40px;
            box-shadow: 0 20px 60px rgba(0,0,0,0.3);
          }
          h1 { color: #333; margin-bottom: 10px; }
          .subtitle { color: #666; margin-bottom: 30px; }
          .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 30px; margin-top: 30px; }
          @media (max-width: 768px) { .grid { grid-template-columns: 1fr; } }
          .card { background: #f8f9fa; border-radius: 12px; padding: 20px; }
          .card h2 { color: #333; margin-bottom: 15px; font-size: 1.2em; }
          .nft-image { width: 100%; border-radius: 12px; margin-bottom: 20px; }
          .info-item { margin-bottom: 15px; }
          .info-label { font-weight: 600; color: #666; font-size: 0.9em; margin-bottom: 5px; }
          .info-value { color: #333; word-break: break-all; font-family: monospace; font-size: 0.9em; }
          .proof-section { margin-top: 20px; }
          .proof-data { background: #1e1e1e; color: #d4d4d4; padding: 15px; border-radius: 8px; font-family: monospace; font-size: 0.85em; overflow-x: auto; max-height: 300px; overflow-y: auto; }
          .status-badge { display: inline-block; padding: 5px 15px; border-radius: 20px; font-size: 0.85em; font-weight: 600; }
          .status-pending { background: #fff3cd; color: #856404; }
          .status-verified { background: #d4edda; color: #155724; }
          .status-failed { background: #f8d7da; color: #721c24; }
          .btn { display: inline-block; padding: 12px 24px; background: #667eea; color: white; text-decoration: none; border-radius: 8px; margin-top: 15px; font-weight: 600; }
          .btn:hover { background: #5568d3; }
        </style>
      </head>
      <body>
        <div class="container">
          <h1>🔐 ZK Proof Verification</h1>
          <p class="subtitle">Claim ID: ${claim_id}</p>
          
          <div class="grid">
            <div class="card">
              <h2>📸 NFT Information</h2>
              ${claim.cid ? `<img src="https://ipfs.io/ipfs/${claim.cid}" alt="NFT" class="nft-image" onerror="this.style.display='none'">` : ''}
              <div class="info-item">
                <div class="info-label">Name</div>
                <div class="info-value">${metadata.name}</div>
              </div>
              ${claim.token_id ? `
              <div class="info-item">
                <div class="info-label">Token ID</div>
                <div class="info-value">#${claim.token_id}</div>
              </div>
              ` : ''}
              ${claim.device_id ? `
              <div class="info-item">
                <div class="info-label">Device ID</div>
                <div class="info-value">${claim.device_id}</div>
              </div>
              ` : ''}
              ${claim.device_address ? `
              <div class="info-item">
                <div class="info-label">Device Address</div>
                <div class="info-value">${claim.device_address}</div>
              </div>
              ` : ''}
              ${claim.image_hash ? `
              <div class="info-item">
                <div class="info-label">Image Hash</div>
                <div class="info-value">${claim.image_hash}</div>
              </div>
              ` : ''}
              ${claim.signature ? `
              <div class="info-item">
                <div class="info-label">Hardware Signature</div>
                <div class="info-value">${claim.signature.substring(0, 20)}...</div>
              </div>
              ` : ''}
              ${claim.tx_hash ? `
              <div class="info-item">
                <div class="info-label">Transaction Hash</div>
                <div class="info-value"><a href="https://sepolia.etherscan.io/tx/${claim.tx_hash}" target="_blank">${claim.tx_hash}</a></div>
              </div>
              ` : ''}
            </div>
            
            <div class="card">
              <h2>🔐 Proof Status</h2>
              ${proofData ? `
                <div class="info-item">
                  <div class="info-label">Status</div>
                  <div class="info-value">
                    <span class="status-badge ${proofData.verification_status === 'verified' ? 'status-verified' : proofData.verification_status === 'failed' ? 'status-failed' : 'status-pending'}">
                      ${proofData.verification_status || 'pending'}
                    </span>
                  </div>
                </div>
                ${proofData.proof_tx_hash ? `
                <div class="info-item">
                  <div class="info-label">Proof TX Hash</div>
                  <div class="info-value"><a href="https://sepolia.etherscan.io/tx/${proofData.proof_tx_hash}" target="_blank">${proofData.proof_tx_hash}</a></div>
                </div>
                ` : ''}
                <div class="proof-section">
                  <div class="info-label">ZK Proof (Raw)</div>
                  <div class="proof-data">${JSON.stringify(proofData, null, 2)}</div>
                </div>
              ` : `
                <div class="info-item">
                  <div class="info-label">Status</div>
                  <div class="info-value">
                    <span class="status-badge status-pending">Proof not generated</span>
                  </div>
                </div>
                <p style="color: #666; margin-top: 15px;">Proof generation is pending. Check back later.</p>
              `}
            </div>
          </div>
          
          <div style="margin-top: 30px; text-align: center;">
            <a href="/claim/${claim_id}" class="btn">View Claim Page</a>
            <a href="/api/metadata/${claim_id}" class="btn" style="margin-left: 10px;">View Metadata API</a>
          </div>
        </div>
      </body>
      </html>
    `);
  } catch (error) {
    console.error('❌ Error serving verification page:', error);
    res.status(500).send(`
      <!DOCTYPE html>
      <html>
      <head><title>Error</title></head>
      <body>
        <h1>Error</h1>
        <p>${error.message}</p>
      </body>
      </html>
    `);
  }
});

app.get('/check-claim', (req, res) => {
  try {
    const { claim_id } = req.query;

    if (!claim_id) {
      return res.status(400).json({
        success: false,
        error: 'claim_id is required'
      });
    }

    const claim = dbService.getClaim(claim_id);

    if (!claim) {
      return res.status(404).json({
        success: false,
        error: 'Claim not found'
      });
    }

    res.json({
      success: true,
      claim_id: claim.claim_id,
      status: claim.status,
      recipient_address: claim.recipient_address || null,
      token_id: claim.token_id || null,
      cid: claim.cid,
      created_at: claim.created_at
    });

  } catch (error) {
    console.error('❌ Error checking claim:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.get('/claim/:claim_id', (req, res) => {
  const { claim_id } = req.params;

  const claim = dbService.getClaim(claim_id);

  if (!claim) {
    return res.status(404).send(`
      <!DOCTYPE html>
      <html>
      <head>
        <title>Claim Not Found</title>
        <style>
          body { font-family: Arial, sans-serif; text-align: center; padding: 50px; }
          .error { color: #d32f2f; }
        </style>
      </head>
      <body>
        <h1 class="error">Claim Not Found</h1>
        <p>The claim ID "${claim_id}" does not exist.</p>
      </body>
      </html>
    `);
  }

  res.send(`
    <!DOCTYPE html>
    <html>
    <head>
      <title>Claim Your NFT - LensMint</title>
      <meta name="viewport" content="width=device-width, initial-scale=1.0">
      <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
          font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
          background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
          min-height: 100vh;
          display: flex;
          align-items: center;
          justify-content: center;
          padding: 20px;
        }
        .container {
          background: white;
          border-radius: 20px;
          padding: 40px;
          max-width: 500px;
          width: 100%;
          box-shadow: 0 20px 60px rgba(0,0,0,0.3);
        }
        h1 {
          color: #333;
          margin-bottom: 10px;
          font-size: 28px;
        }
        .subtitle {
          color: #666;
          margin-bottom: 30px;
          font-size: 14px;
        }
        .status {
          padding: 12px;
          border-radius: 8px;
          margin-bottom: 20px;
          font-size: 14px;
        }
        .status.pending {
          background: #fff3cd;
          color: #856404;
          border: 1px solid #ffeaa7;
        }
        .status.claimed {
          background: #d4edda;
          color: #155724;
          border: 1px solid #c3e6cb;
        }
        .status.completed {
          background: #d1ecf1;
          color: #0c5460;
          border: 1px solid #bee5eb;
        }
        .form-group {
          margin-bottom: 20px;
        }
        label {
          display: block;
          margin-bottom: 8px;
          color: #333;
          font-weight: 500;
          font-size: 14px;
        }
        input[type="text"] {
          width: 100%;
          padding: 12px;
          border: 2px solid #e0e0e0;
          border-radius: 8px;
          font-size: 16px;
          transition: border-color 0.3s;
        }
        input[type="text"]:focus {
          outline: none;
          border-color: #667eea;
        }
        button {
          width: 100%;
          padding: 14px;
          background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
          color: white;
          border: none;
          border-radius: 8px;
          font-size: 16px;
          font-weight: 600;
          cursor: pointer;
          transition: transform 0.2s, box-shadow 0.2s;
        }
        button:hover {
          transform: translateY(-2px);
          box-shadow: 0 10px 20px rgba(102, 126, 234, 0.4);
        }
        button:disabled {
          background: #ccc;
          cursor: not-allowed;
          transform: none;
        }
        .message {
          margin-top: 20px;
          padding: 12px;
          border-radius: 8px;
          font-size: 14px;
        }
        .message.success {
          background: #d4edda;
          color: #155724;
          border: 1px solid #c3e6cb;
        }
        .message.error {
          background: #f8d7da;
          color: #721c24;
          border: 1px solid #f5c6cb;
        }
        .claim-id {
          font-size: 12px;
          color: #999;
          margin-top: 20px;
          word-break: break-all;
        }
        .proof-link {
          display: inline-block;
          padding: 10px 20px;
          background: #667eea;
          color: white;
          text-decoration: none;
          border-radius: 8px;
          font-size: 14px;
          font-weight: 500;
          transition: background 0.3s, transform 0.2s;
        }
        .proof-link:hover {
          background: #5568d3;
          transform: translateY(-2px);
          box-shadow: 0 5px 15px rgba(102, 126, 234, 0.4);
        }
        /* NFT Card Styles */
        .nft-card-container {
          margin: 20px 0;
          perspective: 1000px;
        }
        .nft-card {
          background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
          border-radius: 20px;
          padding: 20px;
          box-shadow: 0 20px 60px rgba(0,0,0,0.3);
          transform-style: preserve-3d;
          transition: transform 0.3s ease;
          position: relative;
          overflow: hidden;
        }
        .nft-card::before {
          content: '';
          position: absolute;
          top: -50%;
          left: -50%;
          width: 200%;
          height: 200%;
          background: radial-gradient(circle, rgba(255,255,255,0.1) 0%, transparent 70%);
          animation: shimmer 3s infinite;
        }
        @keyframes shimmer {
          0% { transform: rotate(0deg); }
          100% { transform: rotate(360deg); }
        }
        .nft-image {
          width: 100%;
          border-radius: 15px;
          margin-bottom: 15px;
          box-shadow: 0 10px 30px rgba(0,0,0,0.3);
          animation: float 3s ease-in-out infinite;
        }
        @keyframes float {
          0%, 100% { transform: translateY(0px); }
          50% { transform: translateY(-10px); }
        }
        .nft-info {
          color: white;
        }
        .nft-name {
          font-size: 24px;
          font-weight: bold;
          margin-bottom: 10px;
          text-shadow: 2px 2px 4px rgba(0,0,0,0.3);
        }
        .nft-description {
          font-size: 14px;
          opacity: 0.9;
          margin-bottom: 15px;
          line-height: 1.5;
        }
        .nft-attributes {
          display: flex;
          flex-wrap: wrap;
          gap: 10px;
          margin-top: 15px;
        }
        .nft-attribute {
          background: rgba(255,255,255,0.2);
          padding: 8px 12px;
          border-radius: 10px;
          font-size: 12px;
          backdrop-filter: blur(10px);
          animation: pulse 2s ease-in-out infinite;
        }
        @keyframes pulse {
          0%, 100% { opacity: 0.8; }
          50% { opacity: 1; }
        }
        .nft-token-id {
          position: absolute;
          top: 15px;
          right: 15px;
          background: rgba(0,0,0,0.3);
          padding: 5px 10px;
          border-radius: 10px;
          font-size: 12px;
          color: white;
          backdrop-filter: blur(10px);
        }
      </style>
    </head>
    <body>
      <div class="container">
        <h1>🎨 Claim Your NFT</h1>
        <p class="subtitle">Enter your wallet address to receive your LensMint photo NFT</p>
        
        ${claim.token_id ? `
        <div class="nft-card-container">
          <div class="nft-card" id="nftCard">
            <div class="nft-token-id">Token #${claim.token_id}</div>
            <img src="https://ipfs.io/ipfs/${claim.cid}" alt="LensMint Photo #${claim.token_id}" class="nft-image" onerror="this.style.display='none'">
            <div class="nft-info">
              <div class="nft-name">LensMint Photo #${claim.token_id}</div>
              <div class="nft-description">Captured by LensMint Camera</div>
              <div class="nft-attributes">
                <div class="nft-attribute"><strong>Token ID:</strong> ${claim.token_id}</div>
                ${claim.status === 'open' ? '<div class="nft-attribute"><strong>Status:</strong> Open for Editions</div>' : ''}
              </div>
            </div>
          </div>
        </div>
        ` : ''}
        
        <div id="status" class="status ${claim.status}">
          ${claim.status === 'open' ? `Status: Open - Ready for editions! Original Token ID: ${claim.token_id || 'N/A'}` :
      claim.status === 'claimed' ? `Status: Claimed - Address: ${claim.recipient_address}` :
        claim.status === 'completed' ? 'Status: Completed - NFT Minted! 🎉' :
          'Status: Pending - Waiting for original NFT to be minted...'}
        </div>

        <form id="claimForm" ${claim.status === 'open' ? '' : 'style="display:none;"'}>
          <div class="form-group">
            <label for="walletAddress">Wallet Address (0x...)</label>
            <input
              type="text"
              id="walletAddress"
              name="walletAddress"
              placeholder="0x..."
              pattern="^0x[a-fA-F0-9]{40}$"
              required
            />
          </div>
          <button type="submit" id="submitBtn">Submit Wallet Address</button>
        </form>

        <div id="message"></div>
        <div class="claim-id">Claim ID: ${claim_id}</div>
        
        <div style="margin-top: 20px; text-align: center; padding-top: 20px; border-top: 1px solid #e0e0e0;">
          <a href="/verify/${claim_id}" class="proof-link">
            🔐 Check ZK Proof Verification
          </a>
        </div>
      </div>

      <script>
        const claimId = '${claim_id}';
        const form = document.getElementById('claimForm');
        const submitBtn = document.getElementById('submitBtn');
        const messageDiv = document.getElementById('message');
        const statusDiv = document.getElementById('status');
        const nftCard = document.getElementById('nftCard');

        if (nftCard) {
          nftCard.addEventListener('mousemove', (e) => {
            const rect = nftCard.getBoundingClientRect();
            const x = e.clientX - rect.left;
            const y = e.clientY - rect.top;
            const centerX = rect.width / 2;
            const centerY = rect.height / 2;
            const rotateX = (y - centerY) / 10;
            const rotateY = (centerX - x) / 10;
            nftCard.style.transform = \`perspective(1000px) rotateX(\${rotateX}deg) rotateY(\${rotateY}deg)\`;
          });
          
          nftCard.addEventListener('mouseleave', () => {
            nftCard.style.transform = 'perspective(1000px) rotateX(0) rotateY(0)';
          });
        }

        checkStatus();
        setInterval(checkStatus, 5000);

        function checkStatus() {
          fetch(\`/check-claim?claim_id=\${claimId}\`)
            .then(res => res.json())
            .then(data => {
              if (data.success) {
                updateStatus(data.status, data.recipient_address, data.token_id);
              }
            })
            .catch(err => console.error('Error checking status:', err));
        }

        function updateStatus(status, address, tokenId) {
          statusDiv.className = 'status ' + status;
          
          if (status === 'open') {
            statusDiv.textContent = \`Status: Open - Ready for editions! Original Token ID: \${tokenId || 'N/A'}\`;
            form.style.display = 'block';
          } else if (status === 'claimed') {
            statusDiv.textContent = \`Status: Claimed - Address: \${address}\`;
            form.style.display = 'none';
          } else if (status === 'completed') {
            statusDiv.textContent = 'Status: Completed - NFT Minted! 🎉';
            form.style.display = 'none';
          } else {
            statusDiv.textContent = 'Status: Pending - Waiting for original NFT to be minted...';
            form.style.display = 'none';
          }
        }

        form.addEventListener('submit', async (e) => {
          e.preventDefault();
          
          const walletAddress = document.getElementById('walletAddress').value.trim();
          
          if (!/^0x[a-fA-F0-9]{40}$/.test(walletAddress)) {
            showMessage('Please enter a valid Ethereum address (0x followed by 40 hex characters)', 'error');
            return;
          }

          submitBtn.disabled = true;
          submitBtn.textContent = 'Minting...';

          try {
            const response = await fetch(\`/claim/\${claimId}/submit\`, {
              method: 'POST',
              headers: {
                'Content-Type': 'application/json',
              },
              body: JSON.stringify({ wallet_address: walletAddress })
            });

            const data = await response.json();

            if (data.success) {
              showMessage('✅ Edition request submitted! Your NFT will be minted shortly. You can submit again for another edition.', 'success');
              document.getElementById('walletAddress').value = '';
              submitBtn.disabled = false;
              submitBtn.textContent = 'Mint Another Edition';
              setTimeout(checkStatus, 1000);
            } else {
              showMessage(data.error || 'Failed to submit wallet address', 'error');
              submitBtn.disabled = false;
              submitBtn.textContent = 'Mint Edition';
            }
          } catch (error) {
            showMessage('Error submitting wallet address: ' + error.message, 'error');
            submitBtn.disabled = false;
            submitBtn.textContent = 'Submit Wallet Address';
          }
        });

        function showMessage(text, type) {
          messageDiv.textContent = text;
          messageDiv.className = 'message ' + type;
          messageDiv.style.display = 'block';
        }
      </script>
    </body>
    </html>
  `);
});

app.post('/claim/:claim_id/submit', (req, res) => {
  try {
    const { claim_id } = req.params;
    const { wallet_address } = req.body;

    if (!wallet_address) {
      return res.status(400).json({
        success: false,
        error: 'wallet_address is required'
      });
    }

    if (!/^0x[a-fA-F0-9]{40}$/.test(wallet_address)) {
      return res.status(400).json({
        success: false,
        error: 'Invalid Ethereum address format'
      });
    }

    const claim = dbService.getClaim(claim_id);

    if (!claim) {
      return res.status(404).json({
        success: false,
        error: 'Claim not found'
      });
    }

    if (claim.status !== 'open') {
      return res.status(400).json({
        success: false,
        error: `Claim is ${claim.status}. Editions can only be minted when claim is open.`
      });
    }

    if (!claim.token_id) {
      return res.status(400).json({
        success: false,
        error: 'Original NFT not yet minted. Please wait.'
      });
    }

    const editionRequest = dbService.createEditionRequest(claim_id, wallet_address);

    if (!editionRequest) {
      return res.status(500).json({
        success: false,
        error: 'Failed to create edition request'
      });
    }

    console.log(`✅ Edition request created for claim ${claim_id}: ${wallet_address} (Request ID: ${editionRequest.id})`);

    res.json({
      success: true,
      edition_request_id: editionRequest.id,
      claim_id: claim_id,
      wallet_address: wallet_address,
      message: 'Edition request submitted! Your NFT will be minted shortly.'
    });

  } catch (error) {
    console.error('❌ Error submitting edition request:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.post('/update-claim-status', requireAuth, (req, res) => {
  try {
    const { claim_id, status, token_id, tx_hash } = req.body;

    if (!claim_id || !status) {
      return res.status(400).json({
        success: false,
        error: 'claim_id and status are required'
      });
    }

    const updated = dbService.updateClaimStatus(claim_id, status, token_id, tx_hash);

    if (!updated) {
      return res.status(404).json({
        success: false,
        error: 'Claim not found'
      });
    }

    console.log(`✅ Claim ${claim_id} updated: status=${status}, token_id=${token_id || 'N/A'}`);

    res.json({
      success: true,
      claim_id: updated.claim_id,
      status: updated.status,
      token_id: updated.token_id || null
    });
  } catch (error) {
    console.error('❌ Error updating claim status:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.post('/create-edition-request', requireAuth, (req, res) => {
  try {
    const { claim_id, wallet_address } = req.body;

    if (!claim_id || !wallet_address) {
      return res.status(400).json({
        success: false,
        error: 'claim_id and wallet_address are required'
      });
    }

    if (!/^0x[a-fA-F0-9]{40}$/.test(wallet_address)) {
      return res.status(400).json({
        success: false,
        error: 'Invalid Ethereum address format'
      });
    }

    const claim = dbService.getClaim(claim_id);

    if (!claim) {
      return res.status(404).json({
        success: false,
        error: 'Claim not found'
      });
    }

    if (claim.status !== 'open') {
      return res.status(400).json({
        success: false,
        error: `Claim is ${claim.status}. Editions can only be minted when claim is open.`
      });
    }

    const editionRequest = dbService.createEditionRequest(claim_id, wallet_address);

    res.json({
      success: true,
      edition_request_id: editionRequest.id,
      claim_id: claim_id,
      wallet_address: wallet_address
    });
  } catch (error) {
    console.error('❌ Error creating edition request:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.get('/get-pending-edition-requests', requireAuth, (req, res) => {
  try {
    const limit = parseInt(req.query.limit) || 50;
    const requests = dbService.getPendingEditionRequests(limit);

    res.json({
      success: true,
      edition_requests: requests,
      count: requests.length
    });
  } catch (error) {
    console.error('❌ Error getting pending edition requests:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.post('/update-edition-request', requireAuth, (req, res) => {
  try {
    const { request_id, status, tx_hash, token_id, error_message } = req.body;

    if (!request_id) {
      return res.status(400).json({
        success: false,
        error: 'request_id is required'
      });
    }

    const updates = {};
    if (status) updates.status = status;
    if (tx_hash) updates.tx_hash = tx_hash;
    if (token_id) updates.token_id = token_id;
    if (error_message) updates.error_message = error_message;

    const updated = dbService.updateEditionRequest(request_id, updates);

    if (!updated) {
      return res.status(404).json({
        success: false,
        error: 'Edition request not found'
      });
    }

    res.json({
      success: true,
      edition_request: updated
    });
  } catch (error) {
    console.error('❌ Error updating edition request:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.post('/complete-claim', requireAuth, (req, res) => {
  try {
    const { claim_id, tx_hash, token_id } = req.body;

    if (!claim_id || !tx_hash) {
      return res.status(400).json({
        success: false,
        error: 'claim_id and tx_hash are required'
      });
    }

    const claim = dbService.completeClaim(claim_id, tx_hash, token_id);

    if (!claim) {
      return res.status(404).json({
        success: false,
        error: 'Claim not found'
      });
    }

    console.log(`✅ Claim ${claim_id} completed - Token ID: ${token_id}, TX: ${tx_hash}`);

    res.json({
      success: true,
      claim_id: claim.claim_id,
      status: claim.status,
      tx_hash: claim.tx_hash,
      token_id: claim.token_id
    });

  } catch (error) {
    console.error('❌ Error completing claim:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.get('/health', (req, res) => {
  res.json({ status: 'ok', service: 'LensMint Claim Server' });
});

app.listen(PORT, async () => {
  await initializeServices();

  console.log('═══════════════════════════════════════');
  console.log('🎫 LensMint Claim Server');
  console.log('═══════════════════════════════════════');
  console.log(`🚀 Server running on port ${PORT}`);
  console.log(`🌐 Claim Server URL: ${CLAIM_SERVER_URL}`);
  console.log(`🌐 Frontend URL: ${FRONTEND_URL}`);
  if (FRONTEND_URL.includes('localhost')) {
    console.log(`⚠️  WARNING: Frontend URL is localhost! Claim URLs will be incorrect.`);
    console.log(`   Set FRONTEND_URL=https://lensmint.onrender.com in environment variables.`);
  }
  console.log(`📁 Database: ${process.env.DATABASE_PATH || './database/claims.db'}`);
  console.log(`🌐 Endpoints:`);
  console.log(`   - POST /create-claim`);
  console.log(`   - GET  /check-claim?claim_id=<id>`);
  console.log(`   - GET  /claim/:claim_id`);
  console.log(`   - POST /claim/:claim_id/submit`);
  console.log(`   - POST /complete-claim`);
  console.log(`   - GET  /api/metadata/:claim_id`);
  console.log('═══════════════════════════════════════');
});

