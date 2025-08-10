import type { Config } from '../../config.mts';
import type { MikrotikInteractorBuilder } from '../../interactors/mikrotik/builder.mts';
import type { Logger } from '../../logger.mts';

export interface DNSSwitcherDependencies {
  mikrotikBuilder: MikrotikInteractorBuilder;
  config: Config;
  logger: Logger;
}
