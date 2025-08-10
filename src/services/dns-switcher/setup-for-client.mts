import type { MikrotikInteractor } from '../../interactors/mikrotik/interactor.mts';
import type { DNSSwitcherDependencies } from './types.mts';
import { CLIENT_STATUS } from './util.ts';

// DHCP utility function for creating DNS service

async function findOrCreateDHCPOption(
  mikrotik: MikrotikInteractor,
  config: DNSSwitcherDependencies['config'],
  logger: DNSSwitcherDependencies['logger'],
) {
  const name = 'Custom DNS Server';
  const code = '6'; // DHCP option code for DNS servers
  const value =
    '0x' +
    config.app.customDns
      .split('.')
      .map((octet) => Number.parseInt(octet, 10).toString(16).padStart(2, '0'))
      .join('');

  logger.info({ name, code, value });
  const existingOption = await mikrotik.getDHCPOptionByName(name);
  if (existingOption) {
    return existingOption;
  }

  const createdOption = await mikrotik.createDHCPOption({
    name,
    code,
    value,
    comment: config.app.comment,
  });

  return createdOption;
}

export const setupDHCPLeaseWithCustomDNSForClient = {
  execute:
    (deps: DNSSwitcherDependencies) => async (params: { ip: string }) => {
      const { mikrotikBuilder, config, logger } = deps;
      const { ip } = params;

      logger.info({ ip }, 'Creating DNS lease for IP');

      const updatedLease = await mikrotikBuilder.execute(async (mikrotik) => {
        let lease = await mikrotik.getDHCPLeaseByIP(ip);
        logger.info({ lease });

        if (lease?.dynamic) {
          logger.info('Lease is dynamic, removing it');
          await mikrotik.removeDHCPLease(lease.id);
          lease = undefined;
        }

        if (!lease) {
          const mac = await mikrotik.findMacAddressByIP(ip);

          if (!mac) {
            throw new Error(
              'No DHCP lease found for IP and no ARP entry found',
            );
          }
          const option = await findOrCreateDHCPOption(mikrotik, config, logger);
          lease = await mikrotik.createDHCPLease({
            address: ip,
            'mac-address': mac.macAddress,
            'dhcp-option': option.id,
            comment: config.app.comment,
          });

          logger.info({ lease }, 'Created new static lease');
          return lease;
        }

        if (lease.comment === config.app.comment) {
          logger.info(
            'Lease already static with correct comment - doing nothing',
          );
          return lease;
        }

        logger.info('Lease is static but comment is incorrect');
        throw new Error('Lease is already static but comment is incorrect');
      });

      logger.info({ updatedLease }, 'Updated or created DHCP lease');

      return {
        ip,
        status: CLIENT_STATUS.CUSTOM,
      };
    },
};
