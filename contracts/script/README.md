# LensMint Camera - Deployment and Utility Scripts Guide

This directory contains automation scripts for deploying and interacting with the LensMint Camera smart contracts. All scripts utilize the Foundry framework for secure and efficient blockchain interactions.

## Multichain Deployment Pipeline

The core deployment tool is the `deployMultichain.sh` shell script. It orchestrates deployments across multiple EVM-compatible chains while ensuring security by leveraging Foundry Keystores rather than plaintext private keys in .env files.

### 1. Secure Keystore Setup

Before running deployments, you should import your private key into a Foundry Keystore. This encrypts the key on your local machine and requires a password to unlock during script execution.

```bash
# Import your private key into a named account
cast wallet import <account_name> --interactive
```

### 2. Network Configuration

The deployment script references environment variables for RPC URLs. Configure these in your `contracts/.env` file following the standard naming convention:

```env
# Pattern: <NETWORK_ID>_RPC_URL
SEPOLIA_RPC_URL=https://eth-sepolia.g.alchemy.com/v2/YOUR_API_KEY
POLYGON_AMOY_RPC_URL=https://polygon-amoy.g.alchemy.com/v2/YOUR_API_KEY

# Explorer API keys for contract verification
ETHERSCAN_API_KEY=YOUR_ETHERSCAN_KEY
```

### 3. Executing Deployment

You can deploy to single or multiple networks using the multichain script. The --account flag corresponds to the name you provided during the keystore import.

```bash
# Deploy to a single network
./script/deployMultichain.sh --networks sepolia --account <account_name>

# Deploy to multiple networks
./script/deployMultichain.sh --networks sepolia,polygon_amoy --account <account_name>

# Deploy core contracts along with the LensMint Verifier
./script/deployMultichain.sh --networks sepolia --account <account_name> --deploy-verifier
```

---

## Script Reference

| Script | Functionality | Usage Example |
| :--- | :--- | :--- |
| deployMultichain.sh | Orchestrates deployment across multiple chains | --networks, --account |
| Deploy.s.sol | Standard deployment for DeviceRegistry and LensMintERC1155 | forge script script/Deploy.s.sol |
| DeployVerifier.s.sol | Deploys the verification logic contracts | forge script script/DeployVerifier.s.sol |
| DeployAndVerify.s.sol | Deployment script with integrated manual verification | Inherits from Deploy.s.sol |
| SubmitProof.s.sol | Utility for submitting ZK-proofs for verification | forge script script/SubmitProof.s.sol |
| RegisterDevice.s.sol | Manually registers a device in the central registry | forge script script/RegisterDevice.s.sol |

---

## Utility Scripts Configuration

Utility scripts such as `RegisterDevice.s.sol` and `SubmitProof.s.sol` depend on specific environment variables for execution.

### RegisterDevice.s.sol
This script enables the contract owner to manually onboard new devices.
- DEVICE_REGISTRY_ADDRESS: Address of the deployed registry.
- DEVICE_ADDRESS: The unique address assigned to the hardware device.
- DEVICE_PUBLIC_KEY: The public key associated with the device's secure enclave.
- DEVICE_ID, CAMERA_ID, DEVICE_MODEL, FIRMWARE_VERSION: Metadata for registration.

### SubmitProof.s.sol
This script submits a ZK-proof for verification by the LensMintVerifier contract.
- VERIFIER_CONTRACT_ADDRESS: Address of the deployed verifier.
- PROOF_FILE: Filesystem path to the JSON proof artifact.

---

## Customization and Best Practices

### Adding New Networks
To support a new chain, add the corresponding <NETWORK>_RPC_URL to your .env file and include the network name in the --networks argument of the multichain script.

### Passing Extra Forge Flags
The `deployMultichain.sh` script passes additional arguments directly to the underlying forge command. This allows you to use flags like:
- --verify: Automates contract verification on block explorers.
- --slow: Useful for networks with high congestion to prevent nonce issues.

### Security and Logs
- Ensure private keys are never committed to version control.
- Foundry creates deployment logs in the `broadcast/` directory. These are useful for audit trails but should be reviewed before sharing as they may contain environment-specific metadata.
