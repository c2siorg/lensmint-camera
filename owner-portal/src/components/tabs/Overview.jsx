import { TABS } from '../../constants/tabs'
import { truncate } from '../../utils/validators'
import StatCard from '../ui/StatCard'

// Overview tab — shows account summary, stats and quick actions
export default function Overview({
  user,
  address,
  wallets,
  isConnected,
  sessionSigner,
  signerAddress,
  isSettingUp,
  txCount,
  onSetupSigner,
  onNavigate,
}) {
  return (
    <div className="tab-content">
      <div className="page-header">
        <h1>Overview</h1>
        <p>Your LensMint camera system at a glance.</p>
      </div>

      {/* Stats row */}
      <div className="stats-grid">
        <StatCard
          label="Wallet Status"
          value={isConnected ? 'Connected' : 'Disconnected'}
          valueClass={isConnected ? 'text-success' : 'text-error'}
        />
        <StatCard
          label="Total Mints"
          value={txCount}
        />
        <StatCard
          label="Session Signer"
          value={sessionSigner ? 'Active' : isSettingUp ? 'Setting up...' : 'Inactive'}
          valueClass={sessionSigner ? 'text-success' : 'text-warning'}
        />
        <StatCard
          label="Gas Sponsorship"
          value="Enabled"
          valueClass="text-success"
        />
      </div>

      {/* Account details */}
      <div className="content-card">
        <h2>Account Details</h2>
        <div className="detail-list">
          <div className="detail-row">
            <span className="detail-label">User ID</span>
            <span className="detail-value mono">{truncate(user?.id || '', 20, 6)}</span>
          </div>
          <div className="detail-row">
            <span className="detail-label">Wallet Address</span>
            <span className="detail-value mono">
              {address || wallets[0]?.address
                ? truncate(address || wallets[0]?.address, 8, 6)
                : 'No wallet connected'}
            </span>
            {(address || wallets[0]?.address) && (
              <a
                href={`https://sepolia.etherscan.io/address/${address || wallets[0]?.address}`}
                target="_blank"
                rel="noopener noreferrer"
                className="link-external"
              >
                View on Etherscan
              </a>
            )}
          </div>
          <div className="detail-row">
            <span className="detail-label">Connected Wallets</span>
            <span className="detail-value">{wallets.length}</span>
          </div>
        </div>
      </div>

      {/* Session signer details — shown only when active */}
      {sessionSigner && (
        <div className="content-card">
          <h2>Session Signer</h2>
          <div className="detail-list">
            <div className="detail-row">
              <span className="detail-label">Signer ID</span>
              <span className="detail-value mono">{truncate(sessionSigner.id, 12, 6)}</span>
            </div>
            <div className="detail-row">
              <span className="detail-label">Signer Address</span>
              <span className="detail-value mono">{truncate(signerAddress, 8, 6)}</span>
            </div>
          </div>
        </div>
      )}

      {/* Quick actions */}
      <div className="content-card">
        <h2>Quick Actions</h2>
        <div className="quick-actions">
          <button className="btn btn-primary" onClick={() => onNavigate(TABS.MINT)}>
            Mint New NFT
          </button>
          {!sessionSigner && (
            <button
              className="btn btn-secondary"
              onClick={onSetupSigner}
              disabled={isSettingUp}
            >
              {isSettingUp ? 'Setting up...' : 'Setup Session Signer'}
            </button>
          )}
        </div>
      </div>
    </div>
  )
}