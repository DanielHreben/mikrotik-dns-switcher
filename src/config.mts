import { config as dotenvConfig } from 'dotenv';
import { z } from 'zod';

// Load environment variables
dotenvConfig();

const configSchema = z.object({
  mikrotik: z.object({
    host: z.string().min(1, 'MikroTik host is required'),
    username: z.string().min(1, 'Username is required'),
    password: z.string().min(1, 'Password is required'),
  }),
  app: z.object({
    port: z.coerce.number().default(3000),
    customDns: z.ipv4('Invalid DNS server IP'),
    comment: z.string().default('DNS-Switcher-Managed'),
  }),
});

export const config = configSchema.parse({
  mikrotik: {
    host: process.env.MIKROTIK_HOST,
    username: process.env.MIKROTIK_USERNAME,
    password: process.env.MIKROTIK_PASSWORD,
  },
  app: {
    port: process.env.PORT,
    customDns: process.env.CUSTOM_DNS,
    comment: process.env.APP_COMMENT,
  },
});

export type Config = z.infer<typeof configSchema>;
