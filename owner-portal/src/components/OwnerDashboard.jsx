import { usePrivy, useWallets } from '@privy-io/react-auth'
import { useAccount } from 'wagmi'
import { useState, useEffect } from 'react'
import { TABS } from '../constants/tabs'
import { useSessionSigner } from '../hooks/useSessionSigner'
import Sidebar from './layout/Sidebar'
import Toast from './ui/Toast'
import Overview from './tabs/Overview'
import MintNFT from './tabs/MintNFT'
import History from './tabs/History'
import Settings from './tabs/Settings'
import './OwnerDashboard.css'

const PRIVY_APP_ID = import.meta.env.VITE_PRIVY_APP_ID || 'your-privy-app-id'

// OwnerDashboard is the top-level shell.
// It handles auth state and passes data down to tab components.
function OwnerDashboard() {
  const { ready, authenticated, login, logout, user } = usePrivy()
  const { wallets } = useWallets()
  const { address, isConnected } = useAccount()

  const [activeTab, setActiveTab] = useState(TABS.OVERVIEW)
  const [txHistory, setTxHistory] = useState([])
  const [toast, setToast] = useState({ message: '', type: '' })

  // Session signer logic extracted into a custom hook
  const {
    sessionSigner,
    signerAddress,
    isSettingUp,
    error: signerError,
    setupSessionSigner,
  } = useSessionSigner(wallets, address, user)

  // Show toast when session signer setup fails
  useEffect(() => {
    if (signerError) {
      setToast({ message: signerError, type: 'error' })
    }
  }, [signerError])

  // Auto-clear toast after 5 seconds
  useEffect(() => {
    if (!toast.message) return
    const timer = setTimeout(() => setToast({ message: '', type: '' }), 5000)
    return () => clearTimeout(timer)
  }, [toast])

  // Auto-initialize session signer once wallet is available
  useEffect(() => {
    if (authenticated && wallets.length > 0 && !sessionSigner) {
      setupSessionSigner()
    }
  }, [authenticated, wallets])

  // Called by MintNFT tab after a successful mint
  const handleMintSuccess = (newTx) => {
    setTxHistory(prev => [newTx, ...prev])
    setToast({ message: 'NFT minted successfully.', type: 'success' })
  }

  // Show spinner while Privy initializes
  if (!ready) {
    return (
      <div className="loading-screen">
        <div className="loading-spinner" />
        <p>Initializing...</p>
      </div>
    )
  }

  // Show login page if not authenticated
  if (!authenticated) {
    return (
      <div className="login-screen">
        <div className="login-card">
          <div className="login-logo">
            <div className="logo-icon" />
            <h1>LensMint</h1>
            <span>Owner Portal</span>
          </div>
          <p>Sign in to manage your LensMint camera system.</p>
          {PRIVY_APP_ID === 'your-privy-app-id' && (
            <div className="alert alert-warning">
              VITE_PRIVY_APP_ID is not configured. Please update your .env file.
            </div>
          )}
          <button
            onClick={login}
            className="btn btn-primary btn-full"
            disabled={PRIVY_APP_ID === 'your-privy-app-id'}
          >
            Sign In
          </button>
        </div>
      </div>
    )
  }

  return (
    <div className="dashboard">

      {/* Sidebar with navigation */}
      <Sidebar
        activeTab={activeTab}
        setActiveTab={setActiveTab}
        user={user}
        isConnected={isConnected}
        onSignOut={logout}
      />

      {/* Main content area */}
      <main className="main-content">

        {/* Global toast notification */}
        <Toast message={toast.message} type={toast.type} />

        {/* Tab rendering — only the active tab mounts */}
        {activeTab === TABS.OVERVIEW && (
          <Overview
            user={user}
            address={address}
            wallets={wallets}
            isConnected={isConnected}
            sessionSigner={sessionSigner}
            signerAddress={signerAddress}
            isSettingUp={isSettingUp}
            txCount={txHistory.length}
            onSetupSigner={setupSessionSigner}
            onNavigate={setActiveTab}
          />
        )}

        {activeTab === TABS.MINT && (
          <MintNFT
            address={address}
            sessionSigner={sessionSigner}
            isSettingUp={isSettingUp}
            onSetupSigner={setupSessionSigner}
            onMintSuccess={handleMintSuccess}
          />
        )}

        {activeTab === TABS.HISTORY && (
          <History
            txHistory={txHistory}
            onNavigate={setActiveTab}
          />
        )}

        {activeTab === TABS.SETTINGS && (
          <Settings
            sessionSigner={sessionSigner}
            signerAddress={signerAddress}
            onSignOut={logout}
          />
        )}

      </main>
    </div>
  )
}

export default OwnerDashboard