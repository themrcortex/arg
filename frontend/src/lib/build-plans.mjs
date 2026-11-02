/** Keep only the newest analysis for each release/project root. The API orders
 * newest rows first; array position breaks second-resolution timestamp ties. */
export function latestBuildPlans(plans) {
  const latest = new Map()
  for (const plan of plans || []) {
    const key = `${plan.release_id}\0${plan.subdirectory || ''}`
    if (!latest.has(key)) latest.set(key, plan)
  }
  return [...latest.values()]
}
