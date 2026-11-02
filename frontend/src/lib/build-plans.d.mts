export function latestBuildPlans<T extends {
  release_id: string
  subdirectory?: string | null
}>(plans: T[]): T[]
