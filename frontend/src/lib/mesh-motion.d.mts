export type Point = readonly [number, number]
export function pointOnPolyline(points: readonly Point[], progress: number): { x: number; y: number }
