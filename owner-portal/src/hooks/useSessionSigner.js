import { useState, useCallback } from 'react'
import axios from 'axios'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'http://localhost:5000'

// Custom hook that encapsulates all session signer logic
export function useSessionSigner(wallets, address, user) {
  const [sessionSigner, setSessionSigner] = useState(null)
  const [signerAddress, setSignerAddress] = useState(null)
  const [isSettingUp, setIsSettingUp] = useState(false)
  const [error, setError] = useState('')

  // Creates a Privy session signer for gas-sponsored minting
  const setupSessionSigner = useCallback(async () => {
    try {
      setIsSettingUp(true)
      setError('')

      const wallet = wallets.find(w => w.walletClientType === 'privy') || wallets[0]
      if (!wallet) {
        setError('No wallet found. Please connect a wallet.')
        return
      }

      const walletAddress = wallet.address || address
      if (!walletAddress) {
        setError('Wallet address unavailable. Please reconnect.')
        return
      }

      const response = await axios.post(
        `${BACKEND_URL}/api/privy/create-session-signer`,
        { walletAddress, userId: user?.id || 'unknown' }
      )

      if (response.data.success) {
        setSessionSigner(response.data.sessionSigner)
        setSignerAddress(response.data.signerAddress)
      }
    } catch (err) {
      console.error('Error setting up session signer:', err)
      setError(`Failed to create session signer: ${err.response?.data?.error || err.message}`)
    } finally {
      setIsSettingUp(false)
    }
  }, [wallets, address, user])

  return {
    sessionSigner,
    signerAddress,
    isSettingUp,
    error,
    setupSessionSigner,
  }
}