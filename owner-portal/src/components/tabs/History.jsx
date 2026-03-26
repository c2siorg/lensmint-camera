import { TABS } from '../../constants/tabs'
import { truncate } from '../../utils/validators'

// History tab — shows all minted NFT transactions for the current session
export default function History({ txHistory, onNavigate }) {
  return (
    <div className="tab-content">
      <div className="page-header">
        <h1>Transaction History</h1>
        <p>NFTs minted during this session.</p>
      </div>

      <div className="content-card">
        {txHistory.length === 0 ? (
          // Empty state when no transactions exist yet
          <div className="empty-state">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
              <circle cx="12" cy="12" r="10" />
              <polyline points="12,6 12,12 16,14" />
            </svg>
            <p>No transactions yet.</p>
            <button className="btn btn-primary" onClick={() => onNavigate(TABS.MINT)}>
              Mint your first NFT
            </button>
          </div>
        ) : (
          <div className="tx-table">
            <div className="tx-table-header">
              <span>Transaction</span>
              <span>IPFS Hash</span>
              <span>Editions</span>
              <span>Time</span>
              <span>Status</span>
            </div>
            {txHistory.map((tx, i) => (
              <div key={i} className="tx-table-row">
                <span className="mono">
                  <a
                    href={`https://sepolia.etherscan.io/tx/${tx.txHash}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="tx-link"
                  >
                    {truncate(tx.txHash, 8, 6)}
                  </a>
                </span>
                <span className="mono">{truncate(tx.ipfsHash, 8, 6)}</span>
                <span>{tx.maxEditions}</span>
                <span>{new Date(tx.timestamp).toLocaleTimeString()}</span>
                <span className={`status-pill status-${tx.status}`}>
                  {tx.status}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}