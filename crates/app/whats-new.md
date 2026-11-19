# What's new in PTS Workbench

<!--
  Shown in the app after an update (and when one is waiting for a restart), in plain words for everyone at the shop.
  Add new items at the TOP: a "## heading" (the version, or a name for the update), then one "- item" per change.
  "# Improvements" / "# Bug Fixes" under a heading groups the items after it. An item can carry on to the next line
  if that line is indented. **bold** and `code` work. Each item is only shown once on each computer, so it's fine to
  leave older ones here. (The same file format as the Electron app's src/whats-new.md.)
-->

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

# Bug Fixes

- Corrected an issue with webviews that led to the webview holding keyboard focus even after leaving the webview, leading to an inability to control certain functions of the app.
