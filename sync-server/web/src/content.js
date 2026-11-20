// The site's words, in one place: change them here without touching the components.
// The demo's repair orders, vehicles and hours are made up.

export const brand = { name: 'ProDealer', tld: '.works', app: 'Workbench' };

export const hero = {
  eyebrow: 'Built in a dealership service department',
  title: ['Less clicking.', 'More fixing.'],
  lede: 'Workbench puts the service manual, diagnostics, repair orders and your hours in one window, already open to the vehicle in your bay. Dealership techs spend their time on the car, not on finding things.',
  fine: ['No admin rights needed', 'Updates itself', 'Customer info stays in the shop'],
};

export const problem = {
  title: 'One repair order, six places to look',
  text: 'Every job means jumping between the repair order system, the service manual, bulletins, wiring diagrams, diagnostics and a notepad for your hours, and typing the same VIN into each of them.',
  places: ['Repair orders', 'Service manual', 'Bulletins', 'Wiring', 'Diagnostics', 'Your hours'],
};

export const features = [
  { icon: 'ro', title: 'Opens to the right vehicle', text: "Reads the repair orders assigned to you and opens each vehicle's manual, bulletins and wiring by itself. No more typing VINs." },
  { icon: 'pulse', title: 'Diagnostics in one place', text: "Trouble codes, the vehicle's health history and software updates, vehicle history and labor times, side by side with the manual." },
  { icon: 'clock', title: 'Your hours, kept for you', text: "Flagged hours for every pay period, checked against your pay sheet so nothing goes missing, plus spiffs you've turned in." },
  { icon: 'bell', title: 'Never miss an approval', text: "Know the moment repairs are approved or declined, or the advisor sends a message, even when you've stepped away." },
  { icon: 'lock', title: 'Private by design', text: 'Your notes and hours follow you between computers, encrypted before they leave. Customer information never leaves the shop.' },
  { icon: 'refresh', title: 'Installs without IT', text: 'Runs on the shop computers you already have, with no admin rights, and keeps itself up to date in the background.' },
];

export const flow = [
  { title: 'The RO comes in', text: 'It shows up in My ROs with the vehicle and concern, ready when you are.' },
  { title: 'The vehicle opens', text: 'Its manual, bulletins, wiring and diagnostics are a click away, already for that VIN.' },
  { title: 'You fix it', text: 'Look things up without leaving the bay, and get a heads-up the second more work is approved.' },
  { title: 'Your hours add up', text: 'Flagged time lands on your hours for the pay period, on your computer and your phone.' },
];

export const phone = {
  title: 'The shop in your pocket',
  text: 'Pair your phone once by scanning a code from the desktop app. No app store needed.',
  points: [
    "Notifications for approvals and messages you haven't seen yet",
    'Your open repair orders, and flag hours from anywhere',
    "Snap your pay sheet to see what you've been paid for",
    'Clock in and out, and watch the pay period add up',
  ],
  notes: [
    { title: 'RO 418203: repairs approved', body: 'Spark plugs, ignition coil #1' },
    { title: 'RO 418221: message', body: '"Customer says go ahead with the rear pads too"' },
    { title: 'RO 418230: 1 repair declined', body: 'Cabin air filter' },
  ],
};

export const roadmap = {
  title: 'Built to grow with the dealership',
  text: 'Workbench starts with the technician. Next, it brings the rest of the store onto the same page.',
  items: [
    { title: 'Every brand you sell', text: "The same workbench for multi-franchise stores, whatever's on the lift." },
    { title: 'Dispatch', text: 'Repair orders routed to the right tech automatically, by skill and workload.' },
    { title: 'Customer updates', text: 'Customers see where their vehicle is in the process without calling the desk.' },
    { title: 'Parts and inventory', text: "See what's on the shelf from the job, and keep stock simpler for the parts team." },
  ],
};

export const final = {
  title: 'Get back to the car',
  text: 'Install Workbench on a shop computer in a couple of minutes. No admin rights, no IT ticket.',
};

// The drawn app in the hero
export const demo = {
  ros: [
    { num: '418203', vehicle: '2021 F-150 XLT 3.5L', detail: 'Tag 512 · Check engine light on', tag: ['ok', 'Approved'] },
    { num: '418221', vehicle: '2019 Escape SE 1.5L', detail: 'Tag 538 · Brakes grinding', tag: ['wait', 'Waiting'] },
    { num: '418230', vehicle: '2023 Bronco Sport', detail: 'Tag 541 · Recall + maintenance', tag: ['new', 'New'] },
  ],
  newRo: { num: '418247', vehicle: '2020 Explorer ST', detail: 'Tag 557 · Noise over bumps', tag: ['new', 'Just assigned'] },
  manual: {
    title: 'Brake pads and rotors: removal and installation',
    crumb: '2019 Escape · Brakes · Front disc brake',
    steps: [
      'Raise and support the vehicle. Remove the wheel and tire.',
      'Remove the caliper bolts and position the caliper aside.',
      'Remove the pads, then the caliper anchor plate.',
    ],
  },
  dtcs: [
    { code: 'P0301', text: 'Cylinder 1 misfire detected', tag: ['wait', 'Pending'] },
    { code: 'P0420', text: 'Catalyst efficiency below threshold', tag: ['ok', 'Cleared'] },
  ],
  readings: [['12V battery', '12.6 V'], ['Oil life', '38%'], ['Odometer', '48,210']],
  hours: { flagged: 42.7, ringMax: 60, days: [['Mon', 9.4], ['Tue', 7.5], ['Wed', 10.8], ['Thu', 6.6], ['Fri', 8.4]] },
};

// The download page (/download/). {file} is the installer's name.
export const download = {
  steps: [
    { title: 'Open the installer', text: "Click {file} in your browser's downloads. In Chrome and Edge, that's the download icon at the top right." },
    { title: 'If Windows asks, let it run', text: 'If a blue "Windows protected your PC" box comes up, click More info, then Run anyway.' },
    { title: 'No admin password needed', text: 'It installs just for you, so there\'s nothing to ask IT for. Workbench opens when it\'s done, and it\'s in the Start menu after that.' },
    { title: 'Set it up once', text: 'Workbench walks you through the first-time setup. After that it keeps itself up to date in the background.' },
  ],
  phone: 'Then add your phone: click Pair phone at the bottom right of Workbench and scan the code with your phone\'s camera.',
};
