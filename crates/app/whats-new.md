# What's new in PTS Workbench

<!--
  Shown in the app after an update (and when one is waiting for a restart), in plain words for everyone at the shop.
  Add new items at the TOP: a "## heading" (the version, or a name for the update), then one "- item" per change.
  "# Improvements" / "# Bug Fixes" under a heading groups the items after it. An item can carry on to the next line
  if that line is indented. **bold** and `code` work. Each item is only shown once on each computer, so it's fine to
  leave older ones here. (The same file format as the Electron app's src/whats-new.md.)
-->

## v0.5.6

# Nothing exciting in this update, just lots of bug fixes.

# Improvements
- Added support for viewing XTime Images within Chat.
- Added support for viewing media within XTime View

# Bug Fixes
- Fixed an issue with XTime View that would cause Tabs within Notes to appear like a garbled mess.
- Fixed an issue with Ford WSM Parsing that would cause procedures to be listed out as 1. 1. 1. 1. (so on) instead of 1. 2. 3. Determined issue was caused by Ford returning multiple separate lists within a single procedure, and the parser (until now) took that at face value. Now, the parser pays attention to prior lists in order to effectively lay out instructions.
- Fixed a bug with Ford WSM Rendering that would reset procedure scroll bar position to the very top when opening images.
- Fixed an issue with the XTime View rendering that would list the `Since` Date incorrectly. (may display as /Date(1795638969000+0000)/ instead of the correct date.) Previously, app would not parse XTime's specific date rendering as XTime renders time as milliseconds since 1970 (commonly referred to as the unix time standard), whereas this app was only trained to respond to text-based date formats (like 2026-11-15T09:05)
- Fixed an issue with how wiring diagrams are rendered that would lead to the diagram appearing low-resolution when not actively moving the image.
- Fixed an issue with how FSAs are rendered in that led to incorrect article names being displayed alongside the FSA (like showing S1Bull instead of Dealer Bulletin)

## v0.5.5

# Improvements
- **Windows Application Signing** has now been implemented. What does this mean? No more "Windows Protected your PC" or "Suspicious Download blocked" messages. This program has now been fully validated to run on all windows computers, with no scary popups getting in the way.

## v0.5.4

# Improvements
- **Xtime sign-in:** If you're not signed in to XTime, its sign-in now opens as soon as the app starts (and again if XTime signs you out), instead of waiting for you to find the button.

# Bug Fixes
- **Predictive Pay** now syncs between computers and your phone.
- **Copy Vin** at the top of the vehicle is no longer cut off on smaller windows.

## v0.5.3

# Bug Fixes
- Fixed an issue with the new custom domain getting blocked by company IT, leading to the app not correctly syncing or being able to fetch updates.

## v0.5.2

# Improvements
- **Settings** has now been redesigned to have a similar style to Chrome settings. Settings are separated by tabs (**Profiles**, **Appearance**, **Linked Accounts**, and **About Workbench**)
- **Predictive Pay:** Hours now shows how much you will make before spiffs by looking at the hours you have flagged. Set it up from two of your pay stubs. After initial setup, the app will track your hours live and show much your check will be. Encrypted and synced with the mobile app.
- **Hour Sheet Scanning** is not on the mobile app. It is still in very early testing and issues are expected. All images are kept on-device, and is using a very primitive image upscaling standard in order to get information from images.

## v0.5.1

# Improvements
- You will now receive a notification if your feedback report has been marked as 'Fixed' or if your receive a reply on your report.

# Bug Fixes
- In the mobile app, fixed *toast* notifications staying on-screen at all times.
- Fixed an issue caused by pinning the app to the windows taskbar - The app is launched by a separate internal 'launcher' in order to improve automatic updates, which led to that specific version of the app being pinned instead of the launcher. This would prevent updates from being applied. Windows now respects this apps launch parameters and will pin the launcher instead of the app binary itself.

## The new PTS Workbench

# Improvements
- **A brand new app:** PTS Workbench has been rebuilt from the ground up. It starts quicker, is lighter on the computer, and everything (Xtime, the Service Manual, your hours) is drawn by the app itself.
- **Beta Updates** automatically disable once the latest full-release has been released. So, for anyone running the beta build of this Rust app, you will automatically go to the next full-release version as soon as it is ready.
- **App Profiles:** For shared computers, technicians can now switch between different profiles within the app and track their own hours, spiffs, and ROs separately from everyone else. Profiles can optionally be secured with a 4-digit pin (set in Settings, under Profile). Each new profile asks once whether it's for the **Main Shop** or **Quick Lane**, customizing the experience to better suit techician workloads.
- **Vehicle History** now has its own built-in rendering under the Diagnostics tab.
- **Connected Vehicle Data** has now been fully integrated into the new redesigned app.
- **New Diagnostics Interface:** Designed as a stand-in replacement for IDS and FDRS where available - it communicates on the standard J2534 communication bus that modern VCIs operate under (including Fords VCM3 and VCMM). Adds a helper build next to the app executable for this 64-bit app to communicate with fords 32-bit VCI Drivers. **DIAGNOSTICS MODE IS VERY EXPERIMENTAL. USE AT YOUR OWN RISK. NO INFORMATION IS SHARED WITH FORD SERVERS - NO LOGGING FOR WARRANTY PURPOSES AT THIS TIME.**
- **Diagnostic networks** have been expanded to implement networks aside from just HS-CAN, as well as DIAG networks used by the gateway module on newer vehicles.
- **Your lines in the sidebar:** the vehicle you're on shows its lines under it. Flag each one, click its hours to set what it really pays, and click the date under a flagged line to move it to the day you really flagged it. **Lines** folds them away.
- **Workshop Manual:** the contents (narrowed to this vehicle), search, and procedures in tabs, with pictures you can click to zoom.
- **Pinpoint tests, one step at a time:** answer Yes or No and the next step slides in. Your answers are recorded to Vehicle History, just like PTS.
- **Picks up where you left off:** the vehicles you had open, the tab you were on and how far down you'd scrolled.
- **Updates by itself:** a **Restart to update** button shows at the top when a new version is ready. Turn on **Beta updates** in Settings to get new features first.
- **Spiff tracking:** The app automatically tracks total sold ASR values and, if the total is over $150, gives you a nice printable view to be turned in to your beloved service manager.
- **Cloud Sync** has been re-implemented. It is now stable enough to be implemented by default. Can be turned off in Settings > Sync
- **Hour tracking** has been improved and re-implemented, just the same as Electron was.
- **Labor Times** has been improved and re-implemented, just the same as Electron.
- **Add an RO** has been added back in as a much-needed tool for working on repair orders from other technicians.
- **RO Status** has been updated, so the sidebar will now show the current status of each RO.
- **Accept new ROs from Dispatch:** Finally, through the built-in XTime display, we can finally accept new ROs from dispatch.
- **Backup PTS & XTime Interface:** For those of us that are not yet ready to embrace what technology has to offer, the normal XTime & PTS websites are available as a built-in tool. Web pages will not hold the same performance advantages the rest of the app has.
- **Phone Linking:** Workbench now has a mobile app! Only tested on iPhone so far. Get on-the-go notifications about any ROs you're tracking.

# Bug Fixes

- Corrected an issue with webviews that led to the webview holding keyboard focus even after leaving the webview, leading to an inability to control certain functions of the app.
- Corrected an issue with scroll bars resetting when switching between views
