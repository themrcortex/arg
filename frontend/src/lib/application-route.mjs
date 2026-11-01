const sections = new Set(['deploy', 'runtime', 'resources', 'integrations'])

export function parseApplicationRoute(hash) {
  const query = String(hash).split('?')[1] || ''
  const params = new URLSearchParams(query)
  const requestedSection = params.get('section')
  return {
    appId: params.get('app'),
    serverUuid: params.get('server'),
    section: sections.has(requestedSection) ? requestedSection : 'deploy',
  }
}

export function buildApplicationRoute({ appId, serverUuid, section = 'deploy' }) {
  if (!appId || !serverUuid) return '#/apps'
  const params = new URLSearchParams({ app: appId, server: serverUuid })
  if (sections.has(section) && section !== 'deploy') params.set('section', section)
  return `#/apps?${params.toString()}`
}
