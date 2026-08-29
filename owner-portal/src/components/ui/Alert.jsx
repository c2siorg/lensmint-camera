// Reusable inline alert component
// type: 'success' | 'error' | 'warning' | 'info'
export default function Alert({ message, type = 'info', children }) {
  if (!message && !children) return null
  return (
    <div className={`alert alert-${type}`}>
      {message || children}
    </div>
  )
}