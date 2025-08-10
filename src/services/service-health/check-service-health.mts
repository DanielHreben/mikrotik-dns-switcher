import type { ServiceHealthDependencies } from './types.mts';

export const checkServiceHealth = {
  execute: (deps: ServiceHealthDependencies) => async () => {
    const { logger } = deps;

    logger.info('Health check');

    return Promise.resolve({
      timestamp: new Date().toISOString(),
    });
  },
};
