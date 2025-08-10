import type { DNSSwitcherDependencies } from './types.mts';

export const removeCustomDHCPLeaseForClient = {
  execute:
    (deps: DNSSwitcherDependencies) => async (params: { ip: string }) => {
      const { mikrotikBuilder, logger } = deps;
      const { ip } = params;

      logger.info({ ip }, 'Deleting DNS lease for IP');

      const result = await mikrotikBuilder.execute(async (mikrotik) => {
        const lease = await mikrotik.getDHCPLeaseByIP(ip);

        if (!lease) {
          logger.info({ ip }, 'No DHCP lease found for IP');
          return undefined;
        }

        return await mikrotik.removeDHCPLease(lease.id);
      });

      return {
        ip,
        lease: result
          ? {
              id: result.id,
              ip: result.address,
              mac: result.macAddress,
              comment: result.comment,
            }
          : null,
      };
    },
};
