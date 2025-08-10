import pino from 'pino';

export function createLogger() {
  return pino();
}

export type Logger = Pick<
  ReturnType<typeof createLogger>,
  'info' | 'error' | 'warn' | 'debug'
>;
