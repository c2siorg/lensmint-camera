// Validate IPFS CID format (v0 starts with Qm, v1 starts with bafy)
export const isValidIPFSHash = (hash) =>
  hash?.startsWith('Qm') || hash?.startsWith('bafy')

// Validate 32-byte hex string (image hash from SHA-256)
export const isValidHash = (hash) =>
  /^0x[a-fA-F0-9]{64}$/.test(hash)

// Validate 65-byte ECDSA signature from camera hardware key
export const isValidSignature = (sig) =>
  /^0x[a-fA-F0-9]{130}$/.test(sig)

// Truncate a long address or hash for display
export const truncate = (str, start = 6, end = 4) =>
  str ? `${str.slice(0, start)}...${str.slice(-end)}` : 'N/A'