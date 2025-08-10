import type { DNSSwitcherDependencies } from './types.mts';
import { CLIENT_STATUS } from './util.ts';

export const removeCustomDHCPLeaseForClient = {
  execute:
    (deps: DNSSwitcherDependencies) => async (params: { ip: string }) => {
      const { mikrotikBuilder, logger } = deps;
      const { ip } = params;

      logger.info({ ip }, 'Deleting DNS lease for IP');

      await mikrotikBuilder.execute(async (mikrotik) => {
        const lease = await mikrotik.getDHCPLeaseByIP(ip);

        if (!lease) {
          logger.info({ ip }, 'No DHCP lease found for IP');
          return undefined;
        }

        if (lease.comment !== deps.config.app.comment) {
          logger.info(
            { ip },
            'Lease does not have the expected comment, skipping deletion',
          );

          throw new Error(
            'Lease does not have the expected comment, skipping deletion',
          );
        }

        await mikrotik.removeDHCPLease(lease.id);
        return undefined;
      });

      return {
        ip,
        status: CLIENT_STATUS.DEFAULT,
      };
    },
};
