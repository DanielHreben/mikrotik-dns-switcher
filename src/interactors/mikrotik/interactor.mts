import type { RosApiMenu } from 'routeros-api';

import type { Logger } from '../../logger.mts';
import type {
  ARPResponse,
  DHCPLeaseCreateOptions,
  DHCPLeaseResponse,
  DHCPOptionCreateOptions,
  DHCPOptionResponse,
} from './types.mts';

export class MikrotikInteractor {
  private client: RosApiMenu;
  private logger: Logger;

  public constructor(client: RosApiMenu, logger: Logger) {
    this.client = client;
    this.logger = logger;
  }

  public async getDHCPLeaseByIP(ip: string) {
    const result = (await this.client.menu('/ip dhcp-server lease').getOnly({
      address: ip,
    })) as DHCPLeaseResponse;

    return result;
  }

  public async createDHCPLease(options: DHCPLeaseCreateOptions) {
    const lease = (await this.client.menu('/ip dhcp-server lease').add({
      address: options.address,
      comment: options.comment,
      'mac-address': options['mac-address'],
      'dhcp-option': options['dhcp-option'],
    })) as NonNullable<DHCPLeaseResponse>;

    return lease;
  }

  public async findMacAddressByIP(ip: string) {
    const arp = (await this.client
      .menu('/ip arp')
      .getOnly({ address: ip })) as ARPResponse;

    this.logger.info({ arp });

    return arp;
  }

  public async removeDHCPLease(id: string) {
    const result = (await this.client
      .menu('/ip dhcp-server lease')
      .remove(id)) as DHCPLeaseResponse;
    this.logger.info({ result }, 'Removed DHCP lease');
    return result;
  }

  public async createDHCPOption(options: DHCPOptionCreateOptions) {
    const result = (await this.client.menu('/ip dhcp-server option').add({
      name: options.name,
      code: options.code,
      value: options.value,
      comment: options.comment,
    })) as NonNullable<DHCPOptionResponse>;

    this.logger.info(result, 'Created DHCP option');
    return result;
  }

  public async getDHCPOptionByName(name: string) {
    const result = (await this.client
      .menu('/ip dhcp-server option')
      .getOnly({ name })) as DHCPOptionResponse;

    return result;
  }
}
