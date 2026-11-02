export function pointOnPolyline(points, progress) {
  if (!Array.isArray(points) || points.length === 0) return { x: 0, y: 0 }
  if (points.length === 1) return { x: points[0][0], y: points[0][1] }
  const segments = []
  let total = 0
  for (let index = 1; index < points.length; index += 1) {
    const [x1, y1] = points[index - 1]
    const [x2, y2] = points[index]
    const length = Math.hypot(x2 - x1, y2 - y1)
    segments.push({ x1, y1, x2, y2, length })
    total += length
  }
  if (total === 0) return { x: points[0][0], y: points[0][1] }
  let remaining = total * Math.max(0, Math.min(1, Number.isFinite(progress) ? progress : 0))
  for (const segment of segments) {
    if (remaining <= segment.length || segment === segments.at(-1)) {
      const ratio = segment.length === 0 ? 1 : Math.max(0, Math.min(1, remaining / segment.length))
      return {
        x: segment.x1 + (segment.x2 - segment.x1) * ratio,
        y: segment.y1 + (segment.y2 - segment.y1) * ratio,
      }
    }
    remaining -= segment.length
  }
  const [x, y] = points.at(-1)
  return { x, y }
}
