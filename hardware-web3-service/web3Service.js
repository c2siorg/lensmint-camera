import { ethers } from 'ethers';
import hardwareKeyExtractor from './getHardwareKey.js';
import deploymentService from './deploymentService.js';

class Web3Service {
  constructor() {
    this.provider = null;
    this.deviceRegistry = null;
    this.lensMint = null;
    this.deviceWallet = null;
    this.initialized = false;
  }

  async initialize() {
    try {
      // Support multiple RPC URL environment variables
      const rpcUrl = process.env.SEPOLIA_RPC_URL || process.env.RPC_URL;
      
      let deviceRegistryAddress = deploymentService.getDeviceRegistryAddress();
      let lensMintAddress = deploymentService.getLensMintAddress();
      
      if (!deviceRegistryAddress) {
        deviceRegistryAddress = process.env.DEVICE_REGISTRY_ADDRESS;
        console.log('⚠️  Using DEVICE_REGISTRY_ADDRESS from env (deployment.json not found)');
      } else {
        console.log(`✅ Using DeviceRegistry from deployment.json: ${deviceRegistryAddress}`);
      }
      
      if (!lensMintAddress) {
        lensMintAddress = process.env.LENSMINT_ERC1155_ADDRESS;
        console.log('⚠️  Using LENSMINT_ERC1155_ADDRESS from env (deployment.json not found)');
      } else {
        console.log(`✅ Using LensMintERC1155 from deployment.json: ${lensMintAddress}`);
      }
      
      if (!rpcUrl) {
        throw new Error('RPC_URL or SEPOLIA_RPC_URL not set in environment');
      }

      if (!deviceRegistryAddress || !lensMintAddress) {
        console.warn('⚠️ Contract addresses not set. Web3 features disabled.');
        return false;
      }

      // Initialize provider
      this.provider = new ethers.JsonRpcProvider(rpcUrl);

      // Try to get device private key automatically from hardware identity
      let devicePrivateKey = process.env.DEVICE_PRIVATE_KEY;
      
      if (!devicePrivateKey) {
        console.log('🔄 Auto-extracting hardware private key from Python module...');
        try {
          devicePrivateKey = await hardwareKeyExtractor.getPrivateKey();
          if (devicePrivateKey) {
            console.log('✅ Hardware key extracted automatically');
          }
        } catch (error) {
          console.warn('⚠️ Could not auto-extract hardware key:', error.message);
          console.warn('   Options:');
          console.warn('   1. Remove DEVICE_PRIVATE_KEY from .env (if set)');
          console.warn('   2. Ensure Kivy app has initialized and created .device_key_export file');
          console.warn('   3. Run: cd kivy && python3 export_key.py');
        }
      }

      if (devicePrivateKey) {
        this.deviceWallet = new ethers.Wallet(devicePrivateKey, this.provider);
        const walletAddress = this.deviceWallet.address;
        console.log(`✅ Device wallet initialized: ${walletAddress}`);
        
        try {
          const isActive = await this.isDeviceActive(walletAddress);
          if (isActive) {
            console.log(`✅ Device wallet matches registered device: ${walletAddress}`);
          } else {
            const deviceInfo = await this.getDeviceInfo(walletAddress);
            if (!deviceInfo || !deviceInfo.deviceAddress) {
              console.warn(`⚠️ Device wallet ${walletAddress} is not registered`);
              console.warn(`   This wallet will be used for minting - ensure it's registered first`);
            }
          }
        } catch (error) {
          console.log(`ℹ️ Could not verify device registration: ${error.message}`);
        }
      } else {
        console.warn('⚠️ Device private key not available. Device wallet not initialized.');
        console.warn('   Some features (minting, registration) will be disabled.');
      }

      let deviceRegistryABI = deploymentService.getDeviceRegistryABI();
      let lensMintABI = deploymentService.getLensMintABI();
      
      if (!deviceRegistryABI) {
        console.log('⚠️  Using hardcoded DeviceRegistry ABI (not in deployment.json)');
        deviceRegistryABI = this.getDeviceRegistryABI();
      } else {
        console.log('✅ Using DeviceRegistry ABI from deployment.json');
      }
      
      if (!lensMintABI) {
        console.log('⚠️  Using hardcoded LensMintERC1155 ABI (not in deployment.json)');
        lensMintABI = this.getLensMintABI();
      } else {
        console.log('✅ Using LensMintERC1155 ABI from deployment.json');
      }

      const signerOrProvider = this.deviceWallet || this.provider;
      
      this.deviceRegistry = new ethers.Contract(
        deviceRegistryAddress,
        deviceRegistryABI,
        signerOrProvider
      );

      this.lensMint = new ethers.Contract(
        lensMintAddress,
        lensMintABI,
        signerOrProvider
      );

      this.initialized = true;
      console.log('✅ Web3 service initialized');
      console.log(`   DeviceRegistry: ${deviceRegistryAddress}`);
      console.log(`   LensMintERC1155: ${lensMintAddress}`);

      if (this.deviceWallet) {
        const walletAddress = this.deviceWallet.address;
        try {
          const isActive = await this.isDeviceActive(walletAddress);
          if (isActive) {
            console.log(`✅ Device wallet ${walletAddress} is registered and active`);
          } else {
            const deviceInfo = await this.getDeviceInfo(walletAddress);
            if (deviceInfo && deviceInfo.deviceAddress) {
              console.warn(`⚠️ Device wallet ${walletAddress} is registered but INACTIVE`);
              console.warn(`   Minting will fail until device is activated`);
            } else {
              console.warn(`⚠️ Device wallet ${walletAddress} is NOT registered`);
              console.warn(`   Minting will fail until device is registered and activated`);
            }
          }
        } catch (error) {
          console.log(`ℹ️ Could not verify device registration: ${error.message}`);
        }
      }

      return true;
    } catch (error) {
      console.error('❌ Failed to initialize Web3 service:', error);
      return false;
    }
  }

  async registerDevice(deviceInfo) {
    if (!this.initialized || !this.deviceWallet) {
      throw new Error('Web3 service not initialized or device wallet not set');
    }

    try {
      const {
        deviceAddress,
        publicKey,
        deviceId,
        cameraId,
        model,
        firmwareVersion
      } = deviceInfo;

      const tx = await this.deviceRegistry.registerDevice(
        deviceAddress,
        publicKey,
        deviceId,
        cameraId,
        model,
        firmwareVersion
      );

      console.log(`📝 Registering device: ${deviceId}`);
      console.log(`   Transaction: ${tx.hash}`);

      const receipt = await tx.wait();
      console.log(`✅ Device registered in block ${receipt.blockNumber}`);

      return {
        success: true,
        txHash: tx.hash,
        blockNumber: receipt.blockNumber
      };
    } catch (error) {
      console.error('❌ Device registration failed:', error);
      throw error;
    }
  }

  async isDeviceActive(deviceAddress) {
    if (!this.initialized) {
      return false;
    }

    try {
      return await this.deviceRegistry.isDeviceActive(deviceAddress);
    } catch (error) {
      console.error('Error checking device status:', error);
      return false;
    }
  }

  async updateDevice(deviceAddress, firmwareVersion, isActive) {
    if (!this.initialized || !this.deviceWallet) {
      throw new Error('Web3 service not initialized or device wallet not set');
    }

    try {
      const tx = await this.deviceRegistry.updateDevice(
        deviceAddress,
        firmwareVersion || '1.0.0',
        isActive
      );

      console.log(`📝 Updating device: ${deviceAddress}`);
      console.log(`   Active: ${isActive}`);
      console.log(`   Transaction: ${tx.hash}`);

      const receipt = await tx.wait();
      console.log(`✅ Device updated in block ${receipt.blockNumber}`);

      return {
        success: true,
        txHash: tx.hash,
        blockNumber: receipt.blockNumber,
        isActive
      };
    } catch (error) {
      console.error('❌ Device update failed:', error);
      throw error;
    }
  }

  async getDeviceInfo(deviceAddress) {
    if (!this.initialized) {
      return null;
    }

    try {
      const deviceInfo = await this.deviceRegistry.getDevice(deviceAddress);
      return {
        deviceAddress: deviceInfo.deviceAddress,
        publicKey: deviceInfo.publicKey,
        deviceId: deviceInfo.deviceId,
        model: deviceInfo.model,
        firmwareVersion: deviceInfo.firmwareVersion,
        registrationTime: deviceInfo.registrationTime.toString(),
        isActive: deviceInfo.isActive,
        registeredBy: deviceInfo.registeredBy
      };
    } catch (error) {
      console.error('Error getting device info:', error);
      return null;
    }
  }

  /**
   * Sign mint parameters using EIP-712 typed data.
   * Returns { v, r, s } for on-chain verification.
   */
  async signMintOriginal({ recipient, ipfsHash, imageHash, maxEditions = 0 }) {
    if (!this.deviceWallet) {
      throw new Error('Device wallet not initialized');
    }

    // Read the current nonce from the contract
    const nonce = await this.lensMint.nonces(this.deviceWallet.address);

    // EIP-712 domain — must match the contract's EIP712 constructor args
    const domain = {
      name: 'LensMintERC1155',
      version: '1',
      chainId: (await this.provider.getNetwork()).chainId,
      verifyingContract: await this.lensMint.getAddress()
    };

    const types = {
      MintOriginal: [
        { name: 'to', type: 'address' },
        { name: 'ipfsHash', type: 'string' },
        { name: 'imageHash', type: 'bytes32' },
        { name: 'maxEditions', type: 'uint256' },
        { name: 'nonce', type: 'uint256' }
      ]
    };

    // imageHash must be bytes32 (0x-prefixed 32-byte hex)
    const imageHashBytes32 = imageHash.startsWith('0x') ? imageHash : '0x' + imageHash;

    const value = {
      to: recipient,
      ipfsHash: ipfsHash,
      imageHash: imageHashBytes32,
      maxEditions: maxEditions,
      nonce: nonce
    };

    // Sign using ethers.js EIP-712 signer
    const signature = await this.deviceWallet.signTypedData(domain, types, value);

    // Split into v, r, s
    const sig = ethers.Signature.from(signature);

    console.log(`🔏 EIP-712 signature created for imageHash: ${imageHashBytes32}`);
    return { v: sig.v, r: sig.r, s: sig.s, imageHashBytes32 };
  }

  async mintOriginal(photoData) {
    if (!this.initialized || !this.deviceWallet) {
      throw new Error('Web3 service not initialized or device wallet not set');
    }

    try {
      const {
        recipient,
        ipfsHash,
        imageHash,
        maxEditions = 0
      } = photoData;

      const deviceAddress = this.deviceWallet.address;
      console.log(`🔍 [MINT] Checking device status for: ${deviceAddress}`);
      const isActive = await this.isDeviceActive(deviceAddress);
      console.log(`   📊 isDeviceActive result: ${isActive}`);

      if (!isActive) {
        try {
          const deviceInfo = await this.getDeviceInfo(deviceAddress);
          if (deviceInfo) {
            console.log(`   📊 Device info found:`, {
              deviceAddress: deviceInfo.deviceAddress,
              isActive: deviceInfo.isActive,
              deviceId: deviceInfo.deviceId
            });
            throw new Error(`Device ${deviceAddress} is ${deviceInfo.isActive ? 'registered but inactive' : 'not registered'}`);
          } else {
            throw new Error(`Device ${deviceAddress} is not registered`);
          }
        } catch (error) {
          if (error.message.includes('not registered') || error.message.includes('inactive')) {
            throw error;
          }
          throw new Error(`Device ${deviceAddress} not registered or inactive`);
        }
      }

      console.log(`   ✅ Device ${deviceAddress} is registered and active - proceeding with mint`);

      // Create EIP-712 signature
      const { v, r, s, imageHashBytes32 } = await this.signMintOriginal({
        recipient,
        ipfsHash,
        imageHash,
        maxEditions
      });

      console.log(`   🔏 EIP-712 signed — v:${v} r:${r.slice(0,10)}... s:${s.slice(0,10)}...`);

      // Estimate gas before minting
      try {
        const gasEstimate = await this.lensMint.mintOriginal.estimateGas(
          recipient,
          ipfsHash,
          imageHashBytes32,
          maxEditions,
          v, r, s
        );
        console.log(`   ⛽ Estimated gas: ${gasEstimate.toString()}`);
      } catch (gasError) {
        console.warn(`   ⚠️ Gas estimation failed: ${gasError.message}`);
      }

      const tx = await this.lensMint.mintOriginal(
        recipient,
        ipfsHash,
        imageHashBytes32,
        maxEditions,
        v, r, s
      );

      console.log(`🎨 Minting original photo NFT`);
      console.log(`   Transaction: ${tx.hash}`);
      console.log(`   IPFS: ${ipfsHash}`);

      const receipt = await tx.wait();

      const event = receipt.logs.find(log => {
        try {
          const parsed = this.lensMint.interface.parseLog(log);
          return parsed && parsed.name === 'TokenMinted';
        } catch {
          return false;
        }
      });

      let tokenId = null;
      if (event) {
        const parsed = this.lensMint.interface.parseLog(event);
        tokenId = parsed.args.tokenId.toString();
      }

      return {
        success: true,
        txHash: tx.hash,
        blockNumber: receipt.blockNumber,
        tokenId: tokenId
      };
    } catch (error) {
      console.error('❌ Minting failed:', error);
      throw error;
    }
  }

  async mintEdition(recipient, originalTokenId) {
    if (!this.initialized || !this.deviceWallet) {
      throw new Error('Web3 service not initialized or device wallet not set');
    }

    try {
      console.log(`   🔍 [mintEdition] Received parameters:`);
      console.log(`      - recipient: ${recipient} (type: ${typeof recipient}, length: ${recipient?.length})`);
      console.log(`      - originalTokenId: ${originalTokenId} (type: ${typeof originalTokenId})`);

      if (!recipient) {
        throw new Error('Recipient address is required');
      }
      if (typeof recipient !== 'string') {
        throw new Error(`Recipient must be a string, got ${typeof recipient}`);
      }
      if (!recipient.startsWith('0x')) {
        throw new Error(`Recipient must start with 0x, got ${recipient}`);
      }
      if (recipient.length !== 42) {
        throw new Error(`Recipient must be 42 characters, got ${recipient.length}`);
      }

      const normalizedRecipient = ethers.getAddress(recipient);
      console.log(`   ✅ Normalized recipient: ${normalizedRecipient}`);

      const deviceAddress = this.deviceWallet.address;
      const isActive = await this.isDeviceActive(deviceAddress);
      
      if (!isActive) {
        throw new Error(`Device ${deviceAddress} is not registered or inactive`);
      }

      console.log(`   📤 Calling contract.mintEdition(${normalizedRecipient}, ${originalTokenId})`);
      const tx = await this.lensMint.mintEdition(normalizedRecipient, originalTokenId);

      console.log(`📸 Minting edition of token ${originalTokenId}`);
      console.log(`   Transaction: ${tx.hash}`);
      console.log(`   Recipient: ${normalizedRecipient}`);

      const receipt = await tx.wait();

      let editionTokenId = null;
      try {
        const editionMintedEvent = receipt.logs.find(log => {
          try {
            const parsed = this.lensMint.interface.parseLog({
              topics: log.topics,
              data: log.data
            });
            return parsed && parsed.name === 'EditionMinted';
          } catch {
            return false;
          }
        });
        
        if (editionMintedEvent) {
          const parsed = this.lensMint.interface.parseLog({
            topics: editionMintedEvent.topics,
            data: editionMintedEvent.data
          });
          editionTokenId = parsed.args.tokenId?.toString();
        }
      } catch (e) {
      }

      return {
        success: true,
        txHash: tx.hash,
        blockNumber: receipt.blockNumber,
        tokenId: editionTokenId || 'edition'
      };
    } catch (error) {
      console.error('❌ Edition minting failed:', error);
      
      if (error.data && error.data.length >= 10) {
        const errorSelector = error.data.slice(0, 10);
        console.error(`   🔍 Error selector: ${errorSelector}`);
        
        if (errorSelector === '0x57f447ce') {
          try {
            console.log(`   🔍 Diagnosing error: Checking token ${originalTokenId} on-chain...`);
            const metadata = await this.lensMint.getTokenMetadata(originalTokenId);
            if (!metadata || metadata.deviceAddress === ethers.ZeroAddress) {
              throw new Error(`Token ${originalTokenId} does not exist on-chain. The original NFT may not have been minted yet, or the token ID stored in the claim is incorrect.`);
            }
            if (!metadata.isOriginal) {
              throw new Error(`Token ${originalTokenId} is not an original token (it's an edition). Cannot mint editions of an edition.`);
            }
            // If we get here, token exists and is original - might be max editions or other issue
            const editionCount = await this.lensMint.getEditionCount(originalTokenId);
            if (metadata.maxEditions > 0n && editionCount >= metadata.maxEditions) {
              throw new Error(`Max editions (${metadata.maxEditions}) reached for token ${originalTokenId}. Current count: ${editionCount}`);
            }
            throw new Error(`Token ${originalTokenId} exists and is valid, but contract reverted. This may be a contract-level issue or the device is not authorized.`);
          } catch (diagnosticError) {
            if (diagnosticError.message && !diagnosticError.message.includes('contract-level issue')) {
              console.error(`   ❌ Diagnosis: ${diagnosticError.message}`);
              throw diagnosticError;
            }
          }
        }
      }
      
      if (error.message && error.message.includes('execution reverted')) {
        const helpfulMsg = `Edition minting failed: Contract reverted for token ${originalTokenId}. ` +
          `Possible reasons: token doesn't exist, token is not an original, max editions reached, or device not authorized. ` +
          `Please verify token ${originalTokenId} exists and is an original token.`;
        console.error(`   ${helpfulMsg}`);
        throw new Error(helpfulMsg);
      }
      
      throw error;
    }
  }

  async getDeviceBalance() {
    if (!this.initialized || !this.deviceWallet) {
      console.log('   ⚠️ Web3 service not initialized or device wallet not set');
      return null;
    }

    try {
      const address = this.deviceWallet.address;
      console.log(`   🔗 Querying balance for: ${address}`);
      const balance = await this.provider.getBalance(address);
      const balanceEth = ethers.formatEther(balance);
      console.log(`   💰 Raw balance (wei): ${balance.toString()}`);
      console.log(`   💰 Formatted balance (ETH): ${balanceEth}`);
      
      return {
        address: address,
        balance: balanceEth,
        balanceWei: balance.toString()
      };
    } catch (error) {
      console.error('   ❌ Error getting device balance:', error.message);
      return null;
    }
  }

  async getTokenMetadata(tokenId) {
    if (!this.initialized) {
      return null;
    }

    try {
      const metadata = await this.lensMint.getTokenMetadata(tokenId);
      return {
        deviceAddress: metadata.deviceAddress,
        deviceId: metadata.deviceId,
        ipfsHash: metadata.ipfsHash,
        imageHash: metadata.imageHash,
        signature: metadata.signature,
        timestamp: metadata.timestamp.toString(),
        maxEditions: metadata.maxEditions.toString(),
        isOriginal: metadata.isOriginal,
        originalTokenId: metadata.originalTokenId.toString()
      };
    } catch (error) {
      console.error('Error getting token metadata:', error);
      return null;
    }
  }

  getDeviceRegistryABI() {
    return [
      "function registerDevice(address _deviceAddress, string memory _publicKey, string memory _deviceId, string memory _cameraId, string memory _model, string memory _firmwareVersion) external",
      "function updateDevice(address _deviceAddress, string memory _firmwareVersion, bool _isActive) external",
      "function deactivateDevice(address _deviceAddress) external",
      "function getDevice(address _deviceAddress) external view returns (tuple(address deviceAddress, string publicKey, string deviceId, string model, string firmwareVersion, uint256 registrationTime, bool isActive, address registeredBy))",
      "function isDeviceActive(address _deviceAddress) external view returns (bool)",
      "function getDeviceByDeviceId(string memory _deviceId) external view returns (address)",
      "event DeviceRegistered(address indexed deviceAddress, string deviceId, string publicKey, address indexed registeredBy)",
      "event DeviceUpdated(address indexed deviceAddress, string deviceId, bool isActive)"
    ];
  }

  getLensMintABI() {
    return [
      "function mintOriginal(address _to, string memory _ipfsHash, bytes32 _imageHash, uint256 _maxEditions, uint8 v, bytes32 r, bytes32 s) external returns (uint256)",
      "function mintEdition(address _to, uint256 _originalTokenId) external returns (uint256)",
      "function getTokenMetadata(uint256 _tokenId) external view returns (tuple(address deviceAddress, string deviceId, string ipfsHash, bytes32 imageHash, bytes signature, uint256 timestamp, uint256 maxEditions, bool isOriginal, uint256 originalTokenId))",
      "function getEditionCount(uint256 _originalTokenId) external view returns (uint256)",
      "function nonces(address) external view returns (uint256)",
      "function domainSeparator() external view returns (bytes32)",
      "function usedImageHashes(bytes32) external view returns (bool)",
      "event TokenMinted(uint256 indexed tokenId, address indexed deviceAddress, string deviceId, string ipfsHash, bool isOriginal)",
      "event EditionMinted(uint256 indexed tokenId, uint256 indexed originalTokenId, address indexed to)"
    ];
  }
}

const web3Service = new Web3Service();

export default web3Service;
