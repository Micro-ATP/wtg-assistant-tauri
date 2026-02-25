import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'

export interface PartitionInfo {
  drive_letter: string
  label: string
  filesystem: string
  size: number
  free: number
  disk_number: number
  protocol: string
  media_type: string
}

export function usePartitionList() {
  const [partitions, setPartitions] = useState<PartitionInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const load = async () => {
    try {
      setLoading(true)
      const result = await invoke<PartitionInfo[]>('list_partitions')
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
