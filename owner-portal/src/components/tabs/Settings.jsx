import { truncate } from '../../utils/validators'

const BACKEND_URL = import.meta.env.VITE_BACKEND_URL || 'http://localhost:5000'

// Settings tab — shows session config and sign out option
export default function Settings({ sessionSigner, signerAddress, onSignOut }) {
  return (
    <div className="tab-content">
      <div className="page-header">
        <h1>Settings</h1>
        <p>Manage your portal configuration.</p>
      </div>

      {/* Session configuration details */}
      <div className="content-card">
        <h2>Session</h2>
        <div className="detail-list">
          <div className="detail-row">
            <span className="detail-label">Backend URL</span>
            <span className="detail-value mono">{BACKEND_URL}</span>
          </div>
          <div className="detail-row">
            <span className="detail-label">Network</span>
            <span className="detail-value">Ethereum Sepolia (Testnet)</span>
          </div>
          <div className="detail-row">
            <span className="detail-label">Session Signer</span>
            <span className={`detail-value ${sessionSigner ? 'text-success' : 'text-warning'}`}>
              {sessionSigner
                ? `Active — ${truncate(signerAddress, 8, 6)}`
                : 'Not initialized'}
            </span>
          </div>
        </div>
      </div>

      {/* Danger zone — sign out */}
      <div className="content-card">
        <h2>Danger Zone</h2>
        <p className="text-muted">
          Signing out will end your current session. You will need to sign in again to mint NFTs.
        </p>
        <button onClick={onSignOut} className="btn btn-danger">
          Sign Out
        </button>
      </div>
    </div>
  )
}