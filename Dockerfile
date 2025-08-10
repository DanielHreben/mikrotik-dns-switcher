# Use Ubuntu LTS as base image
FROM ubuntu:22.04

# Set environment variables
ENV NODE_ENV=production

# Install Node.js 22.x and other dependencies
RUN apt-get update && \
  apt-get install -y \
  curl \
  ca-certificates \
  gnupg \
  lsb-release && \
  # Add NodeSource GPG key
  curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg && \
  # Add NodeSource repository
  echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_22.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list && \
  # Update package list and install Node.js
  apt-get update && \
  apt-get install -y nodejs && \
  # Clean up
  apt-get clean && \
  rm -rf /var/lib/apt/lists/*

# Verify Node.js version
RUN npm i -g yarn

# Create app directory
WORKDIR /app

# Create non-root user for security
RUN groupadd -r appuser && useradd -r -g appuser appuser

# Copy package files
COPY package.json yarn.lock* package-lock.json* ./

# Install dependencies
RUN yarn install --frozen-lockfile --production

# Copy application source
COPY src/ ./src/
COPY public/ ./public/

# Change ownership to non-root user
RUN chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Expose the application port
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:3000/api/health || exit 1

# Start the application
CMD ["node", "src/app.mts"]
