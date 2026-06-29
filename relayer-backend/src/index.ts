import express, { Request, Response } from 'express';
import cors from 'cors';
import dotenv from 'dotenv';
import nacl from 'tweetnacl';
import { ethers } from 'ethers';
import { 
    Keypair, TransactionInstruction, PublicKey, 
    TransactionMessage, VersionedTransaction 
} from '@solana/web3.js';
import bs58 from 'bs58';
import dns from 'dns';
import axios from 'axios';

dns.setDefaultResultOrder('ipv4first');
dotenv.config();

const app = express();
app.use(cors());
app.use(express.json());

interface MetadataPayload {
    uuid: string;
    sha256: string;
    phash: string;
    pubkey: string;
    timestamp: number;
    chain: string;
}

interface SignedEnvelope {
    payload_json: string;
    signature: string;
}

const evmRpc = process.env.EVM_RPC_URL ? process.env.EVM_RPC_URL.trim() : '';
const evmKey = process.env.EVM_PRIVATE_KEY ? process.env.EVM_PRIVATE_KEY.trim() : '';
const solRpc = process.env.SOLANA_RPC_URL ? process.env.SOLANA_RPC_URL.trim() : 'https://api.devnet.solana.com';
const solKey = process.env.SOLANA_PRIVATE_KEY ? process.env.SOLANA_PRIVATE_KEY.trim() : '';

const evmProvider = evmRpc ? new ethers.JsonRpcProvider(evmRpc) : null;
const evmWallet = (evmKey && evmProvider) ? new ethers.Wallet(evmKey, evmProvider) : null;
const solanaKeypair = solKey ? Keypair.fromSecretKey(bs58.decode(solKey)) : null;

const SOLANA_MEMO_PROGRAM_ID = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

const HTTP_HEADERS = {
    'Content-Type': 'application/json',
    'Accept': 'application/json',
    'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36'
};

app.post('/api/v1/mint', async (req: Request, res: Response) => {
    try {
        const { payload_json, signature } = req.body as SignedEnvelope;

        if (!payload_json || !signature) {
            return res.status(400).json({ error: 'Missing payload_json or signature' });
        }

        let payload: MetadataPayload;
        try {
            payload = JSON.parse(payload_json);
        } catch (e: any) {
            return res.status(400).json({ error: 'Invalid payload_json format' });
        }

        const { pubkey, chain, sha256, phash, uuid } = payload;

        try {
            const pubKeyBytes = Buffer.from(pubkey, 'hex');
            const sigBytes = Buffer.from(signature, 'hex');
            const msgBytes = Buffer.from(payload_json, 'utf8');

            const isValid = nacl.sign.detached.verify(msgBytes, sigBytes, pubKeyBytes);
            if (!isValid) {
                return res.status(401).json({ error: 'Signature verification failed' });
            }
        } catch (e: any) {
            return res.status(401).json({ error: 'Invalid cryptographic material format' });
        }

        console.log(`\n[Relayer] Auth OK | UUID: ${uuid} | Target: ${chain.toUpperCase()}`);

        const metadataStr = `LensMint|${uuid}|${sha256}|${phash}`;
        let txHash = '';

        if (chain.toLowerCase() === 'evm') {
            if (!evmWallet) {
                return res.status(500).json({ error: 'EVM wallet not configured' });
            }
            
            const txData = ethers.hexlify(ethers.toUtf8Bytes(metadataStr));
            const tx = await evmWallet.sendTransaction({ to: evmWallet.address, value: 0, data: txData });
            txHash = tx.hash;

        } else if (chain.toLowerCase() === 'solana') {
            if (!solanaKeypair) {
                return res.status(500).json({ error: 'Solana keypair not configured' });
            }

            const ix = new TransactionInstruction({
                keys: [{ pubkey: solanaKeypair.publicKey, isSigner: true, isWritable: true }],
                programId: SOLANA_MEMO_PROGRAM_ID,
                data: Buffer.from(metadataStr, 'utf8'),
            });

            console.log(`[Relayer] Fetching Blockhash (Multi-RPC Failover)...`);
            
            const rpcNodes = [
                solRpc,
                'https://api.devnet.solana.com',
                'https://rpc.ankr.com/solana_devnet',
                'https://api.testnet.solana.com'
            ];

            let recentBlockhash = '';
            let successfulRpc = '';

            for (const rpc of rpcNodes) {
                try {
                    const cleanRpc = rpc.replace(/^SOLANA_RPC_URL=/, '').trim();
                    console.log(`[Relayer] Attempting RPC: ${cleanRpc}`);
                    
                    const { data } = await axios.post(cleanRpc, {
                        jsonrpc: '2.0', id: 1, 
                        method: 'getLatestBlockhash', 
                        params: [{ commitment: 'confirmed' }]
                    }, { headers: HTTP_HEADERS, timeout: 5000 });

                    const parsedData = Array.isArray(data) ? data[0] : data;
                    
                    if (parsedData?.result?.value?.blockhash) {
                        recentBlockhash = parsedData.result.value.blockhash;
                        successfulRpc = cleanRpc;
                        console.log(`[Relayer] Blockhash acquired: ${recentBlockhash.substring(0, 10)}...`);
                        break;
                    } else {
                        console.log(`[Relayer] Invalid RPC response format (potential WAF block). Failing over...`);
                    }
                } catch (e: any) {
                    console.log(`[Relayer] RPC connection failed or timed out. Failing over...`);
                }
            }

            if (!recentBlockhash) {
                return res.status(502).json({ error: 'All Solana RPC nodes failed to respond' });
            }

            const messageV0 = new TransactionMessage({
                payerKey: solanaKeypair.publicKey,
                recentBlockhash: recentBlockhash,
                instructions: [ix],
            }).compileToV0Message();

            const transaction = new VersionedTransaction(messageV0);
            transaction.sign([solanaKeypair]);
            
            console.log(`[Relayer] Broadcasting transaction via [${successfulRpc}]...`);

            const serializedTx = Buffer.from(transaction.serialize()).toString('base64');
            try {
                const { data: sendData } = await axios.post(successfulRpc, {
                    jsonrpc: '2.0', id: 2, 
                    method: 'sendTransaction', 
                    params: [serializedTx, { 
                        encoding: 'base64', 
                        skipPreflight: true, 
                        maxRetries: 3 
                    }]
                }, { headers: HTTP_HEADERS, timeout: 10000 });

                const parsedSend = Array.isArray(sendData) ? sendData[0] : sendData;
                
                if (parsedSend.error) throw new Error(parsedSend.error.message);
                txHash = parsedSend.result;
            } catch (e: any) {
                console.error(`[Relayer] SendTx Error:`, e.message);
                return res.status(502).json({ error: `SendTx error: ${e.message}` });
            }
            
        } else {
            return res.status(400).json({ error: `Unsupported chain: ${chain}` });
        }

        console.log(`[Relayer] Tx Broadcasted | Hash: ${txHash}`);
        return res.json({ tx_hash: txHash });

    } catch (error: any) {
        console.error('[Relayer] Transaction failed:', error.message);
        return res.status(500).json({ error: 'Internal server error', details: error.message });
    }
});

const PORT = process.env.PORT ? parseInt(process.env.PORT, 10) : 3000;
app.listen(PORT, '0.0.0.0', () => {
    console.log(`[Relayer] Daemon listening on 0.0.0.0:${PORT}`);
    console.log(`[Relayer] EVM Enabled: ${!!evmWallet}`);
    console.log(`[Relayer] Solana Enabled: ${!!solanaKeypair}`);
});