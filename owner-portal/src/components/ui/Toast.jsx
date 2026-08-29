// Fixed-position toast notification shown at top-right
// type: 'success' | 'error' | 'warning' | 'info'
export default function Toast({ message, type = 'info' }) {
  if (!message) return null
  return (
    <div className={`toast toast-${type}`}>
      {message}
    </div>
  )
}