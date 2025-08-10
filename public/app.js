/**
 * @typedef {Object} DOMElements
 * @property {HTMLElement|null} statusCard - The status card container element
 * @property {HTMLElement|null} statusValue - The status value text element
 * @property {HTMLElement|null} statusDescription - The status description text element
 * @property {HTMLButtonElement|null} actionButton - The main action button element
 * @property {HTMLElement|null} loading - The loading indicator element
 * @property {HTMLElement|null} error - The error message element
 * @property {HTMLElement|null} clientIP - The client IP display element
 */

/**
 * DNS status constants that match the server response
 * @readonly
 * @enum {string}
 */
const DNS_STATUS = {
  /** Client has custom DNS configured */
  CUSTOM: 'CUSTOM',
  /** Client is using default DNS */
  DEFAULT: 'DEFAULT',
  /** Client can not be managed by this app */
  UNMANAGED: 'UNMANAGED'
};

/**
 * @typedef {Object} DNSStatusData
 * @property {string} status - The DNS status: 'CUSTOM', 'DEFAULT', or 'UNMANAGED'
 * @property {string} ip - The client IP address
 */

/**
 * @typedef {Object} APIResponse
 * @property {boolean} ok - Whether the API call was successful
 * @property {DNSStatusData} [data] - The response data if successful
 * @property {Object} [error] - Error information if unsuccessful
 * @property {string} [error.message] - Error message
 */

// DOM element references
/** @type {DOMElements} */
const elements = {
  statusCard: null,
  statusValue: null,
  statusDescription: null,
  actionButton: null,
  loading: null,
  error: null,
  clientIP: null
};

/**
 * Get a query parameter value from the current URL
 * @param {string} name - The parameter name to get
 * @returns {string|null} The parameter value or null if not found
 */
function getQueryParameter(name) {
  const urlParams = new URLSearchParams(window.location.search);
  return urlParams.get(name);
}

/**
 * Get the IP address to use for API requests
 * Either from query parameter 'ip' or null to use client's actual IP
 * @returns {string|null} The IP address or null
 */
function getTargetIP() {
  return getQueryParameter('ip');
}

/**
 * Create headers object for API requests, including X-Real-IP if needed
 * @returns {Object} Headers object for fetch requests
 */
function createAPIHeaders() {
  const headers = {
    'Content-Type': 'application/json'
  };

  const targetIP = getTargetIP();
  if (targetIP) {
    headers['X-Real-IP'] = targetIP;
  }

  return headers;
}

/**
 * Initialize DOM element references by finding them in the document
 * @returns {void}
 */
function initElements() {
  elements.statusCard = document.getElementById('status-card');
  elements.statusValue = document.getElementById('status-value');
  elements.statusDescription = document.getElementById('status-description');
  elements.actionButton = document.getElementById('action-button');
  elements.loading = document.getElementById('loading');
  elements.error = document.getElementById('error');
  elements.clientIP = document.getElementById('client-ip');
}

/**
 * Show or hide the loading indicator and disable/enable the action button
 * @param {boolean} show - Whether to show the loading state
 * @returns {void}
 */
function showLoading(show) {
  if (elements.loading) {
    elements.loading.style.display = show ? 'block' : 'none';
  }
  if (elements.actionButton) {
    elements.actionButton.disabled = show;
  }
}

/**
 * Display an error message to the user
 * @param {string} message - The error message to display
 * @returns {void}
 */
function showError(message) {
  if (elements.error) {
    elements.error.textContent = message;
    elements.error.style.display = 'block';
  }
}

/**
 * Hide the error message
 * @returns {void}
 */
function hideError() {
  if (elements.error) {
    elements.error.style.display = 'none';
  }
}

/**
 * Fetch the current DNS status from the API
 * @returns {Promise<DNSStatusData>} The DNS status data
 * @throws {Error} When the API request fails or returns an error
 */
async function fetchStatus() {
  const targetIP = getTargetIP();
  const requestOptions = {};

  if (targetIP) {
    requestOptions.headers = createAPIHeaders();
  }

  const response = await fetch('/api/dns', requestOptions);
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${response.statusText}`);
  }
  /** @type {APIResponse} */
  const data = await response.json();
  if (!data.ok) {
    throw new Error(data.error?.message || 'Failed to check DNS status');
  }
  return data.data;
}

/**
 * Switch to custom DNS configuration
 * @returns {Promise<DNSStatusData>} The updated DNS status data
 * @throws {Error} When the API request fails or returns an error
 */
async function switchToCustomDNS() {
  const targetIP = getTargetIP();
  const requestOptions = {
    method: 'PUT'
  };

  if (targetIP) {
    requestOptions.headers = {
      ...createAPIHeaders(),
      'Content-Type': 'application/json'
    };
  } else {
    requestOptions.headers = {
      'Content-Type': 'application/json'
    };
  }

  const response = await fetch('/api/dns', requestOptions);
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${response.statusText}`);
  }
  /** @type {APIResponse} */
  const data = await response.json();
  if (!data.ok) {
    throw new Error(data.error?.message || 'Failed to switch to custom DNS');
  }
  return data.data;
}

/**
 * Reset to default DNS configuration
 * @returns {Promise<DNSStatusData>} The updated DNS status data
 * @throws {Error} When the API request fails or returns an error
 */
async function resetToDefaultDNS() {
  const targetIP = getTargetIP();
  const requestOptions = {
    method: 'DELETE'
  };

  if (targetIP) {
    requestOptions.headers = {
      ...createAPIHeaders(),
      'Content-Type': 'application/json'
    };
  } else {
    requestOptions.headers = {
      'Content-Type': 'application/json'
    };
  }

  const response = await fetch('/api/dns', requestOptions);
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${response.statusText}`);
  }
  /** @type {APIResponse} */
  const data = await response.json();
  if (!data.ok) {
    throw new Error(data.error?.message || 'Failed to reset to default DNS');
  }
  return data.data;
}

/**
 * Update the client IP display
 * @param {string} [ip] - The client IP address to display
 * @returns {void}
 */
function updateClientIP(ip) {
  if (elements.clientIP) {
    elements.clientIP.textContent = ip || 'Unknown';
  }
}

/**
 * Update the DNS status display
 * @param {'unmanaged'|'custom'|'default'} type - The status type for CSS classes
 * @param {string} value - The status value text to display
 * @param {string} description - The status description text to display
 * @returns {void}
 */
function updateStatus(type, value, description) {
  if (elements.statusCard) {
    elements.statusCard.className = `status-card ${type}`;
  }
  if (elements.statusValue) {
    elements.statusValue.className = `status-value ${type}`;
    elements.statusValue.textContent = value;
  }
  if (elements.statusDescription) {
    elements.statusDescription.textContent = description;
  }
}

/**
 * Update the action button state and behavior
 * @param {'switch-to-custom'|'reset-to-default'|null} type - The button type for CSS classes
 * @param {string} [text] - The button text to display
 * @param {(() => void)|null} [clickHandler] - The click event handler
 * @returns {void}
 */
function updateActionButton(type, text, clickHandler) {
  if (!elements.actionButton) return;

  if (!type) {
    elements.actionButton.disabled = true;
    elements.actionButton.textContent = 'No Actions Available';
    elements.actionButton.className = 'action-button';
    elements.actionButton.onclick = null;
    return;
  }

  elements.actionButton.disabled = false;
  elements.actionButton.textContent = text;
  elements.actionButton.className = `action-button ${type}`;
  elements.actionButton.onclick = clickHandler;
}

/**
 * Update the entire UI based on DNS status data
 * @param {DNSStatusData} data - The DNS status data from the API
 * @returns {void}
 */
function updateUI(data) {
  const { status, ip } = data;

  updateClientIP(ip);

  switch (status) {
    case DNS_STATUS.UNMANAGED:
      updateStatus('unmanaged', 'Unmanaged', 'Your device DNS can not be managed by this app');
      updateActionButton(null);
      break;

    case DNS_STATUS.CUSTOM:
      updateStatus('custom', 'Custom DNS', 'Your device is using custom DNS servers');
      updateActionButton('reset-to-default', 'Reset to Default DNS', handleResetClick);
      break;

    case DNS_STATUS.DEFAULT:
      updateStatus('default', 'Default DNS', 'Your device is using default DNS servers');
      updateActionButton('switch-to-custom', 'Switch to Custom DNS', handleSwitchClick);
      break;

    default:
      // Fallback for unknown status
      updateStatus('unmanaged', 'Unknown Status', 'Unable to determine DNS configuration');
      updateActionButton(null);
      break;
  }
}

/**
 * Handle the switch to custom DNS button click
 * @returns {Promise<void>}
 */
async function handleSwitchClick() {
  showLoading(true);
  hideError();

  try {
    const data = await switchToCustomDNS();
    updateUI(data);
  } catch (error) {
    showError('Failed to switch to custom DNS: ' + error.message);
  } finally {
    showLoading(false);
  }
}

/**
 * Handle the reset to default DNS button click
 * @returns {Promise<void>}
 */
async function handleResetClick() {
  showLoading(true);
  hideError();

  try {
    const data = await resetToDefaultDNS();
    updateUI(data);
  } catch (error) {
    showError('Failed to reset to default DNS: ' + error.message);
  } finally {
    showLoading(false);
  }
}

/**
 * Load and display the initial DNS status when the app starts
 * @returns {Promise<void>}
 */
async function loadInitialStatus() {
  try {
    hideError();
    const data = await fetchStatus();
    updateUI(data);
  } catch (error) {
    showError('Failed to check DNS status: ' + error.message);
    // Fallback to unknown status when API fails
    updateUI({ status: 'UNKNOWN', ip: 'Unknown' });
  }
}

/**
 * Initialize the application by setting up DOM references and loading initial status
 * @returns {void}
 */
function initApp() {
  initElements();
  loadInitialStatus();
}

/**
 * Start the app when the DOM is fully loaded
 */
document.addEventListener('DOMContentLoaded', initApp);
