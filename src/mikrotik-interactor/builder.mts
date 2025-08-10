import { RouterOSClient } from 'routeros-api';

import type { Config } from '../config.mts';
import type { Logger } from '../logger.mts';
import { MikrotikInteractor } from './index.mts';

type MikrotikInteractorConfig = Config['mikrotik'];

export class MikrotikInteractorBuilder {
  private config: MikrotikInteractorConfig;
  private logger: Logger;

  public constructor(config: MikrotikInteractorConfig, logger: Logger) {
    this.config = config;
    this.logger = logger;
  }

  public async execute<T>(
    action: (client: MikrotikInteractor) => Promise<T>,
  ): Promise<T> {
    const api = new RouterOSClient({
      host: this.config.host,
      user: this.config.username,
      password: this.config.password,
    });

    const client = await api.connect();
    const interactor = new MikrotikInteractor(client, this.logger);
    try {
      return await action(interactor);
    } finally {
      api.close();
    }
  }
}
