# What's new in PTS Workbench

<!--
  Shown in the app after an update (and when one is waiting for a restart), in plain words for everyone at the shop.
  Add new items at the TOP: a "## heading" (a date or a name for the update), then one "- item" per change.
  An item can carry on to the next line if that line is indented. **bold** and `code` work.
  Each item is only shown once on each computer, so it's fine to leave older ones here.
  Changing only this file (and pages) still updates without a restart.
-->

## v0.4.4

- **Consolidated Codebase:** Restructured most of the internal codebase to make development more efficient, and help the app recover from errors.
- **Memory Management:** ROs that have not been opened in a while, or haven't been open on the current launch, will automatically go to sleep to conserve system resources. These ROs are marked by an `ASLEEP` tag next to the RO in **Workbench**
- **Content Preloading:** With the added benefits of putting ROs to sleep, ROs that are first opened preload all relevant data from the workshop manual including SLTS, OASIS, TSBs, Workshop Manual Content, and PC/ED (if supported). Content is automatically unloaded after 15 minutes, or if there is a change pushed by Ford (updated workshop manual)

- **What's new:** after an update, this window shows what changed. Updates that don't need a restart show up in 🔔 notifications instead, so nothing you're doing gets interrupted.
- **PC/ED manual:** older vehicles that have one get a **PC/ED** tab in the Service Manual, next to Workshop Manual. Type a DTC like `P0171` in its search to find its pinpoint tests.
- **Xtime stays signed in:** you no longer have to sign in to Xtime every time you open the app.
- **Xtime View, edit lines:** the ✎ button on a line opens it to change the service, labor type, hours, prices or notes.
- **Xtime View, adding lines:** new lines get the dispatch code and estimated duration (S100, 0.5) like Xtime does. Picking a service fills in Details, Concern and Correction for you.
- **Bigger Technician Note box** when adding or editing a line, with room for a parts list.
- **Settings, Feedback and Notifications** no longer black out the page behind them.
- **Chat** now switches to the RO you click on.
- **Labor Times** scrolls to the top of the operation it opens.
- **FSA documents:** opening a second document (like the dealer bulletin) no longer shows an error.
