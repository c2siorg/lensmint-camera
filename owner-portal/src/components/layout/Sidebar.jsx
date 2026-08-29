import { TABS } from '../../constants/tabs'
import { truncate } from '../../utils/validators'
import './Sidebar.css'

// Sidebar navigation with user info and sign out button
export default function Sidebar({ activeTab, setActiveTab, user, isConnected, onSignOut }) {
  return (
    <aside className="sidebar">

      {/* Logo */}
      <div className="sidebar-logo">
        <div className="logo-icon" />
        <span>LensMint</span>
      </div>

      {/* Navigation links */}
      <nav className="sidebar-nav">
        <button
          className={`nav-item ${activeTab === TABS.OVERVIEW ? 'active' : ''}`}
          onClick={() => setActiveTab(TABS.OVERVIEW)}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <rect x="3" y="3" width="7" height="7" rx="1" />
            <rect x="14" y="3" width="7" height="7" rx="1" />
            <rect x="3" y="14" width="7" height="7" rx="1" />
            <rect x="14" y="14" width="7" height="7" rx="1" />
          </svg>
          <span>Overview</span>
        </button>

        <button
          className={`nav-item ${activeTab === TABS.MINT ? 'active' : ''}`}
          onClick={() => setActiveTab(TABS.MINT)}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="16" />
            <line x1="8" y1="12" x2="16" y2="12" />
          </svg>
          <span>Mint NFT</span>
        </button>

        <button
          className={`nav-item ${activeTab === TABS.HISTORY ? 'active' : ''}`}
          onClick={() => setActiveTab(TABS.HISTORY)}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="10" />
            <polyline points="12,6 12,12 16,14" />
          </svg>
          <span>History</span>
        </button>

        <button
          className={`nav-item ${activeTab === TABS.SETTINGS ? 'active' : ''}`}
          onClick={() => setActiveTab(TABS.SETTINGS)}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="3" />
            <path d="M12 2v3M12 19v3M4.22 4.22l2.12 2.12M17.66 17.66l2.12 2.12M2 12h3M19 12h3M4.22 19.78l2.12-2.12M17.66 6.34l2.12-2.12" />
          </svg>
          <span>Settings</span>
        </button>
      </nav>

      {/* User info and sign out */}
      <div className="sidebar-footer">
        <div className="user-info">
          <div className="user-avatar">
            {user?.email?.address?.[0]?.toUpperCase() || 'U'}
          </div>
          <div className="user-details">
            <span className="user-name">
              {user?.email?.address ? truncate(user.email.address, 10, 4) : 'Owner'}
            </span>
            <span className={`connection-badge ${isConnected ? 'connected' : 'disconnected'}`}>
              {isConnected ? 'Connected' : 'Disconnected'}
            </span>
          </div>
        </div>
        <button onClick={onSignOut} className="btn-signout">Sign Out</button>
      </div>

    </aside>
  )
}