import { useEffect, useState } from 'react'
import { diskApi } from '@/services/api'
import type { DiskInfo } from '@/types'

export function useDiskList() {
  const [disks, setDisks] = useState<DiskInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const loadDisks = async () => {
      try {
        setLoading(true)
        const result = await diskApi.listDisks()
        setDisks(result)
        setError(null)
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load disks')
      } finally {
        setLoading(false)
      }
    }

    loadDisks()
    // Refresh disk list every 3 seconds
    const interval = setInterval(loadDisks, 3000)
    return () => clearInterval(interval)
  }, [])

  const refetch = async () => {
    try {
      setLoading(true)
      const result = await diskApi.listDisks()
      setDisks(result)
      setError(null)
      return result
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load disks')
      throw err
    } finally {
      setLoading(false)
    }
  }

  return { disks, loading, error, refetch }
}
