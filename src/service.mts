import type { Config } from './config.mts';
import type { Logger } from './logger.mts';
import type {
  MikrotikInteractor,
  MikrotikInteractorBuilder,
} from './mikrotik-interactor/index.mts';

export class Service {
  private mikrotikBuilder: MikrotikInteractorBuilder;
  private config: Config;
  private logger: Logger;

  public constructor(
    mikrotikBuilder: MikrotikInteractorBuilder,
    config: Config,
    logger: Logger,
  ) {
    this.mikrotikBuilder = mikrotikBuilder;
    this.config = config;
    this.logger = logger;
  }

  private async findOrCreateDHCPOption(mikrotik: MikrotikInteractor) {
    const name = 'Custom DNS Server';
    const code = '6'; // DHCP option code for DNS servers
    const value =
      '0x' +
      this.config.app.customDns
        .split('.')
        .map((octet) =>
          Number.parseInt(octet, 10).toString(16).padStart(2, '0'),
        )
        .join('');

    this.logger.info({ name, code, value });
    const existingOption = await mikrotik.getDHCPOptionByName(name);
    if (existingOption) {
      return existingOption;
    }

    const createdOption = await mikrotik.createDHCPOption({
      name,
      code,
      value,
      comment: this.config.app.comment,
    });

    if (!createdOption) {
      throw new Error('Failed to create DHCP option');
    }
    return createdOption;
  }

  public async deleteDHCPLeaseByIP(ip: string) {
    const result = await this.mikrotikBuilder.execute(async (mikrotik) => {
      const lease = await mikrotik.getDHCPLeaseByIP(ip);

      if (!lease) {
        this.logger.info({ ip }, 'No DHCP lease found for IP');
        return undefined;
      }

      return await mikrotik.removeDHCPLease(lease.id);
    });

    return result;
  }

  public async getDHCPLeaseByIP(ip: string) {
    const lease = await this.mikrotikBuilder.execute(async (mikrotik) => {
      return mikrotik.getDHCPLeaseByIP(ip);
    });

    if (!lease) {
      this.logger.info({ ip }, 'No DHCP lease found for IP');
      return undefined;
    }

    return lease;
  }

  public async createDHCPLease(ip: string) {
    const updatedLease = await this.mikrotikBuilder.execute(
      async (mikrotik) => {
        let lease = await mikrotik.getDHCPLeaseByIP(ip);
        this.logger.info({ lease });

        if (lease?.dynamic) {
          this.logger.info('Lease is dynamic, removing it');
          await mikrotik.removeDHCPLease(lease.id);
          lease = undefined;
        }

        if (!lease) {
          const mac = await mikrotik.findMacAddressByIP(ip);

          if (!mac) {
            throw new Error(
              `No DHCP lease found for IP ${ip} and no ARP entry found`,
            );
          }
          const option = await this.findOrCreateDHCPOption(mikrotik);
          lease = await mikrotik.createDHCPLease({
            address: ip,
            'mac-address': mac.macAddress,
            'dhcp-option': option.id,
            comment: this.config.app.comment,
          });

          this.logger.info(lease, 'Created new static lease');
          return lease;
        }

        if (lease.comment === this.config.app.comment) {
          this.logger.info(
            'Lease already static with correct comment - doing nothing',
          );
          return lease;
        }

        this.logger.info('Lease is static but comment is incorrect');
        throw new Error('Lease is already static but comment is incorrect');
      },
    );

    this.logger.info({ updatedLease }, 'Updated or created DHCP lease');
    return updatedLease;
  }
}
