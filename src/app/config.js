// App-wide settings and fixed values: the window's bar sizes, the default settings (and which ones are on/off,
// numbers, or fixed), Ford's link codes, which vehicles are Ford's (PTS) or not (ProDemand), the sync server's
// address (package.json "ptsWorkbench"), and small helpers.
'use strict';


const COX_SIGNIN_HOST = /(^|\.)signin\.coxautoinc\.com$/i;

// The sync server's address is set in package.json ("ptsWorkbench": { "syncUrl": … })
const SYNC_URL = (() => { try { return require('../../package.json').ptsWorkbench?.syncUrl || ''; } catch { return ''; } })();
// Feedback (bug reports / feature ideas) goes to the same server; this employee number can read it all
const FEEDBACK_ADMIN = (() => { try { return String(require('../../package.json').ptsWorkbench?.feedbackAdmin || ''); } catch { return ''; } })();

const TOP_BAR = 48;
const TAB_STRIP = 38;
const SUB_STRIP = 34; // the Service Manual's own tab row
const DRAWER = 360;
const CHAT_W = 380;   // the read-only Dealership chat panel on the right

const DEFAULT_SETTINGS = {
  employeeNumber: '',  // your employee number; the Xtime technician ID is looked up from it
  storeId: '3590',
  dealerCode: '',      // Warranty Dealer Code (e.g. the dealership's PA code), asked at setup; used for FSA software validation codes
  xtimeEmployeesPath: '/JS/Employees?storeID={STORE}',
  techId: '',          // Xtime's internal technician ID (filled in from the employee number)
  techIdFor: '',       // the employee number techId was looked up for
  setupDone: false,    // first-launch questions answered
  xtimeUrl: 'https://www.inspect.app.coxautoinc.com/',
  xtimeDataHost: 'www.inspect.app.coxautoinc.com',
  xtimeApiPath: '/api/RepairOrderDashboard/?showCustomerROs=true&showInternalROs=true&showExpressROs=&showWaiterROs=',
  ptsUrl: 'https://www.fordtechservice.dealerconnection.com/',
  // Ford's own deep link: opens the workshop manual with the vehicle already loaded
  ptsTransportUrl: 'https://www.fordtechservice.dealerconnection.com/transport?link={LINK}&params={VIN}',
  oasisUrl: 'https://www.fordtechservice.dealerconnection.com/oasis/Index?vin={VIN}',
  xtimeDetailsPath: '/RepairOrder/Details/{RO}',
  xtimeRoUrl: 'https://www.inspect.app.coxautoinc.com/RepairOrder/Details/{RO}',
  prodemandUrl: 'https://prodemand.com/',
  pollSeconds: 60,
  xtimeRefreshMinutes: 90, // reload Xtime in the background before its ~2 h session runs out
  autoLogin: true,         // press "Sign in" with a saved password when a site asks you to log in
  syncEnabled: false,      // keep flags, hours and the clock in sync across computers (main window's setting)
  sidebarWidth: 380,
  sidebarHidden: false,
  xtimeDashboard: false,   // show the app's own Xtime dashboard instead of Xtime's page
  keepAwake: true,         // keep the computer (and screen) from going to sleep while the app is open
  notifications: true,     // Windows pop-ups: RO assigned, line approved/declined, parts in, customer/warranty answered, new Xtime messages
};
const BOOLEAN_SETTINGS = ['autoLogin', 'setupDone', 'syncEnabled', 'xtimeDashboard', 'notifications', 'sidebarHidden', 'keepAwake'];

// Not shown in Settings (too technical): always the built-in values, updated with the app
const FIXED_SETTINGS = ['ptsUrl', 'ptsTransportUrl', 'oasisUrl', 'xtimeApiPath', 'xtimeRoUrl', 'xtimeDetailsPath', 'xtimeEmployeesPath'];
// Ford transport link codes: 704 = OASIS landing page (workshop manual), 702 = Over-the-Air update
// dashboard, 701 = Connected Vehicle Data
const FORD_LINKS = { pts: '704', ota: '702', cvd: '701' };
// Warranty Dealer Code: letters and numbers only, e.g. USA12345
const cleanDealerCode = (v) => String(v || '').toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 12);
const NUMBER_SETTINGS = { pollSeconds: [15, 3600], sidebarWidth: [260, 700], xtimeRefreshMinutes: [20, 600] };

// Ford, Lincoln and Mercury get PTS; anything else gets ProDemand.
// Make from Xtime first; if there isn't one, the VIN's manufacturer code (first 3 characters).
const FORD_WMI = /^(1F[A-DMT]|2F[ABMT]|3F[A-EMT]|1L[N]|2L[MN]|3L[N]|5L[DMT]|[123]ME|4M2|1ZV|NM0|WF0|MAJ|6FP|MN[AB]|LVS|PE1)/;
function isFordVehicle(vin, ymm) {
  const text = String(ymm || '').toLowerCase();
  if (/\b(ford|lincoln|mercury)\b/.test(text)) return true;
  return FORD_WMI.test(String(vin || '').toUpperCase());
}

// Best effort: type the VIN into a box labelled "VIN" on the ProDemand page (any same-site frame).
const PRODEMAND_FILL = `((vin) => {
  const docs = [document, ...[...document.querySelectorAll('iframe,frame')].map((f) => { try { return f.contentDocument; } catch { return null; } }).filter(Boolean)];
  for (const d of docs) {
    const i = [...d.querySelectorAll('input')].find((x) => x.offsetParent !== null && /(^|[^a-z])vin(number|[^a-z]|$)/i.test([x.id, x.name, x.placeholder, x.getAttribute('aria-label')].join(' ')));
    if (!i) continue;
    if (i.value && i.value.toUpperCase() === vin) return 'filled';
    const w = d.defaultView;
    Object.getOwnPropertyDescriptor(w.HTMLInputElement.prototype, 'value').set.call(i, vin);
    i.dispatchEvent(new w.Event('input', { bubbles: true }));
    i.dispatchEvent(new w.Event('change', { bubbles: true }));
    return 'filled';
  }
  return 'none';
})`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

module.exports = { COX_SIGNIN_HOST, SYNC_URL, FEEDBACK_ADMIN, TOP_BAR, TAB_STRIP, SUB_STRIP, DRAWER, CHAT_W, DEFAULT_SETTINGS, BOOLEAN_SETTINGS, FIXED_SETTINGS, FORD_LINKS, cleanDealerCode, NUMBER_SETTINGS, isFordVehicle, PRODEMAND_FILL, sleep };
