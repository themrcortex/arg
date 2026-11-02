declare module '@xterm/addon-fit' {
  import type { Terminal } from '@xterm/xterm'

  export class FitAddon {
    activate(terminal: Terminal): void
    dispose(): void
    fit(): void
    proposeDimensions(): { rows: number; cols: number } | undefined
  }
}
