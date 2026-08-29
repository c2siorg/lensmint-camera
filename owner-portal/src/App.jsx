import { PrivyProvider } from '@privy-io/react-auth'
import { createConfig, WagmiProvider } from '@privy-io/wagmi'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { http } from 'viem'
import { sepolia } from 'viem/chains'
import OwnerDashboard from './components/OwnerDashboard'

const PRIVY_APP_ID = import.meta.env.VITE_PRIVY_APP_ID || 'your-privy-app-id'

const queryClient = new QueryClient()

const wagmiConfig = createConfig({
  chains: [sepolia],
  transports: {
    [sepolia.id]: http(),
  },
})

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <PrivyProvider
        appId={PRIVY_APP_ID}
        config={{
          loginMethods: ['wallet', 'email'],
          appearance: {
            theme: 'light',
            accentColor: '#667eea',
          },
          embeddedWallets: {
            createOnLogin: 'users-without-wallets',
          },
        }}
      >
        <WagmiProvider config={wagmiConfig}>
          <OwnerDashboard />
        </WagmiProvider>
      </PrivyProvider>
    </QueryClientProvider>
  )
}

export default App