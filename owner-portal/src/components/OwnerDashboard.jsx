import { usePrivy, useWallets } from '@privy-io/react-auth'
import { useAccount } from 'wagmi'
import { useState, useEffect, useCallback } from 'react'
import axios from 'axios'
import './OwnerDashboard.css'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'http://localhost:5000'
const PRIVY_APP_ID = import.meta.env.VITE_PRIVY_APP_ID || 'your-privy-app-id'

// Validate IPFS CID format (v0 and v1)
const isValidIPFSHash = (hash) => hash?.startsWith('Qm') || hash?.startsWith('bafy')

// Validate 32-byte hex string (image hash)
const isValidHash = (hash) => /^0x[a-fA-F0-9]{64}$/.test(hash)

// Validate 65-byte ECDSA signature
const isValidSignature = (sig) => /^0x[a-fA-F0-9]{130}$/.test(sig)

function OwnerDashboard() {
  const { ready, authenticated, login, logout, user } = usePrivy()
  const { wallets } = useWallets()
  const { address, isConnected } = useAccount()

  // Session signer state — used for gas-sponsored transactions
  const [sessionSigner, setSessionSigner] = useState(null)
  const [signerAddress, setSignerAddress] = useState(null)

  // UI feedback states
  const [status, setStatus] = useState('')
  const [mintStatus, setMintStatus] = useState('')
  const [isMinting, setIsMinting] = useState(false)
  const [isSettingUp, setIsSettingUp] = useState(false)
  const [txHash, setTxHash] = useState(null)

  // Mint form inputs
  const [ipfsHash, setIpfsHash] = useState('')
  const [imageHash, setImageHash] = useState('')
  const [signature, setSignature] = useState('')
  const [maxEditions, setMaxEditions] = useState(10)

  // Auto-clear status messages to keep UI clean
  useEffect(() => {
    if (!status) return
    const timer = setTimeout(() => setStatus(''), 5000)
    return () => clearTimeout(timer)
  }, [status])

  useEffect(() => {
    if (!mintStatus) return
    const timer = setTimeout(() => setMintStatus(''), 8000)
    return () => clearTimeout(timer)
  }, [mintStatus])

  // Auto-initialize session signer once wallet is available
  useEffect(() => {
    if (authenticated && wallets.length > 0 && !sessionSigner) {
      setupSessionSigner()
    }
  }, [authenticated, wallets])

  // Creates a Privy session signer for gas-sponsored minting
  const setupSessionSigner = useCallback(async () => {
    try {
      setIsSettingUp(true)
      setStatus('Setting up session signer...')

      const wallet = wallets.find(w => w.walletClientType === 'privy') || wallets[0]
      if (!wallet) {
        setStatus('No wallet found. Please connect a wallet.')
        return
      }

      const walletAddress = wallet.address || address
      if (!walletAddress) {
        setStatus('Wallet address unavailable. Please reconnect.')
        return
      }

      const response = await axios.post(
        `${BACKEND_URL}/api/privy/create-session-signer`,
        { walletAddress, userId: user?.id || 'unknown' }
      )

      if (response.data.success) {
        setSessionSigner(response.data.sessionSigner)
        setSignerAddress(response.data.signerAddress)
        setStatus('Session signer created successfully.')
      }
    } catch (error) {
      console.error('Error setting up session signer:', error)
      setStatus(`Failed to create session signer: ${error.response?.data?.error || error.message}`)
    } finally {
      setIsSettingUp(false)
    }
  }, [wallets, address, user])

  // Validates inputs and submits mint transaction via backend
  const handleMint = async () => {
    if (!ipfsHash || !imageHash || !signature) {
      setMintStatus('All fields are required.')
      return
    }
    if (!isValidIPFSHash(ipfsHash)) {
      setMintStatus('Invalid IPFS hash. Must start with Qm or bafy.')
      return
    }
    if (!isValidHash(imageHash)) {
      setMintStatus('Invalid image hash. Must be a 0x-prefixed 32-byte hex string.')
      return
    }
    if (!isValidSignature(signature)) {
      setMintStatus('Invalid signature. Must be a 0x-prefixed 65-byte hex string.')
      return
    }
    if (!sessionSigner?.id) {
      setMintStatus('Session signer not initialized. Please wait or refresh.')
      return
    }
    if (!address) {
      setMintStatus('No wallet address detected. Please reconnect your wallet.')
      return
    }

    try {
      setIsMinting(true)
      setTxHash(null)
      setMintStatus('Submitting mint transaction...')

      const response = await axios.post(
        `${BACKEND_URL}/api/privy/mint-with-signer`,
        {
          recipient: address,
          ipfsHash,
          imageHash,
          signature,
          maxEditions,
          sessionSignerId: sessionSigner.id
        }
      )

      if (response.data.success) {
        setTxHash(response.data.txHash)
        setMintStatus('NFT minted successfully.')
        // Reset form after successful mint
        setIpfsHash('')
        setImageHash('')
        setSignature('')
        setMaxEditions(10)
      }
    } catch (error) {
      console.error('Error minting:', error)
      setMintStatus(`Minting failed: ${error.response?.data?.error || error.message}`)
    } finally {
      setIsMinting(false)
    }
  }

  // Show initializing state while Privy loads
  if (!ready) {
    return (
      <div className="container">
        <div className="card">
          <p>Initializing...</p>
        </div>
      </div>
    )
  }

  // Show login screen if user is not authenticated
  if (!authenticated) {
    return (
      <div className="container">
        <div className="card">
          <h1>LensMint Owner Portal</h1>
          <p>Sign in to manage your LensMint camera system.</p>
          {PRIVY_APP_ID === 'your-privy-app-id' && (
            <div className="warning-message">
              VITE_PRIVY_APP_ID is not configured. Please update your .env file.
            </div>
          )}
          <button
            onClick={login}
            className="login-button"
            disabled={PRIVY_APP_ID === 'your-privy-app-id'}
          >
            Sign In
          </button>
        </div>
      </div>
    )
  }

  return (
    <div className="container">
      <div className="card">

        {/* Portal header with sign out */}
        <div className="header">
          <h1>LensMint Owner Portal</h1>
          <button onClick={logout} className="logout-button">
            Sign Out
          </button>
        </div>

        {/* Logged-in user and wallet details */}
        <div className="section">
          <h2>Account</h2>
          <div className="info-grid">
            <div>
              <strong>User ID</strong>
              <span>{user?.id || 'N/A'}</span>
            </div>
            <div>
              <strong>Wallet Address</strong>
              <span>{address || wallets[0]?.address || 'No wallet connected'}</span>
            </div>
            <div>
              <strong>Status</strong>
              <span>{isConnected ? 'Connected' : 'Disconnected'}</span>
            </div>
            <div>
              <strong>Wallets</strong>
              <span>{wallets.length}</span>
            </div>
          </div>
        </div>

        {/* Active session signer info — shown after setup */}
        {sessionSigner && (
          <div className="section">
            <h2>Session Signer</h2>
            <div className="info-grid">
              <div>
                <strong>Signer ID</strong>
                <span>{sessionSigner.id}</span>
              </div>
              <div>
                <strong>Signer Address</strong>
                <span>{signerAddress}</span>
              </div>
            </div>
          </div>
        )}

        {/* Global status feedback — auto clears after 5s */}
        {status && (
          <div className="status-message">{status}</div>
        )}

        {/* Mint actions — setup signer first, then show mint form */}
        <div className="section">
          <h2>Actions</h2>

          {!sessionSigner && (
            <button
              onClick={setupSessionSigner}
              className="action-button"
              disabled={isSettingUp}
            >
              {isSettingUp ? 'Setting up...' : 'Setup Session Signer'}
            </button>
          )}

          {/* NFT mint form — visible only when session signer is ready */}
          {sessionSigner && (
            <div className="mint-form">
              <label className="input-label">
                IPFS Hash
                <input
                  type="text"
                  placeholder="Qm... or bafy..."
                  value={ipfsHash}
                  onChange={(e) => setIpfsHash(e.target.value)}
                  className="input-field"
                  disabled={isMinting}
                />
              </label>

              <label className="input-label">
                Image Hash
                <input
                  type="text"
                  placeholder="0x..."
                  value={imageHash}
                  onChange={(e) => setImageHash(e.target.value)}
                  className="input-field"
                  disabled={isMinting}
                />
              </label>

              <label className="input-label">
                Signature
                <input
                  type="text"
                  placeholder="0x..."
                  value={signature}
                  onChange={(e) => setSignature(e.target.value)}
                  className="input-field"
                  disabled={isMinting}
                />
              </label>

              <label className="input-label">
                Max Editions
                <input
                  type="number"
                  placeholder="10"
                  value={maxEditions}
                  min={1}
                  max={1000}
                  onChange={(e) => setMaxEditions(parseInt(e.target.value) || 10)}
                  className="input-field"
                  disabled={isMinting}
                />
              </label>

              <button
                onClick={handleMint}
                className="action-button primary"
                disabled={isMinting}
              >
                {isMinting ? 'Minting...' : 'Mint NFT'}
              </button>
            </div>
          )}

          {/* Mint status feedback — auto clears after 8s */}
          {mintStatus && (
            <div className="status-message">{mintStatus}</div>
          )}

          {/* Etherscan link shown after successful mint */}
          {txHash && (
            <div className="tx-link">
              <a
                href={`https://sepolia.etherscan.io/tx/${txHash}`}
                target="_blank"
                rel="noopener noreferrer"
              >
                View transaction on Etherscan: {txHash.slice(0, 10)}...{txHash.slice(-6)}
              </a>
            </div>
          )}
        </div>

        {/* Gas sponsorship info — transactions are free for the owner */}
        <div className="section">
          <h2>Gas Sponsorship</h2>
          <p className="info-text">
            Gas fees are automatically sponsored through Privy.
            Transactions will be executed without requiring ETH balance.
          </p>
        </div>

      </div>
    </div>
  )
}

export default OwnerDashboard