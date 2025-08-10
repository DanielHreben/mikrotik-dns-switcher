import type { DNSSwitcherDependencies } from './types.mts';
import { getClientStatus } from './util.ts';

export const checkIfClientHasCustomDHCPLease = {
  execute:
    (deps: DNSSwitcherDependencies) => async (params: { ip: string }) => {
      const { mikrotikBuilder, logger } = deps;
      const { ip } = params;

      logger.info({ ip }, 'Getting DNS lease for IP');

      const lease = await mikrotikBuilder.execute(async (mikrotik) => {
        return mikrotik.getDHCPLeaseByIP(ip);
      });

      logger.info({ lease });

      return {
        ip,
        status: getClientStatus(lease, deps.config.app.comment),
      };
    },
};
