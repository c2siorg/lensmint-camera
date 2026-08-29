import { useState } from 'react'
import axios from 'axios'
import { isValidIPFSHash, isValidHash, isValidSignature } from '../../utils/validators'
import Alert from '../ui/Alert'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'http://localhost:5000'

// Mint NFT tab — form with validation and submission logic
export default function MintNFT({
  address,
  sessionSigner,
  isSettingUp,
  onSetupSigner,
  onMintSuccess,
}) {
  const [ipfsHash, setIpfsHash]     = useState('')
  const [imageHash, setImageHash]   = useState('')
  const [signature, setSignature]   = useState('')
  const [maxEditions, setMaxEditions] = useState(10)
  const [isMinting, setIsMinting]   = useState(false)
  const [mintStatus, setMintStatus] = useState({ message: '', type: '' })

  const clearForm = () => {
    setIpfsHash('')
    setImageHash('')
    setSignature('')
    setMaxEditions(10)
    setMintStatus({ message: '', type: '' })
  }

  // Validates all fields and submits mint transaction
  const handleMint = async () => {
    if (!ipfsHash || !imageHash || !signature) {
      setMintStatus({ message: 'All fields are required.', type: 'error' })
      return
    }
    if (!isValidIPFSHash(ipfsHash)) {
      setMintStatus({ message: 'Invalid IPFS hash. Must start with Qm or bafy.', type: 'error' })
      return
    }
    if (!isValidHash(imageHash)) {
      setMintStatus({ message: 'Invalid image hash. Must be a 0x-prefixed 32-byte hex string.', type: 'error' })
      return
    }
    if (!isValidSignature(signature)) {
      setMintStatus({ message: 'Invalid signature. Must be a 0x-prefixed 65-byte hex string.', type: 'error' })
      return
    }
    if (!sessionSigner?.id) {
      setMintStatus({ message: 'Session signer not initialized. Please wait or refresh.', type: 'error' })
      return
    }
    if (!address) {
      setMintStatus({ message: 'No wallet address detected. Please reconnect your wallet.', type: 'error' })
      return
    }

    try {
      setIsMinting(true)
      setMintStatus({ message: 'Submitting mint transaction...', type: 'info' })

      const response = await axios.post(
        `${BACKEND_URL}/api/privy/mint-with-signer`,
        {
          recipient: address,
          ipfsHash,
          imageHash,
          signature,
          maxEditions,
          sessionSignerId: sessionSigner.id,
        }
      )

      if (response.data.success) {
        // Notify parent with new transaction data
        onMintSuccess({
          txHash: response.data.txHash,
          ipfsHash,
          maxEditions,
          timestamp: new Date().toISOString(),
          status: 'success',
        })
        setMintStatus({ message: 'NFT minted successfully.', type: 'success' })
        clearForm()
      }
    } catch (error) {
      console.error('Error minting:', error)
      setMintStatus({
        message: `Minting failed: ${error.response?.data?.error || error.message}`,
        type: 'error',
      })
    } finally {
      setIsMinting(false)
    }
  }

  return (
    <div className="tab-content">
      <div className="page-header">
        <h1>Mint NFT</h1>
        <p>Create a new NFT from a captured LensMint photo.</p>
      </div>

      {/* Session signer warning */}
      {!sessionSigner && (
        <Alert type="warning">
          Session signer is not active.{' '}
          <button
            className="btn btn-sm btn-secondary"
            onClick={onSetupSigner}
            disabled={isSettingUp}
          >
            {isSettingUp ? 'Setting up...' : 'Setup Now'}
          </button>
        </Alert>
      )}

      <div className="content-card">
        <h2>NFT Details</h2>

        <div className="form-group">
          <label className="form-label">
            IPFS Hash
            <span className="form-hint">Content identifier from Filecoin storage</span>
          </label>
          <input
            type="text"
            placeholder="Qm... or bafy..."
            value={ipfsHash}
            onChange={(e) => setIpfsHash(e.target.value)}
            className={`form-input ${ipfsHash && !isValidIPFSHash(ipfsHash) ? 'input-error' : ''}`}
            disabled={isMinting}
          />
          {ipfsHash && !isValidIPFSHash(ipfsHash) && (
            <span className="field-error">Must start with Qm or bafy</span>
          )}
        </div>

        <div className="form-group">
          <label className="form-label">
            Image Hash
            <span className="form-hint">SHA-256 hash of the captured image</span>
          </label>
          <input
            type="text"
            placeholder="0x..."
            value={imageHash}
            onChange={(e) => setImageHash(e.target.value)}
            className={`form-input ${imageHash && !isValidHash(imageHash) ? 'input-error' : ''}`}
            disabled={isMinting}
          />
          {imageHash && !isValidHash(imageHash) && (
            <span className="field-error">Must be a 0x-prefixed 32-byte hex string</span>
          )}
        </div>

        <div className="form-group">
          <label className="form-label">
            Device Signature
            <span className="form-hint">ECDSA signature from the camera hardware key</span>
          </label>
          <input
            type="text"
            placeholder="0x..."
            value={signature}
            onChange={(e) => setSignature(e.target.value)}
            className={`form-input ${signature && !isValidSignature(signature) ? 'input-error' : ''}`}
            disabled={isMinting}
          />
          {signature && !isValidSignature(signature) && (
            <span className="field-error">Must be a 0x-prefixed 65-byte hex string</span>
          )}
        </div>

        <div className="form-group">
          <label className="form-label">
            Max Editions
            <span className="form-hint">Maximum number of NFT copies that can be claimed</span>
          </label>
          <input
            type="number"
            placeholder="10"
            value={maxEditions}
            min={1}
            max={1000}
            onChange={(e) => setMaxEditions(parseInt(e.target.value) || 10)}
            className="form-input form-input-sm"
            disabled={isMinting}
          />
        </div>

        {/* Mint status feedback */}
        {mintStatus.message && (
          <Alert message={mintStatus.message} type={mintStatus.type} />
        )}

        <div className="form-actions">
          <button
            onClick={handleMint}
            className="btn btn-primary"
            disabled={isMinting || !sessionSigner}
          >
            {isMinting ? 'Minting...' : 'Mint NFT'}
          </button>
          <button
            className="btn btn-ghost"
            onClick={clearForm}
            disabled={isMinting}
          >
            Clear
          </button>
        </div>
      </div>

      {/* Gas sponsorship info */}
      <div className="content-card info-card">
        <h2>Gas Sponsorship</h2>
        <p>
          Gas fees are automatically sponsored through Privy.
          Transactions will be executed without requiring ETH balance.
        </p>
      </div>
    </div>
  )
}