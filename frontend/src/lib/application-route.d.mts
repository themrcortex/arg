export type ApplicationSection = 'deploy' | 'launch' | 'runtime' | 'resources' | 'integrations'
export type ApplicationRoute = { appId: string | null; serverUuid: string | null; section: ApplicationSection }
export function parseApplicationRoute(hash: string): ApplicationRoute
export function buildApplicationRoute(route: { appId: string | null; serverUuid: string | null; section?: ApplicationSection }): string
