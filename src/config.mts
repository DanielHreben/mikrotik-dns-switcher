import { z } from 'zod';

import { parseRange } from './services/dns-switcher/allowed-range.ts';

const configSchema = z.object({
  mikrotik: z.object({
    host: z.string().min(1, 'MikroTik host is required'),
    username: z.string().min(1, 'Username is required'),
    password: z.string().min(1, 'Password is required'),
    // Router CA certificate (PEM file). Set: API-SSL on 8729, certificate
    // verified. Unset: plain API on 8728 (local development only).
    tlsCa: z.string().min(1).optional(),
  }),
  app: z.object({
    port: z.coerce.number().default(3000),
    customDns: z.ipv4('Invalid DNS server IP'),
    comment: z.string().default('DNS-Switcher-Managed'),
    // The reverse proxy whose X-Real-IP header is trusted.
    trustedProxy: z.ipv4('Invalid trusted proxy IP').optional(),
    // Trust X-Real-IP from anyone (?ip= in the page). Local development only.
    allowTargetIP: z.stringbool().default(false),
    // The addresses the switcher may manage, e.g. "192.168.88.20-192.168.88.99".
    allowedRange: z
      .string()
      .optional()
      .transform((spec) => (spec === undefined ? undefined : parseRange(spec))),
  }),
});

export const config = configSchema.parse({
  mikrotik: {
    host: process.env.MIKROTIK_HOST,
    username: process.env.MIKROTIK_USERNAME,
    password: process.env.MIKROTIK_PASSWORD,
    tlsCa: process.env.MIKROTIK_TLS_CA,
  },
  app: {
    port: process.env.PORT,
    customDns: process.env.CUSTOM_DNS,
    comment: process.env.APP_COMMENT,
    trustedProxy: process.env.TRUSTED_PROXY,
    allowTargetIP: process.env.ALLOW_TARGET_IP,
    allowedRange: process.env.ALLOWED_RANGE,
  },
});

export type Config = z.infer<typeof configSchema>;
