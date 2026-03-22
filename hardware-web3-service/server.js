import express from 'express';
import cors from 'cors';
import path from 'path';
import fs from 'fs';
import { z } from 'zod';
import { v4 as uuidv4 } from 'uuid';
import multer from 'multer';
import dotenv from 'dotenv';
import { fileURLToPath } from 'url';
import os from 'os';
import { ethers } from 'ethers';

dotenv.config();

import dbService from './dbService.js';
import web3Service from './web3Service.js';
import filecoinService from './filecoinService.js';
import claimClient from './claimClient.js';
import { generateProof, submitProofOnChain } from './vlayerService.js';
import privyService from './privyService.js';
import deploymentService from './deploymentService.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const app = express();
const PORT = process.env.PORT || 5000;

// Enable CORS for all routes
app.use(cors());

function ensureCapturesDirectory() {
  let capturesPath = process.env.CAPTURES_PATH;
  
  if (!capturesPath) {
    capturesPath = path.resolve(__dirname, '../captures');
  } else {
    capturesPath = path.resolve(capturesPath);
  }

  if (!fs.existsSync(capturesPath)) {
    try {
      fs.mkdirSync(capturesPath, { recursive: true, mode: 0o755 });
      console.log(`✅ Created captures directory: ${capturesPath}`);
    } catch (mkdirError) {
      if (mkdirError.code === 'EACCES') {
        const fallbackPath = path.join(os.homedir(), '.lensmint', 'captures');
        capturesPath = fallbackPath;
        if (!fs.existsSync(capturesPath)) {
          fs.mkdirSync(capturesPath, { recursive: true, mode: 0o755 });
        }
        console.log(`⚠️  Permission denied, using fallback path: ${capturesPath}`);
      } else {
        throw mkdirError;
      }
    }
  }
  
  return capturesPath;
}

const CAPTURES_PATH = ensureCapturesDirectory();

app.use(express.json({ limit: '50mb' }));
app.use(express.urlencoded({ extended: true, limit: '50mb' }));

app.use('/captures', express.static(CAPTURES_PATH));

const upload = multer({
  dest: CAPTURES_PATH,
  limits: { fileSize: 50 * 1024 * 1024 }
});

let servicesInitialized = false;

async function initializeServices() {
  try {
    console.log('🔄 Initializing services...');

    dbService.initialize();
    console.log('✅ Database initialized');

    await web3Service.initialize();

    if (web3Service.deviceWallet) {
      await filecoinService.initialize(web3Service.deviceWallet);
      if (filecoinService.initialized) {
        console.log('✅ Filecoin service initialized');
      } else {
        console.warn('⚠️ Filecoin service not available');
      }
    } else {
      console.warn('⚠️ Device wallet not available, Filecoin service disabled');
    }

    const claimHealth = await claimClient.healthCheck();
    if (claimHealth.status === 'ok') {
      console.log('✅ Claim server connected');
    } else {
      console.warn('⚠️ Claim server not available');
    }

    privyService.initialize();
    if (privyService.initialized) {
      console.log('✅ Privy service initialized');
    } else {
      console.warn('⚠️ Privy service not available');
    }

    servicesInitialized = true;
    console.log('✅ All services initialized');
  } catch (error) {
    console.error('❌ Error initializing services:', error);
    servicesInitialized = false;
  }
}

let claimPollingInterval = null;
const processingEditionRequests = new Set();

function startClaimPolling() {
  if (claimPollingInterval) return;

  claimPollingInterval = setInterval(async () => {
    try {
      await processEditionRequests();
    } catch (error) {
      console.error('❌ Error in claim polling:', error);
    }
  }, 10000);

  async function processEditionRequests() {
    try {
      const response = await claimClient.getPendingEditionRequests(50);
      
      if (!response || !response.success || !response.edition_requests || response.edition_requests.length === 0) {
        return;
      }

      const newRequests = response.edition_requests.filter(req => !processingEditionRequests.has(req.id));
      
      if (newRequests.length === 0) {
        return;
      }

      console.log(`🔄 Processing ${newRequests.length} pending edition request(s)...`);

      for (const request of newRequests) {
        if (processingEditionRequests.has(request.id)) {
          continue;
        }

        processingEditionRequests.add(request.id);

        try {
          console.log(`🔄 Processing edition request: ${request.id} for claim ${request.claim_id}`);
          console.log(`   📍 Wallet address: ${request.wallet_address}`);
          console.log(`   📍 Original Token ID: ${request.original_token_id}`);

          if (!request.original_token_id) {
            console.warn(`   ⚠️ No original token ID for request ${request.id}, skipping`);
            processingEditionRequests.delete(request.id);
            continue;
          }

          if (!request.wallet_address) {
            console.error(`   ❌ No wallet address for request ${request.id}, skipping`);
            processingEditionRequests.delete(request.id);
            continue;
          }

          let recipientAddress = request.wallet_address;
          if (typeof recipientAddress === 'string') {
            recipientAddress = recipientAddress.trim();
            if (!recipientAddress.startsWith('0x')) {
              console.error(`   ❌ Invalid address format: ${recipientAddress}`);
              processingEditionRequests.delete(request.id);
              continue;
            }
            if (recipientAddress.length !== 42) {
              console.error(`   ❌ Invalid address length: ${recipientAddress.length} (expected 42)`);
              processingEditionRequests.delete(request.id);
              continue;
            }
          }
          
          const originalTokenId = Number(request.original_token_id);
          if (isNaN(originalTokenId)) {
            console.error(`   ❌ Invalid token ID: ${request.original_token_id}`);
            processingEditionRequests.delete(request.id);
            continue;
          }
          
          try {
            await claimClient.updateEditionRequest(request.id, {
              status: 'processing'
            });
          } catch (e) {
            console.warn(`   ⚠️ Could not mark request as processing: ${e.message}`);
          }
          
          const mintResult = await web3Service.mintEdition(
            recipientAddress,
            originalTokenId
          );

          await claimClient.updateEditionRequest(request.id, {
            status: 'completed',
            tx_hash: mintResult.txHash,
            token_id: mintResult.tokenId || 'edition'
          });

          console.log(`✅ Edition minted! Request ID: ${request.id}, TX: ${mintResult.txHash}`);
        } catch (error) {
          console.error(`❌ Error processing edition request ${request.id}:`, error);
          try {
            await claimClient.updateEditionRequest(request.id, {
              status: 'failed',
              error_message: error.message
            });
          } catch (e) {
            console.error(`   ⚠️ Could not update edition request status: ${e.message}`);
          }
        } finally {
          processingEditionRequests.delete(request.id);
        }
      }
    } catch (error) {
      if (error.message && !error.message.includes('404')) {
        console.error('❌ Error fetching edition requests:', error.message);
      }
    }
  }

  console.log('✅ Claim polling started');
}

app.get('/health', (req, res) => {
  res.json({
    status: 'ok',
    service: 'LensMint Backend',
    servicesInitialized
  });
});

app.get('/api/status', async (req, res) => {
  try {
    const claimHealth = await claimClient.healthCheck();

    // Get device info if available
    let deviceInfo = null;
    let deviceBalance = null;
    if (web3Service.deviceWallet) {
      try {
        deviceInfo = await web3Service.getDeviceInfo(web3Service.deviceWallet.address);
        deviceBalance = await web3Service.getDeviceBalance();
      } catch (error) {
        // Device not registered yet
      }
    }

    res.json({
      success: true,
      status: 'online',
      services: {
        database: servicesInitialized,
        blockchain: web3Service.initialized,
        filecoin: filecoinService.initialized,
        claimServer: claimHealth.status === 'ok'
      },
      device: deviceInfo,
      balance: deviceBalance,
      timestamp: new Date().toISOString()
    });
  } catch (error) {
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.get('/api/balance', async (req, res) => {
  try {
    const { address } = req.query;
    console.log(`\n💰 [BALANCE CHECK] Request from: ${address || 'unknown'}`);

    if (!web3Service.deviceWallet) {
      console.log('   ❌ Device wallet not initialized');
      return res.status(400).json({
        success: false,
        error: 'Device wallet not initialized'
      });
    }

    console.log('   🔍 Checking ETH balance...');
    console.log(`   📍 Wallet address: ${web3Service.deviceWallet.address}`);
    const ethBalance = await web3Service.getDeviceBalance();
    
    if (!ethBalance) {
      console.log('   ❌ Failed to get ETH balance');
      return res.status(500).json({
        success: false,
        error: 'Failed to get ETH balance'
      });
    }

    const balanceEth = parseFloat(ethBalance.balance);
    const minEthBalance = parseFloat(process.env.MIN_ETH_BALANCE || '0.01');
    const hasEnoughEth = balanceEth >= minEthBalance;
    
    console.log(`   📊 ETH Balance: ${balanceEth} ETH`);
    console.log(`   📊 ETH Required: ${minEthBalance} ETH`);
    console.log(`   ${hasEnoughEth ? '✅' : '⚠️'} ETH Status: ${hasEnoughEth ? 'Sufficient' : 'Low - needs funding'}`);

    let usdfcBalance = null;
    let hasEnoughUsdfc = false;
    const minUsdfcBalance = parseFloat(process.env.MIN_USDFC_BALANCE || '0.1');

    if (filecoinService.initialized) {
      try {
        console.log('   🔍 Checking USDFC balance...');
        const balanceUsdfc = await filecoinService.getUSDFCBalance();
        hasEnoughUsdfc = balanceUsdfc >= minUsdfcBalance;
        
        console.log(`   📊 USDFC Balance: ${balanceUsdfc} USDFC`);
        console.log(`   📊 USDFC Required: ${minUsdfcBalance} USDFC`);
        console.log(`   ${hasEnoughUsdfc ? '✅' : '⚠️'} USDFC Status: ${hasEnoughUsdfc ? 'Sufficient' : 'Low - needs funding'}`);
        
        usdfcBalance = {
          address: filecoinService.wallet.address,
          balance: balanceUsdfc,
          minBalance: minUsdfcBalance,
          hasEnoughBalance: hasEnoughUsdfc,
          needsFunding: !hasEnoughUsdfc
        };
      } catch (error) {
        console.warn('   ⚠️ Could not get USDFC balance:', error.message);
      }
    } else {
      console.log('   ⚠️ Filecoin service not initialized - skipping USDFC check');
    }

    const allFunded = hasEnoughEth && (usdfcBalance ? hasEnoughUsdfc : false);
    const needsFunding = !hasEnoughEth || (usdfcBalance ? !hasEnoughUsdfc : true);
    
    console.log(`   ${allFunded ? '✅' : '⚠️'} Overall Status: ${allFunded ? 'All balances sufficient' : 'Some balances need funding'}`);
    console.log(`💰 [BALANCE CHECK] Complete - Returning response\n`);

    res.json({
      success: true,
      address: ethBalance.address,
      eth: {
        balance: ethBalance.balance,
        balanceEth: balanceEth,
        minBalance: minEthBalance,
        hasEnoughBalance: hasEnoughEth,
        needsFunding: !hasEnoughEth
      },
      usdfc: usdfcBalance || {
        balance: '0',
        balanceUsdfc: 0,
        minBalance: minUsdfcBalance,
        hasEnoughBalance: false,
        needsFunding: true,
        available: false
      },
      hasEnoughBalance: allFunded,
      needsFunding: needsFunding
    });
  } catch (error) {
    console.error('❌ Error getting balance:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.post('/api/device/register', async (req, res) => {
  try {
    const {
      deviceAddress,
      publicKey,
      deviceId,
      cameraId,
      model,
      firmwareVersion
    } = req.body;

    if (!deviceAddress || !publicKey || !deviceId || !cameraId) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: deviceAddress, publicKey, deviceId, cameraId'
      });
    }

    const result = await web3Service.registerDevice({
      deviceAddress,
      publicKey,
      deviceId,
      cameraId,
      model: model || 'Raspberry Pi',
      firmwareVersion: firmwareVersion || '1.0.0'
    });

    dbService.cacheDevice({
      device_address: deviceAddress,
      device_id: deviceId,
      camera_id: cameraId,
      public_key: publicKey,
      is_registered: true
    });

    res.json({
      success: true,
      txHash: result.txHash,
      blockNumber: result.blockNumber,
      deviceAddress
    });
  } catch (error) {
    console.error('❌ Device registration failed:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.post('/api/device/ensure-registered', async (req, res) => {
  try {
    const {
      deviceAddress,
      publicKey,
      deviceId,
      cameraId,
      model,
      firmwareVersion
    } = req.body;

    console.log(`\n📋 [DEVICE REGISTRATION] Starting for: ${deviceAddress}`);
    console.log(`   Device ID: ${deviceId}`);
    console.log(`   Camera ID: ${cameraId}`);

    if (!deviceAddress || !publicKey || !deviceId || !cameraId) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: deviceAddress, publicKey, deviceId, cameraId'
      });
    }

    let registered = false;
    let activated = false;
    let registrationTx = null;
    let activationTx = null;

    // Step 1: Check if device is registered and active
    console.log(`\n🔍 Step 1: Checking device registration status...`);
    try {
      // Use isDeviceActive which is more reliable
      const isActive = await web3Service.isDeviceActive(deviceAddress);
      console.log(`   📊 isDeviceActive result: ${isActive}`);
      
      if (isActive) {
        // Device is registered and active
        registered = true;
        activated = true;
        console.log(`   ✅ Device ${deviceAddress} is registered and active`);
        
        // Get full device info for logging
        try {
          const deviceInfo = await web3Service.getDeviceInfo(deviceAddress);
          if (deviceInfo) {
            console.log(`   📊 Registration details:`);
            console.log(`      - Device ID: ${deviceInfo.deviceId}`);
            console.log(`      - Model: ${deviceInfo.model}`);
            console.log(`      - Firmware: ${deviceInfo.firmwareVersion}`);
            console.log(`      - Registered by: ${deviceInfo.registeredBy}`);
            console.log(`      - Registration time: ${new Date(Number(deviceInfo.registrationTime) * 1000).toISOString()}`);
          }
        } catch (e) {
          // Ignore - we already know it's active
        }
      } else {
        // Device is not active - check if it's registered at all
        console.log(`   📊 Device is not active, checking registration status...`);
        const deviceInfo = await web3Service.getDeviceInfo(deviceAddress);
        
        if (deviceInfo && deviceInfo.deviceAddress && deviceInfo.deviceAddress !== '0x0000000000000000000000000000000000000000') {
          // Device is registered but inactive
          registered = true;
          activated = false;
          console.log(`   ⚠️ Device ${deviceAddress} is registered but INACTIVE`);
          console.log(`   🔄 Activating device...`);
          
          try {
            // Activate the device
            const updateResult = await web3Service.updateDevice(
              deviceAddress,
              firmwareVersion || '1.0.0',
              true
            );
            activationTx = updateResult.txHash;
            activated = true;
            console.log(`   ✅ Activation transaction submitted: ${activationTx}`);
            console.log(`   ⏳ Waiting for transaction confirmation...`);
            console.log(`   ✅ Device ${deviceAddress} activated in block ${updateResult.blockNumber}`);
          } catch (updateError) {
            if (updateError.message?.includes('not registered')) {
              console.warn(`   ⚠️ Activation failed: Device not registered (contract state mismatch)`);
              console.warn(`   🔄 Will try to register device instead...`);
              registered = false; // Mark as not registered so we register it
            } else {
              throw updateError;
            }
          }
        } else {
          console.log(`   ❌ Device ${deviceAddress} is NOT registered`);
        }
      }
    } catch (error) {
      // Device not registered or error checking
      console.log(`   ❌ Error checking device status: ${error.message}`);
      console.log(`   ℹ️ Assuming device is not registered`);
    }

    // Step 3: Register if not registered
    if (!registered) {
      console.log(`\n📝 Step 3: Registering device...`);
      console.log(`   Device Address: ${deviceAddress}`);
      console.log(`   Device ID: ${deviceId}`);
      console.log(`   Camera ID: ${cameraId}`);
      console.log(`   Model: ${model || 'Raspberry Pi'}`);
      console.log(`   Firmware: ${firmwareVersion || '1.0.0'}`);
      
      try {
        console.log(`   🔄 Calling registerDevice()...`);
        const result = await web3Service.registerDevice({
          deviceAddress,
          publicKey,
          deviceId,
          cameraId,
          model: model || 'Raspberry Pi',
          firmwareVersion: firmwareVersion || '1.0.0'
        });
        
        registrationTx = result.txHash;
        registered = true;
        activated = true; // Registration sets isActive to true
        console.log(`   ✅ Registration transaction submitted: ${registrationTx}`);
        console.log(`   ⏳ Waiting for transaction confirmation...`);
        console.log(`   ✅ Device registered in block ${result.blockNumber}`);
        console.log(`   ✅ Device is automatically set to ACTIVE on registration`);

        // Cache device info in database
        dbService.cacheDevice({
          device_address: deviceAddress,
          device_id: deviceId,
          camera_id: cameraId,
          public_key: publicKey,
          is_registered: true
        });
        console.log(`   💾 Device info cached in database`);
      } catch (error) {
        console.log(`   ❌ Registration error: ${error.message}`);
        
        // Check if error is "already registered"
        if (error.message?.includes('already registered') || error.message?.includes('Device already registered')) {
          console.log(`   ℹ️ Device appears to be already registered (race condition?)`);
          registered = true;
          
          // Try to activate if not active
          try {
            console.log(`   🔄 Re-checking device status...`);
            const deviceInfo = await web3Service.getDeviceInfo(deviceAddress);
            if (deviceInfo && !deviceInfo.isActive) {
              console.log(`   🔄 Device is registered but inactive, activating...`);
              const updateResult = await web3Service.updateDevice(
                deviceAddress,
                firmwareVersion || '1.0.0',
                true
              );
              activationTx = updateResult.txHash;
              activated = true;
              console.log(`   ✅ Activation transaction: ${activationTx}`);
            } else if (deviceInfo && deviceInfo.isActive) {
              activated = true;
              console.log(`   ✅ Device is already active`);
            }
          } catch (e) {
            console.error(`   ❌ Error checking/activating device: ${e.message}`);
          }
        } else {
          throw error;
        }
      }
    }

    console.log(`\n📊 [DEVICE REGISTRATION] Summary:`);
    console.log(`   Registered: ${registered ? '✅' : '❌'}`);
    console.log(`   Active: ${activated ? '✅' : '❌'}`);
    if (registrationTx) {
      console.log(`   Registration TX: ${registrationTx}`);
    }
    if (activationTx) {
      console.log(`   Activation TX: ${activationTx}`);
    }
    console.log(`✅ [DEVICE REGISTRATION] Complete\n`);

    res.json({
      success: true,
      registered,
      activated,
      deviceAddress,
      registrationTx,
      activationTx
    });
  } catch (error) {
    console.error(`\n❌ [DEVICE REGISTRATION] Failed: ${error.message}`);
    console.error(`   Stack: ${error.stack}\n`);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.post('/api/device/update', async (req, res) => {
  try {
    const {
      deviceAddress,
      firmwareVersion,
      isActive
    } = req.body;

    if (!deviceAddress || typeof isActive !== 'boolean') {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: deviceAddress, isActive (boolean)'
      });
    }

    const result = await web3Service.updateDevice(
      deviceAddress,
      firmwareVersion || '1.0.0',
      isActive
    );

    res.json({
      success: true,
      txHash: result.txHash,
      blockNumber: result.blockNumber,
      deviceAddress,
      isActive: result.isActive
    });
  } catch (error) {
    console.error('❌ Device update failed:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

const captureSchema = z.object({
  deviceAddress: z.string().regex(/^0x[a-fA-F0-9]{40}$/),
  imageHash: z.string().regex(/^(0x)?[a-fA-F0-9]{64}$/),
  cameraId: z.string().max(64),
  signature: z.string().optional()
});

app.post('/api/images/upload', upload.single('image'), async (req, res) => {
  try {
    if (!req.file) {
      return res.status(400).json({
        success: false,
        error: 'No image file provided'
      });
    }

    // Validate file type
    const allowedMimeTypes = ['image/jpeg', 'image/jpg', 'image/png', 'image/webp'];
    if (!allowedMimeTypes.includes(req.file.mimetype)) {
      fs.unlinkSync(req.file.path);
      return res.status(400).json({
        success: false,
        error: `Invalid file type. Allowed types: ${allowedMimeTypes.join(', ')}`
      });
    }

    // Validate file size (max 50MB)
    const maxSize = 50 * 1024 * 1024;
    if (req.file.size > maxSize) {
      fs.unlinkSync(req.file.path);
      return res.status(400).json({
        success: false,
        error: `File too large. Maximum size: ${maxSize / 1024 / 1024}MB`
      });
    }

    const {
      imageHash,
      signature,
      cameraId,
      deviceAddress
    } = req.body;

    if (!imageHash || !signature || !cameraId || !deviceAddress) {
      // Clean up uploaded file
      fs.unlinkSync(req.file.path);
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: imageHash, signature, cameraId, deviceAddress'
      });
    }
    
    try {
  req.body = captureSchema.parse({
    imageHash,
    signature,
    cameraId,
    deviceAddress
  });
} catch (error) {
  fs.unlinkSync(req.file.path);
  return res.status(400).json({
    success: false,
    error: 'Invalid request payload',
    details: error.errors?.map(e => e.message) || error.message
  });
}

    const filename = `photo_${Date.now()}.jpg`;
    const filepath = path.join(CAPTURES_PATH, filename);
    fs.renameSync(req.file.path, filepath);

    const image = dbService.createImage({
      filename,
      filepath,
      image_hash: imageHash,
      camera_id: cameraId,
      device_address: deviceAddress,
      signature
    });

    console.log(`📸 Image uploaded: ${image.id} - ${filename}`);

    let filecoinCid = null;
    let metadataCid = null;
    let claimId = null;
    let claimUrl = null;

    console.log(`\n📸 [IMAGE UPLOAD] Processing image: ${image.id}`);
    console.log(`   File: ${filename}`);
    console.log(`   Device: ${deviceAddress}`);
    console.log(`   Camera ID: ${cameraId}`);

    try {
      if (filecoinService.initialized) {
        console.log(`   🔄 Uploading to Filecoin...`);
        filecoinCid = await filecoinService.uploadImage(filepath);
        console.log(`   ✅ Image uploaded to Filecoin: ${filecoinCid}`);

        const deviceInfo = dbService.getDevice(deviceAddress);
        const deviceId = deviceInfo?.device_id || 'unknown';

        const metadata = filecoinService.createMetadata({
          name: `LensMint Photo #${image.id}`,
          description: 'Captured by LensMint Camera',
          imageCid: filecoinCid,
          deviceAddress,
          deviceId,
          cameraId,
          imageHash,
          signature,
          timestamp: new Date().toISOString()
        });

        console.log(`   🔄 Uploading metadata to Filecoin...`);
        metadataCid = await filecoinService.uploadMetadata(metadata);
        console.log(`   ✅ Metadata uploaded to Filecoin: ${metadataCid}`);

        dbService.updateImageStatus(image.id, 'uploaded', {
          filecoin_cid: filecoinCid,
          filecoin_metadata_cid: metadataCid
        });

        console.log(`   🔄 Creating claim with claim server...`);
        claimId = uuidv4();
        try {
          const deviceInfo = dbService.getDevice(deviceAddress);
          const deviceId = deviceInfo?.device_id || 'unknown';

          const claimResult = await claimClient.createClaim(
            claimId, 
            filecoinCid,
            metadataCid,
            deviceId,
            cameraId,
            imageHash,
            signature,
            deviceAddress
          );
          claimUrl = claimResult.claim_url;

          dbService.createClaim(claimId, image.id, filecoinCid);

          dbService.updateImageStatus(image.id, 'uploaded', {
            claim_id: claimId
          });

          console.log(`   ✅ Claim created: ${claimId}`);
          console.log(`   ✅ Claim URL: ${claimUrl}`);

          const ownerWallet = process.env.OWNER_WALLET_ADDRESS;
          if (ownerWallet) {
            console.log(`   🎨 Minting original NFT to owner wallet: ${ownerWallet}`);
            try {
              const mintResult = await web3Service.mintOriginal({
                recipient: ownerWallet,
                ipfsHash: filecoinCid,
                imageHash: imageHash,
                signature: signature,
                maxEditions: 0
              });

              dbService.updateImageStatus(image.id, 'minted', {
                token_id: mintResult.tokenId,
                tx_hash: mintResult.txHash,
                recipient_address: ownerWallet
              });

              dbService.updateClaim(claimId, {
                token_id: mintResult.tokenId,
                tx_hash: mintResult.txHash,
                recipient_address: ownerWallet,
                status: 'open'
              });

              try {
                await claimClient.updateClaimStatus(claimId, 'open', mintResult.tokenId, mintResult.txHash);
                console.log(`   ✅ Claim server updated: status=open, token_id=${mintResult.tokenId}`);
              } catch (e) {
                console.warn(`   ⚠️ Could not update claim server status: ${e.message}`);
              }

              console.log(`   ✅ Original NFT minted! Token ID: ${mintResult.tokenId}, TX: ${mintResult.txHash}`);
              console.log(`   ✅ Claim is now OPEN - users can mint unlimited editions`);

              (async () => {
                try {
                  console.log(`\n🔐 [VLAYER] Generating ZK proof for claim: ${claimId} (background)`);
                  const proofData = await generateProof(claimId);
                  
                  dbService.createProof(
                    claimId,
                    mintResult.tokenId,
                    proofData.data.zkProof,
                    proofData.data.journalDataAbi
                  );
                  
                  console.log(`   ✅ ZK proof generated and stored for claim: ${claimId}`);
                  
                  try {
                    console.log(`   🔄 Syncing proof status to claim server (pending)...`);
                    await claimClient.updateProofStatus(claimId, mintResult.tokenId, 'pending', null);
                    console.log(`   ✅ Proof status synced to claim server: pending`);
                  } catch (syncError) {
                    console.error(`   ❌ Could not sync proof status to claim server: ${syncError.message}`);
                    console.error(`   ❌ Error details:`, syncError);
                  }
                  
                  try {
                    console.log(`   📤 Submitting proof on-chain...`);
                    const deviceWallet = web3Service.deviceWallet;
                    const submissionResult = await submitProofOnChain(
                      claimId,
                      proofData.data.zkProof,
                      proofData.data.journalDataAbi,
                      deviceWallet
                    );
                    
                    dbService.updateProof(claimId, {
                      proof_tx_hash: submissionResult.transactionHash,
                      verification_status: submissionResult.status,
                      submitted_at: 'now'
                    });
                    
                    try {
                      console.log(`   🔄 Syncing proof submission to claim server (${submissionResult.status})...`);
                      await claimClient.updateProofStatus(
                        claimId,
                        mintResult.tokenId,
                        submissionResult.status,
                        submissionResult.transactionHash
                      );
                      console.log(`   ✅ Proof submission synced to claim server: ${submissionResult.status}`);
                    } catch (syncError) {
                      console.error(`   ❌ Could not sync proof submission to claim server: ${syncError.message}`);
                      console.error(`   ❌ Error details:`, syncError);
                    }
                    
                    console.log(`   ✅ Proof submitted on-chain! TX: ${submissionResult.transactionHash}`);
                  } catch (submitError) {
                    console.error(`   ⚠️ Proof submission failed: ${submitError.message}`);
                    console.error(`   ⚠️ Proof generated but not submitted - can retry later`);
                    dbService.updateProof(claimId, {
                      verification_status: 'failed'
                    });
                    
                    try {
                      console.log(`   🔄 Syncing failed status to claim server...`);
                      await claimClient.updateProofStatus(claimId, mintResult.tokenId, 'failed', null);
                      console.log(`   ✅ Failed status synced to claim server`);
                    } catch (syncError) {
                      console.error(`   ❌ Could not sync failed status to claim server: ${syncError.message}`);
                      console.error(`   ❌ Error details:`, syncError);
                    }
                  }
                } catch (proofError) {
                  console.error(`   ⚠️ ZK proof generation failed: ${proofError.message}`);
                  console.error(`   ⚠️ NFT minted successfully but proof generation failed - can retry later`);
                }
              })();
            } catch (mintError) {
              console.error(`   ❌ Original NFT minting failed: ${mintError.message}`);
              console.error(`   ⚠️ Image uploaded but original NFT not minted - will retry later`);
            }
          } else {
            console.warn(`   ⚠️ OWNER_WALLET_ADDRESS not set - original NFT will not be minted`);
            console.warn(`   ⚠️ Set OWNER_WALLET_ADDRESS in .env to enable automatic minting`);
          }
        } catch (claimError) {
          console.error(`   ❌ Claim creation error: ${claimError.message}`);
          console.error(`   ⚠️ Filecoin upload succeeded but claim creation failed`);
          console.error(`   ⚠️ Check if claim server is running at ${process.env.CLAIM_SERVER_URL || 'https://lensmint.onrender.com'}`);
        }
      } else {
        console.warn(`   ⚠️ Filecoin not initialized, skipping upload and claim creation`);
        console.warn(`   ⚠️ Image saved locally but no claim created`);
      }
    } catch (error) {
      console.error(`   ❌ Filecoin upload error: ${error.message}`);
      console.error(`   ⚠️ Image saved locally but Filecoin upload failed`);
    }

    console.log(`📸 [IMAGE UPLOAD] Complete - Claim ID: ${claimId || 'none'}\n`);

    res.json({
      success: true,
      imageId: image.id,
      filename,
      filepath: `/captures/${filename}`,
      imageHash,
      filecoinCid: filecoinCid || null,
      metadataCid: metadataCid || null,
      claimId: claimId || null,
      claimUrl: claimUrl || null,
      qrCodeUrl: claimUrl || null, // Same as claimUrl for QR code
      status: filecoinCid ? 'uploaded' : 'saved'
    });
  } catch (error) {
    console.error('❌ Image upload failed:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.get('/api/images/list', (req, res) => {
  try {
    const { status } = req.query;
    const images = dbService.getImages(status, 100);

    res.json({
      success: true,
      count: images.length,
      images: images.map(img => ({
        id: img.id,
        filename: img.filename,
        filepath: `/captures/${img.filename}`,
        imageHash: img.image_hash,
        cameraId: img.camera_id,
        status: img.status,
        filecoinCid: img.filecoin_cid,
        claimId: img.claim_id,
        tokenId: img.token_id,
        txHash: img.tx_hash,
        createdAt: img.created_at
      }))
    });
  } catch (error) {
    console.error('❌ Error listing images:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.get('/api/images/:id', async (req, res) => {
  try {
    const { id } = req.params;
    const image = dbService.getImageById(parseInt(id));

    if (!image) {
      return res.status(404).json({
        success: false,
        error: 'Image not found'
      });
    }

    let claimStatus = null;
    if (image.claim_id) {
      try {
        const claim = await claimClient.checkClaim(image.claim_id);
        claimStatus = claim;
      } catch (error) {
      }
    }

    const claimUrl = image.claim_id
      ? `${process.env.CLAIM_SERVER_URL || 'https://lensmint.onrender.com'}/claim/${image.claim_id}`
      : null;

    res.json({
      success: true,
      image: {
        id: image.id,
        filename: image.filename,
        filepath: `/captures/${image.filename}`,
        imageHash: image.image_hash,
        cameraId: image.camera_id,
        deviceAddress: image.device_address,
        status: image.status,
        filecoinCid: image.filecoin_cid,
        metadataCid: image.filecoin_metadata_cid,
        claimId: image.claim_id,
        claimUrl,
        tokenId: image.token_id,
        txHash: image.tx_hash,
        recipientAddress: image.recipient_address,
        signature: image.signature,
        createdAt: image.created_at,
        uploadedAt: image.uploaded_at,
        mintedAt: image.minted_at,
        errorMessage: image.error_message
      },
      claimStatus
    });
  } catch (error) {
    console.error('❌ Error getting image:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.get('/api/claims/check', async (req, res) => {
  try {
    const { claim_id } = req.query;

    if (!claim_id) {
      return res.status(400).json({
        success: false,
        error: 'claim_id is required'
      });
    }

    const claim = await claimClient.checkClaim(claim_id);

    res.json({
      success: true,
      ...claim
    });
  } catch (error) {
    console.error('❌ Error checking claim:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.get('/status', (req, res) => {
  res.json({
    success: true,
    status: 'online',
    servicesInitialized,
    timestamp: new Date().toISOString()
  });
});

/**
 * GET /api/proofs/:claim_id
 * Get proof data for a claim ID
 */
app.get('/api/proofs/:claim_id', async (req, res) => {
  try {
    const { claim_id } = req.params;

    const proof = dbService.getProof(claim_id);

    if (!proof) {
      return res.status(404).json({
        success: false,
        error: 'Proof not found for this claim'
      });
    }

    res.json({
      success: true,
      claim_id: proof.claim_id,
      token_id: proof.token_id,
      zk_proof: proof.zk_proof,
      journal_data_abi: proof.journal_data_abi,
      proof_tx_hash: proof.proof_tx_hash,
      verification_status: proof.verification_status,
      created_at: proof.created_at,
      submitted_at: proof.submitted_at,
      verified_at: proof.verified_at
    });
  } catch (error) {
    console.error('❌ Error getting proof:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.get('/api/proofs/token/:token_id', async (req, res) => {
  try {
    const { token_id } = req.params;

    const proof = dbService.getProofByTokenId(parseInt(token_id));

    if (!proof) {
      return res.status(404).json({
        success: false,
        error: 'Proof not found for this token ID'
      });
    }

    res.json({
      success: true,
      claim_id: proof.claim_id,
      token_id: proof.token_id,
      zk_proof: proof.zk_proof,
      journal_data_abi: proof.journal_data_abi,
      proof_tx_hash: proof.proof_tx_hash,
      verification_status: proof.verification_status,
      created_at: proof.created_at,
      submitted_at: proof.submitted_at,
      verified_at: proof.verified_at
    });
  } catch (error) {
    console.error('❌ Error getting proof by token ID:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

// Privy Session Signer Endpoints
app.post('/api/privy/create-session-signer', async (req, res) => {
  try {
    if (!privyService.initialized) {
      return res.status(503).json({
        success: false,
        error: 'Privy service not initialized'
      });
    }

    const { walletAddress, userId } = req.body;

    if (!walletAddress || !userId) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: walletAddress, userId'
      });
    }

    const sessionSigner = await privyService.createSessionSigner(walletAddress, userId);

    res.json({
      success: true,
      sessionSigner: {
        id: sessionSigner.id,
        permissions: sessionSigner.permissions
      },
      signerAddress: sessionSigner.address
    });
  } catch (error) {
    console.error('❌ Error creating session signer:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

app.post('/api/privy/mint-with-signer', async (req, res) => {
  try {
    if (!privyService.initialized) {
      return res.status(503).json({
        success: false,
        error: 'Privy service not initialized'
      });
    }

    const {
      recipient,
      ipfsHash,
      imageHash,
      signature,
      maxEditions,
      sessionSignerId
    } = req.body;

    if (!sessionSignerId || !recipient || !ipfsHash || !imageHash || !signature) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields'
      });
    }

    if (!web3Service.initialized || !web3Service.lensMint) {
      return res.status(503).json({
        success: false,
        error: 'Web3 service not initialized'
      });
    }

    // Get contract ABI and address
    const lensMintAddress = deploymentService.getLensMintAddress() || process.env.LENSMINT_ERC1155_ADDRESS;
    
    if (!lensMintAddress) {
      return res.status(400).json({
        success: false,
        error: 'LensMint contract address not configured'
      });
    }

    // Get contract ABI
    const lensMintABI = deploymentService.getLensMintABI() || web3Service.getLensMintABI();
    const lensMintInterface = new ethers.Interface(lensMintABI);
    
    // Prepare transaction data
    const transaction = {
      to: lensMintAddress,
      data: lensMintInterface.encodeFunctionData('mintOriginal', [
        recipient,
        ipfsHash,
        imageHash,
        signature,
        maxEditions || 0
      ])
    };

    // Send transaction using Privy session signer with gas sponsorship
    const result = await privyService.sendTransaction(sessionSignerId, transaction);

    console.log(`🎨 Minted NFT using Privy session signer`);
    console.log(`   Transaction: ${result.txHash}`);
    console.log(`   Block: ${result.blockNumber}`);

    res.json({
      success: true,
      txHash: result.txHash,
      blockNumber: result.blockNumber,
      message: 'NFT minted successfully with gas sponsorship'
    });
  } catch (error) {
    console.error('❌ Error minting with session signer:', error);
    res.status(500).json({
      success: false,
      error: error.message
    });
  }
});

async function startServer() {
  await initializeServices();

  if (servicesInitialized) {
    startClaimPolling();
  }

  app.listen(PORT, () => {
    console.log('═══════════════════════════════════════');
    console.log('📷 LensMint Backend Server');
    console.log('═══════════════════════════════════════');
    console.log(`🚀 Server running on port ${PORT}`);
    console.log(`📁 Captures directory: ${CAPTURES_PATH}`);
    console.log(`🌐 API Endpoints:`);
    console.log(`   - GET  /health`);
    console.log(`   - GET  /api/status`);
    console.log(`   - POST /api/device/register`);
    console.log(`   - POST /api/images/upload`);
    console.log(`   - GET  /api/images/list`);
    console.log(`   - GET  /api/images/:id`);
    console.log(`   - GET  /api/claims/check`);
    console.log(`   - GET  /api/proofs/:claim_id`);
    console.log(`   - GET  /api/proofs/token/:token_id`);
    console.log(`   - POST /api/privy/create-session-signer`);
    console.log(`   - POST /api/privy/mint-with-signer`);
    console.log('═══════════════════════════════════════');
  });
}

process.on('SIGINT', () => {
  console.log('\n🛑 Shutting down gracefully...');

  if (claimPollingInterval) {
    clearInterval(claimPollingInterval);
  }

  dbService.close();
  process.exit(0);
});

startServer().catch(error => {
  console.error('❌ Failed to start server:', error);
  process.exit(1);
});
