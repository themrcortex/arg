import { useEffect, useState } from 'react'
import { pointOnPolyline, type Point } from '../lib/mesh-motion.mjs'

export function MeshPacket({
  points,
  startedAt,
  durationMs,
  color,
  radius = 5,
  filter,
  title,
}: {
  points: readonly Point[]
  startedAt: number
  durationMs: number
  color: string
  radius?: number
  filter?: string
  title: string
}) {
  const [position, setPosition] = useState(() => pointOnPolyline(points, 0))
  const pointKey = points.map(([x, y]) => `${x},${y}`).join(';')

  useEffect(() => {
    let frame = 0
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    if (reduced) {
      setPosition(pointOnPolyline(points, 1))
      return
    }
    const tick = () => {
      const progress = Math.max(0, Math.min(1, (Date.now() - startedAt) / Math.max(1, durationMs)))
      setPosition(pointOnPolyline(points, progress))
      if (progress < 1) frame = window.requestAnimationFrame(tick)
    }
    frame = window.requestAnimationFrame(tick)
    return () => window.cancelAnimationFrame(frame)
  }, [pointKey, startedAt, durationMs])

  return <circle cx={position.x} cy={position.y} r={radius} fill={color} filter={filter} className="arg-topology-packet"><title>{title}</title></circle>
}
