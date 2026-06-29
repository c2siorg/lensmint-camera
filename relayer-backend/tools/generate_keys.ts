import { ethers } from 'ethers';
import { Keypair } from '@solana/web3.js';
import bs58 from 'bs58';

// 1. Generate EVM (Sepolia) Burner Wallet
const evmWallet = ethers.Wallet.createRandom();
console.log("================ EVM (Sepolia) ================");
console.log("Address (For Faucet):", evmWallet.address);
console.log("Private Key (For .env):", evmWallet.privateKey);
console.log("");

// 2. Generate Solana (Devnet) Burner Wallet
const solanaKeypair = Keypair.generate();
console.log("================ Solana (Devnet) ==============");
console.log("Address (For Faucet):", solanaKeypair.publicKey.toBase58());
console.log("Private Key (For .env):", bs58.encode(solanaKeypair.secretKey));
console.log("===============================================");
