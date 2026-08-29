// Reusable stat card used in the Overview tab
export default function StatCard({ label, value, valueClass }) {
  return (
    <div className="stat-card">
      <span className="stat-label">{label}</span>
      <span className={`stat-value ${valueClass || ''}`}>{value}</span>
    </div>
  )
}