import Router from '@koa/router';
import http from 'http';
import type { Context } from 'koa';
import Koa from 'koa';
import bodyParser from 'koa-bodyparser';
import serve from 'koa-static';

import { config } from './config.mts';
import { createController } from './controller/builder.mts';
import { extractClientIP } from './controller/util.ts';
import { MikrotikInteractorBuilder } from './interactors/mikrotik/builder.mts';
import { createLogger } from './logger.mts';
import { checkIfClientHasCustomDHCPLease } from './services/dns-switcher/check-for-client.mts';
import { removeCustomDHCPLeaseForClient } from './services/dns-switcher/reset-for-client.mts';
import { setupDHCPLeaseWithCustomDNSForClient } from './services/dns-switcher/setup-for-client.mts';
import type { DNSSwitcherDependencies } from './services/dns-switcher/types.mts';
import { checkServiceHealth } from './services/service-health/check-service-health.mts';
import type { ServiceHealthDependencies } from './services/service-health/types.mts';

const logger = createLogger();
const mikrotikBuilder = new MikrotikInteractorBuilder(config.mikrotik, logger);

// Create controller factory
const dnsSwitcherController = createController({
  mikrotikBuilder,
  config,
  logger,
} satisfies DNSSwitcherDependencies);
const serviceHealthController = createController({
  logger,
} satisfies ServiceHealthDependencies);

const app = new Koa({ proxy: true, proxyIpHeader: 'X-Real-IP' });
const apiRouter = new Router().prefix('/api');

// Middleware
app.use(bodyParser());
app.use(serve('public'));

// DNS Routes
apiRouter.get(
  '/dns',
  dnsSwitcherController(checkIfClientHasCustomDHCPLease)
    .extractParams((ctx: Context) => ({ ip: extractClientIP(ctx) }))
    .buildHandler(),
);

apiRouter.put(
  '/dns',
  dnsSwitcherController(setupDHCPLeaseWithCustomDNSForClient)
    .extractParams((ctx: Context) => ({ ip: extractClientIP(ctx) }))
    .buildHandler(),
);

apiRouter.delete(
  '/dns',
  dnsSwitcherController(removeCustomDHCPLeaseForClient)
    .extractParams((ctx: Context) => ({ ip: extractClientIP(ctx) }))
    .buildHandler(),
);

// API routes
app.use(apiRouter.routes());
app.use(apiRouter.allowedMethods());

// Health check using controller
app.use(
  new Router()
    .get('/health', serviceHealthController(checkServiceHealth).buildHandler())
    .routes(),
);

// Error handling
app.on('error', (err: Error) => {
  logger.fatal({ err }, 'Server error');
});

// Start server
await new Promise<void>((resolve, reject) => {
  http
    // eslint-disable-next-line @typescript-eslint/no-misused-promises
    .createServer(app.callback())
    .on('error', reject)
    .listen(config.app.port, resolve);
});

logger.info(
  `🚀 MikroTik DNS Switcher running on http://localhost:${config.app.port.toString()}`,
);
