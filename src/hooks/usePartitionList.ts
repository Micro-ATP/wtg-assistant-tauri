import { useEffect, useState } from 'react'
import { toolsApi } from '@/services/api'
import type { PartitionInfo } from '@/types'

export function usePartitionList() {
  const [partitions, setPartitions] = useState<PartitionInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const load = async () => {
    try {
      setLoading(true)
      const result = await toolsApi.listPartitions()
      setPartitions(result)
      setError(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load partitions')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
  }, [])

  return { partitions, loading, error, refetch: load }
}
