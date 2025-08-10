import type { DNSSwitcherDependencies } from './types.mts';

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
        lease: lease
          ? {
              id: lease.id,
              ip: lease.address,
              mac: lease.macAddress,
              comment: lease.comment,
            }
          : null,
      };
    },
};
