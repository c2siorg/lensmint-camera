# LensMint Owner Portal

Web dashboard for LensMint camera owners to manage 
their device, NFTs, and wallet.

## Tech Stack
- React + Vite
- Privy (wallet authentication)
- Wagmi + Viem (blockchain interaction)
- TanStack Query

## Prerequisites
- Node.js v18+
- A Privy account → https://console.privy.io

## Setup

### 1. Install dependencies
\```bash
npm install --legacy-peer-deps
\```

### 2. Configure environment
\```bash
cp .env.example .env
\```
Edit `.env` and add your Privy App ID from https://console.privy.io

### 3. Start development server
\```bash
npm run dev
\```
Portal runs at http://localhost:3000

## Known Issues
- Use `--legacy-peer-deps` flag during install due to peer 
  dependency conflicts between @privy-io packages
- Disable Solana in Privy console if you see Solana connector warnings